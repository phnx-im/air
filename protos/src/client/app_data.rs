// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Typed views of Air's MLS app data dictionary.
//!
//! There are four layers:
//!
//! 1. Component bodies: bytes under an id. Live in `component.rs`.
//! 2. The dictionary: `AppDataDictionary`, a map from id to bytes, plus the meta entries
//!    `AppComponents` and `SafeAad`. This is what the types below model.
//! 3. The extension: `Extension::AppDataDictionary(AppDataDictionaryExtension)`, one entry in an
//!    extensions list. Wrapper around the second layer.
//! 4. The extension list: `Extensions<LeafNode>`, `Extensions<KeyPackage>`,
//!    `Extensions<GroupContext>`. Holds the third layer next to unrelated things: the QS client
//!    reference in leaves, required capabilities and group data extension in the group context.

use mls_assist::components::ComponentsList;
use openmls::{
    component::{ComponentId, ComponentType},
    components::vc_derivation_info::VC_COMPONENT_ID,
    group::GroupContext,
    prelude::{
        AppDataDictionary, AppDataDictionaryExtension, Extension, Extensions, KeyPackage, LeafNode,
    },
};
use tls_codec::{DeserializeBytes, Serialize};
use tracing::error;

use crate::client::component::{AIR_COMPONENT_ID, AirComponent, AirFeatures};

/// List of components supported by this client.
const SUPPORTED_COMPONENTS: &[ComponentId] = &[AIR_COMPONENT_ID];

/// App data a client advertises in its leaf node or key package.
///
/// Typed view of the app data dictionary in those places. The group context has its own view, see
/// [`GroupAppData`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAppData {
    /// Features of the client operating the leaf.
    pub features: AirFeatures,
    /// The leaf is operated by a virtual client
    ///
    /// Read from the `AppComponents` entry, see `VC_COMPONENT_ID`.
    pub virtual_client: bool,
}

/// App data describing a group, stored in its group context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupAppData {
    /// Fixed at creation
    pub is_self_group: bool,
    /// Components whose SafeAAD is required.
    ///
    /// None if the group does not use SafeAAD.
    pub safe_aad_components: Option<Vec<ComponentId>>,
    // LATER
    // pub group_profile: Option<GroupProfileComponent>,
}

impl ClientAppData {
    /// What this version of the client advertises.
    pub fn current() -> Self {
        Self {
            features: AirFeatures::default_leaf_or_key_package_features(),
            virtual_client: false,
        }
    }

    pub fn current_virtual_client() -> Self {
        Self {
            virtual_client: true,
            ..Self::current()
        }
    }

    // Layer 2: the dictionary

    /// `None` if the dictionary carries no Air component.
    fn from_dictionary(dict: &AppDataDictionary) -> Option<Self> {
        let component = air_component(dict)?;
        Some(Self {
            features: component.features,
            virtual_client: is_virtual_client(dict),
        })
    }

    /// Refreshes the client app data in an existing leaf dictionary to what this version of the
    /// client advertises.
    ///
    /// Whether the leaf is operated by a virtual client is preserved. Entries not owned by Air are
    /// left untouched.
    pub fn refresh(dict: &mut AppDataDictionary) {
        Self {
            virtual_client: is_virtual_client(dict),
            ..Self::current()
        }
        .write_into(dict);
    }

    /// Same as [`Self::refresh`], but advertises `features` instead of the current ones.
    ///
    /// Whether the leaf is operated by a virtual client is preserved. Entries not owned by Air are
    /// left untouched.
    pub fn refresh_features(dict: &mut AppDataDictionary, features: AirFeatures) {
        Self {
            features,
            virtual_client: is_virtual_client(dict),
        }
        .write_into(dict);
    }

    fn write_into(&self, dict: &mut AppDataDictionary) {
        let mut component_ids = SUPPORTED_COMPONENTS.to_vec();
        if self.virtual_client {
            component_ids.push(VC_COMPONENT_ID);
        }
        insert_components_list(dict, ComponentType::AppComponents.into(), component_ids);
        insert_air_component(
            dict,
            &AirComponent {
                features: self.features.clone(),
                // In a leaf node or a key package, this flag has no meaning
                // => it must be false.
                is_self_group: false,
            },
        );
    }

    fn to_dictionary(&self) -> AppDataDictionary {
        let mut dict = AppDataDictionary::new();
        self.write_into(&mut dict);
        dict
    }

    // Layer 3 and 4: extension and extensions list.

    pub fn from_leaf(leaf: &LeafNode) -> Option<Self> {
        Self::from_leaf_extensions(leaf.extensions())
    }

    fn from_leaf_extensions(extensions: &Extensions<LeafNode>) -> Option<Self> {
        Self::from_dictionary(extensions.app_data_dictionary()?.dictionary())
    }

    /// Whether the leaf is operated by a virtual client.
    ///
    /// Only looks at the `AppComponents` entry, so it also works for leaves
    /// without a decodable Air component.
    pub fn leaf_is_virtual_client(leaf: &LeafNode) -> bool {
        leaf.extensions()
            .app_data_dictionary()
            .is_some_and(|extension| is_virtual_client(extension.dictionary()))
    }

