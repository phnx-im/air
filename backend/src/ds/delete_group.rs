// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::utils::removed_clients;
use airprotos::client::{
    app_data::GroupAppData,
    component::{GroupTombstone, TombstoneContext},
};
use mls_assist::{
    group::{
        ApqProcessedAssistedMessagePlus, ProcessedAssistedMessage,
        apq::{ApqGroupRef, ApqRetainedWelcomeInfo},
    },
    messages::{AssistedMessageIn, SerializedMlsMessage},
    openmls::prelude::{LeafNodeIndex, ProcessedMessage, ProcessedMessageContent, Sender},
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
    /// Deletes a group.
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

        let removed_clients =
            self.validate_delete_commit(processed_message, MembershipCheck::MemberProfiles)?;

        // Nobody is left to process the commit of a group's only member, so we
        // delete the group.
        self.marked_for_deletion = self.member_profiles.len() == 1;

        let retained_welcome_info = self.group.accept_processed_message(
            self.provider.storage(),
            processed_assisted_message_plus.processed_assisted_message,
        )?;
        self.remove_profiles(removed_clients);
        self.stage_welcome_info(retained_welcome_info);

        #[cfg(debug_assertions)]
        self.check_member_profiles("delete");

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
        let t_removed_clients = t_group_state.validate_delete_commit(
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

        let ApqRetainedWelcomeInfo {
            t_retained_welcome_info,
            pq_retained_welcome_info,
        } = ApqGroupRef::from_groups(&mut t_group_state.group, &mut pq_group_state.group)
            .accept_apq_processed_message(
                t_group_state.provider.storage(),
                pq_group_state.provider.storage(),
                processed_assisted_message,
            )?;
        t_group_state.remove_profiles(t_removed_clients);
        t_group_state.stage_welcome_info(t_retained_welcome_info);
        pq_group_state.stage_welcome_info_without_profile_keys(pq_retained_welcome_info);

        #[cfg(debug_assertions)]
        t_group_state.check_member_profiles("apq_delete");

        Ok(SerializedMlsMessage::combine_apq(
            t_serialized_message,
            pq_serialized_message,
        ))
    }

    /// Checks that the commit is a member commit that removes every member of
    /// the group except its sender. Returns the removed leaves.
    fn validate_delete_commit(
        &self,
        processed_message: &ProcessedMessage,
        membership_check: MembershipCheck,
    ) -> Result<Vec<LeafNodeIndex>, GroupDeletionError> {
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
        // A delete commit may set the tombstone, but the self-group flag is
        // fixed at group creation.
        let current_extensions = self.group().group_info().group_context().extensions();
        let staged_extensions = staged_commit.group_context().extensions();
        if GroupAppData::is_self_group_context(staged_extensions)
            != GroupAppData::is_self_group_context(current_extensions)
        {
            warn!("Delete group commit would toggle the self-group flag");
            return Err(GroupDeletionError::InvalidMessage);
        }
        // Legacy delete commits set no tombstone.
        match GroupAppData::tombstone(staged_extensions) {
            None
            | Some(GroupTombstone {
                timestamp: Some(_),
                context: TombstoneContext::Deleted,
            }) => {}
            Some(tombstone) => {
                warn!(?tombstone, "Delete group commit sets an invalid tombstone");
                return Err(GroupDeletionError::InvalidMessage);
            }
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

        Ok(removed_clients)
    }
}

#[cfg(test)]
mod test {
    use aircommon::{
        credentials::{
            UserCredentialCsr, UserCredentialPayload, VerifiableUserCredential,
            keys::AsIntermediateSignature,
        },
        crypto::{aead::keys::EncryptedUserProfileKey, hash::Hash},
        identifiers::{QsReference, SealedClientReference, UserId},
        messages::client_ds::{
            AadMessage, AadPayload, GroupOperationParams, GroupOperationParamsAad,
        },
        mls_group_config::default_leaf_node_capabilities,
        time::TimeStamp,
    };
    use airprotos::client::component::{AIR_COMPONENT_ID, AirComponent};
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
                OpenMlsProvider, PURE_PLAINTEXT_WIRE_FORMAT_POLICY, Proposal, SignatureScheme,
            },
        },
        openmls_rust_crypto::OpenMlsRustCrypto,
        openmls_traits::signatures::Signer,
    };
    use tls_codec::{Deserialize, DeserializeBytes, Serialize};

    use crate::{
        ds::{
            group_operation::ProcessedGroupOperation, group_state::MemberProfile, process::Provider,
        },
        errors::GroupOperationError,
    };

    use super::*;
    use crate::ds::create_group::ensure_created_without_tombstone;

    const MAX_DEVICES: u32 = 5;
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
            let user_id = UserId::random("example.com".parse().unwrap());
            let (csr, _prelim_key) =
                UserCredentialCsr::new(user_id, SignatureScheme::ED25519).unwrap();
            let payload = UserCredentialPayload::new(csr, None, Hash::from_bytes([0; 32]));
            // Unsigned, since the DS only parses the credential.
            let identity = VerifiableUserCredential::new(payload, AsIntermediateSignature::empty())
                .tls_serialize_detached()
                .unwrap();
            let credential = ApqCredentialWithKey::new(&identity, &signer);
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
        t_group_context_extensions_with(None)
    }

    fn t_group_context_extensions_with(
        tombstone: Option<GroupTombstone>,
    ) -> Extensions<GroupContext> {
        let app_data = GroupAppData {
            is_self_group: false,
            safe_aad_components: None,
            profile: None,
            tombstone,
        };
        Extensions::from_vec(vec![app_data.to_extension().unwrap()]).unwrap()
    }

    fn new_t_group(creator: &Client) -> MlsGroup {
        new_t_group_with(creator, t_group_context_extensions())
    }

    fn new_t_group_with(creator: &Client, extensions: Extensions<GroupContext>) -> MlsGroup {
        MlsGroup::builder()
            .ciphersuite(T_CIPHERSUITE)
            .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
            .with_capabilities(default_leaf_node_capabilities())
            .with_group_context_extensions(extensions)
            .build(
                &creator.provider,
                creator.signer.t_signer(),
                creator.credential.t_credential.clone(),
            )
            .unwrap()
    }

    /// A change a commit makes to the Air component of the group context.
    type ComponentUpdate = fn(&mut AirComponent);

    fn set_deleted(component: &mut AirComponent) {
        component.tombstone = Some(GroupTombstone::deleted(TimeStamp::now()));
    }

    fn set_tombstone_with_unknown_context(component: &mut AirComponent) {
        component.tombstone = Some(GroupTombstone {
            timestamp: Some(TimeStamp::now()),
            context: TombstoneContext::Unknown,
        });
    }

    fn set_tombstone_without_timestamp(component: &mut AirComponent) {
        component.tombstone = Some(GroupTombstone {
            timestamp: None,
            context: TombstoneContext::Deleted,
        });
    }

    fn set_deleted_and_self_group(component: &mut AirComponent) {
        set_deleted(component);
        component.is_self_group = true;
    }

    fn updated_component_data(
        extensions: &Extensions<GroupContext>,
        update: ComponentUpdate,
    ) -> ComponentData {
        let mut component = GroupAppData::air_component(extensions).unwrap();
        update(&mut component);
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
    /// With `update`, the commit changes the Air component like a delete
    /// commit.
    fn t_commit(
        creator: &Client,
        group: &mut MlsGroup,
        adds: Vec<KeyPackage>,
        removals: Vec<LeafNodeIndex>,
        update: Option<ComponentUpdate>,
    ) -> (MlsMessageOut, MlsMessageOut) {
        let component_data =
            update.map(|update| updated_component_data(group.extensions(), update));
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

    /// Deletes a group of `size` members as its creator and returns the DS
    /// group state afterwards.
    fn delete_t_group(
        size: usize,
        update: Option<ComponentUpdate>,
    ) -> Result<DsGroupState, GroupDeletionError> {
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
        let (commit, group_info) = t_commit(&creator, &mut group, Vec::new(), removed, update);
        state.delete_group(assisted(commit, group_info))?;
        Ok(state)
    }

    /// The APQ counterpart of [`delete_t_group`]. Returns the T and the PQ
    /// leg state.
    fn delete_apq_group(
        size: usize,
        update: Option<ComponentUpdate>,
    ) -> Result<(DsGroupState, DsGroupState), GroupDeletionError> {
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
            update.map(|update| updated_component_data(group.t_group.extensions(), update));
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
        )?;
        Ok((t_state, pq_state))
    }

    /// Submits a self-update of the creator as a group operation. With
    /// `update`, the self-update changes the Air component like a delete
    /// commit.
    async fn self_update_group_operation(
        update: Option<ComponentUpdate>,
    ) -> Result<ProcessedGroupOperation, GroupOperationError> {
        let creator = Client::new();
        let mut group = new_t_group(&creator);
        let mut state = ds_group_state(&creator, &group, creator.signer.t_signer());

        let aad = AadMessage::from(AadPayload::GroupOperation(GroupOperationParamsAad {
            new_encrypted_user_profile_keys: Vec::new(),
        }));
        group.set_aad(aad.tls_serialize_detached().unwrap());
        let (commit, group_info) = t_commit(&creator, &mut group, Vec::new(), Vec::new(), update);
        let params = GroupOperationParams {
            commit: assisted(commit, group_info),
            add_users_info_option: None,
        };
        state.process_group_operation(params, MAX_DEVICES).await
    }

    #[test]
    fn a_delete_by_the_only_member_deletes_the_group_state() {
        for update in [None, Some(set_deleted as ComponentUpdate)] {
            assert!(delete_t_group(1, update).unwrap().is_marked_for_deletion());
            let (t_state, pq_state) = delete_apq_group(1, update).unwrap();
            assert!(t_state.is_marked_for_deletion());
            assert!(pq_state.is_marked_for_deletion());
        }
    }

    #[test]
    fn a_delete_that_removes_other_members_keeps_the_group_state() {
        for update in [None, Some(set_deleted as ComponentUpdate)] {
            assert!(!delete_t_group(2, update).unwrap().is_marked_for_deletion());
            let (t_state, pq_state) = delete_apq_group(2, update).unwrap();
            assert!(!t_state.is_marked_for_deletion());
            assert!(!pq_state.is_marked_for_deletion());
        }
    }

    #[test]
    fn a_merged_delete_leaves_only_the_deleter_and_carries_the_tombstone_it_sets() {
        for update in [None, Some(set_deleted as ComponentUpdate)] {
            let state = delete_t_group(3, update).unwrap();
            assert_eq!(state.group().members().count(), 1);
            assert_eq!(state.member_profiles.len(), 1);
            assert_eq!(state.has_tombstone(), update.is_some());

            let (t_state, pq_state) = delete_apq_group(3, update).unwrap();
            assert_eq!(t_state.group().members().count(), 1);
            assert_eq!(t_state.member_profiles.len(), 1);
            assert_eq!(pq_state.group().members().count(), 1);
            assert_eq!(t_state.has_tombstone(), update.is_some());
            assert!(!pq_state.has_tombstone());
        }
    }

    #[test]
    fn a_delete_that_sets_the_self_group_flag_is_rejected() {
        let update = Some(set_deleted_and_self_group as ComponentUpdate);
        let Err(GroupDeletionError::InvalidMessage) = delete_t_group(2, update) else {
            panic!("the T delete is accepted");
        };
        let Err(GroupDeletionError::InvalidMessage) = delete_apq_group(2, update) else {
            panic!("the APQ delete is accepted");
        };
    }

    #[test]
    fn a_delete_that_sets_an_invalid_tombstone_is_rejected() {
        for update in [
            set_tombstone_with_unknown_context as ComponentUpdate,
            set_tombstone_without_timestamp,
        ] {
            let Err(GroupDeletionError::InvalidMessage) = delete_t_group(2, Some(update)) else {
                panic!("the T delete is accepted");
            };
            let Err(GroupDeletionError::InvalidMessage) = delete_apq_group(2, Some(update)) else {
                panic!("the APQ delete is accepted");
            };
        }
    }

    #[test]
    fn a_group_created_with_a_tombstone_is_rejected() {
        let creator = Client::new();
        let group = new_t_group(&creator);
        let state = ds_group_state(&creator, &group, creator.signer.t_signer());
        ensure_created_without_tombstone(state.group()).unwrap();

        for tombstone in [
            GroupTombstone::deleted(TimeStamp::now()),
            GroupTombstone {
                timestamp: None,
                context: TombstoneContext::Deleted,
            },
            GroupTombstone {
                timestamp: Some(TimeStamp::now()),
                context: TombstoneContext::Unknown,
            },
        ] {
            let extensions = t_group_context_extensions_with(Some(tombstone));
            let group = new_t_group_with(&creator, extensions);
            let state = ds_group_state(&creator, &group, creator.signer.t_signer());
            assert!(ensure_created_without_tombstone(state.group()).is_err());
        }
    }

    #[tokio::test]
    async fn a_group_operation_that_sets_a_tombstone_is_rejected() {
        self_update_group_operation(None).await.unwrap();
        let Err(GroupOperationError::InvalidMessage) =
            self_update_group_operation(Some(set_deleted)).await
        else {
            panic!("the group operation is accepted");
        };
    }
}
