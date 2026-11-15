//! Packet simulation: an ICMP echo walked hop by hop through the topology.
//!
//! Layer 2: switches flood to every linked port (loop-safe), an "ARP" succeeds
//! when a reachable interface owns the next-hop address.
//! Layer 3: hosts use their subnet or default gateway; routers pick the longest
//! matching prefix among connected networks and static routes.

use crate::model::{DeviceId, Endpoint, Topology};
use std::collections::{HashSet, VecDeque};
use std::net::Ipv4Addr;

pub const INITIAL_TTL: u8 = 64;

#[derive(Debug, Clone, PartialEq)]
pub struct PingResult {
    pub success: bool,
    /// Human-readable trace, one line per event.
    pub lines: Vec<String>,
    /// Devices visited in order (request then reply), for the animation.
    pub path: Vec<DeviceId>,
    /// Number of entries of `path` that belong to the request; the rest is the reply.
    pub request_len: usize,
}

/// One direction of travel.
#[derive(Debug)]
struct Leg {
    path: Vec<DeviceId>,
    src_ip: Ipv4Addr,
    ttl: u8,
}

#[derive(Debug)]
struct Failure {
    message: String,
    path: Vec<DeviceId>,
}

pub fn ping(topo: &Topology, src: DeviceId, dst: Ipv4Addr) -> PingResult {
    let Some(src_dev) = topo.device(src) else {
        return failed(vec![], "Unknown source device.".into());
    };
    let mut lines = vec![format!("Pinging {dst} from {}", src_dev.name)];

    if src_dev.kind.is_switch() {
        lines.push("A switch has no IP address. Ping from a PC, server or router.".into());
        return PingResult {
            success: false,
            lines,
            path: vec![],
            request_len: 0,
        };
    }

    let request = match deliver(topo, src, dst) {
        Ok(leg) => leg,
        Err(f) => {
            lines.push(format!("Request failed: {}", f.message));
            let request_len = f.path.len();
            return PingResult {
                success: false,
                lines,
                path: f.path,
                request_len,
            };
        }
    };
    let target = *request.path.last().expect("path starts with the source");
    let target_name = name(topo, target);

    let reply = match deliver(topo, target, request.src_ip) {
        Ok(leg) => leg,
        Err(f) => {
            lines.push(format!(
                "Request reached {target_name}, but the reply was lost: {}",
                f.message
            ));
            let request_len = request.path.len();
            let mut path = request.path;
            path.extend(f.path.into_iter().skip(1));
            return PingResult {
                success: false,
                lines,
                path,
                request_len,
            };
        }
    };

    let routers = INITIAL_TTL - request.ttl;
    lines.push(format!(
        "Reply from {dst} ({target_name}): routers crossed={routers} ttl={}",
        reply.ttl
    ));
    let request_len = request.path.len();
    let mut path = request.path;
    path.extend(reply.path.into_iter().skip(1));
    PingResult {
        success: true,
        lines,
        path,
        request_len,
    }
}

fn failed(path: Vec<DeviceId>, message: String) -> PingResult {
    let request_len = path.len();
    PingResult {
        success: false,
        lines: vec![message],
        path,
        request_len,
    }
}

fn name(topo: &Topology, id: DeviceId) -> String {
    topo.device(id)
        .map_or_else(|| "?".into(), |d| d.name.clone())
}

