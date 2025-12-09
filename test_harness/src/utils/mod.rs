#![allow(dead_code)]

// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{net::SocketAddr, time::Duration};

pub mod setup;

use airbackend::{
    air_service::BackendService,
    auth_service::AuthService,
    ds::{Ds, storage::Storage},
    qs::Qs,
    settings::RateLimitsSettings,
};
use aircommon::identifiers::Fqdn;
use airserver::{
    ServerRunParams, configurations::get_configuration_from_str,
    enqueue_provider::SimpleEnqueueProvider, network_provider::MockNetworkProvider,
    push_notification_provider::ProductionPushNotificationProvider, run,
};
use semver::VersionReq;
use tokio::net::TcpListener;
use uuid::Uuid;

use crate::init_test_tracing;

const BASE_CONFIG: &str = include_str!("../../../server/configuration/base.yaml");
const LOCAL_CONFIG: &str = include_str!("../../../server/configuration/local.yaml");

const TEST_RATE_LIMITS: RateLimitsSettings = RateLimitsSettings {
    period: Duration::from_millis(1),
    burst: 1000,
};

pub(crate) async fn spawn_app(
    domain: Fqdn,
    network_provider: MockNetworkProvider,
    rate_limits: RateLimitsSettings,
    client_version_req: Option<VersionReq>,
    invitation_only: bool,
) -> (SocketAddr, Vec<String>) {
    init_test_tracing();

    // Load configuration
    let mut configuration = get_configuration_from_str(BASE_CONFIG, LOCAL_CONFIG)
        .expect("Could not load configuration.");
    configuration.database.name = Uuid::new_v4().to_string();

    // Port binding
    let mut listen = configuration.application.listen;
    listen.set_port(0); // Bind to a random port

    let listener = TcpListener::bind(listen)
        .await
        .expect("Failed to bind to random port.");
    let address = listener.local_addr().unwrap();

    // DS storage provider
    let mut ds = Ds::new(
        &configuration.database,
        domain.clone(),
        client_version_req.clone(),
    )
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

    let mut auth_service = AuthService::new(
        &configuration.database,
        domain.clone(),
        client_version_req.clone(),
    )
    .await
    .expect("Failed to connect to database.");
    let codes = if !invitation_only {
        auth_service.disable_invitation_only();
        Vec::new()
    } else {
        const N: usize = 10;
        auth_service.invitation_codes_generate(N).await.unwrap();
        let redeemed = false;
        auth_service
            .invitation_codes_list(N, redeemed)
            .await
            .unwrap()
            .map(|(code, _)| code)
            .collect::<Vec<_>>()
    };

    // New database name for the QS provider
    configuration.database.name = Uuid::new_v4().to_string();

    let qs = Qs::new(
        &configuration.database,
        domain.clone(),
        client_version_req.clone(),
    )
    .await
    .expect("Failed to connect to database.");

    let push_notification_provider = ProductionPushNotificationProvider::new(None, None).unwrap();

    let qs_connector = SimpleEnqueueProvider {
        qs: qs.clone(),
        push_notification_provider,
        network: network_provider.clone(),
    };

    // Start the server
    let server = run(ServerRunParams {
        listener,
        metrics_listener: None,
        ds,
        auth_service,
        qs,
        qs_connector,
        rate_limits,
    })
    .await;

    // Execute the server in the background
    tokio::spawn(server);

    // Return the address
    (address, codes)
}
