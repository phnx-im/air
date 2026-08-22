// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Challenges a gated registration can answer with.
//!
//! A closed gate takes the first configured kind the request carries and
//! consumes exactly that one, inside the registration transaction.
//!
//! Adding a kind means one optional field on `RegisterUserRequest`, one
//! verify-and-consume arm here, and one entry in `registration.challenges`.

use aircommon::registration::{AdmissionSession, ChallengeKind, RegistrationChallenge};
use airprotos::auth_service::v1::RegisterUserRequest;
use metrics::counter;
use sqlx::PgTransaction;
use tracing::{error, warn};

use crate::{
    auth_service::{AuthService, invitation_code_record::InvitationCodeRecord},
    errors::auth_service::RegisterUserError,
};

/// Whether a challenge response admits the registration carrying it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChallengeVerdict {
    Accepted,
    Rejected,
}

impl AuthService {
    /// The first configured kind the request carries, if any.
    ///
    /// Only called for a closed gate, so an unrequired response is left
    /// unspent.
    pub(crate) fn select_challenge(
        &self,
        request: &RegisterUserRequest,
    ) -> Option<RegistrationChallenge> {
        self.registration_gate
            .settings()
            .challenges
            .iter()
            .find_map(|kind| match kind {
                ChallengeKind::InvitationCode => request
                    .invitation_code
                    .as_ref()
                    .map(|code| RegistrationChallenge::InvitationCode(code.code.clone())),
                ChallengeKind::AdmissionSession => request
                    .admission_session
                    .as_ref()
                    .and_then(|session| Some((session.session_id?, session)))
                    .map(|(id, session)| {
                        RegistrationChallenge::AdmissionSession(AdmissionSession {
                            session_id: id.into(),
                            challenge: session.challenge.clone(),
                        })
                    }),
            })
    }

    /// The challenge kinds this server verifies, in its own preference order. A
    /// challenge kind it cannot actually run is not offered.
    pub(crate) fn accepted_challenges(&self) -> Vec<ChallengeKind> {
        self.registration_gate
            .settings()
            .challenges
            .iter()
            .copied()
            .filter(|kind| match kind {
                ChallengeKind::InvitationCode => true,
                ChallengeKind::AdmissionSession => self.has_challenge_sender(),
            })
            .collect()
    }

    /// Verifies a response and consumes it.
    ///
    /// Runs on the registration transaction, so the response is spent exactly
    /// when the account is created.
    pub(crate) async fn consume_challenge(
        &self,
        txn: &mut PgTransaction<'_>,
        challenge: &RegistrationChallenge,
    ) -> Result<ChallengeVerdict, RegisterUserError> {
        let kind = challenge.kind();
        let result = match challenge {
            RegistrationChallenge::InvitationCode(code) => {
                self.consume_invitation_code(txn, code).await
            }
            RegistrationChallenge::AdmissionSession(session) => {
                self.consume_admission_session(txn, session).await
            }
        };

        let outcome = match &result {
            Ok(ChallengeVerdict::Accepted) => "accepted",
            Ok(ChallengeVerdict::Rejected) => "rejected",
            Err(_) => "error",
        };
        counter!(
            "air_registration_challenges_total",
            "type" => kind.as_str(),
            "outcome" => outcome,
        )
        .increment(1);

        result
    }

    async fn consume_invitation_code(
        &self,
        txn: &mut PgTransaction<'_>,
        code: &str,
    ) -> Result<ChallengeVerdict, RegisterUserError> {
        if !InvitationCodeRecord::validate_code(code) {
            return Ok(ChallengeVerdict::Rejected);
        }

        // The unredeemable code is never in the table, so there is nothing to
        // spend and nothing to run out.
        if self.is_unredeemable_code(code) {
            warn!("used secret unredeemable code to register account");
            return Ok(ChallengeVerdict::Accepted);
        }

        let redeemed = InvitationCodeRecord::redeem(txn, code)
            .await
            .map_err(|error| {
                error!(%error, "failed to redeem invitation code");
                RegisterUserError::StorageError
            })?;

        if !redeemed {
            return Ok(ChallengeVerdict::Rejected);
        }

        counter!("air_invitation_codes_redeemed_total").increment(1);
        Ok(ChallengeVerdict::Accepted)
    }
}

#[cfg(test)]
mod test {
    use airprotos::auth_service::v1::InvitationCode;
    use sqlx::PgPool;
    use tokio_util::sync::CancellationToken;

    use crate::{air_service::BackendService, settings::RegistrationSettings};

    use super::*;

    async fn service(pool: &PgPool) -> anyhow::Result<AuthService> {
        Ok(AuthService::initialize(
            pool.clone(),
            "example.com".parse()?,
            None,
            CancellationToken::new(),
        )
        .await?)
    }

