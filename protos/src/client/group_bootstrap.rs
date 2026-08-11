// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Wire format for handing a newly created or externally joined group to the
//! sibling clients of a virtual client.
//!
//! When one emulator client creates a group or joins one via external commit,
//! its siblings need the group's keys and, for connection groups, the contact
//! context the acting client persisted. Both travel in a
//! [`GroupBootstrapBlob`]: at creation as an opaque parameter of the
//! create-group request which the DS echoes to the sibling queues, at an
//! external join as an `AppEphemeral` proposal inside the commit.
//!
//! The [`GroupBootstrap`] payload is padded-AEAD-encrypted under a
//! [`GroupBootstrapKey`] derived from the emulation epoch that also derives the
//! acting client's leaf. Only siblings hold that epoch state, so the AEAD both
//! hides the payload from the DS and authenticates the blob as
//! sibling-authored. The epoch id travels in the clear, it tells a receiving
//! sibling which registered epoch to derive the key from. The padding hides the
//! variable length of the connection context.
//!
//! Every struct here is a tagged map and every enum a tagged union with an
//! `#[unknown]` catch-all, so a client can adopt new tags before all of a
//! user's devices understand them. A tagged map defaults absent fields, which
//! means fields that are required by this protocol are `Option` and a receiver
//! rejects a payload that leaves them out.
//!
//! Keys and secrets travel as plain byte strings rather than as their Rust
//! types, so that the encoding does not depend on the `serde` shape of a type
//! defined elsewhere. The receiver converts them into the typed keys and
//! rejects values of the wrong size. The one exception is
//! [`EncryptedGroupBootstrap`], whose encoding the `AppEphemeral` payloads
//! already ship:
//!
//! ```cddl
//! Ciphertext = { "ciphertext": bstr, "nonce": bstr .size 12 }
//! ```

use aircommon::{
    crypto::aead::{
        Ciphertext, PaddedAeadDecryptable, PaddedAeadEncryptable, keys::GroupBootstrapKey,
    },
    identifiers::{Fqdn, FqdnError, UserId},
    messages::{FriendshipToken, client_as::ConnectionOfferHash},
};
use airmacros::{
    DeserializeTaggedMap, DeserializeTaggedUnion, SerializeTaggedMap, SerializeTaggedUnion,
};
use uuid::Uuid;

/// Marker for the ciphertext of [`GroupBootstrap`].
#[derive(Debug)]
pub struct GroupBootstrapCtype;

/// Padded-AEAD ciphertext of a [`GroupBootstrap`] payload.
pub type EncryptedGroupBootstrap = Ciphertext<GroupBootstrapCtype>;

/// Cleartext wrapper around an [`EncryptedGroupBootstrap`].
///
/// This is what travels on the wire: as the `group_bootstrap` parameter of a
/// create-group request and back out of the DS in a `GroupCreationEcho`, or as
/// the payload of an `AppEphemeral` proposal in an external commit.
///
/// ## CDDL Definition
///
/// ```cddl
/// GroupBootstrapBlob = {
///   epoch_id: bstr .tag 1,
///   ? encrypted_bootstrap: Ciphertext .tag 2,
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct GroupBootstrapBlob {
    /// Id of the virtual client's emulation epoch that derives the decryption
    /// key.
    ///
    /// In the clear so a receiving sibling knows which of its registered
    /// emulation epochs to use.
    #[tag(1)]
    pub epoch_id: Vec<u8>,
    #[tag(2)]
    pub encrypted_bootstrap: Option<EncryptedGroupBootstrap>,
}

