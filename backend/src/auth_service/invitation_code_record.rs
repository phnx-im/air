// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use rand::{CryptoRng, Rng};
use sqlx::PgPool;

use crate::auth_service::cli::InvitationCodeStats;

pub struct InvitationCodeRecord {
    pub(crate) code: String,
    pub(crate) redeemed: bool,
}

const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTUVWXYZ";
const CODE_LEN: usize = 8;

impl InvitationCodeRecord {
    pub(crate) async fn stats(pool: &PgPool) -> sqlx::Result<InvitationCodeStats> {
        let count = sqlx::query_scalar!("SELECT COUNT(*) FROM invitation_code")
            .fetch_one(pool)
            .await?;
        let redeemed =
            sqlx::query_scalar!("SELECT COUNT(*) FROM invitation_code WHERE redeemed = TRUE")
                .fetch_one(pool)
                .await?;
        Ok(InvitationCodeStats {
            count: count.and_then(|c| c.try_into().ok()).unwrap_or(0),
            redeemed: redeemed.and_then(|r| r.try_into().ok()).unwrap_or(0),
        })
    }

    fn generate_code(rng: &mut (impl CryptoRng + Rng), code: &mut String) {
        for _ in 0..CODE_LEN {
            code.push(ALPHABET[rng.gen_range(0..ALPHABET.len())] as char);
        }
    }

    pub(crate) fn check_code(code: &str) -> bool {
        code.len() == CODE_LEN && code.bytes().all(|c| ALPHABET.contains(&c))
    }
}

mod persistence {
    use super::*;

    use sqlx::{PgPool, query, query_as};

    impl InvitationCodeRecord {
        pub(crate) async fn load_all(
            pool: &PgPool,
            include_redeemed: bool,
            limit: usize,
        ) -> sqlx::Result<Vec<InvitationCodeRecord>> {
            if include_redeemed {
                query_as!(
                    InvitationCodeRecord,
                    "
                        SELECT code, redeemed
                        FROM invitation_code
                        ORDER BY code
                        LIMIT $1
                    ",
                    limit as i64,
                )
                .fetch_all(pool)
                .await
            } else {
                query_as!(
                    InvitationCodeRecord,
                    "
                        SELECT code, redeemed
                        FROM invitation_code
                        WHERE redeemed = FALSE
                        ORDER BY code
                        LIMIT $1
                    ",
                    limit as i64,
                )
                .fetch_all(pool)
                .await
            }
        }

        pub(crate) async fn load(
            pool: &PgPool,
            code: &str,
        ) -> sqlx::Result<Option<InvitationCodeRecord>> {
            query_as!(
                InvitationCodeRecord,
                "
                    SELECT code, redeemed
                    FROM invitation_code
                    WHERE code = $1
                ",
                code
            )
            .fetch_optional(pool)
            .await
        }

        pub(crate) async fn insert(
            pool: &PgPool,
            code: &str,
            redeemed: bool,
        ) -> sqlx::Result<bool> {
            let result = query!(
                "
                    INSERT INTO invitation_code (code, redeemed)
                    VALUES ($1, $2)
                ",
                code,
                redeemed
            )
            .execute(pool)
            .await?;
            Ok(result.rows_affected() > 0)
        }

        pub(crate) async fn save(&self, pool: &PgPool) -> sqlx::Result<()> {
            query!(
                "
                    INSERT INTO invitation_code (code, redeemed)
                    VALUES ($1, $2)
                    ON CONFLICT (code) DO UPDATE SET redeemed = $2
                ",
                self.code,
                self.redeemed
            )
            .execute(pool)
            .await?;
            Ok(())
        }

        pub(crate) async fn generate(
            pool: &PgPool,
            rng: &mut (impl CryptoRng + Rng),
            n: usize,
        ) -> sqlx::Result<()> {
            let mut code = String::with_capacity(6);
            for _ in 0..n {
                loop {
                    code.clear();
                    Self::generate_code(rng, &mut code);
                    if Self::insert(pool, &code, false).await? {
                        break;
                    }
                }
            }
            Ok(())
        }
    }

    #[cfg(test)]
    mod test {
        use sqlx::PgPool;

        use super::*;

        #[sqlx::test]
        async fn load_all_includes_redeemed(pool: PgPool) -> anyhow::Result<()> {
            InvitationCodeRecord::insert(&pool, "CODE_A", true).await?;
            InvitationCodeRecord::insert(&pool, "CODE_B", false).await?;

            let records = InvitationCodeRecord::load_all(&pool, true, 10).await?;

            assert_eq!(records.len(), 2);

            let code_a = records.iter().find(|r| r.code == "CODE_A");
            assert!(code_a.is_some());
            assert!(code_a.unwrap().redeemed);

            let code_b = records.iter().find(|r| r.code == "CODE_B");
            assert!(code_b.is_some());
            assert!(!code_b.unwrap().redeemed);

            Ok(())
        }

        #[sqlx::test]
        async fn load_all_excludes_redeemed(pool: PgPool) -> anyhow::Result<()> {
            InvitationCodeRecord::insert(&pool, "CODE_C", true).await?;
            InvitationCodeRecord::insert(&pool, "CODE_D", false).await?;

            let records = InvitationCodeRecord::load_all(&pool, false, 10).await?;

            assert_eq!(records.len(), 1);
            assert_eq!(records[0].code, "CODE_D");
            assert!(!records[0].redeemed);

            Ok(())
        }

        #[sqlx::test]
        async fn load_existing_code(pool: PgPool) -> anyhow::Result<()> {
            InvitationCodeRecord::insert(&pool, "LOAD_ME", true).await?;

            let result = InvitationCodeRecord::load(&pool, "LOAD_ME").await?;

            assert!(result.is_some());
            let record = result.unwrap();
            assert_eq!(record.code, "LOAD_ME");
            assert!(record.redeemed);

            Ok(())
        }

        #[sqlx::test]
        async fn load_non_existing_code(pool: PgPool) -> anyhow::Result<()> {
            let result = InvitationCodeRecord::load(&pool, "DOES_NOT_EXIST").await?;
            assert!(result.is_none());
            Ok(())
        }

        #[sqlx::test]
        async fn save_updates_existing_record(pool: PgPool) -> anyhow::Result<()> {
            InvitationCodeRecord::insert(&pool, "UPDATE_ME", false).await?;

            let updated_record = InvitationCodeRecord {
                code: "UPDATE_ME".to_string(),
                redeemed: true, // Changing the state
            };

            updated_record.save(&pool).await?;

            let loaded = InvitationCodeRecord::load(&pool, "UPDATE_ME").await?;
            assert!(loaded.is_some());
            assert!(loaded.unwrap().redeemed); // Should be updated

            // Check that no duplicate was created
            let all = InvitationCodeRecord::load_all(&pool, true, 10).await?;
            assert_eq!(all.len(), 1);

            Ok(())
        }

        #[sqlx::test]
        async fn generate_multiple_codes(pool: PgPool) -> anyhow::Result<()> {
            let mut rng = rand::thread_rng();

            let n = 5;
            InvitationCodeRecord::generate(&pool, &mut rng, n).await?;

            let all_codes = InvitationCodeRecord::load_all(&pool, true, 10).await?;
            assert_eq!(all_codes.len(), n);

            for record in all_codes {
                assert_eq!(record.code.len(), CODE_LEN);
                assert!(!record.redeemed);
            }

            Ok(())
        }
    }
}
