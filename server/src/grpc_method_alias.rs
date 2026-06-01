// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! A layer that adds method aliases to the gRPC service.
//!
//! Useful for backwards compatibility with older clients when methods were renamed.

use tonic::codegen::http::{Request, Uri};
use tower::util::MapRequestLayer;

/// A layer that adds method aliases to the gRPC service.
///
/// Useful for backwards compatibility with older clients when methods were renamed.
pub(crate) fn layer<B>() -> MapRequestLayer<impl Fn(Request<B>) -> Request<B> + Clone> {
    MapRequestLayer::new(|mut req: Request<B>| {
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
        req
    })
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
