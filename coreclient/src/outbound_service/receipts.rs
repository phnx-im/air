// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashSet;

use aircommon::{
    crypto::aead::keys::GroupStateEarKey,
    identifiers::MimiId,
    messages::client_ds_out::{SendMessageCollisionTag, SendMessageParamsOut},
    time::TimeStamp,
};
use anyhow::Context;
use mimi_content::{
    Disposition, MessageStatus, MessageStatusReport, MimiContent, NestedPart, PerMessageStatus,
};
use openmls::group::GroupEpoch;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};
use uuid::Uuid;

use crate::{
    Chat, ChatId, ChatStatus, MessageId,
    chats::StatusRecord,
    db::access::WriteDbTransaction,
    groups::{Group, handle_group_not_found_on_ds, openmls_provider::AirOpenMlsProvider},
    job::pending_chat_operation::PendingChatOperation,
    outbound_service::{
        error::{OutboundServiceError, classify_ds_error},
        resync::Resync,
    },
};

use super::{OutboundService, OutboundServiceContext, receipt_queue::ReceiptQueue};

impl OutboundService {
    pub async fn enqueue_receipts<'a>(
        &self,
        chat_id: ChatId,
        statuses: impl Iterator<Item = (MessageId, &'a MimiId, MessageStatus)> + Send,
    ) -> anyhow::Result<()> {
        self.context
            .db
            .with_write_transaction(async |txn| {
                self.schedule_receipts(txn, chat_id, statuses).await
            })
            .await?;
        self.notify_work();
        Ok(())
    }

    pub(crate) async fn schedule_receipts<'a>(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        chat_id: ChatId,
        statuses: impl Iterator<Item = (MessageId, &'a MimiId, MessageStatus)> + Send,
    ) -> anyhow::Result<()> {
        if Chat::is_blocked(&mut *txn, chat_id).await? {
            return Ok(());
        }

        for (message_id, mimi_id, status) in statuses {
            let receipt_queue = ReceiptQueue::new(message_id, status);
            receipt_queue.enqueue(&mut *txn, chat_id, mimi_id).await?;
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

            let Some((chat_id, statuses)) =
                ReceiptQueue::dequeue(self.db.write().await?, task_id).await?
            else {
                return Ok(());
            };

            // If a resync is pending, we skip sending receipts for this chat
            if Resync::is_pending_for_chat(self.db.read().await?, &chat_id).await? {
                debug!(?chat_id, "Skipping sending receipt due to pending resync");
                continue;
            }

            // If a chat operation is pending, we skip sending receipts for this chat
            if PendingChatOperation::is_pending_for_chat(self.db.read().await?, chat_id).await? {
                debug!(
                    ?chat_id,
                    "Skipping sending receipt due to pending chat operation"
                );
                continue;
            }

            debug!(?chat_id, num_statuses = statuses.len(), "dequeued receipt");

            match UnsentReceipt::new(statuses.iter().map(|(mimi_id, status)| (mimi_id, *status))) {
                Ok(Some(receipt)) => match self.send_chat_receipt(chat_id, receipt).await {
                    Ok(ReceiptSendOutcome::Sent) => {
                        ReceiptQueue::remove(self.db.write().await?, task_id).await?;
                    }
                    Ok(ReceiptSendOutcome::Collided { delivered }) => {
                        // A sibling already delivered `delivered`; drop just those
                        // receipts from the queue. The rest stay locked so they are
                        // re-encrypted and resent at a later generation.
                        ReceiptQueue::remove_delivered(self.db.write().await?, task_id, &delivered)
                            .await?;
                        continue;
                    }
                    Err(OutboundServiceError::Fatal(error)) => {
                        error!(%error, ?chat_id, "Failed to send receipt; dropping");
                        ReceiptQueue::remove(self.db.write().await?, task_id).await?;
                        continue;
                    }
                    Err(OutboundServiceError::Recoverable(error)) => {
                        error!(%error, "Failed to send receipt; will retry later");
                        // Don't unlock the receipts now; they will be unlocked after a threshold.
                        continue;
                    }
                },
                Ok(None) => {
                    // Nothing to send => Remove from the queue
                    ReceiptQueue::remove(self.db.write().await?, task_id).await?;
                }
                Err(error) => {
                    error!(%error, "Failed to create receipt; dropping");
                    // There is no chance we will be able to create a receipt next time
                    // => Remove from the queue
                    ReceiptQueue::remove(self.db.write().await?, task_id).await?;
                }
            };
        }
    }

