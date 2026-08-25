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
    Database, Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError, query, query_as,
    query_scalar, sqlite::SqliteTypeInfo,
};
use uuid::Uuid;

use crate::{
    db::access::{ReadConnection, WriteConnection},
    utils::persistence::open_air_db,
};

use super::store::{ClientRecord, ClientRecordState, UserCreationState};

// When adding a variant to this enum, the new variant must be called
// `CurrentVersion` and the current version must be renamed to `VX`, where `X`
// is the next version number. The content type of the old `CurrentVersion` must
// be renamed and otherwise preserved to ensure backwards compatibility.
//
// The codec keys variants by name, so every variant also pins the tag it reads
// and writes. Renaming a variant without pinning its tag would orphan the rows
// written under the old name.
#[derive(Deserialize)]
enum StorableUserCreationState {
    #[serde(rename = "CurrentVersion")]
    V1(v1::UserCreationState),
    #[serde(rename = "V2")]
    CurrentVersion(UserCreationState),
}

// Only change this enum in tandem with its non-Ref variant.
#[derive(Serialize)]
enum StorableUserCreationStateRef<'a> {
    #[serde(rename = "V2")]
    CurrentVersion(&'a UserCreationState),
}

/// The shapes the registration states had while the invitation code was stored
/// alongside them.
///
/// A challenge response is short-lived, so it is passed through registration
/// rather than persisted with it. A state written by an older client still
/// decodes and its code is dropped: a registration that has not reached the AS
/// yet asks for a challenge again.
mod v1 {
    use aircommon::{
        credentials::{
            AsIntermediateCredential, UserCredentialPayload, VerifiableUserCredential,
            keys::PreliminaryUserSigningKey,
        },
        crypto::kdf::keys::RatchetSecret,
        identifiers::UserId,
        messages::{
            client_as_out::EncryptedUserProfile,
            push_token::{EncryptedPushToken, PushToken},
        },
    };
    use serde::Deserialize;

    use crate::{clients::create_user, key_stores::MemoryUserKeyStoreBase};

    #[derive(Deserialize)]
    pub(super) enum UserCreationState {
        BasicUserData(BasicUserData),
        InitialUserState(InitialUserState),
        PostRegistrationInitState(PostAsRegistrationState),
        UnfinalizedRegistrationState(create_user::UnfinalizedRegistrationState),
        AsRegisteredUserState(create_user::AsRegisteredUserState),
        QsRegisteredUserState(create_user::QsRegisteredUserState),
        FinalUserState(create_user::PersistedUserState),
    }

    #[derive(Deserialize)]
    pub(super) struct BasicUserData {
        user_id: UserId,
        push_token: Option<PushToken>,
        #[expect(dead_code, reason = "decoded, then dropped")]
        invitation_code: String,
    }

    #[derive(Deserialize)]
    pub(super) struct InitialUserState {
        #[serde(rename = "client_credential_payload")]
        user_credential_payload: UserCredentialPayload,
        as_intermediate_credential: AsIntermediateCredential,
        encrypted_push_token: Option<EncryptedPushToken>,
        encrypted_user_profile: EncryptedUserProfile,
        key_store: MemoryUserKeyStoreBase<PreliminaryUserSigningKey>,
        qs_initial_ratchet_secret: RatchetSecret,
        #[expect(dead_code, reason = "decoded, then dropped")]
        invitation_code: String,
    }

    #[derive(Deserialize)]
    pub(super) struct PostAsRegistrationState {
        initial_user_state: InitialUserState,
        #[serde(rename = "client_credential")]
        user_credential: VerifiableUserCredential,
    }

    impl From<UserCreationState> for super::UserCreationState {
        fn from(state: UserCreationState) -> Self {
            match state {
                UserCreationState::BasicUserData(state) => Self::BasicUserData(state.into()),
                UserCreationState::InitialUserState(state) => Self::InitialUserState(state.into()),
                UserCreationState::PostRegistrationInitState(state) => {
                    Self::PostRegistrationInitState(state.into())
                }
                UserCreationState::UnfinalizedRegistrationState(state) => {
                    Self::UnfinalizedRegistrationState(state)
                }
                UserCreationState::AsRegisteredUserState(state) => {
                    Self::AsRegisteredUserState(state)
                }
                UserCreationState::QsRegisteredUserState(state) => {
                    Self::QsRegisteredUserState(state)
                }
                UserCreationState::FinalUserState(state) => Self::FinalUserState(state),
            }
        }
    }

    impl From<BasicUserData> for create_user::BasicUserData {
        fn from(state: BasicUserData) -> Self {
            Self {
                user_id: state.user_id,
                push_token: state.push_token,
            }
        }
    }

