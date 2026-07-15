// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Configuration for MLS groups.

use apqmls::ApqCiphersuite;
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

/// An app-level MLS component that can be stored in the app data dictionary of a group, leaf node,
/// or key package.
///
/// This is a dependency-inversion. The implementation lives in higher-level crates so that this
/// crate stays free of any specific component's data model.
pub trait AppComponent {
    /// The component id under which the serialized component is stored in the app data dictionary.
    const COMPONENT_ID: ComponentId;

    /// The default component instance to store in a freshly-created leaf node or key package.
    fn default_for_leaf_or_key_package() -> Self;

    /// The default component instance to store in a freshly-created self-group for virtual clients.
    fn default_for_self_group() -> Self;

    /// Serializes the component into the on-the-wire bytes that are stored in the app data
    /// dictionary.
    fn to_bytes(&self) -> Vec<u8>;
}

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
];
pub const SUPPORTED_CREDENTIALS: &[CredentialType] = REQUIRED_CREDENTIALS;

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

/// Extension used in the leaf node.
pub fn default_leaf_node_extensions<C: AppComponent>() -> Extensions<LeafNode> {
    default_extensions::<LeafNode, C>()
}

/// Extension used in the key package.
pub fn default_key_package_extensions<C: AppComponent>() -> Extensions<KeyPackage> {
    default_extensions::<KeyPackage, C>()
}

/// # Panics
///
/// Since we are building a single static essential extension here, we can assume that this
/// function never panics. Panic-safety is additionally tested in the unit tests.
fn default_extensions<T, C>() -> Extensions<T>
where
    T: ExtensionValidator,
    InvalidExtensionError: From<T::Error>,
    C: AppComponent,
{
    Extensions::from_vec(vec![default_app_data_dictionary_extension::<C>()])
        .expect("invalid extensions")
}

/// Extension which contains the default app data dictionary for the group context.
///
/// Embeds `component` in the dictionary and advertises it via the `AppComponents` entry.
///
/// If `safe_aad_required` is true, the app data dictionary sets the `SafeAad` component as a
/// required component.
pub fn default_group_context_app_data_dictionary_extension<C: AppComponent>(
    component: C,
    safe_aad_components: Option<Vec<ComponentId>>,
) -> Extension {
    let mut component_ids = vec![C::COMPONENT_ID];
    if safe_aad_components.is_some() {
        component_ids.push(ComponentType::SafeAad.into());
    }

    let mut app_data_dictionary = AppDataDictionary::new();
    app_data_dictionary.insert(
        ComponentType::AppComponents.into(),
        ComponentsList { component_ids }
            .tls_serialize_detached()
            .expect("invalid component list"),
    );
    app_data_dictionary.insert(C::COMPONENT_ID, component.to_bytes());
    if let Some(component_ids) = safe_aad_components {
        app_data_dictionary.insert(
            ComponentType::SafeAad.into(),
            ComponentsList { component_ids }
                .tls_serialize_detached()
                .expect("invalid component list"),
        );
    }

    Extension::AppDataDictionary(AppDataDictionaryExtension::new(app_data_dictionary))
}

/// Extension which contains the default app data dictionary for the leaf node/key package.
pub fn default_app_data_dictionary_extension<C: AppComponent>() -> Extension {
    app_data_dictionary_extension::<C>(vec![C::COMPONENT_ID])
}

/// App data dictionary for a leaf node/key package which embeds the default
/// component and advertises `component_ids` via the `AppComponents` entry.
fn app_data_dictionary_extension<C: AppComponent>(component_ids: Vec<ComponentId>) -> Extension {
    let mut app_data_dictionary = AppDataDictionary::new();

    // Advertise the supported components in the app data dictionary.
    app_data_dictionary.insert(
        ComponentType::AppComponents.into(),
        ComponentsList { component_ids }
            .tls_serialize_detached()
            .expect("invalid component list"),
    );

    // Add the component to the app data dictionary.
    app_data_dictionary.insert(
        C::COMPONENT_ID,
        C::default_for_leaf_or_key_package().to_bytes(),
    );

    Extension::AppDataDictionary(AppDataDictionaryExtension::new(app_data_dictionary))
}

#[cfg(test)]
mod test {
    use super::*;

    struct TestComponent;

    impl AppComponent for TestComponent {
        const COMPONENT_ID: ComponentId = 0x8043;

        fn default_for_leaf_or_key_package() -> Self {
            Self
        }

        fn default_for_self_group() -> Self {
            Self
        }

        fn to_bytes(&self) -> Vec<u8> {
            b"test".to_vec()
        }
    }

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

    fn dictionary_of(extension: Extension) -> AppDataDictionary {
        let Extension::AppDataDictionary(extension) = extension else {
            panic!("not an app data dictionary extension");
        };
        extension.dictionary().clone()
    }

    /// `safe_aad_required()` on the group context checks for a dictionary entry whose *key* is the
    /// SafeAad component id. This pins that the helper puts the marker in the right place: a wrong
    /// placement compiles and runs, but silently disables the entire SafeAAD pipeline.
    #[test]
    fn group_context_dictionary_with_safe_aad() {
        const REQUIRED_SAFE_AAD_COMPONENT_ID: ComponentId = 0x8042;
        let dictionary = dictionary_of(default_group_context_app_data_dictionary_extension(
            TestComponent,
            Some(vec![REQUIRED_SAFE_AAD_COMPONENT_ID]),
        ));

        // The SafeAad entry is present as a dictionary key...
        let safe_aad_id = ComponentId::from(ComponentType::SafeAad);
        assert!(dictionary.contains(&safe_aad_id));

        // ...and its value parses as a `ComponentsList` carrying the given ids
        // (`safe_aad_required_components()` errors on unparsable values).
        let value = dictionary.get(&safe_aad_id).unwrap();
        let list: ComponentsList = tls_codec::Deserialize::tls_deserialize_exact(value).unwrap();
        assert_eq!(list.component_ids, vec![REQUIRED_SAFE_AAD_COMPONENT_ID]);

        // The AppComponents entry is present and parseable, too, and advertises the embedded
        // component.
        let value = dictionary
            .get(&ComponentId::from(ComponentType::AppComponents))
            .unwrap();
        let list: ComponentsList = tls_codec::Deserialize::tls_deserialize_exact(value).unwrap();
        assert!(list.component_ids.contains(&TestComponent::COMPONENT_ID));

        // The component itself is embedded in the dictionary.
        assert_eq!(
            dictionary.get(&TestComponent::COMPONENT_ID).unwrap(),
            TestComponent.to_bytes()
        );
    }

    #[test]
    fn group_context_dictionary_without_safe_aad() {
        let dictionary = dictionary_of(default_group_context_app_data_dictionary_extension(
            TestComponent,
            None,
        ));

        assert!(!dictionary.contains(&ComponentId::from(ComponentType::SafeAad)));

        let value = dictionary
            .get(&ComponentId::from(ComponentType::AppComponents))
            .unwrap();
        let list: ComponentsList = tls_codec::Deserialize::tls_deserialize_exact(value).unwrap();
        assert_eq!(list.component_ids, vec![TestComponent::COMPONENT_ID]);
    }
}
