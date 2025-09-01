#![allow(dead_code)]

// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{net::SocketAddr, time::Duration};

pub mod setup;

use airbackend::{
    auth_service::AuthService,
    ds::{Ds, storage::Storage},
    infra_service::InfraService,
    qs::Qs,
};
use aircommon::identifiers::Fqdn;
use airserver::{
    RateLimitsConfig, ServerRunParams, configurations::get_configuration_from_str,
    dispatch::DispatchNotifier, enqueue_provider::SimpleEnqueueProvider,
    network_provider::MockNetworkProvider,
    push_notification_provider::ProductionPushNotificationProvider, run,
};
use tokio::net::TcpListener;
use uuid::Uuid;

use crate::init_test_tracing;

const BASE_CONFIG: &str = include_str!("../../../server/configuration/base.yaml");
const LOCAL_CONFIG: &str = include_str!("../../../server/configuration/local.yaml");

const TEST_RATE_LIMITS: RateLimitsConfig = RateLimitsConfig {
    period: Duration::from_millis(1),
    burst_size: 1000,
};

/// Start the server and initialize the database connection.
///
/// Returns the HTTP and gRPC addresses, and a `DispatchWebsocketNotifier` to dispatch
/// notifications.
pub async fn spawn_app(
    domain: impl Into<Option<Fqdn>>,
    network_provider: MockNetworkProvider,
) -> (SocketAddr, DispatchNotifier) {
    spawn_app_with_rate_limits(domain, network_provider, TEST_RATE_LIMITS).await
}

/// Same as [`spawn_app`], but allows to configure rate limits.
pub async fn spawn_app_with_rate_limits(
    domain: impl Into<Option<Fqdn>>,
    network_provider: MockNetworkProvider,
    rate_limits: RateLimitsConfig,
) -> (SocketAddr, DispatchNotifier) {
    init_test_tracing();

    // Load configuration
    let mut configuration = get_configuration_from_str(BASE_CONFIG, LOCAL_CONFIG)
        .expect("Could not load configuration.");
    configuration.database.name = Uuid::new_v4().to_string();

    // Port binding
    let host = configuration.application.host;
    let domain = domain.into().unwrap_or_else(|| host.parse().unwrap());

    let grpc_listener = TcpListener::bind(format!("{host}:0"))
        .await
        .expect("Failed to bind to random port.");
    let grpc_address = grpc_listener.local_addr().unwrap();

    let dispatch_notifier = DispatchNotifier::new();

    // DS storage provider
    let mut ds = Ds::new(&configuration.database, domain.clone())
        .await
        .expect("Failed to connect to database.");
    ds.set_storage(Storage::new(
        configuration
            .storage
            .clone()
            .expect("no storage configuration"),
    ));

    // New database name for the AS provider
    configuration.database.name = Uuid::new_v4().to_string();

    let auth_service = AuthService::new(&configuration.database, domain.clone())
        .await
        .expect("Failed to connect to database.");

    // New database name for the QS provider
    configuration.database.name = Uuid::new_v4().to_string();

    let qs = Qs::new(&configuration.database, domain.clone())
        .await
        .expect("Failed to connect to database.");

    let push_notification_provider = ProductionPushNotificationProvider::new(None, None).unwrap();

    let qs_connector = SimpleEnqueueProvider {
        qs: qs.clone(),
        notifier: dispatch_notifier.clone(),
        push_notification_provider,
        network: network_provider.clone(),
    };

    // Start the server
    let server = run(ServerRunParams {
        listener: grpc_listener,
        ds,
        auth_service,
        qs,
        qs_connector,
        dispatch_notifier: dispatch_notifier.clone(),
        rate_limits,
    })
    .await;

    // Execute the server in the background
    tokio::spawn(server);

    // Return the address
    (grpc_address, dispatch_notifier)
}
