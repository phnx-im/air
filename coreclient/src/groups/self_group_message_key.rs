// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Derivation and persistence of the per-epoch self-group message key.
//!
//! Self-group commits carry encrypted `SelfGroupMessages` payloads (settings
//! updates today) under a symmetric key that is scoped to a single self-group
//! epoch. The key is derived from the MLS safe exporter of the T group for
//! [`AIR_COMPONENT_ID`] and then run through one further KDF step.
//!
//! The MLS application export tree is a puncturable PRF: exporting the same
//! component ID twice within one epoch fails, because the first export
//! punctures the tree. That is fine for the sender, but the receive path also
//! needs the key, and in a commit race the losing committer has to decrypt the
//! *winning* commit against the same (pre-merge) epoch it originally derived
//! the key in — a second export. We therefore derive lazily and cache the key
//! in a per-group table: the first access in an epoch performs the single
//! permitted export and persists the result, and every later access in that
//! epoch returns the cached key. The punctured tree and the cached key are
//! written in the same transaction, so they can never diverge.

use aircommon::codec::PersistenceCodec;
use aircommon::{
    credentials::keys::ClientSigningKey,
    crypto::{
        aead::{
            AEAD_KEY_SIZE, PaddedAeadDecryptable, PaddedAeadEncryptable, keys::SelfGroupMessageKey,
        },
        kdf::{KdfDerivable, keys::SelfGroupExporterSecret},
    },
    messages::{
        client_ds::{AadMessage, AadPayload, GroupOperationParamsAad},
        client_ds_out::ApqGroupOperationParamsOut,
    },
};
use airprotos::client::{
    component::{AIR_COMPONENT_ID, AirComponent},
    self_group::{AppEphemeralPayload, SelfGroupMessage, SelfGroupMessages, SettingsUpdate},
};
use anyhow::{Result, anyhow, ensure};
use openmls::prelude::{
    AppEphemeralProposal, Extensions, GroupContext, Proposal, StagedCommit,
    tls_codec::Serialize as _,
};
use openmls_traits::OpenMlsProvider;
use tracing::{debug, warn};

use crate::{
    db::access::WriteDbTransaction,
    groups::{Group, openmls_provider::AirOpenMlsProvider},
};

/// Reads the `is_self_group` flag from a group context's app-data dictionary.
///
/// The flag lives in the [`AirComponent`] entry under [`AIR_COMPONENT_ID`]. A
/// missing component or entry means "not a self-group". Works on any
/// [`Extensions<GroupContext>`], so it can be applied both to a live group's
/// extensions and to the provisional post-commit context of a
/// [`StagedCommit`](openmls::prelude::StagedCommit).
pub(crate) fn extensions_claim_self_group(extensions: &Extensions<GroupContext>) -> bool {
    extensions
        .app_data_dictionary()
        .and_then(|dict| dict.dictionary().get(&AIR_COMPONENT_ID))
        .and_then(|data| AirComponent::from_bytes(data).ok())
        .map(|component| component.is_self_group)
        .unwrap_or(false)
}

impl Group {
    /// Returns whether this group claims to be a self-group, per the
    /// `is_self_group` flag in the AIR component of the T group context.
    ///
    /// The flag lives in group-creator-controlled state. Commit-time
    /// validation rejects any commit that flips it (see
    /// `post_process_staged_commit` in `groups::process`), so a group can
    /// never change its self-group status after the fact. Join-time
    /// validation (rejecting a joined group whose flag disagrees with the
    /// locally known self-group id) is deferred to a follow-up. Until then we
    /// assume an adversary cannot make a client join a non-legitimate
    /// self-group.
    pub(crate) fn is_self_group(&self) -> bool {
        extensions_claim_self_group(self.mls_group().extensions())
    }

