// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::{QsClientId, QsUserId};
use openmls::group::GroupId;
use privacypass::private_tokens::VoprfServer;
use rand::{SeedableRng, rngs::StdRng};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::*;

const OPERATION_TYPE: OperationType = OperationType::AddUsername;

/// Token seed of the frozen vectors.
const KAT_TOKEN_SEED: [u8; SEED_LEN] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];

/// Key fingerprint of the frozen vectors.
const KAT_FINGERPRINT: KeyFingerprint = [
    0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xeb, 0xec, 0xed, 0xee, 0xef,
    0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa, 0xfb, 0xfc, 0xfd, 0xfe, 0xff,
];

/// Allowance epoch of the frozen vectors (2026-08 in months since 1970-01).
const KAT_EPOCH: u32 = 679;

/// Seed of the fixed server keypair of the frozen request vector.
const KAT_SERVER_SEED: u64 = 0x0DDB_1A5E_5BAD_5EED;

const KAT_DOMAIN: &str = "example.com";

/// A VOPRF public key, serialized as the AS advertises it.
fn voprf_public_key(seed: u64) -> Vec<u8> {
    let server = VoprfServer::<Ristretto255>::new(&mut StdRng::seed_from_u64(seed))
        .expect("failed to create a VOPRF server");
    serialize_public_key::<Ristretto255>(server.get_public_key())
}

fn advertised_key(public_key: &[u8], is_current: bool) -> BatchedTokenKeyResponse {
    let key = deserialize_public_key::<Ristretto255>(public_key).expect("invalid public key");
    BatchedTokenKeyResponse {
        operation_type: i32::from(OPERATION_TYPE),
        token_key_id: public_key_to_truncated_token_key_id::<Ristretto255>(&key),
        public_key: public_key.to_vec(),
        is_current,
    }
}

fn synthetic_key(token_key_id: u8, is_current: bool) -> BatchedTokenKeyResponse {
    BatchedTokenKeyResponse {
        operation_type: i32::from(OPERATION_TYPE),
        token_key_id,
        public_key: vec![token_key_id; 32],
        is_current,
    }
}

fn fingerprint_of(public_key: &[u8]) -> KeyFingerprint {
    fingerprint_of_bytes(public_key).expect("invalid public key")
}

async fn store_keys(db: &DbAccess, keys: &[BatchedTokenKeyResponse]) -> anyhow::Result<()> {
    db.with_write_transaction(async |txn| store_batched_token_keys(txn, keys).await)
        .await
}

/// A migrated in-memory client database.
async fn migrated_db() -> anyhow::Result<DbAccess> {
    let pool = SqlitePool::connect("sqlite://:memory:").await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(DbAccess::for_tests(pool))
}

/// Stores a committed token seed, as a lone device or a provisioning snapshot
/// would.
async fn commit_seed(
    db: &DbAccess,
    fingerprint: &KeyFingerprint,
    seed: &[u8; SEED_LEN],
) -> anyhow::Result<()> {
    persistence::adopt_seed(db.write().await?, OPERATION_TYPE, fingerprint, seed).await?;
    Ok(())
}

/// The wire form of a seed, as a sibling would publish it.
fn wire_seed(fingerprint: &KeyFingerprint, seed: [u8; SEED_LEN]) -> TokenSeed {
    TokenSeed {
        operation_type: operation_type_value(OPERATION_TYPE),
        key_fingerprint: *fingerprint,
        seed,
    }
}

/// Stores the `own_client_info` row `is_alone` reads.
async fn store_own_client_info(
    db: &DbAccess,
    self_group_id: Option<GroupId>,
) -> anyhow::Result<()> {
    db.with_write_transaction(async |txn| -> anyhow::Result<()> {
        OwnClientInfo {
            qs_user_id: QsUserId::random(),
            qs_client_id: QsClientId::random(&mut rand::rng()),
            user_id: UserId::random("example.com".parse()?),
            client_id: Uuid::new_v4(),
            self_group_id,
            self_group_signing_key: None,
        }
        .store(&mut *txn)
        .await?;
        Ok(())
    })
    .await
}

