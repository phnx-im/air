// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

pub mod controlled_listener;
pub mod setup;

use airbackend::{
    air_service::{BackendService, MaxDevices},
    auth_service::{
        AuthService,
        admission::{ChallengeSendError, ChallengeSender},
    },
    ds::{Ds, storage::Storage},
    qs::Qs,
    relay_service::Rs,
    settings::{DatabaseSettings, RateLimitsSettings, RegistrationPolicy},
};
use aircommon::{
    identifiers::Fqdn,
    messages::push_token::{PushToken, PushTokenOperator},
};
use airserver::{
    Addressed as _, ServerRunParams, as_connector::SimpleAsConnector,
    configurations::get_configuration_from_str, network_provider::MockNetworkProvider,
    push_notification_provider::ProductionPushNotificationProvider,
    qs_connector::SimpleEnqueueProvider, run,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{AssertSqlSafe, Connection, PgConnection};
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

/// What the server tried to send to a push endpoint. The tests read the
/// challenge here instead of from a push service.
pub type SentChallenges = Arc<Mutex<Vec<String>>>;

#[derive(Debug)]
struct LoopbackChallengeSender {
    sent: SentChallenges,
}

#[async_trait]
impl ChallengeSender for LoopbackChallengeSender {
    fn supports(&self, _operator: &PushTokenOperator) -> bool {
        true
    }

    async fn send_challenge(
        &self,
        _push_token: &PushToken,
        _session_id: Uuid,
        challenge: &str,
        _expires_at: DateTime<Utc>,
    ) -> Result<(), ChallengeSendError> {
        self.sent
            .lock()
            .expect("the challenge lock is poisoned")
            .push(challenge.to_owned());
        Ok(())
    }
}

pub struct SpawnedApp {
    pub address: SocketAddr,
    pub control_handle: ControlHandle,
    pub codes: Vec<String>,
    pub sent_challenges: SentChallenges,
    pub max_devices: [MaxDevices; 3],
    db_settings: DatabaseSettings,
    db_name: Uuid,
    stop: CancellationToken,
    server_handle: Option<JoinHandle<()>>,
}

impl SpawnedApp {
    async fn cleanup(&mut self) {
        let db_name = self.db_name;
        execute_without_database(&self.db_settings, format!(r#"DROP DATABASE "{db_name}""#)).await;
        info!(%db_name, "Dropped test database");
    }
}

async fn execute_without_database(db_settings: &DatabaseSettings, statement: String) {
    let mut connection = PgConnection::connect(&db_settings.connection_string_without_database())
        .await
        .unwrap();
    sqlx::query(AssertSqlSafe(statement))
        .execute(&mut connection)
        .await
        .unwrap();
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
        version_policy,
        registration,
        unredeemable_code,
        max_attachment_size,
        max_devices,
    } = params;

    // Load configuration
    let configuration = get_configuration_from_str(BASE_CONFIG, LOCAL_CONFIG)
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
            Mode::DropNextResponse if interceptor_control_handle.take_armed() => {
                interceptor_control_handle.set_drop_connection_on_write()
            }
            Mode::DropNextRequest if interceptor_control_handle.take_armed() => {
                interceptor_control_handle.set_normal();
                return Err(Status::unavailable("cancelled for interop test"));
            }
            Mode::RejectNextRequest if interceptor_control_handle.take_armed() => {
                interceptor_control_handle.set_normal();
                return Err(Status::invalid_argument("rejected for interop test"));
            }
            _ => {}
        }
        Ok(request)
    };

    let address = listener.local_addr().unwrap();

    let db_name = Uuid::new_v4();

    let stop = CancellationToken::new();

    let max_devices = max_devices.unwrap_or(configuration.application.max_devices);

    // The services share a single schema and use disjoint tables, so they also
    // share a single database. It is created upfront, because concurrent
    // services would all try to create it.
    execute_without_database(
        &configuration.database,
        format!(r#"CREATE DATABASE "{db_name}""#),
    )
    .await;
    let db_settings = DatabaseSettings {
        name: db_name.to_string(),
        ..configuration.database.clone()
    };
    let (mut ds, mut auth_service, qs) = tokio::try_join!(
        Ds::new(
            &db_settings,
            domain.clone(),
            version_policy.clone(),
            max_devices,
            stop.clone(),
        ),
        AuthService::new(
            &db_settings,
            domain.clone(),
            version_policy.clone(),
            max_devices,
            stop.clone(),
        ),
        Qs::new(
            &db_settings,
            domain.clone(),
            version_policy.clone(),
            max_devices,
            stop.clone(),
        ),
    )
    .expect("Failed to connect to database.");

    let mut storage_config = configuration
        .storage
        .clone()
        .expect("no storage configuration");
    storage_config.max_attachment_size = max_attachment_size;
    storage_config.require_content_length = true;
    ds.set_storage(Storage::new(storage_config));

    let as_connector = SimpleAsConnector::new(&auth_service);

    // Any policy that can close the gate needs codes to answer it with.
    let codes = if registration.policy == RegistrationPolicy::Open {
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
    auth_service.set_registration_settings(registration);
    let sent_challenges = SentChallenges::default();
    auth_service.set_challenge_sender(Arc::new(LoopbackChallengeSender {
        sent: sent_challenges.clone(),
    }));
    if let Some(code) = unredeemable_code {
        auth_service.set_unredeemable_code(code);
    }

    let max_devices = [
        ds.max_devices_handle(),
        auth_service.max_devices_handle(),
        qs.max_devices_handle(),
    ];

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
        sent_challenges,
        max_devices,
        db_settings: configuration.database,
        db_name,
        stop,
        server_handle: Some(server_handle),
    }
}