    pub fn to_extension(&self) -> Extension {
        Extension::AppDataDictionary(AppDataDictionaryExtension::new(self.to_dictionary()))
    }

    pub fn leaf_node_extensions(&self) -> Extensions<LeafNode> {
        Extensions::from_vec(vec![self.to_extension()]).expect("invalid extensions")
    }

    pub fn key_package_extensions(&self) -> Extensions<KeyPackage> {
        Extensions::from_vec(vec![self.to_extension()]).expect("invalid extensions")
    }
}

impl GroupAppData {
    // Layer 2: the dictionary

    fn from_dictionary(dict: &AppDataDictionary) -> Option<Self> {
        let component = air_component(dict)?;
        let safe_aad_components =
            components_list(dict, ComponentType::SafeAad.into()).map(|list| list.component_ids);
        Some(Self {
            is_self_group: component.is_self_group,
            safe_aad_components,
        })
    }

    fn to_dictionary(&self) -> AppDataDictionary {
        let mut component_ids = SUPPORTED_COMPONENTS.to_vec();
        if self.safe_aad_components.is_some() {
            component_ids.push(ComponentType::SafeAad.into());
        }
        let mut dict = AppDataDictionary::new();
        insert_components_list(
            &mut dict,
            ComponentType::AppComponents.into(),
            component_ids,
        );
        if let Some(ids) = &self.safe_aad_components {
            insert_components_list(&mut dict, ComponentType::SafeAad.into(), ids.clone());
        }
        insert_air_component(
            &mut dict,
            &AirComponent {
                // TODO: What is actually the meaning of this feature matrix in the group context?
                features: AirFeatures::default_leaf_or_key_package_features(),
                is_self_group: self.is_self_group,
            },
        );
        dict
    }

    // Layer 3 and 4: extension and extensions list.

    fn from_group_context(extensions: &Extensions<GroupContext>) -> Option<Self> {
        Self::from_dictionary(extensions.app_data_dictionary()?.dictionary())
    }

    pub fn is_self_group_context(extensions: &Extensions<GroupContext>) -> bool {
        Self::from_group_context(extensions).is_some_and(|data| data.is_self_group)
    }

    pub fn to_extension(&self) -> Extension {
        Extension::AppDataDictionary(AppDataDictionaryExtension::new(self.to_dictionary()))
    }
}

/// Returns the components list for the given component id.
///
/// Note that both components `AppComponents` and `SafeAad` are stored in the app data as a
/// components list.
fn components_list(dict: &AppDataDictionary, id: ComponentId) -> Option<ComponentsList> {
    let data = dict.get(&id)?;
    ComponentsList::tls_deserialize_exact_bytes(data)
        .inspect_err(|error| error!(%error, "Failed to deserialize components list"))
        .ok()
}

fn insert_components_list(
    dict: &mut AppDataDictionary,
    id: ComponentId,
    component_ids: Vec<ComponentId>,
) {
    dict.insert(
        id,
        ComponentsList { component_ids }
            .tls_serialize_detached()
            .expect("invalid components list"),
    );
}

fn air_component(dict: &AppDataDictionary) -> Option<AirComponent> {
    let data = dict.get(&AIR_COMPONENT_ID)?;
    AirComponent::from_bytes(data)
        .inspect_err(|error| error!(%error, "Failed to deserialize Air component"))
        .ok()
}

fn insert_air_component(dict: &mut AppDataDictionary, component: &AirComponent) {
    dict.insert(
        AIR_COMPONENT_ID,
        component.to_bytes().expect("invalid Air component"),
    );
}

fn is_virtual_client(dict: &AppDataDictionary) -> bool {
    components_list(dict, ComponentType::AppComponents.into())
        .is_some_and(|list| list.component_ids.contains(&VC_COMPONENT_ID))
}

#[cfg(test)]
mod test {
    use aircommon::codec::PersistenceCodec;

    use super::*;

    fn dictionary_of(extension: Extension) -> AppDataDictionary {
        let Extension::AppDataDictionary(extension) = extension else {
            panic!("not an app data dictionary extension");
        };
        extension.dictionary().clone()
    }

    #[test]
    fn default_extensions_are_valid() {
        // Checks that the functions below never panic
        let _ = ClientAppData::current().to_extension();
        let _ = ClientAppData::current().leaf_node_extensions();
        let _ = ClientAppData::current().key_package_extensions();
        let _ = ClientAppData::current_virtual_client().leaf_node_extensions();
    }