    async fn fresh_code(service: &AuthService) -> anyhow::Result<String> {
        service.invitation_codes_generate(1).await?;
        let code = service
            .invitation_codes_list(1, false)
            .await?
            .next()
            .expect("no code was generated")
            .0;
        Ok(code)
    }

    fn request_with_code(code: Option<&str>) -> RegisterUserRequest {
        RegisterUserRequest {
            invitation_code: code.map(|code| InvitationCode {
                code: code.to_owned(),
            }),
            ..Default::default()
        }
    }

    #[sqlx::test]
    async fn selects_the_invitation_code(pool: PgPool) -> anyhow::Result<()> {
        let service = service(&pool).await?;

        let selected = service.select_challenge(&request_with_code(Some("ABCD1234")));

        assert!(matches!(
            selected,
            Some(RegistrationChallenge::InvitationCode(code)) if code == "ABCD1234"
        ));

        Ok(())
    }

    #[sqlx::test]
    async fn selects_nothing_from_a_bare_request(pool: PgPool) -> anyhow::Result<()> {
        let service = service(&pool).await?;

        assert!(service.select_challenge(&request_with_code(None)).is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn selects_nothing_when_the_kind_is_not_configured(pool: PgPool) -> anyhow::Result<()> {
        let mut service = service(&pool).await?;
        service.set_registration_settings(RegistrationSettings {
            challenges: Vec::new(),
            ..Default::default()
        });

        assert!(
            service
                .select_challenge(&request_with_code(Some("ABCD1234")))
                .is_none()
        );

        Ok(())
    }

    #[sqlx::test]
    async fn accepts_a_fresh_code_once(pool: PgPool) -> anyhow::Result<()> {
        let service = service(&pool).await?;
        let code = fresh_code(&service).await?;

        let mut txn = pool.begin().await?;
        let first = service
            .consume_challenge(
                &mut txn,
                &RegistrationChallenge::InvitationCode(code.clone()),
            )
            .await?;
        txn.commit().await?;
        assert_eq!(first, ChallengeVerdict::Accepted);

        let mut txn = pool.begin().await?;
        let second = service
            .consume_challenge(&mut txn, &RegistrationChallenge::InvitationCode(code))
            .await?;

        assert_eq!(second, ChallengeVerdict::Rejected);

        Ok(())
    }

    #[sqlx::test]
    async fn rejects_a_malformed_code(pool: PgPool) -> anyhow::Result<()> {
        let service = service(&pool).await?;

        let mut txn = pool.begin().await?;
        let result = service
            .consume_challenge(
                &mut txn,
                &RegistrationChallenge::InvitationCode("lowercase!".to_owned()),
            )
            .await?;

        assert_eq!(result, ChallengeVerdict::Rejected);

        Ok(())
    }

    #[sqlx::test]
    async fn the_unredeemable_code_is_never_spent(pool: PgPool) -> anyhow::Result<()> {
        let mut service = service(&pool).await?;
        service.set_unredeemable_code("E111E000".to_owned());

        for _ in 0..2 {
            let mut txn = pool.begin().await?;
            let verdict = service
                .consume_challenge(
                    &mut txn,
                    &RegistrationChallenge::InvitationCode("E111E000".to_owned()),
                )
                .await?;
            txn.commit().await?;
            assert_eq!(verdict, ChallengeVerdict::Accepted);
        }

        // It stays out of the table entirely, so it can neither be listed nor
        // marked redeemed.
        assert_eq!(service.invitation_code_stats().await?.count, 0);

        Ok(())
    }

    /// Two registrations racing on one code admit exactly one.
    #[sqlx::test]
    async fn one_code_admits_one_registration(pool: PgPool) -> anyhow::Result<()> {
        let service = service(&pool).await?;
        let code = fresh_code(&service).await?;

        let mut first = pool.begin().await?;
        let verdict = service
            .consume_challenge(
                &mut first,
                &RegistrationChallenge::InvitationCode(code.clone()),
            )
            .await?;
        assert_eq!(verdict, ChallengeVerdict::Accepted);

        // The second transaction waits on the row the first one locked, so the
        // commit has to land while it is waiting.
        let contender = tokio::spawn({
            let service = service.clone();
            let pool = pool.clone();
            async move {
                let mut txn = pool.begin().await?;
                let verdict = service
                    .consume_challenge(&mut txn, &RegistrationChallenge::InvitationCode(code))
                    .await?;
                txn.commit().await?;
                anyhow::Ok(verdict)
            }
        });

        first.commit().await?;

        assert_eq!(contender.await??, ChallengeVerdict::Rejected);

        Ok(())
    }
}
