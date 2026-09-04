// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::credentials::test_utils::create_test_credentials;
use aircommon::crypto::aead::keys::{
    IdentityLinkWrapperKey, PushTokenEarKey, WelcomeAttributionInfoEarKey,
};
use aircommon::crypto::hpke::ClientIdDecryptionKey;
use aircommon::crypto::mdl::pake::MdlPsk;
use aircommon::identifiers::{QsClientId, QsUserId, QualifiedGroupId, UserId};
use aircommon::messages::FriendshipToken;
use airprotos::client::self_group::TokenSeed;
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

/// Builds a [`ProvisioningPackage`] with the given synced-settings snapshot
/// and token seeds, and otherwise freshly generated key material.
fn sample_package(
    synced_settings: SettingsUpdate,
    token_seeds: Vec<TokenSeed>,
) -> anyhow::Result<ProvisioningPackage> {
    let user_id = UserId::random(DOMAIN.parse()?);
    let (_as_key, user_signing_key) = create_test_credentials(user_id.clone());
    let self_group_id = GroupId::from(QualifiedGroupId::new(Uuid::new_v4(), DOMAIN.parse()?));
    Ok(ProvisioningPackage {
        user_signing_key,
        qs_user_id: QsUserId::random(),
        qs_user_signing_key: aircommon::crypto::signatures::keys::QsUserSigningKey::generate()?,
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
        identity_link_wrapper_key: IdentityLinkWrapperKey::random()?,
        synced_settings,
        token_seeds,
        device_name: "Work laptop".to_owned(),
        groups: Vec::new(),
        user_id,
    })
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

    let seeds = vec![TokenSeed {
        operation_type: 1,
        key_fingerprint: [0x11; 32],
        seed: [0x22; 32],
    }];
    let package = sample_package(
        SettingsUpdate {
            send_read_receipts: Some(false),
            linked_devices: None,
        },
        seeds.clone(),
    )?;
    let user_id = package.user_id.clone();

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
    assert_eq!(decoded.user_id, user_id);
    assert_eq!(decoded.token_seeds, seeds);
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
fn a_wrong_code_fails_the_welcome() -> anyhow::Result<()> {
    let new_code = LinkingCode::generate("417")?;
    let mistyped = LinkingCode::generate("417")?;
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
