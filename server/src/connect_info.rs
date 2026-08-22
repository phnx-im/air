// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::net::IpAddr;

use airbackend::client_ip::ClientIp;
use tonic::{Request, Status, metadata::MetadataMap, service::Interceptor};

/// Resolves the client address once per request, for the rate limiter's key
/// extractor and for handlers that gate on it.
#[derive(Debug, Clone)]
pub struct ConnectInfoInterceptor;

/// The one header our ingress controller overwrites, so it cannot be forged.
/// `X-Forwarded-For` and `Forwarded` arrive attacker-controlled.
const REAL_IP_HEADER: &str = "x-real-ip";

impl Interceptor for ConnectInfoInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        if let Some(client_ip) = resolve_client_ip(&request) {
            request.extensions_mut().insert(client_ip);
        }

        let metadata = request.metadata();
        if metadata.contains_key(REAL_IP_HEADER)
            || metadata.contains_key("x-forwarded-for")
            || metadata.contains_key("forwarded")
        {
            return Ok(request);
        }
        // fallback to remote_addr: this won't work behind a reverse proxy
        let addr = request
            .remote_addr()
            .ok_or_else(|| Status::internal("failed to extract remote address from request"))?;
        request.extensions_mut().insert(addr);
        Ok(request)
    }
}

/// The client address, from `X-Real-IP` where the ingress set it and from the
/// peer address otherwise, which is what local development has.
fn resolve_client_ip(request: &Request<()>) -> Option<ClientIp> {
    real_ip_header(request.metadata())
        .or_else(|| request.remote_addr().map(|addr| addr.ip()))
        .map(ClientIp::new)
}

fn real_ip_header(metadata: &MetadataMap) -> Option<IpAddr> {
    metadata
        .get(REAL_IP_HEADER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

#[cfg(test)]
mod test {
    use std::net::{Ipv4Addr, SocketAddr};

    use super::*;

    fn request_with(header: Option<&str>, peer: Option<SocketAddr>) -> Request<()> {
        let mut request = Request::new(());
        if let Some(header) = header {
            request
                .metadata_mut()
                .insert(REAL_IP_HEADER, header.parse().unwrap());
        }
        if let Some(peer) = peer {
            request
                .extensions_mut()
                .insert(tonic::transport::server::TcpConnectInfo {
                    local_addr: None,
                    remote_addr: Some(peer),
                });
        }
        request
    }

    #[test]
    fn the_header_wins_over_the_peer_address() {
        let peer = SocketAddr::from((Ipv4Addr::new(10, 0, 0, 1), 4242));
        let resolved = resolve_client_ip(&request_with(Some("203.0.113.7"), Some(peer)));
        assert_eq!(
            resolved,
            Some(ClientIp::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7))))
        );
    }

    #[test]
    fn the_peer_address_is_the_fallback() {
        let peer = SocketAddr::from((Ipv4Addr::new(10, 0, 0, 1), 4242));
        let resolved = resolve_client_ip(&request_with(None, Some(peer)));
        assert_eq!(
            resolved,
            Some(ClientIp::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))))
        );
    }

    #[test]
    fn an_unparsable_header_falls_back() {
        let peer = SocketAddr::from((Ipv4Addr::new(10, 0, 0, 1), 4242));
        let resolved = resolve_client_ip(&request_with(Some("not an address"), Some(peer)));
        assert_eq!(
            resolved,
            Some(ClientIp::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))))
        );
    }

    #[test]
    fn no_header_and_no_peer_resolves_to_nothing() {
        assert_eq!(resolve_client_ip(&request_with(None, None)), None);
    }
}
