// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::qs_api::QsRequestError;
use aircommon::{
    crypto::ear::EarEncryptable,
    messages::push_token::PushToken,
    time::{Duration, TimeStamp},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

use crate::{clients::push_token_state, outbound_service::error::OutboundServiceError};

use super::OutboundServiceContext;

impl OutboundServiceContext {
    /// Processes a single due push token update with clamped retry timestamps.
    pub(super) async fn send_pending_push_token_updates(
        &self,
        run_token: &CancellationToken,
    ) -> anyhow::Result<()> {
        if run_token.is_cancelled() {
            return Ok(());
        }

        let now = TimeStamp::now();
        push_token_state::clamp_pending_future(&self.pool, now).await?;
        let Some(state) = push_token_state::load_pending(&self.pool, now).await? else {
            return Ok(());
        };

        let push_token = match state.to_push_token() {
            Ok(push_token) => push_token,
            Err(error) => {
                error!(%error, "Invalid push token state; dropping");
                push_token_state::clear_pending(&self.pool).await?;
                return Err(error);
            }
        };

        match self.update_push_token_on_qs(push_token).await {
            Ok(()) => {
                push_token_state::clear_pending(&self.pool).await?;
            }
            Err(OutboundServiceError::Fatal(error)) => {
                error!(%error, "Failed to update push token; dropping");
                push_token_state::clear_pending(&self.pool).await?;
                return Err(error);
            }
            Err(OutboundServiceError::Recoverable(error)) => {
                error!(%error, "Failed to update push token; will retry later");
                let retry_at = next_retry_at(now);
                push_token_state::schedule_retry(&self.pool, retry_at).await?;
            }
        }
        Ok(())
    }

    /// Encrypts and sends the push token update to QS, classifying failures.
    async fn update_push_token_on_qs(
        &self,
        push_token: Option<PushToken>,
    ) -> Result<(), OutboundServiceError> {
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
                    .map_err(OutboundServiceError::fatal)?,
            ),
            None => None,
        };

        self.api_clients
            .default_client()
            .map_err(OutboundServiceError::fatal)?
            .qs_update_client(
                self.qs_client_id,
                queue_encryption_key.clone(),
                encrypted_push_token,
                &signing_key,
            )
            .await
            .map_err(classify_qs_error)?;
        Ok(())
    }
}

/// Returns the next retry time, capped to the max pending window.
fn next_retry_at(now: TimeStamp) -> TimeStamp {
    TimeStamp::from(
        *now.as_ref() + Duration::seconds(push_token_state::PUSH_TOKEN_PENDING_MAX_FUTURE_SECS),
    )
}

/// Treats protocol/validation errors as fatal and transport errors as recoverable.
fn classify_qs_error(error: QsRequestError) -> OutboundServiceError {
    if error.is_unsupported_version() {
        return OutboundServiceError::fatal(error);
    }

    match error {
        QsRequestError::MissingField(_)
        | QsRequestError::UnexpectedResponse
        | QsRequestError::Tls(_) => OutboundServiceError::fatal(error),
        QsRequestError::Tonic(_) => OutboundServiceError::recoverable(error),
    }
}