    async fn send_chat_receipt(
        &self,
        chat_id: ChatId,
        unsent_receipt: UnsentReceipt,
    ) -> Result<ReceiptSendOutcome, OutboundServiceError> {
        debug!(%chat_id, ?unsent_receipt, "sending receipt");

        // load chat
        let chat = self
            .db
            .with_read_transaction(async |txn| Chat::load(txn, &chat_id).await)
            .await
            .map_err(OutboundServiceError::recoverable)?
            .with_context(|| format!("Can't find chat with id {chat_id}"))
            .map_err(OutboundServiceError::fatal)?;
        if let ChatStatus::Blocked = chat.status() {
            return Ok(ReceiptSendOutcome::Sent);
        }

        // load group and create MLS message
        let (group_state_ear_key, params) = self
            .new_mls_message(
                &chat,
                unsent_receipt.content,
                Some(unsent_receipt.report.clone()),
            )
            .await
            .map_err(OutboundServiceError::fatal)?;
        let epoch = params.epoch;
        let sent_tags = params.collision_tags.clone();
        let generation = params.generation;

        // send MLS message to DS
        if let Err(ds_error) = self
            .api_clients
            .get(&chat.owner_domain())
            .map_err(OutboundServiceError::fatal)?
            .ds_send_message(params, self.signing_key(), &group_state_ear_key)
            .await
        {
            if ds_error.is_not_found() {
                self.db
                    .with_write_transaction(async |txn| {
                        handle_group_not_found_on_ds(txn, chat.group_id()).await
                    })
                    .await
                    .map_err(OutboundServiceError::fatal)?;
                return Err(classify_ds_error(ds_error));
            }

            let collisions = ds_error.process_tag_collisions(&sent_tags);
            if collisions.is_empty() {
                // Not a collision we can recover from; propagate the error.
                return Err(classify_ds_error(ds_error));
            }

            // The DS rejects the whole message on any collision.
            // Split the report into the receipts a sibling already
            // delivered (their tag collided) and the ones still pending.
            let (delivered, any_pending) =
                partition_collided_receipts(&unsent_receipt.report, &sent_tags, &collisions);

            // Record the receipts a sibling already delivered so we treat them as
            // sent and stop trying to resend them.
            if !delivered.statuses.is_empty() {
                self.store_receipt_report(delivered.clone()).await?;
            }

            if any_pending {
                // Either the generation collided, or only some receipts did. The
                // surviving receipts must be re-encrypted and resent at a later
                // generation, so leave them queued.
                debug!(%chat_id, "Receipt collided; re-enqueuing surviving receipts for a later generation");
                return Ok(ReceiptSendOutcome::Collided { delivered });
            } else {
                // Every receipt was already delivered by a sibling (the
                // generation collision, if any, is moot — nothing left to send).
                debug!(%chat_id, "All receipts already delivered by a sibling; treating as sent");
                return Ok(ReceiptSendOutcome::Sent);
            }
        }

        // message accepted by DS, confirm.
        self.confirm_mls_message(&chat, epoch, generation)
            .await
            .inspect_err(|error| error!(%error, "failed to confirm MLS message"))
            .ok();

        // store delivery receipt report
        self.store_receipt_report(unsent_receipt.report).await?;

        Ok(ReceiptSendOutcome::Sent)
    }

    /// Record `report` locally as sent by this user (a sibling may have been the
    /// one to actually deliver it).
    async fn store_receipt_report(
        &self,
        report: MessageStatusReport,
    ) -> Result<(), OutboundServiceError> {
        self.db
            .with_write_transaction(async |txn| {
                StatusRecord::borrowed(self.user_id(), report, TimeStamp::now())
                    .store_report(txn)
                    .await
            })
            .await
            .map_err(OutboundServiceError::fatal)
    }

