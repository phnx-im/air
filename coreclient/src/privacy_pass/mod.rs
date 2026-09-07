// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{BTreeSet, HashMap},
    time::Duration,
};

use airapiclient::{ApiClient, as_api::TokenBatchResponse};
use aircommon::{
    credentials::keys::UserSigningKey,
    identifiers::{Fqdn, UserId},
    messages::client_as::{
        BatchedTokenKeyResponse, SerializedToken, SerializedTokenRequest, SerializedTokenResponse,
    },
};
use airprotos::{auth_service::v1::OperationType, client::self_group::TokenSeed};
use anyhow::Context;
use chrono::{DateTime, Utc};
use privacypass::{
    TokenType,
    amortized_tokens::{AmortizedBatchTokenRequest, AmortizedBatchTokenResponse},
    auth::authenticate::TokenChallenge,
    common::private::{
        PublicKey, deserialize_public_key, public_key_to_truncated_token_key_id,
        serialize_public_key,
    },
    private_tokens::Ristretto255,
};
use rand::TryRng;
use sha2::{Digest, Sha256};
use tls_codec::{Deserialize, Serialize};
use tokio::time;
use tracing::{debug, info, warn};

use crate::{
    clients::own_client_info::OwnClientInfo,
    db::access::{DbAccess, ReadConnection, WriteConnection, WriteDbTransaction},
    groups::self_group::SelfGroup,
};

use self::derivation::SEED_LEN;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct TokenId {
    pub id: i64,
    pub created_at: DateTime<Utc>,
}

pub(crate) mod derivation;
pub(crate) mod persistence;

/// SHA-256 of the serialized VOPRF public key.
///
/// Identifies a key epoch. The truncated one-byte token key ID cannot: it
/// recurs across key generations.
type KeyFingerprint = [u8; 32];

/// Result of a replenishment run, reduced to what the caller has to decide:
/// whether to come back soon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplenishOutcome {
    /// The cache holds everything this allowance epoch grants.
    Settled,
    /// Something still has to converge, so the caller should retry soon.
    RetrySoon,
}

/// Result of trying to make the current allowance epoch's batch available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenBatchOutcome {
    /// The batch was fetched and stored.
    Fetched { tokens: usize },
    /// The batch of this allowance epoch was already fetched.
    AlreadyFetched,
    /// The user's devices have not agreed on a token seed for the current key
    /// yet. A proposal is stored, and the outbound service carries it to the
    /// siblings on a self-group commit.
    AwaitingSeedAgreement,
    /// The server rejected the claimed allowance epoch and reported its own.
    EpochRejected { current_epoch: u32 },
    /// The server holds a different request for this allowance epoch, so the
    /// user's devices derived diverging seeds.
    Conflict,
}

/// Makes tokens available for `operation_type`.
///
/// Issuance is idempotent: the request is derived from a token seed the user's
/// devices agree on, so the AS answers a repeat of it for free and every device
/// finalizes byte-identical tokens. Nothing is metered by a counter, which is
/// what makes a rotation recovery and a retry free.
pub(crate) async fn replenish(
    db: &DbAccess,
    api_client: &ApiClient,
    user_id: UserId,
    signing_key: &UserSigningKey,
    operation_type: OperationType,
) -> anyhow::Result<ReplenishOutcome> {
    let outcome = ensure_token_batch(db, api_client, user_id, signing_key, operation_type).await?;

    Ok(match outcome {
        TokenBatchOutcome::Fetched { .. } | TokenBatchOutcome::AlreadyFetched => {
            ReplenishOutcome::Settled
        }
        TokenBatchOutcome::AwaitingSeedAgreement => {
            debug!(%operation_type, "waiting for the devices to agree on a token seed");
            ReplenishOutcome::RetrySoon
        }
        TokenBatchOutcome::EpochRejected { current_epoch } => {
            warn!(
                %operation_type,
                current_epoch,
                "server rejected the claimed allowance epoch"
            );
            ReplenishOutcome::RetrySoon
        }
        TokenBatchOutcome::Conflict => {
            // A sibling locked this allowance epoch under a seed we do not
            // hold, which seed agreement is meant to rule out. Retrying cannot
            // help: the AS keeps answering the request it already registered
            // until the epoch rolls over or the key rotates. The merge rule
            // converges the seeds in the meantime, so the next epoch is fetched
            // under the agreed one.
            warn!(
                %operation_type,
                "another device locked this allowance epoch under a different token seed"
            );
            ReplenishOutcome::Settled
        }
    })
}

