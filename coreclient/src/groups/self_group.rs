// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

#[cfg(any(test, feature = "test_utils"))]
use aircommon::credentials::LeafCredentialError;
use aircommon::{
    credentials::{
        LeafCredential,
        keys::{LeafSigningKey, SelfGroupSigningKey},
    },
    crypto::{aead::keys::IdentityLinkWrapperKey, indexed_aead::keys::UserProfileKey},
    messages::{
        client_ds::{AadMessage, AadPayload, GroupOperationParamsAad},
        client_ds_out::ApqGroupOperationParamsOut,
    },
    time::TimeStamp,
};
use airprotos::client::{
    group::GroupData,
    virtual_client::{
        VirtualClientAction, VirtualClientCommitData, extract_virtual_client_commit_data,
    },
};
use anyhow::{Context, bail, ensure};
use openmls::{
    components::vc_derivation_info::{KeyPackageUpload, process_vc_key_package_upload},
    group::GroupId,
    prelude::{LeafNodeIndex, ProcessedMessage},
};
use openmls_traits::OpenMlsProvider;
use tls_codec::Serialize;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::{
    Chat, ChatId, ChatMessage, SystemMessage,
    chats::ChatAttributes,
    clients::{CoreUser, own_client_info::OwnClientInfo},
    db::access::{ReadConnection, ReadTransaction, WriteConnection, WriteDbTransaction},
    groups::{Group, NewGroupContext, VerifiedGroup, openmls_provider::AirOpenMlsProvider},
    key_stores::{
        HeterogeneousVcKeyPackageBatch,
        indexed_keys::StorableIndexedKey,
        key_package_refs::{delete_orphaned_key_packages, mark_key_packages_as_live},
    },
};

/// Title of the per-user "self group" chat, as shown in the UI.
pub(crate) const SELF_CHAT_TITLE: &str = "Notes to self";

#[derive(Debug)]
pub struct SelfGroup {
    group: Group,
}

impl SelfGroup {
    #[cfg(test)]
    pub(crate) fn new_for_test(group: Group) -> Self {
        Self { group }
    }

    pub(crate) fn group(&self) -> &Group {
        &self.group
    }

    pub(crate) fn group_mut(&mut self) -> &mut Group {
        &mut self.group
    }

    pub(crate) fn into_verified_group(self) -> VerifiedGroup {
        VerifiedGroup(self.group)
    }

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

    pub(crate) async fn has_linked_devices(
        mut connection: impl ReadConnection,
    ) -> sqlx::Result<bool> {
        let Some(group_id) = OwnClientInfo::load_self_group_id(&mut connection).await? else {
            return Ok(false);
        };
        let Some(group) = Group::load(connection, &group_id).await? else {
            debug!("self group not joined yet, assuming linked devices");
            return Ok(true);
        };
        let self_group = Self { group };
        match self_group.client_ids() {
            Ok(client_ids) => Ok(client_ids.len() > 1),
            Err(error) => {
                // Since there is a self group, there is a channel to other
                // devices, so assume there are some.
                warn!(%error, "cannot count linked devices, assuming there are some");
                Ok(true)
            }
        }
    }

    /// The chat of the self group, if there is one.
    pub(crate) async fn load_chat(mut txn: impl ReadTransaction) -> sqlx::Result<Option<Chat>> {
        let Some(group_id) = OwnClientInfo::load_self_group_id(&mut txn).await? else {
            return Ok(None);
        };
        Chat::load_by_group_id(txn, &group_id).await
    }

    pub fn group_id(&self) -> &GroupId {
        self.group.group_id()
    }

    /// The client ids of all leaves in this group, in member order.
    pub fn client_ids(&self) -> anyhow::Result<Vec<Uuid>> {
        self.group
            .mls_group()
            .members()
            .map(
                |member| match LeafCredential::from_credential(&member.credential)? {
                    LeafCredential::SelfGroup(credential) => Ok(credential.client_id()),
                    LeafCredential::User(_) => {
                        bail!("a self-group leaf carries a user credential")
                    }
                },
            )
            .collect()
    }

    /// The parsed leaf credentials of the self-group members, in member order.
    #[cfg(any(test, feature = "test_utils"))]
    pub fn credentials(&self) -> Result<Vec<LeafCredential>, LeafCredentialError> {
        self.group
            .mls_group()
            .members()
            .map(|member| LeafCredential::from_credential(&member.credential))
            .collect()
    }

