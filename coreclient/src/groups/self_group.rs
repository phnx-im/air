// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::{aead::keys::IdentityLinkWrapperKey, indexed_aead::keys::UserProfileKey},
    mls_group_config::AppComponent,
};
use airprotos::client::{
    component::AirComponent, group::GroupData,
    virtual_client::VIRTUAL_CLIENT_KP_UPLOAD_COMPONENT_ID,
};
use anyhow::Context;
use tracing::{debug, warn};

use crate::{
    chats::GroupDataExt,
    clients::{CoreUser, own_client_info::OwnClientInfo},
    groups::Group,
    key_stores::indexed_keys::StorableIndexedKey,
};

impl CoreUser {
    pub async fn ensure_self_group(&self) -> anyhow::Result<()> {
        let mut read = self.db().read().await?;
        let own_client_info = OwnClientInfo::load(&mut read).await?;
        if let Some(group_id) = own_client_info.self_group_id {
            match Group::load(read, &group_id).await? {
                Some(_group) => {
                    debug!("Self-group found");
                    return Ok(());
                }
                None => {
                    warn!("Self-group not found, recreating it");
                }
            }
        } else {
            drop(read);
        }

        self.create_self_group().await?;
        Ok(())
    }

    async fn create_self_group(&self) -> anyhow::Result<Group> {
        let api_client = self.api_client()?;

        // Request group IDs
        let provision_group_profile_size = None;
        let request_pq_group_id = true;
        let (group_id, pq_group_id, _) = api_client
            .ds_request_group_id(provision_group_profile_size, request_pq_group_id)
            .await?;
        let pq_group_id = pq_group_id.context("Missing PQ group ID")?;

        // Self group has empty group data
        let group_data_bytes = GroupData {
            legacy_title: None,
            legacy_picture: None,
            encrypted_title: None,
            external_group_profile: None,
        }
        .encode()?;

        // Create group locally
        let key_store = self.key_store();
        let (group, partial_params, user_profile_key) = self
            .db()
            .with_write_transaction(async move |txn| -> anyhow::Result<_> {
                let safe_aad_components = Some(vec![VIRTUAL_CLIENT_KP_UPLOAD_COMPONENT_ID]);
                let (group, partial_params) = Group::create_apq_group(
                    &mut *txn,
                    &key_store.signing_key,
                    IdentityLinkWrapperKey::random()?,
                    group_id,
                    pq_group_id,
                    group_data_bytes,
                    safe_aad_components,
                    AirComponent::default_for_self_group(),
                )?;

                let user_profile_key = UserProfileKey::load_own(&mut *txn).await?;

                group.store(txn).await?;
                Ok((group, partial_params, user_profile_key))
            })
            .await?;

        // TODO: Technically, we don't have to provide neither of this information for a self-group.
        let client_reference = self.create_own_client_reference();
        let encrypted_user_profile_key =
            user_profile_key.encrypt(group.identity_link_wrapper_key(), self.user_id())?;
        let params = partial_params.into_params(client_reference, encrypted_user_profile_key);

        // Create group on the server
        if let Err(error) = api_client
            .ds_create_group(params, &key_store.signing_key, group.group_state_ear_key())
            .await
        {
            self.db()
                .with_write_transaction(async |txn| {
                    Group::delete_from_db(txn, group.group_id()).await
                })
                .await?;
            return Err(error.into());
        }

        // Assign the group to the own client info
        OwnClientInfo::set_self_group_id(self.db().write().await?, group.group_id()).await?;

        Ok(group)
    }
}

#[cfg(test)]
mod tests {
    use airserver_test_harness::utils::setup::TestBackend;

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn ensure_self_group_creates_apq_group() -> anyhow::Result<()> {
        let mut setup = TestBackend::single().await;
        let user_id = setup.add_user().await;
        let user = &setup.get_user(&user_id).user;

        user.ensure_self_group().await?;

        let is_apq = user
            .self_group_is_apq()
            .await?
            .expect("self group should be persisted");
        assert!(is_apq, "self-group must be an APQ (T+PQ) group");

        Ok(())
    }
}
