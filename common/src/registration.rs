// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Registration challenges, as both sides see them.
//!
//! A deployment can require registration to answer a challenge, either always
//! or once its signup counters cross a threshold. These are the kinds a
//! challenge comes in and the responses to them, shared so that the server's
//! configuration, the wire types, and the client's sign-up flow all name the
//! same thing.

use serde::Deserialize;

/// A kind of challenge a gated registration can be answered with.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChallengeKind {
    InvitationCode,
}

impl ChallengeKind {
    /// Stable name for metric labels.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvitationCode => "invitation_code",
        }
    }
}

/// A response to a registration challenge.
///
/// Short-lived by nature, so it is passed through registration rather than
/// persisted with it.
#[derive(Debug, Clone)]
pub enum RegistrationChallenge {
    InvitationCode(String),
}

impl RegistrationChallenge {
    pub fn kind(&self) -> ChallengeKind {
        match self {
            Self::InvitationCode(_) => ChallengeKind::InvitationCode,
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
