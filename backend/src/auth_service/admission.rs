// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Push-admission challenge.
//!
//! The server sends a random challenge to a push endpoint and hands the client
//! the session id over HTTPS.

use std::{fmt, sync::Arc};

use aircommon::{
    messages::push_token::{PushToken, PushTokenOperator},
    registration::{AdmissionSession, NewAdmissionSession},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use metrics::counter;
use rand::RngExt;
use sha2::{Digest, Sha256};
use sqlx::{PgExecutor, PgPool, PgTransaction};
use tracing::{debug, error, warn};
use uuid::Uuid;

use crate::{
    auth_service::{AuthService, registration_challenge::ChallengeVerdict},
    bucket_key::BucketKey,
    errors::auth_service::{AdmissionError, RegisterUserError},
    settings::{AdmissionSettings, RegistrationThreshold},
    window_counter::{bucket_lock_key, longest_window, reached, saturating_count, window_seconds},
};

/// Domain separation for endpoint names
const ENDPOINT_LABEL: &[u8] = b"AirAdmissionEndpoint";

/// Class of the advisory lock that serializes quota counting.
const QUOTA_LOCK_CLASS: i32 = 0x4144_4d53;

/// APNs tokens are 64 hex characters and FCM tokens run to about 200.
const MAX_PUSH_TOKEN_LEN: usize = 512;

/// Sends an admission challenge to a push endpoint.
#[async_trait]
pub trait ChallengeSender: fmt::Debug + Send + Sync + 'static {
    /// Whether this sender supports the given push token operator.
    fn supports(&self, operator: &PushTokenOperator) -> bool;

    /// Sends `challenge` to the endpoint. The challenge is dropped after
    /// `expires_at`.
    async fn send_challenge(
        &self,
        push_token: &PushToken,
        session_id: Uuid,
        challenge: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), ChallengeSendError>;
}

/// Why a challenge did not reach the push service.
#[derive(Debug, thiserror::Error)]
pub enum ChallengeSendError {
    #[error("no credentials are configured for this push platform")]
    PlatformUnavailable,
    #[error("the push service does not know this endpoint")]
    EndpointRejected,
    #[error("the push service did not accept the challenge: {0}")]
    NotAccepted(String),
}

impl ChallengeSendError {
    fn as_str(&self) -> &'static str {
        match self {
            Self::PlatformUnavailable => "platform_unavailable",
            Self::EndpointRejected => "endpoint_rejected",
            Self::NotAccepted(_) => "not_accepted",
        }
    }
}

/// Whether the endpoint may have another challenge sent to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SendVerdict {
    Send,
    Throttled,
    QuotaExhausted,
}

impl SendVerdict {
    fn as_str(self) -> &'static str {
        match self {
            Self::Send => "sent",
            Self::Throttled => "throttled",
            Self::QuotaExhausted => "quota_exhausted",
        }
    }
}

impl AuthService {
    /// Opens a session and sends its challenge to the endpoint.
    ///
    /// The answer looks the same whatever the endpoint's history is, so a
    /// throttled or exhausted endpoint gets a session no challenge arrives for.
    pub(crate) async fn create_admission_session(
        &self,
        push_token: PushToken,
    ) -> Result<NewAdmissionSession, AdmissionError> {
        let sender = self.challenge_sender()?.clone();
        let settings = &self.registration_gate().settings().admission;
        validate_push_token(push_token.token())?;

        if !sender.supports(push_token.operator()) {
            return Err(AdmissionError::Unavailable);
        }

        let platform = platform_label(push_token.operator());
        let bucket = self.endpoint_bucket(&push_token);

        let session_id = Uuid::new_v4();
        let challenge_bytes: [u8; 32] = rand::rng().random();
        let challenge = hex::encode(challenge_bytes);
        let lifetime = settings.sessionlifetime;
        let expires_at = Utc::now() + lifetime;
        let answer = NewAdmissionSession {
            session_id,
            lifetime,
        };

        let verdict = self
            .send_verdict(&bucket, settings)
            .await
            .map_err(|error| {
                error!(%error, "failed to read the admission counters");
                AdmissionError::StorageError
            })?;
        if verdict != SendVerdict::Send {
            report_session(platform, verdict.as_str());
            return Ok(answer);
        }

        self.open_session(session_id, &challenge, &bucket, platform, expires_at)
            .await?;

        let outcome = match sender
            .send_challenge(&push_token, session_id, &challenge, expires_at)
            .await
        {
            Ok(()) => SendVerdict::Send.as_str(),
            Err(error) => {
                warn!(%error, platform, "failed to send an admission challenge");
                error.as_str()
            }
        };
        debug!(%session_id, platform, "opened an admission session");
        report_session(platform, outcome);

        Ok(answer)
    }

