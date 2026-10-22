// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Test-only hook that lets the test harness fail chosen requests.

use tonic::{Request, Status, codegen::http};
use tower::util::MapRequestLayer;

/// The gRPC path of a request, e.g.
/// `/auth_service.v1.AuthService/DeleteUsername`.
#[derive(Clone)]
struct RpcPath(String);

/// Records the gRPC path of each request for [`interceptor`], which only sees
/// the request without its URI.
pub(crate) fn rpc_path_layer<B>()
-> MapRequestLayer<impl Fn(http::Request<B>) -> http::Request<B> + Clone> {
    MapRequestLayer::new(|mut request: http::Request<B>| {
        let path = RpcPath(request.uri().path().to_owned());
        request.extensions_mut().insert(path);
        request
    })
}

/// Calls `hook` with the gRPC path of each request. An error fails the request
/// with that status.
pub(crate) fn interceptor(
    hook: impl Fn(&str) -> Result<(), Status> + Clone + Send + Sync + 'static,
) -> impl FnMut(Request<()>) -> Result<Request<()>, Status> + Clone + Send + Sync + 'static {
    move |request: Request<()>| {
        let path = request
            .extensions()
            .get::<RpcPath>()
            .map_or("", |path| path.0.as_str());
        hook(path)?;
        Ok(request)
    }
}
