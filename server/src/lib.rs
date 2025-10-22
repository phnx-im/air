// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server that makes the logic implemented in the backend available to clients via a REST API

#![warn(clippy::large_futures)]

use std::time::Duration;

use airbackend::{
    auth_service::{AuthService, grpc::GrpcAs},
    ds::{Ds, GrpcDs},
    qs::{
        Qs, QsConnector, errors::QsEnqueueError, grpc::GrpcQs, network_provider::NetworkProvider,
    },
};
use airprotos::{
    auth_service::v1::auth_service_server::AuthServiceServer,
    delivery_service::v1::delivery_service_server::DeliveryServiceServer,
    queue_service::v1::queue_service_server::QueueServiceServer,
};
use connect_info::ConnectInfoInterceptor;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::service::InterceptorLayer;
use tonic_health::pb::health_server::{Health, HealthServer};
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor,
};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::{Level, enabled, info};

pub mod configurations;
mod connect_info;
pub mod enqueue_provider;
pub mod network_provider;
pub mod push_notification_provider;
pub mod telemetry;

pub struct ServerRunParams<Qc> {
    pub listener: tokio::net::TcpListener,
    pub ds: Ds,
    pub auth_service: AuthService,
    pub qs: Qs,
    pub qs_connector: Qc,
    pub rate_limits: RateLimitsConfig,
}

/// Every `period`, allow bursts of up to `burst_size`-many requests, and replenish one element
/// after the `period`.
pub struct RateLimitsConfig {
    pub period: Duration,
    pub burst_size: u32,
}

/// Configure and run the server application.
pub async fn run<
    Qc: QsConnector<EnqueueError = QsEnqueueError<Np>> + Clone,
    Np: NetworkProvider,
>(
    ServerRunParams {
        listener: grpc_listener,
        ds,
        auth_service,
        qs,
        qs_connector,
        rate_limits,
    }: ServerRunParams<Qc>,
) -> impl Future<Output = Result<(), tonic::transport::Error>> {
    let grpc_addr = grpc_listener
        .local_addr()
        .expect("Could not get local address");

    info!(%grpc_addr, "Starting server");

    // GRPC server
    let grpc_as = GrpcAs::new(auth_service);
    let grpc_ds = GrpcDs::new(ds, qs_connector);
    let grpc_qs = GrpcQs::new(qs);

    let RateLimitsConfig { period, burst_size } = rate_limits;
    let governor_config = GovernorConfigBuilder::default()
        .period(period)
        .burst_size(burst_size)
        .key_extractor(SmartIpKeyExtractor)
        .finish()
        .expect("invalid governor config");

    // task cleaning up limiter tokens
    let governor_limiter = governor_config.limiter().clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            governor_limiter.retain_recent();
        }
    });

    let health_service = configure_health_service::<Qc, Np>().await;

    tonic::transport::Server::builder()
        .http2_keepalive_interval(Some(Duration::from_secs(30)))
        .layer(InterceptorLayer::new(ConnectInfoInterceptor))
        .layer(
            TraceLayer::new_for_grpc()
                .make_span_with(
                    DefaultMakeSpan::new()
                        .level(Level::INFO)
                        .include_headers(enabled!(Level::DEBUG)),
                )
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .include_headers(enabled!(Level::DEBUG)),
                ),
        )
        .layer(GovernorLayer::new(governor_config))
        .add_service(health_service)
        .add_service(AuthServiceServer::new(grpc_as))
        .add_service(DeliveryServiceServer::new(grpc_ds))
        .add_service(QueueServiceServer::new(grpc_qs))
        .serve_with_incoming(TcpListenerStream::new(grpc_listener))
}

async fn configure_health_service<
    Qc: QsConnector<EnqueueError = QsEnqueueError<Np>> + Clone,
    Np: NetworkProvider,
>() -> HealthServer<impl Health> {
    let (reporter, service) = tonic_health::server::health_reporter();
    reporter.set_serving::<AuthServiceServer<GrpcAs>>().await;
    reporter
        .set_serving::<DeliveryServiceServer<GrpcDs<Qc>>>()
        .await;
    reporter.set_serving::<QueueServiceServer<GrpcQs>>().await;
    service
}
