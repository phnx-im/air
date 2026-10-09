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

use http_body::{Body, Frame, SizeHint};
use metrics::{Unit, counter, describe_counter, describe_histogram, histogram};
use pin_project::{pin_project, pinned_drop};
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
    type Response = Response<GrpcMetricsBody<C>>;

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
    type Output = Result<Response<GrpcMetricsBody<B>>, E>;

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
        let handled = Handled {
            service: this.service.clone(),
            method: this.method.clone(),
            started_at: *started_at,
        };

        let response = match result {
            Ok(response) => response,
            Err(error) => {
                handled.record(Code::Unknown);
                return Poll::Ready(Err(error));
            }
        };
        let handled = match response.headers().get("grpc-status") {
            // Trailers-only response, e.g. an error before any message
            Some(status) => {
                handled.record(Code::from_bytes(status.as_bytes()));
                None
            }
            // The status comes in the trailers, so streams are handled when
            // they end
            None => Some(handled),
        };

        Poll::Ready(Ok(response.map(|inner| GrpcMetricsBody { inner, handled })))
    }
}

#[pin_project(PinnedDrop)]
pub(crate) struct GrpcMetricsBody<B> {
    #[pin]
    inner: B,
    handled: Option<Handled>,
}

impl<B: Body> Body for GrpcMetricsBody<B> {
    type Data = B::Data;
    type Error = B::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.project();
        let frame = ready!(this.inner.poll_frame(cx));

        let code = match &frame {
            Some(Ok(frame)) => frame.trailers_ref().map(|trailers| {
                trailers
                    .get("grpc-status")
                    .map_or(Code::Unknown, |status| Code::from_bytes(status.as_bytes()))
            }),
            // A body error or an end without trailers, which gRPC doesn't allow
            Some(Err(_)) | None => Some(Code::Unknown),
        };
        if let Some(code) = code
            && let Some(handled) = this.handled.take()
        {
            handled.record(code);
        }

        Poll::Ready(frame)
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

#[pinned_drop]
impl<B> PinnedDrop for GrpcMetricsBody<B> {
    fn drop(self: Pin<&mut Self>) {
        // The client went away before the stream ended
        if let Some(handled) = self.project().handled.take() {
            handled.record(Code::Cancelled);
        }
    }
}

struct Handled {
    service: String,
    method: String,
    started_at: Instant,
}

impl Handled {
    fn record(self, code: Code) {
        let code = format!("{:?}", code);

        counter!(
            "grpc_server_handled_total",
            "grpc_service" => self.service.clone(),
            "grpc_method" => self.method.clone(),
            "grpc_code" => code.clone(),
        )
        .increment(1);

        histogram!(
            "grpc_server_handling_seconds",
            "grpc_service" => self.service,
            "grpc_method" => self.method,
            "grpc_code" => code,
        )
        .record(self.started_at.elapsed().as_secs_f64());
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, convert::Infallible, future::poll_fn};

    use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
    use tonic::codegen::{
        Bytes,
        http::{HeaderMap, HeaderValue},
    };

    use super::*;

    struct TestBody(VecDeque<Frame<Bytes>>);

    impl Body for TestBody {
        type Data = Bytes;
        type Error = Infallible;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
            Poll::Ready(self.0.pop_front().map(Ok))
        }
    }

    /// Answers a `Listen` call with a stream of two messages that ends with
    /// `Internal`.
    async fn listen() -> GrpcMetricsBody<TestBody> {
        let mut trailers = HeaderMap::new();
        trailers.insert("grpc-status", HeaderValue::from(Code::Internal as i32));
        let message = || Frame::data(Bytes::from_static(b"message"));
        let mut body = Some(TestBody(
            [message(), message(), Frame::trailers(trailers)].into(),
        ));

        let mut service = GrpcMetricsLayer::new().layer(tower::service_fn(move |_: Request<()>| {
            let body = body.take().expect("called once");
            std::future::ready(Ok::<_, Infallible>(Response::new(body)))
        }));
        let request = Request::builder()
            .uri("/queue_service.v1.QueueService/Listen")
            .body(())
            .unwrap();
        service.call(request).await.unwrap().into_body()
    }

    /// Sorted counts by name and `grpc_code` since the last call, snapshots
    /// drain.
    fn counts(snapshotter: &Snapshotter) -> Vec<(String, Option<String>, u64)> {
        let mut counts = snapshotter
            .snapshot()
            .into_vec()
            .into_iter()
            .filter_map(|(key, .., value)| {
                let DebugValue::Counter(value) = value else {
                    return None;
                };
                let code = key
                    .key()
                    .labels()
                    .find(|label| label.key() == "grpc_code")
                    .map(|label| label.value().to_owned());
                Some((key.key().name().to_owned(), code, value))
            })
            .filter(|(.., value)| *value > 0)
            .collect::<Vec<_>>();
        counts.sort();
        counts
    }

    fn started() -> (String, Option<String>, u64) {
        ("grpc_server_started_total".to_owned(), None, 1)
    }

    fn handled(code: &str) -> (String, Option<String>, u64) {
        (
            "grpc_server_handled_total".to_owned(),
            Some(code.to_owned()),
            1,
        )
    }

    #[tokio::test]
    async fn stream_is_handled_when_it_ends() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let _guard = metrics::set_default_local_recorder(&recorder);

        // Open stream: started, not handled
        let mut body = listen().await;
        assert_eq!(counts(&snapshotter), [started()]);

        while poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
            .await
            .is_some()
        {}
        assert_eq!(counts(&snapshotter), [handled("Internal")]);

        drop(body);
        assert_eq!(counts(&snapshotter), []);
    }

    #[tokio::test]
    async fn dropped_stream_is_cancelled() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let _guard = metrics::set_default_local_recorder(&recorder);

        drop(listen().await);
        assert_eq!(counts(&snapshotter), [handled("Cancelled"), started()]);
    }
}