    /// Stores an open session and counts the challenge against its endpoint.
    ///
    /// Recorded before the send is attempted, so a failed send still counts.
    async fn open_session(
        &self,
        session_id: Uuid,
        challenge: &str,
        bucket: &[u8; 32],
        platform: &'static str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), AdmissionError> {
        let mut txn = self.db_pool.begin().await.map_err(storage_error)?;
        sqlx::query!(
            "INSERT INTO admission_session (
                session_id, challenge_hash, endpoint_bucket, platform, expires_at
            ) VALUES ($1, $2, $3, $4, $5)",
            session_id,
            digest(challenge.as_bytes()),
            bucket.as_slice(),
            platform,
            expires_at,
        )
        .execute(txn.as_mut())
        .await
        .map_err(storage_error)?;
        sqlx::query!(
            "INSERT INTO admission_send_record (endpoint_bucket) VALUES ($1)",
            bucket.as_slice(),
        )
        .execute(txn.as_mut())
        .await
        .map_err(storage_error)?;
        txn.commit().await.map_err(storage_error)
    }

    /// Verifies a challenge answer and spends the session it belongs to.
    ///
    /// Runs on the registration transaction, so a registration that does not
    /// complete leaves the session spendable.
    pub(crate) async fn consume_admission_session(
        &self,
        txn: &mut PgTransaction<'_>,
        session: &AdmissionSession,
    ) -> Result<ChallengeVerdict, RegisterUserError> {
        let Some(spent) = self.spend_session(txn, session).await? else {
            return Ok(ChallengeVerdict::Rejected);
        };
        let (bucket, platform) = spent;

        // Registrations sharing an endpoint serialize here, so two of them
        // cannot both read the quota as free.
        sqlx::query!(
            "SELECT pg_advisory_xact_lock($1, $2)",
            QUOTA_LOCK_CLASS,
            bucket_lock_key(&bucket),
        )
        .execute(txn.as_mut())
        .await
        .map_err(|error| {
            error!(%error, "failed to lock an admission endpoint");
            RegisterUserError::StorageError
        })?;

        let quotas = &self.registration_gate().settings().admission.quotas;
        let used = count_consumptions(txn.as_mut(), &bucket, quotas)
            .await
            .map_err(|error| {
                error!(%error, "failed to count admission consumptions");
                RegisterUserError::StorageError
            })?;
        if reached(quotas, &used) {
            // The same answer a wrong challenge gets. Labelled apart from the
            // create-time verdict, which sends no challenge in the first place.
            report_session(&platform, "spend_quota_exhausted");
            return Ok(ChallengeVerdict::Rejected);
        }

        sqlx::query!(
            "INSERT INTO admission_consumption_record (endpoint_bucket) VALUES ($1)",
            bucket.as_slice(),
        )
        .execute(txn.as_mut())
        .await
        .map_err(|error| {
            error!(%error, "failed to record an admission consumption");
            RegisterUserError::StorageError
        })?;

        debug!(session_id = %session.session_id, "spent an admission session");
        report_session(&platform, "spent");
        Ok(ChallengeVerdict::Accepted)
    }

    /// Takes the session out of the table, and with it the endpoint the account
    /// counts against.
    ///
    /// One statement, so two registrations racing on one session admit one.
    async fn spend_session(
        &self,
        txn: &mut PgTransaction<'_>,
        session: &AdmissionSession,
    ) -> Result<Option<([u8; 32], String)>, RegisterUserError> {
        let spent = sqlx::query!(
            "DELETE FROM admission_session
            WHERE session_id = $1 AND challenge_hash = $2 AND expires_at > now()
            RETURNING endpoint_bucket, platform",
            session.session_id,
            digest(session.challenge.as_bytes()),
        )
        .fetch_optional(txn.as_mut())
        .await
        .map_err(|error| {
            error!(%error, "failed to spend an admission session");
            RegisterUserError::StorageError
        })?;

        let Some(spent) = spent else {
            return Ok(None);
        };
        let Ok(bucket) = <[u8; 32]>::try_from(spent.endpoint_bucket) else {
            error!("an admission endpoint bucket is not 32 bytes");
            return Err(RegisterUserError::StorageError);
        };
        Ok(Some((bucket, spent.platform)))
    }

    pub(crate) fn has_challenge_sender(&self) -> bool {
        self.challenge_sender().is_ok()
    }

    /// The sender, when this deployment offers the challenge at all.
    fn challenge_sender(&self) -> Result<&Arc<dyn ChallengeSender>, AdmissionError> {
        if !self
            .registration_gate()
            .settings()
            .offers_admission_sessions()
        {
            return Err(AdmissionError::Unavailable);
        }
        self.challenge_sender
            .as_ref()
            .ok_or(AdmissionError::Unavailable)
    }

    async fn send_verdict(
        &self,
        bucket: &[u8; 32],
        settings: &AdmissionSettings,
    ) -> sqlx::Result<SendVerdict> {
        let throttle = std::slice::from_ref(&settings.sendthrottle);
        let sends = count_sends(&self.db_pool, bucket, throttle).await?;
        if reached(throttle, &sends) {
            return Ok(SendVerdict::Throttled);
        }

        let used = count_consumptions(&self.db_pool, bucket, &settings.quotas).await?;
        if reached(&settings.quotas, &used) {
            return Ok(SendVerdict::QuotaExhausted);
        }

        Ok(SendVerdict::Send)
    }

    /// The name this endpoint counts under.
    fn endpoint_bucket(&self, push_token: &PushToken) -> [u8; 32] {
        let token = push_token.token().as_bytes();
        let mut input = Vec::with_capacity(token.len() + 1);
        input.push(match push_token.operator() {
            PushTokenOperator::Apple => 1,
            PushTokenOperator::Google => 2,
        });
        input.extend_from_slice(token);
        self.endpoint_bucket_key.bucket(ENDPOINT_LABEL, &input)
    }
}