    impl From<InitialUserState> for create_user::InitialUserState {
        fn from(state: InitialUserState) -> Self {
            Self {
                user_credential_payload: state.user_credential_payload,
                as_intermediate_credential: state.as_intermediate_credential,
                encrypted_push_token: state.encrypted_push_token,
                encrypted_user_profile: state.encrypted_user_profile,
                key_store: state.key_store,
                qs_initial_ratchet_secret: state.qs_initial_ratchet_secret,
            }
        }
    }

    impl From<PostAsRegistrationState> for create_user::PostAsRegistrationState {
        fn from(state: PostAsRegistrationState) -> Self {
            Self {
                initial_user_state: state.initial_user_state.into(),
                user_credential: state.user_credential,
            }
        }
    }
}

impl Type<Sqlite> for UserCreationState {
    fn type_info() -> SqliteTypeInfo {
        <Vec<u8> as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for UserCreationState {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer,
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
            StorableUserCreationState::V1(state) => Ok(state.into()),
            StorableUserCreationState::CurrentVersion(state) => Ok(state),
        }
    }
}

impl UserCreationState {
    pub(super) async fn load(
        mut connection: impl ReadConnection,
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
        .fetch_optional(connection.as_mut())
        .await
    }

    pub(super) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
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
        .execute(connection.as_mut())
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
        buf: &mut <Sqlite as Database>::ArgumentBuffer,
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
    client_record_id: Uuid,
    user_uuid: Uuid,
    user_domain: Fqdn,
    client_record_state: ClientRecordState,
    created_at: DateTime<Utc>,
    is_default: bool,
}

impl From<SqlClientRecord> for ClientRecord {
    fn from(value: SqlClientRecord) -> Self {
        Self {
            client_record_id: value.client_record_id,
            user_id: UserId::new(value.user_uuid, value.user_domain),
            client_record_state: value.client_record_state,
            created_at: value.created_at,
            is_default: value.is_default,
        }
    }
}

impl ClientRecord {
    pub async fn load_all_from_air_db(air_db_path: &str) -> sqlx::Result<Vec<Self>> {
        let db = open_air_db(air_db_path).await?;
        Self::load_all(db.read().await?).await
    }

    pub(crate) async fn load_all(mut connection: impl ReadConnection) -> sqlx::Result<Vec<Self>> {
        let records = query_as!(
            SqlClientRecord,
            r#"
            SELECT
                client_record_id AS "client_record_id: _",
                user_uuid AS "user_uuid: _",
                user_domain AS "user_domain: _",
                record_state AS "client_record_state: _",
                created_at AS "created_at: _",
                is_default
            FROM client_record"#
        )
        .fetch_all(connection.as_mut())
        .await?;
        Ok(records.into_iter().map(From::from).collect())
    }

    pub(crate) async fn load(
        mut connection: impl ReadConnection,
        client_record_id: Uuid,
    ) -> sqlx::Result<Option<Self>> {
        query_as!(
            SqlClientRecord,
            r#"SELECT
                client_record_id AS "client_record_id: _",
                user_uuid AS "user_uuid: _",
                user_domain AS "user_domain: _",
                record_state AS "client_record_state: _",
                created_at AS "created_at: _",
                is_default
            FROM client_record WHERE client_record_id = ?"#,
            client_record_id,
        )
        .fetch_optional(connection.as_mut())
        .await
        .map(|res| res.map(From::from))
    }

