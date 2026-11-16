//! Topology data model: devices, interfaces, links. Pure data, no UI.

use crate::i18n::tr;
use crate::tf;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;

pub type DeviceId = u32;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum DeviceKind {
    Pc,
    Server,
    Switch,
    Router,
    Cloud,
}

impl DeviceKind {
    pub const ALL: [DeviceKind; 5] = [
        DeviceKind::Pc,
        DeviceKind::Server,
        DeviceKind::Switch,
        DeviceKind::Router,
        DeviceKind::Cloud,
    ];

    /// Kind name in the current language; also the prefix of auto-generated
    /// names ("PC1", "Routeur2"…).
    pub fn label(self) -> &'static str {
        match self {
            DeviceKind::Pc => "PC",
            DeviceKind::Server => tr("Serveur", "Server"),
            DeviceKind::Switch => "Switch",
            DeviceKind::Router => tr("Routeur", "Router"),
            DeviceKind::Cloud => "Internet",
        }
    }

    /// "un routeur" / "a router", for sentences.
    pub fn with_article(self) -> &'static str {
        match self {
            DeviceKind::Pc => tr("un PC", "a PC"),
            DeviceKind::Server => tr("un serveur", "a server"),
            DeviceKind::Switch => tr("un switch", "a switch"),
            DeviceKind::Router => tr("un routeur", "a router"),
            DeviceKind::Cloud => tr("un nuage Internet", "an Internet cloud"),
        }
    }

    /// End hosts: one NIC, a default gateway, never forward packets.
    pub fn is_host(self) -> bool {
        matches!(self, DeviceKind::Pc | DeviceKind::Server)
    }

    /// Layer-3 forwarders: several routed interfaces and a static route table.
    pub fn is_router(self) -> bool {
        matches!(self, DeviceKind::Router | DeviceKind::Cloud)
    }

    /// Layer-2 only: ports carry no IP address.
    pub fn is_switch(self) -> bool {
        matches!(self, DeviceKind::Switch)
    }

    fn default_interfaces(self) -> Vec<Interface> {
        let names: Vec<String> = match self {
            DeviceKind::Pc | DeviceKind::Server => vec!["eth0".into()],
            DeviceKind::Switch => (1..=8).map(|i| format!("Fa0/{i}")).collect(),
            DeviceKind::Router => (0..4).map(|i| format!("Gi0/{i}")).collect(),
            DeviceKind::Cloud => (0..4).map(|i| format!("wan{i}")).collect(),
        };
        names.into_iter().map(Interface::new).collect()
    }
}

/// An IPv4 address with its prefix length (10.0.0.1/24).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct IpConfig {
    pub addr: Ipv4Addr,
    pub prefix: u8,
}

impl IpConfig {
    pub fn new(addr: Ipv4Addr, prefix: u8) -> Self {
        Self {
            addr,
            prefix: prefix.min(32),
        }
    }

    pub fn network(&self) -> Ipv4Addr {
        network(self.addr, self.prefix)
    }

