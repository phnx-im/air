// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The ephemeral MLS pairing group of the multi-device linking protocol.
//!
//! The group is created fresh per linking session, holds exactly two
//! members and lives only in memory. Its authenticity comes from the
//! external PSK that the CPace exchange produced, not from the basic
//! credentials, which are minted per session and carry no account
//! information.

use aircommon::crypto::mdl::pake::MdlPsk;
use airprotos::relay_service::mdl::{
    GroupMessage, LinkingPayload, LinkingPayloadType, MDL_RESPONDER_LABEL, MdlMessage,
};
use airprotos::relay_service::v1::RelayFrame;
use anyhow::{Context as _, anyhow, bail};
use openmls::{
    group::{MlsGroup, MlsGroupCreateConfig, MlsGroupJoinConfig, ProcessedWelcome, WelcomeError},
    messages::group_info::GroupInfoError,
    prelude::{
        BasicCredential, CredentialWithKey, GroupSecretsError, KeyPackage, MlsMessageBodyIn,
        MlsMessageIn, MlsMessageOut, PreSharedKeyProposal, ProcessedMessageContent, Proposal,
        ProtocolVersion,
    },
    schedule::{ExternalPsk, PreSharedKeyId, Psk, errors::PskError},
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;
use tls_codec::{Deserialize as _, DeserializeBytes as _, Serialize as _};
use tracing::warn;

use crate::clients::CIPHERSUITE;

use super::LinkingError;

/// Members the pairing group must have once it is up.
const PAIRING_GROUP_SIZE: usize = 2;

/// A per-session MLS identity: an in-memory provider, a fresh signature key
/// pair and a basic credential naming the role.
pub(super) struct PairingIdentity {
    provider: OpenMlsRustCrypto,
    signature_keys: SignatureKeyPair,
    credential_with_key: CredentialWithKey,
}

impl PairingIdentity {
    pub(super) fn new(identity: &str) -> anyhow::Result<Self> {
        let provider = OpenMlsRustCrypto::default();
        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())
            .context("generate the pairing signature key")?;
        signature_keys
            .store(provider.storage())
            .map_err(|error| anyhow!("store the pairing signature key: {error}"))?;
        let credential_with_key = CredentialWithKey {
            credential: BasicCredential::new(identity.as_bytes().to_vec()).into(),
            signature_key: signature_keys.to_public_vec().into(),
        };
        Ok(Self {
            provider,
            signature_keys,
            credential_with_key,
        })
    }

    /// A fresh KeyPackage for this identity, serialized as an `MLSMessage`.
    ///
    /// The private material stays in this identity's provider, so the same
    /// value must later process the Welcome.
    pub(super) fn key_package(&self) -> anyhow::Result<Vec<u8>> {
        let bundle = KeyPackage::builder()
            .build(
                CIPHERSUITE,
                &self.provider,
                &self.signature_keys,
                self.credential_with_key.clone(),
            )
            .context("build the pairing key package")?;
        MlsMessageOut::from(bundle)
            .to_bytes()
            .context("serialize the pairing key package")
    }
}

/// A live pairing group.
pub(super) struct PairingGroup {
    identity: PairingIdentity,
    group: MlsGroup,
}

impl PairingGroup {
    /// Creates the group on the existing device and adds the new device.
    ///
    /// The single commit carries the `Add` and the `PreSharedKey` proposal,
    /// so the Welcome the new device receives already has the PAKE-derived
    /// secret in its key schedule.
    pub(super) fn create(
        psk: &MdlPsk,
        key_package: KeyPackage,
    ) -> Result<(Self, Vec<u8>), LinkingError> {
        let identity = PairingIdentity::new(MDL_RESPONDER_LABEL).map_err(LinkingError::Protocol)?;
        let (group, welcome) =
            Self::create_inner(&identity, psk, key_package).map_err(LinkingError::Protocol)?;
        Ok((Self { identity, group }, welcome))
    }

    fn create_inner(
        identity: &PairingIdentity,
        psk: &MdlPsk,
        key_package: KeyPackage,
    ) -> anyhow::Result<(MlsGroup, Vec<u8>)> {
        let provider = &identity.provider;
        let psk_id = PreSharedKeyId::new(
            CIPHERSUITE,
            provider.rand(),
            Psk::External(ExternalPsk::new(psk.id().to_vec())),
        )
        .context("build the pairing psk id")?;
        // The copy the provider keeps lives exactly as long as this
        // per-session in-memory provider.
        psk_id
            .store(provider, psk.secret())
            .context("store the pairing psk")?;

        let group_config = MlsGroupCreateConfig::builder()
            .use_ratchet_tree_extension(true)
            .ciphersuite(CIPHERSUITE)
            .build();
        let mut group = MlsGroup::new(
            provider,
            &identity.signature_keys,
            &group_config,
            identity.credential_with_key.clone(),
        )
        .context("create the pairing group")?;

        let (_commit, welcome, _group_info) = group
            .commit_builder()
            .propose_adds([key_package])
            .add_proposal(Proposal::PreSharedKey(Box::new(PreSharedKeyProposal::new(
                psk_id,
            ))))
            .load_psks(provider.storage())?
            .build(
                provider.rand(),
                provider.crypto(),
                &identity.signature_keys,
                |_| true,
            )?
            .stage_commit(provider)?
            .into_messages();
        group
            .merge_pending_commit(provider)
            .context("merge the pairing commit")?;

        let welcome = welcome
            .context("the pairing commit produced no welcome")?
            .to_bytes()
            .context("serialize the pairing welcome")?;
        Ok((group, welcome))
    }

