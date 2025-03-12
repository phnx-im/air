// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ops::Deref;

use chrono::{DateTime, SubsecRound, TimeZone, Utc};
#[cfg(feature = "sqlite")]
use rusqlite::{types::FromSql, ToSql};

use super::*;

pub use chrono::Duration;

/// A time stamp that can be used to represent a point in time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Eq, Hash, Copy)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct TimeStamp(DateTime<Utc>);

impl AsRef<DateTime<Utc>> for TimeStamp {
    fn as_ref(&self) -> &DateTime<Utc> {
        &self.0
    }
}

impl Deref for TimeStamp {
    type Target = DateTime<Utc>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<DateTime<Utc>> for TimeStamp {
    fn from(time: DateTime<Utc>) -> Self {
        Self(time)
    }
}

impl From<TimeStamp> for DateTime<Utc> {
    fn from(time: TimeStamp) -> Self {
        time.0
    }
}

impl TryFrom<TimeStamp> for i64 {
    type Error = TimeStampError;

    fn try_from(time: TimeStamp) -> Result<Self, Self::Error> {
        time.0
            .timestamp_nanos_opt()
            .ok_or(TimeStampError::InvalidInput)
    }
}

impl From<i64> for TimeStamp {
    fn from(time: i64) -> Self {
        Utc.timestamp_nanos(time).into()
    }
}

#[derive(Error, Debug)]
pub enum TimeStampError {
    #[error("Invalid input")]
    InvalidInput,
}

const I64_SIZE: usize = std::mem::size_of::<i64>();

impl Size for TimeStamp {
    fn tls_serialized_len(&self) -> usize {
        I64_SIZE
    }
}

impl TlsSerializeTrait for TimeStamp {
    fn tls_serialize<W: std::io::Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        let time_i64 = i64::try_from(*self).map_err(|e| {
            tracing::error!("Failed to serialize timestamp: {}", e);
            tls_codec::Error::InvalidInput
        })?;
        time_i64.to_be_bytes().tls_serialize(writer)
    }
}

impl TlsDeserializeBytesTrait for TimeStamp {
    fn tls_deserialize_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), tls_codec::Error>
    where
        Self: Sized,
    {
        let time_i64_bytes: [u8; I64_SIZE] = bytes
            .get(..I64_SIZE)
            .ok_or(tls_codec::Error::EndOfStream)?
            .try_into()
            .map_err(|_| tls_codec::Error::EndOfStream)?;
        let time_i64 = i64::from_be_bytes(time_i64_bytes);
        let time = TimeStamp::from(time_i64);
        Ok((time, &bytes[I64_SIZE..]))
    }
}

#[cfg(feature = "sqlite")]
impl ToSql for TimeStamp {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

#[cfg(feature = "sqlite")]
impl FromSql for TimeStamp {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let time = DateTime::<Utc>::column_result(value)?;
        Ok(time.into())
    }
}

impl TimeStamp {
    /// Same as [`Utc::now`], but rounded to microsecond precision.
    pub fn now() -> Self {
        // Note: databases only support microsecond precision.
        Utc::now().round_subsecs(6).into()
    }

    /// Checks if this time stamp is more than `expiration` in the past.
    pub fn has_expired(&self, expiration: Duration) -> bool {
        let time_left = Utc::now() - expiration;
        time_left >= self.0
    }

    fn is_between(&self, start: &Self, end: &Self) -> bool {
        self.0 >= start.0 && self.0 <= end.0
    }
}

#[cfg(test)]
mod timestamp_conversion {
    use super::*;

    #[test]
    fn timestamp_conversion() {
        let time = TimeStamp::now();
        let time_i64 = i64::try_from(time).unwrap();
        let time_converted = TimeStamp::from(time_i64);
        assert_eq!(time, time_converted);

        let time = TimeStamp::now();
        let time_serialized = time.tls_serialize_detached().unwrap();
        let time_deserialized = TimeStamp::tls_deserialize_exact_bytes(&time_serialized).unwrap();
        assert_eq!(time, time_deserialized);
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, TlsDeserializeBytes, TlsSerialize, TlsSize, Serialize, Deserialize,
)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(type_name = "expiration"))]
pub struct ExpirationData {
    not_before: TimeStamp,
    not_after: TimeStamp,
}

impl ExpirationData {
    /// Create a new instance of [`ExpirationData`] that expires in `lifetime`
    /// days and the validity of which starts now.
    pub fn new(lifetime: Duration) -> Self {
        // Note: databases only support microsecond precision.
        let not_before = Utc::now().round_subsecs(6) - Duration::minutes(15);
        Self {
            not_before: TimeStamp::from(not_before),
            not_after: TimeStamp::from(not_before + lifetime),
        }
    }

    /// Return false either if the `not_after` date has passed, or if the
    /// `not_before` date has not passed yet.
    pub fn validate(&self) -> bool {
        let now = TimeStamp::now();
        now.is_between(&self.not_before, &self.not_after)
    }

    pub fn not_before(&self) -> TimeStamp {
        self.not_before
    }

    pub fn not_after(&self) -> TimeStamp {
        self.not_after
    }
}