    pub fn contains(&self, ip: Ipv4Addr) -> bool {
        network(ip, self.prefix) == self.network()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Interface {
    pub name: String,
    #[serde(default)]
    pub ip: Option<IpConfig>,
}

impl Interface {
    pub fn new(name: String) -> Self {
        Self { name, ip: None }
    }
}

/// Static route: packets for `dest/prefix` are sent to `next_hop`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Route {
    pub dest: Ipv4Addr,
    pub prefix: u8,
    pub next_hop: Ipv4Addr,
}

impl Route {
    pub fn matches(&self, ip: Ipv4Addr) -> bool {
        network(ip, self.prefix) == network(self.dest, self.prefix)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Device {
    pub id: DeviceId,
    pub kind: DeviceKind,
    pub name: String,
    pub pos: [f32; 2],
    pub interfaces: Vec<Interface>,
    /// Default gateway (hosts only).
    #[serde(default)]
    pub gateway: Option<Ipv4Addr>,
    /// Static routes (routers and Internet only).
    #[serde(default)]
    pub routes: Vec<Route>,
}

impl Device {
    pub fn owns_ip(&self, ip: Ipv4Addr) -> bool {
        self.interfaces
            .iter()
            .any(|i| i.ip.is_some_and(|c| c.addr == ip))
    }
}

/// One side of a link: a device and the index of one of its interfaces.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Endpoint {
    pub device: DeviceId,
    pub iface: usize,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Link {
    pub a: Endpoint,
    pub b: Endpoint,
}

impl Link {
    pub fn other(&self, ep: Endpoint) -> Option<Endpoint> {
        if self.a == ep {
            Some(self.b)
        } else if self.b == ep {
            Some(self.a)
        } else {
            None
        }
    }

    pub fn touches(&self, device: DeviceId) -> bool {
        self.a.device == device || self.b.device == device
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    SameDevice,
    NoFreePort(String),
    AlreadyLinked,
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            LinkError::SameDevice => tr(
                "Un appareil ne peut pas être relié à lui-même.",
                "A device can't be linked to itself.",
            )
            .to_owned(),
            LinkError::NoFreePort(name) => {
                tf!(
                    "{name} n'a plus de port libre.",
                    "{name} has no free port left."
                )
            }
            LinkError::AlreadyLinked => tr(
                "Ces appareils sont déjà reliés.",
                "These devices are already linked.",
            )
            .to_owned(),
        };
        f.write_str(&text)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Topology {
    pub devices: Vec<Device>,
    pub links: Vec<Link>,
    #[serde(default)]
    next_id: DeviceId,
}

impl Topology {
    pub fn device(&self, id: DeviceId) -> Option<&Device> {
        self.devices.iter().find(|d| d.id == id)
    }

    pub fn device_mut(&mut self, id: DeviceId) -> Option<&mut Device> {
        self.devices.iter_mut().find(|d| d.id == id)
    }

    pub fn add_device(&mut self, kind: DeviceKind, pos: [f32; 2]) -> DeviceId {
        // Keep ids unique even for files written by hand without `next_id`.
        let max_id = self.devices.iter().map(|d| d.id + 1).max().unwrap_or(0);
        let id = self.next_id.max(max_id);
        self.next_id = id + 1;
        let name = self.unique_name(kind.label());
        self.devices.push(Device {
            id,
            kind,
            name,
            pos,
            interfaces: kind.default_interfaces(),
            gateway: None,
            routes: Vec::new(),
        });
        id
    }

    fn unique_name(&self, prefix: &str) -> String {
        (1..)
            .map(|n| format!("{prefix}{n}"))
            .find(|n| !self.devices.iter().any(|d| &d.name == n))
            .expect("infinite iterator")
    }

    pub fn remove_device(&mut self, id: DeviceId) {
        self.devices.retain(|d| d.id != id);
        self.links.retain(|l| !l.touches(id));
    }

    pub fn link_at(&self, ep: Endpoint) -> Option<(usize, &Link)> {
        self.links
            .iter()
            .enumerate()
            .find(|(_, l)| l.a == ep || l.b == ep)
    }

    pub fn is_linked(&self, ep: Endpoint) -> bool {
        self.link_at(ep).is_some()
    }

    pub fn free_port(&self, device: DeviceId) -> Option<Endpoint> {
        let dev = self.device(device)?;
        (0..dev.interfaces.len())
            .map(|iface| Endpoint { device, iface })
            .find(|ep| !self.is_linked(*ep))
    }

    /// Link two devices on their first free ports.
    pub fn connect(&mut self, a: DeviceId, b: DeviceId) -> Result<usize, LinkError> {
        if a == b {
            return Err(LinkError::SameDevice);
        }
        if self.links.iter().any(|l| l.touches(a) && l.touches(b)) {
            return Err(LinkError::AlreadyLinked);
        }
        let name = |id| self.device(id).map(|d| d.name.clone()).unwrap_or_default();
        let ea = self
            .free_port(a)
            .ok_or_else(|| LinkError::NoFreePort(name(a)))?;
        let eb = self
            .free_port(b)
            .ok_or_else(|| LinkError::NoFreePort(name(b)))?;
        self.links.push(Link { a: ea, b: eb });
        Ok(self.links.len() - 1)
    }

    pub fn iface_name(&self, ep: Endpoint) -> &str {
        self.device(ep.device)
            .and_then(|d| d.interfaces.get(ep.iface))
            .map_or("?", |i| i.name.as_str())
    }
}

// ── IPv4 helpers ────────────────────────────────────────────────────────────

pub fn prefix_to_mask(prefix: u8) -> Ipv4Addr {
    let p = prefix.min(32) as u32;
    let bits = if p == 0 { 0 } else { u32::MAX << (32 - p) };
    Ipv4Addr::from(bits)
}

/// Dotted mask → prefix length; `None` if the mask is not contiguous.
pub fn mask_to_prefix(mask: Ipv4Addr) -> Option<u8> {
    let bits = u32::from(mask);
    let ones = bits.leading_ones();
    (bits.checked_shl(ones).unwrap_or(0) == 0).then_some(ones as u8)
}

/// Accepts "255.255.255.0", "/24" or "24".
pub fn parse_mask(s: &str) -> Option<u8> {
    let s = s.trim();
    let digits = s.strip_prefix('/').unwrap_or(s);
    if let Ok(p) = digits.parse::<u8>() {
        return (p <= 32).then_some(p);
    }
    mask_to_prefix(s.parse().ok()?)
}

pub fn network(ip: Ipv4Addr, prefix: u8) -> Ipv4Addr {
    Ipv4Addr::from(u32::from(ip) & u32::from(prefix_to_mask(prefix)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_round_trip() {
        for p in 0..=32u8 {
            assert_eq!(mask_to_prefix(prefix_to_mask(p)), Some(p));
        }
        assert_eq!(mask_to_prefix("255.0.255.0".parse().unwrap()), None);
        assert_eq!(parse_mask("255.255.255.0"), Some(24));
        assert_eq!(parse_mask("/16"), Some(16));
        assert_eq!(parse_mask("8"), Some(8));
        assert_eq!(parse_mask("33"), None);
        assert_eq!(parse_mask("abc"), None);
    }

    #[test]
    fn connect_uses_free_ports_and_rejects_duplicates() {
        let mut t = Topology::default();
        let pc = t.add_device(DeviceKind::Pc, [0.0, 0.0]);
        let sw = t.add_device(DeviceKind::Switch, [0.0, 0.0]);
        let pc2 = t.add_device(DeviceKind::Pc, [0.0, 0.0]);
        t.connect(pc, sw).unwrap();
        assert_eq!(t.connect(pc, sw), Err(LinkError::AlreadyLinked));
        assert!(matches!(t.connect(pc, pc2), Err(LinkError::NoFreePort(_))));
        t.connect(pc2, sw).unwrap();
        assert_eq!(t.links[1].b.iface, 1);
        assert_eq!(t.device(pc2).unwrap().name, "PC2");
        t.remove_device(sw);
        assert!(t.links.is_empty());
    }
}
