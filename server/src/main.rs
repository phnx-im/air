// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airbackend::{
    air_service::BackendService,
    auth_service::AuthService,
    ds::{Ds, storage::Storage},
    qs::Qs,
};
use aircommon::identifiers::Fqdn;
use airserver::{
    ServerRunParams, as_connector::SimpleAsConnector, code_command::run_code_command,
    configurations::*, enqueue_provider::SimpleEnqueueProvider, logging::init_logging,
    network_provider::MockNetworkProvider,
    push_notification_provider::ProductionPushNotificationProvider, run,
    username_command::run_username_command,
};
use anyhow::{Context, bail};
use clap::Parser;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();
    let args = airserver::args::Args::parse();

    let mut configuration = get_configuration("server/").context("Could not load configuration")?;

    if configuration.application.domain.is_empty() {
        bail!("No domain name configured");
    }
    let base_db_name = configuration.database.name.clone();

    let domain: Fqdn = configuration
        .application
        .domain
        .parse()
        .expect("Invalid domain");

    match args.cmd.unwrap_or_default() {
        airserver::args::Command::Run => (),
        airserver::args::Command::Code(code_args) => {
            configuration.database.name = format!("{base_db_name}_as");
            return run_code_command(code_args, configuration, domain).await;
        }
        airserver::args::Command::Username(username_args) => {
            configuration.database.name = format!("{base_db_name}_as");
            return run_username_command(username_args, configuration, domain).await;
        }
    }

    info!(%domain, "Starting server");

    // Port binding
    let listener = TcpListener::bind(configuration.application.listen)
        .await
        .expect("Failed to bind");
    let metrics_listener = TcpListener::bind(configuration.application.listen_metrics)
        .await
        .expect("Failed to bind");

    let version_req = configuration.application.versionreq.as_ref();
    info!(
        %domain,
        version_req =? version_req.map(|v| v.to_string()),
        "Starting server"
    );
    let network_provider = MockNetworkProvider::new();

    // DS storage provider
    configuration.database.name = format!("{base_db_name}_ds");
    info!(
        host = configuration.database.host,
        "Connecting to postgres server",
    );
    let shutdown = CancellationToken::new();
    let mut ds_result = Ds::new(
        &configuration.database,
        domain.clone(),
        version_req.cloned(),
        shutdown.clone(),
    )
    .await;

    // Try again for 10 times each second in case the postgres server is coming up.
    let mut counter = 0;
    while let Err(e) = ds_result {
        info!("Failed to connect to postgres server: {}", e);
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        counter += 1;
        if counter > 10 {
            panic!("Database not ready after 10 seconds.");
        }
        ds_result = Ds::new(
            &configuration.database,
            domain.clone(),
            version_req.cloned(),
            shutdown.clone(),
        )
        .await;
    }
    let mut ds = ds_result.unwrap();
    if let Some(storage_settings) = &configuration.storage {
        let storage = Storage::new(storage_settings.clone());
        ds.set_storage(storage);
    }

    // New database name for the QS provider
    configuration.database.name = format!("{base_db_name}_qs");
    // QS storage provider
    let qs = Qs::new(
        &configuration.database,
        domain.clone(),
        version_req.cloned(),
        shutdown.clone(),
    )
    .await
    .expect("Failed to connect to database.");

    // New database name for the AS provider
    configuration.database.name = format!("{base_db_name}_as");
    let mut auth_service = AuthService::new(
        &configuration.database,
        domain.clone(),
        version_req.cloned(),
        shutdown.clone(),
    )
    .await
    .expect("Failed to connect to database.");
    if let Some(code) = configuration.application.unredeemablecode {
        auth_service.set_unredeemable_code(code);
    }

    let as_connector = SimpleAsConnector::new(&auth_service);

    let push_notification_provider =
        ProductionPushNotificationProvider::new(configuration.fcm, configuration.apns)?;
    let qs_connector = SimpleEnqueueProvider {
        qs: qs.clone(),
        push_notification_provider,
        network: network_provider.clone(),
    };

    tokio::spawn(listen_to_sigterm(shutdown.clone()));

    // Start the server
    let server = run(
        ServerRunParams {
            listener,
            metrics_listener: Some(metrics_listener),
            ds,
            auth_service,
            as_connector,
            qs,
            qs_connector,
            rate_limits: configuration.ratelimits,
            shutdown,
        },
        #[cfg(any(feature = "test_utils", test))]
        Ok,
    )
    .await;

    server.await?;
    Ok(())
}

#[cfg(unix)]
async fn listen_to_sigterm(shutdown: CancellationToken) {
    use tokio::signal::unix::{SignalKind, signal};

    let mut sigterm = match signal(SignalKind::terminate()) {
        Ok(sigterm) => sigterm,
        Err(error) => {
            tracing::error!(%error, "Failed to register SIGTERM handler");
            return;
        }
    };
    sigterm.recv().await;
    tracing::info!("SIGTERM received, shutting down");
    shutdown.cancel();
}

#[cfg(not(unix))]
async fn listen_to_sigterm(_shutdown: CancellationToken) {}
