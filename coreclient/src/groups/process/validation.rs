// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

use aircommon::{
    credentials::{
        AsIntermediateCredential, AsIntermediateCredentialBody, LeafCredential, RoomPolicyIdentity,
        UserCredential, VerifiableUserCredential,
    },
    crypto::hash::Hash,
    messages::client_ds::{AadMessage, AadPayload},
    utils::removed_client,
};
use anyhow::{Context, Result, bail, ensure};
use mimi_room_policy::RoleIndex;
use openmls::prelude::{
    LeafNodeIndex, MlsGroup, ProcessedMessage, Sender, SignaturePublicKey, StagedCommit,
};
use tls_codec::DeserializeBytes as _;

use crate::{
    db::access::WriteDbTransaction,
    groups::{Group, client_auth_info::VerifiableUserCredentialExt},
};

impl Group {
    /// Process the remove proposals in a staged commit by value. Currently only
    /// removes are allowed. Returns `true` if we were removed.
    pub(super) fn process_remove_proposals(
        &self,
        staged_commit: &StagedCommit,
        sender_credential: &LeafCredential,
    ) -> Result<bool> {
        let mut we_were_removed = false;
        for queued_proposal in staged_commit.queued_proposals() {
            if matches!(queued_proposal.sender(), Sender::NewMemberCommit) {
                // This can only happen if the removed member is rejoining
                // as part of the commit. No need to process the removal.
                continue;
            }
            // Load the removed client's index.
            let Some(removed_index) = removed_client(queued_proposal) else {
                // This is not a remove proposal, so we skip it.
                continue;
            };

            // Check that the signature keys of the removed user match
            self.verify_pq_signature_key_at(removed_index)?;

            let removed_credential = self
                .unverified_credential_at(removed_index)?
                .context("Removed user credential not found")?;

            // Room policy checks
            self.verify_role_change_identity(
                &sender_credential.room_policy_identity(),
                &removed_credential.room_policy_identity(),
                RoleIndex::Outsider,
            )?;

            if removed_index == self.mls_group().own_leaf_index() {
                we_were_removed = true;
            }
        }
        Ok(we_were_removed)
    }

    /// Verify that the T and PQ leaf nodes at `index` have matching signature
    /// keys. A no-op if there is no PQ group.
    pub(super) fn verify_pq_signature_key_at(&self, index: LeafNodeIndex) -> Result<()> {
        if let Some(pq_group) = self.pq.as_ref() {
            let pq_leaf = pq_group
                .mls_group
                .public_group()
                .leaf(index)
                .context("PQ sender leaf not found")?;
            let t_leaf = self
                .mls_group
                .public_group()
                .leaf(index)
                .context("T sender leaf not found")?;
            ensure!(
                pq_leaf.signature_key() == t_leaf.signature_key(),
                "T and PQ sender signature keys do not match"
            );
        }
        Ok(())
    }

    pub(super) async fn process_adds(
        &mut self,
        sender_identity: &RoomPolicyIdentity,
        staged_commit: &StagedCommit,
        txn: &mut WriteDbTransaction<'_>,
        as_credentials: &HashMap<Hash<AsIntermediateCredentialBody>, AsIntermediateCredential>,
    ) -> Result<Vec<UserCredential>> {
        let mut credentials = Vec::new();

        for proposal in staged_commit.add_proposals() {
            let leaf_node = proposal.add_proposal().key_package().leaf_node();

            // Verify the credential
            let credential = LeafCredential::from_credential(leaf_node.credential())?
                .into_user()
                .context("expected a user credential in a regular-group add")?;
            let credential =
                credential.verify_and_validate(leaf_node.signature_key(), None, as_credentials)?;

            self.verify_role_change_identity(
                sender_identity,
                &RoomPolicyIdentity::User(credential.user_id().clone()),
                RoleIndex::Regular,
            )?;

            credential.store_checked(&mut *txn).await?;
            credentials.push(credential.into());
        }

        // TODO: Validation:
        // * Check that this commit only contains (inline) add proposals
        // * User ids MUST be unique within the group (check both new
        //   and existing credentials for duplicates).
        // * Client IDs MUST be unique within the group (only need to
        //   check new credentials, as client IDs are scoped to user
        //   names).

        Ok(credentials)
    }