    /// Returns the message key for the self-group's current epoch, deriving and
    /// persisting it on first access in an epoch.
    ///
    /// On first access in an epoch this exports `AIR_COMPONENT_ID` from the T
    /// group's safe exporter (which punctures the export tree), derives the
    /// [`SelfGroupMessageKey`], and upserts it into the cache — replacing any
    /// row left over from a previous epoch. Every later access in the same
    /// epoch returns the cached key without re-exporting. See the module-level
    /// docs for why the cache is mandatory rather than an optimization.
    ///
    /// The export tree is persisted by `safe_export_secret` through `txn`, and
    /// the cached key is written through the same `txn`, so they commit
    /// atomically.
    ///
    /// Errors if called on a group that is not the self-group: deriving the AIR
    /// component export for an ordinary chat group would puncture that group's
    /// export tree for nothing. The guard reads the in-memory `is_self_group`
    /// flag (see [`Group::is_self_group`] for its trust model).
    pub(crate) async fn self_group_message_key(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
    ) -> Result<SelfGroupMessageKey> {
        ensure!(
            self.is_self_group(),
            "self_group_message_key must only be called on the self group"
        );

        let group_id = self.group_id().clone();
        let current_epoch = self.mls_group().epoch().as_u64();

        // Return the cached key if it was derived for the current epoch.
        if let Some(stored) = persistence::load(&mut *txn, &group_id).await?
            && stored.epoch == current_epoch
        {
            return Ok(stored.key);
        }

        // Otherwise export from the T group and derive. This punctures the
        // export tree; the punctured state is persisted by `safe_export_secret`
        // through the provider's storage, i.e. through `txn`.
        let key = {
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            let (t_mls_group, _pq_mls_group) = self.apq_mls_groups_mut()?;
            let exporter_bytes = t_mls_group.safe_export_secret(
                provider.crypto(),
                provider.storage(),
                AIR_COMPONENT_ID,
            )?;
            let exporter_bytes: [u8; AEAD_KEY_SIZE] = exporter_bytes
                .try_into()
                .map_err(|_| anyhow!("unexpected self-group exporter secret length"))?;
            let exporter = SelfGroupExporterSecret::from_bytes(exporter_bytes);
            SelfGroupMessageKey::derive(&exporter, &Vec::new())?
            // `exporter` is zeroized here as it is dropped at the end of this
            // scope (its inner secret is `ZeroizeOnDrop`).
        };

        // Upsert the cache, replacing any stale epoch's key.
        persistence::store(&mut *txn, &group_id, current_epoch, &key).await?;

        Ok(key)
    }

    /// Stages a self-group commit that carries the given settings update.
    ///
    /// The update is encrypted under the current-epoch self-group message key
    /// (which enforces the self-group guard), wrapped into an
    /// [`AppEphemeralPayload`], and committed as an `AppEphemeral` proposal on a
    /// forced self-update. The commit shape mirrors `stage_apq_invite` minus the
    /// invitees, so it carries no welcome or attribution infos.
    pub(crate) async fn stage_settings_update(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        signer: &ClientSigningKey,
        update: &SettingsUpdate,
    ) -> Result<ApqGroupOperationParamsOut> {
        // Derive the current-epoch key. This also enforces the self-group guard.
        let key = self.self_group_message_key(txn).await?;

        // Encrypt the update and wrap it into an app-ephemeral payload.
        let messages = SelfGroupMessages(vec![SelfGroupMessage::SettingsUpdate(update.clone())]);
        let encrypted = messages.encrypt_padded(&key)?;
        let payload = AppEphemeralPayload::EncryptedSelfGroupMessages(encrypted);
        let payload_bytes = PersistenceCodec::to_vec(&payload)?;
        let proposal = Proposal::AppEphemeral(Box::new(AppEphemeralProposal::new(
            AIR_COMPONENT_ID,
            payload_bytes,
        )));

        // Set the AAD for a group operation without any added users.
        let aad = AadMessage::from(AadPayload::GroupOperation(GroupOperationParamsAad {
            new_encrypted_user_profile_keys: Vec::new(),
        }))
        .tls_serialize_detached()?;
        self.mls_group.set_aad(aad);

        let provider = AirOpenMlsProvider::new(txn.as_mut());
        let (t_mls_group, pq_mls_group) = self.apq_mls_groups_mut()?;
        let bundle = apqmls::commit_builder::CommitBuilder::from_groups(t_mls_group, pq_mls_group)
            .force_self_update(true)
            .add_t_proposal(proposal)
            .create_group_info(true)
            .finalize(&provider, signer, |_| true, |_| true)?;

        debug_assert!(bundle.welcome.is_none());
        ensure!(
            bundle.group_info.is_some(),
            "No group info in APQMLS bundle"
        );

        Ok(ApqGroupOperationParamsOut {
            bundle,
            encrypted_welcome_attribution_infos: Vec::new(),
        })
    }

