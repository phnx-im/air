// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! What the two devices hand each other over the linking channel.

use aircommon::credentials::keys::UserSigningKey;
use aircommon::crypto::RatchetDecryptionKey;
use aircommon::crypto::aead::keys::{
    GroupStateEarKey, IdentityLinkWrapperKey, PushTokenEarKey, WelcomeAttributionInfoEarKey,
};
use aircommon::crypto::hpke::ClientIdEncryptionKey;
use aircommon::crypto::indexed_aead::keys::UserProfileKey;
use aircommon::crypto::kdf::keys::RatchetSecret;
use aircommon::crypto::signatures::keys::{QsClientSigningKey, QsUserSigningKey};
use aircommon::identifiers::{QsClientId, QsUserId, UserId};
use aircommon::messages::FriendshipToken;
use airprotos::client::self_group::{
    BlockedContactEntry, ConnectionRequestEntry, LinkedDevice, RedeemedTokens, SettingsUpdate,
    TokenSeed,
};
use apqmls::messages::ApqKeyPackage;
use openmls::group::GroupId;
use serde::{Deserialize, Serialize};

use crate::chats::outgoing_requests::OutgoingRequest;

/// Everything the old (existing) device hands to the new device over the
/// secure linking channel so the new device can bootstrap a working
/// [`CoreUser`] and join the user's self group.
///
/// [`CoreUser`]: crate::clients::CoreUser
#[derive(Serialize, Deserialize)]
pub(crate) struct ProvisioningPackage {
    /// Identity + AS user credential (shared across devices for the MVP).
    ///
    /// Contains the user id.
    pub(crate) user_signing_key: UserSigningKey,
    /// User-level QS key material (shared by all of the user's devices).
    pub(crate) qs_user_id: QsUserId,
    pub(crate) qs_user_signing_key: QsUserSigningKey,
    pub(crate) friendship_token: FriendshipToken,
    pub(crate) push_token_ear_key: PushTokenEarKey,
    pub(crate) wai_ear_key: WelcomeAttributionInfoEarKey,
    pub(crate) qs_client_id_encryption_key: ClientIdEncryptionKey,
    /// Freshly created queue for the new device (created by the old device).
    pub(crate) qs_client_id: QsClientId,
    pub(crate) qs_client_signing_key: QsClientSigningKey,
    pub(crate) qs_queue_decryption_key: RatchetDecryptionKey,
    pub(crate) qs_initial_ratchet_secret: RatchetSecret,
    /// User profile
    pub(crate) user_profile_key: UserProfileKey,
    /// Self-group metadata not carried by the Welcome.
    pub(crate) self_group_id: GroupId,
    /// Synced user settings snapshot so the new device starts with the
    /// provisioner's values.
    pub(crate) synced_settings: SettingsUpdate,
    /// The agreed Privacy Pass token seeds, so the new device derives the same
    /// token requests as its sibling instead of running an agreement round for a
    /// key whose allowance epoch the sibling has already locked.
    pub(crate) token_seeds: Vec<TokenSeed>,
    /// Contacts blocked so far.
    pub(crate) blocked_contacts: Vec<BlockedContactEntry>,
    /// The tokens the user's devices have redeemed so far.
    #[serde(default)]
    pub(crate) redeemed_tokens: Vec<RedeemedTokens>,
    /// The incoming connection requests that are pending.
    #[serde(default)]
    pub(crate) connection_requests: Vec<ConnectionRequestEntry>,
    /// The name the confirming user gave this device. Empty means "no choice
    /// made", and the new device falls back to its own platform label.
    pub(crate) device_name: String,
    /// The higher-level groups the virtual client is already a member of, which
    /// the new emulator client onboards itself into.
    pub(crate) groups: Vec<HigherLevelGroup>,
}

/// What the new device sends back over the secure linking channel.
///
/// The metadata entry travels with the key package so the old device can publish
/// it on the very same self-group commit that adds the new leaf. Sending it as a
/// settings commit of its own would advance the self-group epoch behind the back
/// of the device performing the add, breaking the next link with a wrong-epoch
/// rejection.
#[derive(Serialize, Deserialize)]
pub(crate) struct SelfGroupJoinRequest {
    pub(crate) key_package: ApqKeyPackage,
    pub(crate) device: LinkedDevice,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct HigherLevelGroup {
    pub(crate) group_id: GroupId,
    pub(crate) pq_group_id: Option<GroupId>,
    pub(crate) group_state_ear_key: GroupStateEarKey,
    pub(crate) identity_link_wrapper_key: IdentityLinkWrapperKey,
    pub(crate) vc_leaf_index: u32,
    /// Set if the group backs a connection chat rather than a group chat.
    pub(crate) connection: Option<ConnectionContact>,
    /// Set if the group backs an outgoing connection request the recipient
    /// has not answered yet.
    pub(crate) outgoing_request: Option<OutgoingRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ConnectionContact {
    pub(crate) user_id: UserId,
    pub(crate) wai_ear_key: WelcomeAttributionInfoEarKey,
    pub(crate) friendship_token: FriendshipToken,
}