/// Plaintext of an [`EncryptedGroupBootstrap`].
///
/// Everything a sibling needs to install the group beyond the MLS material it
/// fetches from the DS epoch snapshot.
///
/// ## CDDL Definition
///
/// ```cddl
/// GroupBootstrap = {
///   group_id: bstr .tag 1,
///   ? pq_group_id: bstr .tag 2,
///   ? group_state_ear_key: bstr .size 32 .tag 3,
///   ? identity_link_wrapper_key: bstr .size 32 .tag 4,
///   ? connection: ConnectionContext .tag 5,
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct GroupBootstrap {
    /// Group id of the group, the T leg for APQ groups.
    ///
    /// The receiver checks this id, and [`Self::pq_group_id`], against the
    /// `GroupInfo` it fetches. That binds the keys below to the group.
    #[tag(1)]
    pub group_id: Vec<u8>,
    /// Group id of the PQ leg, present iff the group is an APQ group.
    #[tag(2)]
    pub pq_group_id: Option<Vec<u8>>,
    /// Raw `GroupStateEarKey`, 32 bytes.
    #[tag(3)]
    pub group_state_ear_key: Option<Vec<u8>>,
    /// Raw `IdentityLinkWrapperKey`, 32 bytes.
    #[tag(4)]
    pub identity_link_wrapper_key: Option<Vec<u8>>,
    /// Absent for group chats. Their chat attributes come from the group-data
    /// extension in the `GroupInfo` instead.
    #[tag(5)]
    pub connection: Option<ConnectionContext>,
}

impl PaddedAeadEncryptable<GroupBootstrapKey, GroupBootstrapCtype> for GroupBootstrap {}
impl PaddedAeadDecryptable<GroupBootstrapKey, GroupBootstrapCtype> for GroupBootstrap {}

/// Context a sibling needs to mirror the contact rows of a connection chat.
///
/// The variant records how the acting client got into the connection group,
/// which decides what the sibling has to persist.
///
/// ## CDDL Definition
///
/// ```cddl
/// ConnectionContext = {
///   1: HandleInitiatorContext //
///   2: TargetedInitiatorContext //
///   3: AcceptContext
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, SerializeTaggedUnion, DeserializeTaggedUnion)]
pub enum ConnectionContext {
    #[tag(1)]
    HandleInitiator(HandleInitiatorContext),
    #[tag(2)]
    TargetedInitiator(TargetedInitiatorContext),
    #[tag(3)]
    Accept(AcceptContext),
    /// A context kind this client does not understand. Ignored on receive.
    #[unknown]
    Unknown,
}

/// The acting client initiated a connection via a user handle.
///
/// ## CDDL Definition
///
/// ```cddl
/// HandleInitiatorContext = {
///   username: tstr .tag 1,
///   ? friendship_package_ear_key: bstr .size 32 .tag 2,
///   ? connection_offer_hash: bstr .size 32 .tag 3,
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct HandleInitiatorContext {
    /// The handle the offer went to. Validated into a `Username` on receive.
    #[tag(1)]
    pub username: String,
    /// Raw `FriendshipPackageEarKey`, 32 bytes.
    #[tag(2)]
    pub friendship_package_ear_key: Option<Vec<u8>>,
    /// Id and value of the connection-offer PSK.
    #[tag(3)]
    pub connection_offer_hash: Option<ConnectionOfferHash>,
}

/// The acting client initiated a connection via a targeted message.
///
/// ## CDDL Definition
///
/// ```cddl
/// TargetedInitiatorContext = {
///   ? user_id: PeerUserId .tag 1,
///   ? friendship_package_ear_key: bstr .size 32 .tag 2,
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct TargetedInitiatorContext {
    /// The user the targeted message went to.
    #[tag(1)]
    pub user_id: Option<PeerUserId>,
    /// Raw `FriendshipPackageEarKey`, 32 bytes.
    #[tag(2)]
    pub friendship_package_ear_key: Option<Vec<u8>>,
}

/// The acting client accepted a connection request by externally joining the
/// peer's connection group.
///
/// [`Self::friendship_token`], [`Self::wai_ear_key`] and
/// [`Self::user_profile_base_secret`] are the contents of the peer's
/// `FriendshipPackage`, which only the device that received the offer has.
/// [`Self::connection_offer_hash`] is both the id and the value of the
/// connection-offer PSK, so the sibling needs it to re-validate the PSK
/// proposal in the external commit it processes.
///
/// ## CDDL Definition
///
/// ```cddl
/// AcceptContext = {
///   ? user_id: PeerUserId .tag 1,
///   ? friendship_token: bstr .tag 2,
///   ? wai_ear_key: bstr .size 32 .tag 3,
///   ? user_profile_base_secret: bstr .size 32 .tag 4,
///   ? connection_offer_hash: bstr .size 32 .tag 5,
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct AcceptContext {
    /// The connection peer, i.e. the sender of the connection offer.
    #[tag(1)]
    pub user_id: Option<PeerUserId>,
    #[tag(2)]
    pub friendship_token: Option<FriendshipToken>,
    /// Raw `WelcomeAttributionInfoEarKey`, 32 bytes.
    #[tag(3)]
    pub wai_ear_key: Option<Vec<u8>>,
    /// Raw `UserProfileBaseSecret`, 32 bytes.
    #[tag(4)]
    pub user_profile_base_secret: Option<Vec<u8>>,
    #[tag(5)]
    pub connection_offer_hash: Option<ConnectionOfferHash>,
}