    /// Stages an empty self-update commit on the self-group carrying a [`KeyPackageUpload`] in its
    /// SafeAAD.
    ///
    /// The DS extracts the hint from the T commit and asks the QS to promote the previously staged
    /// key packages.
    pub(crate) fn stage_key_package_upload(
        &mut self,
        mut connection: impl WriteConnection,
        signer: &SelfGroupSigningKey,
        upload: KeyPackageUpload,
    ) -> anyhow::Result<ApqGroupOperationParamsOut> {
        let provider = AirOpenMlsProvider::new(connection.as_mut());
        let (t_mls_group, pq_mls_group) = self.group.apq_mls_groups_mut()?;

        // SafeAAD hint the DS extracts from the T commit to trigger promotion.
        let commit_data =
            VirtualClientCommitData::new(vec![VirtualClientAction::KeyPackageUpload(upload)])?;
        t_mls_group.set_safe_aad(vec![commit_data.to_safe_aad_item()?])?;

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

    /// Stages a commit removing the self-group leaves with the given client ids.
    pub(crate) async fn stage_apq_remove_clients(
        &mut self,
        mut connection: impl WriteConnection,
        signer: &SelfGroupSigningKey,
        mut client_ids: Vec<Uuid>,
    ) -> anyhow::Result<ApqGroupOperationParamsOut> {
        let mut remove_indices = Vec::with_capacity(client_ids.len());
        for member in self.group.mls_group.members() {
            let LeafCredential::SelfGroup(credential) =
                LeafCredential::from_credential(&member.credential)?
            else {
                bail!("a self-group leaf carries a user credential");
            };
            if let Some(idx) = client_ids
                .iter()
                .position(|id| id == &credential.client_id())
            {
                remove_indices.push(member.index);
                client_ids.swap_remove(idx);
            }
            if client_ids.is_empty() {
                break;
            }
        }
        ensure!(
            client_ids.is_empty(),
            "clients to remove are not in the self group: {client_ids:?}"
        );

        let vc_group_id = self
            .group
            .resolve_vc_emulation_group(&mut connection)
            .await?;
        let provider = AirOpenMlsProvider::new(connection.as_mut());
        let (t_mls_group, pq_mls_group) = self.group.apq_mls_groups_mut()?;

        let aad_payload = AadPayload::GroupOperation(GroupOperationParamsAad {
            new_encrypted_user_profile_keys: Vec::new(),
        });
        let aad = AadMessage::from(aad_payload).tls_serialize_detached()?;
        t_mls_group.set_aad(aad);

        let mut builder =
            apqmls::commit_builder::CommitBuilder::from_groups(t_mls_group, pq_mls_group)
                .force_self_update(true)
                .propose_removals(remove_indices)
                .create_group_info(true);
        if let Some(group_id) = vc_group_id {
            builder = builder.vc_emulation(group_id);
        }
        let bundle = builder.finalize(&provider, signer, |_| true, |_| true)?;

        debug_assert!(bundle.welcome.is_none());
        ensure!(
            bundle.group_info.is_some(),
            "No group info in APQMLS bundle"
        );

        Ok(ApqGroupOperationParamsOut {
            bundle,
            encrypted_welcome_attribution_infos: Vec::new(),
        })
    }
}

impl CoreUser {
    /// Creates the self group if this client never had one. A client that is
    /// still joining one (linked, Welcome not processed yet) is left alone.
    pub async fn ensure_self_group(&self) -> anyhow::Result<SelfGroup> {
        let _creation = self.inner.self_group_creation.lock().await;
        let self_group = match SelfGroup::load(self.db().read().await?).await? {
            Some(self_group) => self_group,
            None => {
                ensure!(
                    OwnClientInfo::load_self_group_id(self.db().read().await?)
                        .await?
                        .is_none(),
                    "self group not joined yet"
                );
                SelfGroup {
                    group: self.create_self_group().await?,
                }
            }
        };
        self.ensure_self_chat(self_group.group_id()).await?;
        Ok(self_group)
    }

    /// Creates the "Notes to self" chat of the self group if it is missing, so
    /// the group shows in the UI.
    ///
    /// Clients whose self group predates that chat only have the group, so
    /// their self chat has to be backfilled here.
    async fn ensure_self_chat(&self, group_id: &GroupId) -> anyhow::Result<()> {
        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<()> {
                if ChatId::load_from_group_id(&mut *txn, group_id)
                    .await?
                    .is_some()
                {
                    return Ok(());
                }

                let chat = Chat::new_group_chat(
                    group_id.clone(),
                    ChatAttributes {
                        title: SELF_CHAT_TITLE.to_owned(),
                        picture: None,
                    },
                );
                chat.store(&mut *txn).await?;
                ChatMessage::new_system_message(
                    chat.id(),
                    TimeStamp::now(),
                    SystemMessage::SelfChatCreated,
                )
                .store(&mut *txn)
                .await?;
                debug!("Created the missing self chat");

                Ok(())
            })
            .await?;

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

        let identity_link_wrapper_key = IdentityLinkWrapperKey::random()?;
        // The self chat's title is the local constant, so the group carries no profile.
        let group_data = GroupData::empty();

