// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::iter;

use aircommon::{
    credentials::{LeafCredential, UserCredential},
    crypto::aead::keys::EncryptedUserProfileKey,
    messages::client_ds::{
        AadMessage, AadPayload, GroupOperationParamsAad, JoinConnectionGroupParamsAad,
    },
};
use anyhow::{Context, Result, anyhow, ensure};
use openmls::prelude::{ProcessedMessage, Sender, SignaturePublicKey, StagedCommit};
use tls_codec::DeserializeBytes as TlsDeserializeBytes;

use crate::{
    clients::api_clients::ApiClients,
    db::access::WriteDbTransaction,
    groups::{
        Group,
        client_auth_info::VerifiableUserCredentialExt,
        process::{
            PostProcessAadResult, expect_staged_commit,
            validation::{user_credential_transition, verify_pq_added_signature_keys},
        },
    },
    key_stores::as_credentials::AsCredentials,
};

impl Group {
    /// Process the AAD payload of a staged commit, dispatching to the handler
    /// for the concrete operation.
    pub(super) async fn process_aad_payload(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        processed_message: &ProcessedMessage,
        pq_staged_commit: Option<&StagedCommit>,
        sender_credential: LeafCredential,
    ) -> Result<PostProcessAadResult> {
        // Let's figure out which operation this is meant to be.
        let aad_payload =
            AadMessage::tls_deserialize_exact_bytes(processed_message.tail_aad())?.into_payload();
        let result = match aad_payload {
            AadPayload::GroupOperation(group_operation_payload) => {
                let encrypted_profile_infos = self
                    .process_group_operation_aad(
                        txn,
                        api_clients,
                        processed_message,
                        pq_staged_commit,
                        sender_credential,
                        group_operation_payload,
                    )
                    .await?;
                PostProcessAadResult {
                    we_were_removed: false,
                    encrypted_profile_infos,
                }
            }
            AadPayload::JoinConnectionGroup(join_connection_group_payload) => {
                let profile_info = self
                    .process_join_connection_group_aad(
                        txn,
                        api_clients,
                        processed_message,
                        join_connection_group_payload,
                    )
                    .await?;
                PostProcessAadResult {
                    we_were_removed: false,
                    encrypted_profile_infos: vec![profile_info],
                }
            }
            AadPayload::Resync => {
                self.process_resync_aad(txn, api_clients, processed_message)
                    .await?;
                PostProcessAadResult {
                    we_were_removed: false,
                    encrypted_profile_infos: Vec::new(),
                }
            }
            // The group was deleted; there is nothing else to do at this point.
            AadPayload::DeleteGroup => {
                ensure_delete_keeps_sender_credential(processed_message)?;
                PostProcessAadResult {
                    we_were_removed: true,
                    encrypted_profile_infos: Vec::new(),
                }
            }
        };

        Ok(result)
    }

