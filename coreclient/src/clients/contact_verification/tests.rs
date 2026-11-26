// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::{UserCredential, test_utils::create_test_credentials},
    crypto::{aead::keys::WelcomeAttributionInfoEarKey, hash::Hash},
    identifiers::UserId,
    messages::FriendshipToken,
    time::TimeStamp,
};
use airprotos::client::self_group::{
    ContactUnverified, ContactVerificationEntry, ContactVerificationsUpdate, ContactVerified,
    SelfGroupMessage,
};
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use crate::{
    ChatId, ChatMessage, Contact, EventMessage, Message, SystemMessage,
    chats::persistence::tests::test_chat,
    db::access::{DbAccess, WriteDbTransaction},
    groups::client_auth_info::StorableUserCredential,
    job::pending_chat_operation::complete_sent_messages,
};

use super::{
    ContactVerification, ContactVerificationStatus,
    sync::{
        apply_contact_verifications_update, complete_sent_entries, contact_verifications_snapshot,
        set_and_park, staged_entries,
    },
};

fn new_user() -> UserId {
    UserId::random("example.com".parse().unwrap())
}

fn new_credential(user_id: &UserId) -> UserCredential {
    create_test_credentials(user_id.clone())
        .1
        .credential()
        .clone()
}

async fn pin(txn: &mut WriteDbTransaction<'_>, credential: &UserCredential) -> anyhow::Result<()> {
    StorableUserCredential::new(credential.clone())
        .store_checked(&mut *txn)
        .await
}

/// A user with a credential pinned on this device, and its fingerprint.
async fn pinned_user(
    txn: &mut WriteDbTransaction<'_>,
) -> anyhow::Result<(UserId, Hash<UserCredential>)> {
    let user_id = new_user();
    let credential = new_credential(&user_id);
    pin(txn, &credential).await?;
    Ok((user_id, credential.fingerprint()))
}

/// A pinned user who is a contact, with the chat that records changes of the
/// mark.
async fn pinned_contact(
    txn: &mut WriteDbTransaction<'_>,
) -> anyhow::Result<(UserId, Hash<UserCredential>, ChatId)> {
    let (user_id, fingerprint) = pinned_user(txn).await?;
    let chat = test_chat();
    chat.store(&mut *txn).await?;
    Contact {
        user_id: user_id.clone(),
        wai_ear_key: WelcomeAttributionInfoEarKey::random()?,
        friendship_token: FriendshipToken::random()?,
        chat_id: chat.id(),
        supported_features: None,
    }
    .upsert(&mut *txn)
    .await?;
    Ok((user_id, fingerprint, chat.id()))
}

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_767_225_600 + seconds, 0).unwrap()
}

fn set_at(seconds: i64) -> TimeStamp {
    at(seconds).into()
}

/// The system messages in the chat, oldest first, with their times.
async fn records(
    txn: &mut WriteDbTransaction<'_>,
    chat_id: ChatId,
) -> anyhow::Result<Vec<(SystemMessage, DateTime<Utc>)>> {
    let messages = ChatMessage::load_multiple(&mut *txn, chat_id, 100).await?;
    Ok(messages
        .iter()
        .filter_map(|message| match message.message() {
            Message::Event(EventMessage::System(system)) => {
                Some((system.clone(), message.timestamp()))
            }
            Message::Event(EventMessage::Error(_)) | Message::Content(_) => None,
        })
        .collect())
}

fn verified_record(user_id: &UserId) -> SystemMessage {
    SystemMessage::ContactVerified {
        contact: user_id.clone(),
    }
}

fn unverified_record(user_id: &UserId) -> SystemMessage {
    SystemMessage::ContactUnverified {
        contact: user_id.clone(),
    }
}

async fn status(
    txn: &mut WriteDbTransaction<'_>,
    user_id: &UserId,
) -> anyhow::Result<ContactVerificationStatus> {
    Ok(ContactVerification::status(&mut *txn, user_id).await?)
}

fn verified(
    user_id: &UserId,
    fingerprint: Hash<UserCredential>,
    verified_at: DateTime<Utc>,
) -> ContactVerificationEntry {
    ContactVerificationEntry::Verified(ContactVerified {
        user_id: user_id.clone().into(),
        credential_fingerprint: Some(fingerprint),
        verified_at,
    })
}

