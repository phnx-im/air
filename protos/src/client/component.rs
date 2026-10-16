// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    codec::{self, PersistenceCodec},
    time::TimeStamp,
};
use airmacros::{
    DeserializeTaggedMap, DeserializeTaggedUnion, SerializeTaggedMap, SerializeTaggedUnion,
};
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
    /// Marks the group as closed. No further operations are accepted on a
    /// group with a tombstone, whatever its context.
    ///
    /// Only meaningful in the group context. Always `None` in leaf nodes and
    /// key packages.
    #[tag(3)]
    pub tombstone: Option<GroupTombstone>,
}

/// When and why a group was closed.
///
/// ## CDDL Definition
///
/// ```cddl
/// GroupTombstone = {
///   ? timestamp: tstr .tag 1,
///   ? context: TombstoneContext .tag 2,
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct GroupTombstone {
    /// The closing client's local time when it built the commit that set the
    /// tombstone. Always set, the DS rejects a tombstone without it or more
    /// than a day away from its own time.
    #[tag(1)]
    pub timestamp: Option<TimeStamp>,
    #[tag(2)]
    pub context: TombstoneContext,
}

/// Why a group was closed.
///
/// ## CDDL Definition
///
/// ```cddl
/// TombstoneContext = {
///   1: null   ; the group was deleted by a member
/// }
/// ```
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Hash, SerializeTaggedUnion, DeserializeTaggedUnion,
)]
pub enum TombstoneContext {
    /// A member deleted the group.
    #[tag(1)]
    Deleted,
    /// A context this client does not understand, or none at all.
    #[default]
    #[unknown]
    Unknown,
}

impl GroupTombstone {
    /// The tombstone a member sets when deleting the group at `timestamp`.
    pub fn deleted(timestamp: TimeStamp) -> Self {
        Self {
            timestamp: Some(timestamp),
            context: TombstoneContext::Deleted,
        }
    }
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
            tombstone: Some(GroupTombstone::deleted(
                Utc.with_ymd_and_hms(2026, 10, 13, 12, 0, 0).unwrap().into(),
            )),
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

    /// The component as it was before the `tombstone` field existed.
    #[derive(Debug, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
    struct LegacyAirComponent {
        #[tag(1)]
        features: AirFeatures,
        #[tag(2)]
        is_self_group: bool,
    }

    #[test]
    fn tombstone_encodes_as_before_when_unset_and_decodes_on_legacy_component() {
        let features = AirFeatures::default_leaf_or_key_package_features();
        let legacy = LegacyAirComponent {
            features: features.clone(),
            is_self_group: true,
        };
        let current = AirComponent {
            features,
            is_self_group: true,
            tombstone: None,
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

    /// A tombstone context a later version might add.
    #[derive(SerializeTaggedUnion, DeserializeTaggedUnion)]
    enum FutureTombstoneContext {
        #[tag(9)]
        Archived,
        #[unknown]
        Unknown,
    }

    #[derive(SerializeTaggedMap, DeserializeTaggedMap)]
    struct FutureTombstone {
        #[tag(1)]
        timestamp: Option<TimeStamp>,
        #[tag(2)]
        context: Option<FutureTombstoneContext>,
    }

    #[derive(SerializeTaggedMap, DeserializeTaggedMap)]
    struct FutureAirComponent {
        #[tag(1)]
        features: AirFeatures,
        #[tag(3)]
        tombstone: Option<FutureTombstone>,
    }

    #[test]
    fn tombstone_with_an_unknown_context_decodes() {
        let timestamp = TimeStamp::from(Utc.with_ymd_and_hms(2026, 10, 13, 12, 0, 0).unwrap());
        let future = FutureAirComponent {
            features: AirFeatures::default_leaf_or_key_package_features(),
            tombstone: Some(FutureTombstone {
                timestamp: Some(timestamp),
                context: Some(FutureTombstoneContext::Archived),
            }),
        };
        let bytes = PersistenceCodec::to_vec(&future).unwrap();

        let decoded = AirComponent::from_bytes(&bytes).unwrap();
        assert_eq!(
            decoded.tombstone,
            Some(GroupTombstone {
                timestamp: Some(timestamp),
                context: TombstoneContext::Unknown,
            })
        );
    }
}
