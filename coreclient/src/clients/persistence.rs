// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    codec::PersistenceCodec,
    identifiers::{Fqdn, UserId},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{
    Database, Decode, Encode, Sqlite, SqliteExecutor, Type, encode::IsNull, error::BoxDynError,
    query, query_as, query_scalar, sqlite::SqliteTypeInfo,
};
use uuid::Uuid;

use crate::utils::persistence::open_air_db;

use super::store::{ClientRecord, ClientRecordState, UserCreationState};

// When adding a variant to this enum, the new variant must be called
// `CurrentVersion` and the current version must be renamed to `VX`, where `X`
// is the next version number. The content type of the old `CurrentVersion` must
// be renamed and otherwise preserved to ensure backwards compatibility.
#[derive(Serialize, Deserialize)]
enum StorableUserCreationState {
    CurrentVersion(UserCreationState),
}

// Only change this enum in tandem with its non-Ref variant.
#[derive(Serialize)]
enum StorableUserCreationStateRef<'a> {
    CurrentVersion(&'a UserCreationState),
}

impl Type<Sqlite> for UserCreationState {
    fn type_info() -> SqliteTypeInfo {
        <Vec<u8> as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for UserCreationState {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        let state = StorableUserCreationStateRef::CurrentVersion(self);
        let bytes = PersistenceCodec::to_vec(&state)?;
        Encode::<Sqlite>::encode(bytes, buf)
    }
}

impl<'r> Decode<'r, Sqlite> for UserCreationState {
    fn decode(value: <Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes: &[u8] = Decode::<Sqlite>::decode(value)?;
        let state = PersistenceCodec::from_slice(bytes)?;
        match state {
            StorableUserCreationState::CurrentVersion(state) => Ok(state),
        }
    }
}

impl UserCreationState {
    pub(super) async fn load(
        executor: impl SqliteExecutor<'_>,
        user_id: &UserId,
    ) -> sqlx::Result<Option<Self>> {
        let uuid = user_id.uuid();
        let domain = user_id.domain();
        query_scalar!(
            r#"SELECT state AS "state: _"
            FROM user_creation_state WHERE user_uuid = ? AND user_domain = ?"#,
            uuid,
            domain
        )
        .fetch_optional(executor)
        .await
    }

    pub(super) async fn store(&self, executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
        let user_id = self.user_id();
        let uuid = user_id.uuid();
        let domain = user_id.domain();
        query!(
            "INSERT OR REPLACE INTO user_creation_state
                (user_uuid, user_domain, state)
            VALUES (?, ?, ?)",
            uuid,
            domain,
            self
        )
        .execute(executor)
        .await?;
        Ok(())
    }
}

impl Type<Sqlite> for ClientRecordState {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <&str as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for ClientRecordState {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        Encode::<Sqlite>::encode(self.as_str(), buf)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid ClientRecordState: {state}")]
struct InvalidClientRecordState {
    state: String,
}

impl<'r> Decode<'r, Sqlite> for ClientRecordState {
    fn decode(value: <Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let state: &str = Decode::<Sqlite>::decode(value)?;
        Self::from_str(state).ok_or_else(|| -> BoxDynError {
            Box::new(InvalidClientRecordState {
                state: state.to_string(),
            })
        })
    }
}

struct SqlClientRecord {
    user_uuid: Uuid,
    user_domain: Fqdn,
    client_record_state: ClientRecordState,
    created_at: DateTime<Utc>,
    is_default: bool,
}

impl From<SqlClientRecord> for ClientRecord {
    fn from(value: SqlClientRecord) -> Self {
        Self {
            user_id: UserId::new(value.user_uuid, value.user_domain),
            client_record_state: value.client_record_state,
            created_at: value.created_at,
            is_default: value.is_default,
        }
    }
}

impl ClientRecord {
    pub async fn load_all_from_air_db(air_db_path: &str) -> sqlx::Result<Vec<Self>> {
        let pool = open_air_db(air_db_path).await?;
        Self::load_all(&pool).await
    }

    pub async fn load_all(executor: impl SqliteExecutor<'_>) -> sqlx::Result<Vec<Self>> {
        let records = query_as!(
            SqlClientRecord,
            r#"
            SELECT
                user_uuid AS "user_uuid: _",
                user_domain AS "user_domain: _",
                record_state AS "client_record_state: _",
                created_at AS "created_at: _",
                is_default
            FROM client_record"#
        )
        .fetch_all(executor)
        .await?;
        Ok(records.into_iter().map(From::from).collect())
    }

    pub(crate) async fn load(
        executor: impl SqliteExecutor<'_>,
        user_id: &UserId,
    ) -> sqlx::Result<Option<Self>> {
        let uuid = user_id.uuid();
        let domain = user_id.domain();
        query_as!(
            SqlClientRecord,
            r#"SELECT
                user_uuid AS "user_uuid: _",
                user_domain AS "user_domain: _",
                record_state AS "client_record_state: _",
                created_at AS "created_at: _",
                is_default
            FROM client_record WHERE user_uuid = ? AND user_domain = ?"#,
            uuid,
            domain
        )
        .fetch_optional(executor)
        .await
        .map(|res| res.map(From::from))
    }

