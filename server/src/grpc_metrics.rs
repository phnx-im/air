// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

// Metrics are taken from the Go middleware implementation:
// <https://github.com/grpc-ecosystem/go-grpc-middleware/blob/390bcef25adebe4b0c7dbb365230c0a856737afe/providers/prometheus/server_metrics.go>

use std::{
    pin::Pin,
    task::{Context, Poll, ready},
    time::Instant,
};

use metrics::{Unit, counter, describe_counter, describe_histogram, histogram};
use pin_project::pin_project;
use tonic::{
    Code,
    codegen::http::{Request, Response},
};
use tower::{Layer, Service};

#[derive(Clone, Default)]
pub(crate) struct GrpcMetricsLayer {}

impl GrpcMetricsLayer {
    pub(crate) fn new() -> Self {
        Default::default()
    }

    pub(crate) fn describe_metrics() {
        describe_counter!(
            "grpc_server_started_total",
            "Total number of RPCs started on the server."
        );
        describe_counter!(
            "grpc_server_handled_total",
            "Total number of RPCs completed on the server, regardless of success or failure."
        );
        describe_histogram!(
            "grpc_server_handling_seconds",
            Unit::Seconds,
            "Histogram of response latency (seconds) of gRPC that had been application-level \
                handled by the server.",
        );
    }
}

impl<S> Layer<S> for GrpcMetricsLayer {
    type Service = GrpcMetricsService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        GrpcMetricsService { inner }
    }
}

#[derive(Clone)]
pub(crate) struct GrpcMetricsService<S> {
    inner: S,
}

impl<S, B, C> Service<Request<B>> for GrpcMetricsService<S>
where
    S: Service<Request<B>, Response = Response<C>>,
{
    type Response = S::Response;

    type Error = S::Error;

    type Future = GrpcMetricsFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let path = req.uri().path().to_string();
        let path = path.trim_start_matches('/');
        let (service, method) = path.split_once('/').unwrap_or(("", path));

        GrpcMetricsFuture {
            inner: self.inner.call(req),
            service: service.to_owned(),
            method: method.to_owned(),
            started_at: None,
        }
    }
}

#[pin_project]
pub(crate) struct GrpcMetricsFuture<F> {
    #[pin]
    inner: F,
    service: String,
    method: String,
    started_at: Option<Instant>,
}

impl<F, B, E> Future for GrpcMetricsFuture<F>
where
    F: Future<Output = Result<Response<B>, E>>,
{
    type Output = Result<Response<B>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        let started_at = this.started_at.get_or_insert_with(|| {
            counter!(
                "grpc_server_started_total",
                "grpc_service" => this.service.clone(),
                "grpc_method" => this.method.clone(),
            )
            .increment(1);
            Instant::now()
        });

        let result = ready!(this.inner.poll(cx));
        let elapsed = started_at.elapsed();

        let code = result
            .as_ref()
            .ok()
            .map(|response| {
                response
                    .headers()
                    .get("grpc-status")
                    .map(|status| Code::from_bytes(status.as_bytes()))
                    // In streaming responses, `grpc-status` is not yet set
                    .unwrap_or(Code::Ok)
            })
            .unwrap_or(Code::Unknown);
        let code = format!("{:?}", code);

        counter!(
            "grpc_server_handled_total",
            "grpc_service" => this.service.clone(),
            "grpc_method" => this.method.clone(),
            "grpc_code" => code.clone(),
        )
        .increment(1);

        histogram!(
            "grpc_server_handling_seconds",
            "grpc_service" => this.service.clone(),
            "grpc_method" => this.method.clone(),
            "grpc_code" => code,
        )
        .record(elapsed.as_secs_f64());

        Poll::Ready(result)
    }
}
