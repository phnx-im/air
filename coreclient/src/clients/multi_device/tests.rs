// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::credentials::keys::UserSigningKey;
use aircommon::credentials::test_utils::create_test_credentials;
use aircommon::crypto::aead::keys::{
    IdentityLinkWrapperKey, PushTokenEarKey, WelcomeAttributionInfoEarKey,
};
use aircommon::crypto::hpke::{ClientIdDecryptionKey, ClientIdEncryptionKey};
use aircommon::crypto::mdl::pake::MdlPsk;
use aircommon::crypto::signatures::keys::QsUserSigningKey;
use aircommon::identifiers::{QsClientId, QsUserId, QualifiedGroupId, UserId};
use aircommon::messages::FriendshipToken;
use airprotos::auth_service::v1::OperationType;
use airprotos::client::self_group::{
    BlockedContactEntry, ContactBlocked, RedeemedTokens, TokenSeed,
};
use airprotos::relay_service::mdl::MDL_INITIATOR_LABEL;
use openmls::group::GroupId;
use uuid::Uuid;

use super::*;

const DOMAIN: &str = "example.com";

/// One side's view of a finished CPace exchange plus everything the pairing
/// group needs from it.
struct Handshake {
    new_device: PairingIdentity,
    key_package: Vec<u8>,
    new_psk: MdlPsk,
    existing_psk: MdlPsk,
}

/// Runs the exchange the way the two flows do, with real contexts. The two
/// codes are the same in the happy case and differ when a wrong code is
/// under test.
fn handshake(new_code: &LinkingCode, existing_code: &LinkingCode) -> Handshake {
    let ci_new = MdlContext::new(DOMAIN.to_owned(), new_code.rendezvous_id().to_owned())
        .tls_serialize_detached()
        .unwrap();
    let ci_existing = MdlContext::new(DOMAIN.to_owned(), existing_code.rendezvous_id().to_owned())
        .tls_serialize_detached()
        .unwrap();
    let sid = [7u8; SID_LEN];

    let new_device = PairingIdentity::new(MDL_INITIATOR_LABEL).unwrap();
    let key_package = new_device.key_package().unwrap();
    let initiator = MdlInitiator::start(new_code.password(), &ci_new, &sid, &key_package);
    let msg_a = initiator.msg_a().to_vec();

    let response = pake::respond(existing_code.password(), &ci_existing, &sid, &msg_a).unwrap();
    assert_eq!(
        response.key_package, key_package,
        "the key package must travel as the cpace associated data"
    );

    let kdf_ctx = |ci: &[u8], msg_b: &[u8]| {
        MdlKdfContext {
            ci: VLByteSlice(ci),
            sid: VLByteSlice(&sid),
            msg_a: VLByteSlice(&msg_a),
            msg_b: VLByteSlice(msg_b),
        }
        .tls_serialize_detached()
        .unwrap()
    };

    let existing_psk = response
        .isk
        .derive_psk(&kdf_ctx(&ci_existing, &response.msg_b));
    let new_psk = initiator
        .finish(&response.msg_b)
        .unwrap()
        .derive_psk(&kdf_ctx(&ci_new, &response.msg_b));

    Handshake {
        new_device,
        key_package,
        new_psk,
        existing_psk,
    }
}

