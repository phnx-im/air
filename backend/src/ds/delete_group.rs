// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::utils::removed_clients;
use mls_assist::{
    group::{ApqProcessedAssistedMessagePlus, ProcessedAssistedMessage, apq::ApqGroupRef},
    messages::{AssistedMessageIn, SerializedMlsMessage},
    openmls::prelude::{ProcessedMessage, ProcessedMessageContent, Sender},
    provider_traits::MlsAssistProvider,
};
use tracing::warn;

use crate::errors::GroupDeletionError;

use super::group_state::DsGroupState;

/// Source of truth for the group's membership when validating that a
/// delete-group commit removes all other members.
enum MembershipCheck {
    /// The DS member profiles. Used for regular (T) groups.
    MemberProfiles,
    /// The MLS ratchet tree. Used for PQ groups, whose member profiles are
    /// not maintained.
    RatchetTree,
}

impl DsGroupState {
    pub(crate) fn delete_group(
        &mut self,
        commit: AssistedMessageIn,
    ) -> Result<SerializedMlsMessage, GroupDeletionError> {
        // Process message (but don't apply it yet). This performs mls-assist-level validations.
        let processed_assisted_message_plus = self
            .group()
            .process_assisted_message(self.provider.crypto(), commit)
            .map_err(|_| GroupDeletionError::ProcessingError)?;

        // Perform DS-level validation
        // Make sure that we have the right message type.
        let ProcessedAssistedMessage::Commit(processed_message, _group_info) =
            &processed_assisted_message_plus.processed_assisted_message
        else {
            // This should be a commit.
            warn!("Received non-commit message for delete_group operation");
            return Err(GroupDeletionError::InvalidMessage);
        };

        self.validate_delete_commit(processed_message, MembershipCheck::MemberProfiles)?;

        // Nobody is left to process the commit of a group's only member, so we
        // delete the group.
        self.marked_for_deletion = self.member_profiles.len() == 1;

        Ok(processed_assisted_message_plus.serialized_mls_message)
    }

    /// Deletes an APQ group, i.e. both of its legs.
    ///
    /// The two commits go through the APQ paired path, so they have to agree on
    /// the session before either of them is looked at on its own. The removals
    /// are then validated per leg: the T leg against the DS member profiles and
    /// the PQ leg against the ratchet tree, since the DS keeps no member
    /// profiles for PQ groups.
    pub(crate) fn delete_apq_group(
        t_group_state: &mut Self,
        pq_group_state: &mut Self,
        t_message: AssistedMessageIn,
        pq_message: AssistedMessageIn,
    ) -> Result<SerializedMlsMessage, GroupDeletionError> {
        // Process both legs as a unit (but don't apply them). This performs the
        // mls-assist- and APQ-level validations.
        let ApqProcessedAssistedMessagePlus {
            processed_assisted_message,
            t_serialized_message,
            pq_serialized_message,
        } = ApqGroupRef::from_groups(&mut t_group_state.group, &mut pq_group_state.group)
            .process_apq_assisted_message(
                t_group_state.provider.crypto(),
                t_message,
                pq_message,
                |_, _| true,
            )
            .map_err(|error| {
                warn!(%error, "Failed to process APQ delete group commit");
                GroupDeletionError::ProcessingError
            })?;

        // Perform DS-level validation on each leg against its own source of
        // truth for the group's membership.
        let apq_processed_message = &processed_assisted_message.processed_message;
        t_group_state.validate_delete_commit(
            &apq_processed_message.t_message,
            MembershipCheck::MemberProfiles,
        )?;
        pq_group_state.validate_delete_commit(
            &apq_processed_message.pq_message,
            MembershipCheck::RatchetTree,
        )?;

        // The T leg of the group is the source of truth for the DS member
        // profiles, so we only mark the group for deletion if the T leg is the
        // only member left.
        let sole_member = t_group_state.member_profiles.len() == 1;
        t_group_state.marked_for_deletion = sole_member;
        pq_group_state.marked_for_deletion = sole_member;

        Ok(SerializedMlsMessage::combine_apq(
            t_serialized_message,
            pq_serialized_message,
        ))
    }

