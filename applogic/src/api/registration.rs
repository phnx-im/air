// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Registration challenges
//!
//! A deployment can require a sign-up to answer a challenge. The flow asks what
//! is needed before it draws its steps, and handles being told a challenge is
//! needed after all, since the answer can change in between.

use aircommon::identifiers::Fqdn;
pub(crate) use aircommon::registration::{
    AdmissionSession, ChallengeKind, NewAdmissionSession, RegistrationChallenge, RegistrationInfo,
};
use aircoreclient::clients::{CoreUser, registration::RegistrationError};
use chrono::Duration;
use flutter_rust_bridge::frb;
use uuid::Uuid;

use super::user::PlatformPushToken;

#[doc(hidden)]
#[frb(mirror(ChallengeKind))]
pub enum _ChallengeKind {
    InvitationCode,
    AdmissionSession,
}

#[doc(hidden)]
#[frb(mirror(RegistrationChallenge))]
pub enum _RegistrationChallenge {
    InvitationCode(String),
    AdmissionSession(AdmissionSession),
}

#[doc(hidden)]
#[frb(mirror(AdmissionSession))]
pub struct _AdmissionSession {
    pub session_id: Uuid,
    pub challenge: String,
}

#[doc(hidden)]
#[frb(mirror(NewAdmissionSession))]
pub struct _NewAdmissionSession {
    pub session_id: Uuid,
    pub lifetime: Duration,
}

#[doc(hidden)]
#[frb(mirror(RegistrationInfo))]
#[frb(dart_metadata = ("freezed"))]
pub struct _RegistrationInfo {
    pub challenge_required: bool,
    pub accepted_challenges: Vec<ChallengeKind>,
}

/// Asks a server whether signing up with it needs a challenge right now.
pub async fn get_registration_info(domain: String) -> anyhow::Result<RegistrationInfo> {
    let domain: Fqdn = domain.parse()?;
    CoreUser::get_registration_info(domain).await
}

/// Asks the server to send a challenge to this device's push endpoint.
pub async fn create_admission_session(
    domain: String,
    push_token: PlatformPushToken,
) -> anyhow::Result<NewAdmissionSession> {
    let domain: Fqdn = domain.parse()?;
    CoreUser::create_admission_session(domain, push_token.into()).await
}

/// Why a sign-up did not produce an account.
#[frb(dart_metadata = ("freezed"))]
pub enum CreateUserError {
    /// The server wants one of these kinds of challenge answered first.
    ChallengeRequired { accepted: Vec<ChallengeKind> },
    /// The server turned down the challenge response.
    ChallengeRejected,
    /// Anything else, carrying the message for the log and the banner.
    Other { message: String },
}

impl CreateUserError {
    /// Splits the gate's two answers out of a failed registration.
    pub(crate) fn from_anyhow(error: anyhow::Error) -> Self {
        match error.downcast::<RegistrationError>() {
            Ok(RegistrationError::ChallengeRequired(accepted)) => {
                Self::ChallengeRequired { accepted }
            }
            Ok(RegistrationError::ChallengeRejected) => Self::ChallengeRejected,
            Err(error) => Self::Other {
                message: error.to_string(),
            },
        }
    }
}
