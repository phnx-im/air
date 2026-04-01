// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use rand::{CryptoRng, Rng, thread_rng};
use sqlx::PgPool;
use tracing::warn;

use crate::auth_service::cli::InvitationCodeStats;

#[derive(Debug, PartialEq, Eq)]
pub struct InvitationCodeRecord {
    pub(crate) code: String,
    pub(crate) redeemed: bool,
}

const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTUVWXYZ";
const CODE_LEN: usize = 8;
const CODES_PER_USER: usize = 25;

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

    // Make sure a given user has enough invitation codes available to be redeemed
    pub(crate) async fn replenish(pool: &PgPool, user_id: &UserId) -> sqlx::Result<()> {
        let mut txn = pool.begin().await?;
        let redeemable_count = Self::redeemable_count(txn.as_mut(), user_id).await?;

        for _ in redeemable_count..CODES_PER_USER {
            loop {
                let code = Self::generate_code(&mut thread_rng());
                if Self::insert_for_user(txn.as_mut(), user_id, &code).await? {
                    break;
                }

                warn!("invite code collision, generating another one.");
            }
        }

        txn.commit().await?;
        Ok(())
    }

    fn generate_code(rng: &mut (impl CryptoRng + Rng)) -> String {
        let mut code = String::with_capacity(CODE_LEN);
        for _ in 0..CODE_LEN {
            code.push(ALPHABET[rng.gen_range(0..ALPHABET.len())] as char);
        }
        code
    }

    pub(crate) fn validate_code(code: &str) -> bool {
        code.len() == CODE_LEN && code.bytes().all(|c| ALPHABET.contains(&c))
    }
}

mod persistence {
    use super::*;

    use sqlx::{PgExecutor, query, query_as, query_scalar};

    impl InvitationCodeRecord {
        pub(crate) async fn redeemable_count(
            executor: impl PgExecutor<'_>,
            user_id: &UserId,
        ) -> sqlx::Result<usize> {
            query_scalar!(
                "
                        SELECT COUNT(code)
                        FROM invitation_code
                        WHERE user_uuid = $1 AND user_domain = $2 AND redeemed = FALSE
                    ",
                user_id.uuid(),
                user_id.domain() as _,
            )
            .fetch_one(executor)
            .await
            .map(|count| count.unwrap_or_default() as usize)
        }

        pub(crate) async fn load_all(
            executor: impl PgExecutor<'_>,
            user_id: &UserId,
            include_redeemed: bool, // TODO: remove?
            limit: usize,
        ) -> sqlx::Result<Vec<InvitationCodeRecord>> {
            if include_redeemed {
                query_as!(
                    InvitationCodeRecord,
                    "
                        SELECT code, redeemed
                        FROM invitation_code
                        WHERE user_uuid = $1 AND user_domain = $2
                        ORDER BY created_at
                        LIMIT $3
                    ",
                    user_id.uuid(),
                    user_id.domain() as _,
                    limit as i64,
                )
                .fetch_all(executor)
                .await
            } else {
                query_as!(
                    InvitationCodeRecord,
                    "
                        SELECT code, redeemed
                        FROM invitation_code
                        WHERE user_uuid = $1 AND user_domain = $2 AND redeemed = FALSE
                        ORDER BY created_at
                        LIMIT $3
                    ",
                    user_id.uuid(),
                    user_id.domain() as _,
                    limit as i64,
                )
                .fetch_all(executor)
                .await
            }
        }

        pub(crate) async fn load(
            executor: impl PgExecutor<'_>,
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
            .fetch_optional(executor)
            .await
        }

        /// Inserts an invitation code in the database, returns true if it was inserted or false if it already exists.
        pub(crate) async fn insert(
            executor: impl PgExecutor<'_>,
            code: &str,
        ) -> sqlx::Result<bool> {
            let result = query!(
                "
                    INSERT INTO invitation_code (code)
                    VALUES ($1)
                ",
                code,
            )
            .execute(executor)
            .await?;
            Ok(result.rows_affected() > 0)
        }

        /// Inserts an invitation code in the database, returns true if it was inserted or false if it already exists.
        pub(crate) async fn insert_for_user(
            executor: impl PgExecutor<'_>,
            user_id: &UserId,
            code: &str,
        ) -> sqlx::Result<bool> {
            let result = query!(
                "
                    INSERT INTO invitation_code (user_uuid, user_domain, code)
                    VALUES ($1, $2, $3)
                ",
                user_id.uuid(),
                user_id.domain() as _,
                code,
            )
            .execute(executor)
            .await?;
            Ok(result.rows_affected() > 0)
        }

        pub(crate) async fn redeem(mut self, executor: impl PgExecutor<'_>) -> sqlx::Result<Self> {
            let result = query!(
                "
                    UPDATE invitation_code
                    SET redeemed = TRUE
                    WHERE code = $1
                ",
                self.code
            )
            .execute(executor)
            .await?;

            self.redeemed = result.rows_affected() > 0;
            Ok(self)
        }
    }