    /// Creates a new MLS message for the given chat.
    pub(super) async fn new_mls_message(
        &self,
        chat: &Chat,
        mimi_content: MimiContent,
        message_status_report: Option<MessageStatusReport>,
    ) -> anyhow::Result<(GroupStateEarKey, SendMessageParamsOut)> {
        self.db
            .with_write_transaction(async |txn| {
                let group_id = chat.group_id();
                let mut group = Group::load_clean(&mut *txn, group_id)
                    .await?
                    .with_context(|| format!("Can't find group with id {group_id:?}"))?;
                let provider = AirOpenMlsProvider::new(txn.as_mut());
                let params = group.create_message(
                    &provider,
                    self.signing_key(),
                    mimi_content,
                    message_status_report,
                )?;
                Ok((group.group_state_ear_key().clone(), params))
            })
            .await
    }

    /// Confirms a MLS message was sent to the DS.
    pub(super) async fn confirm_mls_message(
        &self,
        chat: &Chat,
        epoch: GroupEpoch,
        generation: u32,
    ) -> anyhow::Result<()> {
        self.db
            .with_write_transaction(async |txn| {
                let group_id = chat.group_id();
                let mut group = Group::load(&mut *txn, group_id)
                    .await?
                    .with_context(|| format!("Can't find group with id {group_id:?}"))?;
                let provider = AirOpenMlsProvider::new(txn.as_mut());
                group.confirm_application_message(&provider, epoch, generation)?;
                Ok(())
            })
            .await
    }
}

enum ReceiptSendOutcome {
    /// The receipt was delivered, or every per-message status it carried had
    /// already been delivered by a sibling. All rows locked for this task can be
    /// removed from the queue.
    Sent,
    /// A collision occurred. `delivered` are the per-message statuses a sibling
    /// had already delivered — remove just those from the queue. The remaining
    /// locked rows are left in place so they are re-encrypted and resent at a
    /// later generation.
    Collided { delivered: MessageStatusReport },
}

/// Split `report` into the per-message statuses whose collision tag the DS
/// reported as already present (a sibling client already delivered them), and a
/// flag indicating whether any statuses still need (re)sending.
fn partition_collided_receipts(
    report: &MessageStatusReport,
    sent_tags: &[SendMessageCollisionTag],
    collisions: &[SendMessageCollisionTag],
) -> (MessageStatusReport, bool) {
    let collided: HashSet<i64> = collisions.iter().map(|tag| tag.value()).collect();
    let receipt_tags = sent_tags
        .iter()
        .filter(|tag| !matches!(tag, SendMessageCollisionTag::Generation(_)));

    let mut delivered = Vec::new();
    let mut any_pending = false;
    for (status, tag) in report.statuses.iter().zip(receipt_tags) {
        if collided.contains(&tag.value()) {
            delivered.push(status.clone());
        } else {
            any_pending = true;
        }
    }

    (
        MessageStatusReport {
            statuses: delivered,
        },
        any_pending,
    )
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
                    mimi_id: id.as_ref().to_vec(),
                    status,
                })
                .collect(),
        };

        if report.statuses.is_empty() {
            return Ok(None);
        }

        let content = MimiContent {
            salt: aircommon::crypto::secrets::Secret::<16>::random()?
                .secret()
                .to_vec(),
            nested_part: NestedPart::SinglePart {
                disposition: Disposition::Unspecified,
                content_type: "application/mimi-message-status".to_owned(),
                content: report.serialize()?,
                language: Default::default(),
            },
            ..Default::default()
        };

        Ok(Some(Self { report, content }))
    }
}

#[cfg(test)]
mod tests {
    use aircommon::messages::client_ds_out::SendMessageCollisionTag;
    use mimi_content::{MessageStatus, MessageStatusReport, PerMessageStatus};

    use super::partition_collided_receipts;

    fn status(mimi_id: u8, status: MessageStatus) -> PerMessageStatus {
        PerMessageStatus {
            mimi_id: vec![mimi_id],
            status,
        }
    }