/// Frozen derivation vectors.
///
/// These bytes are a cross-device protocol invariant: every device of a user
/// derives its token request from them, and a device that derives different
/// bytes gets an issuance conflict instead of tokens. A failure here means the
/// derivation changed, which is a breaking client change. Never regenerate the
/// vectors to make the test pass.
#[test]
fn frozen_derivation_vectors() {
    let batch_seed = derivation::batch_seed(
        &KAT_TOKEN_SEED,
        operation_type_value(OPERATION_TYPE),
        &KAT_FINGERPRINT,
        KAT_EPOCH,
    );

    assert_eq!(
        hex::encode(batch_seed),
        "46a581691641084158bdac1d70a5df16dbea1918ca37927683456230cd001e71"
    );
    assert_eq!(
        hex::encode(derivation::nonce(&batch_seed, 0)),
        "bece4ad93a060d7697dec9c3f332d18dc3669bf1732abc6d0302c7d8b0b804e4"
    );
    assert_eq!(
        hex::encode(derivation::nonce(&batch_seed, 9)),
        "da504e51af601e7f9ccdef63426bcafd9ef571a0b3d3205a45a40ac7dc19cec4"
    );
    assert_eq!(
        hex::encode(derivation::blind(&batch_seed, 0).to_bytes()),
        "e8b0c71b8703a95c4cf027cf6b139830de2b3ba1d234aa54973c0bbb3d68aa0c"
    );
    assert_eq!(
        hex::encode(derivation::blind(&batch_seed, 9).to_bytes()),
        "53311f6b960fc198db13cf0a93cd8e0f20282dc3692f1679031931ba0750720f"
    );
}

/// Frozen token request vector, same rules as the derivation vectors above.
#[test]
fn frozen_token_request() {
    let public_key = voprf_public_key(KAT_SERVER_SEED);
    assert_eq!(
        hex::encode(&public_key),
        "88f9c7d1e0d27fe0450648115b33eb0f4aa1bf29da12c9d4db446185f7bee135"
    );

    let batch = build_batch_request(
        &public_key,
        OPERATION_TYPE,
        &KAT_TOKEN_SEED,
        KAT_EPOCH,
        KAT_DOMAIN,
    )
    .expect("failed to build the token request");

    assert_eq!(
        hex::encode(batch.request.as_bytes()),
        concat!(
            "0005f941400e6f68f1b793a86c8d737e498b6330f3a9014e72126da538b753f9",
            "79a331c5195c84406283c2e7cf843ad5264b99e071a349f2341f487941936ff8",
            "4e0de0077500ac921bbc9fdf40c1d95fae2ef0704becefce23efac22fb91bea0",
            "db41dafb5cdab869b6236cf2cd759626b809db067f0cd50792f44d184530961b",
            "7f0d949806c6ba30bf9126eb80b08f09af1d7e0e3229355108cede2088bab2b2",
            "ca1ffabe1fca42066d8325d320caa5dfe6c7fe5b8139bc85ea0583bcb37ca6ca",
            "38168e17112ac1b3c9ef57350c010ace064ca6809d4620e78474d596b0d7ed4d",
            "e009876223363917c7fc51b1b8884206b38728e06af526c10a70567f837ea13a",
            "d1c67e8e5436a499e4037bbc4534b8b2120c328e44e3210e917d2899f662ed38",
            "448c7043067c9d213530ffdecb1641db542f59a5356808f76dc1ef144baee035",
            "68aa12c349",
        )
    );
}

