// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Event IDs of system messages.
//!
//! Each device of the user stores its own copy of a system message. We need a
//! stable event ID to be able to reference these messages in order to
//! synchronize the deletion of system messages across devices. Depending on the
//! system message, we use different information to derive the event ID. The
//! following rules apply:
//!
//! - A group change takes the group ID, the DS fan-out time and the users it
//!   names. The committer gets that time back from the DS and the siblings get
//!   it with the commit.
//! - Group creation and deletion and the steps of a request happen once per
//!   group, so they only take the group ID. Their times differ between devices.
//! - The announcement of a received request may be stored in a different chat
//!   on each device. It takes the sender and the time the request arrived.

use aircommon::{
    crypto::{Labeled, hash::Hashable},
    identifiers::{EventId, UserId},
    time::TimeStamp,
};
use openmls::group::GroupId;
use tls_codec::{TlsSerialize, TlsSize};

use super::SystemMessage;

#[derive(Clone, Copy)]
#[repr(u8)]
enum Kind {
    Add = 1,
    Remove = 2,
    ChangeTitle = 3,
    ChangePicture = 4,
    CreateGroup = 5,
    DeleteGroup = 6,
    ReceivedRequest = 7,
    SentRequest = 8,
    AcceptedRequest = 9,
    RequestConfirmed = 10,
    RequestUnavailable = 11,
    RequestExpired = 12,
    SentRequestExpired = 13,
}

impl SystemMessage {
    /// Calculate a stable event ID for a system message.
    pub(crate) fn event_id(&self, group_id: &GroupId, timestamp: TimeStamp) -> Option<EventId> {
        let group = Some(group_id);
        let at = Some(timestamp);
        let event_id = match self {
            SystemMessage::Add(Some(actor), subject) => {
                derive(Kind::Add, group, at, &[actor, subject])
            }
            SystemMessage::Remove(Some(actor), subject) => {
                derive(Kind::Remove, group, at, &[actor, subject])
            }
            SystemMessage::ChangeTitle { user_id, .. } => {
                derive(Kind::ChangeTitle, group, at, &[user_id])
            }
            SystemMessage::ChangePicture(user_id) => {
                derive(Kind::ChangePicture, group, at, &[user_id])
            }
            SystemMessage::CreateGroup(_) => derive(Kind::CreateGroup, group, None, &[]),
            SystemMessage::DeleteGroup(_) => derive(Kind::DeleteGroup, group, None, &[]),
            SystemMessage::ReceivedDirectConnectionRequest { sender, .. }
            | SystemMessage::ReceivedHandleConnectionRequest { sender, .. }
            | SystemMessage::ReceivedGroupConnectionRequest { sender, .. }
            | SystemMessage::ReceivedAdditionalUsernameConnectionRequest { sender, .. }
            | SystemMessage::ReceivedAdditionalDirectConnectionRequest { sender, .. }
            | SystemMessage::ReceivedAdditionalGroupConnectionRequest { sender, .. } => {
                derive(Kind::ReceivedRequest, None, at, &[sender])
            }
            SystemMessage::NewHandleConnectionChat(_)
            | SystemMessage::NewDirectConnectionChat(_)
            | SystemMessage::SentGroupConnectionRequest { .. } => {
                derive(Kind::SentRequest, group, None, &[])
            }
            SystemMessage::AcceptedConnectionRequest { .. } => {
                derive(Kind::AcceptedRequest, group, None, &[])
            }
            SystemMessage::ReceivedConnectionConfirmation { .. } => {
                derive(Kind::RequestConfirmed, group, None, &[])
            }
            SystemMessage::ConnectionRequestUnavailable => {
                derive(Kind::RequestUnavailable, group, None, &[])
            }
            SystemMessage::ConnectionRequestExpired => {
                derive(Kind::RequestExpired, group, None, &[])
            }
            SystemMessage::SentConnectionRequestExpired => {
                derive(Kind::SentRequestExpired, group, None, &[])
            }
            SystemMessage::Add(None, _)
            | SystemMessage::Remove(None, _)
            | SystemMessage::Onboarded
            | SystemMessage::DeviceLinked(_)
            | SystemMessage::DeviceUnlinked(_)
            | SystemMessage::SelfChatCreated => return None,
        };
        Some(event_id)
    }
}

#[derive(TlsSize, TlsSerialize)]
struct EventIdInput<'a> {
    kind: u8,
    group_id: Option<&'a GroupId>,
    timestamp_ms: Option<u64>,
    users: &'a [&'a UserId],
}

impl Labeled for EventIdInput<'_> {
    const LABEL: &'static str = "EventId";
}

impl Hashable for EventIdInput<'_> {}

