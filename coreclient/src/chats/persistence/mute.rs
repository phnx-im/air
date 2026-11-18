// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Synchronizing chat mutes with the user's other devices.
//!
//! Muting a chat updates it locally and parks a [`ChatMuteEntry`] for the next
//! self-group commit. The siblings apply the entry when the commit arrives.

use aircommon::codec::PersistenceCodec;
use airprotos::client::self_group::{ChatMuteEntry, MutedChat, UnmutedChat};
use anyhow::Context as _;
use chrono::SubsecRound;
use openmls::group::GroupId;
use tracing::debug;

use crate::{
    ChatId,
    chats::{Chat, ChatMuted},
    clients::self_group_outbox::{self, OutboxKind},
    db::access::{ReadConnection, WriteDbTransaction},
    outbound_service::resync::Resync,
};

/// Mutes or unmutes the chat and parks the change for the siblings.
pub(crate) async fn set_and_park_mute(
    txn: &mut WriteDbTransaction<'_>,
    chat_id: ChatId,
    muted_until: Option<ChatMuted>,
) -> anyhow::Result<()> {
    let group_id = Chat::load_group_id(&mut *txn, chat_id)
        .await?
        .with_context(|| format!("Can't mute chat {chat_id}, it does not exist"))?;
    // Siblings receive the end in milliseconds. We store the same value here.
    let muted_until = muted_until.map(|muted| match muted {
        ChatMuted::Until(until) => ChatMuted::Until(until.trunc_subsecs(3)),
        ChatMuted::Forever => ChatMuted::Forever,
    });
    Chat::set_muted_until(&mut *txn, chat_id, muted_until).await?;
    let entry = mute_entry(&group_id, muted_until);
    self_group_outbox::stage(
        txn,
        OutboxKind::ChatMute,
        group_id.as_slice(),
        &PersistenceCodec::to_vec(&entry)?,
        None,
    )
    .await?;
    Ok(())
}

/// The entry that sets the chat of `group_id` to `muted_until`.
fn mute_entry(group_id: &GroupId, muted_until: Option<ChatMuted>) -> ChatMuteEntry {
    let group_id = Some(group_id.clone());
    match muted_until {
        Some(ChatMuted::Until(until)) => ChatMuteEntry::Muted(MutedChat {
            group_id,
            until: Some(until),
        }),
        Some(ChatMuted::Forever) => ChatMuteEntry::Muted(MutedChat {
            group_id,
            until: None,
        }),
        None => ChatMuteEntry::Unmuted(UnmutedChat { group_id }),
    }
}

/// The group an entry names and the mute it sets, or `None` for an entry this
/// client cannot use.
fn decode_mute_entry(entry: &ChatMuteEntry) -> Option<(&GroupId, Option<ChatMuted>)> {
    let muted_until = match entry {
        ChatMuteEntry::Muted(MutedChat { until, .. }) => {
            Some(until.map_or(ChatMuted::Forever, ChatMuted::Until))
        }
        ChatMuteEntry::Unmuted(_) => None,
        ChatMuteEntry::Unknown => return None,
    };
    Some((entry.group_id()?, muted_until))
}

pub(crate) async fn staged_mutes(
    connection: impl ReadConnection,
) -> anyhow::Result<Vec<ChatMuteEntry>> {
    self_group_outbox::load_kind(connection, OutboxKind::ChatMute)
        .await?
        .iter()
        .map(|entry| Ok(PersistenceCodec::from_slice(&entry.payload)?))
        .collect()
}

/// Drops the parked changes an accepted commit contained. A chat muted again
/// while the commit was in flight keeps its newer change parked.
pub(crate) async fn complete_sent_mutes(
    txn: &mut WriteDbTransaction<'_>,
    sent: &[ChatMuteEntry],
) -> anyhow::Result<()> {
    for entry in sent {
        let Some(group_id) = entry.group_id() else {
            continue;
        };
        self_group_outbox::complete_sent(
            &mut *txn,
            OutboxKind::ChatMute,
            group_id.as_slice(),
            &PersistenceCodec::to_vec(entry)?,
        )
        .await?;
    }
    Ok(())
}

