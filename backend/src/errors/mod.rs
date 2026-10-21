// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airprotos::common::v1::{
    DeviceLimitReachedDetail, StatusDetails, StatusDetailsCode, UncommittedSelfRemoveDetail,
    WrongEpochDetail,
    status_details::{self, Detail},
};
use apqmls::processing::ApqProcessPublicMessageError;
use displaydoc::Display;
use mls_assist::{
    group::{
        self,
        errors::{ProcessApqAssistedMessageError, ProcessAssistedMessageError},
    },
    memory_provider::MlsAssistMemoryStorage,
    openmls::group::{MergeCommitError, PublicProcessMessageError, ValidationError},
};
use prost::Message;
use thiserror::Error;
use tokio::sync::mpsc;
use tonic::{Code, Status};
use tracing::{error, warn};

use aircommon::codec::PersistenceCodec;

use crate::ds::group_state::SelfRemoveCheckError;

pub(crate) mod auth_service;
pub(crate) mod qs;

pub(crate) type CborMlsAssistStorage = MlsAssistMemoryStorage<PersistenceCodec>;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error("Error deserializing column: {0}")]
    Serde(#[from] aircommon::codec::Error),
}

impl From<sqlx::Error> for StorageError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e.into())
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for StorageError {
    fn from(e: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Self::Database(e.into())
    }
}

