// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Configuration for MLS groups.

use mls_assist::{
    components::ComponentsList,
    openmls::{
        component::{ComponentId, ComponentType},
        group::{MlsGroupJoinConfig, PURE_PLAINTEXT_WIRE_FORMAT_POLICY},
        prelude::{
            AppDataDictionary, AppDataDictionaryExtension, Capabilities, Ciphersuite,
            CredentialType, Extension, ExtensionType, ExtensionValidator, Extensions,
            InvalidExtensionError, KeyPackage, LeafNode, ProposalType, ProtocolVersion,
            RequiredCapabilitiesExtension, SenderRatchetConfiguration,
        },
    },
};
use tls_codec::Serialize;

use crate::component::AirComponent;

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

const PQ_CIPHERSUITE: Ciphersuite = Ciphersuite::AIR_128_MLKEM768_AES256GCM_SHA384_Ed25519;

// Required capabilities
const REQUIRED_PROTOCOL_VERSIONS: &[ProtocolVersion] = &[DEFAULT_MLS_VERSION];
const REQUIRED_CIPHERSUITES: &[Ciphersuite] = &[DEFAULT_CIPHERSUITE];
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
    ExtensionType::Unknown(QS_CLIENT_REFERENCE_EXTENSION_TYPE),
    ExtensionType::Unknown(GROUP_DATA_EXTENSION_TYPE),
    ExtensionType::LastResort,
    ExtensionType::AppDataDictionary,
];
pub const SUPPORTED_PROPOSALS: &[ProposalType] = REQUIRED_PROPOSALS;
pub const SUPPORTED_CREDENTIALS: &[CredentialType] = REQUIRED_CREDENTIALS;
pub const SUPPORTED_COMPONENTS: &[ComponentId] = &[AIR_COMPONENT_ID];

/// Capabilities that are required to be a member of a group.
///
/// Warning: changing this capabilities requires backwards compatibility considerations.
pub fn default_required_group_capabilities() -> Capabilities {
    Capabilities::new(
        Some(REQUIRED_PROTOCOL_VERSIONS),
        Some(REQUIRED_CIPHERSUITES),
        Some(REQUIRED_EXTENSIONS),
        Some(REQUIRED_PROPOSALS),
        Some(REQUIRED_CREDENTIALS),
    )
}

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

/// The component id of the Air component.
pub const AIR_COMPONENT_ID: ComponentId = 0x8000;

/// Extension used in the leaf node.
pub fn default_leaf_node_extensions() -> Extensions<LeafNode> {
    default_extensions()
}

/// Extension used in the key package.
pub fn default_key_package_extensions() -> Extensions<KeyPackage> {
    default_extensions()
}

/// # Panics
///
/// Since we are building a single static essential extension here, we can assume that this
/// function never panics. Panic-safety is additionally tested in the unit tests.
fn default_extensions<T>() -> Extensions<T>
where
    T: ExtensionValidator,
    InvalidExtensionError: From<T::Error>,
{
    Extensions::from_vec(vec![default_app_data_dictionary_extension()]).expect("invalid extensions")
}

/// # Panics
///
/// Since we are building a static list of components here, we can assume that this function never
/// panics. Panic-safety is additionally tested in the unit tests.
pub fn default_app_data_dictionary_extension() -> Extension {
    let mut app_data_dictionary = AppDataDictionary::new();

    // Advertise that we support the Air component in the app data dictionary.
    app_data_dictionary.insert(
        ComponentType::AppComponents.into(),
        ComponentsList {
            component_ids: SUPPORTED_COMPONENTS.to_vec(),
        }
        .tls_serialize_detached()
        .expect("invalid component list"),
    );

    // Add Air component to the app data dictionary.
    app_data_dictionary.insert(
        AIR_COMPONENT_ID,
        AirComponent::default_leaf_or_key_package_component()
            .to_bytes()
            .expect("invalid Air component"),
    );

    Extension::AppDataDictionary(AppDataDictionaryExtension::new(app_data_dictionary))
}

#[cfg(test)]
mod test {
    use mls_assist::openmls::component::PrivateComponentId;

    use crate::codec::PersistenceCodec;

    use super::*;

    #[test]
    fn required_capabilities_is_subset_of_supported_capabilities() {
        for version in REQUIRED_PROTOCOL_VERSIONS {
            assert!(SUPPORTED_PROTOCOL_VERSIONS.contains(version));
        }
        for ciphersuite in REQUIRED_CIPHERSUITES {
            assert!(SUPPORTED_CIPHERSUITES.contains(ciphersuite));
        }
        for extension in REQUIRED_EXTENSIONS {
            assert!(SUPPORTED_EXTENSIONS.contains(extension));
        }
        for proposal in REQUIRED_PROPOSALS {
            assert!(SUPPORTED_PROPOSALS.contains(proposal));
        }
        for credential in REQUIRED_CREDENTIALS {
            assert!(SUPPORTED_CREDENTIALS.contains(credential));
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

    #[test]
    fn air_component_id_is_private() {
        PrivateComponentId::new(AIR_COMPONENT_ID).expect("Should be private");
    }

    #[test]
    fn default_extensions_are_valid() {
        // Checks that the function below never panic
        let _ = default_app_data_dictionary_extension();
        let _ = default_leaf_node_extensions();
        let _ = default_key_package_extensions();
    }

    #[test]
    fn default_required_group_capabilities_stability() {
        let capabilities = default_required_group_capabilities();
        let bytes = PersistenceCodec::to_vec(&capabilities).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    /// Default extensions can be extended by must be backwards compatible.
    #[test]
    fn default_extensions_stability() {
        let leaf_node_extensions = default_leaf_node_extensions();
        let key_package_extensions = default_key_package_extensions();
        for (a, b) in leaf_node_extensions
            .iter()
            .zip(key_package_extensions.iter())
        {
            assert_eq!(a, b);
        }

        let bytes = PersistenceCodec::to_vec(&leaf_node_extensions).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }
}
