// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Helpers for DS tests that build MLS groups and feed their messages to the DS, and AS
//! connectors that authenticate the credentials entering them.

use std::{collections::HashMap, io};

use aircommon::{
    credentials::{
        LeafCredential, VerifiableUserCredential,
        keys::{ClientVerifyingKey, UserSigningKey},
        test_utils::other_key_of,
    },
    crypto::aead::keys::EncryptedUserProfileKey,
    identifiers::{QsReference, SealedClientReference, UserId},
    messages::client_ds::{AadMessage, AadPayload, GroupOperationParamsAad},
    mls_group_config::QS_CLIENT_REFERENCE_EXTENSION_TYPE,
    time::TimeStamp,
};
use airprotos::client::{app_data::GroupAppData, component::GroupTombstone};
use apqmls::{commit_builder::ApqCommitMessageBundle, messages::ApqMlsMessageOut};
use mimi_room_policy::{RoleIndex, RoomPolicy, VerifiedRoomState};
use mls_assist::{
    group::Group,
    messages::{ApqAssistedMessageIn, AssistedMessageIn, AssistedMessageOut},
    openmls::{
        group::GroupContext,
        prelude::{
            Credential, Extension, Extensions, HpkeCiphertext, LeafNodeIndex, MlsGroup,
            MlsMessageBodyIn, MlsMessageIn, MlsMessageOut, OpenMlsProvider, UnknownExtension,
            group_info::VerifiableGroupInfo,
        },
    },
    openmls_traits::signatures::Signer,
};
use tls_codec::{Deserialize, DeserializeBytes, Serialize};
use tonic::{Code, Status};

use crate::auth_service::AsConnector;

use super::{
    group_state::{DsGroupState, MemberProfile},
    process::Provider,
};

pub(super) fn qs_reference() -> QsReference {
    QsReference {
        client_homeserver_domain: "example.com".parse().unwrap(),
        sealed_reference: SealedClientReference::from(HpkeCiphertext {
            kem_output: vec![1, 2, 3].into(),
            ciphertext: vec![4, 5, 6].into(),
        }),
    }
}

/// The leaf node extension that carries [`qs_reference`].
pub(super) fn qs_reference_extension() -> Extension {
    Extension::Unknown(
        QS_CLIENT_REFERENCE_EXTENSION_TYPE,
        UnknownExtension(qs_reference().tls_serialize_detached().unwrap()),
    )
}

/// The group context extensions of a group as the client creates it.
pub(super) fn group_context_extensions(
    is_self_group: bool,
    tombstone: Option<GroupTombstone>,
) -> Extensions<GroupContext> {
    let app_data = GroupAppData {
        is_self_group,
        safe_aad_components: None,
        profile: None,
        tombstone,
    };
    Extensions::from_vec(vec![app_data.to_extension().unwrap()]).unwrap()
}

pub(super) fn verifiable_group_info(message: MlsMessageOut) -> VerifiableGroupInfo {
    let bytes = message.tls_serialize_detached().unwrap();
    let MlsMessageBodyIn::GroupInfo(group_info) = MlsMessageIn::tls_deserialize_exact(bytes)
        .unwrap()
        .extract()
    else {
        panic!("expected a group info");
    };
    group_info
}

pub(super) fn apq_assisted(bundle: ApqCommitMessageBundle) -> ApqAssistedMessageIn {
    let (t_commit, pq_commit) = bundle.commit.split();
    let (t_group_info, pq_group_info) = ApqMlsMessageOut::from(bundle.group_info.unwrap()).split();
    ApqAssistedMessageIn {
        t_message: assisted(t_commit, t_group_info),
        pq_message: assisted(pq_commit, pq_group_info),
    }
}

pub(super) fn assisted(message: MlsMessageOut, group_info: MlsMessageOut) -> AssistedMessageIn {
    let bytes = AssistedMessageOut::new(message, Some(group_info))
        .tls_serialize_detached()
        .unwrap();
    AssistedMessageIn::tls_deserialize_exact_bytes(&bytes).unwrap()
}

/// The AAD of a group operation that adds `new_members` members.
pub(super) fn group_operation_aad(new_members: usize) -> Vec<u8> {
    AadMessage::from(AadPayload::GroupOperation(GroupOperationParamsAad {
        new_encrypted_user_profile_keys: (0..new_members)
            .map(|_| EncryptedUserProfileKey::dummy())
            .collect(),
    }))
    .tls_serialize_detached()
    .unwrap()
}

pub(super) fn room_identity(credential: &Credential) -> Option<Vec<u8>> {
    LeafCredential::from_credential(credential)
        .ok()?
        .room_policy_identity()
        .to_bytes()
        .ok()
}