fn report_session(platform: &str, outcome: &'static str) {
    debug!(platform, outcome, "admission session");
    counter!(
        "air_admission_sessions_total",
        "platform" => platform.to_owned(),
        "outcome" => outcome,
    )
    .increment(1);
}

fn digest(bytes: &[u8]) -> Vec<u8> {
    Sha256::digest(bytes).to_vec()
}

fn storage_error(error: sqlx::Error) -> AdmissionError {
    error!(%error, "admission storage error");
    AdmissionError::StorageError
}

fn platform_label(operator: &PushTokenOperator) -> &'static str {
    match operator {
        PushTokenOperator::Apple => "apple",
        PushTokenOperator::Google => "google",
    }
}

fn validate_push_token(token: &str) -> Result<(), AdmissionError> {
    let plausible = !token.is_empty()
        && token.len() <= MAX_PUSH_TOKEN_LEN
        && token.bytes().all(|byte| byte.is_ascii_graphic());
    if plausible {
        Ok(())
    } else {
        Err(AdmissionError::InvalidPushToken)
    }
}

/// Challenges sent to this endpoint, one count per window.
async fn count_sends(
    executor: impl PgExecutor<'_>,
    bucket: &[u8; 32],
    thresholds: &[RegistrationThreshold],
) -> sqlx::Result<Vec<u64>> {
    // Counting the joined column, so a window that matches nothing counts zero
    // rather than the one row the outer join leaves it.
    let counts = sqlx::query_scalar!(
        r#"
        SELECT COUNT(s.created_at) AS "count!"
        FROM unnest($2::BIGINT[]) WITH ORDINALITY AS w(win, ord)
        LEFT JOIN admission_send_record s
            ON s.endpoint_bucket = $1
            AND s.created_at >= now() - w.win * INTERVAL '1 second'
        GROUP BY w.ord
        ORDER BY w.ord
        "#,
        bucket.as_slice(),
        &window_seconds(thresholds),
    )
    .fetch_all(executor)
    .await?;

    Ok(counts.into_iter().map(saturating_count).collect())
}