    /// Checks that the commit is a member commit that removes every member of
    /// the group except its sender.
    fn validate_delete_commit(
        &self,
        processed_message: &ProcessedMessage,
        membership_check: MembershipCheck,
    ) -> Result<(), GroupDeletionError> {
        let Sender::Member(sender_index) = processed_message.sender() else {
            // Delete group should be a regular commit
            warn!("Invalid sender");
            return Err(GroupDeletionError::InvalidMessage);
        };

        let ProcessedMessageContent::StagedCommitMessage(staged_commit) =
            processed_message.content()
        else {
            warn!("Invalid message content");
            return Err(GroupDeletionError::InvalidMessage);
        };

        // Check that the commit only contains removes.
        if staged_commit.add_proposals().count() > 0 || staged_commit.update_proposals().count() > 0
        {
            warn!("Found add or update proposals in delete group commit");
            return Err(GroupDeletionError::InvalidMessage);
        }
        // Process remove proposals, but only non-inline ones.

        // Note: The staged commit yields the remove proposals in no
        // particular order, so we compare sorted lists.
        let mut removed_clients: Vec<_> = removed_clients(staged_commit);
        removed_clients.sort_unstable();
        let existing_clients: Vec<_> = match membership_check {
            MembershipCheck::MemberProfiles => self
                .member_profiles
                .keys()
                .filter(|index| index != &sender_index)
                .copied()
                .collect(),
            MembershipCheck::RatchetTree => self
                .group()
                .members()
                .map(|member| member.index)
                .filter(|index| index != sender_index)
                .collect(),
        };
        // Check that we're indeed removing all the clients.
        if removed_clients != existing_clients {
            warn!(
                ?removed_clients,
                ?existing_clients,
                "Incomplete remove proposals in delete group commit"
            );
            return Err(GroupDeletionError::InvalidMessage);
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use aircommon::{
        crypto::aead::keys::EncryptedUserProfileKey,
        identifiers::{QsReference, SealedClientReference},
        mls_group_config::default_leaf_node_capabilities,
        time::TimeStamp,
    };
    use airprotos::client::{app_data::GroupAppData, component::AIR_COMPONENT_ID};
    use apqmls::{
        ApqCiphersuite, ApqMlsGroup,
        authentication::{
            ApqCredentialWithKey, ApqSignatureKeyPair, ApqSignatureScheme, ApqSigner,
        },
        messages::{ApqKeyPackage, ApqMlsMessageOut},
    };
    use mimi_room_policy::{RoomPolicy, VerifiedRoomState};
    use mls_assist::{
        group::Group,
        messages::AssistedMessageOut,
        openmls::{
            component::ComponentData,
            group::GroupContext,
            prelude::{
                AppDataUpdateProposal, Ciphersuite, Extensions, HpkeCiphertext, KeyPackage,
                LeafNodeIndex, MlsGroup, MlsMessageBodyIn, MlsMessageIn, MlsMessageOut,
                OpenMlsProvider, PURE_PLAINTEXT_WIRE_FORMAT_POLICY, Proposal,
            },
        },
        openmls_rust_crypto::OpenMlsRustCrypto,
        openmls_traits::signatures::Signer,
    };
    use tls_codec::{Deserialize, DeserializeBytes, Serialize};

    use crate::ds::{group_state::MemberProfile, process::Provider};

    use super::*;

    const T_CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

    /// Both legs sign with Ed25519, since ML-DSA key generation would overflow
    /// the test stack.
    fn apq_ciphersuite() -> ApqCiphersuite {
        ApqCiphersuite::new(
            T_CIPHERSUITE,
            Ciphersuite::MLS_128_MLKEM768_AES256GCM_SHA384_Ed25519,
        )
    }

    struct Client {
        provider: OpenMlsRustCrypto,
        signer: ApqSignatureKeyPair,
        credential: ApqCredentialWithKey,
    }

    impl Client {
        fn new() -> Self {
            let signer =
                ApqSignatureKeyPair::new(ApqSignatureScheme::from(apq_ciphersuite())).unwrap();
            let credential = ApqCredentialWithKey::new(b"client", &signer);
            Self {
                provider: OpenMlsRustCrypto::default(),
                signer,
                credential,
            }
        }
    }

    fn qs_reference() -> QsReference {
        QsReference {
            client_homeserver_domain: "example.com".parse().unwrap(),
            sealed_reference: SealedClientReference::from(HpkeCiphertext {
                kem_output: vec![1, 2, 3].into(),
                ciphertext: vec![4, 5, 6].into(),
            }),
        }
    }

    /// Returns the DS state of `group` as its creation leaves it, i.e. with
    /// the creator's member profile only.
    fn ds_group_state(creator: &Client, group: &MlsGroup, signer: &impl Signer) -> DsGroupState {
        let group_info = group
            .export_group_info(creator.provider.crypto(), signer, false)
            .unwrap()
            .tls_serialize_detached()
            .unwrap();
        let MlsMessageBodyIn::GroupInfo(group_info) =
            MlsMessageIn::tls_deserialize_exact(group_info)
                .unwrap()
                .extract()
        else {
            panic!("expected a group info");
        };
        let provider = Provider::default();
        let ds_group =
            Group::new(&provider, group_info, group.export_ratchet_tree().into()).unwrap();
        let room_state =
            VerifiedRoomState::new(b"creator".to_vec(), RoomPolicy::default_trusted_private())
                .unwrap();
        DsGroupState::new(
            provider,
            ds_group,
            EncryptedUserProfileKey::dummy(),
            qs_reference(),
            room_state,
        )
    }

    /// Adds the member profiles the DS records when members join.
    fn add_joiner_profiles(state: &mut DsGroupState) {
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
        }
    }

    /// The group context of a T group as the client creates it.
    fn t_group_context_extensions() -> Extensions<GroupContext> {
        let app_data = GroupAppData {
            is_self_group: false,
            safe_aad_components: None,
            profile: None,
            deleted: None,
        };
        Extensions::from_vec(vec![app_data.to_extension().unwrap()]).unwrap()
    }

    fn new_t_group(creator: &Client) -> MlsGroup {
        MlsGroup::builder()
            .ciphersuite(T_CIPHERSUITE)
            .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
            .with_capabilities(default_leaf_node_capabilities())
            .with_group_context_extensions(t_group_context_extensions())
            .build(
                &creator.provider,
                creator.signer.t_signer(),
                creator.credential.t_credential.clone(),
            )
            .unwrap()
    }

    /// The Air component of the group context with the deleted flag set.
    fn deleted_component_data(
        extensions: &Extensions<GroupContext>,
        deleted: TimeStamp,
    ) -> ComponentData {
        let mut component = GroupAppData::air_component(extensions).unwrap();
        component.deleted = Some(deleted);
        component.to_component_data().unwrap()
    }

    fn others(group: &MlsGroup) -> Vec<LeafNodeIndex> {
        group
            .members()
            .map(|member| member.index)
            .filter(|index| *index != group.own_leaf_index())
            .collect()
    }

    /// Stages a commit of the creator and returns it with the new group info.
    /// With `deleted`, the commit sets the deleted flag like a delete commit.
    fn t_commit(
        creator: &Client,
        group: &mut MlsGroup,
        adds: Vec<KeyPackage>,
        removals: Vec<LeafNodeIndex>,
        deleted: Option<TimeStamp>,
    ) -> (MlsMessageOut, MlsMessageOut) {
        let component_data =
            deleted.map(|deleted| deleted_component_data(group.extensions(), deleted));
        let mut builder = group
            .commit_builder()
            .force_self_update(true)
            .propose_adds(adds)
            .propose_removals(removals);
        if let Some(component_data) = &component_data {
            builder = builder.add_proposal(Proposal::AppDataUpdate(Box::new(
                AppDataUpdateProposal::update(AIR_COMPONENT_ID, component_data.data().to_vec()),
            )));
        }
        let mut builder = builder.load_psks(creator.provider.storage()).unwrap();
        if let Some(component_data) = component_data {
            let mut updater = builder.app_data_dictionary_updater();
            updater.set(component_data);
            builder.with_app_data_dictionary_updates(updater.changes());
        }
        let (commit, _welcome, group_info) = builder
            .create_group_info(true)
            .build(
                creator.provider.rand(),
                creator.provider.crypto(),
                creator.signer.t_signer(),
                |_| true,
            )
            .unwrap()
            .stage_commit(&creator.provider)
            .unwrap()
            .into_contents();
        (commit, group_info.unwrap().into())
    }

    fn assisted(message: MlsMessageOut, group_info: MlsMessageOut) -> AssistedMessageIn {
        let bytes = AssistedMessageOut::new(message, Some(group_info))
            .tls_serialize_detached()
            .unwrap();
        AssistedMessageIn::tls_deserialize_exact_bytes(&bytes).unwrap()
    }

    /// Deletes a group of `size` members as its creator and returns whether
    /// the DS marks the group state for deletion.
    fn delete_t_group(size: usize, deleted: Option<TimeStamp>) -> bool {
        let creator = Client::new();
        let mut group = new_t_group(&creator);
        let key_packages: Vec<_> = (1..size)
            .map(|_| {
                let joiner = Client::new();
                KeyPackage::builder()
                    .leaf_node_capabilities(default_leaf_node_capabilities())
                    .build(
                        T_CIPHERSUITE,
                        &joiner.provider,
                        joiner.signer.t_signer(),
                        joiner.credential.t_credential,
                    )
                    .unwrap()
                    .key_package()
                    .clone()
            })
            .collect();
        if !key_packages.is_empty() {
            t_commit(&creator, &mut group, key_packages, Vec::new(), None);
            group.merge_pending_commit(&creator.provider).unwrap();
        }

        let mut state = ds_group_state(&creator, &group, creator.signer.t_signer());
        add_joiner_profiles(&mut state);

        let removed = others(&group);
        let (commit, group_info) = t_commit(&creator, &mut group, Vec::new(), removed, deleted);
        state.delete_group(assisted(commit, group_info)).unwrap();

        state.is_marked_for_deletion()
    }

    /// The APQ counterpart of [`delete_t_group`]. Returns the marker of the T
    /// and the PQ leg state.
    fn delete_apq_group(size: usize, deleted: Option<TimeStamp>) -> (bool, bool) {
        let creator = Client::new();
        let mut group = ApqMlsGroup::builder()
            .with_ciphersuite(apq_ciphersuite())
            .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
            .with_capabilities(default_leaf_node_capabilities())
            .with_group_context_extensions(t_group_context_extensions(), Extensions::empty())
            .unwrap()
            .build(
                &creator.provider,
                &creator.signer,
                creator.credential.clone(),
            )
            .unwrap();
        let key_packages: Vec<_> = (1..size)
            .map(|_| {
                let joiner = Client::new();
                ApqKeyPackage::builder()
                    .leaf_node_capabilities(default_leaf_node_capabilities())
                    .build(
                        &joiner.provider,
                        apq_ciphersuite(),
                        &joiner.signer,
                        joiner.credential,
                    )
                    .unwrap()
                    .into_key_package()
            })
            .collect();
        if !key_packages.is_empty() {
            group
                .commit_builder()
                .propose_adds(key_packages)
                .finalize(&creator.provider, &creator.signer, |_| true, |_| true)
                .unwrap();
            group.merge_pending_commit(&creator.provider).unwrap();
        }

        // The DS records joiners on the T leg only.
        let mut t_state = ds_group_state(&creator, &group.t_group, creator.signer.t_signer());
        add_joiner_profiles(&mut t_state);
        let mut pq_state = ds_group_state(&creator, group.pq_group(), creator.signer.pq_signer());

        let removed = others(&group.t_group);
        let component_data =
            deleted.map(|deleted| deleted_component_data(group.t_group.extensions(), deleted));
        let mut builder = group
            .commit_builder()
            .force_self_update(true)
            .propose_removals(removed)
            .create_group_info(true);
        if let Some(component_data) = component_data {
            builder = builder.update_t_app_data(component_data);
        }
        let bundle = builder
            .finalize(&creator.provider, &creator.signer, |_| true, |_| true)
            .unwrap();
        let (t_commit, pq_commit) = bundle.commit.split();
        let (t_group_info, pq_group_info) =
            ApqMlsMessageOut::from(bundle.group_info.unwrap()).split();
        DsGroupState::delete_apq_group(
            &mut t_state,
            &mut pq_state,
            assisted(t_commit, t_group_info),
            assisted(pq_commit, pq_group_info),
        )
        .unwrap();

        (
            t_state.is_marked_for_deletion(),
            pq_state.is_marked_for_deletion(),
        )
    }

    /// Stages a self-update of the creator and returns whether the DS sees
    /// the group flags unchanged by it. With `deleted`, the update sets the
    /// deleted flag.
    fn group_flags_unchanged_by_update(deleted: Option<TimeStamp>) -> bool {
        let creator = Client::new();
        let mut group = new_t_group(&creator);
        let state = ds_group_state(&creator, &group, creator.signer.t_signer());

        let (commit, group_info) = t_commit(&creator, &mut group, Vec::new(), Vec::new(), deleted);
        let processed = state
            .group()
            .process_assisted_message(state.provider.crypto(), assisted(commit, group_info))
            .unwrap();
        let ProcessedAssistedMessage::Commit(processed_message, _) =
            &processed.processed_assisted_message
        else {
            panic!("expected a commit");
        };
        let ProcessedMessageContent::StagedCommitMessage(staged_commit) =
            processed_message.content()
        else {
            panic!("expected a staged commit");
        };
        state.group_flags_unchanged(staged_commit)
    }

    #[test]
    fn a_delete_by_the_only_member_deletes_the_group_state() {
        for deleted in [None, Some(TimeStamp::now())] {
            assert!(delete_t_group(1, deleted));
            assert_eq!(delete_apq_group(1, deleted), (true, true));
        }
    }

    #[test]
    fn a_delete_that_removes_other_members_keeps_the_group_state() {
        for deleted in [None, Some(TimeStamp::now())] {
            assert!(!delete_t_group(2, deleted));
            assert_eq!(delete_apq_group(2, deleted), (false, false));
        }
    }

    #[test]
    fn a_regular_commit_that_sets_the_deleted_flag_changes_the_group_flags() {
        assert!(group_flags_unchanged_by_update(None));
        assert!(!group_flags_unchanged_by_update(Some(TimeStamp::now())));
    }
}