/// Whether this device is the only one that could hold a token seed.
///
/// A lone device commits the seed it generates without an agreement round: it is
/// the only seed there is, and a device linked later receives it in its
/// provisioning package. Anything short of a counted membership answers no,
/// which costs an agreement round and never risks two devices locking the same
/// allowance epoch to different requests.
async fn is_alone(db: &DbAccess) -> anyhow::Result<bool> {
    let mut read = db.read().await?;
    if OwnClientInfo::load_self_group_id(&mut read)
        .await?
        .is_none()
    {
        // No self group at all: no other device to diverge from, and no channel
        // to agree over either.
        return Ok(true);
    }

    let Some(self_group) = SelfGroup::load(&mut read).await? else {
        // A self group we have not joined yet, as during linking.
        debug!("self group not loaded, proposing the token seed rather than committing it");
        return Ok(false);
    };
    match self_group.client_ids() {
        Ok(client_ids) => Ok(client_ids.len() <= 1),
        Err(error) => {
            warn!(%error, "cannot count linked devices, proposing the token seed");
            Ok(false)
        }
    }
}

/// A deterministically derived token request and the state to finalize its
/// response.
struct BatchRequest {
    request: SerializedTokenRequest,
    state: TokenState,
}

/// Builds the full-allowance token request of an allowance epoch.
///
/// Pure: everything the request depends on is an argument, which is what makes
/// the requests of two devices byte-identical. `public_key_bytes` is the
/// serialized VOPRF public key as the AS advertised it.
fn build_batch_request(
    public_key_bytes: &[u8],
    operation_type: OperationType,
    token_seed: &[u8; SEED_LEN],
    allowance_epoch: u32,
    domain: &str,
) -> anyhow::Result<BatchRequest> {
    let public_key: PublicKey<Ristretto255> =
        deserialize_public_key::<Ristretto255>(public_key_bytes)
            .map_err(|_| anyhow::anyhow!("failed to deserialize VOPRF public key"))?;
    let token_key_id = public_key_to_truncated_token_key_id::<Ristretto255>(&public_key);
    let fingerprint = key_fingerprint(&public_key);

    let batch_seed = derivation::batch_seed(
        token_seed,
        operation_type_value(operation_type),
        &fingerprint,
        allowance_epoch,
    );

    // The whole allowance, always: identical batch sizes are a precondition for
    // byte-identical requests, and uniform sizes stop the request from leaking
    // how many tokens are left in the cache.
    let batch_size = operation_type.max_tokens_allowance();
    let mut nonces = Vec::with_capacity(usize::from(batch_size));
    let mut blinds = Vec::with_capacity(usize::from(batch_size));
    for index in 0..u32::from(batch_size) {
        nonces.push(derivation::nonce(&batch_seed, index));
        blinds.push(derivation::blind(&batch_seed, index));
    }

    let origins = [domain.to_string()];
    let challenge = TokenChallenge::new(TokenType::PrivateRistretto255, domain, None, &origins);

    let (token_request, token_state) =
        AmortizedBatchTokenRequest::<Ristretto255>::issue_token_request_with_params(
            public_key, &challenge, nonces, blinds,
        )
        .context("failed to construct deterministic token request")?;

    let request = SerializedTokenRequest::new(
        token_request
            .tls_serialize_detached()
            .context("failed to serialize token request")?,
    );
    Ok(BatchRequest {
        request,
        state: TokenState {
            inner: token_state,
            token_key_id,
        },
    })
}

/// Everything a batch fetch needs that does not depend on the allowance epoch.
struct BatchFetch<'a> {
    db: &'a DbAccess,
    api_client: &'a ApiClient,
    user_id: &'a UserId,
    signing_key: &'a UserSigningKey,
    operation_type: OperationType,
    public_key_bytes: &'a [u8],
    fingerprint: &'a KeyFingerprint,
    token_seed: [u8; SEED_LEN],
}

