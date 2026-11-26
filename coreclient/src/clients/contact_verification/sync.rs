// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Synchronizing verification marks with the user's other devices.
//!
//! Marking a user updates the mark locally and parks a
//! [`ContactVerificationEntry`] for the next self-group commit. Each device
//! records a change of the mark in the chat with the contact, at the time of
//! the change the entry contains.

use aircommon::{codec::PersistenceCodec, identifiers::UserId, time::TimeStamp};
use airprotos::client::self_group::{ContactUnverified, ContactVerificationEntry, ContactVerified};
use anyhow::Context as _;
use chrono::{DateTime, Utc};
use tracing::debug;

use crate::{
    Contact, SystemMessage,
    chats::messages::TimestampedMessage,
    clients::{
        CoreUser,
        self_group_outbox::{self, OutboxKind},
    },
    db::access::{ReadConnection, WriteDbTransaction},
    groups::client_auth_info::StorableUserCredential,
};

use super::ContactVerification;

/// The write side of [`CoreUser::set_contact_verified`]. Does nothing if the
/// user already has the requested status.
pub(super) async fn set_and_park(
    txn: &mut WriteDbTransaction<'_>,
    user_id: UserId,
    verified: bool,
    at: TimeStamp,
) -> anyhow::Result<()> {
    let stored = ContactVerification::load(&mut *txn, &user_id).await?;
    let entry = if verified {
        let credential_fingerprint = StorableUserCredential::load_fingerprint(&mut *txn, &user_id)
            .await?
            .with_context(|| format!("Can't verify {user_id:?}, no credential is stored"))?;
        if stored.is_some_and(|stored| stored.credential_fingerprint == credential_fingerprint) {
            return Ok(());
        }
        let mark = ContactVerification {
            user_id,
            credential_fingerprint,
            verified_at: *at,
        };
        mark.store(&mut *txn).await?;
        record_verified(txn, &mark).await?;
        ContactVerificationEntry::from(&mark)
    } else {
        if stored.is_none() {
            return Ok(());
        }
        ContactVerification::delete(&mut *txn, &user_id).await?;
        record_unverified(txn, &user_id, *at).await?;
        ContactVerificationEntry::Unverified(ContactUnverified {
            user_id: user_id.into(),
            unverified_at: *at,
        })
    };
    let key = outbox_key(&entry).context("A verification entry names no user")?;
    self_group_outbox::stage(
        txn,
        OutboxKind::ContactVerification,
        &key,
        &PersistenceCodec::to_vec(&entry)?,
        None,
    )
    .await?;
    Ok(())
}

impl From<&ContactVerification> for ContactVerificationEntry {
    fn from(mark: &ContactVerification) -> Self {
        Self::Verified(ContactVerified {
            user_id: mark.user_id.clone().into(),
            credential_fingerprint: Some(mark.credential_fingerprint),
            verified_at: mark.verified_at,
        })
    }
}

/// Stores the record of a change of the mark in the chat with the contact. A
/// user who is no contact has no such chat, and we record nothing.
async fn record_change(
    txn: &mut WriteDbTransaction<'_>,
    user_id: &UserId,
    record: SystemMessage,
    at: DateTime<Utc>,
) -> anyhow::Result<()> {
    let Some(contact) = Contact::load(&mut *txn, user_id).await? else {
        return Ok(());
    };
    let record = TimestampedMessage::system_message(record, at.into());
    CoreUser::store_new_messages(txn, contact.chat_id, vec![record]).await?;
    Ok(())
}

async fn record_verified(
    txn: &mut WriteDbTransaction<'_>,
    mark: &ContactVerification,
) -> anyhow::Result<()> {
    let record = SystemMessage::ContactVerified {
        contact: mark.user_id.clone(),
    };
    record_change(txn, &mark.user_id, record, mark.verified_at).await
}

async fn record_unverified(
    txn: &mut WriteDbTransaction<'_>,
    user_id: &UserId,
    unverified_at: DateTime<Utc>,
) -> anyhow::Result<()> {
    let record = SystemMessage::ContactUnverified {
        contact: user_id.clone(),
    };
    record_change(txn, user_id, record, unverified_at).await
}

enum DecodedEntry {
    Verified(ContactVerification),
    Unverified {
        user_id: UserId,
        unverified_at: DateTime<Utc>,
    },
}

