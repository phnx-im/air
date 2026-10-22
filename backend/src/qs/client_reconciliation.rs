// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashSet;

use aircommon::{
    crypto::hpke::{ClientIdDecryptionKey, HpkeDecryptable},
    identifiers::{ClientConfig, QsClientId, QsReference},
};
use displaydoc::Display;
use sqlx::PgTransaction;
use tracing::warn;

use crate::{messages::intra_backend::ClientReferencesReconciliation, qs::errors::EnqueueError};

use super::{Qs, client_record::QsClientRecord, user_record::UserRecord};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ReconcileOutcome {
    /// The push tokens of this many client records were cleared.
    Reconciled {
        cleared: u64,
    },
    Rejected(ReconcileRejection),
}

#[derive(Debug, Display, PartialEq, Eq)]
pub(super) enum ReconcileRejection {
    /// More client references than the device limit allows
    TooManyClients,
    /// Client reference of another domain
    ForeignClient,
    /// Client reference that names no client record
    UnknownClient,
    /// Client references of more than one user
    MultipleOwners,
    /// Sender of the commit is not among the client references
    SenderNotIncluded,
    /// Owner of the client references has no active user record
    UnknownUser,
    /// Signature does not match the client references
    InvalidSignature,
}

impl Qs {
    /// Clears the push tokens of the user that owns `sender`. We keep the push
    /// tokens of the client records that `reconciliation` references.
    pub(super) async fn reconcile_clients(
        &self,
        txn: &mut PgTransaction<'_>,
        decryption_key: &ClientIdDecryptionKey,
        sender: &QsClientId,
        reconciliation: ClientReferencesReconciliation,
    ) -> Result<ReconcileOutcome, EnqueueError> {
        use ReconcileOutcome::{Reconciled, Rejected};
        use ReconcileRejection::*;

        let ClientReferencesReconciliation {
            confirmed_transcript_hash,
            client_references,
            signature,
        } = reconciliation;
        let client_ids = match self.referenced_client_ids(&client_references, decryption_key) {
            Ok(client_ids) => client_ids,
            Err(rejection) => return Ok(Rejected(rejection)),
        };
        if !client_ids.contains(sender) {
            return Ok(Rejected(SenderNotIncluded));
        }
        let client_ids: Vec<QsClientId> = client_ids.into_iter().collect();

        let owners = QsClientRecord::count_by_owner(txn.as_mut(), &client_ids).await?;
        let owner = match owners.as_slice() {
            [(owner, count)] if usize::try_from(*count) == Ok(client_ids.len()) => *owner,
            [] | [_] => return Ok(Rejected(UnknownClient)),
            [_, _, ..] => return Ok(Rejected(MultipleOwners)),
        };

        let Some(verifying_key) = UserRecord::load_verifying_key_for_update(txn, &owner).await?
        else {
            return Ok(Rejected(UnknownUser));
        };
        if let Err(error) = signature.verify(
            confirmed_transcript_hash,
            client_references,
            decryption_key,
            &verifying_key,
        ) {
            warn!(%error, "Invalid client references signature");
            return Ok(Rejected(InvalidSignature));
        }

        let cleared =
            QsClientRecord::clear_push_tokens_except(txn.as_mut(), &owner, &client_ids).await?;
        Ok(Reconciled { cleared })
    }

    /// The ids of the client records that `client_references` name.
    fn referenced_client_ids(
        &self,
        client_references: &[QsReference],
        decryption_key: &ClientIdDecryptionKey,
    ) -> Result<HashSet<QsClientId>, ReconcileRejection> {
        use ReconcileRejection::*;

        // We only enforce the limit when adding new devices. We cannot shrink
        // the number of devices.
        if let Some(max_devices) = self.max_devices.limit()
            && client_references.len() > max_devices
        {
            return Err(TooManyClients);
        }

        let mut client_ids = HashSet::new();
        for client_reference in client_references {
            if client_reference.client_homeserver_domain != self.domain {
                return Err(ForeignClient);
            }
            let client_config = ClientConfig::decrypt(
                client_reference.sealed_reference.clone(),
                decryption_key,
                &[],
                &[],
            )
            .map_err(|_| UnknownClient)?;
            client_ids.insert(client_config.client_id);
        }
        Ok(client_ids)
    }
}

#[cfg(test)]
pub(super) mod tests {
    use aircommon::{
        crypto::signatures::keys::QsUserSigningKey,
        identifiers::{Fqdn, QsUserId},
        messages::client_references::{ConfirmedTranscriptHash, SealedClientReferencesSignature},
    };
    use sqlx::PgPool;
    use tokio_util::sync::CancellationToken;

    use crate::{
        air_service::{BackendService, MaxDevices},
        qs::{
            client_id_decryption_key::StorableClientIdDecryptionKey,
            client_record::persistence::tests::store_random_client_record,
            user_record::persistence::tests::store_user_record,
        },
    };

