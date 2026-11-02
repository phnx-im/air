// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub(crate) mod external;
pub(crate) mod vc_sibling;
pub(crate) mod welcome;

use std::collections::HashMap;

use aircommon::{
    credentials::{LeafCredential, UserCredential},
    crypto::{aead::keys::EncryptedUserProfileKey, indexed_aead::keys::UserProfileKey},
    identifiers::UserId,
};
use anyhow::{Context, Result};
use openmls::{
    prelude::{Credential, LeafNodeIndex, group_info::VerifiableGroupInfo},
    treesync::RatchetTreeIn,
};
use tracing::warn;

use crate::{
    db::access::WriteDbTransaction,
    groups::{Group, client_auth_info::StorableUserCredential},
};

#[derive(Debug)]
pub(crate) struct DecryptedProfileInfos {
    /// Profile infos of *other* members
    pub(crate) members: Vec<ProfileInfo>,
    /// None = DS entry missing or undecryptable
    pub(crate) own_profile_key: Option<UserProfileKey>,
}

#[derive(Debug)]
pub(crate) struct ProfileInfo {
    pub(crate) user_credential: UserCredential,
    pub(crate) user_profile_key: UserProfileKey,
}

impl From<(UserCredential, UserProfileKey)> for ProfileInfo {
    fn from((user_credential, user_profile_key): (UserCredential, UserProfileKey)) -> Self {
        Self {
            user_credential,
            user_profile_key,
        }
    }
}

impl Group {
    /// Pair the encrypted user profile keys with the group members and decrypt them.
    ///
    /// Keys that are missing or fail to decrypt are skipped with a warning: a stale DS entry must
    /// not fail the join/resync.
    fn decrypt_member_profile_keys(
        &self,
        credentials: Vec<StorableUserCredential>,
        indexed_keys: HashMap<LeafNodeIndex, EncryptedUserProfileKey>,
        // Positional fallback for servers that don't send indexed keys yet
        fallback_keys: HashMap<UserId, EncryptedUserProfileKey>,
    ) -> DecryptedProfileInfos {
        let own_user_id = self.own_user_id();
        let indices = self.mls_group().members().map(|m| m.index);

        let mut members = Vec::with_capacity(credentials.len());
        let mut own_profile_key = None;

        for (index, credential) in indices.zip(credentials) {
            let eupk = if !indexed_keys.is_empty() {
                indexed_keys.get(&index)
            } else {
                fallback_keys.get(credential.user_id())
            };
            let Some(eupk) = eupk else {
                if credential.user_id() != own_user_id {
                    // Own key is sometimes expected to be missing
                    warn!(
                        user_id =? credential.user_id(),
                        "No user profile key for member; skipping"
                    );
                }
                continue;
            };
            match UserProfileKey::decrypt(
                &self.identity_link_wrapper_key,
                eupk,
                credential.user_id(),
            ) {
                Ok(user_profile_key) => {
                    if credential.user_id() == own_user_id {
                        own_profile_key = Some(user_profile_key);
                    } else {
                        members.push(ProfileInfo {
                            user_profile_key,
                            user_credential: credential.into(),
                        });
                    }
                }
                Err(error) => {
                    warn!(
                        %error,
                        user_id =? credential.user_id(),
                        "Failed to decrypt user profile key; skipping"
                    );
                    continue;
                }
            }
        }
        DecryptedProfileInfos {
            members,
            own_profile_key,
        }
    }

    /// Build the positional fallback map (user id -> encrypted profile key).
    ///
    /// Used when the commit info carries no indexed keys. Credentials are verified later, so
    /// undecodable leaves are simply skipped here.
    fn encrypted_profile_keys_fallback(
        ratchet_tree: &RatchetTreeIn,
        encrypted_user_profile_keys: Vec<EncryptedUserProfileKey>,
        indexed_encrypted_user_profile_keys: &HashMap<LeafNodeIndex, EncryptedUserProfileKey>,
        own_user_id: &UserId,
    ) -> HashMap<UserId, EncryptedUserProfileKey> {
        if !indexed_encrypted_user_profile_keys.is_empty() {
            return HashMap::new();
        }
        ratchet_tree
            .leaves()
            .zip(encrypted_user_profile_keys)
            .filter_map(|(leaf_node, profile_key)| {
                let cred = LeafCredential::from_credential(leaf_node.credential()).ok()?;
                Some((cred.user_id(own_user_id).clone(), profile_key))
            })
            .collect()
    }

    /// Persist a freshly joined group after an external commit.
    ///
    /// Replace any prior group with the same id, store the user credentials, and decrypt the
    /// member profile keys.
    async fn store_after_external_join(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        credentials: Vec<StorableUserCredential>,
        indexed_encrypted_user_profile_keys: HashMap<LeafNodeIndex, EncryptedUserProfileKey>,
        encrypted_profile_keys_fallback: HashMap<UserId, EncryptedUserProfileKey>,
    ) -> anyhow::Result<DecryptedProfileInfos> {
        // If the group previously existed, delete it first.
        Group::delete_from_db(txn, self.group_id()).await?;
        self.store(&mut *txn).await?;

        for credential in &credentials {
            credential.store(&mut *txn).await?;
        }

        Ok(self.decrypt_member_profile_keys(
            credentials,
            indexed_encrypted_user_profile_keys,
            encrypted_profile_keys_fallback,
        ))
    }
}

/// Returns the leaf credential of the member that signed the group info. The
/// credential is taken from the ratchet tree that came with the group info.
///
/// Only trustworthy once the join has verified the group info and the tree.
fn group_info_signer_credential(
    group_info: &VerifiableGroupInfo,
    ratchet_tree: &RatchetTreeIn,
) -> Option<Credential> {
    let signer = group_info.signer();
    ratchet_tree
        .full_leaves()
        .find(|(index, _)| *index == signer)
        .map(|(_, leaf)| leaf.credential().clone())
}

/// The user that set a group's tombstone. Since the DS doesn't allow any group
/// operations on a group with a tombstone, the creator of the group info must
/// be the one that set it.
fn closer(group_info_signer: Option<&Credential>, own_user_id: &UserId) -> Result<UserId> {
    let credential = group_info_signer.context("group info signer has no leaf")?;
    Ok(LeafCredential::from_credential(credential)?
        .user_id(own_user_id)
        .clone())
}