fn unverified(user_id: &UserId, unverified_at: DateTime<Utc>) -> ContactVerificationEntry {
    ContactVerificationEntry::Unverified(ContactUnverified {
        user_id: user_id.clone().into(),
        unverified_at,
    })
}

#[sqlx::test]
async fn marking_names_the_pinned_credential(pool: SqlitePool) -> anyhow::Result<()> {
    let pool = DbAccess::for_tests(pool);

    pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
        let (user, fingerprint) = pinned_user(txn).await?;
        assert_eq!(
            status(txn, &user).await?,
            ContactVerificationStatus::Unverified
        );

        set_and_park(txn, user.clone(), true, set_at(0)).await?;

        assert_eq!(
            status(txn, &user).await?,
            ContactVerificationStatus::Verified
        );
        assert_eq!(
            staged_entries(&mut *txn).await?,
            vec![verified(&user, fingerprint, at(0))]
        );
        Ok(())
    })
    .await
}

#[sqlx::test]
async fn marking_a_user_without_a_credential_fails(pool: SqlitePool) -> anyhow::Result<()> {
    let pool = DbAccess::for_tests(pool);

    pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
        let user = new_user();

        let result = set_and_park(txn, user.clone(), true, set_at(0)).await;

        assert!(result.is_err());
        assert_eq!(ContactVerification::load(&mut *txn, &user).await?, None);
        assert!(staged_entries(&mut *txn).await?.is_empty());
        Ok(())
    })
    .await
}

#[sqlx::test]
async fn unmarking_folds_into_one_parked_entry(pool: SqlitePool) -> anyhow::Result<()> {
    let pool = DbAccess::for_tests(pool);

    pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
        let (user, _) = pinned_user(txn).await?;

        set_and_park(txn, user.clone(), true, set_at(0)).await?;
        set_and_park(txn, user.clone(), false, set_at(1)).await?;

        assert_eq!(ContactVerification::load(&mut *txn, &user).await?, None);
        assert_eq!(
            staged_entries(&mut *txn).await?,
            vec![unverified(&user, at(1))]
        );
        Ok(())
    })
    .await
}

#[sqlx::test]
async fn complete_sent_keeps_a_user_marked_again(pool: SqlitePool) -> anyhow::Result<()> {
    let pool = DbAccess::for_tests(pool);

    pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
        let (marked_again, _) = pinned_user(txn).await?;
        let (sent_once, _) = pinned_user(txn).await?;
        set_and_park(txn, marked_again.clone(), true, set_at(0)).await?;
        set_and_park(txn, sent_once.clone(), true, set_at(0)).await?;
        let sent = staged_entries(&mut *txn).await?;

        set_and_park(txn, marked_again.clone(), false, set_at(1)).await?;
        complete_sent_entries(txn, &sent).await?;

        assert_eq!(
            staged_entries(&mut *txn).await?,
            vec![unverified(&marked_again, at(1))]
        );
        Ok(())
    })
    .await
}

// The outbound service sends a new commit for as long as an entry stays
// parked. We check the completion here, where a regression fails instead of
// looping.
#[sqlx::test]
async fn an_accepted_commit_drops_the_parked_entries(pool: SqlitePool) -> anyhow::Result<()> {
    let pool = DbAccess::for_tests(pool);

    pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
        let (user, _) = pinned_user(txn).await?;
        set_and_park(txn, user, true, set_at(0)).await?;
        let contacts = staged_entries(&mut *txn).await?;

        let sent = [SelfGroupMessage::ContactVerificationsUpdate(
            ContactVerificationsUpdate { contacts },
        )];
        complete_sent_messages(txn, &sent).await?;

        assert!(staged_entries(&mut *txn).await?.is_empty());
        Ok(())
    })
    .await
}

