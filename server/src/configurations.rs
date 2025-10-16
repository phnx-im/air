// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airbackend::settings::Settings;
use config::{Config, ConfigError, File, Source};

/// The possible runtime environment for our application.
pub enum Environment {
    Local,
    Production,
}

impl Environment {
    pub fn from_env() -> Result<Self, String> {
        std::env::var("APP_ENVIRONMENT")
            .unwrap_or_else(|_| "local".into())
            .try_into()
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Production => "production",
        }
    }
}

impl TryFrom<String> for Environment {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "production" => Ok(Self::Production),
            other => Err(format!(
                "{other} is not a supported environment. Use either `local` or `production`."
            )),
        }
    }
}

/// Load the configuration from the configuration file.
pub fn get_configuration(prefix: &str) -> Result<Settings, ConfigError> {
    // Directories
    let base_path = std::env::current_dir().expect("Failed to determine the current directory");
    let configuration_directory = base_path.join(format!("{prefix}configuration"));

    // Detect the running environment.
    // Default to `local` if unspecified.
    let environment = Environment::from_env().map_err(ConfigError::Message)?;

    get_configuration_impl(
        File::from(configuration_directory.join("base")).required(true),
        File::from(configuration_directory.join(environment.as_str())).required(true),
    )
}

/// Load the configuration from the given configuration strings (in YAML format).
pub fn get_configuration_from_str(base: &str, environment: &str) -> Result<Settings, ConfigError> {
    get_configuration_impl(
        File::from_str(base, config::FileFormat::Yaml),
        File::from_str(environment, config::FileFormat::Yaml),
    )
}

fn get_configuration_impl(
    base: impl Source + Send + Sync + 'static,
    environment: impl Source + Send + Sync + 'static,
) -> Result<Settings, ConfigError> {
    let builder = Config::builder()
        // Read the "default" configuration file
        .add_source(base)
        // Layer on the environment-specific values.
        .add_source(environment)
        // Add in settings from environment variables (with a prefix of APP and '_' as separator)
        // E.g. `AIR_APPLICATION_PORT=5001 would set `Settings.application.port`
        .add_source(config::Environment::with_prefix("AIR").separator("_"));
    builder.build()?.try_deserialize()
}