/// Two separately populated stores holding the same key and seed derive
/// byte-identical requests.
#[tokio::test]
async fn two_stores_derive_identical_requests() -> anyhow::Result<()> {
    let public_key = voprf_public_key(7);
    let fingerprint = fingerprint_of(&public_key);
    let seed = [0x5a; SEED_LEN];

    let mut requests = Vec::new();
    for _ in 0..2 {
        let db = migrated_db().await?;
        store_keys(&db, &[advertised_key(&public_key, true)]).await?;
        commit_seed(&db, &fingerprint, &seed).await?;

        // Everything the request depends on comes out of this store.
        let keys = persistence::load_batched_token_keys(db.read().await?, OPERATION_TYPE).await?;
        let (_, stored_key) = keys.first().expect("no key stored");
        let stored_fingerprint = fingerprint_of(stored_key);
        let stored_seed =
            persistence::load_seed(db.read().await?, OPERATION_TYPE, &stored_fingerprint)
                .await?
                .expect("no seed stored")
                .seed;

        let batch = build_batch_request(
            stored_key,
            OPERATION_TYPE,
            &stored_seed,
            KAT_EPOCH,
            KAT_DOMAIN,
        )?;
        requests.push(batch.request.into_bytes());
    }

    assert_eq!(requests[0], requests[1]);
    Ok(())
}

/// A request built from a different seed differs, so the comparison above has
/// teeth.
#[test]
fn a_different_seed_changes_the_request() -> anyhow::Result<()> {
    let public_key = voprf_public_key(7);

    let first = build_batch_request(
        &public_key,
        OPERATION_TYPE,
        &[0x5a; SEED_LEN],
        KAT_EPOCH,
        KAT_DOMAIN,
    )?;
    let second = build_batch_request(
        &public_key,
        OPERATION_TYPE,
        &[0x5b; SEED_LEN],
        KAT_EPOCH,
        KAT_DOMAIN,
    )?;

    assert_ne!(first.request.as_bytes(), second.request.as_bytes());
    Ok(())
}

/// Storing the same finalized batch twice stores it once.
#[sqlx::test]
async fn restoring_a_batch_is_a_no_op(pool: SqlitePool) -> anyhow::Result<()> {
    let db = DbAccess::for_tests(pool);
    let fingerprint = fingerprint_of(&voprf_public_key(7));
    let tokens = vec![b"token_0".to_vec(), b"token_1".to_vec()];

    store_batch_tokens(&db, OPERATION_TYPE, 3, &fingerprint, KAT_EPOCH, &tokens).await?;
    store_batch_tokens(&db, OPERATION_TYPE, 3, &fingerprint, KAT_EPOCH, &tokens).await?;

    assert_eq!(
        persistence::token_count(db.read().await?, OPERATION_TYPE).await?,
        2
    );
    assert!(
        persistence::batch_was_fetched(db.read().await?, OPERATION_TYPE, &fingerprint, KAT_EPOCH)
            .await?
    );

    Ok(())
}

/// The first seed of a key is the seed of that key: a later run adopts it
/// instead of replacing it.
#[sqlx::test]
async fn the_first_seed_of_a_key_wins(pool: SqlitePool) -> anyhow::Result<()> {
    let db = DbAccess::for_tests(pool);
    let fingerprint = fingerprint_of(&voprf_public_key(7));
    let first = [0x11; SEED_LEN];
    let second = [0x22; SEED_LEN];

    assert_eq!(
        persistence::insert_seed(
            db.write().await?,
            OPERATION_TYPE,
            &fingerprint,
            &first,
            SeedState::Committed
        )
        .await?
        .seed,
        first
    );
    assert_eq!(
        persistence::insert_seed(
            db.write().await?,
            OPERATION_TYPE,
            &fingerprint,
            &second,
            SeedState::Committed
        )
        .await?
        .seed,
        first,
        "the stored seed is returned, not the new candidate"
    );
    assert_eq!(
        persistence::load_seed(db.read().await?, OPERATION_TYPE, &fingerprint).await?,
        Some(StoredSeed {
            seed: first,
            state: SeedState::Committed
        })
    );

    Ok(())
}