    use super::{ReconcileRejection::*, *};

    pub(in crate::qs) async fn init_qs(pool: &PgPool) -> anyhow::Result<Qs> {
        let qs = Qs::initialize(
            pool.clone(),
            "example.com".parse()?,
            Default::default(),
            0,
            CancellationToken::new(),
        )
        .await?;
        Ok(qs)
    }

    pub(in crate::qs) async fn store_user(
        pool: &PgPool,
    ) -> anyhow::Result<(QsUserId, QsUserSigningKey)> {
        let signing_key = QsUserSigningKey::generate()?;
        let user = store_user_record(pool, signing_key.verifying_key().clone()).await?;
        Ok((user.user_id, signing_key))
    }

    async fn decryption_key(pool: &PgPool) -> anyhow::Result<StorableClientIdDecryptionKey> {
        Ok(StorableClientIdDecryptionKey::load(pool)
            .await?
            .expect("missing QS decryption key"))
    }

    pub(in crate::qs) async fn reference(
        qs: &Qs,
        client_id: QsClientId,
    ) -> anyhow::Result<QsReference> {
        let sealed_reference = decryption_key(&qs.db_pool)
            .await?
            .encryption_key()
            .seal_client_config(ClientConfig {
                client_id,
                push_token_ear_key: None,
            })?;
        Ok(QsReference {
            client_homeserver_domain: qs.domain.clone(),
            sealed_reference,
        })
    }

    pub(in crate::qs) async fn signed_reconciliation(
        qs: &Qs,
        client_references: Vec<QsReference>,
        signing_key: &QsUserSigningKey,
    ) -> anyhow::Result<ClientReferencesReconciliation> {
        let confirmed_transcript_hash = ConfirmedTranscriptHash::new_for_test(vec![0x0c; 32]);
        let signature = SealedClientReferencesSignature::seal(
            confirmed_transcript_hash.clone(),
            client_references.clone(),
            signing_key,
            decryption_key(&qs.db_pool).await?.encryption_key(),
        )?;
        Ok(ClientReferencesReconciliation {
            confirmed_transcript_hash,
            client_references,
            signature,
        })
    }

    async fn reconcile(
        qs: &Qs,
        sender: &QsClientId,
        reconciliation: ClientReferencesReconciliation,
    ) -> anyhow::Result<ReconcileOutcome> {
        let decryption_key = decryption_key(&qs.db_pool).await?;
        let mut txn = qs.db_pool.begin().await?;
        let outcome = qs
            .reconcile_clients(&mut txn, &decryption_key, sender, reconciliation)
            .await?;
        txn.commit().await?;
        Ok(outcome)
    }

    #[sqlx::test]
    async fn clears_push_tokens_of_other_clients(pool: PgPool) -> anyhow::Result<()> {
        let qs = init_qs(&pool).await?;
        let (user_id, signing_key) = store_user(&pool).await?;
        let first = store_random_client_record(&pool, user_id).await?;
        let second = store_random_client_record(&pool, user_id).await?;
        let ghost = store_random_client_record(&pool, user_id).await?;
        let (other_user_id, _) = store_user(&pool).await?;
        let other_client = store_random_client_record(&pool, other_user_id).await?;

        let client_references = vec![
            reference(&qs, first.client_id).await?,
            reference(&qs, second.client_id).await?,
            // Sealing a reference uses randomness. Two leaves may therefore
            // name the same client with different references.
            reference(&qs, second.client_id).await?,
        ];
        let reconciliation = signed_reconciliation(&qs, client_references, &signing_key).await?;
        let outcome = reconcile(&qs, &first.client_id, reconciliation).await?;

        assert_eq!(outcome, ReconcileOutcome::Reconciled { cleared: 1 });
        assert!(Qs::client_has_push_token(&pool, &first.client_id).await?);
        assert!(Qs::client_has_push_token(&pool, &second.client_id).await?);
        assert!(!Qs::client_has_push_token(&pool, &ghost.client_id).await?);
        assert!(Qs::client_has_push_token(&pool, &other_client.client_id).await?);
        Ok(())
    }

    #[sqlx::test]
    async fn accepts_deleted_clients(pool: PgPool) -> anyhow::Result<()> {
        let qs = init_qs(&pool).await?;
        let (user_id, signing_key) = store_user(&pool).await?;
        let first = store_random_client_record(&pool, user_id).await?;
        let deleted = store_random_client_record(&pool, user_id).await?;
        let ghost = store_random_client_record(&pool, user_id).await?;
        QsClientRecord::soft_delete(&pool, &deleted.client_id).await?;

        let client_references = vec![
            reference(&qs, first.client_id).await?,
            reference(&qs, deleted.client_id).await?,
        ];
        let reconciliation = signed_reconciliation(&qs, client_references, &signing_key).await?;
        let outcome = reconcile(&qs, &first.client_id, reconciliation).await?;

        assert_eq!(outcome, ReconcileOutcome::Reconciled { cleared: 1 });
        assert!(Qs::client_has_push_token(&pool, &first.client_id).await?);
        assert!(!Qs::client_has_push_token(&pool, &ghost.client_id).await?);
        Ok(())
    }