/// A user id, encoded with primitive CBOR shapes.
///
/// ## CDDL Definition
///
/// ```cddl
/// PeerUserId = {
///   uuid: bstr .size 16 .tag 1,
///   domain: tstr .tag 2,
/// }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct PeerUserId {
    #[tag(1)]
    pub uuid: Uuid,
    #[tag(2)]
    pub domain: String,
}

impl From<UserId> for PeerUserId {
    fn from(user_id: UserId) -> Self {
        let (uuid, domain) = user_id.into_parts();
        Self {
            uuid,
            domain: domain.to_string(),
        }
    }
}

impl TryFrom<PeerUserId> for UserId {
    type Error = FqdnError;

    fn try_from(peer: PeerUserId) -> Result<Self, Self::Error> {
        Ok(UserId::new(peer.uuid, peer.domain.parse::<Fqdn>()?))
    }
}

#[cfg(test)]
mod test {
    use aircommon::{
        codec::PersistenceCodec,
        crypto::{
            aead::AEAD_KEY_SIZE,
            kdf::{KdfDerivable, keys::SelfGroupExporterSecret},
        },
    };

    use super::*;

    fn bootstrap_key_from(secret_bytes: [u8; 32]) -> GroupBootstrapKey {
        let exporter = SelfGroupExporterSecret::from_bytes(secret_bytes);
        GroupBootstrapKey::derive(&exporter, &Vec::new()).unwrap()
    }

    fn key(byte: u8) -> Vec<u8> {
        vec![byte; AEAD_KEY_SIZE]
    }

    fn user_id() -> UserId {
        UserId::new(
            Uuid::from_u128(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10),
            "example.com".parse::<Fqdn>().unwrap(),
        )
    }

    fn sample_bootstrap() -> GroupBootstrap {
        GroupBootstrap {
            group_id: b"t-group-id".to_vec(),
            pq_group_id: Some(b"pq-group-id".to_vec()),
            group_state_ear_key: Some(key(1)),
            identity_link_wrapper_key: Some(key(2)),
            connection: Some(ConnectionContext::Accept(AcceptContext {
                user_id: Some(user_id().into()),
                friendship_token: Some(FriendshipToken::from_bytes(vec![3; 32])),
                wai_ear_key: Some(key(4)),
                user_profile_base_secret: Some(key(5)),
                connection_offer_hash: Some(ConnectionOfferHash::from_bytes([6u8; 32])),
            })),
        }
    }

    #[test]
    fn group_bootstrap_roundtrip() {
        let bootstrap = sample_bootstrap();
        let bytes = PersistenceCodec::to_vec(&bootstrap).unwrap();
        let decoded: GroupBootstrap = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(bootstrap, decoded);
    }