/// `resolve_seed` generates a seed on first use and returns the same one after.
#[sqlx::test]
async fn resolve_seed_is_stable(pool: SqlitePool) -> anyhow::Result<()> {
    let db = DbAccess::for_tests(pool);
    store_own_client_info(&db, None).await?;
    let fingerprint = fingerprint_of(&voprf_public_key(7));

    let generated = resolve_seed(&db, OPERATION_TYPE, &fingerprint).await?;
    assert_eq!(
        resolve_seed(&db, OPERATION_TYPE, &fingerprint).await?,
        generated
    );
    // A different key gets its own seed.
    let other = fingerprint_of(&voprf_public_key(8));
    assert_ne!(resolve_seed(&db, OPERATION_TYPE, &other).await?, generated);

    Ok(())
}

/// Two runs racing on the same store converge on one seed.
///
/// The interactive flow and the replenishment task can both reach seed
/// resolution, and a run that returned its own losing candidate would derive
/// requests the server answers with a conflict.
#[tokio::test]
async fn concurrent_resolution_converges_on_one_seed() -> anyhow::Result<()> {
    let db = migrated_db().await?;
    store_own_client_info(&db, None).await?;
    let fingerprint = fingerprint_of(&voprf_public_key(7));

    let (first, second) = tokio::join!(
        resolve_seed(&db, OPERATION_TYPE, &fingerprint),
        resolve_seed(&db, OPERATION_TYPE, &fingerprint)
    );

    let stored = persistence::load_seed(db.read().await?, OPERATION_TYPE, &fingerprint)
        .await?
        .expect("no seed stored");
    assert_eq!(first?, Some(stored.seed));
    assert_eq!(second?, Some(stored.seed));

    Ok(())
}

/// A device without a self group is alone by definition, so the seed it
/// generates is agreed immediately and derives requests right away.
#[sqlx::test]
async fn a_lone_device_commits_its_own_seed(pool: SqlitePool) -> anyhow::Result<()> {
    let db = DbAccess::for_tests(pool);
    store_own_client_info(&db, None).await?;
    let fingerprint = fingerprint_of(&voprf_public_key(7));

    let seed = resolve_seed(&db, OPERATION_TYPE, &fingerprint)
        .await?
        .expect("a lone device must be able to derive a request right away");
    assert_eq!(
        persistence::load_seed(db.read().await?, OPERATION_TYPE, &fingerprint).await?,
        Some(StoredSeed {
            seed,
            state: SeedState::Committed
        })
    );
    // Nothing to tell anyone about: there is no self group to commit on.
    assert!(seeds_to_broadcast(db.read().await?).await?.is_empty());

    Ok(())
}

/// A self group this device has not joined yet says nothing about how many
/// devices there are, so the seed is only proposed and withheld from issuance
/// until the siblings have seen it.
#[sqlx::test]
async fn a_device_that_cannot_count_its_siblings_proposes(pool: SqlitePool) -> anyhow::Result<()> {
    let db = DbAccess::for_tests(pool);
    store_own_client_info(&db, Some(GroupId::from_slice(b"unjoined"))).await?;
    let fingerprint = fingerprint_of(&voprf_public_key(7));

    assert_eq!(resolve_seed(&db, OPERATION_TYPE, &fingerprint).await?, None);

    let stored = persistence::load_seed(db.read().await?, OPERATION_TYPE, &fingerprint)
        .await?
        .expect("the proposal must be stored");
    assert_eq!(stored.state, SeedState::Proposed);
    // The outbound service has to carry the proposal to the siblings.
    assert_eq!(
        seeds_to_broadcast(db.read().await?).await?,
        vec![wire_seed(&fingerprint, stored.seed)]
    );
    // A second run keeps proposing the same seed rather than inventing another.
    assert_eq!(resolve_seed(&db, OPERATION_TYPE, &fingerprint).await?, None);
    assert_eq!(
        persistence::load_seed(db.read().await?, OPERATION_TYPE, &fingerprint)
            .await?
            .map(|s| s.seed),
        Some(stored.seed)
    );

    Ok(())
}

