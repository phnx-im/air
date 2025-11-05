// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use chrono::Duration;
use serde::Deserialize;

/// Configuration for the server.
#[derive(Deserialize, Clone, Debug)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub application: ApplicationSettings,
    /// If this isn't present, the provider will not send push notifications to
    /// apple devices.
    pub apns: Option<ApnsSettings>,
    /// If this isn't present, the provider will not send push notifications to
    /// android devices.
    pub fcm: Option<FcmSettings>,
    /// If this isn't present, the support for attachments is disabled.
    pub storage: Option<StorageSettings>,
}

/// Configuration for the application.
#[derive(Deserialize, Clone, Debug)]
pub struct ApplicationSettings {
    /// The address to listen for incoming requests
    #[serde(default = "default_listen")]
    pub listen: SocketAddr,
    /// The address to serve metrics on
    ///
    /// Note: This is not the same address as the address for the incoming request, because the
    /// metrics *must not* be exposed to the outside world.
    #[serde(default = "default_listen_metrics")]
    pub listen_metrics: SocketAddr,
    /// The domain of the users on this server
    ///
    /// Users on this server will have ids of the form `<id>@<domain>`.
    ///
    /// Can *not* be changed after the first start of the server.
    pub domain: String,
}

fn default_listen() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080)
}

fn default_listen_metrics() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9090)
}

/// Configuration for the database.
#[derive(Deserialize, Clone, Debug)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: String,
    pub port: u16,
    pub host: String,
    pub name: String,
    pub cacertpath: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FcmSettings {
    // The path to the service account key file.
    pub path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ApnsSettings {
    pub keyid: String,
    pub teamid: String,
    pub privatekeypath: String,
}

/// Settings for an external object storage provider
#[derive(Debug, Deserialize, Clone)]
pub struct StorageSettings {
    /// Endpoint for the storage provider
    pub endpoint: String,
    /// Region for the storage provider
    pub region: String,
    /// Access key ID for the storage provider
    pub access_key_id: String,
    /// Secret access key for the storage provider
    pub secret_access_key: String,
    /// Force path style for the storage provider
    #[serde(default)]
    pub force_path_style: bool,
    /// Expiration for signed upload URLs
    ///
    /// Default is 5 minutes.
    #[serde(default = "default_5min", with = "duration_seconds")]
    pub upload_expiration: Duration,
    /// Expiration for signed download URLs
    ///
    /// Default is 5 minutes.
    #[serde(default = "default_5min", with = "duration_seconds")]
    pub download_expiration: Duration,
}

impl DatabaseSettings {
    /// Add the TLS mode to the connection string if the CA certificate path is
    /// set.
    fn add_tls_mode(&self, mut connection_string: String) -> String {
        if let Some(ref ca_cert_path) = self.cacertpath {
            connection_string.push_str(&format!("?sslmode=verify-ca&sslrootcert={ca_cert_path}"));
        } else {
            tracing::warn!(
                "No CA certificate path set for database connection. TLS will not be enabled."
            );
        }
        connection_string
    }

    /// Compose the base connection string without the database name.
    fn base_connection_string(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}",
            self.username, self.password, self.host, self.port
        )
    }

    /// Get the connection string for the database.
    pub fn connection_string(&self) -> String {
        let mut connection_string = self.base_connection_string();
        connection_string.push('/');
        connection_string.push_str(&self.name);
        self.add_tls_mode(connection_string)
    }

    /// Get the connection string for the database without the database name.
    /// Enables TLS by default.
    pub fn connection_string_without_database(&self) -> String {
        let connection_string = self.base_connection_string();
        self.add_tls_mode(connection_string)
    }
}

fn default_5min() -> Duration {
    Duration::seconds(5 * 60)
}

mod duration_seconds {
    use serde::de;

    use chrono::Duration;

    pub fn deserialize<'de, D>(d: D) -> Result<Duration, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let seconds: u64 = serde::Deserialize::deserialize(d)?;
        let seconds: i64 = seconds
            .try_into()
            .map_err(|_| de::Error::custom("out of range"))?;
        Ok(Duration::seconds(seconds))
    }
}
