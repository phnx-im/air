// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use aircommon::registration::ChallengeKind;
use chrono::{DateTime, Duration, Utc};
use semver::Version;
use serde::Deserialize;
use zeroize::Zeroize;

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
    #[serde(default)]
    pub ratelimits: RateLimitsSettings,
    #[serde(default)]
    pub registration: RegistrationSettings,
    #[serde(default)]
    pub relay: RelaySettings,
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
    /// List of version expirations
    ///
    /// 1. Version older than some entry -> the earliest matching expires_on governs: past means
    ///    blocked, future means allowed-with-warning.
    /// 2. Otherwise -> allowed, unconditionally. This includes versions newer than any entry.
    #[serde(default)]
    pub version_expirations: Vec<VersionExpiration>,
    /// Special invitation code that is never redeemed.
    ///
    /// This code can be used to register as many users as desired. Useful for testing.
    pub unredeemablecode: Option<String>,
}

/// A version expiration entry
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub struct VersionExpiration {
    /// The version that is no longer allowed.
    pub older_than: Version,
    /// When any version < `older_than` is not allowed anymore.
    ///
    /// Accepts a date `YYYY-MM-DD` (midnight UTC) or an RFC 3339 timestamp.
    #[serde(with = "date_or_datetime")]
    pub expires_on: DateTime<Utc>,
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
    pub path: PathBuf,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ApnsSettings {
    pub keyid: String,
    pub teamid: String,
    pub privatekeypath: PathBuf,
    /// The app's bundle id.
    pub topic: Option<String>,
    /// Production or sandbox.
    pub endpoint: Option<String>,
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
    pub secret_access_key: SecretAccessKey,
    /// Bucket name for the storage provider
    #[serde(default = "default_bucket")]
    pub bucket: String,
    #[serde(default = "default_debug_logs_bucket")]
    pub debug_logs_bucket: String,
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
    /// Expiration for signed debug logs download URLs
    ///
    /// Default is the maximum allowed time by the AWS API (1 week).
    #[serde(default = "default_1week", with = "duration_seconds")]
    pub download_debug_logs_expiration: Duration,
    /// Maximum size of an attachment in bytes
    ///
    /// Default is 20 MiB.
    #[serde(default = "default_20mib")]
    pub max_attachment_size: u64,
    /// Enables attachment provisioning for uploads via POST policy
    #[serde(default)]
    pub use_post_policy: bool,
    /// Requires content length to be present when provisioning an attachment
    #[serde(default = "default_require_content_length")]
    pub require_content_length: bool,
    /// Path prefixes in the bucket for different storage object types
    #[serde(default)]
    pub storage_paths: StoragePaths,
}

#[derive(Debug, Deserialize, Clone, Zeroize)]
pub struct SecretAccessKey(String);

impl AsRef<str> for SecretAccessKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<SecretAccessKey> for String {
    fn from(secret_access_key: SecretAccessKey) -> Self {
        secret_access_key.0
    }
}

impl From<String> for SecretAccessKey {
    fn from(secret_access_key: String) -> Self {
        Self(secret_access_key)
    }
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

/// Every `period`, allow bursts of up to `burst`-many requests, and replenish one element after
/// the `period`.
#[derive(Debug, Deserialize, Clone)]
pub struct RateLimitsSettings {
    #[serde(with = "duration_millis", default = "default_500ms")]
    pub period: std::time::Duration,
    #[serde(default = "default_burst")]
    pub burst: u32,
}

impl Default for RateLimitsSettings {
    fn default() -> Self {
        Self {
            period: std::time::Duration::from_millis(500),
            burst: 100,
        }
    }
}

/// Lifetime and reuse policy of a device-linking rendezvous session.
///
/// Field names carry no underscores, because environment overrides split on
/// them (`AIR_RELAY_SESSIONTTL`).
#[derive(Debug, Deserialize, Clone)]
pub struct RelaySettings {
    /// How long a session lives after its rendezvous ID is assigned.
    #[serde(with = "duration_millis", default = "default_session_ttl")]
    pub sessionttl: std::time::Duration,
    /// How long an ended session's rendezvous ID is held back from reuse. A
    /// user typing a stale code must not consume an unrelated fresh session,
    /// so this is never shorter than the session lifetime in practice.
    #[serde(with = "duration_millis", default = "default_id_quarantine")]
    pub idquarantine: std::time::Duration,
}

impl Default for RelaySettings {
    fn default() -> Self {
        Self {
            sessionttl: default_session_ttl(),
            idquarantine: default_id_quarantine(),
        }
    }
}

fn default_session_ttl() -> std::time::Duration {
    std::time::Duration::from_secs(10 * 60)
}

fn default_id_quarantine() -> std::time::Duration {
    std::time::Duration::from_secs(10 * 60)
}

/// How registration is gated.
///
/// Field names carry no underscores, because environment overrides split on
/// them (`AIR_REGISTRATION_POLICY`).
#[derive(Debug, Deserialize, Clone)]
pub struct RegistrationSettings {
    #[serde(default)]
    pub policy: RegistrationPolicy,
    /// Challenge types a gated registration may answer with, in the order the
    /// server verifies them.
    #[serde(default = "default_challenges")]
    pub challenges: Vec<ChallengeKind>,
    /// Attempts at registration one client address bucket may make before the
    /// gate closes for it. Any window closes the gate, and an empty list never
    /// does.
    #[serde(default = "default_perip_thresholds")]
    pub perip: Vec<RegistrationThreshold>,
    /// Challenge-free registrations the deployment may complete before the gate
    /// closes for everyone. Any window closes the gate, and an empty list never
    /// does.
    #[serde(default = "default_total_thresholds")]
    pub total: Vec<RegistrationThreshold>,
    /// Terms of the push-admission challenge, which is only offered while
    /// `admissionsession` is among the challenges.
    #[serde(default)]
    pub admission: AdmissionSettings,
}

impl Default for RegistrationSettings {
    fn default() -> Self {
        Self {
            policy: RegistrationPolicy::default(),
            challenges: default_challenges(),
            perip: default_perip_thresholds(),
            total: default_total_thresholds(),
            admission: AdmissionSettings::default(),
        }
    }
}

impl RegistrationSettings {
    pub fn offers_admission_sessions(&self) -> bool {
        self.challenges.contains(&ChallengeKind::AdmissionSession)
    }
}

/// Terms of the push-admission challenge.
#[derive(Debug, Deserialize, Clone)]
pub struct AdmissionSettings {
    /// How long a session stays open for its challenge to arrive and be spent.
    #[serde(default = "default_session_lifetime", with = "duration_seconds")]
    pub sessionlifetime: Duration,
    /// Challenges one endpoint may have sent to it.
    #[serde(default = "default_send_throttle")]
    pub sendthrottle: RegistrationThreshold,
    /// Registrations one endpoint admits. Every window applies.
    #[serde(default = "default_endpoint_quotas")]
    pub quotas: Vec<RegistrationThreshold>,
}

impl Default for AdmissionSettings {
    fn default() -> Self {
        Self {
            sessionlifetime: default_session_lifetime(),
            sendthrottle: default_send_throttle(),
            quotas: default_endpoint_quotas(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RegistrationPolicy {
    /// Registration never needs a challenge.
    Open,
    /// Registration needs a challenge once any threshold is reached.
    Adaptive,
    /// Registration always needs a challenge.
    #[default]
    Required,
}

/// A limit and the rolling window it applies to.
#[derive(Debug, Deserialize, Clone, Copy)]
pub struct RegistrationThreshold {
    pub limit: u64,
    /// Required, since the two dimensions count over windows orders of
    /// magnitude apart.
    #[serde(with = "duration_seconds")]
    pub window: Duration,
}

fn default_challenges() -> Vec<ChallengeKind> {
    vec![ChallengeKind::InvitationCode]
}

fn default_perip_thresholds() -> Vec<RegistrationThreshold> {
    vec![
        RegistrationThreshold {
            limit: 3,
            window: Duration::seconds(10),
        },
        RegistrationThreshold {
            limit: 10,
            window: Duration::hours(1),
        },
        RegistrationThreshold {
            limit: 20,
            window: Duration::days(1),
        },
    ]
}

fn default_total_thresholds() -> Vec<RegistrationThreshold> {
    vec![
        RegistrationThreshold {
            limit: 100,
            window: Duration::seconds(60),
        },
        RegistrationThreshold {
            limit: 300,
            window: Duration::hours(1),
        },
        RegistrationThreshold {
            limit: 1000,
            window: Duration::days(1),
        },
    ]
}

fn default_session_lifetime() -> Duration {
    Duration::minutes(5)
}

fn default_send_throttle() -> RegistrationThreshold {
    RegistrationThreshold {
        limit: 10,
        window: Duration::hours(1),
    }
}

/// An honest signup needs one account, so the day window is what an attacker
/// runs into and the month window caps what token rotation buys.
fn default_endpoint_quotas() -> Vec<RegistrationThreshold> {
    vec![
        RegistrationThreshold {
            limit: 5,
            window: Duration::days(1),
        },
        RegistrationThreshold {
            limit: 10,
            window: Duration::days(7),
        },
    ]
}

#[derive(Debug, Deserialize, Clone)]
pub struct StoragePaths {
    /// Path prefix in the bucket for attachments
    pub attachments_path: String,
    /// Path prefix in the bucket for group profiles
    pub group_profiles_path: String,
    /// Path prefix in the bucket for user profiles
    pub user_profiles_path: String,
}

impl Default for StoragePaths {
    fn default() -> Self {
        Self {
            attachments_path: "attachments".to_owned(),
            group_profiles_path: "group-profiles".to_owned(),
            user_profiles_path: "user-profiles".to_owned(),
        }
    }
}

fn default_5min() -> Duration {
    Duration::seconds(5 * 60)
}

fn default_1week() -> Duration {
    Duration::days(7)
}

fn default_500ms() -> std::time::Duration {
    std::time::Duration::from_millis(500)
}

fn default_20mib() -> u64 {
    20 * 1024 * 1024
}

fn default_burst() -> u32 {
    100
}

fn default_bucket() -> String {
    "data".to_string()
}

fn default_debug_logs_bucket() -> String {
    "debug-logs".to_string()
}

mod date_or_datetime {
    use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
    use serde::de;

    /// Accepts a date `YYYY-MM-DD` (midnight UTC) or an RFC 3339 timestamp.
    pub fn deserialize<'de, D>(d: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s: String = serde::Deserialize::deserialize(d)?;
        if let Ok(date) = s.parse::<NaiveDate>() {
            return Ok(date.and_time(NaiveTime::MIN).and_utc());
        }
        DateTime::parse_from_rfc3339(&s)
            .map(|dt| dt.to_utc())
            .map_err(de::Error::custom)
    }
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

mod duration_millis {
    use serde::de;

    use std::time::Duration;

    pub fn deserialize<'de, D>(d: D) -> Result<Duration, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let millis: u64 = serde::Deserialize::deserialize(d)?;
        Ok(Duration::from_millis(millis))
    }
}

fn default_require_content_length() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn version_expiration_accepts_date_and_timestamp() {
        let expiration: VersionExpiration = serde_json::from_value(json!({
            "older_than": "0.20.0",
            "expires_on": "2026-09-02",
        }))
        .unwrap();
        assert_eq!(
            expiration.expires_on,
            "2026-09-02T00:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );

        let expiration: VersionExpiration = serde_json::from_value(json!({
            "older_than": "0.20.0",
            "expires_on": "2026-09-02T15:30:00Z",
        }))
        .unwrap();
        assert_eq!(
            expiration.expires_on,
            "2026-09-02T15:30:00Z".parse::<DateTime<Utc>>().unwrap()
        );

        let result = serde_json::from_value::<VersionExpiration>(json!({
            "older_than": "0.20.0",
            "expires_on": "not a date",
        }));
        assert!(result.is_err());
    }
}
