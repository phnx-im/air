// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::{DateTime, Utc};

use crate::{ChatId, clients::CoreUser};

use super::FetchedLinkPreview;

impl CoreUser {
    pub async fn draft_link_preview(
        &self,
        chat_id: ChatId,
    ) -> anyhow::Result<Option<DraftLinkPreview>> {
        Ok(DraftLinkPreview::load(self.db().read().await?, chat_id).await?)
    }

    pub async fn store_draft_link_preview(
        &self,
        chat_id: ChatId,
        preview: Option<&DraftLinkPreview>,
    ) -> anyhow::Result<()> {
        self.db()
            .with_write_transaction(async |txn| {
                if let Some(preview) = preview {
                    preview.store(txn, chat_id).await
                } else {
                    DraftLinkPreview::delete(txn, chat_id).await
                }
            })
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum DraftLinkPreview {
    Fetched {
        preview: FetchedLinkPreview,
        fetched_at: DateTime<Utc>,
    },
    Dismissed {
        url: String,
    },
}

mod persistence {
    use airprotos::client::link_preview::LinkPreviewCard;
    use chrono::{DateTime, Utc};
    use sqlx::{query, query_as};

    use crate::{
        ChatId,
        db::access::{ReadConnection, WriteConnection},
        link_preview::FetchedLinkPreview,
    };

    use super::DraftLinkPreview;

    impl DraftLinkPreview {
        pub(super) async fn load(
            mut connection: impl ReadConnection,
            chat_id: ChatId,
        ) -> sqlx::Result<Option<Self>> {
            struct Record {
                url: String,
                card: Option<Vec<u8>>,
                image: Option<Vec<u8>>,
                fetched_at: Option<DateTime<Utc>>,
            }
            let Some(record) = query_as!(
                Record,
                r#"SELECT
                    url,
                    card,
                    image,
                    fetched_at AS "fetched_at: _"
                FROM message_draft_link_preview
                WHERE chat_id = ?"#,
                chat_id,
            )
            .fetch_optional(connection.as_mut())
            .await?
            else {
                return Ok(None);
            };
            let draft = match (record.card, record.fetched_at) {
                (Some(card), Some(fetched_at)) => {
                    let Ok(card) = LinkPreviewCard::decode(&card) else {
                        return Ok(None);
                    };
                    let Some(preview) = FetchedLinkPreview::new(card, record.image) else {
                        return Ok(None);
                    };
                    Some(Self::Fetched {
                        preview,
                        fetched_at,
                    })
                }
                (None, None) => Some(Self::Dismissed { url: record.url }),
                _ => None,
            };
            Ok(draft)
        }

        pub(super) async fn store(
            &self,
            mut connection: impl WriteConnection,
            chat_id: ChatId,
        ) -> sqlx::Result<()> {
            let (url, card, image, fetched_at) = match self {
                DraftLinkPreview::Fetched {
                    preview,
                    fetched_at,
                } => {
                    let url = preview.card().url();
                    let card = preview
                        .card()
                        .encode()
                        .map_err(|error| sqlx::Error::Encode(error.into()))?;
                    let image = preview.image_bytes();
                    (url, Some(card), image, Some(fetched_at))
                }
                DraftLinkPreview::Dismissed { url } => (url.as_str(), None, None, None),
            };
            query!(
                "INSERT INTO message_draft_link_preview (
                    chat_id, url, card, image, fetched_at
                ) VALUES (?, ?, ?, ?, ?)
                ON CONFLICT(chat_id) DO UPDATE SET
                    url = excluded.url,
                    card = excluded.card,
                    image = excluded.image,
                    fetched_at = excluded.fetched_at",
                chat_id,
                url,
                card,
                image,
                fetched_at
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        pub(crate) async fn delete(
            mut connection: impl WriteConnection,
            chat_id: ChatId,
        ) -> sqlx::Result<()> {
            query!(
                "DELETE FROM message_draft_link_preview WHERE chat_id = ?",
                chat_id
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }
    }

    #[cfg(test)]
    mod test {
        use airprotos::client::link_preview::{LinkPreviewImage, LinkPreviewKind};
        use chrono::SubsecRound;
        use sqlx::SqlitePool;

        use crate::{MessageDraft, chats::persistence::tests::test_chat, db::access::DbAccess};

        use super::*;

        fn card(url: &str, with_image: bool) -> LinkPreviewCard {
            let image = with_image
                .then(|| LinkPreviewImage::new(1, 100, 50, Some("LEHV6n".into()), Some("alt")))
                .flatten();
            LinkPreviewCard::new(
                url.to_string(),
                "Title",
                "Description",
                LinkPreviewKind::Default,
                image,
            )
            .unwrap()
        }

        fn fetched(url: &str, image: Option<Vec<u8>>) -> DraftLinkPreview {
            let preview = FetchedLinkPreview::new(card(url, image.is_some()), image).unwrap();
            DraftLinkPreview::Fetched {
                preview,
                fetched_at: Utc::now().round_subsecs(6),
            }
        }

        fn assert_same(loaded: Option<DraftLinkPreview>, expected: &DraftLinkPreview) {
            match (loaded.expect("preview is stored"), expected) {
                (
                    DraftLinkPreview::Fetched {
                        preview,
                        fetched_at,
                    },
                    DraftLinkPreview::Fetched {
                        preview: expected_preview,
                        fetched_at: expected_fetched_at,
                    },
                ) => {
                    assert_eq!(preview.card().url(), expected_preview.card().url());
                    assert_eq!(
                        preview.card().encode().unwrap(),
                        expected_preview.card().encode().unwrap()
                    );
                    assert_eq!(preview.image_bytes(), expected_preview.image_bytes());
                    assert_eq!(&fetched_at, expected_fetched_at);
                }
                (
                    DraftLinkPreview::Dismissed { url },
                    DraftLinkPreview::Dismissed { url: expected_url },
                ) => assert_eq!(&url, expected_url),
                (loaded, expected) => panic!("expected {expected:?}, got {loaded:?}"),
            }
        }

        #[sqlx::test]
        async fn load_missing_is_none(pool: SqlitePool) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);
            let chat = test_chat();
            chat.store(pool.write().await?).await?;

            let loaded = DraftLinkPreview::load(pool.read().await?, chat.id()).await?;
            assert!(loaded.is_none());
            Ok(())
        }

        #[sqlx::test]
        async fn store_load_fetched_without_image(pool: SqlitePool) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);
            let chat = test_chat();
            chat.store(pool.write().await?).await?;

            let preview = fetched("https://example.com/a", None);
            preview.store(pool.write().await?, chat.id()).await?;

            let loaded = DraftLinkPreview::load(pool.read().await?, chat.id()).await?;
            assert_same(loaded, &preview);
            Ok(())
        }

        #[sqlx::test]
        async fn store_load_fetched_with_image(pool: SqlitePool) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);
            let chat = test_chat();
            chat.store(pool.write().await?).await?;

