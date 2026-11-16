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
use openmls::prelude::{Credential, LeafNode, LeafNodeIndex, MlsGroup, SignaturePublicKey};
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
    /// Validate the leaf of a client about to be added to this self-group: it must carry a
    /// self-group credential whose client id is not yet used by any existing member, and the
    /// credential must verify for the leaf and our own user.
    pub(crate) fn validate_self_group_add(
        &self,
        added: &LeafNode,
        own_user_credential: &UserCredential,
    ) -> Result<()> {
        validate_self_group_add_credential(
            self.mls_group.members().map(|member| member.credential),
            added,
            own_user_credential,
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
/// `self_group_user` is our own user credential. It must be `Some` if `mls_group` is our own self
/// group, and `None` otherwise.
///
/// Might do a network request to fetch AS credentials.
///
/// Returns the credentials of the group members.
pub(super) async fn verify_member_credentials(
    txn: &mut WriteDbTransaction<'_>,
    api_clients: &ApiClients,
    mls_group: &MlsGroup,
    self_group_user: Option<&UserCredential>,
) -> anyhow::Result<Vec<StorableUserCredential>> {
    let unverified_credentials = classify_member_credentials(
        mls_group.members().map(|member| {
            (
                member.credential,
                SignaturePublicKey::from(member.signature_key),
            )
        }),
        self_group_user,
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
/// When classifying member credential of the self-group, `self_group_user` must be `Some` with our
/// own user credential.
fn classify_member_credentials(
    members: impl Iterator<Item = (Credential, SignaturePublicKey)>,
    self_group_user: Option<&UserCredential>,
) -> anyhow::Result<Vec<(VerifiableUserCredential, SignaturePublicKey)>> {
    let mut client_ids = HashSet::new();
    let mut unverified_credentials = Vec::new();
    for (credential, signature_key) in members {
        match LeafCredential::from_credential(&credential) {
            Ok(LeafCredential::User(credential)) => {
                ensure!(
                    self_group_user.is_none(),
                    "user credential in the self-group"
                );
                unverified_credentials.push((credential, signature_key));
            }
            Ok(LeafCredential::SelfGroup(credential)) => {
                let own_user =
                    self_group_user.context("self-group credential outside the self-group")?;
                credential
                    .verify(
                        signature_key.as_slice(),
                        own_user.user_id(),
                        own_user.verifying_key(),
                    )
                    .context("invalid self-group credential")?;
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

/// Validate the leaf of a client about to be added to the self-group.
fn validate_self_group_add_credential(
    members: impl Iterator<Item = Credential>,
    added: &LeafNode,
    own_user_credential: &UserCredential,
) -> anyhow::Result<()> {
    let added_signature_key = added.signature_key().as_slice();
    let LeafCredential::SelfGroup(added) = LeafCredential::from_credential(added.credential())?
    else {
        bail!("expected a self-group credential");
    };
    added
        .verify(
            added_signature_key,
            own_user_credential.user_id(),
            own_user_credential.verifying_key(),
        )
        .context("invalid self-group credential of the added client")?;
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
        credentials::{
            SelfGroupCredential,
            keys::UserSigningKey,
            test_utils::{malformed_self_group_credential, random_user_signing_key},
        },
        mls_group_config::self_group_leaf_node_capabilities,
    };
    use openmls::prelude::{CredentialWithKey, KeyPackage};
    use openmls_basic_credential::SignatureKeyPair;
    use openmls_rust_crypto::OpenMlsRustCrypto;
    use uuid::Uuid;

    use crate::clients::CIPHERSUITE;

    use super::*;

    fn self_group_credential(client_id: Uuid) -> Credential {
        SelfGroupCredential::new(client_id)
            .to_credential()
            .expect("serializing a self-group credential")
    }

    fn signed_credential(
        client_id: Uuid,
        signature_key: &SignaturePublicKey,
        user: &UserSigningKey,
    ) -> Credential {
        SelfGroupCredential::signed(client_id, signature_key.as_slice(), user)
            .unwrap()
            .to_credential()
            .unwrap()
    }

    fn user_credential() -> Credential {
        Credential::try_from(random_user_signing_key().credential())
            .expect("serializing a user credential")
    }

    fn signature_key() -> SignaturePublicKey {
        SignaturePublicKey::from(vec![0u8; 32])
    }

    fn other_signature_key() -> SignaturePublicKey {
        SignaturePublicKey::from(vec![1u8; 32])
    }

    #[test]
    fn self_group_members_with_unique_client_ids_is_accepted() {
        let own = random_user_signing_key();
        let members = [
            (self_group_credential(Uuid::from_u128(1)), signature_key()),
            (
                signed_credential(Uuid::from_u128(2), &other_signature_key(), &own),
                other_signature_key(),
            ),
        ];
        let unverified = classify_member_credentials(members.into_iter(), Some(own.credential()))
            .expect("unique client ids should be accepted");
        assert!(unverified.is_empty());
    }

    #[test]
    fn duplicate_client_ids_in_self_group_are_rejected() {
        let own = random_user_signing_key();
        let client_id = Uuid::from_u128(1);
        let members = [
            (self_group_credential(client_id), signature_key()),
            (self_group_credential(client_id), signature_key()),
        ];
        let error = classify_member_credentials(members.into_iter(), Some(own.credential()))
            .expect_err("duplicate client ids should be rejected");
        assert!(
            error.to_string().contains("duplicate client id"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn invalid_self_group_credentials_are_rejected() {
        let own = random_user_signing_key();
        let client_id = Uuid::from_u128(1);
        let invalid = [
            // Signed by another user.
            (
                signed_credential(client_id, &signature_key(), &random_user_signing_key()),
                signature_key(),
            ),
            // Signed for another leaf key.
            (
                signed_credential(client_id, &other_signature_key(), &own),
                signature_key(),
            ),
            (malformed_self_group_credential(client_id), signature_key()),
        ];
        for member in invalid {
            let members = [
                member,
                (self_group_credential(Uuid::from_u128(2)), signature_key()),
            ];
            let error = classify_member_credentials(members.into_iter(), Some(own.credential()))
                .expect_err("an invalid self-group credential should be rejected");
            assert!(
                error.to_string().contains("invalid self-group credential"),
                "unexpected error: {error:#}"
            );
        }
    }

    #[test]
    fn self_group_credential_outside_self_group_is_rejected() {
        let members = [(self_group_credential(Uuid::from_u128(1)), signature_key())];
        let error = classify_member_credentials(members.into_iter(), None)
            .expect_err("self-group credential outside the self-group should be rejected");
        assert!(
            error.to_string().contains("outside the self-group"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn user_credential_in_self_group_is_rejected() {
        let own = random_user_signing_key();
        let members = [
            (user_credential(), signature_key()),
            (self_group_credential(Uuid::from_u128(1)), signature_key()),
        ];
        let error = classify_member_credentials(members.into_iter(), Some(own.credential()))
            .expect_err("user credential in the self-group should be rejected");
        assert!(
            error.to_string().contains("user credential"),
            "unexpected error: {error:#}"
        );
    }

    /// The leaf of a key package with a fresh signature key and the credential `credential`
    /// builds for that key.
    fn added_leaf(credential: impl FnOnce(&SignaturePublicKey) -> Credential) -> LeafNode {
        let provider = OpenMlsRustCrypto::default();
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        let signature_key = SignaturePublicKey::from(signer.public().to_vec());
        let credential_with_key = CredentialWithKey {
            credential: credential(&signature_key),
            signature_key,
        };
        KeyPackage::builder()
            .leaf_node_capabilities(self_group_leaf_node_capabilities())
            .build(CIPHERSUITE, &provider, &signer, credential_with_key)
            .unwrap()
            .key_package()
            .leaf_node()
            .clone()
    }

    #[test]
    fn adding_a_fresh_client_id_is_accepted() {
        let own = random_user_signing_key();
        let members = [self_group_credential(Uuid::from_u128(1))];
        for added in [
            added_leaf(|_| self_group_credential(Uuid::from_u128(2))),
            added_leaf(|key| signed_credential(Uuid::from_u128(2), key, &own)),
        ] {
            validate_self_group_add_credential(
                members.clone().into_iter(),
                &added,
                own.credential(),
            )
            .expect("fresh client id should be accepted");
        }
    }

    #[test]
    fn adding_an_invalid_self_group_credential_is_rejected() {
        let own = random_user_signing_key();
        let members = [self_group_credential(Uuid::from_u128(1))];
        for added in [
            added_leaf(|key| {
                signed_credential(Uuid::from_u128(2), key, &random_user_signing_key())
            }),
            added_leaf(|_| signed_credential(Uuid::from_u128(2), &other_signature_key(), &own)),
            added_leaf(|_| malformed_self_group_credential(Uuid::from_u128(2))),
        ] {
            let error = validate_self_group_add_credential(
                members.clone().into_iter(),
                &added,
                own.credential(),
            )
            .expect_err("an invalid credential should be rejected");
            assert!(
                error.to_string().contains("invalid self-group credential"),
                "unexpected error: {error:#}"
            );
        }
    }

    #[test]
    fn a_member_with_an_invalid_credential_does_not_block_an_add() {
        let own = random_user_signing_key();
        let members = [
            malformed_self_group_credential(Uuid::from_u128(1)),
            signed_credential(
                Uuid::from_u128(3),
                &signature_key(),
                &random_user_signing_key(),
            ),
        ];
        let added = added_leaf(|_| self_group_credential(Uuid::from_u128(2)));
        validate_self_group_add_credential(members.into_iter(), &added, own.credential())
            .expect("an add should not depend on other members' signatures");
    }

    #[test]
    fn adding_a_duplicate_client_id_is_rejected() {
        let own = random_user_signing_key();
        let client_id = Uuid::from_u128(1);
        let members = [self_group_credential(client_id)];
        let added = added_leaf(|_| self_group_credential(client_id));
        let error =
            validate_self_group_add_credential(members.into_iter(), &added, own.credential())
                .expect_err("duplicate client id should be rejected");
        assert!(
            error.to_string().contains("already present"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn adding_a_user_credential_is_rejected() {
        let own = random_user_signing_key();
        let members = [self_group_credential(Uuid::from_u128(1))];
        let added = added_leaf(|_| user_credential());
        let error =
            validate_self_group_add_credential(members.into_iter(), &added, own.credential())
                .expect_err("user credential should be rejected");
        assert!(
            error
                .to_string()
                .contains("expected a self-group credential"),
            "unexpected error: {error:#}"
        );
    }
}