    /// Process a group operation AAD payload: verify and persist any added
    /// credentials and a potentially updated sender leaf credential. Returns
    /// the encrypted user profile keys of any added clients.
    async fn process_group_operation_aad(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        processed_message: &ProcessedMessage,
        pq_staged_commit: Option<&StagedCommit>,
        sender_credential: LeafCredential,
        group_operation_payload: GroupOperationParamsAad,
    ) -> Result<Vec<(UserCredential, EncryptedUserProfileKey)>> {
        let staged_commit = expect_staged_commit(processed_message)?;

        let mut encrypted_profile_infos: Vec<(UserCredential, EncryptedUserProfileKey)> =
            Vec::new();

        let number_of_adds = staged_commit.add_proposals().count();
        let number_of_upks = group_operation_payload
            .new_encrypted_user_profile_keys
            .len();
        ensure!(
            number_of_adds == number_of_upks,
            "Number of add proposals and user profile keys don't match"
        );

        // Process adds if there are any.
        if !group_operation_payload
            .new_encrypted_user_profile_keys
            .is_empty()
        {
            // Verify that T/PQ added user signature keys match
            verify_pq_added_signature_keys(staged_commit, pq_staged_commit)?;

            // Compute the sender identity before the `&mut self` calls below.
            let sender_identity = sender_credential.room_policy_identity();

            if self.is_self_group() {
                // A self-group add links a new device of the own user. Its leaf
                // carries a self-group credential without AS material, and the
                // own user profile needs no fetching, so there are no profile
                // infos to collect.
                self.process_self_group_adds(&sender_identity, staged_commit)?;
            } else {
                // Collect the verifiable credentials
                let mut verifiable_credentials = Vec::with_capacity(number_of_adds);
                for ap in staged_commit.add_proposals() {
                    let credential = ap.add_proposal().key_package().leaf_node().credential();
                    let credential = LeafCredential::from_credential(credential)?
                        .into_user()
                        .context("expected a user credential in a regular-group add")?;
                    verifiable_credentials.push(credential);
                }

                let as_credentials = AsCredentials::fetch_for_verification(
                    &mut *txn,
                    api_clients,
                    verifiable_credentials.iter(),
                )
                .await?;
                let credentials = self
                    .process_adds(&sender_identity, staged_commit, &mut *txn, &as_credentials)
                    .await?;
                // Match up user credentials and new UserProfileKeys
                let new_profile_infos: Vec<_> = credentials
                    .into_iter()
                    .zip(group_operation_payload.new_encrypted_user_profile_keys)
                    .collect();
                encrypted_profile_infos.extend(new_profile_infos);
            }
        }

        // Process updates if there are any.
        // Check if the client has updated its leaf credential.
        let (new_sender_credential, new_sender_leaf_key) =
            update_path_leaf_node_info(staged_commit)?;

        if let Some((new_credential, old_credential)) =
            user_credential_transition(new_sender_credential, sender_credential, "update path")?
        {
            let as_credentials = AsCredentials::fetch_for_verification(
                &mut *txn,
                api_clients,
                iter::once(&new_credential),
            )
            .await?;
            if new_credential != old_credential {
                let credential = new_credential.verify_and_validate(
                    new_sender_leaf_key,
                    Some(&old_credential),
                    &as_credentials,
                )?;
                credential.store(txn).await?;
            }
        }

        // Process a resync if this is one
        if matches!(processed_message.sender(), Sender::NewMemberCommit) {
            self.process_resync(processed_message.credential(), staged_commit)?;
        }

        Ok(encrypted_profile_infos)
    }

    /// Process a join-connection-group AAD payload: verify and persist the
    /// joiner's user credential. Returns the joiner's encrypted user profile
    /// key.
    async fn process_join_connection_group_aad(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        processed_message: &ProcessedMessage,
        join_connection_group_payload: JoinConnectionGroupParamsAad,
    ) -> Result<(UserCredential, EncryptedUserProfileKey)> {
        let staged_commit = expect_staged_commit(processed_message)?;

        validate_join_connection_group_commit(
            processed_message.sender(),
            staged_commit.add_proposals().next().is_some()
                || staged_commit.update_proposals().next().is_some()
                || staged_commit.remove_proposals().next().is_some(),
            self.mls_group.members().count(),
        )?;

        // JoinConnectionGroup Phase 1: Decrypt and verify the
        // user credential of the joiner
        let (sender_credential, sender_leaf_key) = update_path_leaf_node_info(staged_commit)?;
        let sender_credential = sender_credential
            .into_user()
            .context("expected a user credential joining a connection group")?;

        let as_credentials = AsCredentials::fetch_for_verification(
            &mut *txn,
            api_clients,
            iter::once(&sender_credential),
        )
        .await?;

        let sender_credential = sender_credential.verify_and_validate(
            sender_leaf_key,
            None, // Since the join is an external commit, we don't have an old credential.
            &as_credentials,
        )?;

        // TODO: (More) validation:
        // * Check that the user id is unique.

        // JoinConnectionGroup Phase 2: Persist the user credential
        sender_credential.store(txn).await?;
        Ok((
            sender_credential.into(),
            join_connection_group_payload.encrypted_user_profile_key,
        ))
    }