/// Returns the DS state of `group` as its creation leaves it, i.e. with the creator's member
/// profile only.
///
/// `provider` and `signer` are those of the group's own leaf.
pub(super) fn ds_group_state(
    provider: &impl OpenMlsProvider,
    group: &MlsGroup,
    signer: &impl Signer,
) -> DsGroupState {
    let group_info = verifiable_group_info(
        group
            .export_group_info(provider.crypto(), signer, false)
            .unwrap(),
    );
    let ds_provider = Provider::default();
    let ds_group =
        Group::new(&ds_provider, group_info, group.export_ratchet_tree().into()).unwrap();
    let creator_identity = group
        .own_leaf_node()
        .and_then(|leaf| room_identity(leaf.credential()))
        .unwrap_or_else(|| b"creator".to_vec());
    let room_state =
        VerifiedRoomState::new(creator_identity, RoomPolicy::default_trusted_private()).unwrap();
    DsGroupState::new(
        ds_provider,
        ds_group,
        EncryptedUserProfileKey::dummy(),
        qs_reference(),
        room_state,
    )
}

/// Adds the member profiles and room roles the DS records when the creator, at leaf 0, adds
/// members.
pub(super) fn add_joiner_profiles(state: &mut DsGroupState) {
    let creator = state
        .leaf_credential(LeafNodeIndex::new(0))
        .map(|credential| credential.room_policy_identity());
    let epoch = state.group().epoch();
    let joiners: Vec<_> = state
        .group()
        .members()
        .map(|member| member.index)
        .filter(|index| !state.member_profiles.contains_key(index))
        .collect();
    for leaf_index in joiners {
        let profile = MemberProfile {
            leaf_index,
            client_queue_config: qs_reference(),
            activity_time: TimeStamp::now(),
            activity_epoch: epoch,
            encrypted_user_profile_key: EncryptedUserProfileKey::dummy(),
        };
        state.member_profiles.insert(leaf_index, profile);
        let joiner = state
            .leaf_credential(leaf_index)
            .map(|credential| credential.room_policy_identity());
        if let (Some(creator), Some(joiner)) = (&creator, joiner)
            && *creator != joiner
        {
            state
                .room_state_change_role(creator, &joiner, RoleIndex::Regular)
                .unwrap();
        }
    }
}

/// An AS that stores a verifying key per user.
#[derive(Debug, Default)]
pub(super) struct StoredKeys(HashMap<UserId, ClientVerifyingKey>);

impl StoredKeys {
    /// An AS that stores the verifying keys of `signing_keys` for their users.
    pub(super) fn of(signing_keys: &[&UserSigningKey]) -> Self {
        Self(
            signing_keys
                .iter()
                .map(|signing_key| {
                    (
                        signing_key.credential().user_id().clone(),
                        signing_key.credential().verifying_key().clone(),
                    )
                })
                .collect(),
        )
    }

    pub(super) fn register(&mut self, credential: &VerifiableUserCredential) {
        self.0.insert(
            credential.user_id().clone(),
            credential.verifying_key().clone(),
        );
    }
}

impl AsConnector for StoredKeys {
    type Error = io::Error;

    async fn client_verifying_key(
        &self,
        user_id: &UserId,
    ) -> Result<Option<ClientVerifyingKey>, Self::Error> {
        Ok(self.0.get(user_id).cloned())
    }
}

/// An AS whose every lookup fails.
#[derive(Debug)]
pub(super) struct FailingAs;

impl AsConnector for FailingAs {
    type Error = io::Error;

    async fn client_verifying_key(
        &self,
        _user_id: &UserId,
    ) -> Result<Option<ClientVerifyingKey>, Self::Error> {
        Err(io::Error::other("storage unavailable"))
    }
}

/// What the AS stores, who signs an entering credential of `alice`, and whether the AS
/// authenticates it.
pub(super) fn authentication_cases(
    alice: &UserSigningKey,
) -> [(StoredKeys, UserSigningKey, bool); 3] {
    [
        (StoredKeys::of(&[alice]), alice.clone(), true),
        // The signature doesn't verify with the stored key.
        (StoredKeys::of(&[alice]), other_key_of(alice), false),
        // The AS doesn't know the user.
        (StoredKeys::default(), alice.clone(), false),
    ]
}

/// Asserts that `result` is a success if `accepted` and a permission denied error otherwise.
#[track_caller]
pub(super) fn assert_authenticated<T>(result: Result<T, Status>, accepted: bool) {
    match result {
        Ok(_) => assert!(accepted, "expected permission denied"),
        Err(status) => {
            assert!(!accepted, "{status:?}");
            assert_eq!(status.code(), Code::PermissionDenied, "{status:?}");
        }
    }
}
