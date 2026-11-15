//! Which addresses a card fetcher may connect to (media spec 6.3).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// IPv4 ranges never fetched from, as (network, prefix length).
const DENIED_V4: &[(u32, u32)] = &[
    (0x0000_0000, 8),  // 0.0.0.0/8: "this network", the unspecified address
    (0x0a00_0000, 8),  // 10.0.0.0/8: private
    (0x6440_0000, 10), // 100.64.0.0/10: carrier-grade NAT
    (0x7f00_0000, 8),  // 127.0.0.0/8: loopback
    (0xa9fe_0000, 16), // 169.254.0.0/16: link-local, the cloud metadata address among them
    (0xac10_0000, 12), // 172.16.0.0/12: private
    (0xc000_0000, 24), // 192.0.0.0/24: IETF protocol assignments
    (0xc000_0200, 24), // 192.0.2.0/24: documentation
    (0xc058_6300, 24), // 192.88.99.0/24: the retired 6to4 relay anycast
    (0xc0a8_0000, 16), // 192.168.0.0/16: private
    (0xc612_0000, 15), // 198.18.0.0/15: benchmarking
    (0xc633_6400, 24), // 198.51.100.0/24: documentation
    (0xcb00_7100, 24), // 203.0.113.0/24: documentation
    (0xe000_0000, 4),  // 224.0.0.0/4: multicast
    (0xf000_0000, 4),  // 240.0.0.0/4: reserved, the broadcast address among them
];

/// Ranges inside IPv6 global unicast (2000::/3) never fetched from, as (network, prefix
/// length). Everything outside 2000::/3 is refused already: unspecified, loopback,
/// unique-local (fc00::/7, fd00:ec2::254 among them), link-local, multicast, discard-only,
/// local-use NAT64 and the rest.
const DENIED_V6: &[(u128, u32)] = &[
    (0x2001_0000_0000_0000_0000_0000_0000_0000, 32), // Teredo: its IPv4 is obscured
    (0x2001_0002_0000_0000_0000_0000_0000_0000, 48), // benchmarking
    (0x2001_0010_0000_0000_0000_0000_0000_0000, 28), // ORCHID
    (0x2001_0020_0000_0000_0000_0000_0000_0000, 28), // ORCHIDv2
    (0x2001_0db8_0000_0000_0000_0000_0000_0000, 32), // documentation
    (0x3fff_0000_0000_0000_0000_0000_0000_0000, 20), // documentation (RFC 9637)
];

/// Whether a card fetcher may connect to `ip` (6.3): never to a private, loopback,
/// link-local, unique-local, multicast, unspecified, broadcast, documentation, benchmarking
/// or carrier-grade NAT address, nor the cloud metadata address. An IPv6 address that
/// carries an IPv4 one (IPv4-mapped, IPv4-compatible, IPv4-translated, NAT64's
/// 64:ff9b::/96, 6to4's 2002::/16) is judged by that IPv4 address, and an IPv6 address
/// outside global unicast (2000::/3) is refused.
pub fn address_allowed(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4_allowed(v4),
        IpAddr::V6(v6) => v6_allowed(v6),
    }
}

fn v4_allowed(ip: Ipv4Addr) -> bool {
    let n = u32::from(ip);
    !DENIED_V4
        .iter()
        .any(|&(net, len)| (n ^ net).checked_shr(32 - len).unwrap_or(0) == 0)
}

fn v6_allowed(ip: Ipv6Addr) -> bool {
    let n = u128::from(ip);
    if let Some(v4) = embedded_v4(n) {
        return v4_allowed(v4);
    }
    let global = n >> 125 == 0b001;
    global
        && !DENIED_V6
            .iter()
            .any(|&(net, len)| (n ^ net).checked_shr(128 - len).unwrap_or(0) == 0)
}

/// The IPv4 address an IPv6 address carries, if it is one of the forms that carry one.
fn embedded_v4(n: u128) -> Option<Ipv4Addr> {
    // ::/96 IPv4-compatible (:: and ::1 come out as 0.0.0.0 and 0.0.0.1, refused),
    // ::ffff:0:0/96 IPv4-mapped, ::ffff:0:0:0/96 IPv4-translated, and 64:ff9b::/96,
    // NAT64's well-known prefix: the IPv4 address is the last 32 bits.
    let carries = matches!(
        n >> 32,
        0 | 0xffff | 0xffff_0000_0000 | 0x0064_ff9b_0000_0000_0000_0000
    );
    if carries {
        return Some(Ipv4Addr::from((n & 0xffff_ffff) as u32));
    }
    // 2002::/16, 6to4: the IPv4 address is the next 32 bits.
    (n >> 112 == 0x2002).then(|| Ipv4Addr::from(((n >> 80) & 0xffff_ffff) as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(ip: &str) -> bool {
        address_allowed(ip.parse().unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED)))
    }

    #[test]
    fn public_addresses_are_allowed() {
        assert!(allowed("93.184.215.14"));
        assert!(allowed("1.1.1.1"));
        assert!(allowed("2606:4700:4700::1111"));
        assert!(allowed("::ffff:8.8.8.8"));
        assert!(allowed("64:ff9b::8.8.8.8"));
        assert!(allowed("2002:0808:0808::1"));
    }

    #[test]
    fn the_edges_of_a_range_are_refused_and_their_neighbours_allowed() {
        assert!(allowed("100.63.255.255"));
        assert!(!allowed("100.64.0.0"));
        assert!(!allowed("100.127.255.255"));
        assert!(allowed("100.128.0.0"));
        assert!(allowed("172.15.255.255"));
        assert!(!allowed("172.31.255.255"));
        assert!(allowed("172.32.0.0"));
        assert!(allowed("223.255.255.255"));
        assert!(!allowed("255.255.255.255"));
        assert!(!allowed("1fff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"));
        assert!(allowed("2001:db7:ffff:ffff::1"));
        assert!(!allowed("2001:db8::1"));
        assert!(allowed("2001:db9::1"));
    }
}
