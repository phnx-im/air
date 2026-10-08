// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Outgoing connection requests.
//!
//! A sibling client learns about the request from the group bootstrap echo when
//! the request is sent, or from the provisioning package at linking time.
//!
//! Retracting a request deletes its connection group on the DS, which the DS
//! allows since the group has no other member yet, and then erases the chat.
//! The siblings share the sender's leaf, so the DS hands them the delete
//! commit, and they erase the chat when they process it. The deletion also
//! travels through the self-group, for a sibling that cannot process the
//! commit.

use std::collections::HashSet;

use aircommon::{
    credentials::UserCredential,
    crypto::{
        aead::{AeadDecryptable, keys::FriendshipPackageEarKey},
        indexed_aead::keys::UserProfileKey,
    },
    identifiers::{UserId, Username},
    messages::{
        client_as::{ConnectionOfferHash, EncryptedFriendshipPackage},
        client_ds::{AadMessage, AadPayload},
    },
    time::TimeStamp,
};
use anyhow::{Context, bail, ensure};
use openmls::{
    group::GroupId,
    prelude::{ProtocolMessage, Sender},
};
use serde::{Deserialize, Serialize};
use tls_codec::{DeserializeBytes, Serialize as _, VLBytes};

use crate::{
    Chat, ChatId, ChatMessage, ChatStatus, ChatType, SystemMessage,
    chats::connection_requests,
    clients::{CoreUser, connection_offer::FriendshipPackage},
    contacts::{PartialContact, PartialContactType, TargetedMessageContact, UsernameContact},
    db::access::{ReadDbTransaction, WriteDbTransaction},
    groups::{Group, client_auth_info::StorableUserCredential},
};

/// An outgoing connection request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum OutgoingRequest {
    /// Sent to a username.
    Username {
        username: Username,
        friendship_package_ear_key: FriendshipPackageEarKey,
        /// Id and value of the connection-offer PSK the recipient's join
        /// references.
        connection_offer_hash: ConnectionOfferHash,
    },
    /// Sent as a targeted message in a group chat.
    Targeted {
        user_id: UserId,
        friendship_package_ear_key: FriendshipPackageEarKey,
        /// The group of that group chat. Absent if the sender did not keep it.
        origin_group_id: Option<GroupId>,
    },
}

impl OutgoingRequest {
    /// The request a connection chat stands for, if it is an outgoing one.
    pub(crate) async fn load(
        txn: &mut ReadDbTransaction<'_>,
        chat: &Chat,
    ) -> sqlx::Result<Option<Self>> {
        let request = match chat.chat_type() {
            ChatType::HandleConnection(username) => UsernameContact::load(&mut *txn, username)
                .await?
                .filter(|contact| contact.chat_id == chat.id())
                .map(|contact| Self::Username {
                    username: contact.username,
                    friendship_package_ear_key: contact.friendship_package_ear_key,
                    connection_offer_hash: contact.connection_offer_hash,
                }),
            ChatType::TargetedMessageConnection(user_id) => {
                TargetedMessageContact::load(&mut *txn, user_id)
                    .await?
                    .filter(|contact| contact.chat_id == chat.id())
                    .map(|contact| Self::Targeted {
                        user_id: contact.user_id,
                        friendship_package_ear_key: contact.friendship_package_ear_key,
                        origin_group_id: contact.origin_group_id,
                    })
            }
            ChatType::Group(_) | ChatType::Connection(_) | ChatType::PendingConnection(_) => None,
        };
        Ok(request)
    }