// Final step when deriving a stable `EventId`
fn derive(
    kind: Kind,
    group_id: Option<&GroupId>,
    timestamp: Option<TimeStamp>,
    users: &[&UserId],
) -> EventId {
    let input = EventIdInput {
        kind: kind as u8,
        group_id,
        timestamp_ms: timestamp.map(|timestamp| timestamp.timestamp_millis().cast_unsigned()),
        users,
    };
    EventId::from(input.hash().into_bytes())
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::Username;
    use chrono::{Duration, TimeZone, Utc};
    use uuid::Uuid;

    use crate::ChatId;

    use super::*;

    fn user(n: u128) -> UserId {
        UserId::new(Uuid::from_u128(n), "example.com".parse().unwrap())
    }

    fn group(n: u8) -> GroupId {
        GroupId::from_slice(&[n; 16])
    }

    fn at(millis: i64) -> TimeStamp {
        at_micros(millis * 1000)
    }

    fn at_micros(micros: i64) -> TimeStamp {
        (Utc.timestamp_opt(1_700_000_000, 0).unwrap() + Duration::microseconds(micros)).into()
    }

    fn id(message: &SystemMessage, group_id: &GroupId, timestamp: TimeStamp) -> EventId {
        message.event_id(group_id, timestamp).unwrap()
    }

    #[test]
    fn a_group_change_depends_on_group_time_and_users() {
        let add = SystemMessage::Add(Some(user(1)), user(2));
        let base = id(&add, &group(1), at(0));

        assert_eq!(id(&add, &group(1), at(0)), base);
        assert_ne!(id(&add, &group(2), at(0)), base);
        assert_ne!(id(&add, &group(1), at(1)), base);
        assert_ne!(
            id(
                &SystemMessage::Add(Some(user(1)), user(3)),
                &group(1),
                at(0)
            ),
            base
        );
        assert_ne!(
            id(
                &SystemMessage::Add(Some(user(3)), user(2)),
                &group(1),
                at(0)
            ),
            base
        );
        assert_ne!(
            id(
                &SystemMessage::Remove(Some(user(1)), user(2)),
                &group(1),
                at(0)
            ),
            base
        );
    }

    #[test]
    fn the_time_counts_in_milliseconds() {
        let add = SystemMessage::Add(Some(user(1)), user(2));
        assert_eq!(
            id(&add, &group(1), at_micros(5_999)),
            id(&add, &group(1), at(5))
        );
    }

    #[test]
    fn a_title_change_ignores_the_old_title() {
        let change = |old_title: &str| SystemMessage::ChangeTitle {
            user_id: user(1),
            old_title: old_title.to_owned(),
            new_title: "New".to_owned(),
        };
        assert_eq!(
            id(&change("Old"), &group(1), at(0)),
            id(&change("Older"), &group(1), at(0))
        );
    }

    #[test]
    fn group_creation_and_deletion_ignore_the_time() {
        for message in [
            SystemMessage::CreateGroup(user(1)),
            SystemMessage::DeleteGroup(user(1)),
        ] {
            assert_eq!(
                id(&message, &group(1), at(0)),
                id(&message, &group(1), at(9))
            );
            assert_ne!(
                id(&message, &group(1), at(0)),
                id(&message, &group(2), at(0))
            );
        }
        assert_ne!(
            id(&SystemMessage::CreateGroup(user(1)), &group(1), at(0)),
            id(&SystemMessage::DeleteGroup(user(1)), &group(1), at(0))
        );
    }

    /// Devices disagree on whether a request is additional and on the chat
    /// that shows it.
    #[test]
    fn a_received_request_depends_on_sender_and_time_only() {
        let username = Username::new("ellie-03".to_owned()).unwrap();
        let first =
            SystemMessage::received_username_connection_request(user(1), username.clone(), false);
        let additional =
            SystemMessage::received_username_connection_request(user(1), username, true);
        let base = id(&first, &group(1), at(0));

        assert_eq!(id(&additional, &group(2), at(0)), base);
        assert_ne!(id(&first, &group(1), at(1)), base);
        assert_ne!(
            id(
                &SystemMessage::received_group_connection_request(user(2), ChatId::random(), false),
                &group(1),
                at(0)
            ),
            base
        );
    }

    #[test]
    fn the_steps_of_a_request_depend_on_the_group_only() {
        let steps = [
            SystemMessage::NewHandleConnectionChat(Username::new("joel-03".to_owned()).unwrap()),
            SystemMessage::AcceptedConnectionRequest {
                contact: user(1),
                user_handle: None,
            },
            SystemMessage::ReceivedConnectionConfirmation {
                sender: user(1),
                user_handle: None,
            },
            SystemMessage::ConnectionRequestUnavailable,
            SystemMessage::ConnectionRequestExpired,
            SystemMessage::SentConnectionRequestExpired,
        ];
        let mut ids = Vec::new();
        for step in &steps {
            assert_eq!(id(step, &group(1), at(0)), id(step, &group(1), at(9)));
            ids.push(id(step, &group(1), at(0)));
        }
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), steps.len());
    }

    /// An older sibling announces a request through a group chat without that
    /// chat.
    #[test]
    fn a_sent_request_matches_with_or_without_its_group_chat() {
        assert_eq!(
            id(
                &SystemMessage::NewDirectConnectionChat(user(1)),
                &group(1),
                at(0)
            ),
            id(
                &SystemMessage::SentGroupConnectionRequest {
                    recipient: user(1),
                    origin_chat_id: ChatId::random(),
                },
                &group(1),
                at(0)
            )
        );
    }

    #[test]
    fn copies_on_one_device_only_get_no_id() {
        for message in [
            SystemMessage::Add(None, user(1)),
            SystemMessage::Remove(None, user(1)),
            SystemMessage::Onboarded,
            SystemMessage::DeviceLinked(Uuid::from_u128(1)),
            SystemMessage::DeviceUnlinked(Uuid::from_u128(1)),
            SystemMessage::SelfChatCreated,
        ] {
            assert_eq!(message.event_id(&group(1), at(0)), None, "{message:?}");
        }
    }

    /// Devices on different versions must derive the same ID.
    #[test]
    fn the_id_is_stable() {
        let add = SystemMessage::Add(Some(user(1)), user(2));
        insta::assert_debug_snapshot!(id(&add, &group(1), at(0)));
    }
}
