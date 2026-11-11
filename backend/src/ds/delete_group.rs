// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::utils::removed_clients;
use airprotos::client::{
    app_data::GroupAppData,
    component::{AIR_COMPONENT_ID, AirComponent, GroupTombstone, TombstoneContext},
};
use apqmls::extension::APQMLS_COMPONENT_ID;
use chrono::{TimeDelta, Utc};
use mimi_room_policy::RoleIndex;
use mls_assist::{
    group::{
        ApqProcessedAssistedMessagePlus, ProcessedAssistedMessage,
        apq::{ApqGroupRef, ApqRetainedWelcomeInfo},
    },
    messages::{ApqAssistedMessageIn, AssistedMessageIn, SerializedMlsMessage},
    openmls::{
        group::GroupContext,
        prelude::{
            AppDataDictionaryExtension, Extension, Extensions, LeafNodeIndex, ProcessedMessage,
            ProcessedMessageContent, Sender,
        },
    },
    provider_traits::MlsAssistProvider,
};
use tracing::warn;

use crate::errors::GroupDeletionError;

use super::group_state::DsGroupState;

/// What a delete commit is for, beyond what every delete commit must do.
#[derive(Debug, Clone, Copy)]
pub(crate) enum DeleteKind {
    /// A member deletes the group.
    Member,
    /// The member at `joiner` closes a connection group it joined in the same
    /// DS call, see [`DsGroupState::close_connection_group`]. It must set an
    /// `AlreadyConnected` tombstone.
    Close { joiner: LeafNodeIndex },
}