fn decode_entry(entry: &ContactVerificationEntry) -> Option<DecodedEntry> {
    let user_id = entry.user_id()?.to_user_id().ok()?;
    match entry {
        ContactVerificationEntry::Verified(ContactVerified {
            credential_fingerprint: Some(credential_fingerprint),
            verified_at,
            ..
        }) => Some(DecodedEntry::Verified(ContactVerification {
            user_id,
            credential_fingerprint: *credential_fingerprint,
            verified_at: *verified_at,
        })),
        ContactVerificationEntry::Unverified(ContactUnverified { unverified_at, .. }) => {
            Some(DecodedEntry::Unverified {
                user_id,
                unverified_at: *unverified_at,
            })
        }
        ContactVerificationEntry::Verified(_) | ContactVerificationEntry::Unknown => None,
    }
}

/// The outbox key of the user mentioned in the entry. We key by the wire form
/// of the user id, as blocked contacts do.
fn outbox_key(entry: &ContactVerificationEntry) -> Option<Vec<u8>> {
    entry
        .user_id()
        .and_then(|id| PersistenceCodec::to_vec(id).ok())
}

async fn is_parked(
    connection: impl ReadConnection,
    entry: &ContactVerificationEntry,
) -> anyhow::Result<bool> {
    let Some(key) = outbox_key(entry) else {
        return Ok(false);
    };
    let parked = self_group_outbox::load(connection, OutboxKind::ContactVerification, &key).await?;
    Ok(parked.is_some())
}

pub(crate) async fn staged_entries(
    connection: impl ReadConnection,
) -> anyhow::Result<Vec<ContactVerificationEntry>> {
    self_group_outbox::load_kind(connection, OutboxKind::ContactVerification)
        .await?
        .iter()
        .map(|entry| Ok(PersistenceCodec::from_slice(&entry.payload)?))
        .collect()
}

/// Drops the parked changes an accepted commit contained. A user marked again
/// while the commit was in flight keeps the newer change parked.
pub(crate) async fn complete_sent_entries(
    txn: &mut WriteDbTransaction<'_>,
    sent: &[ContactVerificationEntry],
) -> anyhow::Result<()> {
    for entry in sent {
        let Some(key) = outbox_key(entry) else {
            continue;
        };
        self_group_outbox::complete_sent(
            &mut *txn,
            OutboxKind::ContactVerification,
            &key,
            &PersistenceCodec::to_vec(entry)?,
        )
        .await?;
    }
    Ok(())
}

/// Applies the entries of a sibling's accepted commit, or of the provisioning
/// package.
///
/// A mark that starts naming a credential and a removed mark get a record in
/// the chat with the contact. The provisioning package arrives before any chat
/// exists, so it records nothing.
pub(crate) async fn apply_contact_verifications_update(
    txn: &mut WriteDbTransaction<'_>,
    entries: &[ContactVerificationEntry],
) -> anyhow::Result<()> {
    for entry in entries {
        let Some(decoded) = decode_entry(entry) else {
            debug!("Skipping a contact verification entry this client cannot use");
            continue;
        };
        if is_parked(&mut *txn, entry).await? {
            continue;
        }
        match decoded {
            DecodedEntry::Verified(mark) => {
                let stored = ContactVerification::load(&mut *txn, &mark.user_id).await?;
                if stored.as_ref() == Some(&mark) {
                    continue;
                }
                mark.store(&mut *txn).await?;
                let names_new_credential = stored.is_none_or(|stored| {
                    stored.credential_fingerprint != mark.credential_fingerprint
                });
                if names_new_credential {
                    record_verified(txn, &mark).await?;
                }
            }
            DecodedEntry::Unverified {
                user_id,
                unverified_at,
            } => {
                if ContactVerification::load(&mut *txn, &user_id)
                    .await?
                    .is_none()
                {
                    continue;
                }
                ContactVerification::delete(&mut *txn, &user_id).await?;
                record_unverified(txn, &user_id, unverified_at).await?;
            }
        }
    }
    Ok(())
}

/// Snapshot used in device linking.
pub(crate) async fn contact_verifications_snapshot(
    connection: impl ReadConnection,
) -> sqlx::Result<Vec<ContactVerificationEntry>> {
    Ok(ContactVerification::load_all(connection)
        .await?
        .iter()
        .map(ContactVerificationEntry::from)
        .collect())
}
