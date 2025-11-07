// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Configuration for MLS groups.

use mls_assist::openmls::{
    group::{MlsGroupJoinConfig, PURE_PLAINTEXT_WIRE_FORMAT_POLICY},
    prelude::{
        Capabilities, Ciphersuite, CredentialType, ExtensionType, ProposalType, ProtocolVersion,
        RequiredCapabilitiesExtension, SenderRatchetConfiguration,
    },
};

/// Dictates for how many past epochs we want to keep around message secrets.
pub const MAX_PAST_EPOCHS: usize = 5;

/// Determines the out-of-order tolerance for the sender ratchet. See
/// [`SenderRatchetConfiguration`].
pub const OUT_OF_ORDER_TOLERANCE: u32 = 20;
/// Determines the maximum forward distance for the sender ratchet. See
/// [`SenderRatchetConfiguration`].
pub const MAXIMUM_FORWARD_DISTANCE: u32 = 1000;

pub fn default_sender_ratchet_configuration() -> SenderRatchetConfiguration {
    SenderRatchetConfiguration::new(OUT_OF_ORDER_TOLERANCE, MAXIMUM_FORWARD_DISTANCE)
}

pub fn default_mls_group_join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .max_past_epochs(MAX_PAST_EPOCHS)
        .sender_ratchet_configuration(default_sender_ratchet_configuration())
        .wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
        .build()
}

/// Proposal type of the friendship package proposal.
pub const FRIENDSHIP_PACKAGE_PROPOSAL_TYPE: u16 = 0xff00;
pub const GROUP_DATA_EXTENSION_TYPE: u16 = 0xff01;
pub const QS_CLIENT_REFERENCE_EXTENSION_TYPE: u16 = 0xff00;

pub const DEFAULT_MLS_VERSION: ProtocolVersion = ProtocolVersion::Mls10;
pub const DEFAULT_CIPHERSUITE: Ciphersuite =
    Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

pub const REQUIRED_EXTENSION_TYPES: [ExtensionType; 3] = [
    ExtensionType::Unknown(QS_CLIENT_REFERENCE_EXTENSION_TYPE),
    ExtensionType::Unknown(GROUP_DATA_EXTENSION_TYPE),
    ExtensionType::LastResort,
];
pub const REQUIRED_PROPOSAL_TYPES: [ProposalType; 1] =
    [ProposalType::Custom(FRIENDSHIP_PACKAGE_PROPOSAL_TYPE)];
pub const REQUIRED_CREDENTIAL_TYPES: [CredentialType; 1] = [CredentialType::Basic];

pub fn default_required_capabilities() -> RequiredCapabilitiesExtension {
    RequiredCapabilitiesExtension::new(
        &REQUIRED_EXTENSION_TYPES,
        &REQUIRED_PROPOSAL_TYPES,
        &REQUIRED_CREDENTIAL_TYPES,
    )
}

// Default capabilities for every leaf node we create.
pub const SUPPORTED_PROTOCOL_VERSIONS: [ProtocolVersion; 1] = [DEFAULT_MLS_VERSION];
pub const SUPPORTED_CIPHERSUITES: [Ciphersuite; 1] = [DEFAULT_CIPHERSUITE];
pub const SUPPORTED_EXTENSIONS: [ExtensionType; 3] = REQUIRED_EXTENSION_TYPES;
pub const SUPPORTED_PROPOSALS: [ProposalType; 1] = REQUIRED_PROPOSAL_TYPES;
pub const SUPPORTED_CREDENTIALS: [CredentialType; 1] = REQUIRED_CREDENTIAL_TYPES;

pub fn default_capabilities() -> Capabilities {
    Capabilities::new(
        Some(&SUPPORTED_PROTOCOL_VERSIONS),
        Some(&SUPPORTED_CIPHERSUITES),
        Some(&SUPPORTED_EXTENSIONS),
        Some(&SUPPORTED_PROPOSALS),
        Some(&SUPPORTED_CREDENTIALS),
    )
}