    #[test]
    fn group_bootstrap_stability() {
        let bytes = PersistenceCodec::to_vec(&sample_bootstrap()).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    #[test]
    fn group_bootstrap_of_group_chat_omits_absent_fields() {
        let bootstrap = GroupBootstrap {
            group_id: b"t-group-id".to_vec(),
            pq_group_id: None,
            group_state_ear_key: Some(key(1)),
            identity_link_wrapper_key: Some(key(2)),
            connection: None,
        };
        let bytes = PersistenceCodec::to_vec(&bootstrap).unwrap();
        // map(3): only the group id and the two keys are encoded.
        assert_eq!(bytes[1], 0xA3);
        let decoded: GroupBootstrap = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(bootstrap, decoded);
    }

    #[test]
    fn group_bootstrap_encrypt_decrypt_roundtrip() {
        let key = bootstrap_key_from([7u8; 32]);
        let bootstrap = sample_bootstrap();
        let encrypted = bootstrap.encrypt_padded(&key).unwrap();
        let decrypted = GroupBootstrap::decrypt_padded(&key, &encrypted).unwrap();
        assert_eq!(bootstrap, decrypted);

        let other_key = bootstrap_key_from([8u8; 32]);
        assert!(GroupBootstrap::decrypt_padded(&other_key, &encrypted).is_err());
    }

    #[test]
    fn group_bootstrap_blob_roundtrip() {
        let key = bootstrap_key_from([9u8; 32]);
        let blob = GroupBootstrapBlob {
            epoch_id: b"emulation-epoch-id".to_vec(),
            encrypted_bootstrap: Some(sample_bootstrap().encrypt_padded(&key).unwrap()),
        };
        let bytes = PersistenceCodec::to_vec(&blob).unwrap();
        let decoded: GroupBootstrapBlob = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(blob, decoded);
    }

    #[test]
    fn connection_context_variants_roundtrip() {
        let contexts = [
            ConnectionContext::HandleInitiator(HandleInitiatorContext {
                username: "alice".to_owned(),
                friendship_package_ear_key: Some(key(1)),
                connection_offer_hash: Some(ConnectionOfferHash::from_bytes([2u8; 32])),
            }),
            ConnectionContext::TargetedInitiator(TargetedInitiatorContext {
                user_id: Some(user_id().into()),
                friendship_package_ear_key: Some(key(3)),
            }),
        ];
        for context in contexts {
            let bytes = PersistenceCodec::to_vec(&context).unwrap();
            let decoded: ConnectionContext = PersistenceCodec::from_slice(&bytes).unwrap();
            assert_eq!(context, decoded);
        }
    }

    #[test]
    fn peer_user_id_roundtrip() {
        let user_id = user_id();
        let peer = PeerUserId::from(user_id.clone());
        assert_eq!(UserId::try_from(peer).unwrap(), user_id);
    }

    #[test]
    fn peer_user_id_with_invalid_domain_is_rejected() {
        let peer = PeerUserId {
            uuid: Uuid::nil(),
            domain: "not a domain".to_owned(),
        };
        assert!(UserId::try_from(peer).is_err());
    }

    #[test]
    fn connection_context_stability() {
        let context = ConnectionContext::HandleInitiator(HandleInitiatorContext {
            username: "alice".to_owned(),
            friendship_package_ear_key: Some(key(1)),
            connection_offer_hash: Some(ConnectionOfferHash::from_bytes([2u8; 32])),
        });
        let bytes = PersistenceCodec::to_vec(&context).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    /// A newer client sends a context kind this version does not know.
    #[test]
    fn connection_context_unknown_tag_decodes_to_unknown() {
        #[derive(Debug, Clone, PartialEq, SerializeTaggedUnion, DeserializeTaggedUnion)]
        enum ConnectionContextV2 {
            #[tag(4)]
            Something(u64),
            #[unknown]
            Unknown,
        }

        let newer = ConnectionContextV2::Something(42);
        let bytes = PersistenceCodec::to_vec(&newer).unwrap();
        let decoded: ConnectionContext = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(decoded, ConnectionContext::Unknown);
    }

    /// A newer client sends a tag this version does not know, and an older one
    /// leaves out a tag this version does know.
    #[test]
    fn group_bootstrap_tolerates_unknown_and_absent_tags() {
        #[derive(Debug, Clone, SerializeTaggedMap)]
        struct GroupBootstrapV2 {
            #[tag(1)]
            group_id: Vec<u8>,
            #[tag(3)]
            group_state_ear_key: Option<Vec<u8>>,
            #[tag(6)]
            something_new: Option<u64>,
        }

        let newer = GroupBootstrapV2 {
            group_id: b"t-group-id".to_vec(),
            group_state_ear_key: Some(key(1)),
            something_new: Some(7),
        };
        let bytes = PersistenceCodec::to_vec(&newer).unwrap();
        let decoded: GroupBootstrap = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(decoded.group_id, b"t-group-id".to_vec());
        assert_eq!(decoded.group_state_ear_key, Some(key(1)));
        assert_eq!(decoded.identity_link_wrapper_key, None);
        assert_eq!(decoded.connection, None);
    }
}