/// Carry a packet from `from` to whichever device owns `dst`.
fn deliver(topo: &Topology, from: DeviceId, dst: Ipv4Addr) -> Result<Leg, Failure> {
    let mut path = vec![from];
    let mut current = from;
    let mut ttl = INITIAL_TTL;
    let mut src_ip = None;

    loop {
        let fail = |message: String, path: &Vec<DeviceId>| Failure {
            message,
            path: path.clone(),
        };
        let dev = topo
            .device(current)
            .ok_or_else(|| fail("device vanished".into(), &path))?;

        if dev.owns_ip(dst) {
            let src_ip = src_ip.unwrap_or(dst);
            return Ok(Leg { path, src_ip, ttl });
        }
        let forwarding = path.len() > 1;
        if forwarding && dev.kind.is_host() {
            return Err(fail(
                format!(
                    "{} received a packet for {dst} but is not a router.",
                    dev.name
                ),
                &path,
            ));
        }
        if forwarding {
            ttl -= 1;
            if ttl == 0 {
                return Err(fail(
                    format!("TTL expired at {} (routing loop?).", dev.name),
                    &path,
                ));
            }
        }

        let (iface, next_hop) = next_hop(topo, current, dst).map_err(|m| fail(m, &path))?;
        if src_ip.is_none() {
            src_ip = dev.interfaces[iface].ip.map(|c| c.addr);
        }
        let ep = Endpoint {
            device: current,
            iface,
        };
        let (next, segment) = resolve(topo, ep, next_hop).ok_or_else(|| {
            fail(
                format!(
                    "{} can't reach {next_hop} through {} (host unreachable).",
                    dev.name, dev.interfaces[iface].name
                ),
                &path,
            )
        })?;
        path.extend(segment);
        current = next;
    }
}

/// Layer-3 decision: which interface to send on and which address to "ARP" for.
fn next_hop(topo: &Topology, id: DeviceId, dst: Ipv4Addr) -> Result<(usize, Ipv4Addr), String> {
    let dev = topo.device(id).ok_or("unknown device")?;
    let up = |i: usize| {
        topo.is_linked(Endpoint {
            device: id,
            iface: i,
        })
    };

    if dev.kind.is_host() {
        let nic = &dev.interfaces[0];
        let cfg = nic
            .ip
            .ok_or_else(|| format!("{} has no IP address on {}.", dev.name, nic.name))?;
        if !up(0) {
            return Err(format!("{} {} is not connected.", dev.name, nic.name));
        }
        if cfg.contains(dst) {
            return Ok((0, dst));
        }
        let gw = dev.gateway.ok_or_else(|| {
            format!(
                "{dst} is outside {}'s subnet and no default gateway is set.",
                dev.name
            )
        })?;
        if !cfg.contains(gw) {
            return Err(format!(
                "{}'s gateway {gw} is outside its own subnet.",
                dev.name
            ));
        }
        return Ok((0, gw));
    }

    if dev.kind.is_router() {
        // (prefix, is_connected, iface, next hop): longest prefix wins, connected beats static on ties.
        let mut best: Option<(u8, bool, usize, Ipv4Addr)> = None;
        let mut consider = |cand: (u8, bool, usize, Ipv4Addr)| {
            if best.is_none_or(|b| (cand.0, cand.1) > (b.0, b.1)) {
                best = Some(cand);
            }
        };
        for (i, iface) in dev.interfaces.iter().enumerate() {
            if let Some(cfg) = iface.ip.filter(|c| up(i) && c.contains(dst)) {
                consider((cfg.prefix, true, i, dst));
            }
        }
        for route in dev.routes.iter().filter(|r| r.matches(dst)) {
            let exit =
                dev.interfaces.iter().enumerate().find(|(i, iface)| {
                    up(*i) && iface.ip.is_some_and(|c| c.contains(route.next_hop))
                });
            if let Some((i, _)) = exit {
                consider((route.prefix, false, i, route.next_hop));
            }
        }
        return best
            .map(|(_, _, i, nh)| (i, nh))
            .ok_or_else(|| format!("{} has no route to {dst}.", dev.name));
    }

    Err(format!("{} can't route packets.", dev.name))
}