    /// Process the add proposals of a self-group commit. Added leaves carry a
    /// self-group credential, which holds no AS material to verify: openmls
    /// has already checked the leaf signature, so only the credential type,
    /// client id uniqueness, and the room policy (keyed on client ids) are
    /// checked.
    pub(super) fn process_self_group_adds(
        &self,
        sender_identity: &RoomPolicyIdentity,
        staged_commit: &StagedCommit,
    ) -> Result<()> {
        for proposal in staged_commit.add_proposals() {
            let leaf_node = proposal.add_proposal().key_package().leaf_node();
            self.validate_self_group_add(leaf_node.credential())?;

            let credential = LeafCredential::from_credential(leaf_node.credential())?;
            self.verify_role_change_identity(
                sender_identity,
                &credential.room_policy_identity(),
                RoleIndex::Regular,
            )?;
        }
        Ok(())
    }
}

/// Pair the new and old leaf credential of a member transition for AS verification.
///
/// Each side is a leaf credential and the leaf's signature key. Returns `None` when both are
/// self-group credentials, since they carry no AS material to verify or store. Their client id and
/// signature key must stay the same, because a self-group credential carries no key the leaf could
/// be bound to. Mixed pairs are rejected.
pub(super) fn user_credential_transition(
    (new, new_key): (LeafCredential, &SignaturePublicKey),
    (old, old_key): (LeafCredential, &SignaturePublicKey),
    context: &str,
) -> Result<Option<(VerifiableUserCredential, VerifiableUserCredential)>> {
    match (new, old) {
        (LeafCredential::User(new), LeafCredential::User(old)) => Ok(Some((new, old))),
        (LeafCredential::SelfGroup(new), LeafCredential::SelfGroup(old)) => {
            ensure!(
                new.client_id() == old.client_id(),
                "self-group client id changed in {context}"
            );
            ensure!(
                new_key == old_key,
                "self-group signature key changed in {context}"
            );
            Ok(None)
        }
        (LeafCredential::User(_), LeafCredential::SelfGroup(_))
        | (LeafCredential::SelfGroup(_), LeafCredential::User(_)) => {
            bail!("mismatched leaf credential types in {context}")
        }
    }
}

/// Performs several checks on a commit to the self group.
pub(super) fn ensure_self_group_commit(
    group: &MlsGroup,
    processed_message: &ProcessedMessage,
    pq_group: Option<&MlsGroup>,
    pq_staged_commit: Option<&StagedCommit>,
) -> Result<()> {
    let staged_commit = super::expect_staged_commit(processed_message)?;
    ensure_member_proposals(staged_commit)?;
    if let Some(pq_staged_commit) = pq_staged_commit {
        ensure_member_proposals(pq_staged_commit)?;
    }

    let Sender::NewMemberCommit = processed_message.sender() else {
        return Ok(());
    };
    ensure_resync_aad(processed_message.tail_aad())?;
    let evicted = ensure_rejoin_on_own_leaf(group, staged_commit)?;
    if let Some(pq_group) = pq_group {
        let pq_staged_commit = pq_staged_commit.context("self-group resync without a PQ leg")?;
        let pq_evicted = ensure_rejoin_on_own_leaf(pq_group, pq_staged_commit)?;
        ensure!(
            pq_evicted == evicted,
            "self-group resync removes different leaves on its two legs"
        );
    }
    Ok(())
}

/// Fails if a proposal in `staged_commit` comes from an external sender or from
/// a new member asking to join.
fn ensure_member_proposals(staged_commit: &StagedCommit) -> Result<()> {
    for proposal in staged_commit.queued_proposals() {
        match proposal.sender() {
            Sender::Member(_) | Sender::NewMemberCommit => {}
            Sender::External(_) | Sender::NewMemberProposal => {
                bail!("self-group commit carries a proposal from a non-member")
            }
        }
    }
    Ok(())
}

