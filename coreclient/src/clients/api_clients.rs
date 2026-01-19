// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{HashMap, hash_map::Entry},
    sync::{Arc, Mutex},
};

use airapiclient::{ApiClient, ApiClientInitError};
use aircommon::identifiers::Fqdn;
use url::Url;

#[derive(Debug, Clone)]
pub(crate) struct ApiClients {
    own_domain: Fqdn,
    /// Override the endpoint for the own domain.
    own_endpoint: Option<Url>,
    clients: Arc<Mutex<HashMap<Fqdn, ApiClient>>>,
}

impl ApiClients {
    pub(super) fn new(own_domain: Fqdn, own_endpoint: Option<Url>) -> Self {
        Self {
            own_domain,
            own_endpoint,
            clients: Default::default(),
        }
    }

    pub(crate) fn get(&self, domain: &Fqdn) -> Result<ApiClient, ApiClientInitError> {
        let mut clients = self.clients.lock().unwrap();
        match clients.entry(domain.clone()) {
            Entry::Occupied(entry) => Ok(entry.get().clone()),
            Entry::Vacant(entry) => {
                let client = if let Some(endpoint) = self.own_endpoint.as_ref()
                    && domain == &self.own_domain
                {
                    ApiClient::with_endpoint(endpoint)?
                } else {
                    ApiClient::with_domain(domain)?
                };
                Ok(entry.insert(client).clone())
            }
        }
    }

    pub(crate) fn default_client(&self) -> Result<ApiClient, ApiClientInitError> {
        self.get(&self.own_domain)
    }
}