/// Makes sure the batch of the current allowance epoch is stored locally.
///
/// Idempotent by construction: the request is derived from the token seed, so a
/// repeat of it is answered by the AS for free and finalizes to the tokens that
/// are already stored.
async fn ensure_token_batch(
    db: &DbAccess,
    api_client: &ApiClient,
    user_id: UserId,
    signing_key: &UserSigningKey,
    operation_type: OperationType,
) -> anyhow::Result<TokenBatchOutcome> {
    let keys = persistence::load_batched_token_keys(db.read().await?, operation_type).await?;
    let (_, public_key_bytes) = keys.first().context("no VOPRF public keys available")?;
    let public_key: PublicKey<Ristretto255> =
        deserialize_public_key::<Ristretto255>(public_key_bytes)
            .map_err(|_| anyhow::anyhow!("failed to deserialize VOPRF public key"))?;
    let fingerprint = key_fingerprint(&public_key);
    let Some(token_seed) = resolve_seed(db, operation_type, &fingerprint).await? else {
        return Ok(TokenBatchOutcome::AwaitingSeedAgreement);
    };

    let fetch = BatchFetch {
        db,
        api_client,
        user_id: &user_id,
        signing_key,
        operation_type,
        public_key_bytes,
        fingerprint: &fingerprint,
        token_seed,
    };

    let allowance_epoch = operation_type.allowance_epoch_at(Utc::now());
    let outcome = fetch_epoch_batch(&fetch, allowance_epoch).await?;
    if let TokenBatchOutcome::EpochRejected { current_epoch } = outcome
        && current_epoch != allowance_epoch
    {
        // The server clock is authoritative for allowance epochs, so we adopt
        // the epoch it reports. Deriving the epoch again from a local clock that
        // is off by more than one epoch may never converge.
        return fetch_epoch_batch(&fetch, current_epoch).await;
    }
    Ok(outcome)
}

/// Fetches and stores the batch of a single allowance epoch.
async fn fetch_epoch_batch(
    fetch: &BatchFetch<'_>,
    allowance_epoch: u32,
) -> anyhow::Result<TokenBatchOutcome> {
    let BatchFetch {
        db,
        api_client,
        user_id,
        signing_key,
        operation_type,
        public_key_bytes,
        fingerprint,
        token_seed,
    } = fetch;
    let operation_type = *operation_type;

    if persistence::batch_was_fetched(
        db.read().await?,
        operation_type,
        fingerprint,
        allowance_epoch,
    )
    .await?
    {
        return Ok(TokenBatchOutcome::AlreadyFetched);
    }

    debug!(%operation_type, allowance_epoch, "requesting privacy pass token batch");
    let domain = user_id.domain().to_string();
    let BatchRequest { request, state } = build_batch_request(
        public_key_bytes,
        operation_type,
        token_seed,
        allowance_epoch,
        &domain,
    )?;

    let response = match api_client
        .as_issue_token_batch(
            operation_type,
            (*user_id).clone(),
            signing_key,
            allowance_epoch,
            request,
        )
        .await?
    {
        TokenBatchResponse::Issued(response) => response,
        TokenBatchResponse::IssuanceConflict => return Ok(TokenBatchOutcome::Conflict),
        TokenBatchResponse::InvalidAllowanceEpoch { current_epoch } => {
            return Ok(TokenBatchOutcome::EpochRejected { current_epoch });
        }
    };

    let token_response =
        AmortizedBatchTokenResponse::<Ristretto255>::tls_deserialize_exact(response.as_bytes())
            .context("failed to deserialize token response")?;
    let tokens = token_response
        .issue_tokens(&state.inner)
        .context("failed to issue tokens")?;
    let tokens: Vec<Vec<u8>> = tokens
        .into_iter()
        .map(|token| token.tls_serialize_detached())
        .collect::<Result<_, _>>()
        .context("failed to serialize tokens")?;

    store_batch_tokens(
        db,
        operation_type,
        state.token_key_id,
        fingerprint,
        allowance_epoch,
        &tokens,
    )
    .await?;

    Ok(TokenBatchOutcome::Fetched {
        tokens: tokens.len(),
    })
}

