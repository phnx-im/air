// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    codec::{self, PersistenceCodec},
    time::TimeStamp,
};
use airmacros::{DeserializeTaggedMap, SerializeTaggedMap};
use mls_assist::openmls::component::{ComponentData, ComponentId};

/// The component id of the Air component.
pub const AIR_COMPONENT_ID: ComponentId = 0x8000;

/// The component id of the component containing the group profile.
pub const AIR_GROUP_PROFILE_COMPONENT_ID: ComponentId = 0x8002;

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
    /// When the group was deleted, if it was.
    ///
    /// Set by the commit that deletes the group, to the deleter's local time
    /// when it built the commit. Only meaningful in the group context. Always
    /// `None` in leaf nodes and key packages.
    #[tag(3)]
    pub deleted: Option<TimeStamp>,
}

/// List of features supported by the client.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct AirFeatures {
    /// Whether the client supports encrypted group profiles.
    ///
    /// Note: Legacy clients not supporting this feature are not supported anymore.
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
    /// Whether this client can establish a connection group as an APQ group.
    #[tag(4)]
    pub apq_connection_groups: bool,
}

impl AirComponent {
    pub fn to_bytes(&self) -> Result<Vec<u8>, codec::Error> {
        PersistenceCodec::to_vec(self)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, codec::Error> {
        PersistenceCodec::from_slice(bytes)
    }

    pub fn to_component_data(&self) -> Result<ComponentData, codec::Error> {
        Ok(ComponentData::from_parts(
            AIR_COMPONENT_ID,
            self.to_bytes()?.into(),
        ))
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
            apq_connection_groups: true,
        }
    }
}

#[cfg(test)]
mod test {
    use chrono::{TimeZone, Utc};
    use mls_assist::openmls::component::PrivateComponentId;

    use super::*;

    #[test]
    fn air_component_id_is_private() {
        PrivateComponentId::new(AIR_COMPONENT_ID).expect("Should be private");
    }

    fn deleted_component() -> AirComponent {
        AirComponent {
            features: AirFeatures::default_leaf_or_key_package_features(),
            is_self_group: false,
            deleted: Some(Utc.with_ymd_and_hms(2026, 10, 13, 12, 0, 0).unwrap().into()),
        }
    }

    #[test]
    fn deleted_component_stability() {
        let bytes = deleted_component().to_bytes().unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    #[test]
    fn deleted_component_round_trip() {
        let component = deleted_component();
        let bytes = component.to_bytes().unwrap();
        assert_eq!(AirComponent::from_bytes(&bytes).unwrap(), component);
    }

    /// The component as it was before the `deleted` field existed.
    #[derive(Debug, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
    struct LegacyAirComponent {
        #[tag(1)]
        features: AirFeatures,
        #[tag(2)]
        is_self_group: bool,
    }

    #[test]
    fn deleted_field_encodes_as_before_when_unset_and_decodes_on_legacy_component() {
        let features = AirFeatures::default_leaf_or_key_package_features();
        let legacy = LegacyAirComponent {
            features: features.clone(),
            is_self_group: true,
        };
        let current = AirComponent {
            features,
            is_self_group: true,
            deleted: None,
        };
        assert_eq!(
            PersistenceCodec::to_vec(&legacy).unwrap(),
            current.to_bytes().unwrap()
        );

        let bytes = deleted_component().to_bytes().unwrap();
        let decoded: LegacyAirComponent = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(
            decoded,
            LegacyAirComponent {
                features: AirFeatures::default_leaf_or_key_package_features(),
                is_self_group: false,
            }
        );
    }
}
