// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Closing a connection request whose sender is a contact already. The
//! recipient joins the connection group and deletes it with a tombstone to
//! indicate the duplication of the connection request.

use aircommon::identifiers::QsReference;
use mls_assist::messages::{ApqAssistedMessageIn, AssistedMessageIn, SerializedMlsMessage};
use thiserror::Error;
use tonic::Status;

use crate::errors::GroupDeletionError;

use super::{
    delete_group::DeleteKind,
    group_state::DsGroupState,
    join_connection_group::{ConnectionGroupJoin, JoinConnectionGroupError},
};

/// The commits of a close, in the order the members get them.
pub(super) struct ClosedConnectionGroup {
    pub(super) join: SerializedMlsMessage,
    pub(super) delete: SerializedMlsMessage,
}

impl DsGroupState {
    /// Joins a connection group with `external_commit` and deletes it with
    /// `delete_commit` and sets an `AlreadyConnected` tombstone.
    pub(super) fn close_connection_group(
        &mut self,
        external_commit: AssistedMessageIn,
        delete_commit: AssistedMessageIn,
        qs_client_reference: QsReference,
    ) -> Result<ClosedConnectionGroup, CloseConnectionGroupError> {
        let join = self.join_connection_group(
            external_commit,
            qs_client_reference,
            ConnectionGroupJoin::Close,
        )?;
        let kind = DeleteKind::Close {
            joiner: join.joiner_index,
        };
        let delete = self.delete_group(delete_commit, kind)?;
        Ok(ClosedConnectionGroup {
            join: join.message,
            delete,
        })
    }

    /// The APQ variant of [`Self::close_connection_group`].
    pub(super) fn apq_close_connection_group(
        t: &mut Self,
        pq: &mut Self,
        external_commit: ApqAssistedMessageIn,
        delete_commit: ApqAssistedMessageIn,
        qs_client_reference: QsReference,
    ) -> Result<ClosedConnectionGroup, CloseConnectionGroupError> {
        let join = Self::apq_join_connection_group(
            t,
            pq,
            external_commit,
            qs_client_reference,
            ConnectionGroupJoin::Close,
        )?;
        let kind = DeleteKind::Close {
            joiner: join.joiner_index,
        };
        let delete = Self::delete_apq_group(t, pq, delete_commit, kind)?;
        Ok(ClosedConnectionGroup {
            join: join.message,
            delete,
        })
    }
}

#[derive(Debug, Error)]
pub(crate) enum CloseConnectionGroupError {
    #[error(transparent)]
    Join(#[from] JoinConnectionGroupError),
    #[error(transparent)]
    Delete(#[from] GroupDeletionError),
}

impl From<CloseConnectionGroupError> for Status {
    fn from(error: CloseConnectionGroupError) -> Self {
        match error {
            CloseConnectionGroupError::Join(error) => error.into(),
            CloseConnectionGroupError::Delete(error) => error.into(),
        }
    }
}
