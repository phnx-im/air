// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{crypto::ear::EarEncryptable, messages::push_token::PushToken};
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::clients::push_token_state;

use super::OutboundServiceContext;

impl OutboundServiceContext {
    pub(super) async fn send_pending_push_token_updates(
        &self,
        run_token: &CancellationToken,
    ) -> anyhow::Result<()> {
        if run_token.is_cancelled() {
            return Ok(());
        }

        let Some(state) = push_token_state::load_pending(&self.pool).await? else {
            return Ok(());
        };

        let push_token = state.to_push_token()?;
        self.update_push_token_on_qs(push_token).await?;
        push_token_state::clear_pending(&self.pool).await?;
        Ok(())
    }

    async fn update_push_token_on_qs(&self, push_token: Option<PushToken>) -> anyhow::Result<()> {
        match &push_token {
            Some(_) => debug!("Updating push token on QS"),
            None => debug!("Clearing push token on QS"),
        }

        let queue_encryption_key = self.key_store.qs_queue_decryption_key.encryption_key();
        let signing_key = self.key_store.qs_client_signing_key.clone();

        let encrypted_push_token = match push_token {
            Some(push_token) => {
                let encrypted_push_token =
                    push_token.encrypt(&self.key_store.push_token_ear_key)?;
                Some(encrypted_push_token)
            }
            None => None,
        };

        self.api_clients
            .default_client()?
            .qs_update_client(
                self.qs_client_id,
                queue_encryption_key.clone(),
                encrypted_push_token,
                &signing_key,
            )
            .await?;
        Ok(())
    }
}