    /// Extracts the settings updates carried by a self-group commit.
    ///
    /// This runs on every self-group commit before merge. Content and crypto
    /// problems must never fail commit processing, so it returns a plain Vec:
    /// a malformed or undecryptable payload is logged and skipped rather than
    /// propagated. Older and newer clients must interoperate, and a malformed
    /// payload from a buggy sibling must never wedge the group.
    ///
    /// The self-group message key is derived lazily, at most once per call and
    /// only when there is an encrypted payload to decrypt. If key derivation
    /// fails, the remaining encrypted payloads are skipped.
    pub(crate) async fn extract_settings_updates(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        staged_commit: &StagedCommit,
    ) -> Vec<SettingsUpdate> {
        let mut updates = Vec::new();
        let mut key: Option<SelfGroupMessageKey> = None;

        for proposal in staged_commit.queued_app_ephemeral_proposals() {
            let proposal = proposal.app_ephemeral_proposal();
            if proposal.component_id() != AIR_COMPONENT_ID {
                debug!(
                    component_id = ?proposal.component_id(),
                    "Skipping app-ephemeral proposal for a foreign component id"
                );
                continue;
            }

            let payload = match PersistenceCodec::from_slice::<AppEphemeralPayload>(proposal.data())
            {
                Ok(payload) => payload,
                Err(error) => {
                    warn!(%error, "Failed to decode self-group app-ephemeral payload; skipping");
                    continue;
                }
            };

            let ciphertext = match payload {
                AppEphemeralPayload::EncryptedSelfGroupMessages(ciphertext) => ciphertext,
                // A payload kind added by a newer client. Nothing to do here.
                AppEphemeralPayload::Unknown => {
                    debug!("Skipping unknown self-group app-ephemeral payload");
                    continue;
                }
            };

            // Derive the key lazily on first need. If derivation fails there is
            // no point retrying it for the remaining payloads in this commit.
            if key.is_none() {
                match self.self_group_message_key(txn).await {
                    Ok(derived) => key = Some(derived),
                    Err(error) => {
                        warn!(
                            %error,
                            "Failed to derive self-group message key; skipping encrypted payloads"
                        );
                        break;
                    }
                }
            }
            let Some(message_key) = &key else {
                break;
            };

            let messages = match SelfGroupMessages::decrypt_padded(message_key, &ciphertext) {
                Ok(messages) => messages,
                Err(error) => {
                    warn!(%error, "Failed to decrypt self-group messages; skipping");
                    continue;
                }
            };

            for message in messages.0 {
                match message {
                    SelfGroupMessage::SettingsUpdate(update) => updates.push(update),
                    // A message kind added by a newer client.
                    SelfGroupMessage::Unknown => debug!("Skipping unknown self-group message"),
                }
            }
        }

        updates
    }
}

mod persistence {
    use aircommon::crypto::aead::keys::SelfGroupMessageKey;
    use openmls::group::GroupId;
    use sqlx::query;

    use crate::{
        db::access::{ReadConnection, WriteConnection},
        utils::persistence::GroupIdRefWrapper,
    };