#[sqlx::test]
async fn sibling_entries_set_the_marks(pool: SqlitePool) -> anyhow::Result<()> {
    let pool = DbAccess::for_tests(pool);

    pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
        let (to_mark, to_mark_fingerprint) = pinned_user(txn).await?;
        let (to_unmark, to_unmark_fingerprint) = pinned_user(txn).await?;
        ContactVerification {
            user_id: to_unmark.clone(),
            credential_fingerprint: to_unmark_fingerprint,
            verified_at: at(0),
        }
        .store(&mut *txn)
        .await?;
        let (without_fingerprint, _) = pinned_user(txn).await?;

        let entries = [
            verified(&to_mark, to_mark_fingerprint, at(1)),
            unverified(&to_unmark, at(1)),
            ContactVerificationEntry::Unknown,
            ContactVerificationEntry::Verified(ContactVerified {
                user_id: without_fingerprint.clone().into(),
                credential_fingerprint: None,
                verified_at: at(1),
            }),
            ContactVerificationEntry::Verified(ContactVerified::default()),
        ];
        apply_contact_verifications_update(txn, &entries).await?;

        assert_eq!(
            status(txn, &to_mark).await?,
            ContactVerificationStatus::Verified
        );
        assert_eq!(
            status(txn, &to_unmark).await?,
            ContactVerificationStatus::Unverified
        );
        assert_eq!(
            ContactVerification::load(&mut *txn, &without_fingerprint).await?,
            None
        );
        assert!(
            staged_entries(&mut *txn).await?.is_empty(),
            "applying a sibling's entries must not park them again"
        );
        Ok(())
    })
    .await
}

#[sqlx::test]
async fn a_parked_change_outlasts_a_sibling_entry(pool: SqlitePool) -> anyhow::Result<()> {
    let pool = DbAccess::for_tests(pool);

    pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
        let (user, fingerprint) = pinned_user(txn).await?;
        set_and_park(txn, user.clone(), true, set_at(0)).await?;

        apply_contact_verifications_update(txn, &[unverified(&user, at(1))]).await?;

        assert_eq!(
            status(txn, &user).await?,
            ContactVerificationStatus::Verified
        );
        assert_eq!(
            staged_entries(&mut *txn).await?,
            vec![verified(&user, fingerprint, at(0))]
        );
        Ok(())
    })
    .await
}

#[sqlx::test]
async fn a_mark_for_another_credential_reads_as_unverified(pool: SqlitePool) -> anyhow::Result<()> {
    let pool = DbAccess::for_tests(pool);

    pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
        let (user, _) = pinned_user(txn).await?;
        let compared_elsewhere = new_credential(&user).fingerprint();

        apply_contact_verifications_update(txn, &[verified(&user, compared_elsewhere, at(0))])
            .await?;

        assert_eq!(
            status(txn, &user).await?,
            ContactVerificationStatus::Unverified
        );
        Ok(())
    })
    .await
}

#[sqlx::test]
async fn a_mark_synced_before_the_credential_counts_once_it_is_pinned(
    pool: SqlitePool,
) -> anyhow::Result<()> {
    let pool = DbAccess::for_tests(pool);

    pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
        let user = new_user();
        let credential = new_credential(&user);

        apply_contact_verifications_update(
            txn,
            &[verified(&user, credential.fingerprint(), at(0))],
        )
        .await?;
        assert_eq!(
            status(txn, &user).await?,
            ContactVerificationStatus::Unverified
        );

        pin(txn, &credential).await?;
        assert_eq!(
            status(txn, &user).await?,
            ContactVerificationStatus::Verified
        );
        Ok(())
    })
    .await
}

#[sqlx::test]
async fn the_snapshot_holds_every_mark(pool: SqlitePool) -> anyhow::Result<()> {
    let pool = DbAccess::for_tests(pool);

    pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
        let (first, first_fingerprint) = pinned_user(txn).await?;
        let (second, second_fingerprint) = pinned_user(txn).await?;
        let (unmarked, _) = pinned_user(txn).await?;
        set_and_park(txn, first.clone(), true, set_at(0)).await?;
        set_and_park(txn, second.clone(), true, set_at(1)).await?;
        set_and_park(txn, unmarked, false, set_at(2)).await?;

        let mut expected = vec![
            verified(&first, first_fingerprint, at(0)),
            verified(&second, second_fingerprint, at(1)),
        ];
        expected.sort_by_key(|entry| entry.user_id().and_then(|id| id.uuid));
        assert_eq!(contact_verifications_snapshot(&mut *txn).await?, expected);
        Ok(())
    })
    .await
}

