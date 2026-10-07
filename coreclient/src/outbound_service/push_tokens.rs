// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::aead::AeadEncryptable,
    messages::push_token::PushToken,
    time::{Duration, TimeStamp},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

use crate::{clients::push_token_state, outbound_service::error::OutboundServiceRunError};

use super::OutboundServiceContext;

impl OutboundServiceContext {
    /// Processes a single due push token update with clamped retry timestamps.
    pub(super) async fn send_pending_push_token_updates(
        &self,
        run_token: &CancellationToken,
    ) -> Result<(), OutboundServiceRunError> {
        if run_token.is_cancelled() {
            return Ok(());
        }

        let now = TimeStamp::now();
        push_token_state::clamp_pending_future(self.db.write().await?, now).await?;
        let Some(state) = push_token_state::load_pending(self.db.write().await?, now).await? else {
            return Ok(());
        };

        let push_token = match state.to_push_token() {
            Ok(push_token) => push_token,
            Err(error) => {
                error!(%error, "Invalid push token state; dropping");
                push_token_state::clear_pending(self.db.write().await?).await?;
                return Err(OutboundServiceRunError::Fatal(error));
            }
        };

        match self.update_push_token_on_qs(push_token).await {
            Ok(()) => {
                push_token_state::clear_pending(self.db.write().await?).await?;
            }
            Err(OutboundServiceRunError::Fatal(error)) => {
                error!(%error, "Failed to update push token; dropping");
                push_token_state::clear_pending(self.db.write().await?).await?;
                return Err(OutboundServiceRunError::Fatal(error));
            }
            // Keep the update pending, it is due again in the next run
            Err(
                error @ (OutboundServiceRunError::NetworkError
                | OutboundServiceRunError::RateLimited { .. }),
            ) => return Err(error),
            Err(error) => {
                error!(%error, "Failed to update push token; will retry later");
                let retry_at = next_retry_at(now);
                push_token_state::schedule_retry(self.db.write().await?, retry_at).await?;
            }
        }
        Ok(())
    }

    /// Encrypts and sends the push token update to QS, classifying failures.
    async fn update_push_token_on_qs(
        &self,
        push_token: Option<PushToken>,
    ) -> Result<(), OutboundServiceRunError> {
        match &push_token {
            Some(_) => debug!("Updating push token on QS"),
            None => debug!("Clearing push token on QS"),
        }

        let queue_encryption_key = self.key_store.qs_queue_decryption_key.encryption_key();
        let signing_key = self.key_store.qs_client_signing_key.clone();

        let encrypted_push_token = match push_token {
            Some(push_token) => Some(
                push_token
                    .encrypt(&self.key_store.push_token_ear_key)
                    .map_err(OutboundServiceRunError::fatal)?,
            ),
            None => None,
        };

        self.api_clients
            .default_client()
            .map_err(OutboundServiceRunError::fatal)?
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

/// Returns the next retry time, capped to the max pending window.
fn next_retry_at(now: TimeStamp) -> TimeStamp {
    TimeStamp::from(
        *now.as_ref() + Duration::seconds(push_token_state::PUSH_TOKEN_PENDING_MAX_FUTURE_SECS),
    )
}