    /// A self-group message key together with the epoch it was derived for.
    pub(super) struct StoredSelfGroupMessageKey {
        pub(super) epoch: u64,
        pub(super) key: SelfGroupMessageKey,
    }

    pub(super) async fn load(
        mut connection: impl ReadConnection,
        group_id: &GroupId,
    ) -> sqlx::Result<Option<StoredSelfGroupMessageKey>> {
        let group_id = GroupIdRefWrapper::from(group_id);
        let row = query!(
            r#"SELECT
                epoch AS "epoch!: i64",
                key AS "key!: SelfGroupMessageKey"
            FROM self_group_message_key
            WHERE group_id = ?"#,
            group_id,
        )
        .fetch_optional(connection.as_mut())
        .await?;
        Ok(row.map(|row| StoredSelfGroupMessageKey {
            epoch: row.epoch as u64,
            key: row.key,
        }))
    }

    pub(super) async fn store(
        mut connection: impl WriteConnection,
        group_id: &GroupId,
        epoch: u64,
        key: &SelfGroupMessageKey,
    ) -> sqlx::Result<()> {
        let group_id = GroupIdRefWrapper::from(group_id);
        let epoch = epoch as i64;
        query!(
            r#"INSERT OR REPLACE INTO self_group_message_key (group_id, epoch, key)
            VALUES (?, ?, ?)"#,
            group_id,
            epoch,
            key,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use aircommon::{RustCrypto, crypto::secrets::Secret};
        use openmls::group::GroupId;
        use sqlx::SqlitePool;

        use crate::db::access::DbAccess;

        use super::*;

        fn random_key() -> SelfGroupMessageKey {
            SelfGroupMessageKey::from(Secret::random().unwrap())
        }

        #[sqlx::test]
        async fn store_and_load(pool: SqlitePool) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);
            let group_id = GroupId::random(&RustCrypto::default());
            let key = random_key();

            assert!(load(pool.read().await?, &group_id).await?.is_none());

            store(pool.write().await?, &group_id, 7, &key).await?;

            let loaded = load(pool.read().await?, &group_id)
                .await?
                .expect("stored key should load");
            assert_eq!(loaded.epoch, 7);
            assert_eq!(loaded.key, key);

            Ok(())
        }

        #[sqlx::test]
        async fn upsert_replaces_on_epoch_change(pool: SqlitePool) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);
            let group_id = GroupId::random(&RustCrypto::default());
            let old_key = random_key();
            let new_key = random_key();

            store(pool.write().await?, &group_id, 1, &old_key).await?;
            store(pool.write().await?, &group_id, 2, &new_key).await?;

            // The stale epoch's key is gone, only the current one remains.
            let loaded = load(pool.read().await?, &group_id)
                .await?
                .expect("stored key should load");
            assert_eq!(loaded.epoch, 2);
            assert_eq!(loaded.key, new_key);

            // Exactly one row per group.
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM self_group_message_key")
                .fetch_one(pool.read().await?.as_mut())
                .await?;
            assert_eq!(count, 1);

            Ok(())
        }
    }
}

#[cfg(test)]
mod derivation_tests {
    use aircommon::{
        codec::PersistenceCodec,
        credentials::{keys::ClientSigningKey, test_utils::create_test_credentials},
        crypto::{
            aead::{
                PaddedAeadDecryptable, PaddedAeadEncryptable,
                keys::{IdentityLinkWrapperKey, SelfGroupMessageKey},
            },
            kdf::{KdfDerivable, keys::SelfGroupExporterSecret},
        },
        identifiers::{QualifiedGroupId, UserId},
        mls_group_config::{AppComponent, default_group_context_app_data_dictionary_extension},
    };
    use airprotos::client::{
        component::{AIR_COMPONENT_ID, AirComponent},
        self_group::{AppEphemeralPayload, SelfGroupMessage, SelfGroupMessages, SettingsUpdate},
    };
    use openmls::prelude::{AppEphemeralProposal, GroupId, Proposal};
    use openmls_traits::OpenMlsProvider;
    use uuid::Uuid;