/// A proposal left over from a time when this device had a sibling is committed
/// once it is alone: waiting for an agreement round with nobody to agree with
/// would withhold tokens forever.
#[sqlx::test]
async fn a_proposal_is_committed_once_the_device_is_alone(pool: SqlitePool) -> anyhow::Result<()> {
    let db = DbAccess::for_tests(pool);
    let fingerprint = fingerprint_of(&voprf_public_key(7));
    let seed = [0x77; SEED_LEN];
    persistence::insert_seed(
        db.write().await?,
        OPERATION_TYPE,
        &fingerprint,
        &seed,
        SeedState::Proposed,
    )
    .await?;
    store_own_client_info(&db, None).await?;

    assert_eq!(
        resolve_seed(&db, OPERATION_TYPE, &fingerprint).await?,
        Some(seed)
    );
    assert_eq!(
        persistence::load_seed(db.read().await?, OPERATION_TYPE, &fingerprint).await?,
        Some(StoredSeed {
            seed,
            state: SeedState::Committed
        })
    );

    Ok(())
}

/// A sibling's seed covers our proposal whatever its value: the DS ordered their
/// commit first, so theirs is the agreed seed.
#[sqlx::test]
async fn an_incoming_seed_covers_our_proposal(pool: SqlitePool) -> anyhow::Result<()> {
    let db = DbAccess::for_tests(pool);
    let fingerprint = fingerprint_of(&voprf_public_key(7));

    // Both a higher and a lower incoming seed take over from a proposal.
    for incoming in [[0x11; SEED_LEN], [0x99; SEED_LEN]] {
        persistence::delete_seed(db.write().await?, OPERATION_TYPE, &fingerprint).await?;
        persistence::insert_seed(
            db.write().await?,
            OPERATION_TYPE,
            &fingerprint,
            &[0x55; SEED_LEN],
            SeedState::Proposed,
        )
        .await?;

        db.with_write_transaction(async |txn| {
            apply_incoming_seed(txn, &wire_seed(&fingerprint, incoming)).await
        })
        .await?;

        assert_eq!(
            persistence::load_seed(db.read().await?, OPERATION_TYPE, &fingerprint).await?,
            Some(StoredSeed {
                seed: incoming,
                state: SeedState::Committed
            }),
            "incoming {incoming:?} must cover the proposal"
        );
        // Converged, so nothing has to go out.
        assert!(seeds_to_broadcast(db.read().await?).await?.is_empty());
    }

    Ok(())
}

/// Two committed seeds only diverge when a device joined without a snapshot. The
/// lower one wins, and the device holding it re-broadcasts it so the other side
/// converges too.
#[sqlx::test]
async fn diverging_committed_seeds_converge_on_the_lower(pool: SqlitePool) -> anyhow::Result<()> {
    let db = DbAccess::for_tests(pool);
    let fingerprint = fingerprint_of(&voprf_public_key(7));
    let lower = [0x11; SEED_LEN];
    let higher = [0x99; SEED_LEN];

    // A lower incoming seed is adopted, and needs no re-broadcast: the sender
    // already holds it.
    commit_seed(&db, &fingerprint, &higher).await?;
    db.with_write_transaction(async |txn| {
        apply_incoming_seed(txn, &wire_seed(&fingerprint, lower)).await
    })
    .await?;
    assert_eq!(
        persistence::load_seed(db.read().await?, OPERATION_TYPE, &fingerprint)
            .await?
            .map(|s| s.seed),
        Some(lower)
    );
    assert!(seeds_to_broadcast(db.read().await?).await?.is_empty());

    // A higher incoming seed loses, and ours goes out once so the sender
    // converges on it.
    db.with_write_transaction(async |txn| {
        apply_incoming_seed(txn, &wire_seed(&fingerprint, higher)).await
    })
    .await?;
    assert_eq!(
        persistence::load_seed(db.read().await?, OPERATION_TYPE, &fingerprint)
            .await?
            .map(|s| s.seed),
        Some(lower)
    );
    assert_eq!(
        seeds_to_broadcast(db.read().await?).await?,
        vec![wire_seed(&fingerprint, lower)]
    );

    // The sender echoing our seed back retires the re-broadcast.
    db.with_write_transaction(async |txn| {
        apply_incoming_seed(txn, &wire_seed(&fingerprint, lower)).await
    })
    .await?;
    assert!(seeds_to_broadcast(db.read().await?).await?.is_empty());

    Ok(())
}