/// Returns the agreed token seed of a key, starting the agreement if there is
/// none yet.
///
/// Only a committed seed derives a request. A device with siblings stores a fresh
/// seed as a proposal and returns `None`: the outbound service carries the
/// proposal to the siblings on a self-group commit, and DS commit order decides
/// which proposal becomes the agreed seed. Deriving from an unagreed seed would
/// lock the allowance epoch to a request the losing device's siblings never
/// send, starving one of them for the rest of it.
async fn resolve_seed(
    db: &DbAccess,
    operation_type: OperationType,
    fingerprint: &KeyFingerprint,
) -> anyhow::Result<Option<[u8; SEED_LEN]>> {
    if let Some(stored) =
        persistence::load_seed(db.read().await?, operation_type, fingerprint).await?
    {
        return match stored.state {
            SeedState::Committed => Ok(Some(stored.seed)),
            // A proposal left over from a time when this device had a sibling.
            // Alone there is nobody left to disagree, so the proposal is the
            // seed and waiting for a commit round would only withhold tokens.
            SeedState::Proposed if is_alone(db).await? => {
                persistence::mark_seed_committed(
                    db.write().await?,
                    operation_type,
                    fingerprint,
                    &stored.seed,
                )
                .await?;
                info!(%operation_type, "committing a token seed proposal, this device is alone");
                Ok(Some(stored.seed))
            }
            SeedState::Proposed => Ok(None),
        };
    }

    let mut candidate = [0u8; SEED_LEN];
    rand::rng()
        .try_fill_bytes(&mut candidate)
        .map_err(|error| anyhow::anyhow!("failed to generate a token seed: {error}"))?;

    let state = if is_alone(db).await? {
        SeedState::Committed
    } else {
        SeedState::Proposed
    };
    // Set once, so two concurrent runs converge on one seed instead of each
    // deriving from its own candidate.
    let stored = persistence::insert_seed(
        db.write().await?,
        operation_type,
        fingerprint,
        &candidate,
        state,
    )
    .await?;
    if stored.seed == candidate {
        info!(%operation_type, ?state, "generated a token seed for the current VOPRF key");
    } else {
        debug!(%operation_type, "adopting a concurrently stored token seed");
    }
    Ok(stored.committed())
}

/// The token seeds that still have to go out on a self-group commit: fresh
/// proposals, and seeds that won a divergence and are re-broadcast so the holder
/// of the losing seed converges.
pub(crate) async fn seeds_to_broadcast(
    connection: impl ReadConnection,
) -> sqlx::Result<Vec<TokenSeed>> {
    Ok(persistence::load_seeds_needing_broadcast(connection)
        .await?
        .into_iter()
        .map(SeedRecord::to_wire)
        .collect())
}

/// Every agreed token seed, for the snapshot a newly linked device receives.
///
/// Without it the joining device would have to run an agreement round for a key
/// its sibling already committed, and would lose that round against an allowance
/// epoch the sibling has already locked.
pub(crate) async fn committed_seeds(
    connection: impl ReadConnection,
) -> sqlx::Result<Vec<TokenSeed>> {
    Ok(persistence::load_committed_seeds(connection)
        .await?
        .into_iter()
        .map(SeedRecord::to_wire)
        .collect())
}

/// Stores the token seeds a newly linked device received in its provisioning
/// package. They are agreed by construction, because the provisioner had them
/// committed.
pub(crate) async fn store_provisioned_seeds(
    txn: &mut WriteDbTransaction<'_>,
    seeds: &[TokenSeed],
) -> anyhow::Result<()> {
    for seed in seeds {
        let Some(record) = SeedRecord::from_wire(seed) else {
            warn!("skipping a malformed token seed in the provisioning package");
            continue;
        };
        persistence::adopt_seed(
            &mut *txn,
            record.operation_type,
            &record.key_fingerprint,
            &record.seed,
        )
        .await?;
    }
    Ok(())
}

/// Applies a token seed a sibling published on a self-group commit.
///
/// A seed is set-once with first-writer-wins, and DS commit order decides who is
/// first: a proposal of ours that an incoming seed covers is given up for it,
/// whatever its value. Two *committed* seeds can only diverge when a device
/// joined without a snapshot to seed it, and then the lower seed wins and its
/// holder re-broadcasts it so the other side converges too.
pub(crate) async fn apply_incoming_seed(
    txn: &mut WriteDbTransaction<'_>,
    incoming: &TokenSeed,
) -> anyhow::Result<()> {
    let Some(record) = SeedRecord::from_wire(incoming) else {
        warn!("skipping a malformed incoming token seed");
        return Ok(());
    };
    let SeedRecord {
        operation_type,
        key_fingerprint,
        seed,
    } = record;

    let stored = persistence::load_seed(&mut *txn, operation_type, &key_fingerprint).await?;
    let Some(stored) = stored else {
        persistence::adopt_seed(&mut *txn, operation_type, &key_fingerprint, &seed).await?;
        info!(%operation_type, "adopted a sibling's token seed");
        return Ok(());
    };

    if stored.state == SeedState::Proposed {
        persistence::adopt_seed(&mut *txn, operation_type, &key_fingerprint, &seed).await?;
        info!(%operation_type, "a sibling's token seed covered our proposal");
    } else if stored.seed == seed {
        // Converged. This also retires a re-broadcast that has done its job.
        persistence::clear_needs_broadcast(&mut *txn, operation_type, &key_fingerprint, &seed)
            .await?;
    } else if seed < stored.seed {
        persistence::adopt_seed(&mut *txn, operation_type, &key_fingerprint, &seed).await?;
        warn!(%operation_type, "adopted a sibling's lower diverging token seed");
    } else {
        persistence::mark_needs_broadcast(&mut *txn, operation_type, &key_fingerprint).await?;
        warn!(%operation_type, "keeping the lower token seed and re-broadcasting it");
    }
    Ok(())
}