impl DeleteKind {
    fn check(self, delete: &ValidatedDelete) -> Result<(), GroupDeletionError> {
        let DeleteKind::Close { joiner } = self else {
            return Ok(());
        };
        if delete.sender_index != joiner {
            warn!("Connection group close by another member than the joiner");
            return Err(GroupDeletionError::InvalidMessage);
        }
        if delete.tombstone_context != Some(TombstoneContext::AlreadyConnected) {
            warn!(
                context = ?delete.tombstone_context,
                "Connection group close without an already connected tombstone"
            );
            return Err(GroupDeletionError::InvalidMessage);
        }
        Ok(())
    }
}

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
        kind: DeleteKind,
    ) -> Result<SerializedMlsMessage, GroupDeletionError> {
        // Process message (but don't apply it yet). This performs mls-assist-level validations.
        let processed_assisted_message_plus = self
            .group()
            .process_assisted_message(self.provider.crypto(), commit)?;

        // Perform DS-level validation
        // Make sure that we have the right message type.
        let ProcessedAssistedMessage::Commit(processed_message, _group_info) =
            &processed_assisted_message_plus.processed_assisted_message
        else {
            // This should be a commit.
            warn!("Received non-commit message for delete_group operation");
            return Err(GroupDeletionError::InvalidMessage);
        };

        let delete =
            self.validate_delete_commit(processed_message, MembershipCheck::MemberProfiles)?;
        kind.check(&delete)?;
        let ValidatedDelete {
            sender_index,
            removed_clients,
            tombstone_context: _,
        } = delete;
        self.remove_from_room_state(sender_index, &removed_clients)?;

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
    ///
    /// `kind` is checked against the T leg, which holds the tombstone.
    pub(crate) fn delete_apq_group(
        t_group_state: &mut Self,
        pq_group_state: &mut Self,
        commit: ApqAssistedMessageIn,
        kind: DeleteKind,
    ) -> Result<SerializedMlsMessage, GroupDeletionError> {
        // Process both legs as a unit (but don't apply them). This performs the
        // mls-assist- and APQ-level validations.
        let ApqProcessedAssistedMessagePlus {
            processed_assisted_message,
            t_serialized_message,
            pq_serialized_message,
        } = ApqGroupRef::from_groups(&mut t_group_state.group, &mut pq_group_state.group)
            .process_apq_assisted_message(t_group_state.provider.crypto(), commit, |_, _| true)?;

        // Perform DS-level validation on each leg against its own source of
        // truth for the group's membership.
        let apq_processed_message = &processed_assisted_message.processed_message;
        let t_delete = t_group_state.validate_delete_commit(
            &apq_processed_message.t_message,
            MembershipCheck::MemberProfiles,
        )?;
        kind.check(&t_delete)?;
        pq_group_state.validate_delete_commit(
            &apq_processed_message.pq_message,
            MembershipCheck::RatchetTree,
        )?;
        let ValidatedDelete {
            sender_index,
            removed_clients: t_removed_clients,
            tombstone_context: _,
        } = t_delete;
        // The room state is kept on the T leg only.
        t_group_state.remove_from_room_state(sender_index, &t_removed_clients)?;

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
    /// the group except its sender, keeps the sender's credential and changes
    /// nothing in the group context but the tombstone.
    fn validate_delete_commit(
        &self,
        processed_message: &ProcessedMessage,
        membership_check: MembershipCheck,
    ) -> Result<ValidatedDelete, GroupDeletionError> {
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
        // A deleting client shouldn't change its credential.
        if let Some(leaf_node) = staged_commit.update_path_leaf_node() {
            let current_credential = self
                .group()
                .leaf(*sender_index)
                .map(|leaf| leaf.credential());
            if current_credential != Some(leaf_node.credential()) {
                warn!("Delete group commit changes the sender's credential");
                return Err(GroupDeletionError::InvalidMessage);
            }
        }
        // The DS keeps the merged state, so a delete commit may set the
        // tombstone and nothing else in the group context.
        let current_extensions = self.group().group_info().group_context().extensions();
        let staged_extensions = staged_commit.group_context().extensions();
        if extensions_besides_tombstone(staged_extensions)
            != extensions_besides_tombstone(current_extensions)
        {
            warn!("Delete group commit changes the group context beyond the tombstone");
            return Err(GroupDeletionError::InvalidMessage);
        }
        // Legacy delete commits set no tombstone.
        let tombstone_context = match GroupAppData::tombstone(staged_extensions) {
            None => None,
            Some(GroupTombstone {
                timestamp: Some(timestamp),
                context: context @ (TombstoneContext::Deleted | TombstoneContext::AlreadyConnected),
            }) if (*timestamp.as_ref() - Utc::now()).abs() <= TOMBSTONE_TIMESTAMP_TOLERANCE => {
                Some(context)
            }
            Some(tombstone) => {
                warn!(?tombstone, "Delete group commit sets an invalid tombstone");
                return Err(GroupDeletionError::InvalidMessage);
            }
        };
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

        Ok(ValidatedDelete {
            sender_index: *sender_index,
            removed_clients,
            tombstone_context,
        })
    }

    /// Removes the users of `removed_clients` from the room state, just like on
    /// the sender side.
    fn remove_from_room_state(
        &mut self,
        sender_index: LeafNodeIndex,
        removed_clients: &[LeafNodeIndex],
    ) -> Result<(), GroupDeletionError> {
        let sender_identity = self
            .leaf_credential(sender_index)
            .ok_or(GroupDeletionError::InvalidMessage)?
            .room_policy_identity();
        for &removed_index in removed_clients {
            let removed_identity = self
                .leaf_credential(removed_index)
                .ok_or(GroupDeletionError::InvalidMessage)?
                .room_policy_identity();
            if removed_identity == sender_identity {
                continue;
            }
            self.room_state_change_role(&sender_identity, &removed_identity, RoleIndex::Outsider)
                .ok_or(GroupDeletionError::InvalidMessage)?;
        }
        Ok(())
    }
}

/// How far a tombstone's timestamp, the deleter's local time, may be from the
/// DS's time.
const TOMBSTONE_TIMESTAMP_TOLERANCE: TimeDelta = TimeDelta::days(1);

struct ValidatedDelete {
    sender_index: LeafNodeIndex,
    removed_clients: Vec<LeafNodeIndex>,
    tombstone_context: Option<TombstoneContext>,
}

/// The group context extensions with the tombstone cleared and the APQ info
/// left out, since every APQ commit updates the latter. Sorted by type, since
/// an app data update can move the dictionary within the list.
fn extensions_besides_tombstone(extensions: &Extensions<GroupContext>) -> Vec<Extension> {
    let mut extensions: Vec<_> = extensions
        .iter()
        .map(|extension| {
            let Extension::AppDataDictionary(dictionary_extension) = extension else {
                return extension.clone();
            };
            let mut dictionary = dictionary_extension.dictionary().clone();
            dictionary.remove(&APQMLS_COMPONENT_ID);
            let component = dictionary
                .get(&AIR_COMPONENT_ID)
                .and_then(|bytes| AirComponent::from_bytes(bytes).ok());
            if let Some(mut component) = component {
                component.tombstone = None;
                if let Ok(bytes) = component.to_bytes() {
                    dictionary.insert(AIR_COMPONENT_ID, bytes);
                }
            }
            Extension::AppDataDictionary(AppDataDictionaryExtension::new(dictionary))
        })
        .collect();
    extensions.sort_by_key(Extension::extension_type);
    extensions
}

#[cfg(test)]
mod test {
    use aircommon::{
        credentials::{
            LeafCredential, UserCredentialCsr, UserCredentialPayload, VerifiableUserCredential,
            keys::{AsIntermediateSignature, ClientVerifyingKey},
        },
        crypto::{aead::keys::EncryptedUserProfileKey, hash::Hash},
        identifiers::{QsReference, SealedClientReference, UserId},
        messages::{
            client_as::EncryptedFriendshipPackage,
            client_ds::{
                AadMessage, AadPayload, CloseConnectionGroupParamsAad, GroupOperationParams,
                GroupOperationParamsAad, JoinConnectionGroupParamsAad,
            },
        },
        mls_group_config::{default_leaf_node_capabilities, default_mls_group_join_config},
        time::TimeStamp,
    };
    use airprotos::client::component::{AIR_COMPONENT_ID, AirComponent};
    use apqmls::{
        ApqCiphersuite, ApqMlsGroup,
        authentication::{
            ApqCredentialWithKey, ApqSignatureKeyPair, ApqSignatureScheme, ApqSigner,
        },
        commit_builder::ApqCommitMessageBundle,
        external_commit_builder::ApqExternalCommitBuilder,
        messages::{ApqKeyPackage, ApqMlsMessageOut, ApqRatchetTreeIn, VerifiableApqGroupInfo},
    };
    use mimi_room_policy::{RoomPolicy, VerifiedRoomState};
    use mls_assist::{
        group::Group,
        messages::AssistedMessageOut,
        openmls::{
            component::ComponentData,
            group::{ExternalCommitBuilder, GroupContext},
            prelude::{
                AppDataUpdateProposal, Ciphersuite, Credential, CredentialWithKey, Extensions,
                HpkeCiphertext, KeyPackage, LeafNodeIndex, LeafNodeParameters, MlsGroup,
                MlsMessageBodyIn, MlsMessageIn, MlsMessageOut, OpenMlsProvider,
                PURE_PLAINTEXT_WIRE_FORMAT_POLICY, Proposal, SignatureScheme,
                group_info::VerifiableGroupInfo,
            },
        },
        openmls_rust_crypto::OpenMlsRustCrypto,
        openmls_traits::signatures::Signer,
    };
    use tls_codec::{Deserialize, DeserializeBytes, Serialize};

    use crate::{
        ds::{
            close_connection_group::CloseConnectionGroupError,
            group_operation::ProcessedGroupOperation,
            group_state::MemberProfile,
            grpc::ensure_no_tombstone,
            join_connection_group::{ConnectionGroupJoin, JoinConnectionGroupError},
            process::Provider,
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
        user_id: UserId,
    }

    impl Client {
        fn new() -> Self {
            Self::of(UserId::random("example.com".parse().unwrap()))
        }

        /// Another client of the same user.
        fn sibling(&self) -> Self {
            Self::of(self.user_id.clone())
        }

        fn of(user_id: UserId) -> Self {
            let signer =
                ApqSignatureKeyPair::new(ApqSignatureScheme::from(apq_ciphersuite())).unwrap();
            let credential = ApqCredentialWithKey::new(&user_identity(&user_id, &signer), &signer);
            Self {
                provider: OpenMlsRustCrypto::default(),
                signer,
                credential,
                user_id,
            }
        }
    }

    /// The credential certifies the T leg key, as the DS checks it against the
    /// leaf signature key.
    fn user_identity(user_id: &UserId, signer: &ApqSignatureKeyPair) -> Vec<u8> {
        let (mut csr, _prelim_key) =
            UserCredentialCsr::new(user_id.clone(), SignatureScheme::ED25519).unwrap();
        csr.verifying_key = ClientVerifyingKey::from_bytes(signer.t_signer().public().to_vec());
        let payload = UserCredentialPayload::new(csr, None, Hash::from_bytes([0; 32]));
        // Unsigned, since the DS only parses the credential.
        VerifiableUserCredential::new(payload, AsIntermediateSignature::empty())
            .tls_serialize_detached()
            .unwrap()
    }

    fn room_identity(credential: &Credential) -> Option<Vec<u8>> {
        LeafCredential::from_credential(credential)
            .ok()?
            .room_policy_identity()
            .to_bytes()
            .ok()
    }

    fn room_users(state: &DsGroupState) -> Vec<Vec<u8>> {
        state.room_state.users().keys().cloned().collect()
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
        let group_info = verifiable_group_info(
            group
                .export_group_info(creator.provider.crypto(), signer, false)
                .unwrap(),
        );
        let provider = Provider::default();
        let ds_group =
            Group::new(&provider, group_info, group.export_ratchet_tree().into()).unwrap();
        let creator_identity = group
            .own_leaf_node()
            .and_then(|leaf| room_identity(leaf.credential()))
            .unwrap_or_else(|| b"creator".to_vec());
        let room_state =
            VerifiedRoomState::new(creator_identity, RoomPolicy::default_trusted_private())
                .unwrap();
        DsGroupState::new(
            provider,
            ds_group,
            EncryptedUserProfileKey::dummy(),
            qs_reference(),
            room_state,
        )
    }

    fn verifiable_group_info(message: MlsMessageOut) -> VerifiableGroupInfo {
        let bytes = message.tls_serialize_detached().unwrap();
        let MlsMessageBodyIn::GroupInfo(group_info) = MlsMessageIn::tls_deserialize_exact(bytes)
            .unwrap()
            .extract()
        else {
            panic!("expected a group info");
        };
        group_info
    }

    /// Adds the member profiles and room roles the DS records when the
    /// creator adds members.
    fn add_joiner_profiles(state: &mut DsGroupState) {
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

    fn set_already_connected(component: &mut AirComponent) {
        component.tombstone = Some(GroupTombstone::already_connected(TimeStamp::now()));
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

    fn set_tombstone_two_days_ago(component: &mut AirComponent) {
        let timestamp = Utc::now() - TimeDelta::days(2);
        component.tombstone = Some(GroupTombstone::deleted(timestamp.into()));
    }

    fn set_tombstone_in_two_days(component: &mut AirComponent) {
        let timestamp = Utc::now() + TimeDelta::days(2);
        component.tombstone = Some(GroupTombstone::deleted(timestamp.into()));
    }

    fn set_tombstone_almost_a_day_ago(component: &mut AirComponent) {
        let timestamp = Utc::now() - TimeDelta::hours(23);
        component.tombstone = Some(GroupTombstone::deleted(timestamp.into()));
    }

    fn set_deleted_and_change_features(component: &mut AirComponent) {
        set_deleted(component);
        component.features.pq_groups = !component.features.pq_groups;
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

    fn apq_assisted(bundle: ApqCommitMessageBundle) -> ApqAssistedMessageIn {
        let (t_commit, pq_commit) = bundle.commit.split();
        let (t_group_info, pq_group_info) =
            ApqMlsMessageOut::from(bundle.group_info.unwrap()).split();
        ApqAssistedMessageIn {
            t_message: assisted(t_commit, t_group_info),
            pq_message: assisted(pq_commit, pq_group_info),
        }
    }

    /// Deletes a group of `size` members as its creator and returns the DS
    /// group state afterwards.
    fn delete_t_group(
        size: usize,
        update: Option<ComponentUpdate>,
    ) -> Result<DsGroupState, GroupDeletionError> {
        let joiners = (1..size).map(|_| Client::new()).collect();
        delete_t_group_with(Client::new(), joiners, update)
    }

    /// A T group of `creator` and `joiners`, and its DS state.
    fn t_group_with(creator: &Client, joiners: Vec<Client>) -> (MlsGroup, DsGroupState) {
        let mut group = new_t_group(creator);
        let key_packages: Vec<_> = joiners
            .into_iter()
            .map(|joiner| {
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
            t_commit(creator, &mut group, key_packages, Vec::new(), None);
            group.merge_pending_commit(&creator.provider).unwrap();
        }

        let mut state = ds_group_state(creator, &group, creator.signer.t_signer());
        add_joiner_profiles(&mut state);
        (group, state)
    }

    fn delete_t_group_with(
        creator: Client,
        joiners: Vec<Client>,
        update: Option<ComponentUpdate>,
    ) -> Result<DsGroupState, GroupDeletionError> {
        let (mut group, mut state) = t_group_with(&creator, joiners);
        let removed = others(&group);
        let (commit, group_info) = t_commit(&creator, &mut group, Vec::new(), removed, update);
        state.delete_group(assisted(commit, group_info), DeleteKind::Member)?;
        Ok(state)
    }

    fn new_apq_group(creator: &Client) -> ApqMlsGroup {
        ApqMlsGroup::builder()
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
            .unwrap()
    }

    /// The APQ counterpart of [`delete_t_group`]. Returns the T and the PQ
    /// leg state.
    fn delete_apq_group(
        size: usize,
        update: Option<ComponentUpdate>,
    ) -> Result<(DsGroupState, DsGroupState), GroupDeletionError> {
        let creator = Client::new();
        let mut group = new_apq_group(&creator);
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
        DsGroupState::delete_apq_group(
            &mut t_state,
            &mut pq_state,
            apq_assisted(bundle),
            DeleteKind::Member,
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
    fn a_delete_that_changes_the_group_context_beyond_the_tombstone_is_rejected() {
        let update = Some(set_deleted_and_change_features as ComponentUpdate);
        let Err(GroupDeletionError::InvalidMessage) = delete_t_group(2, update) else {
            panic!("the T delete is accepted");
        };
        let Err(GroupDeletionError::InvalidMessage) = delete_apq_group(2, update) else {
            panic!("the APQ delete is accepted");
        };
    }

    #[test]
    fn a_delete_that_changes_the_senders_credential_is_rejected() {
        let creator = Client::new();
        let mut group = new_t_group(&creator);
        let mut state = ds_group_state(&creator, &group, creator.signer.t_signer());

        let other_user = UserId::random("example.com".parse().unwrap());
        let credential = ApqCredentialWithKey::new(
            &user_identity(&other_user, &creator.signer),
            &creator.signer,
        )
        .t_credential;
        let (commit, _welcome, group_info) = group
            .commit_builder()
            .force_self_update(true)
            .leaf_node_parameters(
                LeafNodeParameters::builder()
                    .with_credential_with_key(credential)
                    .build(),
            )
            .load_psks(creator.provider.storage())
            .unwrap()
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

        let Err(GroupDeletionError::InvalidMessage) = state.delete_group(
            assisted(commit, group_info.unwrap().into()),
            DeleteKind::Member,
        ) else {
            panic!("the delete is accepted");
        };
    }

    #[test]
    fn a_merged_delete_removes_the_others_from_the_room_state() {
        for update in [None, Some(set_deleted as ComponentUpdate)] {
            let state = delete_t_group(3, update).unwrap();
            let deleter = room_identity(&state.group().members().next().unwrap().credential);
            assert_eq!(room_users(&state), Vec::from_iter(deleter));

            let (t_state, _pq_state) = delete_apq_group(3, update).unwrap();
            let deleter = room_identity(&t_state.group().members().next().unwrap().credential);
            assert_eq!(room_users(&t_state), Vec::from_iter(deleter));
        }
    }

    #[test]
    fn a_delete_that_removes_the_deleters_other_devices_keeps_the_deleter_in_the_room() {
        let creator = Client::new();
        let joiners = vec![creator.sibling(), Client::new()];
        let state = delete_t_group_with(creator, joiners, Some(set_deleted)).unwrap();

        assert_eq!(state.group().members().count(), 1);
        let deleter = room_identity(&state.group().members().next().unwrap().credential);
        assert_eq!(room_users(&state), Vec::from_iter(deleter));
    }

    #[test]
    fn a_delete_that_sets_an_invalid_tombstone_is_rejected() {
        for update in [
            set_tombstone_with_unknown_context as ComponentUpdate,
            set_tombstone_without_timestamp,
            set_tombstone_two_days_ago,
            set_tombstone_in_two_days,
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
    fn a_delete_within_the_timestamp_tolerance_is_accepted() {
        let update = Some(set_tombstone_almost_a_day_ago as ComponentUpdate);
        assert!(delete_t_group(2, update).unwrap().has_tombstone());
        let (t_state, _pq_state) = delete_apq_group(2, update).unwrap();
        assert!(t_state.has_tombstone());
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

    #[test]
    fn a_delete_with_an_already_connected_tombstone_is_accepted() {
        let update = Some(set_already_connected as ComponentUpdate);
        assert!(delete_t_group(2, update).unwrap().has_tombstone());
        let (t_state, _pq_state) = delete_apq_group(2, update).unwrap();
        assert!(t_state.has_tombstone());
    }

    fn close_aad() -> Vec<u8> {
        AadMessage::from(AadPayload::CloseConnectionGroup(
            CloseConnectionGroupParamsAad {
                encrypted_user_profile_key: EncryptedUserProfileKey::dummy(),
            },
        ))
        .tls_serialize_detached()
        .unwrap()
    }

    fn join_aad() -> Vec<u8> {
        AadMessage::from(AadPayload::JoinConnectionGroup(
            JoinConnectionGroupParamsAad {
                encrypted_friendship_package: EncryptedFriendshipPackage::dummy(),
                encrypted_user_profile_key: EncryptedUserProfileKey::dummy(),
            },
        ))
        .tls_serialize_detached()
        .unwrap()
    }

    fn joiner_leaf_parameters() -> LeafNodeParameters {
        LeafNodeParameters::builder()
            .with_capabilities(default_leaf_node_capabilities())
            .build()
    }

    /// A connection group as its inviter created it, with `extra_members`
    /// more members than the inviter, and its DS state.
    fn t_connection_group(extra_members: usize) -> (Client, MlsGroup, DsGroupState) {
        let inviter = Client::new();
        let members = (0..extra_members).map(|_| Client::new()).collect();
        let (group, state) = t_group_with(&inviter, members);
        (inviter, group, state)
    }

    /// The external commit of `joiner` into the inviter's `group`, with its
    /// group info, and the joiner's group after it.
    fn t_external_commit(
        joiner: &Client,
        inviter: &Client,
        group: &MlsGroup,
        aad: Vec<u8>,
    ) -> (MlsGroup, MlsMessageOut, MlsMessageOut) {
        let group_info = group
            .export_group_info(inviter.provider.crypto(), inviter.signer.t_signer(), false)
            .unwrap();
        let (joined, bundle) = ExternalCommitBuilder::new()
            .with_aad(aad)
            .with_config(default_mls_group_join_config())
            .with_ratchet_tree(group.export_ratchet_tree().into())
            .build_group(
                &joiner.provider,
                verifiable_group_info(group_info),
                joiner.credential.t_credential.clone(),
            )
            .unwrap()
            .leaf_node_parameters(joiner_leaf_parameters())
            .load_psks(joiner.provider.storage())
            .unwrap()
            .create_group_info(true)
            .build(
                joiner.provider.rand(),
                joiner.provider.crypto(),
                joiner.signer.t_signer(),
                |_| true,
            )
            .unwrap()
            .finalize(&joiner.provider)
            .unwrap();
        let (commit, _welcome, group_info) = bundle.into_contents();
        (joined, commit, group_info.unwrap().into())
    }

    /// Joins a connection group of `extra_members + 1` members with `aad` and
    /// deletes it with `update` in one close, and returns the DS state
    /// afterwards.
    fn close_t_group(
        extra_members: usize,
        aad: Vec<u8>,
        update: ComponentUpdate,
    ) -> Result<DsGroupState, CloseConnectionGroupError> {
        let (inviter, group, mut state) = t_connection_group(extra_members);
        let joiner = Client::new();
        let (mut joined, join, join_group_info) = t_external_commit(&joiner, &inviter, &group, aad);
        let removed = others(&joined);
        let (delete, delete_group_info) =
            t_commit(&joiner, &mut joined, Vec::new(), removed, Some(update));
        state.close_connection_group(
            assisted(join, join_group_info),
            assisted(delete, delete_group_info),
            qs_reference(),
        )?;
        Ok(state)
    }

    /// Signs both legs with the T key, as clients do. The DS binds the legs of
    /// a joiner by its signature key.
    struct SharedKey<'a>(&'a ApqSignatureKeyPair);

    impl ApqSigner for SharedKey<'_> {
        type TSigner = <ApqSignatureKeyPair as ApqSigner>::TSigner;
        type PqSigner = <ApqSignatureKeyPair as ApqSigner>::TSigner;

        fn t_signer(&self) -> &Self::TSigner {
            self.0.t_signer()
        }

        fn pq_signer(&self) -> &Self::PqSigner {
            self.0.t_signer()
        }
    }

    /// The APQ counterpart of [`close_t_group`] for a group of the inviter
    /// only. Returns the T and the PQ leg state.
    fn close_apq_group(
        aad: Vec<u8>,
        update: ComponentUpdate,
    ) -> Result<(DsGroupState, DsGroupState), CloseConnectionGroupError> {
        let inviter = Client::new();
        let group = new_apq_group(&inviter);
        let mut t_state = ds_group_state(&inviter, &group.t_group, inviter.signer.t_signer());
        let mut pq_state = ds_group_state(&inviter, group.pq_group(), inviter.signer.pq_signer());

        let joiner = Client::new();
        let signer = SharedKey(&joiner.signer);
        let credential = ApqCredentialWithKey {
            pq_credential: CredentialWithKey {
                signature_key: joiner.credential.t_credential.signature_key.clone(),
                ..joiner.credential.pq_credential.clone()
            },
            ..joiner.credential.clone()
        };
        let (t_group_info, pq_group_info) = group
            .export_group_info(inviter.provider.crypto(), &inviter.signer, false)
            .unwrap()
            .split();
        let group_info = VerifiableApqGroupInfo::new(
            verifiable_group_info(t_group_info),
            verifiable_group_info(pq_group_info),
        );
        let ratchet_tree = ApqRatchetTreeIn::new(
            group.t_group.export_ratchet_tree().into(),
            group.pq_group().export_ratchet_tree().into(),
        );
        let (mut joined, join) = ApqExternalCommitBuilder::new()
            .with_ratchet_tree(ratchet_tree)
            .with_aad(aad)
            .with_config(default_mls_group_join_config())
            .leaf_node_parameters(joiner_leaf_parameters(), joiner_leaf_parameters())
            .create_group_info(true)
            .build(&joiner.provider, &signer, credential, group_info, |_, _| {
                true
            })
            .unwrap();

        let component_data = updated_component_data(joined.t_group.extensions(), update);
        let removed = others(&joined.t_group);
        let delete = joined
            .commit_builder()
            .force_self_update(true)
            .propose_removals(removed)
            .create_group_info(true)
            .update_t_app_data(component_data)
            .finalize(&joiner.provider, &signer, |_| true, |_| true)
            .unwrap();

        DsGroupState::apq_close_connection_group(
            &mut t_state,
            &mut pq_state,
            apq_assisted(join),
            apq_assisted(delete),
            qs_reference(),
        )?;
        Ok((t_state, pq_state))
    }

    fn tombstone_context(state: &DsGroupState) -> Option<TombstoneContext> {
        GroupAppData::tombstone(state.group().group_info().group_context().extensions())
            .map(|tombstone| tombstone.context)
    }

    #[test]
    fn a_close_leaves_the_joiner_alone_behind_an_already_connected_tombstone() {
        let state = close_t_group(0, close_aad(), set_already_connected).unwrap();
        assert_eq!(state.group().members().count(), 1);
        assert_eq!(state.member_profiles.len(), 1);
        assert!(!state.is_marked_for_deletion());
        assert_eq!(
            tombstone_context(&state),
            Some(TombstoneContext::AlreadyConnected)
        );
        let closer = room_identity(&state.group().members().next().unwrap().credential);
        assert_eq!(room_users(&state), Vec::from_iter(closer));

        let (t_state, pq_state) = close_apq_group(close_aad(), set_already_connected).unwrap();
        assert_eq!(t_state.group().members().count(), 1);
        assert_eq!(t_state.member_profiles.len(), 1);
        assert_eq!(pq_state.group().members().count(), 1);
        assert_eq!(
            tombstone_context(&t_state),
            Some(TombstoneContext::AlreadyConnected)
        );
        assert!(!pq_state.has_tombstone());
    }

    #[test]
    fn a_closed_group_takes_no_further_close() {
        let state = close_t_group(0, close_aad(), set_already_connected).unwrap();
        let status = ensure_no_tombstone(&state).unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[test]
    fn a_close_with_a_deleted_tombstone_is_rejected() {
        let Err(CloseConnectionGroupError::Delete(GroupDeletionError::InvalidMessage)) =
            close_t_group(0, close_aad(), set_deleted)
        else {
            panic!("the T close is accepted");
        };
        let Err(CloseConnectionGroupError::Delete(GroupDeletionError::InvalidMessage)) =
            close_apq_group(close_aad(), set_deleted)
        else {
            panic!("the APQ close is accepted");
        };
    }

    #[test]
    fn a_close_with_the_join_aad_is_rejected() {
        let Err(CloseConnectionGroupError::Join(JoinConnectionGroupError::InvalidMessage(_))) =
            close_t_group(0, join_aad(), set_already_connected)
        else {
            panic!("the T close is accepted");
        };
        let Err(CloseConnectionGroupError::Join(JoinConnectionGroupError::InvalidMessage(_))) =
            close_apq_group(join_aad(), set_already_connected)
        else {
            panic!("the APQ close is accepted");
        };
    }

    #[test]
    fn a_join_with_the_close_aad_is_rejected() {
        let (inviter, group, mut state) = t_connection_group(0);
        let (_joined, join_message, group_info) =
            t_external_commit(&Client::new(), &inviter, &group, close_aad());
        let kind = ConnectionGroupJoin::Accept;
        let Err(JoinConnectionGroupError::InvalidMessage(_)) =
            state.join_connection_group(assisted(join_message, group_info), qs_reference(), kind)
        else {
            panic!("the join is accepted");
        };
    }

    #[test]
    fn a_close_of_a_group_with_more_than_one_member_is_rejected() {
        let Err(CloseConnectionGroupError::Join(JoinConnectionGroupError::NotAConnectionGroup)) =
            close_t_group(1, close_aad(), set_already_connected)
        else {
            panic!("the close is accepted");
        };
    }

    #[test]
    fn a_close_by_another_member_than_the_joiner_is_rejected() {
        let (inviter, mut group, mut state) = t_connection_group(0);
        let (_joined, join, join_group_info) =
            t_external_commit(&Client::new(), &inviter, &group, close_aad());

        // The inviter merges the join and deletes the group itself.
        let join_in =
            MlsMessageIn::tls_deserialize_exact(join.tls_serialize_detached().unwrap()).unwrap();
        let processed = group
            .process_message(
                &inviter.provider,
                join_in.try_into_protocol_message().unwrap(),
            )
            .unwrap();
        let ProcessedMessageContent::StagedCommitMessage(staged_commit) = processed.into_content()
        else {
            panic!("expected a commit");
        };
        group
            .merge_staged_commit(&inviter.provider, *staged_commit)
            .unwrap();
        let removed = others(&group);
        let (delete, delete_group_info) = t_commit(
            &inviter,
            &mut group,
            Vec::new(),
            removed,
            Some(set_already_connected),
        );

        let Err(CloseConnectionGroupError::Delete(GroupDeletionError::InvalidMessage)) = state
            .close_connection_group(
                assisted(join, join_group_info),
                assisted(delete, delete_group_info),
                qs_reference(),
            )
        else {
            panic!("the close is accepted");
        };
    }
}