/// Fails unless `aad` is the AAD of a resync.
fn ensure_resync_aad(aad: &[u8]) -> Result<()> {
    match AadMessage::tls_deserialize_exact_bytes(aad)?.into_payload() {
        AadPayload::Resync => Ok(()),
        AadPayload::GroupOperation(_)
        | AadPayload::JoinConnectionGroup(_)
        | AadPayload::CloseConnectionGroup(_)
        | AadPayload::DeleteGroup => bail!("self-group external commit is no resync"),
    }
}

/// Checks that an external commit into the self `group` carries exactly one
/// Remove proposal, and the removed leaf has the signature key of the new leaf.
/// Returns the removed leaf.
fn ensure_rejoin_on_own_leaf(
    group: &MlsGroup,
    staged_commit: &StagedCommit,
) -> Result<LeafNodeIndex> {
    let mut removed = staged_commit
        .remove_proposals()
        .map(|proposal| proposal.remove_proposal().removed());
    let (Some(evicted), None) = (removed.next(), removed.next()) else {
        bail!("self-group resync does not remove exactly one leaf");
    };
    let new_key = staged_commit
        .update_path_leaf_node()
        .context("self-group resync without a new leaf")?
        .signature_key();
    let evicted_key = group
        .public_group()
        .leaf(evicted)
        .context("self-group resync removes a blank leaf")?
        .signature_key();
    ensure!(
        new_key == evicted_key,
        "self-group resync changes the signature key of the removed leaf"
    );
    Ok(evicted)
}

/// Verify that the T update-path leaf signature key matches the PQ update-path
/// leaf signature key. A no-op if there is no PQ staged commit.
pub(super) fn verify_pq_update_path_signature_key(
    t_leaf_key: &SignaturePublicKey,
    pq_staged_commit: Option<&StagedCommit>,
) -> Result<()> {
    if let Some(pq_staged_commit) = pq_staged_commit {
        let pq_leaf_node = pq_staged_commit
            .update_path_leaf_node()
            .context("Could not find sender leaf node")?;
        ensure!(
            t_leaf_key == pq_leaf_node.signature_key(),
            "T and PQ sender signature keys do not match"
        );
    }
    Ok(())
}

