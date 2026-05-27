// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! A layer that adds method aliases to the gRPC service.
//!
//! Useful for backwards compatibility with older clients when methods were renamed.

use std::task::{Context, Poll};

use tonic::codegen::http::{Request, Response, Uri};
use tower::{Layer, Service};

/// A layer that adds method aliases to the gRPC service.
///
/// Useful for backwards compatibility with older clients when methods were renamed.
#[derive(Clone, Default)]
pub(crate) struct GrpcMethodAliasLayer {}

impl GrpcMethodAliasLayer {
    pub fn new() -> Self {
        Default::default()
    }
}

impl<S> Layer<S> for GrpcMethodAliasLayer {
    type Service = GrpcMethodAliasService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        GrpcMethodAliasService { inner }
    }
}

#[derive(Clone)]
pub(crate) struct GrpcMethodAliasService<S> {
    inner: S,
}

/// List of renamed methods
const ALIASES: &[(&str, &str)] = &[
    (
        "/auth_service.v1.AuthService/CheckHandleExists",
        "/auth_service.v1.AuthService/CheckUsernameExists",
    ),
    (
        "/auth_service.v1.AuthService/CreateHandle",
        "/auth_service.v1.AuthService/CreateUsername",
    ),
    (
        "/auth_service.v1.AuthService/DeleteHandle",
        "/auth_service.v1.AuthService/DeleteUsername",
    ),
    (
        "/auth_service.v1.AuthService/RefreshHandle",
        "/auth_service.v1.AuthService/RefreshUsername",
    ),
    (
        "/auth_service.v1.AuthService/ConnectHandle",
        "/auth_service.v1.AuthService/ConnectUsername",
    ),
    (
        "/auth_service.v1.AuthService/ListenHandle",
        "/auth_service.v1.AuthService/ListenUsername",
    ),
];

impl<S, B, C> Service<Request<B>> for GrpcMethodAliasService<S>
where
    S: Service<Request<B>, Response = Response<C>>,
{
    type Response = S::Response;

    type Error = S::Error;

    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<B>) -> Self::Future {
        for (from, to) in ALIASES {
            // gRPC over HTTP/2 method is fully identified by the :path pseudo-header alone.
            //
            // See [Grpc-Http2], section "Path".
            //
            // [Grpc-Http2]: https://github.com/grpc/grpc/blob/e34469da3ff85bc165e5cf4fb65fc91f814420b6/doc/PROTOCOL-HTTP2.md
            if req.uri().path() == *from {
                *req.uri_mut() = Uri::from_static(to);
                break;
            }
        }
        self.inner.call(req)
    }
}
