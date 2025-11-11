// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use aws_config::Region;
use aws_sdk_s3::{Client, Config, config::Credentials};
use chrono::Duration;

use crate::settings::{SecretAccessKey, StorageSettings};

#[derive(Debug, Clone)]
pub struct Storage {
    client: Client,
    attributes: Arc<StorageAttributes>,
}

#[derive(Debug)]
pub(crate) struct StorageAttributes {
    pub(crate) endpoint_url: String,
    pub(crate) access_key_id: String,
    pub(crate) secret_access_key: SecretAccessKey,
    pub(crate) region: String,
    pub(crate) upload_expiration: Duration,
    pub(crate) download_expiration: Duration,
    pub(crate) max_attachment_size: u64,
}

impl Storage {
    pub fn new(settings: StorageSettings) -> Self {
        let attributes = Arc::new(StorageAttributes {
            endpoint_url: settings.endpoint.clone(),
            access_key_id: settings.access_key_id.clone(),
            secret_access_key: settings.secret_access_key.clone(),
            region: settings.region.clone(),
            upload_expiration: settings.upload_expiration,
            download_expiration: settings.download_expiration,
            max_attachment_size: settings.max_attachment_size,
        });

        let credentials = Credentials::new(
            settings.access_key_id,
            settings.secret_access_key,
            None,
            None,
            "storage",
        );
        let config = Config::builder()
            .endpoint_url(settings.endpoint)
            .region(Region::new(settings.region))
            .credentials_provider(credentials)
            .force_path_style(settings.force_path_style)
            .behavior_version_latest()
            .build();

        let client = Client::from_conf(config.clone());

        Self { client, attributes }
    }

    pub(crate) fn client(&self) -> Client {
        self.client.clone()
    }

    pub(crate) fn attributes(&self) -> &StorageAttributes {
        &self.attributes
    }
}