    /// Stores the chat of the request, whose connection group is `group`,
    /// with the partial contact the recipient's join completes.
    pub(crate) async fn store(
        self,
        txn: &mut WriteDbTransaction<'_>,
        group: &Group,
        timestamp: TimeStamp,
    ) -> anyhow::Result<Chat> {
        let group_id = group.group_id().clone();
        let chat = match self {
            Self::Username {
                username,
                friendship_package_ear_key,
                connection_offer_hash,
            } => {
                let chat = Chat::new_handle_chat(group_id, username.clone());
                chat.store(&mut *txn).await?;
                ChatMessage::new_system_message(
                    chat.id(),
                    timestamp,
                    SystemMessage::NewHandleConnectionChat(username.clone()),
                )
                .store(&mut *txn)
                .await?;
                UsernameContact::new(
                    username,
                    chat.id(),
                    friendship_package_ear_key,
                    connection_offer_hash,
                )
                .upsert(&mut *txn)
                .await?;
                // The recipient's external commit will reference this PSK.
                group.store_connection_offer_psk(&mut *txn, connection_offer_hash)?;
                chat
            }
            Self::Targeted {
                user_id,
                friendship_package_ear_key,
                origin_group_id,
            } => {
                let system_message = match &origin_group_id {
                    Some(origin_group_id) => SystemMessage::SentGroupConnectionRequest {
                        recipient: user_id.clone(),
                        origin_chat_id: ChatId::try_from(origin_group_id)
                            .context("invalid origin group id of an outgoing request")?,
                    },
                    // An older sibling sent the request without its group chat.
                    None => SystemMessage::NewDirectConnectionChat(user_id.clone()),
                };
                let chat = Chat::new_targeted_message_chat(group_id, user_id.clone());
                chat.store(&mut *txn).await?;
                ChatMessage::new_system_message(chat.id(), timestamp, system_message)
                    .store(&mut *txn)
                    .await?;
                TargetedMessageContact::new(
                    user_id,
                    chat.id(),
                    friendship_package_ear_key,
                    origin_group_id,
                )
                .upsert(&mut *txn)
                .await?;
                chat
            }
        };
        Ok(chat)
    }
}

/// Confirms the outgoing request `chat` stands for, now that `recipient` joined
/// its connection group with `encrypted_friendship_package`.
///
/// Returns the system message that announces the connection.
pub(crate) async fn confirm(
    txn: &mut WriteDbTransaction<'_>,
    chat: &mut Chat,
    recipient: &UserCredential,
    encrypted_friendship_package: &EncryptedFriendshipPackage,
) -> anyhow::Result<SystemMessage> {
    let Some(contact_type) = chat.chat_type().unconfirmed_contact() else {
        bail!("Chat is not unconfirmed");
    };
    let recipient_id = recipient.user_id();
    if let PartialContactType::TargetedMessage(chat_user_id) = &contact_type {
        ensure!(
            recipient_id == chat_user_id,
            "Sender identity does not match targeted message user ID"
        );
    }

    let contact = PartialContact::load(&mut *txn, &contact_type)
        .await?
        .with_context(|| format!("No contact found: {contact_type:?}"))?;
    let friendship_package = FriendshipPackage::decrypt(
        contact.friendship_package_ear_key(),
        encrypted_friendship_package,
    )?;

    let user_profile_key = UserProfileKey::from_base_secret(
        friendship_package.user_profile_base_secret.clone(),
        recipient_id,
    )?;
    CoreUser::schedule_fetch_user_profile(&mut *txn, (recipient.clone(), user_profile_key)).await?;

    let contact = contact
        .mark_as_complete(&mut *txn, recipient_id.clone(), friendship_package)
        .await?;
    chat.confirm(&mut *txn, contact.user_id).await?;

    // Requests the new contact sent us meanwhile are redundant now.
    connection_requests::discard_requests_from(txn, recipient_id).await?;

    let user_handle = match contact_type {
        PartialContactType::Handle(handle) => Some(handle),
        PartialContactType::TargetedMessage(_) => None,
    };
    Ok(SystemMessage::ReceivedConnectionConfirmation {
        sender: recipient_id.clone(),
        user_handle,
    })
}