/// Builds a [`ProvisioningPackage`] with the given synced settings, token
/// seeds, blocked contacts and redeemed tokens, and otherwise freshly
/// generated key material.
fn sample_package(
    synced_settings: SettingsUpdate,
    token_seeds: Vec<TokenSeed>,
    blocked_contacts: Vec<BlockedContactEntry>,
    redeemed_tokens: Vec<RedeemedTokens>,
) -> anyhow::Result<ProvisioningPackage> {
    let user_id = UserId::random(DOMAIN.parse()?);
    let (_as_key, user_signing_key) = create_test_credentials(user_id.clone());
    let self_group_id = GroupId::from(QualifiedGroupId::new(Uuid::new_v4(), DOMAIN.parse()?));
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

fn sample_blocked_contacts() -> anyhow::Result<Vec<BlockedContactEntry>> {
    Ok(vec![BlockedContactEntry::Blocked(ContactBlocked {
        user_id: UserId::random(DOMAIN.parse()?).into(),
        blocked_at: 1_767_225_600,
        last_display_name: "Alice".to_owned(),
    })])
}

/// Unwraps a frame the pairing group produced back into its group message.
fn group_message(frame: RelayFrame) -> airprotos::relay_service::mdl::GroupMessage {
    match MdlMessage::from_frame(&frame).unwrap() {
        MdlMessage::GroupMessage(message) => message,
        other => panic!("expected a group message, got {}", other.kind()),
    }
}

#[test]
fn the_context_wiring_produces_one_psk() -> anyhow::Result<()> {
    let code = LinkingCode::generate("417")?;
    let handshake = handshake(&code, &code);
    assert_eq!(handshake.new_psk.id(), handshake.existing_psk.id());
    assert_eq!(handshake.new_psk.secret(), handshake.existing_psk.secret());
    Ok(())
}

#[test]
fn a_full_session_carries_payloads_both_ways() -> anyhow::Result<()> {
    let code = LinkingCode::generate("417")?;
    let handshake = handshake(&code, &code);

    let key_package = pairing::validate_key_package(&handshake.key_package)?;
    let (mut existing, welcome) = PairingGroup::create(&handshake.existing_psk, key_package)?;
    let mut new = PairingGroup::join(handshake.new_device, &handshake.new_psk, &welcome)?;

    let blocked_contacts = sample_blocked_contacts()?;
    let package = sample_package(
        SettingsUpdate {
            send_read_receipts: Some(false),
            linked_devices: None,
        },
        sample_seeds(),
        blocked_contacts.clone(),
        sample_redeemed(),
    )?;
    let user_id = package.user_signing_key.credential().user_id().clone();

    let frame = existing.send(
        LinkingPayloadType::ProvisioningPackage,
        PersistenceCodec::to_vec(&package)?,
    )?;
    let payload = new.receive(&group_message(frame))?;
    assert_eq!(
        payload.payload_type,
        LinkingPayloadType::ProvisioningPackage
    );
    let decoded: ProvisioningPackage = PersistenceCodec::from_slice(&payload.payload)?;
    assert_eq!(decoded.user_signing_key.credential().user_id(), &user_id);
    assert_eq!(decoded.token_seeds, sample_seeds());
    assert_eq!(decoded.blocked_contacts, blocked_contacts);
    assert_eq!(decoded.redeemed_tokens, sample_redeemed());
    // The confirming user's device name rides along in the same package.
    assert_eq!(decoded.device_name, "Work laptop");
    assert_eq!(
        decoded.synced_settings,
        SettingsUpdate {
            send_read_receipts: Some(false),
            linked_devices: None,
        }
    );

    let frame = new.send(LinkingPayloadType::LinkingComplete, Vec::new())?;
    let payload = existing.receive(&group_message(frame))?;
    assert_eq!(payload.payload_type, LinkingPayloadType::LinkingComplete);
    assert!(payload.payload.is_empty());

    Ok(())
}

#[test]
fn a_device_limit_abort_carries_the_limit() -> anyhow::Result<()> {
    let code = LinkingCode::generate("417")?;
    let handshake = handshake(&code, &code);

    let key_package = pairing::validate_key_package(&handshake.key_package)?;
    let (mut existing, welcome) = PairingGroup::create(&handshake.existing_psk, key_package)?;
    let mut new = PairingGroup::join(handshake.new_device, &handshake.new_psk, &welcome)?;

    let frame = existing.send(
        LinkingPayloadType::LinkingAbort,
        PersistenceCodec::to_vec(&LinkingAbort::DeviceLimitReached { max_devices: 2 })?,
    )?;
    let payload = new.receive(&group_message(frame))?;
    assert_eq!(payload.payload_type, LinkingPayloadType::LinkingAbort);
    let decoded: LinkingAbort = PersistenceCodec::from_slice(&payload.payload)?;
    assert!(matches!(
        decoded,
        LinkingAbort::DeviceLimitReached { max_devices: 2 }
    ));
    Ok(())
}

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

    let decoded: ProvisioningPackage =
        PersistenceCodec::from_slice(&PersistenceCodec::to_vec(&older)?)?;

    assert_eq!(decoded.token_seeds, sample_seeds());
    assert!(decoded.redeemed_tokens.is_empty());

    Ok(())
}