    /// Joins the group on the new device.
    ///
    /// This is the session's single authentication check: the key schedule
    /// includes the PAKE-derived PSK, so a wrong password shows up as a
    /// Welcome decryption failure and never as a mismatching PSK ID.
    pub(super) fn join(
        identity: PairingIdentity,
        psk: &MdlPsk,
        welcome: &[u8],
    ) -> Result<Self, LinkingError> {
        let provider = &identity.provider;

        let welcome = MlsMessageIn::tls_deserialize_exact(welcome)
            .context("deserialize the pairing welcome")
            .map_err(LinkingError::Protocol)?;
        let MlsMessageBodyIn::Welcome(welcome) = welcome.extract() else {
            return Err(LinkingError::Protocol(anyhow!(
                "expected a welcome in pake_share_b"
            )));
        };

        // OpenMLS resolves the referenced PSKs while it decrypts the group
        // secrets, so the secret has to be in storage first. Only the PSK ID
        // keys that storage, which is why the nonce here does not matter.
        // The copy the provider keeps lives exactly as long as this
        // per-session in-memory provider.
        PreSharedKeyId::external(psk.id().to_vec(), Vec::new())
            .store(provider, psk.secret())
            .context("store the pairing psk")
            .map_err(LinkingError::Protocol)?;

        let join_config = MlsGroupJoinConfig::builder()
            .use_ratchet_tree_extension(true)
            .build();
        let processed = ProcessedWelcome::new_from_welcome(provider, &join_config, welcome)
            .map_err(classify_welcome_error)?;

        // A welcome without our PSK would be an unauthenticated group, so it
        // has to be rejected even though it decrypted.
        let [psk_id] = processed.psks() else {
            return Err(LinkingError::validation(
                "the pairing welcome does not carry exactly one psk",
            ));
        };
        let Psk::External(external) = psk_id.psk() else {
            return Err(LinkingError::validation(
                "the pairing welcome's psk is not external",
            ));
        };
        if external.psk_id() != psk.id() {
            return Err(LinkingError::validation(
                "the pairing welcome references a different psk",
            ));
        }

        let group = processed
            .into_staged_welcome(provider, None)
            .context("stage the pairing welcome")
            .map_err(LinkingError::Protocol)?
            .into_group(provider)
            .context("join the pairing group")
            .map_err(LinkingError::Protocol)?;

        if group.members().count() != PAIRING_GROUP_SIZE {
            return Err(LinkingError::validation(
                "the pairing group does not have exactly two members",
            ));
        }
        if group.ciphersuite() != CIPHERSUITE {
            return Err(LinkingError::validation(
                "the pairing group uses an unexpected ciphersuite",
            ));
        }

        Ok(Self { identity, group })
    }

    /// Wraps `payload` into an application message and frames it.
    pub(super) fn send(
        &mut self,
        payload_type: LinkingPayloadType,
        payload: Vec<u8>,
    ) -> anyhow::Result<RelayFrame> {
        let plaintext = LinkingPayload {
            payload_type,
            payload,
        }
        .tls_serialize_detached()
        .context("serialize the linking payload")?;

        let provider = &self.identity.provider;
        let unconfirmed = self
            .group
            .create_unconfirmed_message(provider, &self.identity.signature_keys, &plaintext)
            .context("encrypt the linking payload")?;
        // There is no delivery service behind the pairing group, so the
        // message is confirmed as soon as it is built.
        self.group
            .confirm_application_message(
                provider.storage(),
                unconfirmed.epoch,
                unconfirmed.generation,
            )
            .context("confirm the linking payload")?;

        let mls_message = unconfirmed
            .message
            .to_bytes()
            .context("serialize the linking payload message")?;
        MdlMessage::GroupMessage(GroupMessage { mls_message })
            .into_frame()
            .context("frame the linking payload")
    }

    /// Decrypts an application message of the pairing group.
    pub(super) fn receive(&mut self, message: &GroupMessage) -> anyhow::Result<LinkingPayload> {
        let message = MlsMessageIn::tls_deserialize_exact(message.mls_message.as_slice())
            .context("deserialize the pairing group message")?;
        let protocol_message = message
            .try_into_protocol_message()
            .context("expected a pairing group protocol message")?;
        let processed = self
            .group
            .process_message(&self.identity.provider, protocol_message)
            .context("process the pairing group message")?;
        let ProcessedMessageContent::ApplicationMessage(application) = processed.into_content()
        else {
            bail!("expected an application message in the pairing group");
        };
        LinkingPayload::tls_deserialize_exact_bytes(&application.into_bytes())
            .context("deserialize the linking payload")
    }
}