/// Our accepted commit turns the proposals it carried into agreed seeds.
#[sqlx::test]
async fn completing_a_sent_proposal_agrees_it(pool: SqlitePool) -> anyhow::Result<()> {
    let db = DbAccess::for_tests(pool);
    let fingerprint = fingerprint_of(&voprf_public_key(7));
    let seed = [0x42; SEED_LEN];
    persistence::insert_seed(
        db.write().await?,
        OPERATION_TYPE,
        &fingerprint,
        &seed,
        SeedState::Proposed,
    )
    .await?;

    let sent = vec![wire_seed(&fingerprint, seed)];
    db.with_write_transaction(async |txn| complete_sent_seeds(txn, &sent).await)
        .await?;

    assert_eq!(
        persistence::load_seed(db.read().await?, OPERATION_TYPE, &fingerprint).await?,
        Some(StoredSeed {
            seed,
            state: SeedState::Committed
        })
    );
    assert!(seeds_to_broadcast(db.read().await?).await?.is_empty());

    Ok(())
}

/// A commit that a sibling's commit overtook must not promote the seed it
/// carried: the stored seed is the sibling's by then, and completing ours would
/// resurrect a seed nobody else holds.
#[sqlx::test]
async fn completing_an_overtaken_proposal_keeps_the_agreed_seed(
    pool: SqlitePool,
) -> anyhow::Result<()> {
    let db = DbAccess::for_tests(pool);
    let fingerprint = fingerprint_of(&voprf_public_key(7));
    let ours = [0x42; SEED_LEN];
    let theirs = [0x43; SEED_LEN];
    commit_seed(&db, &fingerprint, &theirs).await?;

    let sent = vec![wire_seed(&fingerprint, ours)];
    db.with_write_transaction(async |txn| complete_sent_seeds(txn, &sent).await)
        .await?;

    assert_eq!(
        persistence::load_seed(db.read().await?, OPERATION_TYPE, &fingerprint).await?,
        Some(StoredSeed {
            seed: theirs,
            state: SeedState::Committed
        })
    );

    Ok(())
}

/// A terminally failed commit drops the proposals it carried, so a later run
/// proposes again. A seed the devices agreed on meanwhile survives.
#[sqlx::test]
async fn rolling_back_drops_proposals_and_keeps_agreed_seeds(
    pool: SqlitePool,
) -> anyhow::Result<()> {
    let db = DbAccess::for_tests(pool);
    let proposed_key = fingerprint_of(&voprf_public_key(7));
    let agreed_key = fingerprint_of(&voprf_public_key(8));
    let seed = [0x42; SEED_LEN];

    persistence::insert_seed(
        db.write().await?,
        OPERATION_TYPE,
        &proposed_key,
        &seed,
        SeedState::Proposed,
    )
    .await?;
    commit_seed(&db, &agreed_key, &seed).await?;

    let sent = vec![wire_seed(&proposed_key, seed), wire_seed(&agreed_key, seed)];
    db.with_write_transaction(async |txn| roll_back_sent_seeds(txn, &sent).await)
        .await?;

    assert_eq!(
        persistence::load_seed(db.read().await?, OPERATION_TYPE, &proposed_key).await?,
        None,
        "the proposal must be dropped so a later run proposes again"
    );
    assert!(
        persistence::load_seed(db.read().await?, OPERATION_TYPE, &agreed_key)
            .await?
            .is_some(),
        "an agreed seed is not this commit's to drop"
    );

    Ok(())
}

