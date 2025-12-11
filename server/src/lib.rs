// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server that makes the logic implemented in the backend available to clients via a REST API

#![warn(clippy::large_futures)]

use std::{future, time::Duration};

use airbackend::{
    auth_service::{AuthService, grpc::GrpcAs},
    ds::{Ds, GrpcDs},
    qs::{
        Qs, QsConnector, errors::QsEnqueueError, grpc::GrpcQs, network_provider::NetworkProvider,
    },
    settings::RateLimitsSettings,
};
use airprotos::{
    auth_service::v1::auth_service_server::AuthServiceServer,
    delivery_service::v1::delivery_service_server::DeliveryServiceServer,
    queue_service::v1::queue_service_server::QueueServiceServer,
};
use axum::extract::State;
use connect_info::ConnectInfoInterceptor;
use futures_core::Stream;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpListener,
};
use tonic::{service::InterceptorLayer, transport::server::Connected};
use tonic_health::pb::health_server::{Health, HealthServer};
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor,
};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::{Level, enabled, error, info};

use crate::grpc_metrics::GrpcMetricsLayer;

pub mod configurations;
mod connect_info;
pub mod enqueue_provider;
mod grpc_metrics;
pub mod logging;
pub mod network_provider;
pub mod push_notification_provider;

pub struct ServerRunParams<Qc, Listener> {
    pub listener: Listener,
    pub metrics_listener: Option<TcpListener>,
    pub ds: Ds,
    pub auth_service: AuthService,
    pub qs: Qs,
    pub qs_connector: Qc,
    pub rate_limits: RateLimitsSettings,
}

pub trait Addressed {
    fn local_addr(&self) -> std::io::Result<std::net::SocketAddr>;
}

impl Addressed for TcpListener {
    fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.local_addr()
    }
}

pub trait IntoStream {
    type Item: AsyncRead + AsyncWrite + Connected + Unpin + Send + 'static;
    type Error: Into<Box<dyn std::error::Error + Send + Sync>>;
    type Stream: Stream<Item = Result<Self::Item, Self::Error>>;

    fn into_stream(self) -> Self::Stream;
}

impl IntoStream for TcpListener {
    type Item = tokio::net::TcpStream;
    type Error = std::io::Error;
    type Stream = tokio_stream::wrappers::TcpListenerStream;

    fn into_stream(self) -> Self::Stream {
        tokio_stream::wrappers::TcpListenerStream::new(self)
    }
}

/// Configure and run the server application.
pub async fn run<
    Qc: QsConnector<EnqueueError = QsEnqueueError<Np>> + Clone,
    Np: NetworkProvider,
    L: Addressed + IntoStream,
>(
    ServerRunParams {
        listener,
        metrics_listener,
        ds,
        auth_service,
        qs,
        qs_connector,
        rate_limits,
    }: ServerRunParams<Qc, L>,
) -> impl Future<Output = Result<(), tonic::transport::Error>> {
    let grpc_addr = listener.local_addr().expect("Could not get local address");

    info!(%grpc_addr, "Starting server");

    serve_metrics(metrics_listener);

    // GRPC server
    let grpc_as = GrpcAs::new(auth_service);
    let grpc_ds = GrpcDs::new(ds, qs_connector);
    let grpc_qs = GrpcQs::new(qs);

    info!(?rate_limits, "Applying rate limits");
    let RateLimitsSettings { period, burst } = rate_limits;

    let governor_config = GovernorConfigBuilder::default()
        .period(period)
        .burst_size(burst)
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
        .layer(GrpcMetricsLayer::new())
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
        .serve_with_incoming(listener.into_stream())
}

fn serve_metrics(metrics_listener: Option<TcpListener>) {
    GrpcMetricsLayer::describe_metrics();
    if let Some(listener) = metrics_listener {
        let addr = listener.local_addr().expect("Could not get local address");

        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        metrics::set_global_recorder(recorder).expect("metrics already set");

        let router = axum::Router::new().route(
            "/metrics",
            axum::routing::get(|State(handle): State<PrometheusHandle>| {
                future::ready(handle.render())
            })
            .with_state(handle.clone()),
        );

        const UPKEEP_TIMEOUT: Duration = Duration::from_secs(5);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(UPKEEP_TIMEOUT).await;
                handle.run_upkeep();
            }
        });

        tokio::spawn(async move {
            info!(%addr, "Serving metrics");
            if let Err(error) = axum::serve(listener, router.into_make_service()).await {
                error!(%error, "Metrics server stopped");
            }
        });
    }
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
