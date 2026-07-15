// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ops::{Deref, DerefMut};

use aircommon::{
    credentials::keys::ClientSigningKey,
    crypto::{aead::keys::IdentityLinkWrapperKey, indexed_aead::keys::UserProfileKey},
    messages::{
        client_ds::{AadMessage, AadPayload, GroupOperationParamsAad},
        client_ds_out::ApqGroupOperationParamsOut,
    },
    mls_group_config::AppComponent,
};
use airprotos::client::{
    component::AirComponent,
    group::GroupData,
    virtual_client::{VIRTUAL_CLIENT_KP_UPLOAD_COMPONENT_ID, extract_key_package_upload},
};
use anyhow::{Context, ensure};
use openmls::{
    components::vc_derivation_info::{KeyPackageUpload, process_vc_key_package_upload},
    framing::SafeAadItem,
    prelude::{LeafNodeIndex, ProcessedMessage},
};
use openmls_traits::OpenMlsProvider;
use tls_codec::Serialize;
use tracing::{debug, warn};

use crate::{
    chats::GroupDataExt,
    clients::{CoreUser, own_client_info::OwnClientInfo},
    db::access::{ReadConnection, WriteConnection, WriteDbTransaction},
    groups::{Group, openmls_provider::AirOpenMlsProvider},
    key_stores::{
        HeterogeneousVcKeyPackageBatch,
        indexed_keys::StorableIndexedKey,
        key_package_refs::{delete_orphaned_key_packages, mark_key_packages_as_live},
    },
};

pub(crate) struct SelfGroup {
    group: Group,
}

impl Deref for SelfGroup {
    type Target = Group;

    fn deref(&self) -> &Self::Target {
        &self.group
    }
}

impl DerefMut for SelfGroup {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.group
    }
}

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

    async fn create_self_group(&self) -> anyhow::Result<SelfGroup> {
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

        Ok(SelfGroup { group })
    }
}

impl SelfGroup {
    pub(crate) async fn load(mut read: impl ReadConnection) -> anyhow::Result<Option<Self>> {
        let own_client_info = OwnClientInfo::load(&mut read).await?;
        let Some(group_id) = own_client_info.self_group_id else {
            return Ok(None);
        };
        let group = Group::load(read, &group_id).await?;
        Ok(group.map(|group| Self { group }))
    }

    /// Stages an empty self-update commit on the self-group carrying a [`KeyPackageUpload`] in its
    /// SafeAAD.
    ///
    /// The DS extracts the hint from the T commit and asks the QS to promote the previously staged
    /// key packages.
    pub(crate) fn stage_key_package_upload(
        &mut self,
        mut connection: impl WriteConnection,
        signer: &ClientSigningKey,
        upload: KeyPackageUpload,
    ) -> anyhow::Result<ApqGroupOperationParamsOut> {
        let provider = AirOpenMlsProvider::new(connection.as_mut());
        let (t_mls_group, pq_mls_group) = self.group.apq_mls_groups_mut()?;

        // SafeAAD hint the DS extracts from the T commit to trigger promotion.
        let upload_bytes = upload.tls_serialize_detached()?;
        t_mls_group.set_safe_aad(vec![SafeAadItem::new(
            VIRTUAL_CLIENT_KP_UPLOAD_COMPONENT_ID,
            upload_bytes,
        )])?;

        // Regular AAD tail (required by DS commit validation)
        let aad_payload = AadPayload::GroupOperation(GroupOperationParamsAad {
            new_encrypted_user_profile_keys: Vec::new(),
        });
        let aad = AadMessage::from(aad_payload).tls_serialize_detached()?;
        t_mls_group.set_aad(aad);

        let bundle = apqmls::commit_builder::CommitBuilder::from_groups(t_mls_group, pq_mls_group)
            .force_self_update(true)
            .create_group_info(true)
            .finalize(&provider, signer, |_| true, |_| true)?;

        ensure!(
            bundle.group_info.is_some(),
            "No group info in APQMLS bundle"
        );

        Ok(ApqGroupOperationParamsOut {
            bundle,
            encrypted_welcome_attribution_infos: Default::default(),
        })
    }
}

impl Group {
    /// Processes a sibling's [`KeyPackageUpload`] announced in the SafeAAD of
    /// a self-group commit: derives the sibling's key package material from
    /// the shared operation tree and marks the announced refs as the new live
    /// set.
    ///
    /// A no-op for commits without the component. Only the self-group may
    /// carry it.
    pub(crate) async fn process_vc_key_package_upload_aad(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        processed_message: &ProcessedMessage,
        sender_index: LeafNodeIndex,
    ) -> anyhow::Result<()> {
        let Some(upload) = extract_key_package_upload(processed_message)? else {
            return Ok(());
        };

        let own_client_info = OwnClientInfo::load(&mut *txn).await?;
        ensure!(
            own_client_info.self_group_id.as_ref() == Some(self.group_id()),
            "KeyPackageUpload component outside the self-group"
        );
        ensure!(
            upload.leaf_index == sender_index,
            "KeyPackageUpload for a leaf other than the sender"
        );
        ensure!(
            upload.leaf_index != self.mls_group().own_leaf_index(),
            "Sibling KeyPackageUpload from own leaf"
        );

        {
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            // A passive sibling may not have registered this epoch's
            // operation tree yet. Registration is idempotent and must happen
            // at the pre-merge epoch, which is the epoch the upload
            // references.
            let epoch_id = self
                .mls_group_mut()
                .register_vc_emulation_epoch(provider.crypto(), provider.storage())?;
            ensure!(
                epoch_id == upload.epoch_id,
                "KeyPackageUpload references a foreign emulation epoch"
            );
            process_vc_key_package_upload(&provider, &upload)?;
        }

        // The sibling's batch replaces the served set; track it as live.
        let (plain_refs, apq_refs) =
            HeterogeneousVcKeyPackageBatch::split_vc_batch_refs(&upload.key_package_info)?;
        mark_key_packages_as_live(&mut *txn, &plain_refs, false).await?;
        mark_key_packages_as_live(&mut *txn, &apq_refs, true).await?;
        delete_orphaned_key_packages(&mut *txn).await?;

        Ok(())
    }
}
