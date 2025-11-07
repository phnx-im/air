// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::group::GroupId;
use sqlx::{Database, Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError};
use uuid::Uuid;

use crate::{MessageId, utils::persistence::GroupIdWrapper};

use super::ChatId;

impl<DB> Type<DB> for ChatId
where
    DB: Database,
    Uuid: Type<DB>,
{
    fn type_info() -> DB::TypeInfo {
        <Uuid as Type<DB>>::type_info()
    }
}

impl<'q, DB> Encode<'q, DB> for ChatId
where
    DB: Database,
    Uuid: Encode<'q, DB>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <DB as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        Encode::<DB>::encode_by_ref(&self.uuid, buf)
    }
}

impl<'r, DB> Decode<'r, DB> for ChatId
where
    DB: Database,
    Uuid: Decode<'r, DB>,
{
    fn decode(value: <DB as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let value: Uuid = Decode::<DB>::decode(value)?;
        Ok(Self::from(value))
    }
}

impl<DB> Type<DB> for MessageId
where
    DB: Database,
    Uuid: Type<DB>,
{
    fn type_info() -> DB::TypeInfo {
        <Uuid as Type<DB>>::type_info()
    }
}

impl<'q, DB> Encode<'q, DB> for MessageId
where
    DB: Database,
    Uuid: Encode<'q, DB>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <DB as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        Encode::<DB>::encode(self.uuid(), buf)
    }
}

impl<'r, DB> Decode<'r, DB> for MessageId
where
    DB: Database,
    Uuid: Decode<'r, DB>,
{
    fn decode(value: <DB as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let value: Uuid = Decode::<DB>::decode(value)?;
        Ok(Self::new(value))
    }
}

impl Type<Sqlite> for GroupIdWrapper {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <&[u8] as Type<Sqlite>>::type_info()
    }
}

impl<'r> Decode<'r, Sqlite> for GroupIdWrapper {
    fn decode(value: <Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let value: &[u8] = Decode::<Sqlite>::decode(value)?;
        Ok(GroupIdWrapper(GroupId::from_slice(value)))
    }
}