    #[sqlx::test]
    async fn rejects_signature_of_another_user(pool: PgPool) -> anyhow::Result<()> {
        let qs = init_qs(&pool).await?;
        let (victim_id, _) = store_user(&pool).await?;
        let victim = store_random_client_record(&pool, victim_id).await?;
        let victim_sibling = store_random_client_record(&pool, victim_id).await?;
        let (_, attacker_signing_key) = store_user(&pool).await?;

        // The self group holds only a reference of the victim. An attacker can
        // take this reference from one of the victim's key packages.
        let client_references = vec![reference(&qs, victim.client_id).await?];
        let reconciliation =
            signed_reconciliation(&qs, client_references, &attacker_signing_key).await?;
        let outcome = reconcile(&qs, &victim.client_id, reconciliation).await?;

        assert_eq!(outcome, ReconcileOutcome::Rejected(InvalidSignature));
        assert!(Qs::client_has_push_token(&pool, &victim_sibling.client_id).await?);
        Ok(())
    }

    #[sqlx::test]
    async fn rejects_signature_of_another_commit(pool: PgPool) -> anyhow::Result<()> {
        let qs = init_qs(&pool).await?;
        let (user_id, signing_key) = store_user(&pool).await?;
        let first = store_random_client_record(&pool, user_id).await?;
        let linked_later = store_random_client_record(&pool, user_id).await?;

        // The signature is from before the second device joined. A replay with
        // a later commit would clear the push token of the second device.
        let client_references = vec![reference(&qs, first.client_id).await?];
        let mut reconciliation =
            signed_reconciliation(&qs, client_references, &signing_key).await?;
        reconciliation.confirmed_transcript_hash =
            ConfirmedTranscriptHash::new_for_test(vec![0x0d; 32]);
        let outcome = reconcile(&qs, &first.client_id, reconciliation).await?;

        assert_eq!(outcome, ReconcileOutcome::Rejected(InvalidSignature));
        assert!(Qs::client_has_push_token(&pool, &linked_later.client_id).await?);
        Ok(())
    }

    #[sqlx::test]
    async fn rejects_deleted_user(pool: PgPool) -> anyhow::Result<()> {
        let qs = init_qs(&pool).await?;
        let (user_id, signing_key) = store_user(&pool).await?;
        let first = store_random_client_record(&pool, user_id).await?;
        UserRecord::soft_delete(&pool, user_id).await?;

        let client_references = vec![reference(&qs, first.client_id).await?];
        let reconciliation = signed_reconciliation(&qs, client_references, &signing_key).await?;
        let outcome = reconcile(&qs, &first.client_id, reconciliation).await?;

        assert_eq!(outcome, ReconcileOutcome::Rejected(UnknownUser));
        Ok(())
    }

    #[sqlx::test]
    async fn rejects_clients_of_multiple_users(pool: PgPool) -> anyhow::Result<()> {
        let qs = init_qs(&pool).await?;
        let (user_id, signing_key) = store_user(&pool).await?;
        let first = store_random_client_record(&pool, user_id).await?;
        let ghost = store_random_client_record(&pool, user_id).await?;
        let (other_user_id, _) = store_user(&pool).await?;
        let other_client = store_random_client_record(&pool, other_user_id).await?;

        let client_references = vec![
            reference(&qs, first.client_id).await?,
            reference(&qs, other_client.client_id).await?,
        ];
        let reconciliation = signed_reconciliation(&qs, client_references, &signing_key).await?;
        let outcome = reconcile(&qs, &first.client_id, reconciliation).await?;

        assert_eq!(outcome, ReconcileOutcome::Rejected(MultipleOwners));
        assert!(Qs::client_has_push_token(&pool, &ghost.client_id).await?);
        Ok(())
    }