/// Applies the entries of a sibling's accepted commit.
///
/// The last entry for a chat wins. We skip a chat whose own change is still
/// parked. The DS accepts our commit after the sibling's, and our change then
/// reaches every device. Applying the sibling's entry here would leave this
/// device on a value the others no longer hold.
pub(crate) async fn apply_chat_mutes_update(
    txn: &mut WriteDbTransaction<'_>,
    entries: &[ChatMuteEntry],
) -> anyhow::Result<()> {
    for entry in entries {
        let Some((group_id, muted_until)) = decode_mute_entry(entry) else {
            debug!("Skipping a chat mute entry this client cannot use");
            continue;
        };
        let parked = self_group_outbox::load(&mut *txn, OutboxKind::ChatMute, group_id.as_slice())
            .await?
            .is_some();
        if parked {
            continue;
        }
        // The chat of a group this device still onboards into does not exist
        // yet. The onboarding creates it with the mute recorded here.
        Resync::record_mute(&mut *txn, group_id, muted_until).await?;
        if let Some(chat_id) = ChatId::load_from_group_id(&mut *txn, group_id).await? {
            Chat::set_muted_until(&mut *txn, chat_id, muted_until).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use aircommon::{
        crypto::aead::keys::{GroupStateEarKey, IdentityLinkWrapperKey},
        identifiers::QualifiedGroupId,
    };
    use chrono::{DateTime, TimeDelta, Utc};
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::{
        ChatAttributes,
        clients::{CoreUser, multi_device::HigherLevelGroup},
        db::access::DbAccess,
    };

    use super::*;

    async fn store_chat(txn: &mut WriteDbTransaction<'_>) -> anyhow::Result<Chat> {
        let group_id = QualifiedGroupId::new(Uuid::new_v4(), "example.com".parse()?).into();
        let chat = Chat::new_group_chat(group_id, ChatAttributes::new("Test chat".into(), None));
        chat.store(&mut *txn).await?;
        Ok(chat)
    }

    async fn muted_until(
        txn: &mut WriteDbTransaction<'_>,
        chat: &Chat,
    ) -> anyhow::Result<Option<ChatMuted>> {
        let chat = Chat::load(&mut *txn, &chat.id())
            .await?
            .context("chat is gone")?;
        Ok(chat.muted_until)
    }

    fn until() -> DateTime<Utc> {
        DateTime::from_timestamp_millis(1_767_225_600_123).unwrap()
    }

    fn muted(chat: &Chat, until: Option<DateTime<Utc>>) -> ChatMuteEntry {
        ChatMuteEntry::Muted(MutedChat {
            group_id: Some(chat.group_id().clone()),
            until,
        })
    }

    fn unmuted(chat: &Chat) -> ChatMuteEntry {
        ChatMuteEntry::Unmuted(UnmutedChat {
            group_id: Some(chat.group_id().clone()),
        })
    }

    /// A group from the linking snapshot, with no chat on this device yet.
    fn onboarding_group(muted_until: Option<ChatMuted>) -> anyhow::Result<HigherLevelGroup> {
        Ok(HigherLevelGroup {
            group_id: QualifiedGroupId::new(Uuid::new_v4(), "example.com".parse()?).into(),
            pq_group_id: None,
            group_state_ear_key: GroupStateEarKey::random()?,
            identity_link_wrapper_key: IdentityLinkWrapperKey::random()?,
            vc_leaf_index: 0,
            connection: None,
            outgoing_request: None,
            muted_until,
        })
    }

    #[sqlx::test]
    async fn muting_again_folds_into_one_parked_entry(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let chat = store_chat(txn).await?;

            set_and_park_mute(txn, chat.id(), Some(ChatMuted::Forever)).await?;
            set_and_park_mute(txn, chat.id(), Some(ChatMuted::Until(until()))).await?;

            assert_eq!(
                muted_until(txn, &chat).await?,
                Some(ChatMuted::Until(until()))
            );
            assert_eq!(
                staged_mutes(&mut *txn).await?,
                vec![muted(&chat, Some(until()))]
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn muting_an_unknown_chat_fails(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let chat_id = ChatId::new(Uuid::new_v4());

            let result = set_and_park_mute(txn, chat_id, Some(ChatMuted::Forever)).await;

            assert!(result.is_err());
            assert!(staged_mutes(&mut *txn).await?.is_empty());
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn complete_sent_keeps_a_chat_muted_again(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let muted_again = store_chat(txn).await?;
            let unmuted_chat = store_chat(txn).await?;
            set_and_park_mute(txn, muted_again.id(), Some(ChatMuted::Forever)).await?;
            set_and_park_mute(txn, unmuted_chat.id(), None).await?;
            let sent = staged_mutes(&mut *txn).await?;

            set_and_park_mute(txn, muted_again.id(), Some(ChatMuted::Until(until()))).await?;
            complete_sent_mutes(txn, &sent).await?;

            assert_eq!(
                staged_mutes(&mut *txn).await?,
                vec![muted(&muted_again, Some(until()))]
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn sibling_entries_set_the_mute_of_known_chats(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let to_unmute = store_chat(txn).await?;
            let to_mute = store_chat(txn).await?;
            let muted_twice = store_chat(txn).await?;
            Chat::set_muted_until(&mut *txn, to_unmute.id(), Some(ChatMuted::Forever)).await?;
            let unknown_chat = ChatMuteEntry::Muted(MutedChat {
                group_id: Some(GroupId::from_slice(&[9; 16])),
                until: None,
            });

            let entries = [
                unmuted(&to_unmute),
                muted(&to_mute, None),
                unknown_chat,
                ChatMuteEntry::Unknown,
                ChatMuteEntry::Muted(MutedChat::default()),
                muted(&muted_twice, None),
                muted(&muted_twice, Some(until())),
            ];
            apply_chat_mutes_update(txn, &entries).await?;

            assert_eq!(muted_until(txn, &to_unmute).await?, None);
            assert_eq!(muted_until(txn, &to_mute).await?, Some(ChatMuted::Forever));
            assert_eq!(
                muted_until(txn, &muted_twice).await?,
                Some(ChatMuted::Until(until()))
            );
            assert!(
                staged_mutes(&mut *txn).await?.is_empty(),
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
            let chat = store_chat(txn).await?;
            set_and_park_mute(txn, chat.id(), Some(ChatMuted::Forever)).await?;

            apply_chat_mutes_update(txn, &[unmuted(&chat)]).await?;

            assert_eq!(muted_until(txn, &chat).await?, Some(ChatMuted::Forever));
            assert_eq!(staged_mutes(&mut *txn).await?, vec![muted(&chat, None)]);
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn a_mute_is_stored_as_siblings_receive_it(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let chat = store_chat(txn).await?;
            let precise = until() + TimeDelta::nanoseconds(456_789);

            set_and_park_mute(txn, chat.id(), Some(ChatMuted::Until(precise))).await?;

            assert_eq!(
                muted_until(txn, &chat).await?,
                Some(ChatMuted::Until(until()))
            );
            assert_eq!(
                staged_mutes(&mut *txn).await?,
                vec![muted(&chat, Some(until()))]
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn sibling_entries_reach_a_chat_still_onboarding(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let unmuted_later = onboarding_group(Some(ChatMuted::Forever))?;
            let muted_later = onboarding_group(None)?;
            let unmuted_id = unmuted_later.group_id.clone();
            let muted_id = muted_later.group_id.clone();
            CoreUser::enqueue_vc_onboarding(txn, vec![unmuted_later, muted_later]).await?;
            assert_eq!(
                Resync::recorded_mute(&mut *txn, &unmuted_id).await?,
                Some(ChatMuted::Forever)
            );

            let entries = [
                ChatMuteEntry::Unmuted(UnmutedChat {
                    group_id: Some(unmuted_id.clone()),
                }),
                ChatMuteEntry::Muted(MutedChat {
                    group_id: Some(muted_id.clone()),
                    until: Some(until()),
                }),
            ];
            apply_chat_mutes_update(txn, &entries).await?;

            assert_eq!(Resync::recorded_mute(&mut *txn, &unmuted_id).await?, None);
            assert_eq!(
                Resync::recorded_mute(&mut *txn, &muted_id).await?,
                Some(ChatMuted::Until(until()))
            );
            Ok(())
        })
        .await
    }
}