            let preview = fetched("https://example.com/a", Some(vec![1, 2, 3, 4]));
            preview.store(pool.write().await?, chat.id()).await?;

            let loaded = DraftLinkPreview::load(pool.read().await?, chat.id()).await?;
            assert_same(loaded, &preview);
            Ok(())
        }

        #[sqlx::test]
        async fn store_load_dismissed(pool: SqlitePool) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);
            let chat = test_chat();
            chat.store(pool.write().await?).await?;

            let preview = DraftLinkPreview::Dismissed {
                url: "https://example.com/dismissed".to_string(),
            };
            preview.store(pool.write().await?, chat.id()).await?;

            let loaded = DraftLinkPreview::load(pool.read().await?, chat.id()).await?;
            assert_same(loaded, &preview);
            Ok(())
        }

        #[sqlx::test]
        async fn store_replaces_existing_row(pool: SqlitePool) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);
            let chat = test_chat();
            chat.store(pool.write().await?).await?;

            let first = fetched("https://example.com/a", Some(vec![9, 9]));
            first.store(pool.write().await?, chat.id()).await?;

            let dismissed = DraftLinkPreview::Dismissed {
                url: "https://example.com/b".to_string(),
            };
            dismissed.store(pool.write().await?, chat.id()).await?;
            let loaded = DraftLinkPreview::load(pool.read().await?, chat.id()).await?;
            assert_same(loaded, &dismissed);

            let second = fetched("https://example.com/c", None);
            second.store(pool.write().await?, chat.id()).await?;
            let loaded = DraftLinkPreview::load(pool.read().await?, chat.id()).await?;
            assert_same(loaded, &second);
            Ok(())
        }

        #[sqlx::test]
        async fn delete_removes_row(pool: SqlitePool) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);
            let chat = test_chat();
            chat.store(pool.write().await?).await?;

            fetched("https://example.com/a", None)
                .store(pool.write().await?, chat.id())
                .await?;
            DraftLinkPreview::delete(pool.write().await?, chat.id()).await?;

            let loaded = DraftLinkPreview::load(pool.read().await?, chat.id()).await?;
            assert!(loaded.is_none());
            Ok(())
        }

        #[sqlx::test]
        async fn inconsistent_rows_load_as_none(pool: SqlitePool) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);
            let chat = test_chat();
            chat.store(pool.write().await?).await?;

            let valid_card = card("https://example.com/a", false).encode()?;
            let now = Utc::now().round_subsecs(6);
            let rows = [
                // undecodable card
                (Some(vec![0xff, 0x00, 0x01]), Some(now)),
                // card without fetched_at
                (Some(valid_card.clone()), None),
                // fetched_at without card
                (None, Some(now)),
            ];
            for (card, fetched_at) in rows {
                sqlx::query(
                    "INSERT INTO message_draft_link_preview (
                        chat_id, url, card, image, fetched_at
                    ) VALUES (?, ?, ?, NULL, ?)
                    ON CONFLICT(chat_id) DO UPDATE SET
                        url = excluded.url,
                        card = excluded.card,
                        image = excluded.image,
                        fetched_at = excluded.fetched_at",
                )
                .bind(chat.id())
                .bind("https://example.com/a")
                .bind(card)
                .bind(fetched_at)
                .execute(pool.write().await?.as_mut())
                .await?;

                let loaded = DraftLinkPreview::load(pool.read().await?, chat.id()).await?;
                assert!(loaded.is_none());
            }
            Ok(())
        }

        fn message_draft(updated_at: DateTime<Utc>) -> MessageDraft {
            MessageDraft {
                message: "text".to_string(),
                updated_at,
                ..MessageDraft::empty()
            }
        }

        #[sqlx::test]
        async fn deleting_message_draft_deletes_preview(pool: SqlitePool) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);
            let chat = test_chat();
            chat.store(pool.write().await?).await?;

            message_draft(Utc::now())
                .store(pool.write().await?, chat.id())
                .await?;
            fetched("https://example.com/a", None)
                .store(pool.write().await?, chat.id())
                .await?;

            MessageDraft::delete(pool.write().await?, chat.id()).await?;

            let loaded = DraftLinkPreview::load(pool.read().await?, chat.id()).await?;
            assert!(loaded.is_none());
            Ok(())
        }

        #[sqlx::test]
        async fn delete_draft_version_deletes_preview_only_on_match(
            pool: SqlitePool,
        ) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);
            let chat = test_chat();
            chat.store(pool.write().await?).await?;

            let updated_at = Utc::now().round_subsecs(6);
            message_draft(updated_at)
                .store(pool.write().await?, chat.id())
                .await?;
            let preview = fetched("https://example.com/a", None);
            preview.store(pool.write().await?, chat.id()).await?;

            let outdated = updated_at - chrono::Duration::seconds(1);
            MessageDraft::delete_version(pool.write().await?, chat.id(), outdated).await?;
            let loaded = DraftLinkPreview::load(pool.read().await?, chat.id()).await?;
            assert_same(loaded, &preview);

            MessageDraft::delete_version(pool.write().await?, chat.id(), updated_at).await?;
            let loaded = DraftLinkPreview::load(pool.read().await?, chat.id()).await?;
            assert!(loaded.is_none());
            Ok(())
        }
    }
}