#[sqlx::test]
async fn marking_records_each_change_in_the_contact_chat(pool: SqlitePool) -> anyhow::Result<()> {
    let pool = DbAccess::for_tests(pool);

    pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
        let (contact, _, chat_id) = pinned_contact(txn).await?;

        set_and_park(txn, contact.clone(), true, set_at(0)).await?;
        set_and_park(txn, contact.clone(), false, set_at(1)).await?;

        assert_eq!(
            records(txn, chat_id).await?,
            vec![
                (verified_record(&contact), at(0)),
                (unverified_record(&contact), at(1)),
            ]
        );
        Ok(())
    })
    .await
}

#[sqlx::test]
async fn setting_the_current_status_does_nothing(pool: SqlitePool) -> anyhow::Result<()> {
    let pool = DbAccess::for_tests(pool);

    pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
        let (contact, fingerprint, chat_id) = pinned_contact(txn).await?;

        set_and_park(txn, contact.clone(), false, set_at(0)).await?;
        assert!(records(txn, chat_id).await?.is_empty());
        assert!(staged_entries(&mut *txn).await?.is_empty());

        set_and_park(txn, contact.clone(), true, set_at(1)).await?;
        set_and_park(txn, contact.clone(), true, set_at(2)).await?;

        assert_eq!(
            records(txn, chat_id).await?,
            vec![(verified_record(&contact), at(1))]
        );
        assert_eq!(
            staged_entries(&mut *txn).await?,
            vec![verified(&contact, fingerprint, at(1))]
        );
        Ok(())
    })
    .await
}

#[sqlx::test]
async fn sibling_changes_are_recorded_at_their_time(pool: SqlitePool) -> anyhow::Result<()> {
    let pool = DbAccess::for_tests(pool);

    pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
        let (contact, fingerprint, chat_id) = pinned_contact(txn).await?;

        apply_contact_verifications_update(txn, &[verified(&contact, fingerprint, at(0))]).await?;
        apply_contact_verifications_update(txn, &[unverified(&contact, at(1))]).await?;

        assert_eq!(
            records(txn, chat_id).await?,
            vec![
                (verified_record(&contact), at(0)),
                (unverified_record(&contact), at(1)),
            ]
        );
        Ok(())
    })
    .await
}

// A sibling that verifies, unverifies and verifies again before its commit
// goes out sends one entry with the last time. The mark takes that time, but
// the status did not change here.
#[sqlx::test]
async fn a_sibling_entry_without_a_change_is_not_recorded(pool: SqlitePool) -> anyhow::Result<()> {
    let pool = DbAccess::for_tests(pool);

    pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
        let (contact, fingerprint, chat_id) = pinned_contact(txn).await?;
        apply_contact_verifications_update(txn, &[verified(&contact, fingerprint, at(0))]).await?;

        apply_contact_verifications_update(txn, &[verified(&contact, fingerprint, at(0))]).await?;
        apply_contact_verifications_update(txn, &[verified(&contact, fingerprint, at(2))]).await?;
        let (not_marked, _, not_marked_chat_id) = pinned_contact(txn).await?;
        apply_contact_verifications_update(txn, &[unverified(&not_marked, at(3))]).await?;

        assert_eq!(
            records(txn, chat_id).await?,
            vec![(verified_record(&contact), at(0))]
        );
        assert_eq!(
            ContactVerification::load(&mut *txn, &contact)
                .await?
                .map(|mark| mark.verified_at),
            Some(at(2))
        );
        assert!(records(txn, not_marked_chat_id).await?.is_empty());
        Ok(())
    })
    .await
}

#[sqlx::test]
async fn a_user_who_is_no_contact_gets_no_record(pool: SqlitePool) -> anyhow::Result<()> {
    let pool = DbAccess::for_tests(pool);

    pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
        let (stranger, stranger_fingerprint) = pinned_user(txn).await?;

        set_and_park(txn, stranger.clone(), true, set_at(0)).await?;
        apply_contact_verifications_update(txn, &[unverified(&stranger, at(1))]).await?;
        apply_contact_verifications_update(
            txn,
            &[verified(&stranger, stranger_fingerprint, at(2))],
        )
        .await?;

        let stored_messages: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM message")
            .fetch_one(txn.as_mut())
            .await?;
        assert_eq!(stored_messages, 0);
        assert_eq!(
            status(txn, &stranger).await?,
            ContactVerificationStatus::Verified
        );
        Ok(())
    })
    .await
}