    fn fixture() -> (MessageStatusReport, Vec<SendMessageCollisionTag>) {
        let report = MessageStatusReport {
            statuses: vec![
                status(10, MessageStatus::Delivered),
                status(11, MessageStatus::Read),
            ],
        };
        let sent_tags = vec![
            SendMessageCollisionTag::generation(0),
            SendMessageCollisionTag::delivery_receipt(1),
            SendMessageCollisionTag::read_receipt(2),
        ];
        (report, sent_tags)
    }

    #[test]
    fn partition_collided_receipts_generation_only_collision_keeps_all_receipts() {
        let (report, sent_tags) = fixture();
        let collisions = vec![SendMessageCollisionTag::generation(0)];

        let (delivered, any_pending) =
            partition_collided_receipts(&report, &sent_tags, &collisions);

        assert!(delivered.statuses.is_empty());
        assert!(any_pending);
    }

    #[test]
    fn partition_collided_receipts_single_receipt_collision_drops_only_that_receipt() {
        let (report, sent_tags) = fixture();
        let collisions = vec![SendMessageCollisionTag::delivery_receipt(1)];

        let (delivered, any_pending) =
            partition_collided_receipts(&report, &sent_tags, &collisions);

        assert_eq!(
            delivered.statuses,
            vec![status(10, MessageStatus::Delivered)]
        );
        assert!(any_pending);
    }

    #[test]
    fn partition_collided_receipts_all_receipts_collided_leaves_nothing_pending() {
        let (report, sent_tags) = fixture();
        let collisions = vec![
            SendMessageCollisionTag::delivery_receipt(1),
            SendMessageCollisionTag::read_receipt(2),
        ];

        let (delivered, any_pending) =
            partition_collided_receipts(&report, &sent_tags, &collisions);

        assert_eq!(delivered.statuses, report.statuses);
        assert!(!any_pending);
    }

    #[test]
    fn partition_collided_receipts_generation_and_receipt_collision_drops_only_that_receipt() {
        let (report, sent_tags) = fixture();
        let collisions = vec![
            SendMessageCollisionTag::generation(0),
            SendMessageCollisionTag::read_receipt(2),
        ];

        let (delivered, any_pending) =
            partition_collided_receipts(&report, &sent_tags, &collisions);

        assert_eq!(delivered.statuses, vec![status(11, MessageStatus::Read)]);
        assert!(any_pending);
    }

    #[test]
    fn partition_collided_receipts_all_receipts_and_generation_collided_leaves_nothing_pending() {
        let (report, sent_tags) = fixture();
        let collisions = vec![
            SendMessageCollisionTag::generation(0),
            SendMessageCollisionTag::delivery_receipt(1),
            SendMessageCollisionTag::read_receipt(2),
        ];

        let (delivered, any_pending) =
            partition_collided_receipts(&report, &sent_tags, &collisions);

        assert_eq!(delivered.statuses, report.statuses);
        assert!(!any_pending);
    }

    #[test]
    fn partition_collided_receipts_no_collision_keeps_all_receipts_pending() {
        let (report, sent_tags) = fixture();

        let (delivered, any_pending) = partition_collided_receipts(&report, &sent_tags, &[]);

        assert!(delivered.statuses.is_empty());
        assert!(any_pending);
    }

    #[test]
    fn partition_collided_receipts_generation_tag_value_is_never_treated_as_a_receipt() {
        // A generation tag whose value coincides with a colliding value should
        // not drop a receipt: only the non-generation tags are paired against
        // the report statuses.
        let report = MessageStatusReport {
            statuses: vec![status(10, MessageStatus::Delivered)],
        };
        let sent_tags = vec![
            SendMessageCollisionTag::generation(42),
            SendMessageCollisionTag::delivery_receipt(99),
        ];
        let collisions = vec![SendMessageCollisionTag::generation(42)];

        let (delivered, any_pending) =
            partition_collided_receipts(&report, &sent_tags, &collisions);

        assert!(delivered.statuses.is_empty());
        assert!(any_pending);
    }
}