    #[cfg(test)]
    mod test {
        use sqlx::PgPool;

        use crate::auth_service::user_record::persistence::tests::store_random_user_record;

        use super::*;

        #[sqlx::test]
        async fn load_all_includes_redeemed(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;
            let user_id = user_record.user_id();

            InvitationCodeRecord::insert_for_user(&pool, user_id, "CODE_A").await?;

            let code_a = InvitationCodeRecord::load(&pool, "CODE_A")
                .await?
                .unwrap()
                .redeem(&pool)
                .await?;

            InvitationCodeRecord::insert_for_user(&pool, user_id, "CODE_B").await?;
            let code_b = InvitationCodeRecord::load(&pool, "CODE_B").await?.unwrap();

            let records = InvitationCodeRecord::load_all(&pool, user_id, true, 10).await?;
            dbg!(&records);

            assert_eq!(records.len(), 2);
            assert!(records.contains(&code_a));
            assert!(records.contains(&code_b));

            Ok(())
        }

        #[sqlx::test]
        async fn load_all_excludes_redeemed(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;
            let user_id = user_record.user_id();

            InvitationCodeRecord::insert_for_user(&pool, user_id, "CODE_C").await?;
            InvitationCodeRecord::load(&pool, "CODE_C")
                .await?
                .unwrap()
                .redeem(&pool)
                .await?;
            InvitationCodeRecord::insert_for_user(&pool, user_id, "CODE_D").await?;

            let records = InvitationCodeRecord::load_all(&pool, user_id, false, 10).await?;

            assert_eq!(records.len(), 1);
            assert_eq!(records[0].code, "CODE_D");
            assert!(!records[0].redeemed);

            Ok(())
        }

        #[sqlx::test]
        async fn load_existing_code(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;
            let user_id = user_record.user_id();

            InvitationCodeRecord::insert_for_user(&pool, user_id, "LOAD_ME").await?;
            let load_me = InvitationCodeRecord::load(&pool, "LOAD_ME").await?.unwrap();
            let result = InvitationCodeRecord::load(&pool, "LOAD_ME").await?.unwrap();

            assert_eq!(load_me, result);

            Ok(())
        }

        #[sqlx::test]
        async fn load_non_existing_code(pool: PgPool) -> anyhow::Result<()> {
            let result = InvitationCodeRecord::load(&pool, "DOES_NOT_EXIST").await?;
            assert!(result.is_none());
            Ok(())
        }

        #[sqlx::test]
        async fn redeem_updates_existing_record(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;
            let user_id = user_record.user_id();

            InvitationCodeRecord::insert_for_user(&pool, user_id, "UPDATE_ME").await?;
            let update_me = InvitationCodeRecord::load(&pool, "UPDATE_ME")
                .await?
                .unwrap();
            assert!(!update_me.redeemed);

            let loaded = update_me.redeem(&pool).await?;
            assert!(loaded.redeemed); // Should be updated

            // Check that no duplicate was created
            let all = InvitationCodeRecord::load_all(&pool, user_id, true, 10).await?;
            assert_eq!(all.len(), 1);

            Ok(())
        }

        #[sqlx::test]
        async fn replenish_codes(pool: PgPool) -> anyhow::Result<()> {
            let mut rng = rand::thread_rng();
            let user_record = store_random_user_record(&pool).await?;
            let user_id = user_record.user_id();

            // Generate the initial set
            InvitationCodeRecord::replenish(&pool, &mut rng, user_id).await?;

            let mut all_codes =
                InvitationCodeRecord::load_all(&pool, user_id, true, CODES_PER_USER).await?;
            assert_eq!(all_codes.len(), CODES_PER_USER);
            assert_eq!(
                all_codes
                    .iter()
                    .filter(|c| !c.redeemed && c.code.len() == CODE_LEN)
                    .count(),
                CODES_PER_USER as usize
            );

            // Mark 6 codes redeemed
            const CODES_TO_REDEEM: usize = 6;
            for _ in 0..CODES_TO_REDEEM {
                all_codes.pop().unwrap().redeem(&pool).await?;
            }

            // Reload the set
            let redeemable_codes =
                InvitationCodeRecord::load_all(&pool, user_id, false, 100).await?;
            let redeemable_codes_count =
                InvitationCodeRecord::redeemable_count(&pool, user_id).await?;
            assert_eq!(redeemable_codes.len(), redeemable_codes_count);
            assert_eq!(redeemable_codes_count, CODES_PER_USER - CODES_TO_REDEEM);

            // Replenish codes
            InvitationCodeRecord::replenish(&pool, &mut rng, user_id).await?;
            let redeemable_codes =
                InvitationCodeRecord::load_all(&pool, user_id, false, 100).await?;
            let redeemable_codes_count =
                InvitationCodeRecord::redeemable_count(&pool, user_id).await?;
            assert_eq!(redeemable_codes.len(), redeemable_codes_count);
            assert_eq!(redeemable_codes_count, CODES_PER_USER);

            Ok(())
        }
    }
}
