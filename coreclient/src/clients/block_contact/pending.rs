// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Synchronizing blocked contacts with the user's other devices.
//!
//! Blocking is device-local state that is mirrored to the user's other devices
//! through the self-group.
use aircommon::identifiers::UserId;
use airprotos::client::self_group::{BlockedContactEntry, ContactBlocked, ContactUnblocked};
use chrono::DateTime;
use tracing::{debug, warn};

use super::BlockedContact;

/// The blocked state of one contact, as stored in `blocked_contact`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BlockedState {
    Blocked(BlockedContact),
    Unblocked { user_id: UserId },
}

/// NB: `blocked_at` loses sub-second precision here.
impl From<&BlockedContact> for BlockedContactEntry {
    fn from(
        BlockedContact {
            user_id,
            last_display_name,
            blocked_at,
        }: &BlockedContact,
    ) -> Self {
        Self::Blocked(ContactBlocked {
            user_id: user_id.clone().into(),
            blocked_at: blocked_at.timestamp().max(0) as u64,
            last_display_name: last_display_name.to_string(),
        })
    }
}

impl From<&BlockedState> for BlockedContactEntry {
    fn from(state: &BlockedState) -> Self {
        match state {
            BlockedState::Blocked(contact) => contact.into(),
            BlockedState::Unblocked { user_id } => Self::Unblocked(ContactUnblocked {
                user_id: user_id.clone().into(),
            }),
        }
    }
}

impl BlockedState {
    /// The state an incoming entry asserts.
    ///
    /// `None` for an entry this client cannot make sense of.
    pub(super) fn from_entry(entry: &BlockedContactEntry) -> Option<Self> {
        match entry {
            BlockedContactEntry::Blocked(ContactBlocked {
                user_id,
                blocked_at,
                last_display_name,
            }) => {
                let Some(blocked_at) = i64::try_from(*blocked_at)
                    .ok()
                    .and_then(|seconds| DateTime::from_timestamp(seconds, 0))
                else {
                    warn!(
                        %blocked_at,
                        "Skipping a blocked-contact entry with an out-of-range timestamp"
                    );
                    return None;
                };
                let last_display_name = last_display_name
                    .parse()
                    .inspect_err(|error| {
                        warn!(
                            %error,
                            "Skipping a blocked-contact entry with an invalid display name"
                        );
                    })
                    .ok()?;
                Some(Self::Blocked(BlockedContact {
                    user_id: user_id.to_user_id().ok()?,
                    last_display_name,
                    blocked_at,
                }))
            }
            BlockedContactEntry::Unblocked(ContactUnblocked { user_id }) => Some(Self::Unblocked {
                user_id: user_id.to_user_id().ok()?,
            }),
            BlockedContactEntry::Unknown => {
                debug!("Skipping a blocked-contact entry with an unknown state");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn user(n: u128) -> UserId {
        UserId::new(Uuid::from_u128(n), "localhost".parse().unwrap())
    }

    fn contact(user_id: &UserId, blocked_at: i64, last_display_name: &str) -> BlockedContact {
        BlockedContact {
            user_id: user_id.clone(),
            last_display_name: last_display_name.parse().unwrap(),
            blocked_at: DateTime::from_timestamp(blocked_at, 0).unwrap(),
        }
    }

    fn blocked_entry(
        user_id: &UserId,
        blocked_at: u64,
        last_display_name: &str,
    ) -> BlockedContactEntry {
        BlockedContactEntry::Blocked(ContactBlocked {
            user_id: user_id.clone().into(),
            blocked_at,
            last_display_name: last_display_name.to_owned(),
        })
    }

    #[test]
    fn entry_conversion_round_trips() {
        let user = user(1);
        for state in [
            BlockedState::Blocked(contact(&user, 10, "Alice")),
            BlockedState::Unblocked {
                user_id: user.clone(),
            },
        ] {
            let entry = BlockedContactEntry::from(&state);
            assert_eq!(BlockedState::from_entry(&entry), Some(state));
        }
    }

    #[test]
    fn from_entry_rejects_unusable_entries() {
        let user = user(1);
        assert_eq!(
            BlockedState::from_entry(&blocked_entry(&user, u64::MAX, "Alice")),
            None,
            "an out-of-range timestamp must be rejected"
        );
        assert_eq!(
            BlockedState::from_entry(&blocked_entry(&user, 10, " \n ")),
            None,
            "a display name that parses to empty must be rejected"
        );
        assert_eq!(
            BlockedState::from_entry(&BlockedContactEntry::Unknown),
            None
        );
    }
}
