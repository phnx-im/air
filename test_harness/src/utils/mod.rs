#![allow(dead_code)]

// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{net::SocketAddr, time::Duration};

pub mod controlled_listener;
pub mod setup;

use airbackend::{
    air_service::BackendService,
    auth_service::AuthService,
    ds::{Ds, storage::Storage},
    qs::Qs,
    relay_service::Rs,
    settings::{DatabaseSettings, RateLimitsSettings},
};
use aircommon::identifiers::Fqdn;
use airserver::{
    Addressed as _, ServerRunParams, as_connector::SimpleAsConnector,
    configurations::get_configuration_from_str, network_provider::MockNetworkProvider,
    push_notification_provider::ProductionPushNotificationProvider,
    qs_connector::SimpleEnqueueProvider, run,
};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};
use tokio::{
    runtime::Handle,
    task::{JoinHandle, block_in_place},
};
use tokio_util::sync::CancellationToken;
use tonic::Status;
use tracing::info;
use uuid::Uuid;

use crate::{
    init_test_tracing,
    utils::{
        controlled_listener::{ControlHandle, ControlledIncoming, Mode},
        setup::TestBackendParams,
    },
};

const BASE_CONFIG: &str = include_str!("../../../server/configuration/base.yaml");
const LOCAL_CONFIG: &str = include_str!("../../../server/configuration/local.yaml");

const TEST_RATE_LIMITS: RateLimitsSettings = RateLimitsSettings {
    period: Duration::from_millis(1),
    burst: 1000,
};

pub struct SpawnedApp {
    pub address: SocketAddr,
    pub control_handle: ControlHandle,
    pub codes: Vec<String>,
    db_settings: DatabaseSettings,
    db_names: DbNames,
    stop: CancellationToken,
    server_handle: Option<JoinHandle<()>>,
}

struct DbNames {
    ds: Uuid,
    as_: Uuid,
    qs: Uuid,
}

impl DbNames {
    pub fn random() -> Self {
        Self {
            ds: Uuid::new_v4(),
            as_: Uuid::new_v4(),
            qs: Uuid::new_v4(),
        }
    }
}

impl SpawnedApp {
    async fn cleanup(&mut self) {
        // Drop test databases
        for db_name in [self.db_names.as_, self.db_names.ds, self.db_names.qs] {
            let mut db_settings = self.db_settings.clone();
            db_settings.name = db_name.to_string();
            let mut connection =
                PgConnection::connect(&db_settings.connection_string_without_database())
                    .await
                    .unwrap();

            let db_size: String = sqlx::query(AssertSqlSafe(format!(
                r#"SELECT pg_size_pretty( pg_database_size('{db_name}'))"#
            )))
            .fetch_one(&mut connection)
            .await
            .unwrap()
            .try_get(0)
            .unwrap();

            sqlx::query(AssertSqlSafe(format!(r#"DROP DATABASE "{db_name}""#)))
                .execute(&mut connection)
                .await
                .unwrap();
            info!(%db_name, db_size, "Dropped test database");
        }
    }
}

impl Drop for SpawnedApp {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(handle) = self.server_handle.take() {
            block_in_place(|| {
                Handle::current().block_on(async move {
                    handle.await.expect("Server stopped with an error");
                    self.cleanup().await;
                });
            });
        }
        info!("Test server stopped");
    }
}

pub(crate) async fn spawn_app(
    domain: Fqdn,
    network_provider: MockNetworkProvider,
    params: TestBackendParams,
) -> SpawnedApp {
    init_test_tracing();

    let TestBackendParams {
        rate_limits,
        client_version_req,
        invitation_only,
        unredeemable_code,
        max_attachment_size,
    } = params;

    // Load configuration
    let mut configuration = get_configuration_from_str(BASE_CONFIG, LOCAL_CONFIG)
        .expect("Could not load configuration.");

    // Port binding
    let mut listen = configuration.application.listen;
    listen.set_port(0); // Bind to a random port

    // Controlled listener
    let (listener, control_handle) = ControlledIncoming::bind(listen)
        .await
        .expect("Failed to bind controlled listener.");

    let interceptor_control_handle = control_handle.clone();

    let interceptor = move |request| {
        match interceptor_control_handle.mode() {
            Mode::DropNextResponse => interceptor_control_handle.set_drop_connection_on_write(),
            Mode::DropNextRequest => {
                interceptor_control_handle.set_normal();
                return Err(Status::unavailable("cancelled for interop test"));
            }
            _ => {}
        }
        Ok(request)
    };

    let address = listener.local_addr().unwrap();

    let db_names = DbNames::random();

    let stop = CancellationToken::new();

    // DS storage provider
    configuration.database.name = db_names.ds.to_string();
    let mut ds = Ds::new(
        &configuration.database,
        domain.clone(),
        client_version_req.clone(),
        stop.clone(),
    )
    .await
    .expect("Failed to connect to database.");
    let mut storage_config = configuration
        .storage
        .clone()
        .expect("no storage configuration");
    storage_config.max_attachment_size = max_attachment_size;
    storage_config.require_content_length = true;
    ds.set_storage(Storage::new(storage_config));

    // New database name for the AS provider
    configuration.database.name = db_names.as_.to_string();

    let mut auth_service = AuthService::new(
        &configuration.database,
        domain.clone(),
        client_version_req.clone(),
        stop.clone(),
    )
    .await
    .expect("Failed to connect to database.");

    let as_connector = SimpleAsConnector::new(&auth_service);

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
    if let Some(code) = unredeemable_code {
        auth_service.set_unredeemable_code(code);
    }

    // New database name for the QS provider
    configuration.database.name = db_names.qs.to_string();

    let qs = Qs::new(
        &configuration.database,
        domain.clone(),
        client_version_req.clone(),
        stop.clone(),
    )
    .await
    .expect("Failed to connect to database.");

    let push_notification_provider = ProductionPushNotificationProvider::new(None, None).unwrap();

    let qs_connector = SimpleEnqueueProvider {
        qs: qs.clone(),
        push_notification_provider,
        network: network_provider.clone(),
    };

    let rs = Rs::new(stop.clone());

    // Start the server
    let server = run(
        ServerRunParams {
            listener,
            metrics_listener: None,
            ds,
            auth_service,
            as_connector,
            qs,
            qs_connector,
            rs,
            rate_limits: rate_limits.unwrap_or(TEST_RATE_LIMITS),
            shutdown: stop.clone(),
        },
        interceptor,
    )
    .await;

    // Execute the server in the background
    let server_handle = tokio::spawn(async move {
        server.await.expect("Server stopped with an error");
    });

    // Return the address
    SpawnedApp {
        address,
        control_handle,
        codes,
        db_settings: configuration.database,
        db_names,
        stop,
        server_handle: Some(server_handle),
    }
}
