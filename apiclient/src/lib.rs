// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Client for the server gRPC API

use std::{sync::Arc, time::Duration};

use aircommon::identifiers::Fqdn;
use airprotos::{
    auth_service::v1::auth_service_client::AuthServiceClient, common::v1::ClientMetadata,
    delivery_service::v1::delivery_service_client::DeliveryServiceClient,
    queue_service::v1::queue_service_client::QueueServiceClient,
    relay_service::v1::relay_service_client::RelayServiceClient,
};
use thiserror::Error;
use tokio::time::timeout;
use tokio_stream::{Stream, StreamExt};
use tonic::{
    Status,
    transport::{Channel, ClientTlsConfig, Endpoint, Uri},
};
use tracing::{info, warn};
use url::{Host, Url};

pub mod as_api;
pub mod ds_api;
mod metadata;
pub mod qs_api;
pub mod rs_api;

/// Waits for the server to close the response stream after the request stream was half-closed.
///
/// The server closes the response stream with OK only after it has processed all requests sent
/// before the half-close. A non-OK status or a timeout means that the last requests may not have
/// been processed.
pub(crate) async fn await_close_confirmation<T>(
    stream: &mut (impl Stream<Item = Result<T, Status>> + Unpin),
) {
    const CLOSE_CONFIRMATION_TIMEOUT: Duration = Duration::from_secs(5);
    loop {
        match timeout(CLOSE_CONFIRMATION_TIMEOUT, stream.next()).await {
            // Late response => ignored, the server owes us nothing here
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(error))) => {
                warn!(%error, "listen stream did not close with OK");
                break;
            }
            // OK trailers => all sent requests were processed
            Ok(None) => break,
            Err(_elapsed) => {
                warn!("timeout waiting for listen stream to close");
                break;
            }
        }
    }
}

/// The port used for localhost connections.
///
/// Also see server's listen configuration.
const LOCALHOST_PORT: u16 = 8080;

/// Errors that can occur when creating an API client.
#[derive(Error, Debug)]
pub enum ApiClientInitError {
    #[error("Failed to parse URL {0}")]
    UrlParsingError(String),
    #[error("Invalid URL {0}")]
    InvalidUrl(String),
    #[error(transparent)]
    TonicTransport(#[from] tonic::transport::Error),
}

/// ApiClient is a thin wrapper around the gRPC clients.
///
/// It exposes a single function for each API endpoint. Internally, it holds a single TCP
/// connection to the server.
#[derive(Debug, Clone)]
pub struct ApiClient {
    inner: Arc<ApiClientInner>,
}

#[derive(Debug)]
struct ApiClientInner {
    as_grpc_client: AuthServiceClient<Channel>,
    qs_grpc_client: QueueServiceClient<Channel>,
    ds_grpc_client: DeliveryServiceClient<Channel>,
    rs_grpc_client: RelayServiceClient<Channel>,
}

impl ApiClient {
    pub fn with_endpoint(url: &Url) -> Result<Self, ApiClientInitError> {
        info!(%url, "Connecting lazily to GRPC server");
        let uri: Uri = url
            .as_str()
            .parse()
            .map_err(|_| ApiClientInitError::InvalidUrl(url.to_string()))?;
        let channel = Endpoint::from(uri)
            .tls_config(ClientTlsConfig::new().with_webpki_roots())?
            .http2_keep_alive_interval(Duration::from_secs(30))
            .connect_lazy();
        let as_grpc_client = AuthServiceClient::new(channel.clone());
        let ds_grpc_client = DeliveryServiceClient::new(channel.clone());
        let qs_grpc_client = QueueServiceClient::new(channel.clone());
        let rs_grpc_client = RelayServiceClient::new(channel);

        Ok(Self {
            inner: Arc::new(ApiClientInner {
                as_grpc_client,
                qs_grpc_client,
                ds_grpc_client,
                rs_grpc_client,
            }),
        })
    }

    pub fn with_domain(domain: &Fqdn) -> Result<Self, ApiClientInitError> {
        let domain_str = if domain.is_localhost() {
            format!("http://localhost:{LOCALHOST_PORT}")
        } else if domain == &Fqdn::from(Host::Domain("air.ms".to_string())) {
            // Rewrite the domain to the production endpoint.
            //
            // At somepoint, we will discover the production endpoint e.g. via DNS or a .well-known
            // endpoint.
            "https://prod.air.ms".to_string()
        } else {
            format!("https://{domain}")
        };
        let url: Url = domain_str
            .parse()
            .map_err(|_| ApiClientInitError::InvalidUrl(domain_str))?;
        Self::with_endpoint(&url)
    }

    pub(crate) fn as_grpc_client(&self) -> AuthServiceClient<Channel> {
        self.inner.as_grpc_client.clone()
    }

    pub(crate) fn qs_grpc_client(&self) -> QueueServiceClient<Channel> {
        self.inner.qs_grpc_client.clone()
    }

    pub(crate) fn ds_grpc_client(&self) -> DeliveryServiceClient<Channel> {
        self.inner.ds_grpc_client.clone()
    }

    pub(crate) fn rs_grpc_client(&self) -> RelayServiceClient<Channel> {
        self.inner.rs_grpc_client.clone()
    }

    pub(crate) fn metadata(&self) -> &ClientMetadata {
        &metadata::METADATA
    }
}