/// Verify that the T and PQ add proposals have matching signature keys, and
/// that there are at least as many PQ add proposals as T add proposals. A
/// no-op if there is no PQ staged commit.
pub(super) fn verify_pq_added_signature_keys(
    staged_commit: &StagedCommit,
    pq_staged_commit: Option<&StagedCommit>,
) -> Result<()> {
    let Some(pq_staged_commit) = pq_staged_commit else {
        return Ok(());
    };
    let mut pq_add_proposals = pq_staged_commit.add_proposals();
    for ap in staged_commit.add_proposals() {
        let pq_add_proposal = pq_add_proposals
            .next()
            .context("Less PQ add proposals than T")?;
        let pq_signature_key = pq_add_proposal
            .add_proposal()
            .key_package()
            .leaf_node()
            .signature_key();
        let t_signature_key = ap.add_proposal().key_package().leaf_node().signature_key();
        ensure!(
            pq_signature_key == t_signature_key,
            "T and PQ added user signature keys do not match"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use aircommon::{
        credentials::SelfGroupCredential, messages::client_ds::GroupOperationParamsAad,
        mls_group_config::default_mls_group_join_config,
    };
    use openmls::{
        group::ExternalCommitBuilder,
        prelude::{
            BasicCredential, Credential, CredentialWithKey, JoinProposal, KeyPackage,
            MlsMessageBodyIn, MlsMessageIn, MlsMessageOut, OpenMlsProvider,
            PURE_PLAINTEXT_WIRE_FORMAT_POLICY, ProcessedMessageContent, Proposal, RemoveProposal,
            tls_codec::Serialize as _,
        },
    };
    use openmls_basic_credential::SignatureKeyPair;
    use openmls_rust_crypto::OpenMlsRustCrypto;
    use uuid::Uuid;

    use super::*;
    use crate::{clients::CIPHERSUITE, groups::process::expect_staged_commit};

    fn self_group_credential(client_id: u128) -> LeafCredential {
        LeafCredential::SelfGroup(SelfGroupCredential::new(Uuid::from_u128(client_id)))
    }

    fn signature_key(byte: u8) -> SignaturePublicKey {
        SignaturePublicKey::from(vec![byte; 32])
    }

    #[test]
    fn self_group_transition_preserves_client_id_and_key() -> anyhow::Result<()> {
        let key = signature_key(1);
        for context in ["update path", "resync"] {
            let transition = user_credential_transition(
                (self_group_credential(1), &key),
                (self_group_credential(1), &key),
                context,
            )?;

            assert!(transition.is_none(), "{context}");
        }
        Ok(())
    }

    #[test]
    fn self_group_transition_rejects_client_id_change() {
        let key = signature_key(1);
        for context in ["update path", "resync"] {
            let transition = user_credential_transition(
                (self_group_credential(2), &key),
                (self_group_credential(1), &key),
                context,
            );

            assert!(transition.is_err(), "{context}");
        }
    }

    #[test]
    fn self_group_transition_rejects_signature_key_change() {
        for context in ["update path", "resync"] {
            let transition = user_credential_transition(
                (self_group_credential(1), &signature_key(2)),
                (self_group_credential(1), &signature_key(1)),
                context,
            );

            assert!(transition.is_err(), "{context}");
        }
    }

    struct Device {
        provider: OpenMlsRustCrypto,
        signer: SignatureKeyPair,
        credential_with_key: CredentialWithKey,
    }

    impl Device {
        fn new() -> Self {
            let credential = BasicCredential::new(Uuid::new_v4().as_bytes().to_vec());
            Self::with_credential(credential.into())
        }

        /// A device with the credential of `self` and a fresh signature key.
        fn with_fresh_key(&self) -> Self {
            Self::with_credential(self.credential_with_key.credential.clone())
        }

        fn with_credential(credential: Credential) -> Self {
            let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
            let credential_with_key = CredentialWithKey {
                credential,
                signature_key: signer.public().into(),
            };
            Self {
                provider: OpenMlsRustCrypto::default(),
                signer,
                credential_with_key,
            }
        }

        fn key_package(&self) -> KeyPackage {
            KeyPackage::builder()
                .build(
                    CIPHERSUITE,
                    &self.provider,
                    &self.signer,
                    self.credential_with_key.clone(),
                )
                .unwrap()
                .into_key_package()
        }

        fn commit(&self, group: &mut MlsGroup, adds: Vec<KeyPackage>, removed: Vec<LeafNodeIndex>) {
            group
                .commit_builder()
                .force_self_update(true)
                .propose_adds(adds)
                .propose_removals(removed)
                .load_psks(self.provider.storage())
                .unwrap()
                .build(
                    self.provider.rand(),
                    self.provider.crypto(),
                    &self.signer,
                    |_| true,
                )
                .unwrap()
                .stage_commit(&self.provider)
                .unwrap();
        }
    }

    /// The group of `creator`, with `members` on the leaves after it.
    fn new_group(creator: &Device, members: Vec<KeyPackage>) -> MlsGroup {
        let mut group = MlsGroup::builder()
            .ciphersuite(CIPHERSUITE)
            .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
            .build(
                &creator.provider,
                &creator.signer,
                creator.credential_with_key.clone(),
            )
            .unwrap();
        creator.commit(&mut group, members, Vec::new());
        group.merge_pending_commit(&creator.provider).unwrap();
        group
    }

    /// The group of `creator`, with the blank leaf of a device that left,
    /// `device` at leaf 2 and a sibling at leaf 3.
    fn group_with_blank_leaf(creator: &Device, device: &Device) -> MlsGroup {
        let members = vec![
            Device::new().key_package(),
            device.key_package(),
            Device::new().key_package(),
        ];
        let mut group = new_group(creator, members);
        creator.commit(&mut group, Vec::new(), vec![LeafNodeIndex::new(1)]);
        group.merge_pending_commit(&creator.provider).unwrap();
        group
    }

    /// The external commit of `device` into the `group` of `creator`.
    ///
    /// The commit contains the AAD `payload`. Returns the commit as the creator
    /// processes it.
    fn external_commit(
        device: &Device,
        creator: &Device,
        group: &mut MlsGroup,
        payload: AadPayload,
        removed: Option<LeafNodeIndex>,
    ) -> ProcessedMessage {
        try_external_commit(device, creator, group, payload, removed).unwrap()
    }

    /// Like [`external_commit`], but returns the error of the creator if it
    /// fails to process the commit.
    fn try_external_commit(
        device: &Device,
        creator: &Device,
        group: &mut MlsGroup,
        payload: AadPayload,
        removed: Option<LeafNodeIndex>,
    ) -> Result<ProcessedMessage> {
        let group_info = group
            .export_group_info(creator.provider.crypto(), &creator.signer, false)
            .unwrap();
        let MlsMessageBodyIn::GroupInfo(group_info) = to_message_in(&group_info).extract() else {
            panic!("expected a group info");
        };
        let removals = removed.map(|index| {
            // `RemoveProposal` has no public constructor. Its encoding is the
            // leaf index.
            let proposal =
                RemoveProposal::tls_deserialize_exact_bytes(&index.u32().to_be_bytes()).unwrap();
            Proposal::Remove(Box::new(proposal))
        });
        let (_joined, bundle) = ExternalCommitBuilder::new()
            .with_aad(AadMessage::from(payload).tls_serialize_detached().unwrap())
            .with_config(default_mls_group_join_config())
            .with_ratchet_tree(group.export_ratchet_tree().into())
            .build_group(
                &device.provider,
                group_info,
                device.credential_with_key.clone(),
            )
            .unwrap()
            .add_proposals(removals)
            .load_psks(device.provider.storage())
            .unwrap()
            .build(
                device.provider.rand(),
                device.provider.crypto(),
                &device.signer,
                |_| true,
            )
            .unwrap()
            .finalize(&device.provider)
            .unwrap();
        let (commit, _welcome, _group_info) = bundle.into_contents();
        let commit = to_message_in(&commit).try_into_protocol_message().unwrap();
        Ok(group.process_message(&creator.provider, commit)?)
    }

    fn to_message_in(message: &MlsMessageOut) -> MlsMessageIn {
        MlsMessageIn::tls_deserialize_exact_bytes(&message.tls_serialize_detached().unwrap())
            .unwrap()
    }

    #[test]
    fn a_self_group_rejoin_on_the_own_leaf_is_accepted() {
        let (creator, device) = (Device::new(), Device::new());
        let mut group = group_with_blank_leaf(&creator, &device);
        let processed = external_commit(&device, &creator, &mut group, AadPayload::Resync, None);
        assert!(ensure_self_group_commit(&group, &processed, None, None).is_ok());
    }

    #[test]
    fn a_self_group_rejoin_with_a_fresh_key_is_rejected() {
        let (creator, device) = (Device::new(), Device::new());
        let mut group = group_with_blank_leaf(&creator, &device);
        let processed = external_commit(
            &device.with_fresh_key(),
            &creator,
            &mut group,
            AadPayload::Resync,
            Some(LeafNodeIndex::new(2)),
        );
        assert!(ensure_self_group_commit(&group, &processed, None, None).is_err());
    }

    /// OpenMLS rejects an external commit with more than one Remove, before
    /// [`ensure_self_group_commit`] sees it.
    #[test]
    fn a_self_group_rejoin_that_also_removes_a_sibling_is_rejected() {
        let (creator, device) = (Device::new(), Device::new());
        let mut group = group_with_blank_leaf(&creator, &device);
        let processed = try_external_commit(
            &device,
            &creator,
            &mut group,
            AadPayload::Resync,
            Some(LeafNodeIndex::new(3)),
        );
        assert!(processed.is_err());
    }

    #[test]
    fn a_self_group_rejoin_on_the_own_leaf_of_both_legs_is_accepted() {
        let (creator, device) = (Device::new(), Device::new());
        let mut group = group_with_blank_leaf(&creator, &device);
        let mut pq_group = group_with_blank_leaf(&creator, &device);
        let processed = external_commit(&device, &creator, &mut group, AadPayload::Resync, None);
        let pq_processed =
            external_commit(&device, &creator, &mut pq_group, AadPayload::Resync, None);
        let pq_staged_commit = expect_staged_commit(&pq_processed).unwrap();
        let checked =
            ensure_self_group_commit(&group, &processed, Some(&pq_group), Some(pq_staged_commit));
        assert!(checked.is_ok());
    }

    #[test]
    fn a_self_group_rejoin_without_a_pq_leg_is_rejected() {
        let (creator, device) = (Device::new(), Device::new());
        let mut group = group_with_blank_leaf(&creator, &device);
        let pq_group = group_with_blank_leaf(&creator, &device);
        let processed = external_commit(&device, &creator, &mut group, AadPayload::Resync, None);
        let error =
            ensure_self_group_commit(&group, &processed, Some(&pq_group), None).unwrap_err();
        assert_eq!(error.to_string(), "self-group resync without a PQ leg");
    }

    #[test]
    fn a_self_group_rejoin_that_removes_different_leaves_on_the_two_legs_is_rejected() {
        let (creator, device) = (Device::new(), Device::new());
        let mut group = group_with_blank_leaf(&creator, &device);
        // The device is at leaf 1 of this leg, and at leaf 2 of the other.
        let mut pq_group = new_group(&creator, vec![device.key_package()]);
        let processed = external_commit(&device, &creator, &mut group, AadPayload::Resync, None);
        let pq_processed =
            external_commit(&device, &creator, &mut pq_group, AadPayload::Resync, None);
        let pq_staged_commit = expect_staged_commit(&pq_processed).unwrap();
        let error =
            ensure_self_group_commit(&group, &processed, Some(&pq_group), Some(pq_staged_commit))
                .unwrap_err();
        assert_eq!(
            error.to_string(),
            "self-group resync removes different leaves on its two legs"
        );
    }

    #[test]
    fn a_self_group_external_commit_without_the_resync_aad_is_rejected() {
        let payloads = [
            AadPayload::GroupOperation(GroupOperationParamsAad {
                new_encrypted_user_profile_keys: Vec::new(),
            }),
            AadPayload::DeleteGroup,
        ];
        for payload in payloads {
            let (creator, device) = (Device::new(), Device::new());
            let mut group = group_with_blank_leaf(&creator, &device);
            let processed = external_commit(&device, &creator, &mut group, payload, None);
            assert!(ensure_self_group_commit(&group, &processed, None, None).is_err());
        }
    }

    #[test]
    fn a_self_group_external_commit_without_a_remove_is_rejected() {
        let (creator, device) = (Device::new(), Device::new());
        let mut group = group_with_blank_leaf(&creator, &device);
        let processed = external_commit(
            &Device::new(),
            &creator,
            &mut group,
            AadPayload::Resync,
            None,
        );
        assert!(ensure_self_group_commit(&group, &processed, None, None).is_err());
    }

    #[test]
    fn a_self_group_commit_with_a_join_proposal_is_rejected() {
        let creator = Device::new();
        let mut group = group_with_blank_leaf(&creator, &Device::new());
        let outsider = Device::new();
        let proposal =
            JoinProposal::new::<<OpenMlsRustCrypto as OpenMlsProvider>::StorageProvider>(
                outsider.key_package(),
                group.group_id().clone(),
                group.epoch(),
                &outsider.signer,
            )
            .unwrap();
        let ProcessedMessageContent::ExternalJoinProposalMessage(proposal) = group
            .process_message(
                &creator.provider,
                to_message_in(&proposal)
                    .try_into_protocol_message()
                    .unwrap(),
            )
            .unwrap()
            .into_content()
        else {
            panic!("expected a join proposal");
        };
        group
            .store_pending_proposal(creator.provider.storage(), *proposal)
            .unwrap();
        creator.commit(&mut group, Vec::new(), Vec::new());

        let staged_commit = group.pending_commit().unwrap();
        assert!(ensure_member_proposals(staged_commit).is_err());
    }
}