/// Completes the token seeds a commit of ours asserted, now that the DS accepted
/// it. A proposal becomes the agreed seed; a re-broadcast has served its purpose.
pub(crate) async fn complete_sent_seeds(
    txn: &mut WriteDbTransaction<'_>,
    sent: &[TokenSeed],
) -> anyhow::Result<()> {
    for record in sent.iter().filter_map(SeedRecord::from_wire) {
        let committed = persistence::mark_seed_committed(
            &mut *txn,
            record.operation_type,
            &record.key_fingerprint,
            &record.seed,
        )
        .await?;
        if committed {
            info!(operation_type = %record.operation_type, "token seed agreed");
        } else {
            persistence::clear_needs_broadcast(
                &mut *txn,
                record.operation_type,
                &record.key_fingerprint,
                &record.seed,
            )
            .await?;
        }
    }
    Ok(())
}

/// Drops the proposals a terminally failed commit carried, so a later run
/// proposes again. A seed the devices have meanwhile agreed on does not match
/// and survives.
pub(crate) async fn roll_back_sent_seeds(
    txn: &mut WriteDbTransaction<'_>,
    sent: &[TokenSeed],
) -> anyhow::Result<()> {
    for record in sent.iter().filter_map(SeedRecord::from_wire) {
        persistence::delete_proposed_seed(
            &mut *txn,
            record.operation_type,
            &record.key_fingerprint,
            &record.seed,
        )
        .await?;
    }
    Ok(())
}

/// Stores a finalized batch and records that the epoch's batch was fetched.
///
/// Retries while the database is locked: the AS answers a repeat of the request
/// for free, but only until the key rotates, so losing tokens here is worth
/// avoiding.
async fn store_batch_tokens(
    db: &DbAccess,
    operation_type: OperationType,
    token_key_id: u8,
    fingerprint: &KeyFingerprint,
    allowance_epoch: u32,
    tokens: &[Vec<u8>],
) -> anyhow::Result<()> {
    // TODO: Refactor and use a crate or an abstraction for this.
    const MAX_RETRIES: usize = 10;
    const RETRY_DELAY: Duration = Duration::from_secs(1);
    let mut retries = 0;
    loop {
        let res = db
            .with_write_transaction(async |txn| -> sqlx::Result<()> {
                for (index, token) in (0u16..).zip(tokens) {
                    persistence::store_batch_token(
                        &mut *txn,
                        operation_type,
                        token_key_id,
                        allowance_epoch,
                        index,
                        token,
                    )
                    .await?;
                }
                persistence::store_batch(&mut *txn, operation_type, fingerprint, allowance_epoch)
                    .await
            })
            .await;
        match res {
            Ok(()) => {
                info!(
                    stored = tokens.len(),
                    %operation_type,
                    allowance_epoch,
                    "stored privacy pass token batch"
                );
                return Ok(());
            }
            Err(error) => {
                const DB_LOCKED_CODE: &str = "5"; // SQLITE_BUSY
                let is_db_locked = error
                    .as_database_error()
                    .is_some_and(|e| e.code().as_deref() == Some(DB_LOCKED_CODE));
                if !is_db_locked {
                    return Err(error.into());
                }
                retries += 1;
                if retries >= MAX_RETRIES {
                    return Err(error.into());
                }
                warn!(
                    retries,
                    retry_in =? RETRY_DELAY,
                    "Database is locked when storing privacy pass tokens"
                );
            }
        }

        time::sleep(RETRY_DELAY).await;
    }
}

