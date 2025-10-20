// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use anyhow::{Context, Result, anyhow, bail};
use create_chat_flow::IntitialChatData;
use delete_chat_flow::DeleteChatData;
use leave_chat_flow::LeaveChatData;
use mimi_room_policy::VerifiedRoomState;
use tracing::error;

use crate::{
    MessageId,
    chats::{Chat, messages::ChatMessage},
    groups::{Group, openmls_provider::AirOpenMlsProvider},
    utils::{connection_ext::StoreExt, image::resize_profile_image},
};

use super::{ChatId, CoreUser};

impl CoreUser {
    /// Create new chat.
    ///
    /// Returns the id of the newly created chat.
    pub(crate) async fn create_chat(
        &self,
        title: String,
        picture: Option<Vec<u8>>,
    ) -> Result<ChatId> {
        let group_data = IntitialChatData::new(title, picture)
            .request_group_id(&self.inner.api_clients)
            .await?;

        let created_group = self
            .with_transaction_and_notifier(async |connection, notifier| {
                group_data
                    .create_group(
                        &AirOpenMlsProvider::new(&mut *connection),
                        &self.inner.key_store.signing_key,
                    )?
                    .store_group(&mut *connection, notifier)
                    .await
            })
            .await?;

        created_group
            .create_group_on_ds(
                &self.inner.api_clients,
                self.signing_key(),
                self.create_own_client_reference(),
            )
            .await
    }

    /// Delete the chat with the given [`ChatId`].
    ///
    /// Since this function causes the creation of an MLS commit, it can cause
    /// more than one effect on the group. As a result this function returns a
    /// vector of [`ChatMessage`]s that represents the changes to the
    /// group. Note that these returned message have already been persisted.
    pub(crate) async fn delete_chat(&self, chat_id: ChatId) -> Result<Vec<ChatMessage>> {
        // Phase 1: Load the chat and the group
        let mut txn = self.pool().begin_with("BEGIN IMMEDIATE").await?;

        let delete_chat_data = DeleteChatData::load(&mut txn, chat_id).await?;

        match delete_chat_data {
            DeleteChatData::SingleMember(data) => {
                // No need to send a message to the server if we are the only member.
                // Phase 5: Set the chat to inactive
                self.with_notifier(async |notifier| data.set_inactive(&mut txn, notifier).await)
                    .await?;
                txn.commit().await?;

                Ok(Vec::new())
            }
            DeleteChatData::MultiMember(data) => {
                // Phase 2: Create the delete commit
                let delete = data
                    .stage_delete_commit(&mut txn, self.signing_key())
                    .await?;
                txn.commit().await?;

                // Phase 3: Send the delete to the DS
                let deleted = delete
                    .send_delete_commit(&self.inner.api_clients, self.signing_key())
                    .await?;
                // TODO: Retry send until we get a response
                self.with_transaction_and_notifier(async |connection, notifier| {
                    deleted
                        // Phase 4: Merge the commit into the group
                        .merge_pending_commit(&mut *connection)
                        .await?
                        // Phase 5: Set the chat to inactive
                        .set_inactive(&mut *connection, notifier, chat_id)
                        .await
                })
                .await
            }
        }
    }

