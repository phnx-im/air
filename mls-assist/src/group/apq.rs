// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use apqmls::processing::ApqProcessedMessage;
use chrono::Duration;
use openmls::group::MergeCommitError;

use crate::{
    group::{ApqProcessedAssistedMessage, ProcessedAssistedMessage, errors::StorageError},
    provider_traits::MlsAssistStorageProvider,
};

use super::Group;

pub struct ApqGroupRef<'a> {
    pub(crate) t_group: &'a mut Group,
    pub(crate) pq_group: &'a mut Group,
}

impl<'a> ApqGroupRef<'a> {
    pub fn from_groups(t_group: &'a mut Group, pq_group: &'a mut Group) -> Self {
        Self { t_group, pq_group }
    }

    pub fn accept_apq_processed_message<StorageProvider: MlsAssistStorageProvider>(
        &mut self,
        t_provider: &StorageProvider,
        pq_provider: &StorageProvider,
        ApqProcessedAssistedMessage {
            processed_message:
                ApqProcessedMessage {
                    t_message,
                    pq_message,
                },
            group_info,
        }: ApqProcessedAssistedMessage,
        expiration_time: Duration,
    ) -> Result<(), MergeCommitError<StorageError<StorageProvider>>> {
        let (t_group_info, pq_group_info) = group_info.into_parts();
        self.t_group.accept_processed_message(
            t_provider,
            ProcessedAssistedMessage::Commit(t_message, Box::new(t_group_info)),
            expiration_time,
        )?;
        self.pq_group.accept_processed_message(
            pq_provider,
            ProcessedAssistedMessage::Commit(pq_message, Box::new(pq_group_info)),
            expiration_time,
        )?;
        Ok(())
    }
}