/// Checks the new device's KeyPackage before anything is built on it.
pub(super) fn validate_key_package(bytes: &[u8]) -> Result<KeyPackage, LinkingError> {
    let message = MlsMessageIn::tls_deserialize_exact(bytes)
        .context("deserialize the pairing key package")
        .map_err(LinkingError::Protocol)?;
    let MlsMessageBodyIn::KeyPackage(key_package) = message.extract() else {
        return Err(LinkingError::Protocol(anyhow!(
            "expected a key package in pake_share_a"
        )));
    };
    // Verifies the MLS signature, the protocol version and the lifetime.
    let key_package = key_package
        .validate(
            &openmls_rust_crypto::RustCrypto::default(),
            ProtocolVersion::Mls10,
        )
        .map_err(|error| {
            LinkingError::validation(format!("the pairing key package is invalid: {error}"))
        })?;
    if key_package.ciphersuite() != CIPHERSUITE {
        return Err(LinkingError::validation(
            "the pairing key package uses an unexpected ciphersuite",
        ));
    }
    Ok(key_package)
}

/// Tells apart the ways a pairing Welcome can fail.
///
/// A wrong linking code shows up as a decryption or key-schedule failure,
/// because the group's key schedule mixes in the PAKE-derived secret. A
/// Welcome that names PSKs the two devices never agreed on is a validation
/// failure. Anything else is the peer or the relay not following the
/// protocol.
fn classify_welcome_error<StorageError: std::fmt::Display>(
    error: WelcomeError<StorageError>,
) -> LinkingError {
    match error {
        WelcomeError::GroupSecrets(GroupSecretsError::DecryptionFailed)
        | WelcomeError::GroupInfo(GroupInfoError::DecryptionFailed)
        | WelcomeError::JoinerSecretNotFound
        | WelcomeError::UnableToDecrypt
        | WelcomeError::KeySchedule(_) => {
            warn!("the pairing welcome did not open");
            LinkingError::AuthenticationFailed
        }
        // The PSK set is public, so a mismatch here is the peer naming keys
        // we never agreed on rather than evidence about the code.
        WelcomeError::Psk(
            error @ (PskError::KeyNotFound
            | PskError::TooManyKeys
            | PskError::TypeMismatch { .. }
            | PskError::UsageMismatch { .. }
            | PskError::UsageConflict { .. }
            | PskError::UsageDuplicate { .. }
            | PskError::NonceLengthMismatch { .. }),
        ) => LinkingError::validation(format!("the pairing welcome's psks are wrong: {error}")),
        // Anything left over is malformed input, an unsupported group, or our
        // own storage failing, none of which says anything about the code.
        other => LinkingError::Protocol(anyhow!("the pairing welcome failed: {other}")),
    }
}

/// A pairing group whose commit carries the `Add` but no `PreSharedKey`
/// proposal, which is what an unauthenticated relay could hand a joiner.
#[cfg(test)]
pub(super) fn welcome_without_psk(key_package: KeyPackage) -> anyhow::Result<Vec<u8>> {
    welcome_for_psk(key_package, None)
}

/// A pairing group whose commit names a PSK the joiner never agreed on.
#[cfg(test)]
pub(super) fn welcome_with_a_foreign_psk(key_package: KeyPackage) -> anyhow::Result<Vec<u8>> {
    let mut psk_id = [0u8; 32];
    rand::TryRng::try_fill_bytes(&mut rand::rng(), &mut psk_id);
    welcome_for_psk(key_package, Some(psk_id))
}

/// Builds a pairing group and its Welcome, optionally referencing
/// `psk_id` with a secret only the creator knows.
#[cfg(test)]
fn welcome_for_psk(key_package: KeyPackage, psk_id: Option<[u8; 32]>) -> anyhow::Result<Vec<u8>> {
    let identity = PairingIdentity::new(MDL_RESPONDER_LABEL)?;
    let provider = &identity.provider;
    let group_config = MlsGroupCreateConfig::builder()
        .use_ratchet_tree_extension(true)
        .ciphersuite(CIPHERSUITE)
        .build();
    let mut group = MlsGroup::new(
        provider,
        &identity.signature_keys,
        &group_config,
        identity.credential_with_key.clone(),
    )?;

    let mut builder = group.commit_builder().propose_adds([key_package]);
    if let Some(psk_id) = psk_id {
        let psk_id = PreSharedKeyId::new(
            CIPHERSUITE,
            provider.rand(),
            Psk::External(ExternalPsk::new(psk_id.to_vec())),
        )?;
        psk_id.store(provider, &[0x5a; 32])?;
        builder = builder.add_proposal(Proposal::PreSharedKey(Box::new(
            PreSharedKeyProposal::new(psk_id),
        )));
    }

    let (_commit, welcome, _group_info) = builder
        .load_psks(provider.storage())?
        .build(
            provider.rand(),
            provider.crypto(),
            &identity.signature_keys,
            |_| true,
        )?
        .stage_commit(provider)?
        .into_messages();
    group.merge_pending_commit(provider)?;
    Ok(welcome
        .context("the pairing commit produced no welcome")?
        .to_bytes()?)
}
