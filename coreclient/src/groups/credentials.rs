// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet};

use aircommon::{
    credentials::{
        GroupStorageWitness, LeafCredential, LeafCredentialError, UserCredential,
        VerifiableUserCredential,
    },
    identifiers::UserId,
};
use anyhow::{Context, Result, anyhow, bail, ensure};
use openmls::prelude::{Credential, LeafNodeIndex, MlsGroup, SignaturePublicKey};
use tracing::error;

use crate::{
    clients::api_clients::ApiClients,
    db::access::WriteDbTransaction,
    groups::{
        Group,
        client_auth_info::{StorableUserCredential, VerifiableUserCredentialExt},
    },
    key_stores::as_credentials::AsCredentials,
};

impl Group {
    /// Validate the leaf credential of a client about to be added to this self-group: it must be
    /// a self-group credential whose client id is not yet used by any existing member.
    pub(crate) fn validate_self_group_add(&self, added: &Credential) -> Result<()> {
        validate_self_group_add_credential(
            self.mls_group.members().map(|member| member.credential),
            added,
        )
    }

    /// Returns an iterator over [`UserId`]s of the members of the group.
    pub(crate) fn members(&self) -> impl Iterator<Item = UserId> {
        self.mls_group.members().filter_map(|m| {
            let credential = LeafCredential::from_credential(&m.credential)
                .inspect_err(|error| {
                    error!(%error, "Invalid member credential");
                })
                .ok()?;
            Some(credential.user_id(self.own_user_id()).clone())
        })
    }

    /// Deserializes user credentials from the corresponding leaf node.
    ///
    /// Does not guarantee that the credential was verified and is valid.
    pub(crate) fn unverified_credential_at(
        &self,
        index: LeafNodeIndex,
    ) -> Result<Option<LeafCredential>, LeafCredentialError> {
        self.mls_group
            .member_at(index)
            .map(|m| LeafCredential::from_credential(&m.credential))
            .transpose()
    }

    /// Same as [`Self::unverified_credential_at()`] but guarantees that the credential was
    /// verified and is valid (if leaf contains valid data).
    ///
    /// The guarantee is given by the presence of the `witness` argument.
    pub(crate) fn credential_at(
        &self,
        index: LeafNodeIndex,
        witness: &impl GroupStorageWitness,
    ) -> anyhow::Result<Option<UserCredential>> {
        ensure!(self.group_id() == witness.group_id(), "Group ID mismatch");
        let Some(credential) = self.unverified_credential_at(index)? else {
            return Ok(None);
        };
        match credential {
            LeafCredential::User(credential) => {
                Ok(Some(UserCredential::assume_verified(credential, witness)))
            }
            LeafCredential::SelfGroup(_) => {
                Err(anyhow!("self-group leaf carries no user credential"))
            }
        }
    }

    /// Same as [`Self::credential_at`] but resolves the leaf owner's user id, which also works
    /// for self-group leaves. They carry no user credential and resolve to the own user id.
    pub(crate) fn user_id_at(
        &self,
        index: LeafNodeIndex,
        witness: &impl GroupStorageWitness,
    ) -> anyhow::Result<Option<UserId>> {
        ensure!(self.group_id() == witness.group_id(), "Group ID mismatch");
        let Some(credential) = self.unverified_credential_at(index)? else {
            return Ok(None);
        };
        Ok(Some(credential.user_id(self.own_user_id()).clone()))
    }
}

/// Verify credentials of *all* members of the group.
///
/// Might do a network request to fetch AS credentials.
///
/// Returns the credentials of the group members.
pub(super) async fn verify_member_credentials(
    txn: &mut WriteDbTransaction<'_>,
    api_clients: &ApiClients,
    mls_group: &MlsGroup,
    // Whether this is the user's own self group. A self-group credential carries nothing to
    // verify against the AS, so it is only accepted here.
    is_self_group: bool,
) -> anyhow::Result<Vec<StorableUserCredential>> {
    let unverified_credentials = classify_member_credentials(
        mls_group.members().map(|member| {
            (
                member.credential,
                SignaturePublicKey::from(member.signature_key),
            )
        }),
        is_self_group,
    )?;

    let as_credentials = AsCredentials::fetch_for_verification(
        txn,
        api_clients,
        unverified_credentials.iter().map(|(c, _)| c),
    )
    .await?;

    let mut verified = Vec::with_capacity(unverified_credentials.len());
    for (credential, leaf_verifying_key) in unverified_credentials {
        let credential = VerifiableUserCredential::verify_and_validate(
            credential,
            &leaf_verifying_key,
            None,
            &as_credentials,
        )?;
        verified.push(credential);
    }
    Ok(verified)
}