impl CoreUser {
    /// Retracts the outgoing contact request the chat shows, and erases the
    /// chat on all of the user's devices.
    ///
    /// Deleting the connection group on the DS means the recipient can no
    /// longer accept the request. The recipient is not told. Fails without a
    /// change if the DS cannot be reached, see
    /// [`CoreUser::delete_and_erase_chat`].
    pub async fn retract_contact_request(&self, chat_id: ChatId) -> anyhow::Result<()> {
        let chat = self
            .db()
            .with_read_transaction(async |txn| Chat::load(txn, &chat_id).await)
            .await?
            .with_context(|| format!("Can't find chat with id {chat_id}"))?;
        ensure!(
            chat.is_unconfirmed() && matches!(chat.status(), ChatStatus::Active),
            "Chat {chat_id} is not an open outgoing contact request"
        );
        self.delete_and_erase_chat(chat_id).await
    }
}

/// Confirms the outgoing request `chat` stands for with the friendship package
/// of a recipient's join this device could not process, since it onboarded
/// into `group` at a later epoch.
///
/// The recipient is the other user in `group`, whose credential the onboarding
/// verified.
pub(crate) async fn confirm_missed_join(
    txn: &mut WriteDbTransaction<'_>,
    chat: &mut Chat,
    group: &Group,
    encrypted_friendship_package: &EncryptedFriendshipPackage,
) -> anyhow::Result<SystemMessage> {
    let own_user_id = group.own_user_id();
    let others: HashSet<UserId> = group
        .members()
        .filter(|member| member != own_user_id)
        .collect();
    let mut others = others.into_iter();
    let (Some(recipient), None) = (others.next(), others.next()) else {
        bail!("connection group does not have exactly one other user");
    };
    let credential = StorableUserCredential::load_by_user_id(&mut *txn, &recipient)
        .await?
        .with_context(|| format!("no verified credential for {recipient:?}"))?;
    confirm(txn, chat, &credential.into(), encrypted_friendship_package).await
}