    use crate::{
        db::access::{DbAccess, WriteConnection, WriteDbTransaction},
        groups::{Group, GroupDataBytes, openmls_provider::AirOpenMlsProvider},
        utils::persistence::open_db_in_memory,
    };

    use super::extensions_claim_self_group;

    fn random_group_id() -> GroupId {
        GroupId::from(QualifiedGroupId::new(
            Uuid::new_v4(),
            "example.com".parse().unwrap(),
        ))
    }

    /// Creates a fresh single-member APQ group. When `is_self_group` is set,
    /// the group context carries `AirComponent::default_for_self_group`, which
    /// is what the accessor's guard checks.
    fn create_group(
        txn: &mut WriteDbTransaction<'_>,
        signer: &ClientSigningKey,
        is_self_group: bool,
    ) -> anyhow::Result<Group> {
        let air_component = if is_self_group {
            AirComponent::default_for_self_group()
        } else {
            AirComponent::default_for_leaf_or_key_package()
        };
        let (group, _params) = Group::create_apq_group(
            &mut *txn,
            signer,
            IdentityLinkWrapperKey::random()?,
            random_group_id(),
            random_group_id(),
            GroupDataBytes::from(b"test-group-data".to_vec()),
            None,
            air_component,
        )?;
        Ok(group)
    }

    /// Advances the group's epoch by staging and merging a forced self-update.
    fn self_update_and_merge(
        group: &mut Group,
        txn: &mut WriteDbTransaction<'_>,
        signer: &ClientSigningKey,
    ) -> anyhow::Result<()> {
        let provider = AirOpenMlsProvider::new(txn.as_mut());
        let (t_mls_group, pq_mls_group) = group.apq_mls_groups_mut()?;
        let _bundle = apqmls::commit_builder::CommitBuilder::from_groups(
            &mut *t_mls_group,
            &mut *pq_mls_group,
        )
        .force_self_update(true)
        .finalize(&provider, signer, |_| true, |_| true)?;
        t_mls_group.merge_pending_commit(&provider)?;
        pq_mls_group.merge_pending_commit(&provider)?;
        Ok(())
    }

    /// THE regression test for the PPRF puncture constraint: two accessor calls
    /// in the same epoch must return the same key and must not error, even
    /// though the underlying safe exporter can only be punctured once per epoch.
    #[tokio::test(flavor = "multi_thread")]
    async fn stable_within_epoch() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let (_as_key, signer) = create_test_credentials(UserId::random("example.com".parse()?));

        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let mut group = create_group(&mut txn, &signer, true)?;

        let key_1 = group.self_group_message_key(&mut txn).await?;
        let key_2 = group.self_group_message_key(&mut txn).await?;
        assert_eq!(key_1, key_2, "same-epoch derivations must match");

