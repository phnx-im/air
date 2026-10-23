// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server API surface: rate limits, service endpoints, the listen stream and
//! the harness fault injection.

mod fault_injection;
mod listen;
mod rate_limit;
mod service;

use std::assert_matches;

use aircoreclient::clients::{ListenResponse, listen_response};
use tokio_stream::{Stream, StreamExt};

/// Consumes the version status the server sends first on every listen stream.
async fn skip_version_status(
    stream: &mut (impl Stream<Item = Result<ListenResponse, tonic::Status>> + Unpin),
) {
    assert_matches!(
        stream.next().await,
        Some(Ok(ListenResponse {
            event: Some(listen_response::Event::VersionStatus(_)),
        }))
    );
}