    pub(crate) async fn store(&self, executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
        let record_state_str = match self.client_record_state {
            ClientRecordState::InProgress => "in_progress",
            ClientRecordState::Finished => "finished",
        };
        let uuid = self.user_id.uuid();
        let domain = self.user_id.domain();
        query!(
            "INSERT OR REPLACE INTO client_record
            (user_uuid, user_domain, record_state, created_at, is_default)
            VALUES (?1, ?2, ?3, ?4, ?5)",
            uuid,
            domain,
            record_state_str,
            self.created_at,
            self.is_default,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn set_default(
        executor: impl SqliteExecutor<'_>,
        user_id: &UserId,
    ) -> sqlx::Result<()> {
        let uuid = user_id.uuid();
        let domain = user_id.domain();
        query!(
            "UPDATE client_record SET is_default = (user_uuid == ? AND user_domain == ?)",
            uuid,
            domain,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub(crate) async fn delete(
        executor: impl SqliteExecutor<'_>,
        user_id: &UserId,
    ) -> sqlx::Result<()> {
        let uuid = user_id.uuid();
        let domain = user_id.domain();
        query!(
            "DELETE FROM client_record WHERE user_uuid = ? AND user_domain = ?",
            uuid,
            domain
        )
        .execute(executor)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use aircommon::messages::push_token::{PushToken, PushTokenOperator};
    use chrono::{DateTime, Utc};
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::clients::create_user::BasicUserData;

    use super::*;

    fn new_client_record(id: Uuid, created_at: DateTime<Utc>) -> ClientRecord {
        let user_id = UserId::new(id, "localhost".parse().unwrap());
        ClientRecord {
            user_id,
            client_record_state: ClientRecordState::Finished,
            created_at,
            is_default: false,
        }
    }

    fn test_client_record() -> ClientRecord {
        new_client_record(Uuid::new_v4(), Utc::now())
    }

    #[sqlx::test]
    async fn persistence(pool: SqlitePool) -> anyhow::Result<()> {
        let mut alice_record = test_client_record();
        let mut bob_record = test_client_record();

        // Storing and loading client records works
        alice_record.store(&pool).await?;
        bob_record.store(&pool).await?;
        let records = ClientRecord::load_all(&pool).await?;
        assert_eq!(records, [alice_record.clone(), bob_record.clone()]);

        // Set default to alice set alice is_default
        alice_record.is_default = true;
        ClientRecord::set_default(&pool, &alice_record.user_id).await?;
        let records = ClientRecord::load_all(&pool).await?;
        assert_eq!(records, [alice_record.clone(), bob_record.clone()]);

        // Set default to bob clears alice is_default
        alice_record.is_default = false;
        bob_record.is_default = true;
        ClientRecord::set_default(&pool, &bob_record.user_id).await?;
        let records = ClientRecord::load_all(&pool).await?;
        assert_eq!(records, [alice_record.clone(), bob_record.clone()]);

        // Delete client records
        ClientRecord::delete(&pool, &alice_record.user_id).await?;
        ClientRecord::delete(&pool, &bob_record.user_id).await?;
        let records = ClientRecord::load_all(&pool).await?;
        assert_eq!(records, []);

        Ok(())
    }

    static USER_CREATION_STATE_BASIC: LazyLock<UserCreationState> = LazyLock::new(|| {
        let user_id = Uuid::from_u128(1);

        UserCreationState::BasicUserData(BasicUserData {
            user_id: UserId::new(user_id, "localhost".parse().unwrap()),
            server_url: "localhost".to_owned(),
            push_token: Some(PushToken::new(
                PushTokenOperator::Google,
                "token".to_owned(),
            )),
            invitation_code: "DUMMY007".to_owned(),
        })
    });

    #[test]
    fn user_creation_state_basic_serde_codec() {
        insta::assert_binary_snapshot!(
            ".cbor",
            PersistenceCodec::to_vec(&*USER_CREATION_STATE_BASIC).unwrap()
        );
    }

    #[test]
    fn user_creation_state_basic_json_codec() {
        insta::assert_json_snapshot!(&*USER_CREATION_STATE_BASIC);
    }

    #[test]
    fn client_record_serde_codec() {
        let record = new_client_record(Uuid::from_u128(1), "2025-01-01T00:00:00Z".parse().unwrap());
        insta::assert_binary_snapshot!(".cbor", PersistenceCodec::to_vec(&record).unwrap());
    }

    #[test]
    fn client_record_serde_json() {
        let record = new_client_record(Uuid::from_u128(1), "2025-01-01T00:00:00Z".parse().unwrap());
        insta::assert_json_snapshot!(&record);
    }
}
