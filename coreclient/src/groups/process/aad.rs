// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::iter;

use aircommon::{
    credentials::{LeafCredential, UserCredential, VerifiableUserCredential},
    crypto::aead::keys::EncryptedUserProfileKey,
    messages::client_ds::{
        AadMessage, AadPayload, GroupOperationParamsAad, JoinConnectionGroupParamsAad,
    },
};
use anyhow::{Context, Result, bail, ensure};
use openmls::prelude::{ProcessedMessage, Sender, SignaturePublicKey, StagedCommit};
use tls_codec::DeserializeBytes as TlsDeserializeBytes;

use crate::{
    clients::api_clients::ApiClients,
    db::access::WriteDbTransaction,
    groups::{
        Group,
        client_auth_info::{StorableUserCredential, VerifiableUserCredentialExt},
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
            // The joiner closes the request right away, see
            // `CloseConnectionGroup` in the delivery service.
            AadPayload::CloseConnectionGroup(_) => {
                self.verify_connection_group_joiner(txn, api_clients, processed_message)
                    .await?;
                PostProcessAadResult {
                    we_were_removed: false,
                    encrypted_profile_infos: Vec::new(),
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
                validate_delete_commit(processed_message)?;
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

        // External commits carry the AAD of a resync or a connection-group join or close, and the
        // DS only takes group operations from members. Only a malicious DS can deliver an external
        // commit with this AAD.
        let Sender::Member(sender_index) = *processed_message.sender() else {
            bail!("group operation from a non-member sender");
        };

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
        let sender_leaf_key = self
            .mls_group
            .public_group()
            .leaf(sender_index)
            .context("Could not find sender leaf")?
            .signature_key();

        if let Some((new_credential, old_credential)) = user_credential_transition(
            (new_sender_credential, new_sender_leaf_key),
            (sender_credential, sender_leaf_key),
            "update path",
        )? {
            if new_credential == old_credential {
                // The credential was verified when the member entered the group, but the update
                // path can still rotate the leaf signature key.
                ensure!(
                    new_credential.verifying_key().as_slice() == new_sender_leaf_key.as_slice(),
                    "update path rotates the signature key without a new credential"
                );
            } else {
                verify_and_store_credential(
                    txn,
                    api_clients,
                    new_credential,
                    new_sender_leaf_key,
                    Some(&old_credential),
                )
                .await?;
            }
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
        let sender_credential = self
            .verify_connection_group_joiner(txn, api_clients, processed_message)
            .await?;
        Ok((
            sender_credential,
            join_connection_group_payload.encrypted_user_profile_key,
        ))
    }

    /// Checks that the commit is an external commit of a new user joining a
    /// connection group of one member, and verifies and persists the joiner's
    /// user credential.
    async fn verify_connection_group_joiner(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        processed_message: &ProcessedMessage,
    ) -> Result<UserCredential> {
        ensure!(
            !self.is_self_group(),
            "connection-group commit must not target the self group"
        );

        let staged_commit = expect_staged_commit(processed_message)?;

        let (sender_credential, sender_leaf_key) = update_path_leaf_node_info(staged_commit)?;
        let sender_credential = sender_credential
            .into_user()
            .context("expected a user credential joining a connection group")?;

        validate_join_connection_group_commit(
            processed_message.sender(),
            staged_commit.add_proposals().next().is_some()
                || staged_commit.update_proposals().next().is_some()
                || staged_commit.remove_proposals().next().is_some(),
            self.mls_group.members().count(),
            self.members()
                .any(|member| &member == sender_credential.user_id()),
        )?;

        let sender_credential = verify_and_store_credential(
            txn,
            api_clients,
            sender_credential,
            sender_leaf_key,
            None, // Since the join is an external commit, we don't have an old credential.
        )
        .await?;
        Ok(sender_credential.into())
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

        let old_leaf = self
            .mls_group
            .public_group()
            .leaf(removed_index)
            .context("Could not find removed member in group")?;
        let old_credential = LeafCredential::from_credential(old_leaf.credential())?;

        if let Some((sender_credential, old_credential)) = user_credential_transition(
            (sender_credential, sender_leaf_key),
            (old_credential, old_leaf.signature_key()),
            "resync",
        )? {
            verify_and_store_credential(
                txn,
                api_clients,
                sender_credential,
                sender_leaf_key,
                Some(&old_credential),
            )
            .await?;
        }
        Ok(())
    }
}

/// Make sure that a delete commit comes from a member and keeps the sender's credential.
fn validate_delete_commit(processed_message: &ProcessedMessage) -> Result<()> {
    ensure_delete_sender_is_member(processed_message.sender())?;
    let staged_commit = expect_staged_commit(processed_message)?;
    if let Some(leaf_node) = staged_commit.update_path_leaf_node() {
        ensure!(
            leaf_node.credential() == processed_message.credential(),
            "delete commit changes the sender's credential"
        );
    }
    Ok(())
}

/// Rejects a delete commit whose sender is not a current member of the group.
fn ensure_delete_sender_is_member(sender: &Sender) -> Result<()> {
    ensure!(
        matches!(sender, Sender::Member(_)),
        "DeleteGroup operation must be a member commit"
    );
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

/// Checks the shape of a commit that joins a connection group.
fn validate_join_connection_group_commit(
    sender: &Sender,
    contains_membership_proposal: bool,
    member_count: usize,
    joiner_is_member: bool,
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
    ensure!(
        !joiner_is_member,
        "JoinConnectionGroup joiner is already a member of the group"
    );
    Ok(())
}

/// Verifies `credential` against its AS credential and stores it.
async fn verify_and_store_credential(
    txn: &mut WriteDbTransaction<'_>,
    api_clients: &ApiClients,
    credential: VerifiableUserCredential,
    leaf_key: &SignaturePublicKey,
    old_credential: Option<&VerifiableUserCredential>,
) -> Result<StorableUserCredential> {
    let as_credentials =
        AsCredentials::fetch_for_verification(&mut *txn, api_clients, iter::once(&credential))
            .await?;
    let credential = credential.verify_and_validate(leaf_key, old_credential, &as_credentials)?;
    credential.store_checked(txn).await?;
    Ok(credential)
}

#[cfg(test)]
mod tests {

    use openmls::prelude::LeafNodeIndex;

    use super::*;

    #[test]
    fn delete_commit_must_come_from_a_member() {
        assert!(ensure_delete_sender_is_member(&Sender::Member(LeafNodeIndex::new(0))).is_ok());
        let error = ensure_delete_sender_is_member(&Sender::NewMemberCommit).unwrap_err();
        assert_eq!(
            error.to_string(),
            "DeleteGroup operation must be a member commit"
        );
    }

    #[test]
    fn join_connection_group_validation_enforces_operation_shape() {
        assert!(
            validate_join_connection_group_commit(&Sender::NewMemberCommit, false, 1, false)
                .is_ok()
        );

        let cases = [
            (
                Sender::Member(LeafNodeIndex::new(0)),
                false,
                1,
                false,
                "JoinConnectionGroup operation must be an external commit",
            ),
            (
                Sender::NewMemberCommit,
                true,
                1,
                false,
                "JoinConnectionGroup operation must not contain add, update, or remove proposals",
            ),
            (
                Sender::NewMemberCommit,
                false,
                2,
                false,
                "JoinConnectionGroup operation must target a connection group",
            ),
            (
                Sender::NewMemberCommit,
                false,
                1,
                true,
                "JoinConnectionGroup joiner is already a member of the group",
            ),
        ];

        for (
            sender,
            contains_membership_proposal,
            member_count,
            joiner_is_member,
            expected_error,
        ) in cases
        {
            let error = validate_join_connection_group_commit(
                &sender,
                contains_membership_proposal,
                member_count,
                joiner_is_member,
            )
            .expect_err("invalid JoinConnectionGroup operation should fail");
            assert!(
                error.to_string().contains(expected_error),
                "unexpected error: {error:#}"
            );
        }
    }
}