    #[sqlx::test]
    async fn rejects_unknown_clients(pool: PgPool) -> anyhow::Result<()> {
        let qs = init_qs(&pool).await?;
        let (user_id, signing_key) = store_user(&pool).await?;
        let first = store_random_client_record(&pool, user_id).await?;
        let ghost = store_random_client_record(&pool, user_id).await?;

        let unknown = QsClientId::random(&mut rand::rng());
        let client_references = vec![
            reference(&qs, first.client_id).await?,
            reference(&qs, unknown).await?,
        ];
        let reconciliation = signed_reconciliation(&qs, client_references, &signing_key).await?;
        let outcome = reconcile(&qs, &first.client_id, reconciliation).await?;
        assert_eq!(outcome, ReconcileOutcome::Rejected(UnknownClient));

        let other_decryption_key = ClientIdDecryptionKey::generate()?;
        let undecryptable = QsReference {
            client_homeserver_domain: qs.domain.clone(),
            sealed_reference: other_decryption_key.encryption_key().seal_client_config(
                ClientConfig {
                    client_id: ghost.client_id,
                    push_token_ear_key: None,
                },
            )?,
        };
        let client_references = vec![reference(&qs, first.client_id).await?, undecryptable];
        let reconciliation = signed_reconciliation(&qs, client_references, &signing_key).await?;
        let outcome = reconcile(&qs, &first.client_id, reconciliation).await?;
        assert_eq!(outcome, ReconcileOutcome::Rejected(UnknownClient));

        let mut foreign = reference(&qs, ghost.client_id).await?;
        foreign.client_homeserver_domain = "other.example.com".parse::<Fqdn>()?;
        let client_references = vec![reference(&qs, first.client_id).await?, foreign];
        let reconciliation = signed_reconciliation(&qs, client_references, &signing_key).await?;
        let outcome = reconcile(&qs, &first.client_id, reconciliation).await?;
        assert_eq!(outcome, ReconcileOutcome::Rejected(ForeignClient));

        assert!(Qs::client_has_push_token(&pool, &ghost.client_id).await?);
        Ok(())
    }

    #[sqlx::test]
    async fn rejects_sender_outside_references(pool: PgPool) -> anyhow::Result<()> {
        let qs = init_qs(&pool).await?;
        let (user_id, signing_key) = store_user(&pool).await?;
        let first = store_random_client_record(&pool, user_id).await?;
        let removed = store_random_client_record(&pool, user_id).await?;

        let client_references = vec![reference(&qs, first.client_id).await?];
        let reconciliation = signed_reconciliation(&qs, client_references, &signing_key).await?;
        let outcome = reconcile(&qs, &removed.client_id, reconciliation).await?;

        assert_eq!(outcome, ReconcileOutcome::Rejected(SenderNotIncluded));

        let reconciliation = signed_reconciliation(&qs, Vec::new(), &signing_key).await?;
        let outcome = reconcile(&qs, &first.client_id, reconciliation).await?;
        assert_eq!(outcome, ReconcileOutcome::Rejected(SenderNotIncluded));

        assert!(Qs::client_has_push_token(&pool, &removed.client_id).await?);
        Ok(())
    }

    #[sqlx::test]
    async fn rejects_more_clients_than_the_device_limit(pool: PgPool) -> anyhow::Result<()> {
        let mut qs = init_qs(&pool).await?;
        qs.max_devices = MaxDevices::new(2);
        let (user_id, signing_key) = store_user(&pool).await?;
        let first = store_random_client_record(&pool, user_id).await?;
        let second = store_random_client_record(&pool, user_id).await?;
        let third = store_random_client_record(&pool, user_id).await?;
        let ghost = store_random_client_record(&pool, user_id).await?;

        let client_references = vec![
            reference(&qs, first.client_id).await?,
            reference(&qs, second.client_id).await?,
            reference(&qs, third.client_id).await?,
        ];
        let reconciliation = signed_reconciliation(&qs, client_references, &signing_key).await?;
        let outcome = reconcile(&qs, &first.client_id, reconciliation).await?;
        assert_eq!(outcome, ReconcileOutcome::Rejected(TooManyClients));
        assert!(Qs::client_has_push_token(&pool, &ghost.client_id).await?);

        let client_references = vec![
            reference(&qs, first.client_id).await?,
            reference(&qs, second.client_id).await?,
        ];
        let reconciliation = signed_reconciliation(&qs, client_references, &signing_key).await?;
        let outcome = reconcile(&qs, &first.client_id, reconciliation).await?;
        assert_eq!(outcome, ReconcileOutcome::Reconciled { cleared: 2 });
        Ok(())
    }

    #[sqlx::test]
    async fn accepts_any_client_count_without_device_limit(pool: PgPool) -> anyhow::Result<()> {
        let mut qs = init_qs(&pool).await?;
        qs.max_devices = MaxDevices::new(0);
        let (user_id, signing_key) = store_user(&pool).await?;
        let first = store_random_client_record(&pool, user_id).await?;
        let ghost = store_random_client_record(&pool, user_id).await?;

        let client_references = vec![reference(&qs, first.client_id).await?; 20];
        let reconciliation = signed_reconciliation(&qs, client_references, &signing_key).await?;
        let outcome = reconcile(&qs, &first.client_id, reconciliation).await?;
        assert_eq!(outcome, ReconcileOutcome::Reconciled { cleared: 1 });
        assert!(!Qs::client_has_push_token(&pool, &ghost.client_id).await?);
        Ok(())
    }
}