/// A malformed incoming seed is ignored rather than stored. An absent tag decodes
/// to zero bytes, and a zero seed would derive requests no sibling sends.
#[sqlx::test]
async fn a_malformed_incoming_seed_is_ignored(pool: SqlitePool) -> anyhow::Result<()> {
    let db = DbAccess::for_tests(pool);
    let fingerprint = fingerprint_of(&voprf_public_key(7));

    let malformed = [
        wire_seed(&fingerprint, [0u8; SEED_LEN]),
        wire_seed(&[0u8; 32], [0x42; SEED_LEN]),
        TokenSeed {
            operation_type: 0,
            key_fingerprint: fingerprint,
            seed: [0x42; SEED_LEN],
        },
        TokenSeed {
            operation_type: 99,
            key_fingerprint: fingerprint,
            seed: [0x42; SEED_LEN],
        },
    ];
    for incoming in &malformed {
        db.with_write_transaction(async |txn| apply_incoming_seed(txn, incoming).await)
            .await?;
    }

    assert!(
        persistence::load_committed_seeds(db.read().await?)
            .await?
            .is_empty()
    );

    Ok(())
}

/// The provisioning snapshot carries only agreed seeds, and the joining device
/// can derive requests from them without an agreement round of its own.
#[sqlx::test]
async fn provisioned_seeds_are_agreed_immediately(pool: SqlitePool) -> anyhow::Result<()> {
    let provisioner = DbAccess::for_tests(pool);
    let agreed_key = fingerprint_of(&voprf_public_key(7));
    let proposed_key = fingerprint_of(&voprf_public_key(8));
    let seed = [0x42; SEED_LEN];

    commit_seed(&provisioner, &agreed_key, &seed).await?;
    persistence::insert_seed(
        provisioner.write().await?,
        OPERATION_TYPE,
        &proposed_key,
        &[0x43; SEED_LEN],
        SeedState::Proposed,
    )
    .await?;

    let snapshot = committed_seeds(provisioner.read().await?).await?;
    assert_eq!(snapshot, vec![wire_seed(&agreed_key, seed)]);

    // The joining device stores them and is ready to issue, with nothing left to
    // agree on and nothing to broadcast.
    let joiner = migrated_db().await?;
    joiner
        .with_write_transaction(async |txn| store_provisioned_seeds(txn, &snapshot).await)
        .await?;
    // A sibling exists, so a seed this device had to invent would only be
    // proposed. The provisioned one is already agreed.
    store_own_client_info(&joiner, Some(GroupId::from_slice(b"unjoined"))).await?;
    assert_eq!(
        resolve_seed(&joiner, OPERATION_TYPE, &agreed_key).await?,
        Some(seed)
    );
    assert!(seeds_to_broadcast(joiner.read().await?).await?.is_empty());

    Ok(())
}

/// Tokens stored before batch tagging existed are spent first.
#[sqlx::test]
async fn metered_tokens_are_consumed_first(pool: SqlitePool) -> anyhow::Result<()> {
    let db = DbAccess::for_tests(pool);

    persistence::store_token(db.write().await?, OPERATION_TYPE, 1, b"metered").await?;
    persistence::store_batch_token(
        db.write().await?,
        OPERATION_TYPE,
        1,
        KAT_EPOCH,
        0,
        b"tagged",
    )
    .await?;

    let first = consume_token(db.write().await?, OPERATION_TYPE)
        .await?
        .expect("no token stored");
    assert_eq!(first.as_bytes(), b"metered");
    let second = consume_token(db.write().await?, OPERATION_TYPE)
        .await?
        .expect("no token stored");
    assert_eq!(second.as_bytes(), b"tagged");

    Ok(())
}

/// A key that is added to the advertised set leaves cached tokens alone.
#[sqlx::test]
async fn added_key_keeps_tokens(pool: SqlitePool) -> anyhow::Result<()> {
    let db = DbAccess::for_tests(pool);

    store_keys(&db, &[synthetic_key(1, true)]).await?;
    persistence::store_token(db.write().await?, OPERATION_TYPE, 1, b"token_a").await?;
    persistence::store_token(db.write().await?, OPERATION_TYPE, 1, b"token_b").await?;

    store_keys(&db, &[synthetic_key(1, false), synthetic_key(2, true)]).await?;

    assert_eq!(
        persistence::token_count(db.read().await?, OPERATION_TYPE).await?,
        2
    );
    // The newly advertised current key comes first.
    let keys = persistence::load_batched_token_keys(db.read().await?, OPERATION_TYPE).await?;
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0].0, 2);

    Ok(())
}