        txn.commit().await?;
        Ok(())
    }

    /// The key rotates when the group's epoch changes.
    #[tokio::test(flavor = "multi_thread")]
    async fn rotates_across_epoch() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let (_as_key, signer) = create_test_credentials(UserId::random("example.com".parse()?));

        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let mut group = create_group(&mut txn, &signer, true)?;

        let key_before = group.self_group_message_key(&mut txn).await?;

        self_update_and_merge(&mut group, &mut txn, &signer)?;

        let key_after = group.self_group_message_key(&mut txn).await?;
        assert_ne!(
            key_before, key_after,
            "key must rotate after an epoch change"
        );

        // And it is stable again within the new epoch.
        let key_after_again = group.self_group_message_key(&mut txn).await?;
        assert_eq!(key_after, key_after_again);

        txn.commit().await?;
        Ok(())
    }

    /// Staging a settings update produces a commit that carries exactly one
    /// AppEphemeral proposal with `AIR_COMPONENT_ID`, whose payload decrypts
    /// (with the cached epoch key) back to the original update.
    #[tokio::test(flavor = "multi_thread")]
    async fn stage_settings_update_carries_encrypted_update() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let (_as_key, signer) = create_test_credentials(UserId::random("example.com".parse()?));

        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let mut group = create_group(&mut txn, &signer, true)?;

        let update = SettingsUpdate {
            send_read_receipts: Some(true),
        };
        group
            .stage_settings_update(&mut txn, &signer, &update)
            .await?;

        // Exactly one AppEphemeral proposal with our component id.
        let staged = group
            .mls_group()
            .pending_commit()
            .expect("commit should be staged");
        let proposals: Vec<_> = staged.queued_app_ephemeral_proposals().collect();
        assert_eq!(proposals.len(), 1);
        let proposal = proposals[0].app_ephemeral_proposal();
        assert_eq!(proposal.component_id(), AIR_COMPONENT_ID);

        // The payload decodes and decrypts back to the original update.
        let payload: AppEphemeralPayload = PersistenceCodec::from_slice(proposal.data())?;
        let AppEphemeralPayload::EncryptedSelfGroupMessages(encrypted) = payload else {
            panic!("expected encrypted self-group messages");
        };
        // Re-derive (cached, same epoch) to exercise the cache on the read side.
        let key = group.self_group_message_key(&mut txn).await?;
        let messages = SelfGroupMessages::decrypt_padded(&key, &encrypted)?;
        assert_eq!(messages.0, vec![SelfGroupMessage::SettingsUpdate(update)]);

        txn.commit().await?;
        Ok(())
    }

    /// The accessor refuses to derive for a group that is not the self-group, so
    /// ordinary chat groups never have their export tree punctured for nothing.
    #[tokio::test(flavor = "multi_thread")]
    async fn rejects_non_self_group() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let (_as_key, signer) = create_test_credentials(UserId::random("example.com".parse()?));

        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let mut group = create_group(&mut txn, &signer, false)?;
        assert!(group.self_group_message_key(&mut txn).await.is_err());

        txn.commit().await?;
        Ok(())
    }

    /// `is_self_group` reflects the AIR component flag in the group context: a
    /// group created for the self-group reports true, an ordinary group false.
    #[tokio::test(flavor = "multi_thread")]
    async fn is_self_group_reflects_air_component() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let (_as_key, signer) = create_test_credentials(UserId::random("example.com".parse()?));

        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let self_group = create_group(&mut txn, &signer, true)?;
        assert!(self_group.is_self_group());

        let ordinary_group = create_group(&mut txn, &signer, false)?;
        assert!(!ordinary_group.is_self_group());

        txn.commit().await?;
        Ok(())
    }

    /// A commit that flips the self-group flag is detectable: the live group
    /// context still claims self-group, but the staged commit's provisional
    /// context does not. This is exactly the difference the commit-time guard
    /// in `post_process_staged_commit` rejects. The full guard runs against a
    /// received commit and needs the client/DS stack, so it is covered by the
    /// integration tests; here we exercise the helper on a real staged commit.
    #[tokio::test(flavor = "multi_thread")]
    async fn commit_flipping_self_group_flag_is_detectable() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let (_as_key, signer) = create_test_credentials(UserId::random("example.com".parse()?));

        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let mut group = create_group(&mut txn, &signer, true)?;
        assert!(group.is_self_group());

        // Build a group-context-extensions commit that clears the self-group
        // flag by replacing the AIR component in the app data dictionary.
        let mut flipped = group.mls_group().extensions().clone();
        flipped.add_or_replace(default_group_context_app_data_dictionary_extension(
            AirComponent::default_for_leaf_or_key_package(),
            None,
        ))?;
        {
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            let (t_mls_group, _pq_mls_group) = group.apq_mls_groups_mut()?;
            t_mls_group
                .commit_builder()
                .propose_group_context_extensions(flipped)?
                .load_psks(provider.storage())?
                .build(provider.rand(), provider.crypto(), &signer, |_| true)?
                .stage_commit(&provider)?;
        }

        let staged = group
            .mls_group()
            .pending_commit()
            .expect("commit should be staged");
        assert!(
            extensions_claim_self_group(group.mls_group().extensions()),
            "live context still claims self-group"
        );
        assert!(
            !extensions_claim_self_group(staged.group_context().extensions()),
            "staged commit context drops the self-group flag"
        );

        txn.commit().await?;
        Ok(())
    }

    /// A staged settings update extracts back to the original update. The
    /// receiver is a second handle to the same group, which mirrors the real
    /// receive path (the staged commit comes from a processed message, not the
    /// receiver's own pending commit) and derives the same per-epoch key from
    /// the cache the sender populated.
    #[tokio::test(flavor = "multi_thread")]
    async fn extract_settings_updates_roundtrip() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let (_as_key, signer) = create_test_credentials(UserId::random("example.com".parse()?));

        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let mut group = create_group(&mut txn, &signer, true)?;
        group.store(&mut txn).await?;

        let update = SettingsUpdate {
            send_read_receipts: Some(true),
        };
        group
            .stage_settings_update(&mut txn, &signer, &update)
            .await?;

        let mut receiver = Group::load(&mut txn, group.group_id())
            .await?
            .expect("group stored above");
        let staged = group
            .mls_group()
            .pending_commit()
            .expect("commit should be staged");
        let extracted = receiver.extract_settings_updates(&mut txn, staged).await;

        assert_eq!(extracted, vec![update]);

        txn.commit().await?;
        Ok(())
    }

    /// Extraction never fails commit processing: a proposal whose data is not
    /// valid CBOR and a well-formed but undecryptable payload are both logged
    /// and skipped, and extraction returns an empty Vec without panicking.
    #[tokio::test(flavor = "multi_thread")]
    async fn extract_settings_updates_tolerates_bad_payloads() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let (_as_key, signer) = create_test_credentials(UserId::random("example.com".parse()?));

        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let mut group = create_group(&mut txn, &signer, true)?;
        group.store(&mut txn).await?;

        // Not valid PersistenceCodec CBOR.
        let garbage = Proposal::AppEphemeral(Box::new(AppEphemeralProposal::new(
            AIR_COMPONENT_ID,
            vec![0xff, 0xff, 0xff, 0xff],
        )));

        // Well-formed payload, but encrypted under a key the receiver will
        // never derive, so decryption fails.
        let foreign_key = SelfGroupMessageKey::derive(
            &SelfGroupExporterSecret::from_bytes([7u8; 32]),
            &Vec::new(),
        )?;
        let ciphertext =
            SelfGroupMessages(vec![SelfGroupMessage::SettingsUpdate(SettingsUpdate {
                send_read_receipts: Some(true),
            })])
            .encrypt_padded(&foreign_key)?;
        let undecryptable_bytes =
            PersistenceCodec::to_vec(&AppEphemeralPayload::EncryptedSelfGroupMessages(ciphertext))?;
        let undecryptable = Proposal::AppEphemeral(Box::new(AppEphemeralProposal::new(
            AIR_COMPONENT_ID,
            undecryptable_bytes,
        )));

        {
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            let (t_mls_group, pq_mls_group) = group.apq_mls_groups_mut()?;
            apqmls::commit_builder::CommitBuilder::from_groups(t_mls_group, pq_mls_group)
                .force_self_update(true)
                .add_t_proposals([garbage, undecryptable])
                .create_group_info(true)
                .finalize(&provider, &signer, |_| true, |_| true)?;
        }

        let mut receiver = Group::load(&mut txn, group.group_id())
            .await?
            .expect("group stored above");
        let staged = group
            .mls_group()
            .pending_commit()
            .expect("commit should be staged");
        let extracted = receiver.extract_settings_updates(&mut txn, staged).await;

        assert!(extracted.is_empty());

        txn.commit().await?;
        Ok(())
    }
}