        // Self-group leaves carry a SelfGroupCredential that identifies the device by its client
        // id and are signed by a per-device key. The creation request itself is authenticated by
        // the user credential sent alongside it.
        let key_store = self.key_store();
        let client_id = OwnClientInfo::load(self.db().read().await?)
            .await?
            .client_id;
        let self_group_signing_key = SelfGroupSigningKey::generate(client_id)?;

        let group_signer = LeafSigningKey::SelfGroup(self_group_signing_key.clone());
        let own_user_id = self.user_id().clone();
        let (group, partial_params, user_profile_key) = self
            .db()
            .with_write_transaction(async move |txn| -> anyhow::Result<_> {
                let (group, partial_params) = Group::create_apq_group(
                    &mut *txn,
                    &group_signer,
                    own_user_id,
                    identity_link_wrapper_key,
                    group_id,
                    pq_group_id,
                    NewGroupContext::SelfGroup(group_data),
                    // The self group is the emulation group itself, not a
                    // virtual client of one.
                    None,
                )?;

                let user_profile_key = UserProfileKey::load_own(&mut *txn).await?;

                group.store(&mut *txn).await?;

                Ok((group, partial_params, user_profile_key))
            })
            .await?;

        let client_reference = self.create_own_client_reference();
        let encrypted_user_profile_key =
            user_profile_key.encrypt(group.identity_link_wrapper_key(), self.user_id())?;
        let mut params = partial_params.into_params(client_reference, encrypted_user_profile_key);
        // Self-group leaves carry no user credential, so the creation is authenticated by the
        // user credential sent with the request instead.
        params.creator_user_credential = Some(key_store.signing_key.credential().clone());