/// Discards every token, seed and batch record, as a VOPRF key rotation does.
///
/// Exposed for tests, which need the state a rotation leaves behind (no seed for
/// the current key on any device) without driving a rotation on the AS.
#[cfg(any(test, feature = "test_utils"))]
pub(crate) async fn reset_for_key_rotation(db: &DbAccess) -> anyhow::Result<()> {
    let mut records = persistence::load_committed_seeds(db.read().await?).await?;
    records.extend(persistence::load_seeds_needing_broadcast(db.read().await?).await?);

    db.with_write_transaction(async |txn| -> sqlx::Result<()> {
        for record in &records {
            persistence::delete_seed(&mut *txn, record.operation_type, &record.key_fingerprint)
                .await?;
            persistence::delete_batches_for_key(
                &mut *txn,
                record.operation_type,
                &record.key_fingerprint,
            )
            .await?;
        }
        for operation_type in OperationType::all() {
            persistence::delete_all_tokens(&mut *txn, operation_type).await?;
            persistence::delete_all_batches(&mut *txn, operation_type).await?;
        }
        Ok(())
    })
    .await?;
    Ok(())
}

/// The cached tokens of an operation type, in consumption order.
#[cfg(any(test, feature = "test_utils"))]
pub(crate) async fn cached_tokens(
    mut connection: impl ReadConnection,
    operation_type: OperationType,
) -> sqlx::Result<Vec<Vec<u8>>> {
    let mut ids = persistence::load_token_ids(&mut connection, operation_type).await?;
    // Consumption is FIFO by row id, which the id order reproduces.
    ids.sort_by_key(|id| id.id);

    let mut tokens = Vec::with_capacity(ids.len());
    for id in &ids {
        if let Some(token) = TokenId::load(&mut connection, id).await? {
            tokens.push(token.into_bytes());
        }
    }
    Ok(tokens)
}

/// Consumes one token from local storage.
pub(crate) async fn consume_token(
    connection: impl WriteConnection,
    operation_type: OperationType,
) -> anyhow::Result<Option<SerializedToken>> {
    Ok(persistence::consume_token(connection, operation_type)
        .await?
        .map(SerializedToken::new))
}

/// Stores batched token keys received from the AS credentials response.
///
/// Cached tokens whose issuing key is no longer advertised are discarded,
/// because the AS can no longer redeem them. Tokens issued under a key that is
/// still advertised survive the rotation overlap window.
pub(crate) async fn store_batched_token_keys(
    txn: &mut WriteDbTransaction<'_>,
    keys: &[BatchedTokenKeyResponse],
) -> anyhow::Result<()> {
    let keys: HashMap<OperationType, Vec<(_, _, _)>> =
        keys.iter().fold(HashMap::new(), |mut keys, key| {
            if let Ok(operation_type) = OperationType::try_from(key.operation_type) {
                keys.entry(operation_type).or_default().push((
                    key.token_key_id,
                    key.public_key.as_slice(),
                    key.is_current,
                ));
            }
            keys
        });

    for (operation_type, keys) in keys {
        let existing = persistence::load_batched_token_keys(&mut *txn, operation_type).await?;

        let discarded =
            discard_tokens_of_removed_keys(txn, operation_type, &existing, &keys).await?;
        if discarded > 0 {
            info!(
                %operation_type,
                %discarded,
                "VOPRF key set changed, discarding tokens of removed keys"
            );
        }

        discard_seeds_of_removed_keys(txn, operation_type, &existing, &keys).await?;

        // Re-store unconditionally: an unchanged key ID set can still come with
        // a different current key.
        persistence::delete_all_batched_token_keys(&mut *txn, operation_type).await?;
        for (token_key_id, public_key, is_current) in keys {
            persistence::store_batched_token_key(
                &mut *txn,
                token_key_id,
                operation_type,
                public_key,
                is_current,
            )
            .await?;
        }
    }
    Ok(())
}

/// Deletes the cached tokens of every key that is no longer advertised or was
/// replaced by different key material under a reused truncated ID.
///
/// The one-byte ID recurs across key generations, so an ID match alone does not
/// prove the key survived. A stale token fails redemption with InvalidToken
/// rather than UnknownKeyId, which bypasses the purge recovery, so it has to go
/// as soon as its key material changes.
///
/// Returns the number of discarded tokens.
async fn discard_tokens_of_removed_keys(
    txn: &mut WriteDbTransaction<'_>,
    operation_type: OperationType,
    existing: &[(u8, Vec<u8>)],
    keys: &[(u8, &[u8], bool)],
) -> sqlx::Result<u64> {
    let mut discarded = 0;
    for token_key_id in persistence::load_token_key_ids(&mut *txn, operation_type).await? {
        let advertised = keys
            .iter()
            .find_map(|(id, public_key, _)| (*id == token_key_id).then_some(*public_key));
        let stored = existing
            .iter()
            .find_map(|(id, public_key)| (*id == token_key_id).then_some(public_key.as_slice()));
        let keep = match (advertised, stored) {
            // The ID is no longer advertised, so the AS cannot redeem these.
            (None, _) => false,
            (Some(new_key), Some(old_key)) => new_key == old_key,
            // Nothing stored to compare against, so the advertised ID decides.
            (Some(_), None) => true,
        };
        if !keep {
            discarded +=
                persistence::delete_tokens_for_key(&mut *txn, operation_type, token_key_id).await?;
        }
    }
    Ok(discarded)
}

