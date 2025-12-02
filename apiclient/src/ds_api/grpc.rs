// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airprotos::delivery_service::v1::delivery_service_client::DeliveryServiceClient;
use tonic::transport::Channel;

#[derive(Debug, Clone)]
pub(crate) struct DsGrpcClient {
    client: DeliveryServiceClient<Channel>,
}

impl DsGrpcClient {
    pub(crate) fn new(client: DeliveryServiceClient<Channel>) -> Self {
        Self { client }
    }

    pub(crate) fn client(&self) -> DeliveryServiceClient<Channel> {
        self.client.clone()
    }
}
