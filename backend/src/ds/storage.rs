// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use aws_config::Region;
use aws_sdk_s3::{Client, Config, config::Credentials};

use crate::settings::StorageSettings;

#[derive(Debug, Clone)]
pub struct Storage {
    client: Client,
    settings: Arc<StorageSettings>,
}

impl Storage {
    pub fn new(settings: StorageSettings) -> Self {
        let credentials = Credentials::new(
            settings.access_key_id.clone(),
            settings.secret_access_key.clone(),
            None,
            None,
            "storage",
        );
        let config = Config::builder()
            .endpoint_url(settings.endpoint.clone())
            .region(Region::new(settings.region.clone()))
            .credentials_provider(credentials)
            .force_path_style(settings.force_path_style)
            .behavior_version_latest()
            .build();

        let client = Client::from_conf(config);

        Self {
            client,
            settings: Arc::new(settings),
        }
    }

    pub(crate) fn client(&self) -> Client {
        self.client.clone()
    }

    pub(crate) fn settings(&self) -> &StorageSettings {
        &self.settings
    }
}