        // Create group on the server
        if let Err(error) = api_client
            .ds_create_group(params, &key_store.signing_key, group.group_state_ear_key())
            .await
        {
            self.db()
                .with_write_transaction(async |txn| -> anyhow::Result<()> {
                    Group::delete_from_db(&mut *txn, group.group_id()).await?;
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

    /// Resets the self group and its chat from the local database only. The
    /// server and linked devices are not told.
    pub async fn danger_reset_self_group(&self) -> anyhow::Result<()> {
        if SelfGroup::has_linked_devices(self.db().read().await?).await? {
            anyhow::bail!("You can only delete a self-group if you have no linked devices!");
        }

        self.db()
            .with_write_transaction(async |txn| -> sqlx::Result<()> {
                let Some(group_id) = OwnClientInfo::load_self_group_id(&mut *txn).await? else {
                    return Ok(());
                };
                if let Some(chat_id) = ChatId::load_from_group_id(&mut *txn, &group_id).await? {
                    Chat::delete(&mut *txn, chat_id).await?;
                }
                Group::delete_from_db(&mut *txn, &group_id).await?;
                OwnClientInfo::clear_self_group(txn).await?;
                Ok(())
            })
            .await?;

        Box::pin(self.ensure_self_group()).await?;
        Ok(())
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
        let Some(commit_data) = extract_virtual_client_commit_data(processed_message)? else {
            return Ok(());
        };

        let own_client_info = OwnClientInfo::load(&mut *txn).await?;
        ensure!(
            own_client_info.self_group_id.as_ref() == Some(self.group_id()),
            "virtual-client component outside the self-group"
        );

        for upload in commit_data.key_package_uploads() {
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
                let epoch_id = self
                    .mls_group()
                    .newest_vc_derivation_epoch(provider.storage())?
                    .context("self group has no derivation epoch")?;
                ensure!(
                    epoch_id == upload.epoch_id,
                    "KeyPackageUpload references a foreign emulation epoch"
                );
                process_vc_key_package_upload(&provider, upload)?;
            }

            // The sibling's batch replaces the served set; track it as live.
            let (plain_refs, apq_refs) =
                HeterogeneousVcKeyPackageBatch::split_vc_batch_refs(&upload.key_package_info)?;
            mark_key_packages_as_live(&mut *txn, &plain_refs, false).await?;
            mark_key_packages_as_live(&mut *txn, &apq_refs, true).await?;
            delete_orphaned_key_packages(&mut *txn).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use aircommon::{
        codec,
        identifiers::{QsClientId, QsUserId, QualifiedGroupId, UserId},
    };

    use crate::{
        db::access::DbAccess,
        groups::openmls_provider::KeyRefWrapper,
        utils::persistence::{GroupIdRefWrapper, open_db_in_memory},
    };

    use super::*;

    const STORED_CIPHERSUITE: &[u8] = b"MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519";
    // Same length as the stored name, so the CBOR text header stays valid.
    const UNKNOWN_CIPHERSUITE: &[u8] = b"MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed00000";

    /// Creates a local self-group, without the DS, and points
    /// `own_client_info` at it. Returns the T and PQ group ids.
    async fn create_local_self_group(
        txn: &mut WriteDbTransaction<'_>,
        user_id: &UserId,
    ) -> anyhow::Result<(GroupId, GroupId)> {
        let signing_key = SelfGroupSigningKey::generate(Uuid::new_v4())?;
        let t_group_id = GroupId::from(QualifiedGroupId::new(
            Uuid::new_v4(),
            "example.com".parse()?,
        ));
        let pq_group_id = GroupId::from(QualifiedGroupId::new(
            Uuid::new_v4(),
            "example.com".parse()?,
        ));
        let (group, _params) = Group::create_apq_group(
            &mut *txn,
            &LeafSigningKey::SelfGroup(signing_key.clone()),
            user_id.clone(),
            IdentityLinkWrapperKey::random()?,
            t_group_id.clone(),
            pq_group_id.clone(),
            NewGroupContext::SelfGroup(GroupData::empty()),
            None,
        )?;
        group.store(&mut *txn).await?;
        OwnClientInfo::set_self_group(&mut *txn, group.group_id(), &signing_key).await?;
        Ok((t_group_id, pq_group_id))
    }

    /// Rewrites the ciphersuite in the persisted `GroupContext` of `group_id`
    /// to a name OpenMLS does not know.
    async fn corrupt_ciphersuite(
        txn: &mut WriteDbTransaction<'_>,
        group_id: &GroupId,
    ) -> anyhow::Result<()> {
        let mut context: Vec<u8> = sqlx::query_scalar(
            "SELECT group_data FROM group_data WHERE group_id = ? AND data_type = 'context'",
        )
        .bind(KeyRefWrapper(group_id))
        .fetch_one(txn.as_mut())
        .await?;
        let start = context
            .windows(STORED_CIPHERSUITE.len())
            .position(|window| window == STORED_CIPHERSUITE)
            .context("ciphersuite not found in the stored group context")?;
        context[start..start + UNKNOWN_CIPHERSUITE.len()].copy_from_slice(UNKNOWN_CIPHERSUITE);
        sqlx::query(
            "UPDATE group_data SET group_data = ? WHERE group_id = ? AND data_type = 'context'",
        )
        .bind(context)
        .bind(KeyRefWrapper(group_id))
        .execute(txn.as_mut())
        .await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn corrupted_self_group_ciphersuite_is_recreated() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let user_id = UserId::random("example.com".parse()?);

        let mut connection = pool.write().await?;
        connection
            .with_transaction(async |txn| -> anyhow::Result<()> {
                OwnClientInfo {
                    qs_user_id: QsUserId::random(),
                    qs_client_id: QsClientId::random(&mut rand::rng()),
                    user_id: user_id.clone(),
                    client_id: Uuid::new_v4(),
                    self_group_id: None,
                    self_group_signing_key: None,
                }
                .store(&mut *txn)
                .await?;

                let (group_id, pq_group_id) = create_local_self_group(txn, &user_id).await?;
                corrupt_ciphersuite(txn, &group_id).await?;

                let error = SelfGroup::load(&mut *txn)
                    .await
                    .expect_err("loaded a self-group with an unknown ciphersuite");
                println!("SelfGroup::load: {error}\n{error:#?}");
                let sqlx::Error::ColumnDecode { source, .. } = &error else {
                    panic!("unexpected error: {error:?}");
                };
                assert!(
                    source.downcast_ref::<codec::Error>().is_some(),
                    "unexpected decode error: {source:?}"
                );

                // Deleting loads the group first, so it fails the same way.
                let error = Group::delete_from_db(txn, &group_id)
                    .await
                    .expect_err("deleted a group that cannot be loaded");
                assert!(matches!(error, sqlx::Error::ColumnDecode { .. }));

                // Recovery: drop the rows of the broken group without loading
                // it, then create a fresh self-group. Other OpenMLS rows of the
                // old group (own leaf nodes, epoch key pairs, ...) stay orphaned.
                for id in [&group_id, &pq_group_id] {
                    sqlx::query("DELETE FROM group_data WHERE group_id = ?")
                        .bind(KeyRefWrapper(id))
                        .execute(txn.as_mut())
                        .await?;
                }
                sqlx::query(r#"DELETE FROM "group" WHERE group_id = ?"#)
                    .bind(GroupIdRefWrapper::from(&group_id))
                    .execute(txn.as_mut())
                    .await?;
                assert!(Group::load(&mut *txn, &group_id).await?.is_none());

                let (new_group_id, _) = create_local_self_group(txn, &user_id).await?;
                let self_group = SelfGroup::load(&mut *txn)
                    .await?
                    .context("no self-group after recreation")?;
                assert_eq!(self_group.group_id(), &new_group_id);

                Ok(())
            })
            .await
    }
}
