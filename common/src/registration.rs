// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Registration challenges, as both sides see them.
//!
//! A deployment can require registration to answer a challenge, either always
//! or once its signup counters cross a threshold.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

/// A kind of challenge a gated registration can be answered with.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChallengeKind {
    InvitationCode,
    AdmissionSession,
}

impl ChallengeKind {
    /// Stable name for metric labels.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvitationCode => "invitation_code",
            Self::AdmissionSession => "admission_session",
        }
    }
}

/// An answered push-admission session.
///
/// The id arrives over HTTPS and the challenge over the push service, so
/// spending it takes both halves.
#[derive(Debug, Clone)]
pub struct AdmissionSession {
    pub session_id: Uuid,
    pub challenge: String,
}

/// A session the server just opened, with the deadline it has to be spent by.
#[derive(Debug, Clone)]
pub struct NewAdmissionSession {
    pub session_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

/// A response to a registration challenge.
///
/// Short-lived, so it is passed through registration rather than persisted
/// with it.
#[derive(Debug, Clone)]
pub enum RegistrationChallenge {
    InvitationCode(String),
    AdmissionSession(AdmissionSession),
}

impl RegistrationChallenge {
    pub fn kind(&self) -> ChallengeKind {
        match self {
            Self::InvitationCode(_) => ChallengeKind::InvitationCode,
            Self::AdmissionSession(_) => ChallengeKind::AdmissionSession,
        }
    }
}

/// What a server says about registering with it right now.
#[derive(Debug, Clone)]
pub struct RegistrationInfo {
    /// Whether a registration has to carry a challenge response.
    ///
    /// This can change before the registration lands, so a client still has to
    /// handle being told a challenge is required.
    pub challenge_required: bool,
    /// The kinds the server verifies, in its own preference order. Kinds this
    /// client does not know are dropped.
    pub accepted_challenges: Vec<ChallengeKind>,
}
