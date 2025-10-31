// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::ear::keys::GroupStateEarKey, identifiers::MimiId,
    messages::client_ds_out::SendMessageParamsOut, time::TimeStamp,
};
use anyhow::Context;
use mimi_content::{
    ByteBuf, Disposition, MessageStatus, MessageStatusReport, MimiContent, NestedPart,
    NestedPartContent, PerMessageStatus,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};
use uuid::Uuid;

use crate::{
    Chat, ChatId, ChatStatus, MessageId,
    chats::StatusRecord,
    groups::{Group, openmls_provider::AirOpenMlsProvider},
    outbound_service::error::OutboundServiceError,
    utils::connection_ext::StoreExt,
};

use super::{OutboundService, OutboundServiceContext, receipt_queue::ReceiptQueue};

impl OutboundService {
    pub async fn enqueue_receipts<'a>(
        &self,
        chat_id: ChatId,
        statuses: impl Iterator<Item = (MessageId, &'a MimiId, MessageStatus)> + Send,
    ) -> anyhow::Result<()> {
        let mut connection = self.context.pool.acquire().await?;

        let chat = Chat::load(&mut connection, &chat_id)
            .await?
            .with_context(|| format!("Can't find chat with id {chat_id}"))?;
        if let ChatStatus::Blocked = chat.status() {
            return Ok(());
        }

        for (message_id, mimi_id, status) in statuses {
            let receipt_queue = ReceiptQueue::new(message_id, status);
            receipt_queue
                .enqueue(&mut *connection, chat_id, mimi_id)
                .await?;
        }

        self.notify_work();

        Ok(())
    }
}

impl OutboundServiceContext {
    pub(super) async fn send_queued_receipts(
        &self,
        run_token: &CancellationToken,
    ) -> anyhow::Result<()> {
        // Used to identify locked receipts by this task
        let task_id = Uuid::new_v4();
        loop {
            if run_token.is_cancelled() {
                return Ok(()); // the task is being stopped
            }

            let Some((chat_id, statuses)) = ReceiptQueue::dequeue(&self.pool, task_id).await?
            else {
                return Ok(());
            };
            debug!(?chat_id, num_statuses = statuses.len(), "dequeued receipt");

            match UnsentReceipt::new(statuses.iter().map(|(mimi_id, status)| (mimi_id, *status))) {
                Ok(Some(receipt)) => match self.send_chat_receipt(chat_id, receipt).await {
                    Ok(_) => {
                        ReceiptQueue::remove(&self.pool, task_id).await?;
                    }
                    Err(OutboundServiceError::Fatal(error)) => {
                        error!(%error, "Failed to send receipt; dropping");
                        ReceiptQueue::remove(&self.pool, task_id).await?;
                        return Err(error);
                    }
                    Err(OutboundServiceError::Recoverable(error)) => {
                        error!(%error, "Failed to send receipt; will retry later");
                        // Don't unlock the receipts now; they will be unlocked after a threshold.
                        continue;
                    }
                },
                Ok(None) => {
                    // Nothing to send => Remove from the queue
                    ReceiptQueue::remove(&self.pool, task_id).await?;
                }
                Err(error) => {
                    error!(%error, "Failed to create receipt; dropping");
                    // There is no chance we will be able to create a receipt next time
                    // => Remove from the queue
                    ReceiptQueue::remove(&self.pool, task_id).await?;
                }
            };
        }
    }

    async fn send_chat_receipt(
        &self,
        chat_id: ChatId,
        unsent_receipt: UnsentReceipt,
    ) -> Result<(), OutboundServiceError> {
        debug!(%chat_id, ?unsent_receipt, "sending receipt");

        // load chat
        let chat = {
            let mut connection = self
                .pool
                .acquire()
                .await
                .map_err(OutboundServiceError::recoverable)?;
            let chat = Chat::load(&mut connection, &chat_id)
                .await
                .map_err(OutboundServiceError::recoverable)?
                .with_context(|| format!("Can't find chat with id {chat_id}"))
                .map_err(OutboundServiceError::fatal)?;
            if let ChatStatus::Blocked = chat.status() {
                return Ok(());
            }
            chat
        };

        // load group and create MLS message
        let (group_state_ear_key, params) =
            self.new_mls_message(&chat, unsent_receipt.content).await?;

        // send MLS message to DS
        self.api_clients
            .get(&chat.owner_domain())
            .map_err(OutboundServiceError::fatal)?
            .ds_send_message(params, &self.signing_key, &group_state_ear_key)
            .await
            .map_err(OutboundServiceError::recoverable)?;

        // store delivery receipt report
        self.with_transaction_and_notifier(async |txn, notifier| {
            StatusRecord::borrowed(self.user_id(), unsent_receipt.report, TimeStamp::now())
                .store_report(txn, notifier)
                .await?;
            Ok(())
        })
        .await
        .map_err(OutboundServiceError::fatal)?;

        Ok(())
    }

    pub(super) async fn new_mls_message(
        &self,
        chat: &Chat,
        mimi_content: MimiContent,
    ) -> Result<(GroupStateEarKey, SendMessageParamsOut), OutboundServiceError> {
        self.with_transaction(async |txn| {
            let group_id = chat.group_id();
            let mut group = Group::load_clean(txn.as_mut(), group_id)
                .await?
                .with_context(|| format!("Can't find group with id {group_id:?}"))?;
            let params = group.create_message(
                &AirOpenMlsProvider::new(txn.as_mut()),
                &self.signing_key,
                mimi_content,
            )?;
            Ok((group.group_state_ear_key().clone(), params))
        })
        .await
        .map_err(OutboundServiceError::fatal)
    }
}

/// Not yet sent receipt message consisting of the content to send and a local message status
/// report.
#[derive(Debug)]
struct UnsentReceipt {
    report: MessageStatusReport,
    content: MimiContent,
}

impl UnsentReceipt {
    fn new<'a>(
        statuses: impl IntoIterator<Item = (&'a MimiId, MessageStatus)>,
    ) -> anyhow::Result<Option<Self>> {
        let report = MessageStatusReport {
            statuses: statuses
                .into_iter()
                .map(|(id, status)| PerMessageStatus {
                    mimi_id: id.as_ref().to_vec().into(),
                    status,
                })
                .collect(),
        };

        if report.statuses.is_empty() {
            return Ok(None);
        }

        let content = MimiContent {
            salt: ByteBuf::from(aircommon::crypto::secrets::Secret::<16>::random()?.secret()),
            nested_part: NestedPart {
                disposition: Disposition::Unspecified,
                part: NestedPartContent::SinglePart {
                    content_type: "application/mimi-message-status".to_owned(),
                    content: report.serialize()?.into(),
                },
                ..Default::default()
            },
            ..Default::default()
        };

        Ok(Some(Self { report, content }))
    }
}
