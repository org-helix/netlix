//! Built-in example: two LANs behind a router, with an uplink to the Internet.

use crate::model::{DeviceKind, IpConfig, Route, Topology};
use std::net::Ipv4Addr;

fn ip(s: &str) -> Ipv4Addr {
    s.parse().expect("valid literal")
}

pub fn office() -> Topology {
    let mut t = Topology::default();
    let pc1 = t.add_device(DeviceKind::Pc, [80.0, 80.0]);
    let pc2 = t.add_device(DeviceKind::Pc, [80.0, 260.0]);
    let sw1 = t.add_device(DeviceKind::Switch, [260.0, 170.0]);
    let r1 = t.add_device(DeviceKind::Router, [460.0, 170.0]);
    let sw2 = t.add_device(DeviceKind::Switch, [660.0, 170.0]);
    let srv = t.add_device(DeviceKind::Server, [840.0, 170.0]);
    let net = t.add_device(DeviceKind::Cloud, [460.0, -40.0]);

    for (a, b) in [
        (pc1, sw1),
        (pc2, sw1),
        (sw1, r1),
        (r1, sw2),
        (srv, sw2),
        (r1, net),
    ] {
        t.connect(a, b).expect("example ports are free");
    }

    let mut set = |id, iface: usize, addr: &str, prefix| {
        t.device_mut(id).expect("exists").interfaces[iface].ip =
            Some(IpConfig::new(ip(addr), prefix));
    };
    set(pc1, 0, "10.0.1.10", 24);
    set(pc2, 0, "10.0.1.11", 24);
    set(srv, 0, "10.0.2.10", 24);
    set(r1, 0, "10.0.1.1", 24);
    set(r1, 1, "10.0.2.1", 24);
    set(r1, 2, "203.0.113.2", 30);
    set(net, 0, "203.0.113.1", 30);
    set(net, 1, "8.8.8.8", 32); // a public host living "in" the Internet

    for (id, gw) in [(pc1, "10.0.1.1"), (pc2, "10.0.1.1"), (srv, "10.0.2.1")] {
        t.device_mut(id).expect("exists").gateway = Some(ip(gw));
    }
    t.device_mut(r1).expect("exists").routes.push(Route {
        dest: ip("0.0.0.0"),
        prefix: 0,
        next_hop: ip("203.0.113.1"),
    });
    t.device_mut(net).expect("exists").routes.push(Route {
        dest: ip("10.0.0.0"),
        prefix: 8,
        next_hop: ip("203.0.113.2"),
    });
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::ping;

    #[test]
    fn example_pings_work() {
        let t = office();
        let pc1 = t.devices[0].id;
        for dst in ["10.0.1.11", "10.0.2.10", "203.0.113.1", "8.8.8.8"] {
            let r = ping(&t, pc1, ip(dst));
            assert!(r.success, "{dst}: {:?}", r.lines);
        }
    }
}

#[cfg(test)]
mod shipped {
    /// `examples/office.netlix` must stay identical to `office()`.
    /// Regenerate with `NETLIX_WRITE_EXAMPLE=1 cargo test shipped`.
    #[test]
    fn shipped_example_matches() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/office.netlix");
        let text = crate::io::to_string(&super::office());
        if std::env::var_os("NETLIX_WRITE_EXAMPLE").is_some() {
            std::fs::write(path, &text).unwrap();
        }
        let shipped = std::fs::read_to_string(path).unwrap().replace("\r\n", "\n");
        assert_eq!(
            shipped, text,
            "run NETLIX_WRITE_EXAMPLE=1 cargo test shipped"
        );
    }
}