#[test]
fn a_wrong_code_fails_the_welcome() -> anyhow::Result<()> {
    let new_code = LinkingCode::generate("417")?;
    let mut digits = new_code.to_digits().into_bytes();
    let last = digits.len() - 1;
    digits[last] = if digits[last] == b'0' { b'1' } else { b'0' };
    let mistyped = LinkingCode::parse(&String::from_utf8(digits)?)?;
    let handshake = handshake(&new_code, &mistyped);

    // The PSK ID comes from public values, so it matches even here. Only the
    // secret differs, which is exactly why the welcome is the check.
    assert_eq!(handshake.new_psk.id(), handshake.existing_psk.id());
    assert_ne!(handshake.new_psk.secret(), handshake.existing_psk.secret());

    let key_package = pairing::validate_key_package(&handshake.key_package)?;
    let (_existing, welcome) = PairingGroup::create(&handshake.existing_psk, key_package)?;
    let Err(error) = PairingGroup::join(handshake.new_device, &handshake.new_psk, &welcome) else {
        panic!("a wrong code must not open the welcome");
    };
    assert!(
        matches!(error, LinkingError::AuthenticationFailed),
        "expected an authentication failure, got {error}"
    );
    Ok(())
}

#[test]
fn a_welcome_without_the_psk_is_rejected() -> anyhow::Result<()> {
    let code = LinkingCode::generate("417")?;
    let handshake = handshake(&code, &code);

    let key_package = pairing::validate_key_package(&handshake.key_package)?;
    let welcome = pairing::welcome_without_psk(key_package)?;

    let Err(error) = PairingGroup::join(handshake.new_device, &handshake.new_psk, &welcome) else {
        panic!("a welcome without the psk must be rejected");
    };
    assert!(
        matches!(error, LinkingError::Validation(_)),
        "expected a validation failure, got {error}"
    );
    Ok(())
}

#[test]
fn a_welcome_for_another_psk_is_a_validation_failure() -> anyhow::Result<()> {
    let code = LinkingCode::generate("417")?;
    let handshake = handshake(&code, &code);

    let key_package = pairing::validate_key_package(&handshake.key_package)?;
    let welcome = pairing::welcome_with_a_foreign_psk(key_package)?;

    let Err(error) = PairingGroup::join(handshake.new_device, &handshake.new_psk, &welcome) else {
        panic!("a welcome for another psk must be rejected");
    };
    assert!(
        matches!(error, LinkingError::Validation(_)),
        "expected a validation failure, got {error}"
    );
    Ok(())
}

#[test]
fn another_version_is_aborted_as_such() {
    check_version(MDL_PROTOCOL_VERSION).unwrap();
    let error = check_version(MDL_PROTOCOL_VERSION + 1).unwrap_err();
    assert!(matches!(error, LinkingError::UnsupportedVersion(v) if v == MDL_PROTOCOL_VERSION + 1));
    assert_eq!(error.abort_code(), Some(AbortCode::UnsupportedVersion));
}

#[test]
fn garbage_in_place_of_a_key_package_is_rejected() -> anyhow::Result<()> {
    let error =
        pairing::validate_key_package(b"not a key package").expect_err("garbage must not validate");
    assert!(matches!(error, LinkingError::Protocol(_)));
    Ok(())
}
