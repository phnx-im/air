// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Undoes the device links the linking flow left unfinished.

use anyhow::Context as _;
use tracing::{error, info};

use crate::{
    clients::multi_device::device_link::{DeviceLink, DeviceLinkState},
    groups::self_group::SelfGroup,
    job::pending_chat_operation::PendingChatOperation,
    outbound_service::OutboundServiceContext,
};

use super::retry_pending_chat_operations::free_self_group;

impl OutboundServiceContext {
    /// Undoes the device links that were abandoned.
    pub(super) async fn advance_device_links(&self) -> anyhow::Result<()> {
        let now = chrono::Utc::now();
        let links = self
            .db
            .with_write_transaction(async |txn| {
                DeviceLink::abandon_expired(&mut *txn, now).await?;
                DeviceLink::load_actionable(&mut *txn).await
            })
            .await?;

        for link in links {
            let link_id = link.link_id;
            let advanced = match link.state {
                DeviceLinkState::Abandoned => self.undo_device_link(link).await,
                DeviceLinkState::Provisioned | DeviceLinkState::Adding => Ok(()),
            };
            if let Err(error) = advanced {
                error!(%error, %link_id, "failed to advance a device link");
            }
        }
        Ok(())
    }

    /// Takes an abandoned link's new device out of the self group if it got
    /// in, then deletes its queue and forgets the link.
    async fn undo_device_link(&self, link: DeviceLink) -> anyhow::Result<()> {
        let in_self_group = self
            .db
            .with_write_transaction(async |txn| -> anyhow::Result<Option<bool>> {
                let self_group = SelfGroup::load(&mut *txn).await?.context("no self group")?;
                // An operation in flight may be the add, which can still land.
                let self_group_id = self_group.group().group_id().clone();
                if free_self_group(txn, &self_group_id).await?.is_none() {
                    return Ok(None);
                }
                let Some(client_id) = link.client_id else {
                    return Ok(Some(false));
                };
                if !self_group.client_ids()?.contains(&client_id) {
                    return Ok(Some(false));
                }
                PendingChatOperation::create_remove_clients(txn, vec![client_id]).await?;
                Ok(Some(true))
            })
            .await?;
        if in_self_group != Some(false) {
            return Ok(());
        }

        let queue = &link.queue;
        let deleted = self
            .api_clients
            .default_client()?
            .qs_delete_client(queue.qs_client_id, &queue.qs_client_signing_key)
            .await;
        match deleted {
            Ok(()) => (),
            Err(error) if error.is_unknown_client() => (),
            Err(error) => return Err(error.into()),
        }
        DeviceLink::delete(self.db.write().await?, link.link_id).await?;
        info!(link_id = %link.link_id, "undid an abandoned device link");
        Ok(())
    }
}
