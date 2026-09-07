// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The client address a request arrived from.

use std::{fmt, net::IpAddr};

/// The client address a request arrived from.
///
/// The server resolves this once and inserts it as a request extension, so
/// handlers do not have to know which header carried it.
///
/// `Debug` prints no address, so a stray `?client_ip` in a tracing call cannot
/// log one.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ClientIp(IpAddr);

/// The bytes an address is counted under.
///
/// Both families use the 16-byte IPv6 layout, which cannot collide.
pub type IpBucket = [u8; 16];

impl ClientIp {
    pub fn new(addr: IpAddr) -> Self {
        Self(addr.to_canonical())
    }

    /// The bucket this address counts under: the full address for IPv4, the
    /// /64 prefix for IPv6.
    pub fn bucket(&self) -> IpBucket {
        match self.0 {
            IpAddr::V4(addr) => addr.to_ipv6_mapped().octets(),
            IpAddr::V6(addr) => {
                let mut octets = addr.octets();
                octets[8..].fill(0);
                octets
            }
        }
    }
}

impl fmt::Debug for ClientIp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ClientIp(redacted)")
    }
}

#[cfg(test)]
mod test {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use super::*;

    #[test]
    fn ipv4_buckets_per_address() {
        let a = ClientIp::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)));
        let b = ClientIp::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 2)));
        assert_ne!(a.bucket(), b.bucket());
    }

    #[test]
    fn ipv6_buckets_per_64_prefix() {
        let a = ClientIp::new(IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 1, 0, 0, 0, 1)));
        let b = ClientIp::new(IpAddr::V6(Ipv6Addr::new(
            0x2001, 0xdb8, 0, 1, 0xffff, 0xffff, 0xffff, 0xffff,
        )));
        let other_prefix =
            ClientIp::new(IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 2, 0, 0, 0, 1)));

        assert_eq!(a.bucket(), b.bucket());
        assert_ne!(a.bucket(), other_prefix.bucket());
    }

    #[test]
    fn families_do_not_share_buckets() {
        let v4 = ClientIp::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)));
        let v6 = ClientIp::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED));
        assert_ne!(v4.bucket(), v6.bucket());
    }

    #[test]
    fn debug_hides_the_address() {
        let ip = ClientIp::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)));
        assert_eq!(format!("{ip:?}"), "ClientIp(redacted)");
    }

    #[test]
    fn mapped_ipv4_addresses() {
        let a = ClientIp::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)));
        let b = ClientIp::new(IpAddr::V6(Ipv4Addr::new(203, 0, 113, 1).to_ipv6_mapped()));
        assert_eq!(a.bucket(), b.bucket());

        let c = ClientIp::new(IpAddr::V6(Ipv4Addr::new(203, 0, 113, 2).to_ipv6_mapped()));
        assert_ne!(a.bucket(), c.bucket());
    }
}