/// Layer-2 delivery: flood from `start` through switches until an interface owning
/// `target` is found. Returns that device and the devices crossed (switches, then it).
fn resolve(
    topo: &Topology,
    start: Endpoint,
    target: Ipv4Addr,
) -> Option<(DeviceId, Vec<DeviceId>)> {
    let mut seen_switches = HashSet::new();
    let mut queue = VecDeque::new();
    let (_, link) = topo.link_at(start)?;
    queue.push_back((link.other(start)?, Vec::new()));

    while let Some((ep, mut path)) = queue.pop_front() {
        let dev = topo.device(ep.device)?;
        path.push(dev.id);
        if dev.kind.is_switch() {
            if !seen_switches.insert(dev.id) {
                continue;
            }
            for port in (0..dev.interfaces.len()).filter(|p| *p != ep.iface) {
                let out = Endpoint {
                    device: dev.id,
                    iface: port,
                };
                if let Some(peer) = topo.link_at(out).and_then(|(_, l)| l.other(out)) {
                    queue.push_back((peer, path.clone()));
                }
            }
        } else if dev
            .interfaces
            .get(ep.iface)?
            .ip
            .is_some_and(|c| c.addr == target)
        {
            return Some((dev.id, path));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DeviceKind, IpConfig, Route};

    fn ip(s: &str) -> Ipv4Addr {
        s.parse().unwrap()
    }

    fn set_ip(t: &mut Topology, id: DeviceId, iface: usize, addr: &str, prefix: u8) {
        t.device_mut(id).unwrap().interfaces[iface].ip = Some(IpConfig::new(ip(addr), prefix));
    }

    /// PC1 — SW1 — R1 — SW2 — PC2, networks 10.0.1.0/24 and 10.0.2.0/24.
    fn two_lans() -> (Topology, DeviceId, DeviceId, DeviceId) {
        let mut t = Topology::default();
        let pc1 = t.add_device(DeviceKind::Pc, [0.0; 2]);
        let sw1 = t.add_device(DeviceKind::Switch, [0.0; 2]);
        let r1 = t.add_device(DeviceKind::Router, [0.0; 2]);
        let sw2 = t.add_device(DeviceKind::Switch, [0.0; 2]);
        let pc2 = t.add_device(DeviceKind::Pc, [0.0; 2]);
        t.connect(pc1, sw1).unwrap();
        t.connect(sw1, r1).unwrap(); // R1 Gi0/0
        t.connect(r1, sw2).unwrap(); // R1 Gi0/1
        t.connect(sw2, pc2).unwrap();
        set_ip(&mut t, pc1, 0, "10.0.1.10", 24);
        set_ip(&mut t, pc2, 0, "10.0.2.10", 24);
        set_ip(&mut t, r1, 0, "10.0.1.1", 24);
        set_ip(&mut t, r1, 1, "10.0.2.1", 24);
        t.device_mut(pc1).unwrap().gateway = Some(ip("10.0.1.1"));
        t.device_mut(pc2).unwrap().gateway = Some(ip("10.0.2.1"));
        (t, pc1, r1, pc2)
    }

    #[test]
    fn same_subnet_through_switch() {
        let (mut t, pc1, _, _) = two_lans();
        let pc3 = t.add_device(DeviceKind::Pc, [0.0; 2]);
        let sw1 = t.devices[1].id;
        t.connect(pc3, sw1).unwrap();
        set_ip(&mut t, pc3, 0, "10.0.1.20", 24);
        let r = ping(&t, pc1, ip("10.0.1.20"));
        assert!(r.success, "{:?}", r.lines);
        assert_eq!(r.path, vec![pc1, sw1, pc3, sw1, pc1]);
    }

    #[test]
    fn across_a_router() {
        let (t, pc1, r1, pc2) = two_lans();
        let r = ping(&t, pc1, ip("10.0.2.10"));
        assert!(r.success, "{:?}", r.lines);
        assert!(r.path.contains(&r1));
        assert_eq!(r.path[4], pc2);
        assert!(r.lines[1].contains("ttl=63"));
    }

    #[test]
    fn missing_gateway() {
        let (mut t, pc1, _, _) = two_lans();
        t.device_mut(pc1).unwrap().gateway = None;
        let r = ping(&t, pc1, ip("10.0.2.10"));
        assert!(!r.success);
        assert!(r.lines[1].contains("no default gateway"), "{:?}", r.lines);
    }

    #[test]
    fn reply_lost_without_remote_gateway() {
        let (mut t, pc1, _, pc2) = two_lans();
        t.device_mut(pc2).unwrap().gateway = None;
        let r = ping(&t, pc1, ip("10.0.2.10"));
        assert!(!r.success);
        assert!(r.lines[1].contains("reply was lost"), "{:?}", r.lines);
    }

    #[test]
    fn wrong_mask_breaks_reachability() {
        let (mut t, pc1, _, _) = two_lans();
        // PC1 now thinks the router is off-subnet.
        set_ip(&mut t, pc1, 0, "10.0.1.10", 30);
        t.device_mut(pc1).unwrap().gateway = Some(ip("10.0.1.1"));
        let r = ping(&t, pc1, ip("10.0.2.10"));
        assert!(!r.success);
        assert!(
            r.lines[1].contains("outside its own subnet"),
            "{:?}",
            r.lines
        );
    }

    #[test]
    fn unknown_host_is_unreachable() {
        let (t, pc1, _, _) = two_lans();
        let r = ping(&t, pc1, ip("10.0.1.99"));
        assert!(!r.success);
        assert!(r.lines[1].contains("unreachable"), "{:?}", r.lines);
    }

    #[test]
    fn static_routes_and_loop_detection() {
        // PC1 — R1 — R2 ; 192.168.0.0/16 routed R1 → R2 → R1 …
        let mut t = Topology::default();
        let pc = t.add_device(DeviceKind::Pc, [0.0; 2]);
        let r1 = t.add_device(DeviceKind::Router, [0.0; 2]);
        let r2 = t.add_device(DeviceKind::Router, [0.0; 2]);
        t.connect(pc, r1).unwrap();
        t.connect(r1, r2).unwrap();
        set_ip(&mut t, pc, 0, "10.0.0.2", 24);
        set_ip(&mut t, r1, 0, "10.0.0.1", 24);
        set_ip(&mut t, r1, 1, "172.16.0.1", 30);
        set_ip(&mut t, r2, 0, "172.16.0.2", 30);
        t.device_mut(pc).unwrap().gateway = Some(ip("10.0.0.1"));
        let to = |nh: &str| Route {
            dest: ip("192.168.0.0"),
            prefix: 16,
            next_hop: ip(nh),
        };
        t.device_mut(r1).unwrap().routes.push(to("172.16.0.2"));
        t.device_mut(r2).unwrap().routes.push(to("172.16.0.1"));

        // Reaching R2 itself needs a return route on R2.
        let r = ping(&t, pc, ip("172.16.0.2"));
        assert!(!r.success, "{:?}", r.lines);
        t.device_mut(r2).unwrap().routes.push(Route {
            dest: ip("0.0.0.0"),
            prefix: 0,
            next_hop: ip("172.16.0.1"),
        });
        assert!(ping(&t, pc, ip("172.16.0.2")).success);

        let r = ping(&t, pc, ip("192.168.1.1"));
        assert!(!r.success);
        assert!(r.lines[1].contains("TTL expired"), "{:?}", r.lines);
    }

    #[test]
    fn switch_cannot_ping_and_loops_are_safe() {
        let mut t = Topology::default();
        let a = t.add_device(DeviceKind::Switch, [0.0; 2]);
        let b = t.add_device(DeviceKind::Switch, [0.0; 2]);
        let c = t.add_device(DeviceKind::Switch, [0.0; 2]);
        let pc = t.add_device(DeviceKind::Pc, [0.0; 2]);
        t.connect(a, b).unwrap();
        t.connect(b, c).unwrap();
        t.connect(c, a).unwrap();
        t.connect(pc, a).unwrap();
        set_ip(&mut t, pc, 0, "10.0.0.1", 24);
        assert!(!ping(&t, a, ip("10.0.0.1")).success);
        let r = ping(&t, pc, ip("10.0.0.2"));
        assert!(!r.success && r.lines[1].contains("unreachable"));
    }
}
