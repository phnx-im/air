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
    utils::removed_client,
};
use anyhow::{Context, Result, anyhow, bail, ensure};
use mimi_room_policy::RoleIndex;
use openmls::prelude::{Credential, LeafNodeIndex, Sender, SignaturePublicKey, StagedCommit};

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

            credential.store(&mut *txn).await?;
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

    pub(super) fn process_resync(
        &self,
        credential: &Credential,
        staged_commit: &StagedCommit,
    ) -> Result<()> {
        let removed_index = staged_commit
            .remove_proposals()
            .next()
            .ok_or(anyhow!(
                "Resync operation did not contain a remove proposal"
            ))?
            .remove_proposal()
            .removed();

        let Some(removed_member) = self.mls_group().member_at(removed_index) else {
            bail!("Could not find removed member in group")
        };

        // Check that the leaf credential hasn't changed during the resync.
        if &removed_member.credential != credential {
            bail!("Invalid resync operation: Leaf credential does not match.")
        }

        // No need to verify or update the credential, since the sender is already member of the
        // group and the credential did not change.

        Ok(())
    }
}

/// Pair the new and old leaf credential of a member transition for AS verification.
///
/// Returns `None` when both are self-group credentials with the same client id, since they carry no
/// AS material to verify or store. Mixed pairs and client id changes are rejected.
pub(super) fn user_credential_transition(
    new: LeafCredential,
    old: LeafCredential,
    context: &str,
) -> Result<Option<(VerifiableUserCredential, VerifiableUserCredential)>> {
    match (new, old) {
        (LeafCredential::User(new), LeafCredential::User(old)) => Ok(Some((new, old))),
        (LeafCredential::SelfGroup(new), LeafCredential::SelfGroup(old)) => {
            ensure!(
                new.client_id() == old.client_id(),
                "self-group client id changed in {context}"
            );
            Ok(None)
        }
        (LeafCredential::User(_), LeafCredential::SelfGroup(_))
        | (LeafCredential::SelfGroup(_), LeafCredential::User(_)) => {
            bail!("mismatched leaf credential types in {context}")
        }
    }
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
    use aircommon::credentials::SelfGroupCredential;
    use uuid::Uuid;

    use super::*;

    fn self_group_credential(client_id: u128) -> LeafCredential {
        LeafCredential::SelfGroup(SelfGroupCredential::new(Uuid::from_u128(client_id)))
    }

    #[test]
    fn self_group_transition_preserves_client_id() -> anyhow::Result<()> {
        let transition = user_credential_transition(
            self_group_credential(1),
            self_group_credential(1),
            "update path",
        )?;

        assert!(transition.is_none());
        Ok(())
    }

    #[test]
    fn self_group_transition_rejects_client_id_change() {
        for context in ["update path", "resync"] {
            let transition = user_credential_transition(
                self_group_credential(2),
                self_group_credential(1),
                context,
            );

            assert!(transition.is_err(), "{context}");
        }
    }
}