    /// Process a resync AAD payload: verify and persist the resyncing member's
    /// (unchanged) user credential.
    async fn process_resync_aad(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        processed_message: &ProcessedMessage,
    ) -> Result<()> {
        let staged_commit = expect_staged_commit(processed_message)?;

        // Check if it's an external commit. This implies that
        // there is only one remove proposal.
        ensure!(
            matches!(processed_message.sender(), Sender::NewMemberCommit),
            "Resync operation must be an external commit"
        );

        let (sender_credential, sender_leaf_key) = update_path_leaf_node_info(staged_commit)?;

        let removed_index = staged_commit
            .remove_proposals()
            .next()
            .context("Resync operation did not contain a remove proposal")?
            .remove_proposal()
            .removed();

        let old_credential = self
            .mls_group
            .member(removed_index)
            .ok_or(anyhow!("Could not find removed member in group"))?;
        let old_credential = LeafCredential::from_credential(old_credential)?;

        if let Some((sender_credential, old_credential)) =
            user_credential_transition(sender_credential, old_credential, "resync")?
        {
            let as_credentials = AsCredentials::fetch_for_verification(
                &mut *txn,
                api_clients,
                iter::once(&sender_credential),
            )
            .await?;
            let sender_credential = sender_credential.verify_and_validate(
                sender_leaf_key,
                Some(&old_credential),
                &as_credentials,
            )?;
            sender_credential.store(txn).await?;
        }
        Ok(())
    }
}

/// Make sure that the sender's credential doesn't change in a delete commit.
fn ensure_delete_keeps_sender_credential(processed_message: &ProcessedMessage) -> Result<()> {
    let staged_commit = expect_staged_commit(processed_message)?;
    if let Some(leaf_node) = staged_commit.update_path_leaf_node() {
        ensure!(
            leaf_node.credential() == processed_message.credential(),
            "delete commit changes the sender's credential"
        );
    }
    Ok(())
}

fn update_path_leaf_node_info(
    staged_commit: &StagedCommit,
) -> Result<(LeafCredential, &SignaturePublicKey)> {
    let leaf_node = staged_commit
        .update_path_leaf_node()
        .context("Could not find sender leaf node")?;
    let credential = LeafCredential::from_credential(leaf_node.credential())?;
    let signature_key = leaf_node.signature_key();
    Ok((credential, signature_key))
}

fn validate_join_connection_group_commit(
    sender: &Sender,
    contains_membership_proposal: bool,
    member_count: usize,
) -> Result<()> {
    ensure!(
        matches!(sender, Sender::NewMemberCommit),
        "JoinConnectionGroup operation must be an external commit"
    );
    ensure!(
        !contains_membership_proposal,
        "JoinConnectionGroup operation must not contain add, update, or remove proposals"
    );
    ensure!(
        member_count == 1,
        "JoinConnectionGroup operation must target a connection group"
    );
    Ok(())
}

#[cfg(test)]
mod tests {

    use openmls::prelude::LeafNodeIndex;

    use super::*;

    #[test]
    fn join_connection_group_validation_enforces_operation_shape() {
        assert!(validate_join_connection_group_commit(&Sender::NewMemberCommit, false, 1).is_ok());

        let cases = [
            (
                Sender::Member(LeafNodeIndex::new(0)),
                false,
                1,
                "JoinConnectionGroup operation must be an external commit",
            ),
            (
                Sender::NewMemberCommit,
                true,
                1,
                "JoinConnectionGroup operation must not contain add, update, or remove proposals",
            ),
            (
                Sender::NewMemberCommit,
                false,
                2,
                "JoinConnectionGroup operation must target a connection group",
            ),
        ];

        for (sender, contains_membership_proposal, member_count, expected_error) in cases {
            let error = validate_join_connection_group_commit(
                &sender,
                contains_membership_proposal,
                member_count,
            )
            .expect_err("invalid JoinConnectionGroup operation should fail");
            assert!(
                error.to_string().contains(expected_error),
                "unexpected error: {error:#}"
            );
        }
    }
}