    pub(crate) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
        let record_state_str = match self.client_record_state {
            ClientRecordState::InProgress => "in_progress",
            ClientRecordState::Finished => "finished",
        };
        let user_uuid = self.user_id.uuid();
        let user_domain = self.user_id.domain();
        query!(
            "INSERT OR REPLACE INTO client_record
            (client_record_id, user_uuid, user_domain, record_state, created_at, is_default)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            self.client_record_id,
            user_uuid,
            user_domain,
            record_state_str,
            self.created_at,
            self.is_default,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    pub(crate) async fn set_default(
        mut connection: impl WriteConnection,
        client_record_id: Uuid,
    ) -> sqlx::Result<()> {
        query!(
            "UPDATE client_record SET is_default = (client_record_id == ?)",
            client_record_id,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    pub(crate) async fn delete(
        mut connection: impl WriteConnection,
        client_record_id: Uuid,
    ) -> sqlx::Result<()> {
        query!(
            "DELETE FROM client_record WHERE client_record_id = ?",
            client_record_id,
        )
        .execute(connection.as_mut())
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

    use crate::{clients::create_user::BasicUserData, db::access::DbAccess};

    use super::*;

    fn new_client_record(id: Uuid, created_at: DateTime<Utc>) -> ClientRecord {
        let user_id = UserId::new(id, "localhost".parse().unwrap());
        ClientRecord {
            user_id,
            client_record_state: ClientRecordState::Finished,
            created_at,
            is_default: false,
            client_record_id: Uuid::new_v4(),
        }
    }

    fn test_client_record() -> ClientRecord {
        new_client_record(Uuid::new_v4(), Utc::now())
    }

    #[sqlx::test]
    async fn persistence(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let mut alice_record = test_client_record();
        let mut bob_record = test_client_record();

        // Storing and loading client records works
        alice_record.store(pool.write().await?).await?;
        bob_record.store(pool.write().await?).await?;
        let records = ClientRecord::load_all(pool.read().await?).await?;
        assert_eq!(records, [alice_record.clone(), bob_record.clone()]);

        // Set default to alice set alice is_default
        alice_record.is_default = true;
        ClientRecord::set_default(pool.write().await?, alice_record.client_record_id).await?;
        let records = ClientRecord::load_all(pool.read().await?).await?;
        assert_eq!(records, [alice_record.clone(), bob_record.clone()]);

        // Set default to bob clears alice is_default
        alice_record.is_default = false;
        bob_record.is_default = true;
        ClientRecord::set_default(pool.write().await?, bob_record.client_record_id).await?;
        let records = ClientRecord::load_all(pool.read().await?).await?;
        assert_eq!(records, [alice_record.clone(), bob_record.clone()]);

        // Delete client records
        ClientRecord::delete(pool.write().await?, alice_record.client_record_id).await?;
        ClientRecord::delete(pool.write().await?, bob_record.client_record_id).await?;
        let records = ClientRecord::load_all(pool.read().await?).await?;
        assert_eq!(records, []);

        Ok(())
    }

    static USER_CREATION_STATE_BASIC: LazyLock<UserCreationState> = LazyLock::new(|| {
        let user_id = Uuid::from_u128(1);

        UserCreationState::BasicUserData(BasicUserData {
            user_id: UserId::new(user_id, "localhost".parse().unwrap()),
            push_token: Some(PushToken::new(
                PushTokenOperator::Google,
                "token".to_owned(),
            )),
        })
    });

    #[test]
    fn user_creation_state_basic_serde_codec() {
        let bytes = PersistenceCodec::to_vec(&*USER_CREATION_STATE_BASIC).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    #[test]
    fn user_creation_state_basic_json_codec() {
        insta::assert_json_snapshot!(&*USER_CREATION_STATE_BASIC);
    }

    #[test]
    fn the_current_state_round_trips() {
        let bytes = PersistenceCodec::to_vec(&StorableUserCreationStateRef::CurrentVersion(
            &USER_CREATION_STATE_BASIC,
        ))
        .unwrap();

        let decoded: StorableUserCreationState = PersistenceCodec::from_slice(&bytes).unwrap();

        assert!(matches!(
            decoded,
            StorableUserCreationState::CurrentVersion(_)
        ));
    }

    /// A state a client wrote while the invitation code was persisted with it
    /// still decodes, and loses the code.
    #[test]
    fn a_v1_state_decodes_without_its_code() {
        #[derive(Serialize)]
        struct BasicUserDataV1<'a> {
            user_id: &'a UserId,
            push_token: Option<PushToken>,
            invitation_code: &'a str,
        }

        #[derive(Serialize)]
        enum UserCreationStateV1<'a> {
            BasicUserData(BasicUserDataV1<'a>),
        }

        // The tag those clients wrote, which is what pins the V1 variant's
        // rename.
        #[derive(Serialize)]
        enum StorableV1<'a> {
            CurrentVersion(UserCreationStateV1<'a>),
        }

        let user_id = UserId::new(Uuid::from_u128(1), "localhost".parse().unwrap());
        let bytes = PersistenceCodec::to_vec(&StorableV1::CurrentVersion(
            UserCreationStateV1::BasicUserData(BasicUserDataV1 {
                user_id: &user_id,
                push_token: None,
                invitation_code: "DUMMY007",
            }),
        ))
        .unwrap();

        let decoded: StorableUserCreationState = PersistenceCodec::from_slice(&bytes).unwrap();
        let StorableUserCreationState::V1(state) = decoded else {
            panic!("a v1 state decoded as the current version");
        };

        let state: UserCreationState = state.into();
        assert_eq!(state.user_id(), &user_id);
    }
}
