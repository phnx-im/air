// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use airbackend::{
    air_service::BackendService,
    auth_service::AuthService,
    ds::{Ds, storage::Storage},
    qs::Qs,
};
use aircommon::identifiers::Fqdn;
use airserver::{
    RateLimitsConfig, ServerRunParams, configurations::*, enqueue_provider::SimpleEnqueueProvider,
    logging::init_logging, network_provider::MockNetworkProvider,
    push_notification_provider::ProductionPushNotificationProvider, run,
};
use tokio::net::TcpListener;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();

    // Load configuration
    let mut configuration = get_configuration("server/").expect("Could not load configuration.");

    if configuration.application.domain.is_empty() {
        panic!("No domain name configured.");
    }

    // Port binding
    let listener = TcpListener::bind(configuration.application.listen)
        .await
        .expect("Failed to bind");
    let metrics_listener = TcpListener::bind(configuration.application.listen_metrics)
        .await
        .expect("Failed to bind");

    let domain: Fqdn = configuration
        .application
        .domain
        .parse()
        .expect("Invalid domain");
    info!(%domain, "Starting server");
    let network_provider = MockNetworkProvider::new();

    let base_db_name = configuration.database.name.clone();
    // DS storage provider
    configuration.database.name = format!("{base_db_name}_ds");
    info!(
        host = configuration.database.host,
        "Connecting to postgres server",
    );
    let mut counter = 0;
    let mut ds_result = Ds::new(&configuration.database, domain.clone()).await;

    // Try again for 10 times each second in case the postgres server is coming up.
    while let Err(e) = ds_result {
        info!("Failed to connect to postgres server: {}", e);
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        counter += 1;
        if counter > 10 {
            panic!("Database not ready after 10 seconds.");
        }
        ds_result = Ds::new(&configuration.database, domain.clone()).await;
    }
    let mut ds = ds_result.unwrap();
    if let Some(storage_settings) = &configuration.storage {
        let storage = Storage::new(storage_settings.clone());
        ds.set_storage(storage);
    }

    // New database name for the QS provider
    configuration.database.name = format!("{base_db_name}_qs");
    // QS storage provider
    let qs = Qs::new(&configuration.database, domain.clone())
        .await
        .expect("Failed to connect to database.");

    // New database name for the AS provider
    configuration.database.name = format!("{base_db_name}_as");
    let auth_service = AuthService::new(&configuration.database, domain.clone())
        .await
        .expect("Failed to connect to database.");

    let push_notification_provider =
        ProductionPushNotificationProvider::new(configuration.fcm, configuration.apns)?;
    let qs_connector = SimpleEnqueueProvider {
        qs: qs.clone(),
        push_notification_provider,
        network: network_provider.clone(),
    };

    // Start the server
    let server = run(ServerRunParams {
        listener,
        metrics_listener: Some(metrics_listener),
        ds,
        auth_service,
        qs,
        qs_connector,
        rate_limits: RateLimitsConfig {
            period: Duration::from_millis(500),
            burst_size: 100,
        },
    })
    .await;

    server.await?;
    Ok(())
}