/// Registrations this endpoint admitted, one count per window.
async fn count_consumptions(
    executor: impl PgExecutor<'_>,
    bucket: &[u8; 32],
    thresholds: &[RegistrationThreshold],
) -> sqlx::Result<Vec<u64>> {
    let counts = sqlx::query_scalar!(
        r#"
        SELECT COUNT(c.created_at) AS "count!"
        FROM unnest($2::BIGINT[]) WITH ORDINALITY AS w(win, ord)
        LEFT JOIN admission_consumption_record c
            ON c.endpoint_bucket = $1
            AND c.created_at >= now() - w.win * INTERVAL '1 second'
        GROUP BY w.ord
        ORDER BY w.ord
        "#,
        bucket.as_slice(),
        &window_seconds(thresholds),
    )
    .fetch_all(executor)
    .await?;

    Ok(counts.into_iter().map(saturating_count).collect())
}

/// Drops sessions that can no longer be spent, and records no counter can still
/// see.
pub(crate) async fn prune_admission_records(
    pool: &PgPool,
    settings: &AdmissionSettings,
) -> sqlx::Result<u64> {
    let sessions = sqlx::query!("DELETE FROM admission_session WHERE expires_at < now()")
        .execute(pool)
        .await?
        .rows_affected();
    let sends = sqlx::query!(
        "DELETE FROM admission_send_record
        WHERE created_at < now() - ($1::BIGINT * INTERVAL '1 second')",
        settings.sendthrottle.window.num_seconds(),
    )
    .execute(pool)
    .await?
    .rows_affected();
    let consumptions = sqlx::query!(
        "DELETE FROM admission_consumption_record
        WHERE created_at < now() - ($1::BIGINT * INTERVAL '1 second')",
        longest_window(&settings.quotas).num_seconds(),
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(sessions + sends + consumptions)
}

/// Loads the deployment's endpoint bucket key, generating it on first start.
///
/// The no-op update makes the insert return the existing row, so a concurrent
/// first start cannot end up with two keys.
pub(crate) async fn load_or_generate_endpoint_bucket_key(pool: &PgPool) -> sqlx::Result<BucketKey> {
    let fresh = BucketKey::random();

    let stored = sqlx::query_scalar!(
        "INSERT INTO admission_endpoint_bucket_key (key)
        VALUES ($1)
        ON CONFLICT (singleton) DO UPDATE SET key = admission_endpoint_bucket_key.key
        RETURNING key",
        fresh.as_bytes(),
    )
    .fetch_one(pool)
    .await?;

    BucketKey::from_stored(stored)
}

#[cfg(test)]
mod test {
    use std::sync::Mutex;

    use aircommon::registration::ChallengeKind;
    use chrono::Duration;
    use sqlx::PgPool;
    use tokio_util::sync::CancellationToken;

    use crate::{
        air_service::BackendService,
        settings::{RegistrationSettings, RegistrationThreshold},
    };

    use super::*;

    /// Keeps what it was asked to deliver instead of delivering it. Has
    /// credentials for Apple only, like a deployment with one push platform.
    #[derive(Debug, Default)]
    struct Loopback {
        sent: Mutex<Vec<String>>,
    }

    impl Loopback {
        fn challenges(&self) -> Vec<String> {
            self.sent
                .lock()
                .expect("the sender lock is poisoned")
                .clone()
        }

        fn last_challenge(&self) -> String {
            self.challenges().pop().expect("no challenge was sent")
        }
    }

    #[async_trait]
    impl ChallengeSender for Loopback {
        fn supports(&self, operator: &PushTokenOperator) -> bool {
            matches!(operator, PushTokenOperator::Apple)
        }

        async fn send_challenge(
            &self,
            _push_token: &PushToken,
            _session_id: Uuid,
            challenge: &str,
            _expires_at: DateTime<Utc>,
        ) -> Result<(), ChallengeSendError> {
            self.sent
                .lock()
                .expect("the sender lock is poisoned")
                .push(challenge.to_owned());
            Ok(())
        }
    }

    fn admission(quota: u64, sends: u64) -> AdmissionSettings {
        AdmissionSettings {
            sessionlifetime: Duration::minutes(5),
            sendthrottle: RegistrationThreshold {
                limit: sends,
                window: Duration::hours(1),
            },
            quotas: vec![RegistrationThreshold {
                limit: quota,
                window: Duration::days(1),
            }],
        }
    }

    async fn service(pool: &PgPool, quota: u64, sends: u64) -> (AuthService, Arc<Loopback>) {
        let mut service = AuthService::initialize(
            pool.clone(),
            "example.com".parse().expect("the test domain is a domain"),
            Default::default(),
            CancellationToken::new(),
        )
        .await
        .expect("failed to initialize the auth service");
        service.set_registration_settings(RegistrationSettings {
            challenges: vec![ChallengeKind::AdmissionSession],
            admission: admission(quota, sends),
            ..Default::default()
        });
        let sender = Arc::new(Loopback::default());
        service.set_challenge_sender(sender.clone());
        (service, sender)
    }

    fn push_token(token: &str) -> PushToken {
        PushToken::new(PushTokenOperator::Apple, token.to_owned())
    }

    /// Pairs a session id with the challenge that reached the endpoint, which
    /// is what a client holds when it registers.
    async fn answered_session(
        service: &AuthService,
        sender: &Loopback,
        token: &str,
    ) -> anyhow::Result<AdmissionSession> {
        let session = service.create_admission_session(push_token(token)).await?;
        Ok(AdmissionSession {
            session_id: session.session_id,
            challenge: sender.last_challenge(),
        })
    }

    #[sqlx::test]
    async fn an_answered_session_admits_one_registration(pool: PgPool) -> anyhow::Result<()> {
        let (service, sender) = service(&pool, 2, 10).await;
        let session = answered_session(&service, &sender, "apns-token").await?;

        let mut txn = pool.begin().await?;
        let first = service
            .consume_admission_session(&mut txn, &session)
            .await?;
        txn.commit().await?;
        assert_eq!(first, ChallengeVerdict::Accepted);

        let mut txn = pool.begin().await?;
        let second = service
            .consume_admission_session(&mut txn, &session)
            .await?;

        assert_eq!(second, ChallengeVerdict::Rejected);

        Ok(())
    }

    #[sqlx::test]
    async fn the_id_is_required_to_spend(pool: PgPool) -> anyhow::Result<()> {
        let (service, sender) = service(&pool, 2, 10).await;
        let session = answered_session(&service, &sender, "apns-token").await?;

        let mut txn = pool.begin().await?;
        let result = service
            .consume_admission_session(
                &mut txn,
                &AdmissionSession {
                    session_id: Uuid::new_v4(),
                    challenge: session.challenge,
                },
            )
            .await?;

        assert_eq!(result, ChallengeVerdict::Rejected);

        Ok(())
    }

    #[sqlx::test]
    async fn the_challenge_is_required_to_spend(pool: PgPool) -> anyhow::Result<()> {
        let (service, _sender) = service(&pool, 2, 10).await;
        let session = service
            .create_admission_session(push_token("apns-token"))
            .await?;

        let mut txn = pool.begin().await?;
        let result = service
            .consume_admission_session(
                &mut txn,
                &AdmissionSession {
                    session_id: session.session_id,
                    challenge: "not the challenge".to_owned(),
                },
            )
            .await?;

        assert_eq!(result, ChallengeVerdict::Rejected);

        Ok(())
    }

    /// Guessing is not what the free attempt buys. The challenge is 256 bits
    /// and every guess is a whole registration the gate already counts.
    #[sqlx::test]
    async fn a_wrong_answer_leaves_the_session_spendable(pool: PgPool) -> anyhow::Result<()> {
        let (service, sender) = service(&pool, 2, 10).await;
        let session = answered_session(&service, &sender, "apns-token").await?;

        let mut txn = pool.begin().await?;
        let wrong = service
            .consume_admission_session(
                &mut txn,
                &AdmissionSession {
                    session_id: session.session_id,
                    challenge: "not the challenge".to_owned(),
                },
            )
            .await?;
        txn.commit().await?;
        assert_eq!(wrong, ChallengeVerdict::Rejected);

        let mut txn = pool.begin().await?;
        let right = service
            .consume_admission_session(&mut txn, &session)
            .await?;
        txn.commit().await?;

        assert_eq!(right, ChallengeVerdict::Accepted);

        Ok(())
    }

    #[sqlx::test]
    async fn an_expired_session_admits_nothing(pool: PgPool) -> anyhow::Result<()> {
        let (service, sender) = service(&pool, 2, 10).await;
        let session = answered_session(&service, &sender, "apns-token").await?;

        sqlx::query!(
            "UPDATE admission_session SET expires_at = now() - INTERVAL '1 second'
            WHERE session_id = $1",
            session.session_id,
        )
        .execute(&pool)
        .await?;

        let mut txn = pool.begin().await?;
        let result = service
            .consume_admission_session(&mut txn, &session)
            .await?;

        assert_eq!(result, ChallengeVerdict::Rejected);

        Ok(())
    }

    /// Both sessions are answered before either is spent, so the quota has to
    /// be read again at the registration.
    #[sqlx::test]
    async fn the_quota_caps_accounts_per_endpoint(pool: PgPool) -> anyhow::Result<()> {
        let (service, sender) = service(&pool, 1, 10).await;

        let first = answered_session(&service, &sender, "apns-token").await?;
        let second = answered_session(&service, &sender, "apns-token").await?;

        let mut txn = pool.begin().await?;
        service.consume_admission_session(&mut txn, &first).await?;
        txn.commit().await?;

        let mut txn = pool.begin().await?;
        let result = service.consume_admission_session(&mut txn, &second).await?;

        assert_eq!(result, ChallengeVerdict::Rejected);

        Ok(())
    }

    #[sqlx::test]
    async fn another_endpoint_has_its_own_quota(pool: PgPool) -> anyhow::Result<()> {
        let (service, sender) = service(&pool, 1, 10).await;

        let first = answered_session(&service, &sender, "apns-token").await?;
        let mut txn = pool.begin().await?;
        service.consume_admission_session(&mut txn, &first).await?;
        txn.commit().await?;

        let other = answered_session(&service, &sender, "another-apns-token").await?;
        let mut txn = pool.begin().await?;
        service.consume_admission_session(&mut txn, &other).await?;
        txn.commit().await?;

        Ok(())
    }

    /// A token only means anything to the service that issued it.
    #[sqlx::test]
    async fn platforms_do_not_share_an_endpoint(pool: PgPool) -> anyhow::Result<()> {
        let (service, _sender) = service(&pool, 1, 10).await;

        let apple = service.endpoint_bucket(&PushToken::new(
            PushTokenOperator::Apple,
            "token".to_owned(),
        ));
        let google = service.endpoint_bucket(&PushToken::new(
            PushTokenOperator::Google,
            "token".to_owned(),
        ));

        assert_ne!(apple, google);

        Ok(())
    }

    #[sqlx::test]
    async fn a_rolled_back_registration_keeps_the_session(pool: PgPool) -> anyhow::Result<()> {
        let (service, sender) = service(&pool, 2, 10).await;
        let session = answered_session(&service, &sender, "apns-token").await?;

        let mut txn = pool.begin().await?;
        service
            .consume_admission_session(&mut txn, &session)
            .await?;
        txn.rollback().await?;

        let mut txn = pool.begin().await?;
        let verdict = service
            .consume_admission_session(&mut txn, &session)
            .await?;
        txn.commit().await?;

        assert_eq!(verdict, ChallengeVerdict::Accepted);

        Ok(())
    }

    /// Without the throttle, opening sessions aims push traffic at somebody
    /// else's endpoint.
    #[sqlx::test]
    async fn the_throttle_caps_challenges_per_endpoint(pool: PgPool) -> anyhow::Result<()> {
        let (service, sender) = service(&pool, 10, 2).await;

        for _ in 0..3 {
            service
                .create_admission_session(push_token("apns-token"))
                .await?;
        }

        assert_eq!(sender.challenges().len(), 2);

        Ok(())
    }

    /// The client cannot tell this from a challenge that went missing.
    #[sqlx::test]
    async fn an_exhausted_endpoint_gets_no_challenge(pool: PgPool) -> anyhow::Result<()> {
        let (service, sender) = service(&pool, 1, 10).await;

        let session = answered_session(&service, &sender, "apns-token").await?;
        let mut txn = pool.begin().await?;
        service
            .consume_admission_session(&mut txn, &session)
            .await?;
        txn.commit().await?;

        let sent_before = sender.challenges().len();
        let answer = service
            .create_admission_session(push_token("apns-token"))
            .await?;

        assert_eq!(sender.challenges().len(), sent_before);
        assert_eq!(answer.lifetime, Duration::minutes(5));

        Ok(())
    }

    #[sqlx::test]
    async fn a_malformed_push_token_is_turned_down(pool: PgPool) -> anyhow::Result<()> {
        let (service, _sender) = service(&pool, 2, 10).await;

        for token in ["", "with a space", &"x".repeat(MAX_PUSH_TOKEN_LEN + 1)] {
            let result = service.create_admission_session(push_token(token)).await;
            assert!(matches!(result, Err(AdmissionError::InvalidPushToken)));
        }

        Ok(())
    }

    /// The client falls back at once instead of waiting for a challenge that
    /// cannot be sent.
    #[sqlx::test]
    async fn an_unsupported_platform_is_turned_down(pool: PgPool) -> anyhow::Result<()> {
        let (service, sender) = service(&pool, 2, 10).await;

        let result = service
            .create_admission_session(PushToken::new(
                PushTokenOperator::Google,
                "fcm-token".to_owned(),
            ))
            .await;

        assert!(matches!(result, Err(AdmissionError::Unavailable)));
        assert!(sender.challenges().is_empty());
        let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admission_session")
            .fetch_one(&pool)
            .await?;
        assert_eq!(sessions, 0);

        Ok(())
    }

    #[sqlx::test]
    async fn the_push_token_is_not_stored(pool: PgPool) -> anyhow::Result<()> {
        let (service, sender) = service(&pool, 2, 10).await;
        let token = "a-very-recognizable-token";
        answered_session(&service, &sender, token).await?;

        let rows = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!" FROM admission_session
            WHERE encode(endpoint_bucket, 'escape') LIKE '%' || $1 || '%'"#,
            token,
        )
        .fetch_one(&pool)
        .await?;

        assert_eq!(rows, 0);

        Ok(())
    }

    /// Advertising it would send a client down a route that admits nobody.
    #[sqlx::test]
    async fn a_challenge_with_no_sender_is_not_offered(pool: PgPool) -> anyhow::Result<()> {
        let (mut service, _sender) = service(&pool, 2, 10).await;
        service.challenge_sender = None;

        assert!(service.accepted_challenges().is_empty());

        Ok(())
    }

    #[sqlx::test]
    async fn a_deployment_without_the_challenge_offers_nothing(pool: PgPool) -> anyhow::Result<()> {
        let (mut service, _sender) = service(&pool, 2, 10).await;
        service.set_registration_settings(RegistrationSettings {
            challenges: vec![ChallengeKind::InvitationCode],
            ..Default::default()
        });

        let result = service
            .create_admission_session(push_token("apns-token"))
            .await;

        assert!(matches!(result, Err(AdmissionError::Unavailable)));
        assert!(service.accepted_challenges() == vec![ChallengeKind::InvitationCode]);

        Ok(())
    }

    #[sqlx::test]
    async fn pruning_drops_what_no_counter_can_see(pool: PgPool) -> anyhow::Result<()> {
        let (service, sender) = service(&pool, 2, 10).await;
        let session = answered_session(&service, &sender, "apns-token").await?;
        sqlx::query!(
            "UPDATE admission_session SET expires_at = now() - INTERVAL '1 second'
            WHERE session_id = $1",
            session.session_id,
        )
        .execute(&pool)
        .await?;
        sqlx::query!("UPDATE admission_send_record SET created_at = now() - INTERVAL '2 hours'")
            .execute(&pool)
            .await?;

        let pruned = prune_admission_records(&pool, &admission(2, 10)).await?;

        assert_eq!(pruned, 2);

        Ok(())
    }
}
