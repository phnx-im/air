// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    codec::{self, PersistenceCodec},
    mls_group_config::AppComponent,
};
use airmacros::{DeserializeTaggedMap, SerializeTaggedMap};
use mls_assist::openmls::{component::ComponentId, group::GroupContext, prelude::Extensions};

/// The component id of the Air component.
pub const AIR_COMPONENT_ID: ComponentId = 0x8000;

/// List of components supported by this client.
pub const SUPPORTED_COMPONENTS: &[ComponentId] = &[AIR_COMPONENT_ID];

/// Custom component storing client-specific features and data.
///
/// Stored in the app data extension of the group context, leaf node or key package.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct AirComponent {
    /// Features supported by the client in the corresponding context (group, leaf node or key
    /// package).
    #[tag(1)]
    pub features: AirFeatures,
    /// Whether the group this component is attached to is a self-group, i.e. a group used by a
    /// single user to sync data between their own clients.
    ///
    /// Only meaningful in the group context; always `false` in leaf nodes and key packages.
    #[tag(2)]
    pub is_self_group: bool,
}

/// List of features supported by the client.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct AirFeatures {
    /// Whether the client supports encrypted group profiles.
    #[tag(1)]
    pub encrypted_group_profiles: bool,
    /// Whether the client supports connection groups without attributes, in particular without a
    /// title.
    ///
    /// When changing the connection group's title to an empty string, the client will not display
    /// any system message about this.
    #[tag(2)]
    pub empty_connection_group_attributes: bool,
    /// Whether the client supports [APQMLS] (Amortized PQ MLS Combiner).
    ///
    /// [APQMLS]: https://datatracker.ietf.org/doc/html/draft-ietf-mls-combiner
    #[tag(3)]
    pub pq_groups: bool,
}

impl AirComponent {
    pub fn to_bytes(&self) -> Result<Vec<u8>, codec::Error> {
        PersistenceCodec::to_vec(self)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, codec::Error> {
        PersistenceCodec::from_slice(bytes)
    }

    /// Reads the component from a group context's extensions.
    ///
    /// Returns `None` if the app data dictionary, the component entry, or a decodable component is
    /// missing.
    pub fn from_group_context_extensions(extensions: &Extensions<GroupContext>) -> Option<Self> {
        let data = extensions
            .app_data_dictionary()?
            .dictionary()
            .get(&AIR_COMPONENT_ID)?;
        Self::from_bytes(data).ok()
    }

    /// Whether the group context extensions mark the group as a self-group.
    ///
    /// A missing or undecodable component counts as not a self-group.
    pub fn is_self_group_context(extensions: &Extensions<GroupContext>) -> bool {
        Self::from_group_context_extensions(extensions)
            .is_some_and(|component| component.is_self_group)
    }
}

impl AirFeatures {
    /// The features that are supported by the current version of the client.
    ///
    /// Note: This is *not* the default implementation of `AirFeatures::default`. It contains all
    /// supported features of the current version of the client.
    pub fn default_leaf_or_key_package_features() -> Self {
        Self {
            encrypted_group_profiles: true,
            empty_connection_group_attributes: true,
            pq_groups: true,
        }
    }
}

impl AppComponent for AirComponent {
    const COMPONENT_ID: ComponentId = AIR_COMPONENT_ID;

    fn default_for_leaf_or_key_package() -> Self {
        Self {
            features: AirFeatures::default_leaf_or_key_package_features(),
            is_self_group: false,
        }
    }

    fn default_for_self_group() -> Self {
        Self {
            features: AirFeatures::default_leaf_or_key_package_features(),
            is_self_group: true,
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        AirComponent::to_bytes(self).expect("invalid component")
    }
}

#[cfg(test)]
mod test {
    use aircommon::{
        codec::PersistenceCodec,
        mls_group_config::{
            default_app_data_dictionary_extension,
            default_group_context_app_data_dictionary_extension, default_key_package_extensions,
            default_leaf_node_extensions,
        },
    };
    use mls_assist::openmls::component::PrivateComponentId;

    use super::*;

    #[test]
    fn air_component_id_is_private() {
        PrivateComponentId::new(AIR_COMPONENT_ID).expect("Should be private");
    }

    #[test]
    fn default_extensions_are_valid() {
        // Checks that the function below never panic
        let _ = default_app_data_dictionary_extension::<AirComponent>();
        let _ = default_leaf_node_extensions::<AirComponent>();
        let _ = default_key_package_extensions::<AirComponent>();
    }

    #[test]
    fn self_group_flag_is_read_from_group_context_extensions() {
        let group_context_extensions = |component| {
            Extensions::from_vec(vec![default_group_context_app_data_dictionary_extension(
                component, None,
            )])
            .unwrap()
        };

        let self_group = group_context_extensions(AirComponent::default_for_self_group());
        assert!(AirComponent::is_self_group_context(&self_group));

        let regular_group =
            group_context_extensions(AirComponent::default_for_leaf_or_key_package());
        assert!(!AirComponent::is_self_group_context(&regular_group));

        // A missing component counts as not a self-group.
        assert!(!AirComponent::is_self_group_context(&Extensions::empty()));
    }

    /// Default extensions can be extended by must be backwards compatible.
    #[test]
    fn default_extensions_stability() {
        let leaf_node_extensions = default_leaf_node_extensions::<AirComponent>();
        let key_package_extensions = default_key_package_extensions::<AirComponent>();
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