    pub(crate) async fn erase_chat(&self, chat_id: ChatId) -> Result<()> {
        self.with_transaction_and_notifier(async |txn, notifier| {
            let chat = Chat::load(txn.as_mut(), &chat_id)
                .await?
                .context("missing chat for deletion")?;
            Group::delete_from_db(txn, chat.group_id())
                .await
                .inspect_err(|error| {
                    error!(%error, "failed to delete group; skipping");
                })
                .ok();
            Chat::delete(txn.as_mut(), notifier, chat.id()).await?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn leave_chat(&self, chat_id: ChatId) -> Result<()> {
        let leave = self
            .with_transaction(async |txn| {
                // Phase 1: Load the chat and the group
                LeaveChatData::load(txn, chat_id)
                    .await?
                    .stage_leave_group(self.user_id(), txn, self.signing_key())
                    .await
            })
            .await?;

        // Phase 2: Send the leave to the DS
        let leave = leave
            .ds_self_remove(&self.inner.api_clients, self.signing_key())
            .await?;
        self.with_transaction_and_notifier(async |txn, notifier| {
            // Phase 3: Merge the commit into the group
            leave
                .store_update(txn, notifier, chat_id, self.user_id())
                .await
        })
        .await?;
        Ok(())
    }

    pub(crate) async fn set_chat_picture(
        &self,
        chat_id: ChatId,
        picture: Option<Vec<u8>>,
    ) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        let mut chat = Chat::load(&mut connection, &chat_id)
            .await?
            .ok_or_else(|| {
                let id = chat_id.uuid();
                anyhow!("Can't find chat with id {id}")
            })?;
        let resized_picture_option =
            picture.and_then(|picture| resize_profile_image(&picture).ok());
        let mut notifier = self.store_notifier();
        chat.set_picture(&mut *connection, &mut notifier, resized_picture_option)
            .await?;
        notifier.notify();
        Ok(())
    }

    pub(crate) async fn message(&self, message_id: MessageId) -> sqlx::Result<Option<ChatMessage>> {
        ChatMessage::load(self.pool(), message_id).await
    }

    pub(crate) async fn prev_message(&self, message_id: MessageId) -> Result<Option<ChatMessage>> {
        Ok(ChatMessage::prev_message(self.pool(), message_id).await?)
    }

    pub(crate) async fn next_message(&self, message_id: MessageId) -> Result<Option<ChatMessage>> {
        Ok(ChatMessage::next_message(self.pool(), message_id).await?)
    }

    pub(crate) async fn chats(&self) -> sqlx::Result<Vec<Chat>> {
        Chat::load_all(self.pool().acquire().await?.as_mut()).await
    }

    pub async fn chat(&self, chat: &ChatId) -> Option<Chat> {
        Chat::load(self.pool().acquire().await.ok()?.as_mut(), chat)
            .await
            .ok()
            .flatten()
    }

    /// Get the most recent `number_of_messages` messages from the chat with the given [`ChatId`].
    pub(crate) async fn get_messages(
        &self,
        chat_id: ChatId,
        number_of_messages: usize,
    ) -> Result<Vec<ChatMessage>> {
        let messages =
            ChatMessage::load_multiple(self.pool(), chat_id, number_of_messages as u32).await?;
        Ok(messages)
    }

    pub async fn load_room_state(&self, chat_id: &ChatId) -> Result<(UserId, VerifiedRoomState)> {
        if let Some(chat_id) = self.chat(chat_id).await {
            let mut connection = self.pool().acquire().await?;
            if let Some(group) = Group::load(&mut connection, chat_id.group_id()).await? {
                return Ok((self.user_id().clone(), group.room_state));
            }
        }
        bail!("Room does not exist")
    }
}

mod create_chat_flow {
    use aircommon::{
        codec::PersistenceCodec,
        credentials::keys::ClientSigningKey,
        crypto::{ear::keys::EncryptedUserProfileKey, indexed_aead::keys::UserProfileKey},
        identifiers::QsReference,
    };
    use anyhow::Result;
    use openmls::group::GroupId;
    use openmls_traits::OpenMlsProvider;

    use crate::{
        Chat, ChatAttributes, ChatId,
        clients::api_clients::ApiClients,
        groups::{Group, GroupData, PartialCreateGroupParams, client_auth_info::GroupMembership},
        key_stores::indexed_keys::StorableIndexedKey,
        store::StoreNotifier,
    };

    pub(super) struct IntitialChatData {
        title: String,
        picture: Option<Vec<u8>>,
    }

    impl IntitialChatData {
        pub(super) fn new(title: String, picture: Option<Vec<u8>>) -> Self {
            Self { title, picture }
        }

        pub(super) async fn request_group_id(
            self,
            api_clients: &ApiClients,
        ) -> Result<ChatGroupData> {
            let Self { title, picture } = self;
            let group_id = api_clients.default_client()?.ds_request_group_id().await?;
            // Store the chat attributes in the group's aad
            let attributes = ChatAttributes::new(title, picture);
            let group_data = PersistenceCodec::to_vec(&attributes)?.into();
            Ok(ChatGroupData {
                group_id,
                group_data,
                attributes,
            })
        }
    }

    pub(super) struct ChatGroupData {
        group_id: GroupId,
        group_data: GroupData,
        attributes: ChatAttributes,
    }

    pub(super) struct CreatedGroup {
        group: Group,
        group_membership: GroupMembership,
        partial_params: PartialCreateGroupParams,
        attributes: ChatAttributes,
    }

    impl ChatGroupData {
        pub(super) fn create_group(
            self,
            provider: &impl OpenMlsProvider,
            signing_key: &ClientSigningKey,
        ) -> Result<CreatedGroup> {
            let Self {
                group_id,
                group_data,
                attributes,
            } = self;

            let (group, group_membership, partial_params) =
                Group::create_group(provider, signing_key, group_id, group_data)?;

            Ok(CreatedGroup {
                group,
                group_membership,
                partial_params,
                attributes,
            })
        }
    }

    impl CreatedGroup {
        pub(super) async fn store_group(
            self,
            txn: &mut sqlx::SqliteTransaction<'_>,
            notifier: &mut StoreNotifier,
        ) -> Result<StoredGroup> {
            let Self {
                group,
                group_membership,
                partial_params,
                attributes,
            } = self;

            let user_profile_key = UserProfileKey::load_own(txn.as_mut()).await?;
            let encrypted_user_profile_key = user_profile_key.encrypt(
                group.identity_link_wrapper_key(),
                group_membership.user_id(),
            )?;

            group_membership.store(txn.as_mut()).await?;
            group.store(txn.as_mut()).await?;

            let chat = Chat::new_group_chat(partial_params.group_id.clone(), attributes);
            chat.store(txn.as_mut(), notifier).await?;

            Ok(StoredGroup {
                group,
                encrypted_user_profile_key,
                partial_params,
                chat_id: chat.id(),
            })
        }
    }

    pub(super) struct StoredGroup {
        group: Group,
        encrypted_user_profile_key: EncryptedUserProfileKey,
        partial_params: PartialCreateGroupParams,
        chat_id: ChatId,
    }

    impl StoredGroup {
        pub(super) async fn create_group_on_ds(
            self,
            api_clients: &ApiClients,
            signer: &ClientSigningKey,
            client_reference: QsReference,
        ) -> Result<ChatId> {
            let Self {
                group,
                encrypted_user_profile_key,
                partial_params,
                chat_id,
            } = self;

            let params = partial_params.into_params(client_reference, encrypted_user_profile_key);
            api_clients
                .default_client()?
                .ds_create_group(params, signer, group.group_state_ear_key())
                .await?;

            Ok(chat_id)
        }
    }
}

mod delete_chat_flow {
    use std::collections::HashSet;

    use aircommon::{
        credentials::keys::ClientSigningKey, identifiers::UserId,
        messages::client_ds_out::DeleteGroupParamsOut, time::TimeStamp,
    };
    use anyhow::Context;
    use sqlx::{SqliteConnection, SqliteTransaction};

    use crate::{
        Chat, ChatId, ChatMessage,
        chats::messages::TimestampedMessage,
        clients::{CoreUser, api_clients::ApiClients},
        groups::Group,
        store::StoreNotifier,
    };

    pub(super) enum DeleteChatData {
        SingleMember(Box<LoadedSingleUserChatData>),
        MultiMember(Box<LoadedChatData<()>>),
    }

    impl DeleteChatData {
        pub(super) async fn load(
            txn: &mut SqliteTransaction<'_>,
            chat_id: ChatId,
        ) -> anyhow::Result<Self> {
            let chat = Chat::load(txn.as_mut(), &chat_id)
                .await?
                .with_context(|| format!("Can't find chat with id {chat_id}"))?;

            let group_id = chat.group_id();
            let group = Group::load_clean(txn, group_id)
                .await?
                .with_context(|| format!("Can't find group with id {group_id:?}"))?;

            let past_members = group.members(txn.as_mut()).await;

            if past_members.len() == 1 {
                let member = past_members.into_iter().next().unwrap();
                Ok(Self::SingleMember(
                    LoadedSingleUserChatData { chat, member }.into(),
                ))
            } else {
                Ok(Self::MultiMember(
                    LoadedChatData {
                        chat,
                        group,
                        past_members,
                        state: (),
                    }
                    .into(),
                ))
            }
        }
    }

    pub(super) struct LoadedSingleUserChatData {
        pub(super) chat: Chat,
        pub(super) member: UserId,
    }

    impl LoadedSingleUserChatData {
        pub(super) async fn set_inactive(
            self,
            connection: &mut SqliteConnection,
            notifier: &mut StoreNotifier,
        ) -> anyhow::Result<()> {
            let Self { mut chat, member } = self;
            chat.set_inactive(connection, notifier, vec![member])
                .await?;
            Ok(())
        }
    }

    pub(super) struct LoadedChatData<S> {
        chat: Chat,
        group: Group,
        past_members: HashSet<UserId>,
        state: S,
    }

    impl LoadedChatData<()> {
        pub(super) async fn stage_delete_commit(
            self,
            connection: &mut SqliteConnection,
            signer: &ClientSigningKey,
        ) -> anyhow::Result<LoadedChatData<DeleteGroupParamsOut>> {
            let Self {
                chat,
                mut group,
                past_members,
                state: _,
            } = self;
            let params = group.stage_delete(connection, signer).await?;
            Ok(LoadedChatData {
                chat,
                group,
                past_members,
                state: params,
            })
        }
    }

    impl LoadedChatData<DeleteGroupParamsOut> {
        pub(super) async fn send_delete_commit(
            self,
            api_clients: &ApiClients,
            signer: &ClientSigningKey,
        ) -> anyhow::Result<LoadedChatData<DeletedGroupOnDs>> {
            let Self {
                chat,
                group,
                past_members,
                state: params,
            } = self;
            let owner_domain = chat.owner_domain();
            let ds_timestamp = api_clients
                .get(&owner_domain)?
                .ds_delete_group(params, signer, group.group_state_ear_key())
                .await?;
            Ok(LoadedChatData {
                chat,
                group,
                past_members,
                state: DeletedGroupOnDs(ds_timestamp),
            })
        }
    }

    pub(super) struct DeletedGroupOnDs(TimeStamp);

    impl LoadedChatData<DeletedGroupOnDs> {
        pub(super) async fn merge_pending_commit(
            self,
            connection: &mut SqliteConnection,
        ) -> anyhow::Result<DeletedGroup> {
            let Self {
                chat,
                mut group,
                past_members,
                state: DeletedGroupOnDs(ds_timestamp),
            } = self;

            let messages = group
                .merge_pending_commit(connection, None, ds_timestamp)
                .await?;

            Ok(DeletedGroup {
                chat,
                past_members,
                messages,
            })
        }
    }

    pub(super) struct DeletedGroup {
        chat: Chat,
        past_members: HashSet<UserId>,
        messages: Vec<TimestampedMessage>,
    }

    impl DeletedGroup {
        pub(super) async fn set_inactive(
            self,
            connection: &mut SqliteConnection,
            notifier: &mut StoreNotifier,
            chat_id: ChatId,
        ) -> anyhow::Result<Vec<ChatMessage>> {
            let Self {
                mut chat,
                past_members,
                messages,
            } = self;
            chat.set_inactive(
                &mut *connection,
                notifier,
                past_members.into_iter().collect(),
            )
            .await?;
            CoreUser::store_new_messages(&mut *connection, notifier, chat_id, messages).await
        }
    }
}

mod leave_chat_flow {
    use aircommon::{
        credentials::keys::ClientSigningKey, identifiers::UserId,
        messages::client_ds_out::SelfRemoveParamsOut, time::TimeStamp,
    };
    use anyhow::Context;
    use mimi_room_policy::RoleIndex;
    use sqlx::{SqliteConnection, SqliteTransaction};

    use crate::{
        Chat, ChatId, SystemMessage, chats::messages::TimestampedMessage, clients::CoreUser,
        groups::Group, store::StoreNotifier,
    };

    pub(super) struct LeaveChatData<S> {
        chat: Chat,
        group: Group,
        state: S,
    }

    impl LeaveChatData<()> {
        pub(super) async fn load(
            txn: &mut SqliteTransaction<'_>,
            chat_id: ChatId,
        ) -> anyhow::Result<LeaveChatData<()>> {
            let chat = Chat::load(txn.as_mut(), &chat_id)
                .await?
                .with_context(|| format!("Can't find chat with id {chat_id}",))?;
            let group_id = chat.group_id();
            let group = Group::load_clean(txn, group_id)
                .await?
                .with_context(|| format!("Can't find group with id {group_id:?}"))?;
            Ok(Self {
                chat,
                group,
                state: (),
            })
        }

        pub(super) async fn stage_leave_group(
            self,
            sender_id: &UserId,
            connection: &mut SqliteConnection,
            signer: &ClientSigningKey,
        ) -> anyhow::Result<LeaveChatData<SelfRemoveParamsOut>> {
            let Self {
                chat,
                mut group,
                state: (),
            } = self;

            group.room_state_change_role(sender_id, sender_id, RoleIndex::Outsider)?;

            let params = group.stage_leave_group(connection, signer)?;

            Ok(LeaveChatData {
                chat,
                group,
                state: params,
            })
        }
    }

    impl LeaveChatData<SelfRemoveParamsOut> {
        pub(super) async fn ds_self_remove(
            self,
            api_clients: &crate::clients::api_clients::ApiClients,
            signer: &ClientSigningKey,
        ) -> anyhow::Result<DsSelfRemoved> {
            let Self {
                chat,
                group,
                state: params,
            } = self;

            let owner_domain = chat.owner_domain();

            let ts = api_clients
                .get(&owner_domain)?
                .ds_self_remove(params, signer, group.group_state_ear_key())
                .await?;

            Ok(DsSelfRemoved {
                group,
                ds_timestamp: ts,
            })
        }
    }

    pub(super) struct DsSelfRemoved {
        group: Group,
        ds_timestamp: TimeStamp,
    }

    impl DsSelfRemoved {
        pub(super) async fn store_update(
            self,
            txn: &mut SqliteTransaction<'_>,
            notifier: &mut StoreNotifier,
            chat_id: ChatId,
            user_id: &UserId,
        ) -> anyhow::Result<()> {
            let Self {
                group,
                ds_timestamp,
            } = self;

            group.store_update(&mut **txn).await?;
            CoreUser::store_new_messages(
                &mut *txn,
                notifier,
                chat_id,
                vec![TimestampedMessage::system_message(
                    SystemMessage::Remove(user_id.clone(), user_id.clone()),
                    ds_timestamp,
                )],
            )
            .await?;
            Ok(())
        }
    }
}