/// Deletes the token seed and the batch records of every key that is no longer
/// advertised.
///
/// Keys are matched by fingerprint, so a key whose public key changed under an
/// unchanged truncated ID also loses its seed.
async fn discard_seeds_of_removed_keys(
    txn: &mut WriteDbTransaction<'_>,
    operation_type: OperationType,
    existing: &[(u8, Vec<u8>)],
    keys: &[(u8, &[u8], bool)],
) -> anyhow::Result<()> {
    let advertised: BTreeSet<KeyFingerprint> = keys
        .iter()
        .filter_map(|(_, public_key, _)| fingerprint_of_bytes(public_key))
        .collect();

    for (_, public_key) in existing {
        let Some(fingerprint) = fingerprint_of_bytes(public_key) else {
            continue;
        };
        if advertised.contains(&fingerprint) {
            continue;
        }
        info!(%operation_type, "discarding seed and batch records of a removed VOPRF key");
        persistence::delete_seed(&mut *txn, operation_type, &fingerprint).await?;
        persistence::delete_batches_for_key(&mut *txn, operation_type, &fingerprint).await?;
    }
    Ok(())
}

/// Creates a single-token request for use in `DeleteHandle`.
///
/// Returns the serialized request bytes and the token state needed to finalize
/// the response. Randomized, unlike batch issuance: the refund is not metered by
/// the allowance, so there is nothing to make idempotent.
pub(crate) async fn prepare_delete_token_request(
    connection: impl ReadConnection,
    domain: &Fqdn,
) -> anyhow::Result<Option<(SerializedTokenRequest, TokenState)>> {
    let keys = persistence::load_batched_token_keys(connection, OperationType::AddUsername).await?;
    let Some((_, pk_bytes)) = keys.first() else {
        return Ok(None);
    };

    let public_key: PublicKey<Ristretto255> = deserialize_public_key::<Ristretto255>(pk_bytes)
        .map_err(|_| anyhow::anyhow!("failed to deserialize VOPRF public key"))?;
    let token_key_id = public_key_to_truncated_token_key_id::<Ristretto255>(&public_key);

    let domain = domain.to_string();
    let challenge = TokenChallenge::new(
        TokenType::PrivateRistretto255,
        &domain,
        None,
        std::slice::from_ref(&domain),
    );

    let (token_request, token_state) =
        AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 1)?;

    let request_bytes = SerializedTokenRequest::new(token_request.tls_serialize_detached()?);
    Ok(Some((
        request_bytes,
        TokenState {
            inner: token_state,
            token_key_id,
        },
    )))
}

/// Finalizes a token response from a `DeleteHandle` response and stores the
/// token locally.
pub(crate) async fn finalize_delete_token_response(
    db: &DbAccess,
    response: &SerializedTokenResponse,
    token_state: TokenState,
) -> anyhow::Result<()> {
    let token_response =
        AmortizedBatchTokenResponse::<Ristretto255>::tls_deserialize_exact(response.as_bytes())?;

    let tokens = token_response.issue_tokens(&token_state.inner)?;
    for token in tokens {
        let token_bytes = token.tls_serialize_detached()?;
        persistence::store_token(
            db.write().await?,
            OperationType::AddUsername,
            token_state.token_key_id,
            &token_bytes,
        )
        .await?;
    }
    Ok(())
}

