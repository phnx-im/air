// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Configuration for MLS groups.

use apqmls::ApqCiphersuite;
use mls_assist::openmls::{
    group::{MlsGroupJoinConfig, PURE_PLAINTEXT_WIRE_FORMAT_POLICY},
    prelude::{
        Capabilities, Ciphersuite, CredentialType, ExtensionType, ProposalType, ProtocolVersion,
        RequiredCapabilitiesExtension, SenderRatchetConfiguration,
    },
};

use crate::credentials::SELF_GROUP_CREDENTIAL_TYPE;

/// Dictates for how many past epochs we want to keep around message secrets.
pub const MAX_PAST_EPOCHS: usize = 5;

/// Determines the out-of-order tolerance for the sender ratchet. See
/// [`SenderRatchetConfiguration`].
const OUT_OF_ORDER_TOLERANCE: u32 = 20;
/// Determines the maximum forward distance for the sender ratchet. See
/// [`SenderRatchetConfiguration`].
const MAXIMUM_FORWARD_DISTANCE: u32 = 1000;

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

const DEFAULT_MLS_VERSION: ProtocolVersion = ProtocolVersion::Mls10;
const DEFAULT_CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

const PQ_CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_MLKEM768_AES256GCM_SHA384_Ed25519;

pub const APQ_CIPHERSUITE: ApqCiphersuite =
    ApqCiphersuite::new(DEFAULT_CIPHERSUITE, PQ_CIPHERSUITE);

// Required capabilities
const REQUIRED_EXTENSIONS: &[ExtensionType] = &[
    ExtensionType::Unknown(QS_CLIENT_REFERENCE_EXTENSION_TYPE),
    ExtensionType::Unknown(GROUP_DATA_EXTENSION_TYPE),
    ExtensionType::LastResort,
];
const REQUIRED_PROPOSALS: &[ProposalType] = &[
    ProposalType::Custom(FRIENDSHIP_PACKAGE_PROPOSAL_TYPE),
    ProposalType::SelfRemove,
];
const REQUIRED_CREDENTIALS: &[CredentialType] = &[CredentialType::Basic];

pub fn default_group_required_extensions() -> RequiredCapabilitiesExtension {
    RequiredCapabilitiesExtension::new(
        REQUIRED_EXTENSIONS,
        REQUIRED_PROPOSALS,
        REQUIRED_CREDENTIALS,
    )
}

// Supported capabilities (subset of required capabilities)
pub const SUPPORTED_PROTOCOL_VERSIONS: &[ProtocolVersion] = &[DEFAULT_MLS_VERSION];
pub const SUPPORTED_CIPHERSUITES: &[Ciphersuite] = &[DEFAULT_CIPHERSUITE];
pub const SUPPORTED_EXTENSIONS: &[ExtensionType] = &[
    ExtensionType::Unknown(QS_CLIENT_REFERENCE_EXTENSION_TYPE), // Also in REQUIRED_EXTENSIONS
    ExtensionType::Unknown(GROUP_DATA_EXTENSION_TYPE),          // Also in REQUIRED_EXTENSIONS
    ExtensionType::LastResort,                                  // Also in REQUIRED_EXTENSIONS
    ExtensionType::AppDataDictionary,
];
pub const SUPPORTED_PROPOSALS: &[ProposalType] = &[
    ProposalType::Custom(FRIENDSHIP_PACKAGE_PROPOSAL_TYPE), // Also in REQUIRED_PROPOSALS
    ProposalType::SelfRemove,                               // Also in REQUIRED_PROPOSALS
    ProposalType::AppDataUpdate,
    ProposalType::AppEphemeral,
];
pub const SUPPORTED_CREDENTIALS: &[CredentialType] = REQUIRED_CREDENTIALS;

/// Credential types advertised by self-group leaves. Self-group leaves carry a
/// self-group credential, and every member of a group must list the credential types of all
/// leaves in its capabilities. Safe because self-groups never contain foreign members.
pub const SELF_GROUP_SUPPORTED_CREDENTIALS: &[CredentialType] = &[
    CredentialType::Basic,
    CredentialType::Other(SELF_GROUP_CREDENTIAL_TYPE),
];

/// Capabilities that are used in the leaf node.
pub fn default_leaf_node_capabilities() -> Capabilities {
    Capabilities::new(
        Some(SUPPORTED_PROTOCOL_VERSIONS),
        Some(SUPPORTED_CIPHERSUITES),
        Some(SUPPORTED_EXTENSIONS),
        Some(SUPPORTED_PROPOSALS),
        Some(SUPPORTED_CREDENTIALS),
    )
}

/// Capabilities used in self-group leaf nodes.
pub fn self_group_leaf_node_capabilities() -> Capabilities {
    Capabilities::new(
        Some(SUPPORTED_PROTOCOL_VERSIONS),
        Some(SUPPORTED_CIPHERSUITES),
        Some(SUPPORTED_EXTENSIONS),
        Some(SUPPORTED_PROPOSALS),
        Some(SELF_GROUP_SUPPORTED_CREDENTIALS),
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn required_capabilities_is_subset_of_supported_capabilities() {
        for extension in REQUIRED_EXTENSIONS {
            assert!(SUPPORTED_EXTENSIONS.contains(extension));
        }
        for proposal in REQUIRED_PROPOSALS {
            assert!(SUPPORTED_PROPOSALS.contains(proposal));
        }
        for credential in REQUIRED_CREDENTIALS {
            assert!(SUPPORTED_CREDENTIALS.contains(credential));
            assert!(SELF_GROUP_SUPPORTED_CREDENTIALS.contains(credential));
        }
    }

    #[test]
    fn group_capabilities_is_subset_of_leaf_node_capabilities() {
        let group_extensions = REQUIRED_EXTENSIONS;
        let leaf_node_extensions = SUPPORTED_EXTENSIONS;
        for capability in group_extensions {
            assert!(leaf_node_extensions.contains(capability));
        }
    }
}