/// Classify the leaf credentials of all group members for verification.
///
/// User credentials are returned together with their leaf signature keys for AS verification.
/// Self-group credentials carry nothing to verify against the AS. They are only accepted inside
/// the user's own self-group, where room policy is keyed on the client id, so each leaf must
/// carry a distinct one. Conversely, the self-group accepts only self-group credentials.
fn classify_member_credentials(
    members: impl Iterator<Item = (Credential, SignaturePublicKey)>,
    is_self_group: bool,
) -> anyhow::Result<Vec<(VerifiableUserCredential, SignaturePublicKey)>> {
    let mut client_ids = HashSet::new();
    let mut unverified_credentials = Vec::new();
    for (credential, signature_key) in members {
        match LeafCredential::from_credential(&credential) {
            Ok(LeafCredential::User(credential)) => {
                ensure!(!is_self_group, "user credential in the self-group");
                unverified_credentials.push((credential, signature_key));
            }
            Ok(LeafCredential::SelfGroup(credential)) => {
                ensure!(
                    is_self_group,
                    "self-group credential outside the self-group"
                );
                ensure!(
                    client_ids.insert(credential.client_id()),
                    "duplicate client id in the self-group"
                );
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(unverified_credentials)
}

/// Validate the leaf credential of a client about to be added to the self-group.
///
/// The credential must be a self-group credential and its client id must not collide with a
/// leaf already in the self group, since room policy is keyed on the client id.
fn validate_self_group_add_credential(
    members: impl Iterator<Item = Credential>,
    added: &Credential,
) -> anyhow::Result<()> {
    let LeafCredential::SelfGroup(added) = LeafCredential::from_credential(added)? else {
        bail!("expected a self-group credential");
    };
    for credential in members {
        match LeafCredential::from_credential(&credential)? {
            LeafCredential::SelfGroup(existing) => {
                ensure!(
                    existing.client_id() != added.client_id(),
                    "client id already present in the self-group"
                );
            }
            LeafCredential::User(_) => bail!("user credential in the self-group"),
        }
    }
    Ok(())
}

/// Verifies that every leaf holds the same signature key in both legs.
///
/// This is the binding between a member's two leaves in our deployment: the PQ
/// leaf carries an empty credential, so the signing key is the only thing
/// tying the two together. The draft requires membership to be consistent
/// across the two sessions without saying how a member is identified, so this
/// is our policy rather than a protocol rule, which is why it lives here and
/// not in `apqmls`.
///
/// Only usable while both ciphersuites agree on the signature algorithm, which
/// holds for every ciphersuite pair we deploy. See
/// [`Group::verify_pq_signature_key_at`] for the per-operation counterpart
/// applied to incoming commits.
pub(super) fn verify_pq_signature_keys(
    t_group: &MlsGroup,
    pq_group: &MlsGroup,
) -> anyhow::Result<()> {
    ensure!(
        t_group.ciphersuite().signature_algorithm() == pq_group.ciphersuite().signature_algorithm(),
        "the two legs must share a signature algorithm to be bound by their signing keys"
    );
    let pq_keys: HashMap<_, _> = pq_group
        .members()
        .map(|member| (member.index, member.signature_key))
        .collect();
    for t_member in t_group.members() {
        let pq_key = pq_keys
            .get(&t_member.index)
            .with_context(|| format!("no PQ leaf at index {:?}", t_member.index))?;
        ensure!(
            &t_member.signature_key == pq_key,
            "T and PQ signature keys at index {:?} do not match",
            t_member.index
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use aircommon::{
        credentials::{SelfGroupCredential, test_utils::create_test_credentials},
        identifiers::UserId,
    };
    use uuid::Uuid;

    use super::*;

    fn self_group_credential(client_id: Uuid) -> Credential {
        SelfGroupCredential::new(client_id)
            .to_credential()
            .expect("serializing a self-group credential")
    }

    fn user_credential() -> Credential {
        let user_id = UserId::random("example.com".parse().unwrap());
        let (_as_signing_key, user_signing_key) = create_test_credentials(user_id);
        Credential::try_from(user_signing_key.credential()).expect("serializing a user credential")
    }

    fn signature_key() -> SignaturePublicKey {
        SignaturePublicKey::from(vec![0u8; 32])
    }

    #[test]
    fn self_group_members_with_unique_client_ids_is_accepted() {
        let members = [
            (self_group_credential(Uuid::from_u128(1)), signature_key()),
            (self_group_credential(Uuid::from_u128(2)), signature_key()),
        ];
        let unverified = classify_member_credentials(members.into_iter(), true)
            .expect("unique client ids should be accepted");
        assert!(unverified.is_empty());
    }

    #[test]
    fn duplicate_client_ids_in_self_group_are_rejected() {
        let client_id = Uuid::from_u128(1);
        let members = [
            (self_group_credential(client_id), signature_key()),
            (self_group_credential(client_id), signature_key()),
        ];
        let error = classify_member_credentials(members.into_iter(), true)
            .expect_err("duplicate client ids should be rejected");
        assert!(
            error.to_string().contains("duplicate client id"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn self_group_credential_outside_self_group_is_rejected() {
        let members = [(self_group_credential(Uuid::from_u128(1)), signature_key())];
        let error = classify_member_credentials(members.into_iter(), false)
            .expect_err("self-group credential outside the self-group should be rejected");
        assert!(
            error.to_string().contains("outside the self-group"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn user_credential_in_self_group_is_rejected() {
        let members = [
            (user_credential(), signature_key()),
            (self_group_credential(Uuid::from_u128(1)), signature_key()),
        ];
        let error = classify_member_credentials(members.into_iter(), true)
            .expect_err("user credential in the self-group should be rejected");
        assert!(
            error.to_string().contains("user credential"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn adding_a_fresh_client_id_is_accepted() {
        let members = [self_group_credential(Uuid::from_u128(1))];
        let added = self_group_credential(Uuid::from_u128(2));
        validate_self_group_add_credential(members.into_iter(), &added)
            .expect("fresh client id should be accepted");
    }

    #[test]
    fn adding_a_duplicate_client_id_is_rejected() {
        let client_id = Uuid::from_u128(1);
        let members = [self_group_credential(client_id)];
        let added = self_group_credential(client_id);
        let error = validate_self_group_add_credential(members.into_iter(), &added)
            .expect_err("duplicate client id should be rejected");
        assert!(
            error.to_string().contains("already present"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn adding_a_user_credential_is_rejected() {
        let members = [self_group_credential(Uuid::from_u128(1))];
        let added = user_credential();
        let error = validate_self_group_add_credential(members.into_iter(), &added)
            .expect_err("user credential should be rejected");
        assert!(
            error
                .to_string()
                .contains("expected a self-group credential"),
            "unexpected error: {error:#}"
        );
    }
}