    /// Default extensions can be extended by must be backwards compatible.
    #[test]
    fn default_extensions_stability() {
        let leaf_node_extensions = ClientAppData::current().leaf_node_extensions();
        let key_package_extensions = ClientAppData::current().key_package_extensions();
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

    #[test]
    fn client_app_data_round_trip() {
        let data = ClientAppData::current_virtual_client();
        let dict = data.to_dictionary();
        assert_eq!(ClientAppData::from_dictionary(&dict), Some(data));

        let leaf = ClientAppData::current().leaf_node_extensions();
        assert_eq!(
            ClientAppData::from_leaf_extensions(&leaf),
            Some(ClientAppData::current())
        );
        assert_eq!(
            ClientAppData::from_leaf_extensions(&Extensions::<LeafNode>::empty()),
            None
        );
    }

    #[test]
    fn refresh_keeps_virtual_client_marker_and_foreign_entries() {
        const FOREIGN_COMPONENT_ID: ComponentId = 0x8043;
        let mut dict = ClientAppData {
            features: AirFeatures::default(),
            virtual_client: true,
        }
        .to_dictionary();
        dict.insert(FOREIGN_COMPONENT_ID, b"foreign".to_vec());

        ClientAppData::refresh(&mut dict);

        let data = ClientAppData::from_dictionary(&dict).unwrap();
        assert!(data.virtual_client);
        assert_eq!(
            data.features,
            AirFeatures::default_leaf_or_key_package_features()
        );
        assert_eq!(
            dict.get(&FOREIGN_COMPONENT_ID).unwrap(),
            b"foreign".to_vec()
        );
    }

    #[test]
    fn refresh_adds_missing_air_component() {
        let mut dict = AppDataDictionary::new();
        assert_eq!(ClientAppData::from_dictionary(&dict), None);

        ClientAppData::refresh(&mut dict);
        assert_eq!(
            ClientAppData::from_dictionary(&dict),
            Some(ClientAppData::current())
        );
    }

    #[test]
    fn self_group_flag_is_read_from_group_context_extensions() {
        let group_context_extensions = |is_self_group| {
            Extensions::from_vec(vec![
                GroupAppData {
                    is_self_group,
                    safe_aad_components: None,
                }
                .to_extension(),
            ])
            .unwrap()
        };

        assert!(GroupAppData::is_self_group_context(
            &group_context_extensions(true)
        ));
        assert!(!GroupAppData::is_self_group_context(
            &group_context_extensions(false)
        ));

        // A missing component counts as not a self-group.
        assert!(!GroupAppData::is_self_group_context(&Extensions::empty()));
    }

    #[test]
    fn group_app_data_round_trip() {
        let data = GroupAppData {
            is_self_group: true,
            safe_aad_components: Some(vec![0x8042]),
        };
        let extensions = Extensions::from_vec(vec![data.to_extension()]).unwrap();
        assert_eq!(GroupAppData::from_group_context(&extensions), Some(data));
    }

    /// `safe_aad_required()` on the group context checks for a dictionary entry whose *key* is the
    /// SafeAad component id. This pins that the helper puts the marker in the right place: a wrong
    /// placement compiles and runs, but silently disables the entire SafeAAD pipeline.
    #[test]
    fn group_context_dictionary_with_safe_aad() {
        const REQUIRED_SAFE_AAD_COMPONENT_ID: ComponentId = 0x8042;
        let dict = dictionary_of(
            GroupAppData {
                is_self_group: false,
                safe_aad_components: Some(vec![REQUIRED_SAFE_AAD_COMPONENT_ID]),
            }
            .to_extension(),
        );

        // The SafeAad entry is present as a dictionary key...
        let safe_aad_id = ComponentId::from(ComponentType::SafeAad);
        assert!(dict.contains(&safe_aad_id));

        // ...and its value parses as a `ComponentsList` carrying the given ids
        // (`safe_aad_required_components()` errors on unparsable values).
        let value = dict.get(&safe_aad_id).unwrap();
        let list: ComponentsList = tls_codec::Deserialize::tls_deserialize_exact(value).unwrap();
        assert_eq!(list.component_ids, vec![REQUIRED_SAFE_AAD_COMPONENT_ID]);

        // The AppComponents entry is present and parseable, too, and advertises the embedded
        // component.
        let value = dict
            .get(&ComponentId::from(ComponentType::AppComponents))
            .unwrap();
        let list: ComponentsList = tls_codec::Deserialize::tls_deserialize_exact(value).unwrap();
        assert!(list.component_ids.contains(&AIR_COMPONENT_ID));

        // The component itself is embedded in the dictionary.
        assert!(dict.contains(&AIR_COMPONENT_ID));
    }

    #[test]
    fn group_context_dictionary_without_safe_aad() {
        let dict = dictionary_of(
            GroupAppData {
                is_self_group: false,
                safe_aad_components: None,
            }
            .to_extension(),
        );

        assert!(!dict.contains(&ComponentId::from(ComponentType::SafeAad)));

        let value = dict
            .get(&ComponentId::from(ComponentType::AppComponents))
            .unwrap();
        let list: ComponentsList = tls_codec::Deserialize::tls_deserialize_exact(value).unwrap();
        assert_eq!(list.component_ids, vec![AIR_COMPONENT_ID]);
    }

    #[test]
    fn refresh_replaces_foreign_component_ids() {
        let mut dict = AppDataDictionary::new();
        insert_components_list(&mut dict, ComponentType::AppComponents.into(), vec![0x8043]);

        ClientAppData::refresh(&mut dict);

        let ids = components_list(&dict, ComponentType::AppComponents.into())
            .unwrap()
            .component_ids;
        assert_eq!(ids, SUPPORTED_COMPONENTS);
    }
}
