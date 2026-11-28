// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Marking users as verified.

use aircommon::{
    credentials::UserCredential, crypto::hash::Hash, identifiers::UserId, time::TimeStamp,
};
use chrono::{DateTime, Utc};
use tracing::warn;

use crate::{
    clients::CoreUser,
    db::access::{ReadConnection, WriteDbTransaction},
    groups::client_auth_info::StorableUserCredential,
};

pub(crate) mod persistence;
pub(crate) mod sync;
#[cfg(test)]
mod tests;

/// Whether the user marked another user as verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactVerificationStatus {
    Unverified,
    Verified,
}

/// A user marked as verified, with the fingerprint of the credential the user
/// compared and the time of the mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContactVerification {
    pub(crate) user_id: UserId,
    pub(crate) credential_fingerprint: Hash<UserCredential>,
    pub(crate) verified_at: DateTime<Utc>,
}

impl CoreUser {
    /// Marks the user as verified or removes the mark.
    ///
    /// The change gets a record in the chat with the contact and is parked for
    /// the siblings.
    pub async fn set_contact_verified(
        &self,
        user_id: UserId,
        verified: bool,
    ) -> anyhow::Result<()> {
        self.db()
            .with_write_transaction(async |txn: &mut WriteDbTransaction<'_>| {
                sync::set_and_park(txn, user_id, verified, TimeStamp::now_millis()).await
            })
            .await?;
        self.outbound_service().notify_pending_chat_operations();
        Ok(())
    }

    /// Whether the user is marked as verified.
    pub async fn contact_verification(
        &self,
        user_id: &UserId,
    ) -> anyhow::Result<ContactVerificationStatus> {
        let connection = self.db().read().await?;
        Ok(ContactVerification::status(connection, user_id).await?)
    }
}

impl ContactVerification {
    /// The status of the user.
    pub(crate) async fn status(
        mut connection: impl ReadConnection,
        user_id: &UserId,
    ) -> sqlx::Result<ContactVerificationStatus> {
        let Some(mark) = Self::load(&mut connection, user_id).await? else {
            return Ok(ContactVerificationStatus::Unverified);
        };
        let Some(pinned) =
            StorableUserCredential::load_fingerprint(&mut connection, user_id).await?
        else {
            return Ok(ContactVerificationStatus::Unverified);
        };
        if pinned != mark.credential_fingerprint {
            warn!(
                ?user_id,
                "The verification mark names another credential than the pinned one"
            );
            return Ok(ContactVerificationStatus::Unverified);
        }
        Ok(ContactVerificationStatus::Verified)
    }
}
