// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::assert_matches;

use aircommon::credentials::keys::UserSigningKey;
use aircommon::credentials::test_utils::create_test_credentials;
use aircommon::crypto::aead::keys::{
    FriendshipPackageEarKey, GroupStateEarKey, IdentityLinkWrapperKey, PushTokenEarKey,
    WelcomeAttributionInfoEarKey,
};
use aircommon::crypto::hpke::{ClientIdDecryptionKey, ClientIdEncryptionKey};
use aircommon::crypto::signatures::keys::QsUserSigningKey;
use aircommon::identifiers::{QsClientId, QsUserId, QualifiedGroupId, UserId, Username};
use aircommon::messages::FriendshipToken;
use aircommon::messages::client_as::ConnectionOfferHash;
use airprotos::auth_service::v1::OperationType;
use airprotos::client::self_group::{
    BlockedContactEntry, ConnectionRequestEntry, ConnectionRequestGroup, ConnectionRequestReceived,
    ConnectionRequestSource, ContactBlocked, RedeemedTokens, TokenSeed,
};
use openmls::group::GroupId;
use uuid::Uuid;

use super::*;

/// Builds a [`ProvisioningPackage`] with the given synced settings, token
/// seeds, blocked contacts and redeemed tokens, and otherwise freshly
/// generated key material.
fn sample_package(
    synced_settings: SettingsUpdate,
    token_seeds: Vec<TokenSeed>,
    blocked_contacts: Vec<BlockedContactEntry>,
    redeemed_tokens: Vec<RedeemedTokens>,
) -> anyhow::Result<ProvisioningPackage> {
    let user_id = UserId::random("example.com".parse()?);
    let (_as_key, user_signing_key) = create_test_credentials(user_id.clone());
    let self_group_id = GroupId::from(QualifiedGroupId::new(
        Uuid::new_v4(),
        "example.com".parse()?,
    ));
    Ok(ProvisioningPackage {
        user_signing_key,
        qs_user_id: QsUserId::random(),
        qs_user_signing_key: QsUserSigningKey::generate()?,
        friendship_token: FriendshipToken::random()?,
        push_token_ear_key: PushTokenEarKey::random()?,
        wai_ear_key: WelcomeAttributionInfoEarKey::random()?,
        qs_client_id_encryption_key: ClientIdDecryptionKey::generate()?.encryption_key().clone(),
        qs_client_id: QsClientId::random(&mut rand::rng()),
        qs_client_signing_key: QsClientSigningKey::generate()?,
        qs_queue_decryption_key: RatchetDecryptionKey::generate()?,
        qs_initial_ratchet_secret: RatchetSecret::random()?,
        user_profile_key: UserProfileKey::random(&user_id)?,
        self_group_id,
        synced_settings,
        token_seeds,
        blocked_contacts,
        redeemed_tokens,
        connection_requests: Vec::new(),
        device_name: "Work laptop".to_owned(),
        groups: Vec::new(),
    })
}

fn sample_seeds() -> Vec<TokenSeed> {
    vec![TokenSeed {
        operation_type: OperationType::AddUsername,
        key_fingerprint: [0x11; 32],
        seed: [0x22; 32],
    }]
}

fn sample_redeemed() -> Vec<RedeemedTokens> {
    vec![RedeemedTokens {
        operation_type: OperationType::AddUsername,
        key_fingerprint: [0x11; 32],
        allowance_epoch: 679,
        token_indices: vec![0, 4],
    }]
}

#[test]
fn linking_abort_roundtrips_through_linking_channel() -> anyhow::Result<()> {
    let key = MultiDeviceLinkingKey::random()?;
    let frame = LinkingMessage::seal(&LinkingAbort::DeviceLimitReached { max_devices: 2 }, &key)?;
    let decoded: LinkingAbort = LinkingMessage::open(frame.as_slice(), &key)?;
    assert_matches!(decoded, LinkingAbort::DeviceLimitReached { max_devices: 2 });
    Ok(())
}

#[test]
fn synced_state_roundtrips_through_linking_channel() -> anyhow::Result<()> {
    let blocked_contacts = vec![BlockedContactEntry::Blocked(ContactBlocked {
        user_id: UserId::random("example.com".parse()?).into(),
        blocked_at: 1_767_225_600,
        last_display_name: "Alice".to_owned(),
    })];
    let mut package = sample_package(
        SettingsUpdate {
            send_read_receipts: Some(false),
            linked_devices: None,
        },
        sample_seeds(),
        blocked_contacts.clone(),
        sample_redeemed(),
    )?;
    let connection_requests = vec![ConnectionRequestEntry::Received(
        ConnectionRequestReceived {
            connection_info: vec![0x11; 8],
            sender_credential: vec![0x12; 8],
            source: ConnectionRequestSource::Group(ConnectionRequestGroup {
                group_id: Some(GroupId::from_slice(&[0x13; 8])),
            }),
            connection_offer_hash: None,
            connection_package_hash: None,
            received_at: 1_767_225_600_123,
        },
    )];
    package.connection_requests = connection_requests.clone();
    let outgoing_request = OutgoingRequest::Username {
        username: Username::new("joel-03".to_owned())?,
        friendship_package_ear_key: FriendshipPackageEarKey::random()?,
        connection_offer_hash: ConnectionOfferHash::new_for_test(vec![0x66; 32]),
    };
    package.groups.push(HigherLevelGroup {
        group_id: GroupId::from_slice(&[0x14; 8]),
        pq_group_id: None,
        group_state_ear_key: GroupStateEarKey::random()?,
        identity_link_wrapper_key: IdentityLinkWrapperKey::random()?,
        vc_leaf_index: 0,
        connection: None,
        outgoing_request: Some(outgoing_request.clone()),
    });
    let user_id = package.user_signing_key.credential().user_id().clone();

    let key = MultiDeviceLinkingKey::random()?;
    let frame = LinkingMessage::seal(&package, &key)?;
    let decoded: ProvisioningPackage = LinkingMessage::open(frame.as_slice(), &key)?;

    assert_eq!(
        decoded.synced_settings,
        SettingsUpdate {
            send_read_receipts: Some(false),
            linked_devices: None,
        }
    );
    assert_eq!(decoded.token_seeds, sample_seeds());
    assert_eq!(decoded.blocked_contacts, blocked_contacts);
    assert_eq!(decoded.redeemed_tokens, sample_redeemed());
    assert_eq!(decoded.connection_requests, connection_requests);
    assert_eq!(
        decoded.groups[0].outgoing_request,
        Some(outgoing_request),
        "an unanswered outgoing request travels with its group"
    );
    assert_eq!(decoded.user_signing_key.credential().user_id(), &user_id);
    // The confirming user's device name rides along in the same package.
    assert_eq!(decoded.device_name, "Work laptop");

    Ok(())
}