/// Refreshes the batched token keys, purges all cached tokens, then replenishes
/// them from the server.
///
/// Called when the server reports that the token key has rotated and our cached
/// tokens are stale. The re-fetch is answered from the registered request, so
/// recovery costs no allowance.
pub(crate) async fn purge_and_replenish(
    db: &DbAccess,
    api_client: &ApiClient,
    user_id: UserId,
    operation_type: OperationType,
    signing_key: &UserSigningKey,
) -> anyhow::Result<()> {
    // Refetch the keys before touching local state, so that a network failure
    // leaves the client with what it had rather than with nothing.
    let credentials_response = api_client.as_as_credentials().await?;
    db.with_write_transaction(async move |txn| {
        store_batched_token_keys(txn, &credentials_response.batched_token_keys).await
    })
    .await?;

    let discarded = persistence::token_count(db.read().await?, operation_type).await?;
    info!(%discarded, "purging stale tokens after server rejected key");
    // The AS had no key for the rejected token's ID, so the whole cache is
    // suspect even when the advertised key set turns out to be unchanged.
    // The batch records have to go with the tokens, or the next run would
    // consider the purged batch fetched and never ask for it again, so both
    // deletes share a transaction and a crash cannot land in between.
    db.with_write_transaction(async |txn| -> sqlx::Result<()> {
        persistence::delete_all_tokens(&mut *txn, operation_type).await?;
        persistence::delete_all_batches(&mut *txn, operation_type).await
    })
    .await?;

    let outcome = replenish(db, api_client, user_id, signing_key, operation_type).await?;
    info!(?outcome, %operation_type, "replenished tokens after key rotation");
    Ok(())
}

/// Opaque wrapper around the privacypass `TokenState` needed to finalize
/// a token response.
///
/// Carries the ID of the key the request was built with, because the token that
/// comes back has to be stored under it and the privacypass state does not
/// expose its public key.
pub(crate) struct TokenState {
    inner: privacypass::amortized_tokens::TokenState<Ristretto255>,
    token_key_id: u8,
}

/// Lifecycle state of a stored token seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SeedState {
    /// Staged for a self-group commit but not agreed yet. Never derives a token
    /// request: a proposal that loses the commit race is replaced by the winner.
    Proposed,
    /// Agreed among the user's devices, and therefore the seed that derives the
    /// token requests of this key.
    Committed,
}

/// A stored token seed and its lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StoredSeed {
    pub(crate) seed: [u8; SEED_LEN],
    pub(crate) state: SeedState,
}

impl StoredSeed {
    /// The seed, if it may derive a token request.
    fn committed(self) -> Option<[u8; SEED_LEN]> {
        match self.state {
            SeedState::Committed => Some(self.seed),
            SeedState::Proposed => None,
        }
    }
}

/// A token seed together with the key and the operation it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SeedRecord {
    pub(crate) operation_type: OperationType,
    pub(crate) key_fingerprint: KeyFingerprint,
    pub(crate) seed: [u8; SEED_LEN],
}

impl SeedRecord {
    /// The form the seed takes on a self-group commit.
    fn to_wire(self) -> TokenSeed {
        TokenSeed {
            operation_type: operation_type_value(self.operation_type),
            key_fingerprint: self.key_fingerprint,
            seed: self.seed,
        }
    }

    /// Reads a seed a sibling sent, or `None` if it is not usable.
    ///
    /// A tag the sender left out decodes to zero bytes, which is also what a
    /// sibling running a newer derivation version would leave us with. Rejecting
    /// zeros covers both: a generated seed and a real fingerprint are zero with
    /// negligible probability.
    fn from_wire(wire: &TokenSeed) -> Option<Self> {
        let operation_type = i32::try_from(wire.operation_type)
            .ok()
            .and_then(|value| OperationType::try_from(value).ok())?;
        if operation_type == OperationType::Unspecified
            || wire.key_fingerprint == [0u8; 32]
            || wire.seed == [0u8; SEED_LEN]
        {
            return None;
        }
        Some(Self {
            operation_type,
            key_fingerprint: wire.key_fingerprint,
            seed: wire.seed,
        })
    }
}

/// SHA-256 over the serialized public key, the same definition the AS uses.
fn key_fingerprint(public_key: &PublicKey<Ristretto255>) -> KeyFingerprint {
    Sha256::digest(serialize_public_key::<Ristretto255>(*public_key)).into()
}

fn fingerprint_of_bytes(public_key_bytes: &[u8]) -> Option<KeyFingerprint> {
    let public_key = deserialize_public_key::<Ristretto255>(public_key_bytes).ok()?;
    Some(key_fingerprint(&public_key))
}

/// The proto enum value the derivation binds into the batch seed.
fn operation_type_value(operation_type: OperationType) -> u32 {
    i32::from(operation_type).cast_unsigned()
}

#[cfg(test)]
mod tests;
