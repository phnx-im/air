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

pub const REQUIRED_EXTENSION_TYPES: &[ExtensionType] = &[
    ExtensionType::Unknown(QS_CLIENT_REFERENCE_EXTENSION_TYPE),
    ExtensionType::Unknown(GROUP_DATA_EXTENSION_TYPE),
    ExtensionType::LastResort,
];
pub const REQUIRED_PROPOSAL_TYPES: &[ProposalType] = &[
    ProposalType::Custom(FRIENDSHIP_PACKAGE_PROPOSAL_TYPE),
    ProposalType::SelfRemove,
];
pub const REQUIRED_CREDENTIAL_TYPES: &[CredentialType] = &[CredentialType::Basic];

pub fn default_required_capabilities() -> RequiredCapabilitiesExtension {
    RequiredCapabilitiesExtension::new(
        REQUIRED_EXTENSION_TYPES,
        REQUIRED_PROPOSAL_TYPES,
        REQUIRED_CREDENTIAL_TYPES,
    )
}

// Default capabilities for every leaf node we create.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[ProtocolVersion] = &[DEFAULT_MLS_VERSION];
pub const SUPPORTED_CIPHERSUITES: &[Ciphersuite] = &[DEFAULT_CIPHERSUITE];
pub const SUPPORTED_EXTENSIONS: &[ExtensionType] = REQUIRED_EXTENSION_TYPES;
pub const SUPPORTED_PROPOSALS: &[ProposalType] = REQUIRED_PROPOSAL_TYPES;
pub const SUPPORTED_CREDENTIALS: &[CredentialType] = REQUIRED_CREDENTIAL_TYPES;

/// Capabilities that are required to be a member of a group.
pub fn default_required_group_capabilities() -> Capabilities {
    Capabilities::new(
        Some(SUPPORTED_PROTOCOL_VERSIONS),
        Some(SUPPORTED_CIPHERSUITES),
        Some(SUPPORTED_EXTENSIONS),
        Some(SUPPORTED_PROPOSALS),
        Some(SUPPORTED_CREDENTIALS),
    )
}

pub const SUPPORTED_LEAF_NODE_EXTENSIONS: &[ExtensionType] = &[
    ExtensionType::Unknown(QS_CLIENT_REFERENCE_EXTENSION_TYPE),
    ExtensionType::Unknown(GROUP_DATA_EXTENSION_TYPE),
    ExtensionType::LastResort,
    ExtensionType::AppDataDictionary,
];

pub fn default_leaf_node_capabilities() -> Capabilities {
    Capabilities::new(
        Some(SUPPORTED_PROTOCOL_VERSIONS),
        Some(SUPPORTED_CIPHERSUITES),
        Some(SUPPORTED_LEAF_NODE_EXTENSIONS),
        Some(SUPPORTED_PROPOSALS),
        Some(SUPPORTED_CREDENTIALS),
    )
}

/// The component id of the Air component.
pub const AIR_COMPONENT_ID: ComponentId = 0x8000;

pub fn default_leaf_node_extensions() -> Extensions<LeafNode> {
    default_extensions()
}

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
            component_ids: vec![AIR_COMPONENT_ID],
        }
        .tls_serialize_detached()
        .expect("invalid component list"),
    );

    // Add the Air component to the app data dictionary.
    app_data_dictionary.insert(AIR_COMPONENT_ID, default_air_component());

    Extension::AppDataDictionary(AppDataDictionaryExtension::new(app_data_dictionary))
}

pub fn default_air_component() -> Vec<u8> {
    // TODO
    vec![]
}

#[cfg(test)]
mod test {
    use mls_assist::openmls::component::PrivateComponentId;

    use super::*;

    #[test]
    fn group_capabilities_is_subset_of_leaf_node_capabilities() {
        let group_extensions = SUPPORTED_EXTENSIONS;
        let leaf_node_extensions = SUPPORTED_LEAF_NODE_EXTENSIONS;
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
}