/// A key that disappears from the advertised set takes only its own tokens.
#[sqlx::test]
async fn removed_key_discards_only_its_tokens(pool: SqlitePool) -> anyhow::Result<()> {
    let db = DbAccess::for_tests(pool);

    store_keys(&db, &[synthetic_key(1, false), synthetic_key(2, true)]).await?;
    persistence::store_token(db.write().await?, OPERATION_TYPE, 1, b"outgoing").await?;
    persistence::store_token(db.write().await?, OPERATION_TYPE, 2, b"current_a").await?;
    persistence::store_token(db.write().await?, OPERATION_TYPE, 2, b"current_b").await?;

    store_keys(&db, &[synthetic_key(2, true)]).await?;

    assert_eq!(
        persistence::token_count(db.read().await?, OPERATION_TYPE).await?,
        2
    );
    assert_eq!(
        persistence::load_token_key_ids(db.read().await?, OPERATION_TYPE).await?,
        vec![2]
    );

    Ok(())
}

/// A key that reuses a truncated ID with different key material takes the old
/// key's tokens with it, while an unchanged key keeps its own. Stale tokens
/// fail redemption with InvalidToken instead of UnknownKeyId, which never
/// triggers the purge recovery.
#[sqlx::test]
async fn changed_key_material_under_a_reused_id_discards_tokens(
    pool: SqlitePool,
) -> anyhow::Result<()> {
    let db = DbAccess::for_tests(pool);

    store_keys(&db, &[synthetic_key(1, false), synthetic_key(2, true)]).await?;
    persistence::store_token(db.write().await?, OPERATION_TYPE, 1, b"stale").await?;
    persistence::store_token(db.write().await?, OPERATION_TYPE, 2, b"current").await?;

    // The same ID set comes back, but ID 1 now carries different key material.
    let replaced = BatchedTokenKeyResponse {
        operation_type: i32::from(OPERATION_TYPE),
        token_key_id: 1,
        public_key: vec![0xaa; 32],
        is_current: false,
    };
    store_keys(&db, &[replaced, synthetic_key(2, true)]).await?;

    assert_eq!(
        persistence::load_token_key_ids(db.read().await?, OPERATION_TYPE).await?,
        vec![2]
    );

    Ok(())
}

/// A key that disappears from the advertised set also loses its seed and its
/// batch records, while the surviving key keeps both.
#[sqlx::test]
async fn removed_key_discards_its_seed_and_batches(pool: SqlitePool) -> anyhow::Result<()> {
    let db = DbAccess::for_tests(pool);

    let removed_key = voprf_public_key(1);
    let surviving_key = voprf_public_key(2);
    let removed = fingerprint_of(&removed_key);
    let surviving = fingerprint_of(&surviving_key);

    store_keys(
        &db,
        &[
            advertised_key(&removed_key, false),
            advertised_key(&surviving_key, true),
        ],
    )
    .await?;
    for fingerprint in [&removed, &surviving] {
        commit_seed(&db, fingerprint, &[0x33; SEED_LEN]).await?;
        persistence::store_batch(db.write().await?, OPERATION_TYPE, fingerprint, KAT_EPOCH).await?;
    }

    store_keys(&db, &[advertised_key(&surviving_key, true)]).await?;

    assert!(
        persistence::load_seed(db.read().await?, OPERATION_TYPE, &removed)
            .await?
            .is_none()
    );
    assert!(
        !persistence::batch_was_fetched(db.read().await?, OPERATION_TYPE, &removed, KAT_EPOCH)
            .await?
    );
    assert!(
        persistence::load_seed(db.read().await?, OPERATION_TYPE, &surviving)
            .await?
            .is_some()
    );
    assert!(
        persistence::batch_was_fetched(db.read().await?, OPERATION_TYPE, &surviving, KAT_EPOCH)
            .await?
    );

    Ok(())
}