/// The encrypted friendship package of a recipient's join, read from `message`
/// without processing it.
///
/// For a join this device cannot process, since it onboards into the group at
/// a later epoch. Neither the signature nor the sender of the commit is
/// checked. Decrypting the package checks it instead, since only the recipient
/// and our own devices hold its key.
///
/// Only the self group uses Safe AAD framing (see `NewGroupContext`), so the
/// authenticated data of a connection group commit is the AAD message alone.
///
/// Returns `None` if `message` is no join of a connection group.
pub(crate) fn join_friendship_package(
    message: &ProtocolMessage,
) -> anyhow::Result<Option<EncryptedFriendshipPackage>> {
    let ProtocolMessage::PublicMessage(message) = message else {
        return Ok(None);
    };
    if !matches!(message.sender(), Sender::NewMemberCommit) {
        return Ok(None);
    }

    // openmls only exposes the authenticated data of a processed message. A
    // public message starts with the group id, the epoch, the sender and then
    // the authenticated data.
    let bytes = message.tls_serialize_detached()?;
    let (_group_id, rest) = VLBytes::tls_deserialize_bytes(&bytes)?;
    let (_epoch, rest) = u64::tls_deserialize_bytes(rest)?;
    let (_sender, rest) = Sender::tls_deserialize_bytes(rest)?;
    let (aad, _) = VLBytes::tls_deserialize_bytes(rest)?;

    match AadMessage::tls_deserialize_exact_bytes(aad.as_slice())?.into_payload() {
        AadPayload::JoinConnectionGroup(payload) => Ok(Some(payload.encrypted_friendship_package)),
        AadPayload::GroupOperation(_) | AadPayload::Resync | AadPayload::DeleteGroup => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use aircommon::{
        crypto::aead::keys::EncryptedUserProfileKey,
        messages::client_ds::JoinConnectionGroupParamsAad,
    };
    use openmls::prelude::{
        BasicCredential, CredentialWithKey, MlsGroup, MlsMessageBodyIn, MlsMessageIn, MlsMessageOut,
    };
    use openmls_basic_credential::SignatureKeyPair;
    use openmls_rust_crypto::OpenMlsRustCrypto;
    use openmls_traits::OpenMlsProvider as _;

    use crate::clients::CIPHERSUITE;

    use super::*;

    struct Member {
        provider: OpenMlsRustCrypto,
        signer: SignatureKeyPair,
        credential: CredentialWithKey,
    }

    fn member(identity: &[u8]) -> Member {
        let provider = OpenMlsRustCrypto::default();
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        signer.store(provider.storage()).unwrap();
        let credential = CredentialWithKey {
            credential: BasicCredential::new(identity.to_vec()).into(),
            signature_key: signer.to_public_vec().into(),
        };
        Member {
            provider,
            signer,
            credential,
        }
    }

    fn message_in(message: MlsMessageOut) -> MlsMessageIn {
        MlsMessageIn::tls_deserialize_exact_bytes(&message.tls_serialize_detached().unwrap())
            .unwrap()
    }

    fn create_group(alice: &Member) -> MlsGroup {
        MlsGroup::builder()
            .ciphersuite(CIPHERSUITE)
            .use_ratchet_tree_extension(true)
            .build(&alice.provider, &alice.signer, alice.credential.clone())
            .unwrap()
    }

    /// Bob's external commit into alice's group, with `aad` as authenticated
    /// data.
    fn external_commit(aad: Vec<u8>) -> ProtocolMessage {
        let alice = member(b"alice");
        let group = create_group(&alice);
        let group_info = group
            .export_group_info(alice.provider.crypto(), &alice.signer, true)
            .unwrap();
        let MlsMessageBodyIn::GroupInfo(group_info) = message_in(group_info).extract() else {
            panic!("expected a group info");
        };

        let bob = member(b"bob");
        let (_, bundle) = MlsGroup::external_commit_builder()
            .with_aad(aad)
            .build_group(&bob.provider, group_info, bob.credential.clone())
            .unwrap()
            .load_psks(bob.provider.storage())
            .unwrap()
            .build(
                bob.provider.rand(),
                bob.provider.crypto(),
                &bob.signer,
                |_| true,
            )
            .unwrap()
            .finalize(&bob.provider)
            .unwrap();
        message_in(bundle.into_commit())
            .try_into_protocol_message()
            .unwrap()
    }

    fn aad(payload: AadPayload) -> Vec<u8> {
        AadMessage::from(payload).tls_serialize_detached().unwrap()
    }

    #[test]
    fn reads_the_friendship_package_of_a_join() {
        let encrypted_friendship_package = EncryptedFriendshipPackage::random();
        let expected = encrypted_friendship_package.aead_ciphertext().clone();
        let message = external_commit(aad(AadPayload::JoinConnectionGroup(
            JoinConnectionGroupParamsAad {
                encrypted_friendship_package,
                encrypted_user_profile_key: EncryptedUserProfileKey::random(),
            },
        )));

        let read = join_friendship_package(&message)
            .unwrap()
            .expect("a join should carry a friendship package");
        assert_eq!(read.aead_ciphertext(), &expected);
    }

    #[test]
    fn a_resync_is_no_join() {
        let message = external_commit(aad(AadPayload::Resync));
        assert!(join_friendship_package(&message).unwrap().is_none());
    }

    #[test]
    fn an_external_commit_without_aad_message_fails() {
        let message = external_commit(b"no aad message".to_vec());
        assert!(join_friendship_package(&message).is_err());
    }

    #[test]
    fn an_application_message_is_no_join() {
        let alice = member(b"alice");
        let mut group = create_group(&alice);
        let message = group
            .create_unconfirmed_message(&alice.provider, &alice.signer, b"hello")
            .unwrap()
            .message;
        let message = message_in(message).try_into_protocol_message().unwrap();
        assert!(join_friendship_package(&message).unwrap().is_none());
    }
}
