// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use phnxbackend::qs::{network_provider_trait::NetworkProvider, qs_api::FederatedProcessingResult};
use phnxtypes::{
    DEFAULT_PORT_HTTP, DEFAULT_PORT_HTTPS, endpoint_paths::ENDPOINT_QS_FEDERATION,
    identifiers::Fqdn,
};
use reqwest::Client;
use thiserror::Error;
use tls_codec::DeserializeBytes;

#[derive(Debug, Error, Clone)]
pub enum MockNetworkError {
    /// Malformed response
    #[error("Malformed response")]
    MalformedResponse,
}

#[derive(Debug, Clone)]
pub enum TransportEncryption {
    On,
    Off,
}

#[derive(Debug, Clone)]
pub struct MockNetworkProvider {
    client: Client,
    transport_encryption: TransportEncryption,
}

impl Default for MockNetworkProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockNetworkProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            transport_encryption: TransportEncryption::Off,
        }
    }
}

#[async_trait]
impl NetworkProvider for MockNetworkProvider {
    type NetworkError = MockNetworkError;

    async fn deliver(
        &self,
        bytes: Vec<u8>,
        destination: Fqdn,
    ) -> Result<FederatedProcessingResult, Self::NetworkError> {
        let (transport_encryption, port) = match self.transport_encryption {
            TransportEncryption::On => ("s", DEFAULT_PORT_HTTPS),
            TransportEncryption::Off => ("", DEFAULT_PORT_HTTP),
        };
        let url = format!(
            "http{}://{}:{}{}",
            transport_encryption, destination, port, ENDPOINT_QS_FEDERATION
        );
        // Reqwest should resolve the hostname on its own.
        let result = match self.client.post(url).body(bytes).send().await {
            // For now we don't care about the response.
            Ok(response_bytes) => FederatedProcessingResult::tls_deserialize_exact_bytes(
                &response_bytes.bytes().await.unwrap(),
            )
            .map_err(|_| MockNetworkError::MalformedResponse)?,
            // TODO: We only care about the happy path for now.
            Err(e) => panic!("Error: {}", e),
        };
        Ok(result)
    }
}