/// A provisioner from before redeemed-token and connection-request sync
/// sends no `redeemed_tokens` or `connection_requests` key. Linking to it
/// has to work, with both empty.
#[test]
fn a_package_without_redeemed_tokens_decodes_as_empty() -> anyhow::Result<()> {
    /// The package as an older provisioner serializes it.
    #[derive(serde::Serialize)]
    struct OlderProvisioningPackage {
        user_id: UserId,
        user_signing_key: UserSigningKey,
        qs_user_id: QsUserId,
        qs_user_signing_key: QsUserSigningKey,
        friendship_token: FriendshipToken,
        push_token_ear_key: PushTokenEarKey,
        wai_ear_key: WelcomeAttributionInfoEarKey,
        qs_client_id_encryption_key: ClientIdEncryptionKey,
        qs_client_id: QsClientId,
        qs_client_signing_key: QsClientSigningKey,
        qs_queue_decryption_key: RatchetDecryptionKey,
        qs_initial_ratchet_secret: RatchetSecret,
        user_profile_key: UserProfileKey,
        self_group_id: GroupId,
        identity_link_wrapper_key: IdentityLinkWrapperKey,
        synced_settings: SettingsUpdate,
        token_seeds: Vec<TokenSeed>,
        blocked_contacts: Vec<BlockedContactEntry>,
        device_name: String,
        groups: Vec<HigherLevelGroup>,
    }

    let ProvisioningPackage {
        user_signing_key,
        qs_user_id,
        qs_user_signing_key,
        friendship_token,
        push_token_ear_key,
        wai_ear_key,
        qs_client_id_encryption_key,
        qs_client_id,
        qs_client_signing_key,
        qs_queue_decryption_key,
        qs_initial_ratchet_secret,
        user_profile_key,
        self_group_id,
        synced_settings,
        token_seeds,
        blocked_contacts,
        redeemed_tokens: _,
        connection_requests: _,
        device_name,
        groups,
    } = sample_package(
        SettingsUpdate::default(),
        sample_seeds(),
        Vec::new(),
        sample_redeemed(),
    )?;
    let older = OlderProvisioningPackage {
        user_id: user_signing_key.credential().user_id().clone(),
        user_signing_key,
        qs_user_id,
        qs_user_signing_key,
        friendship_token,
        push_token_ear_key,
        wai_ear_key,
        qs_client_id_encryption_key,
        qs_client_id,
        qs_client_signing_key,
        qs_queue_decryption_key,
        qs_initial_ratchet_secret,
        user_profile_key,
        self_group_id,
        identity_link_wrapper_key: IdentityLinkWrapperKey::random()?,
        synced_settings,
        token_seeds,
        blocked_contacts,
        device_name,
        groups,
    };

    let key = MultiDeviceLinkingKey::random()?;
    let frame = LinkingMessage::seal(&older, &key)?;
    let decoded: ProvisioningPackage = LinkingMessage::open(frame.as_slice(), &key)?;

    assert_eq!(decoded.token_seeds, sample_seeds());
    assert!(decoded.redeemed_tokens.is_empty());
    assert!(decoded.connection_requests.is_empty());

    Ok(())
}

/// A provisioner from before outgoing requests were handed over sends its
/// groups without an `outgoing_request` key.
#[test]
fn a_group_without_outgoing_request_decodes_as_none() -> anyhow::Result<()> {
    /// The group as an older provisioner serializes it.
    #[derive(serde::Serialize)]
    struct OlderHigherLevelGroup {
        group_id: GroupId,
        pq_group_id: Option<GroupId>,
        group_state_ear_key: GroupStateEarKey,
        identity_link_wrapper_key: IdentityLinkWrapperKey,
        vc_leaf_index: u32,
        connection: Option<ConnectionContact>,
    }

    let older = OlderHigherLevelGroup {
        group_id: GroupId::from_slice(&[0x14; 8]),
        pq_group_id: None,
        group_state_ear_key: GroupStateEarKey::random()?,
        identity_link_wrapper_key: IdentityLinkWrapperKey::random()?,
        vc_leaf_index: 0,
        connection: None,
    };

    let bytes = PersistenceCodec::to_vec(&older)?;
    let decoded: HigherLevelGroup = PersistenceCodec::from_slice(&bytes)?;
    assert_eq!(decoded.group_id, older.group_id);
    assert_eq!(decoded.outgoing_request, None);

    Ok(())
}