impl From<StorageError> for Status {
    fn from(error: StorageError) -> Self {
        error!(%error, "storage error");
        match error {
            StorageError::Database(_) => Self::internal("Database error"),
            StorageError::Serde(_) => Self::internal("Seriazation error"),
        }
    }
}
#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Dynamic(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// General error while accessing the requested queue.
#[derive(Error, Debug, Display)]
pub(super) enum QueueError {
    /// Database error {0}
    Storage(#[from] StorageError),
    /// Payload receiver closed
    PayloadReceiverClosed,
}

impl From<sqlx::Error> for QueueError {
    fn from(e: sqlx::Error) -> Self {
        Self::Storage(e.into())
    }
}

impl From<aircommon::codec::Error> for QueueError {
    fn from(e: aircommon::codec::Error) -> Self {
        Self::Storage(e.into())
    }
}

impl<T> From<mpsc::error::SendError<T>> for QueueError {
    fn from(_: mpsc::error::SendError<T>) -> Self {
        Self::PayloadReceiverClosed
    }
}

impl From<QueueError> for Status {
    fn from(error: QueueError) -> Self {
        let msg = error.to_string();
        match error {
            QueueError::Storage(error) => {
                error!(%error, "storage error");
                Self::internal("Database error")
            }
            QueueError::PayloadReceiverClosed => Self::internal(msg),
        }
    }
}

/// Potential errors when performing a group operation.
#[derive(Debug, Error)]
pub(crate) enum GroupOperationError {
    #[error("Error serializing message: {0}")]
    SerializationError(#[from] tls_codec::Error),
    /// Invalid assisted message.
    #[error("Invalid assisted message")]
    InvalidMessage,
    #[error("Error processing message: {0}")]
    ProcessingError(#[from] ProcessAssistedMessageError),
    #[error("Error processing message: {0}")]
    ApqProcessingError(#[from] ProcessApqAssistedMessageError),
    /// Membership change of an APQ group via a non-APQ group operation.
    #[error("APQ group requires an APQ group operation")]
    ApqMembershipChange,
    /// Missing queue config in client key package.
    #[error("Missing queue config in client key package")]
    MissingQueueConfig,
    /// Incomplete Welcome message.
    #[error("Incomplete Welcome message.")]
    IncompleteWelcome,
    #[error(transparent)]
    SelfRemoveCheck(#[from] SelfRemoveCheckError),
    #[error("Error merging commit: {0}")]
    MergeCommitError(#[from] MergeCommitError<group::errors::StorageError<CborMlsAssistStorage>>),
    #[error("Max devices exceeded")]
    MaxDevicesExceeded { max_devices: u32 },
}

/// The DS's classification of an error from processing an assisted message.
#[derive(Display)]
enum ProcessErrorKind {
    /// Invalid assisted message
    InvalidMessage,
    /// Message epoch differs from the group's epoch
    WrongEpoch,
    /// Error processing message
    ProcessingError,
}

impl ProcessErrorKind {
    /// Converts `error` of this kind into a status and logs it, unless it is a
    /// wrong epoch. `invalid` creates the status of an invalid message.
    fn into_status(self, error: &dyn std::error::Error, invalid: fn(String) -> Status) -> Status {
        let msg = self.to_string();
        match self {
            Self::InvalidMessage => {
                warn!(%error, "Invalid assisted message");
                invalid(msg)
            }
            Self::WrongEpoch => wrong_epoch_status(msg),
            Self::ProcessingError => {
                error!(%error, "Failed to process assisted message");
                Status::internal(msg)
            }
        }
    }
}

impl From<&ProcessAssistedMessageError> for ProcessErrorKind {
    fn from(error: &ProcessAssistedMessageError) -> Self {
        match error {
            ProcessAssistedMessageError::InvalidAssistedMessage
            | ProcessAssistedMessageError::GroupInfoValidation(_) => Self::InvalidMessage,
            ProcessAssistedMessageError::LibraryError(_) => Self::ProcessingError,
            ProcessAssistedMessageError::ProcessMessageError(error) => match error {
                PublicProcessMessageError::ValidationError(error) => match error {
                    ValidationError::LibraryError(_) => Self::ProcessingError,
                    ValidationError::WrongEpoch => Self::WrongEpoch,
                    _ => Self::InvalidMessage,
                },
                _ => Self::ProcessingError,
            },
            ProcessAssistedMessageError::ApqProcessMessage(error) => error.into(),
        }
    }
}

impl From<&ProcessApqAssistedMessageError> for ProcessErrorKind {
    fn from(error: &ProcessApqAssistedMessageError) -> Self {
        match error {
            ProcessApqAssistedMessageError::InvalidAssistedMessage
            | ProcessApqAssistedMessageError::GroupInfoValidation(_) => Self::InvalidMessage,
            ProcessApqAssistedMessageError::LibraryError(_) => Self::ProcessingError,
            ProcessApqAssistedMessageError::ProcessMessageError(error) => error.into(),
        }
    }
}

impl From<&ApqProcessPublicMessageError> for ProcessErrorKind {
    fn from(error: &ApqProcessPublicMessageError) -> Self {
        match error {
            ApqProcessPublicMessageError::Processing(
                PublicProcessMessageError::ValidationError(ValidationError::WrongEpoch),
            ) => Self::WrongEpoch,
            ApqProcessPublicMessageError::Processing(
                PublicProcessMessageError::ValidationError(ValidationError::LibraryError(_)),
            )
            | ApqProcessPublicMessageError::AppDataUpdate(_) => Self::ProcessingError,
            ApqProcessPublicMessageError::Processing(_)
            | ApqProcessPublicMessageError::Validation(_)
            | ApqProcessPublicMessageError::ApqInfoUpdate(_) => Self::InvalidMessage,
        }
    }
}

impl From<GroupOperationError> for Status {
    fn from(e: GroupOperationError) -> Self {
        let msg = e.to_string();
        match e {
            GroupOperationError::InvalidMessage => {
                error!("Invalid message");
                Status::internal(msg)
            }
            GroupOperationError::SerializationError(error) => {
                error!(%error, "Failed to serialize message");
                Status::internal("Error serializing message")
            }
            GroupOperationError::ProcessingError(error) => {
                ProcessErrorKind::from(&error).into_status(&error, Status::internal)
            }
            GroupOperationError::ApqProcessingError(error) => {
                ProcessErrorKind::from(&error).into_status(&error, Status::internal)
            }
            GroupOperationError::MissingQueueConfig | GroupOperationError::IncompleteWelcome => {
                Status::invalid_argument(msg)
            }
            GroupOperationError::ApqMembershipChange => Status::failed_precondition(msg),
            GroupOperationError::SelfRemoveCheck(error) => error.into(),
            GroupOperationError::MergeCommitError(error) => {
                error!(%error, "Failed merging commit");
                Status::internal("Error merging commit")
            }
            GroupOperationError::MaxDevicesExceeded { max_devices } => {
                let details = StatusDetails {
                    code: StatusDetailsCode::DeviceLimitReached.into(),
                    detail: Some(status_details::Detail::DeviceLimitReached(
                        DeviceLimitReachedDetail { max_devices },
                    )),
                };
                Status::with_details(
                    Code::ResourceExhausted,
                    "max devices exceeded",
                    details.encode_to_vec().into(),
                )
            }
        }
    }
}

/// Potential errors when deleting a group.
#[derive(Debug, Error)]
pub(crate) enum GroupDeletionError {
    /// Invalid assisted message.
    #[error("Invalid assisted message")]
    InvalidMessage,
    #[error("Error processing message: {0}")]
    ProcessingError(#[from] ProcessAssistedMessageError),
    #[error("Error processing message: {0}")]
    ApqProcessingError(#[from] ProcessApqAssistedMessageError),
    #[error("Error merging commit: {0}")]
    MergeCommitError(#[from] MergeCommitError<group::errors::StorageError<CborMlsAssistStorage>>),
}

impl From<GroupDeletionError> for Status {
    fn from(e: GroupDeletionError) -> Self {
        let msg = e.to_string();
        match e {
            GroupDeletionError::InvalidMessage => Status::invalid_argument(msg),
            GroupDeletionError::ProcessingError(error) => {
                ProcessErrorKind::from(&error).into_status(&error, Status::invalid_argument)
            }
            GroupDeletionError::ApqProcessingError(error) => {
                ProcessErrorKind::from(&error).into_status(&error, Status::invalid_argument)
            }
            GroupDeletionError::MergeCommitError(error) => {
                error!(%error, "Failed merging commit");
                Status::internal("Error merging commit")
            }
        }
    }
}

/// Potential errors when processing a self remove proposal.
#[derive(Debug, Error)]
pub(crate) enum ClientSelfRemovalError {
    /// Invalid assisted message.
    #[error("Invalid assisted message")]
    InvalidMessage,
    #[error("Error processing message: {0}")]
    ProcessingError(#[from] ProcessAssistedMessageError),
    #[error("Error merging commit: {0}")]
    MergeCommitError(#[from] MergeCommitError<group::errors::StorageError<CborMlsAssistStorage>>),
}

fn wrong_epoch_status(msg: String) -> Status {
    StatusDetails {
        code: StatusDetailsCode::WrongEpoch.into(),
        detail: Some(Detail::WrongEpoch(WrongEpochDetail {})),
    }
    .to_status(Code::InvalidArgument, msg)
}

impl From<SelfRemoveCheckError> for Status {
    fn from(error: SelfRemoveCheckError) -> Self {
        let msg = error.to_string();
        match error {
            SelfRemoveCheckError::ProposalStore => Status::internal(msg),
            SelfRemoveCheckError::Uncommitted => StatusDetails {
                code: StatusDetailsCode::UncommittedSelfRemove.into(),
                detail: Some(Detail::UncommittedSelfRemove(
                    UncommittedSelfRemoveDetail {},
                )),
            }
            .to_status(Code::InvalidArgument, msg),
        }
    }
}

impl From<ClientSelfRemovalError> for Status {
    fn from(e: ClientSelfRemovalError) -> Self {
        let msg = e.to_string();
        match e {
            ClientSelfRemovalError::InvalidMessage => Status::invalid_argument(msg),
            ClientSelfRemovalError::ProcessingError(error) => {
                ProcessErrorKind::from(&error).into_status(&error, Status::invalid_argument)
            }
            ClientSelfRemovalError::MergeCommitError(error) => {
                error!(%error, "Failed merging commit");
                Status::internal("Error merging commit")
            }
        }
    }
}

/// Potential errors when resyncing a client.
#[derive(Debug, Error)]
pub(crate) enum ResyncClientError {
    /// Invalid assisted message.
    #[error("Invalid assisted message")]
    InvalidMessage,
    #[error("Error processing message: {0}")]
    ProcessingError(#[from] ProcessAssistedMessageError),
    #[error("Error processing message: {0}")]
    ApqProcessingError(#[from] ProcessApqAssistedMessageError),
    #[error("Self-remove check error: {0}")]
    SelfRemoveCheck(#[from] SelfRemoveCheckError),
    #[error("Error merging commit: {0}")]
    MergeCommitError(#[from] MergeCommitError<group::errors::StorageError<CborMlsAssistStorage>>),
}

impl From<ResyncClientError> for Status {
    fn from(e: ResyncClientError) -> Self {
        let msg = e.to_string();
        match e {
            ResyncClientError::InvalidMessage => Status::invalid_argument(msg),
            ResyncClientError::ProcessingError(error) => {
                error!(%error, "Failed to process assisted message");
                Status::internal("Error processing message")
            }
            ResyncClientError::ApqProcessingError(error) => {
                error!(%error, "Failed to process APQ assisted message");
                Status::internal("Error processing message")
            }
            ResyncClientError::SelfRemoveCheck(error) => {
                error!(%error, "Failed self-remove check");
                error.into()
            }
            ResyncClientError::MergeCommitError(error) => {
                error!(%error, "Failed merging commit");
                Status::internal("Error merging commit")
            }
        }
    }
}
