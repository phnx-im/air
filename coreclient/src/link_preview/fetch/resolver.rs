// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use ipnet::{Ipv4Net, Ipv6Net};
use reqwest::dns::{Addrs, Name, Resolve};

/// Resolves with the system resolver and drops non-public addresses.
#[derive(Debug, Clone, Copy)]
pub(super) struct PublicResolver;

impl Resolve for PublicResolver {
    fn resolve(&self, name: Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            // The port is replaced by the one from the url
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((name.as_str(), 0))
                .await?
                .filter(|addr| is_public(addr.ip()))
                .collect();
            if addrs.is_empty() {
                return Err("no public address".into());
            }
            let addrs: Addrs = Box::new(addrs.into_iter());
            Ok(addrs)
        })
    }
}

fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => !BLOCKED_V4.iter().any(|net| net.contains(&ip)),
        IpAddr::V6(ip) => match EMBEDDING_V4.iter().find(|(net, _)| net.contains(&ip)) {
            Some(&(_, shift)) => {
                let v4 = Ipv4Addr::from((u128::from(ip) >> shift) as u32);
                is_public(IpAddr::V4(v4))
            }
            None => GLOBAL_UNICAST.contains(&ip) && !BLOCKED_V6.iter().any(|net| net.contains(&ip)),
        },
    }
}

/// Non-public IPv4 ranges
///
/// See <https://www.iana.org/assignments/iana-ipv4-special-registry>.
const BLOCKED_V4: [Ipv4Net; 15] = [
    v4([0, 0, 0, 0], 8),
    v4([10, 0, 0, 0], 8),
    v4([100, 64, 0, 0], 10),
    v4([127, 0, 0, 0], 8),
    v4([169, 254, 0, 0], 16),
    v4([172, 16, 0, 0], 12),
    v4([192, 0, 0, 0], 24),
    v4([192, 0, 2, 0], 24), // TEST-NET-1
    v4([192, 168, 0, 0], 16),
    v4([192, 88, 99, 0], 24), // 6to4 relay anycast
    v4([198, 18, 0, 0], 15),
    v4([198, 51, 100, 0], 24), // TEST-NET-2
    v4([203, 0, 113, 0], 24),  // TEST-NET-3
    v4([224, 0, 0, 0], 4),
    v4([240, 0, 0, 0], 4),
];

/// Public IPv6 space, all else is special or unassigned.
const GLOBAL_UNICAST: Ipv6Net = v6([0x2000, 0, 0, 0, 0, 0, 0, 0], 3);

/// Special ranges inside global unicast.
///
/// See <https://www.iana.org/assignments/iana-ipv6-special-registry>.
const BLOCKED_V6: [Ipv6Net; 3] = [
    // IETF protocol assignments
    v6([0x2001, 0, 0, 0, 0, 0, 0, 0], 23),
    // Documentation
    v6([0x2001, 0xdb8, 0, 0, 0, 0, 0, 0], 32),
    v6([0x3fff, 0, 0, 0, 0, 0, 0, 0], 20),
];

/// IPv6 ranges with an IPv4 address inside, and where it starts in the address, counted in bits
/// from the right.
const EMBEDDING_V4: [(Ipv6Net, u32); 3] = [
    // NAT64, from DNS64 on IPv6-only networks
    (v6([0x64, 0xff9b, 0, 0, 0, 0, 0, 0], 96), 0),
    (v6([0x64, 0xff9b, 1, 0, 0, 0, 0, 0], 48), 0),
    // 6to4
    (v6([0x2002, 0, 0, 0, 0, 0, 0, 0], 16), 80),
];

const fn v4([a, b, c, d]: [u8; 4], len: u8) -> Ipv4Net {
    Ipv4Net::new_assert(Ipv4Addr::new(a, b, c, d), len)
}

const fn v6([a, b, c, d, e, f, g, h]: [u16; 8], len: u8) -> Ipv6Net {
    Ipv6Net::new_assert(Ipv6Addr::new(a, b, c, d, e, f, g, h), len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_public(ips: &[&str], public: bool) {
        for ip in ips {
            let addr: IpAddr = ip.parse().unwrap();
            assert_eq!(is_public(addr), public, "{ip}");
        }
    }

    #[test]
    fn blocked_ipv4() {
        assert_public(
            &[
                "0.0.0.0",
                "0.255.255.255",
                "10.0.0.1",
                "10.255.255.255",
                "100.64.0.0",
                "100.127.255.255",
                "127.0.0.1",
                "169.254.169.254",
                "172.16.0.0",
                "172.31.255.255",
                "192.0.0.8",
                "192.0.2.1",
                "192.88.99.1",
                "192.168.1.1",
                "198.18.0.1",
                "198.19.255.255",
                "198.51.100.1",
                "203.0.113.1",
                "224.0.0.1",
                "239.255.255.255",
                "240.0.0.1",
                "255.255.255.255",
            ],
            false,
        );
    }

    #[test]
    fn public_ipv4() {
        assert_public(
            &[
                "1.1.1.1",
                "8.8.8.8",
                "9.255.255.255",
                "11.0.0.0",
                "100.63.255.255",
                "100.128.0.0",
                "126.255.255.255",
                "128.0.0.0",
                "169.253.255.255",
                "172.15.255.255",
                "172.32.0.0",
                "192.0.1.1",
                "192.88.98.255",
                "192.167.255.255",
                "192.169.0.0",
                "198.17.255.255",
                "198.20.0.0",
                "223.255.255.255",
            ],
            true,
        );
    }

    #[test]
    fn blocked_ipv6() {
        assert_public(
            &[
                "::",
                "::1",
                // IPv4-compatible and IPv4-mapped are outside global unicast
                "::a00:1",
                "::ffff:1.1.1.1",
                "::ffff:127.0.0.1",
                "100::1",
                "5f00::1",
                "fc00::1",
                "fd12:3456::1",
                "fe80::1",
                "fec0::1",
                "ff02::1",
                // Teredo, benchmarking, documentation
                "2001::1",
                "2001:2::1",
                "2001:1ff:ffff::1",
                "2001:db8::1",
                "3fff::1",
                // NAT64 and 6to4 with a private IPv4 inside
                "64:ff9b::a00:1",
                "64:ff9b::7f00:1",
                "64:ff9b:1::c0a8:101",
                "2002:a00:1::",
                "2002:c0a8:101::",
            ],
            false,
        );
    }

    #[test]
    fn public_ipv6() {
        assert_public(
            &[
                "2606:4700:4700::1111",
                "2a00:1450:4001:80b::200e",
                "2001:200::1",
                "2001:4860:4860::8888",
                "3ffe::1",
                // NAT64 and 6to4 with a public IPv4 inside
                "64:ff9b::101:101",
                "64:ff9b:1::101:101",
                "2002:101:101::",
            ],
            true,
        );
    }
}
