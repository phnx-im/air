// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::{ClientSigningKey, PreliminaryClientSigningKey},
    crypto::{aead::keys::IdentityLinkWrapperKey, indexed_aead::keys::UserProfileKey},
    mls_group_config::{AppComponent, default_leaf_node_extensions},
};
use airprotos::client::{
    component::AirComponent,
    group::{EncryptedGroupTitle, GroupData},
};
use anyhow::Context;
use openmls::{components::vc_derivation_info::EpochId, group::GroupId};
use openmls_traits::OpenMlsProvider;
use tracing::debug;

use crate::{
    Chat,
    chats::{ChatAttributes, GroupDataExt},
    clients::{CoreUser, own_client_info::OwnClientInfo},
    db::access::{ReadConnection, WriteConnection},
    groups::{Group, openmls_provider::AirOpenMlsProvider},
    key_stores::indexed_keys::StorableIndexedKey,
};

/// Title of the per-user "self group" chat, as shown in the UI.
pub(crate) const SELF_CHAT_TITLE: &str = "Notes to self";

#[derive(Debug)]
pub struct SelfGroup {
    group: Group,
}

impl SelfGroup {
    pub(crate) async fn load(mut connection: impl ReadConnection) -> sqlx::Result<Option<Self>> {
        if let Some(group_id) = OwnClientInfo::load_self_group_id(&mut connection).await? {
            match Group::load(connection, &group_id).await? {
                Some(group) => {
                    debug!("Self-group found");
                    Ok(Some(SelfGroup { group }))
                }
                None => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    pub fn group_id(&self) -> &GroupId {
        self.group.group_id()
    }

    pub(crate) fn identity_link_wrapper_key(&self) -> &IdentityLinkWrapperKey {
        self.group.identity_link_wrapper_key()
    }

    /// Register a virtual-clients emulation epoch on both the classical and
    /// post-quantum groups.
    pub(crate) fn register_vc_emulation_epoch(
        &mut self,
        mut connection: impl WriteConnection,
    ) -> anyhow::Result<EpochId> {
        let provider = AirOpenMlsProvider::new(connection.as_mut());
        let (t_group, _) = self.group.apq_mls_groups_mut()?;
        let t_epoch_id = t_group
            .register_vc_emulation_epoch(provider.crypto(), provider.storage())
            .context("register VC emulation epoch (t)")?;
        Ok(t_epoch_id)
    }
}

impl CoreUser {
    pub(crate) async fn ensure_self_group(&self) -> anyhow::Result<SelfGroup> {
        if let Some(group) = SelfGroup::load(self.db().read().await?).await? {
            return Ok(group);
        }

        let group = self.create_self_group().await?;
        Ok(SelfGroup { group })
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

        let identity_link_wrapper_key = IdentityLinkWrapperKey::random()?;
        let chat_attributes = ChatAttributes {
            title: SELF_CHAT_TITLE.to_owned(),
            picture: None,
        };
        let encrypted_title =
            EncryptedGroupTitle::encrypt(&chat_attributes.title, &identity_link_wrapper_key)
                .context("Failed to encrypt self-group title")?;
        let group_data_bytes = GroupData {
            legacy_title: Some(chat_attributes.title.clone()),
            legacy_picture: None,
            encrypted_title: Some(encrypted_title),
            external_group_profile: None,
        }
        .encode()?;

        // Advertise the virtual-clients component in this group's leaves
        let vc_leaf_extensions = default_leaf_node_extensions::<AirComponent>();

        // The client signing-key is shared among all emulators, and we use it to sign all request
        // as well as leaves in high-level groups. The self-group leaves are signed with a freshly
        // minted signing key.
        let key_store = self.key_store();
        let self_group_signing_key = ClientSigningKey::from_prelim_key_with_foreign_credential(
            PreliminaryClientSigningKey::generate()?,
            key_store.signing_key.credential().clone(),
        )?;

        let group_signer = self_group_signing_key.clone();
        let (group, partial_params, user_profile_key) = self
            .db()
            .with_write_transaction(async move |txn| -> anyhow::Result<_> {
                let safe_aad_components = None;
                let (group, partial_params) = Group::create_apq_group(
                    &mut *txn,
                    &group_signer,
                    identity_link_wrapper_key,
                    group_id,
                    pq_group_id,
                    group_data_bytes,
                    safe_aad_components,
                    AirComponent::default_for_self_group(),
                    Some(vc_leaf_extensions),
                )?;

                let user_profile_key = UserProfileKey::load_own(&mut *txn).await?;

                group.store(&mut *txn).await?;

                // Create the "Notes to self" chat so the self group shows in the UI.
                let chat = Chat::new_group_chat(group.group_id().clone(), chat_attributes);
                chat.store(&mut *txn).await?;

                Ok((group, partial_params, user_profile_key))
            })
            .await?;

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
                .with_write_transaction(async |txn| -> anyhow::Result<()> {
                    Group::delete_from_db(&mut *txn, group.group_id()).await?;
                    if let Ok(chat_id) = crate::ChatId::try_from(group.group_id()) {
                        Chat::delete(txn, chat_id).await?;
                    }
                    Ok(())
                })
                .await?;
            return Err(error.into());
        }

        // Update the local reference
        OwnClientInfo::set_self_group(
            self.db().write().await?,
            group.group_id(),
            &self_group_signing_key,
        )
        .await?;

        Ok(group)
    }
}
