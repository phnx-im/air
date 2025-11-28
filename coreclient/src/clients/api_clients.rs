// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{HashMap, hash_map::Entry},
    sync::Mutex,
};

use aircommon::identifiers::Fqdn;

use super::*;

#[derive(Debug, Clone)]
pub(crate) struct ApiClients {
    // We store our own domain such that we can manually map our own domain to
    // an API client that uses an IP address instead of the actual domain. This
    // is a temporary workaround and should probably be replaced by a more
    // thought-out mechanism.
    own_domain: Fqdn,
    own_endpoint: String,
    clients: Arc<Mutex<HashMap<String, ApiClient>>>,
}

impl ApiClients {
    pub(super) fn new(own_domain: Fqdn, own_endpoint: impl ToString) -> Self {
        Self {
            own_domain,
            own_endpoint: own_endpoint.to_string(),
            clients: Default::default(),
        }
    }

    pub(crate) fn get(&self, domain: &Fqdn) -> Result<ApiClient, ApiClientsError> {
        let domain = if domain == &self.own_domain {
            self.own_endpoint.clone()
        } else {
            domain.to_string()
        };
        let mut clients = self.clients.lock().unwrap();
        let client = match clients.entry(domain) {
            Entry::Occupied(entry) => entry.get().clone(),
            Entry::Vacant(entry) => {
                let client = ApiClient::new(entry.key())?;
                entry.insert(client).clone()
            }
        };
        Ok(client)
    }

    pub(crate) fn default_client(&self) -> Result<ApiClient, ApiClientsError> {
        let own_domain = self.own_domain.clone();
        self.get(&own_domain)
    }
}

#[derive(Debug, Error)]
pub(crate) enum ApiClientsError {
    #[error(transparent)]
    ApiClientError(#[from] ApiClientInitError),
}
