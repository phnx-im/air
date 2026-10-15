// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::Fqdn;
use anyhow::{Context, bail};
use tokio::sync::{mpsc, oneshot};
use url::Url;
use uuid::Uuid;

use super::MultiDeviceProvisionStep;
use crate::{
    clients::CoreUser,
    db::{access::DbAccess, notification::DbNotificationsSender},
    utils::{
        global_lock::GlobalLock,
        persistence::{open_air_db_in_memory, open_db_in_memory},
    },
};

impl CoreUser {
    /// Same as [`Self::new_linked`], except that databases are ephemeral and are dropped together
    /// with this instance of [`CoreUser`].
    pub async fn new_linked_ephemeral(
        domain: Fqdn,
        server_url: Url,
        session_tx: mpsc::Sender<MultiDeviceProvisionStep>,
    ) -> anyhow::Result<CoreUser> {
        let notifier_tx = DbNotificationsSender::new();
        let air_db =
            DbAccess::with_single_pool(open_air_db_in_memory().await?, notifier_tx.clone());
        let client_db = DbAccess::with_single_pool(open_db_in_memory().await?, notifier_tx);
        let client_record_id = Uuid::new_v4();
        let temp_file = tempfile::NamedTempFile::new()?;
        let global_lock = GlobalLock::from_path(temp_file.path())?;

        Self::new_linked_with_dbs(
            air_db,
            client_db,
            global_lock,
            client_record_id,
            domain,
            Some(server_url),
            session_tx,
        )
        .await
    }

    /// Links a new ephemeral device to this user and returns it.
    ///
    /// Both sides of the linking run in this process, and the link is confirmed
    /// without waiting for the user.
    pub async fn link_ephemeral_device(&self) -> anyhow::Result<CoreUser> {
        Box::pin(self.link_ephemeral_device_named("")).await
    }

    /// Like [`Self::link_ephemeral_device`], but names the new device the way
    /// the confirming user would. An empty `name` keeps the new device's
    /// default.
    pub async fn link_ephemeral_device_named(&self, name: &str) -> anyhow::Result<CoreUser> {
        let domain = self.user_id().domain().clone();
        let server_url = self
            .inner
            .api_clients
            .own_endpoint()
            .context("linking an ephemeral device requires a server URL")?
            .clone();

        let (session_tx, mut session_rx) = mpsc::channel(1);
        let new_device = async { Self::new_linked_ephemeral(domain, server_url, session_tx).await };
        let link = async {
            let code = match session_rx
                .recv()
                .await
                .context("provisioning ended before reporting a linking code")?
            {
                MultiDeviceProvisionStep::Code(code) => code,
                MultiDeviceProvisionStep::Linking => bail!("linking step before the code"),
            };
            let (confirmation_tx, confirmation_rx) = oneshot::channel();
            confirmation_tx
                .send(name.to_owned())
                .expect("the receiver is alive");
            // The sender ignores a dropped receiver.
            let (connected_tx, _) = oneshot::channel();
            self.multi_device_link_client(code, connected_tx, confirmation_rx)
                .await??;
            // The new device still reports a `Linking` step, which fails if the
            // receiver is dropped before it is done.
            anyhow::Ok(session_rx)
        };

        let (new_device, _session_rx) = tokio::try_join!(new_device, link)?;
        Ok(new_device)
    }
}
