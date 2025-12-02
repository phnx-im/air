// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! HTTP client for the server REST API

use std::{sync::Arc, time::Duration};

use airprotos::{
    auth_service::v1::auth_service_client::AuthServiceClient,
    common::v1::{Channel, ClientMetadata, Platform},
    delivery_service::v1::delivery_service_client::DeliveryServiceClient,
    queue_service::v1::queue_service_client::QueueServiceClient,
};
use as_api::grpc::AsGrpcClient;
use ds_api::grpc::DsGrpcClient;
use qs_api::grpc::QsGrpcClient;
use thiserror::Error;
use tonic::transport::ClientTlsConfig;
use tracing::info;
use url::{ParseError, Url};

pub mod as_api;
pub mod ds_api;
pub mod qs_api;
pub(crate) mod util;

// TODO: Turn this on once we have the necessary test infrastructure for
// certificates in place.
const HTTPS_BY_DEFAULT: bool = false;

#[derive(Error, Debug)]
pub enum ApiClientInitError {
    #[error("Failed to parse URL {0}")]
    UrlParsingError(String),
    #[error("Invalid URL {0}")]
    InvalidUrl(String),
    #[error(transparent)]
    TonicTransport(#[from] tonic::transport::Error),
}

// ApiClient is a wrapper around a reqwest client.
// It exposes a single function for each API endpoint.
#[derive(Debug, Clone)]
pub struct ApiClient {
    inner: Arc<ApiClientInner>,
}

#[derive(Debug)]
struct ApiClientInner {
    metadata: ClientMetadata,
    as_grpc_client: AsGrpcClient,
    qs_grpc_client: QsGrpcClient,
    ds_grpc_client: DsGrpcClient,
}

impl ApiClient {
    /// Creates a new API client that connects to the given endpoint.
    ///
    /// The endpoint can be an URL or a hostname with an optional port.
    pub fn new(endpoint: &str) -> Result<Self, ApiClientInitError> {
        // TODO
        let metadata = ClientMetadata {
            version: "0.1.0".to_owned(),
            platform: Platform::Android.into(),
            channel: Channel::Stable.into(),
        };

        let url = match Url::parse(endpoint) {
            // We first check if the domain is a valid URL.
            Ok(url) => url,
            // If not, we try to parse it as a hostname.
            Err(ParseError::RelativeUrlWithoutBase) => {
                let protocol = if HTTPS_BY_DEFAULT { "https" } else { "http" };
                let url = format!("{protocol}://{endpoint}");
                Url::parse(&url).map_err(|_| ApiClientInitError::UrlParsingError(url))?
            }
            Err(_) => return Err(ApiClientInitError::UrlParsingError(endpoint.to_owned())),
        };

        info!(%url, "Connecting lazily to GRPC server");
        let endpoint = tonic::transport::Endpoint::from_shared(url.to_string())
            .map_err(|_| ApiClientInitError::InvalidUrl(url.to_string()))?;
        let channel = endpoint
            .tls_config(ClientTlsConfig::new().with_webpki_roots())?
            .http2_keep_alive_interval(Duration::from_secs(30))
            .connect_lazy();
        let as_grpc_client = AsGrpcClient::new(AuthServiceClient::new(channel.clone()));
        let ds_grpc_client = DsGrpcClient::new(DeliveryServiceClient::new(channel.clone()));
        let qs_grpc_client = QsGrpcClient::new(QueueServiceClient::new(channel));

        Ok(Self {
            inner: Arc::new(ApiClientInner {
                metadata,
                as_grpc_client,
                qs_grpc_client,
                ds_grpc_client,
            }),
        })
    }

    pub(crate) fn as_grpc_client(&self) -> &AsGrpcClient {
        &self.inner.as_grpc_client
    }

    pub(crate) fn qs_grpc_client(&self) -> &QsGrpcClient {
        &self.inner.qs_grpc_client
    }

    pub(crate) fn ds_grpc_client(&self) -> &DsGrpcClient {
        &self.inner.ds_grpc_client
    }

    pub(crate) fn metadata(&self) -> &ClientMetadata {
        &self.inner.metadata
    }
}
