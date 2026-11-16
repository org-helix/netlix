//! Frame-level simulation of a ping.
//!
//! Every frame that crosses a cable is recorded with the step it travels in and a
//! sentence explaining why it was sent. The model:
//! - every interface has a MAC address derived from its device id and index;
//! - hosts and routers keep an ARP cache; a missing entry triggers a broadcast ARP
//!   request, and the IP packet waits until the reply arrives;
//! - switches learn source MACs per port, forward known unicast and flood broadcasts
//!   and unknown destinations;
//! - hosts deliver on-link or to their default gateway; routers pick the longest
//!   matching prefix among connected networks and static routes, and decrement the TTL.
//!
//! ARP caches and MAC tables live in [`NetState`], which survives between pings.

use crate::i18n::tr;
use crate::model::{DeviceId, Endpoint, Topology};
use crate::tf;
use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::net::Ipv4Addr;

pub const INITIAL_TTL: u8 = 64;
/// Safety stop for switching loops (broadcast storms) and runaway runs.
const MAX_FRAMES: usize = 2000;
const MAX_STEPS: usize = 300;

// ── Frames ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Mac(pub [u8; 6]);

impl Mac {
    pub const BROADCAST: Mac = Mac([0xff; 6]);
    pub const ZERO: Mac = Mac([0; 6]);

    /// Locally administered address, stable across save/load: 02:4e:<device id + 1>:<iface>
    /// (+1 so the first device doesn't get an all-zero-looking address).
    pub fn of(ep: Endpoint) -> Mac {
        let id = ep.device.wrapping_add(1);
        Mac([
            0x02,
            0x4e,
            (id >> 16) as u8,
            (id >> 8) as u8,
            id as u8,
            ep.iface as u8,
        ])
    }

    pub fn is_broadcast(self) -> bool {
        self == Self::BROADCAST
    }
}

impl fmt::Display for Mac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d, e, g] = self.0;
        write!(f, "{a:02x}:{b:02x}:{c:02x}:{d:02x}:{e:02x}:{g:02x}")
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icmp {
    EchoRequest { seq: u16 },
    EchoReply { seq: u16 },
}

impl Icmp {
    pub fn label(self) -> &'static str {
        match self {
            Icmp::EchoRequest { .. } => tr("requête d'écho", "echo request"),
            Icmp::EchoReply { .. } => tr("réponse d'écho", "echo reply"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Packet {
    pub src: Ipv4Addr,
    pub dst: Ipv4Addr,
    pub ttl: u8,
    pub icmp: Icmp,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArpOp {
    Request,
    Reply,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Payload {
    Arp {
        op: ArpOp,
        sender_mac: Mac,
        sender_ip: Ipv4Addr,
        target_mac: Mac,
        target_ip: Ipv4Addr,
    },
    Ip(Packet),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Frame {
    pub src: Mac,
    pub dst: Mac,
    pub payload: Payload,
}

/// What a frame carries, for colour-coding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FrameKind {
    Arp,
    Request,
    Reply,
}

impl Frame {
    pub fn kind(&self) -> FrameKind {
        match self.payload {
            Payload::Arp { .. } => FrameKind::Arp,
            Payload::Ip(Packet {
                icmp: Icmp::EchoRequest { .. },
                ..
            }) => FrameKind::Request,
            Payload::Ip(_) => FrameKind::Reply,
        }
    }

    /// One line, e.g. "ARP request: who has 10.0.1.1? Tell 10.0.1.10".
    pub fn summary(&self) -> String {
        match self.payload {
            Payload::Arp {
                op: ArpOp::Request,
                sender_ip,
                target_ip,
                ..
            } => tf!(
                "Requête ARP : qui a {target_ip} ? Répondre à {sender_ip}",
                "ARP request: who has {target_ip}? Tell {sender_ip}"
            ),
            Payload::Arp {
                op: ArpOp::Reply,
                sender_ip,
                sender_mac,
                ..
            } => tf!(
                "Réponse ARP : {sender_ip} est à {sender_mac}",
                "ARP reply: {sender_ip} is at {sender_mac}"
            ),
            Payload::Ip(p) => format!("ICMP {} {} → {}", p.icmp.label(), p.src, p.dst),
        }
    }

    /// Simplified headers, outermost first: (layer, fields).
    pub fn layers(&self) -> Vec<(&'static str, String)> {
        let ethertype = match self.payload {
            Payload::Arp { .. } => "ARP (0x0806)",
            Payload::Ip(_) => "IPv4 (0x0800)",
        };
        let mut out = vec![(
            "Ethernet",
            format!("dst {}  src {}  type {ethertype}", self.dst, self.src),
        )];
        match self.payload {
            Payload::Arp {
                op,
                sender_mac,
                sender_ip,
                target_mac,
                target_ip,
            } => {
                let op = match op {
                    ArpOp::Request => tr("requête (1)", "request (1)"),
                    ArpOp::Reply => tr("réponse (2)", "reply (2)"),
                };
                out.push((
                    "ARP",
                    tf!(
                        "op {op}  émetteur {sender_ip} {sender_mac}  cible {target_ip} {target_mac}",
                        "op {op}  sender {sender_ip} {sender_mac}  target {target_ip} {target_mac}"
                    ),
                ));
            }
            Payload::Ip(p) => {
                out.push((
                    "IPv4",
                    format!(
                        "src {}  dst {}  ttl {}  proto ICMP (1)",
                        p.src, p.dst, p.ttl
                    ),
                ));
                let (kind, seq) = match p.icmp {
                    Icmp::EchoRequest { seq } => {
                        (tr("requête d'écho (8)", "echo request (8)"), seq)
                    }
                    Icmp::EchoReply { seq } => (tr("réponse d'écho (0)", "echo reply (0)"), seq),
                };
                out.push(("ICMP", format!("type {kind}  id 1  seq {seq}")));
            }
        }
        out
    }
}

// ── State kept between pings ───────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ArpEntry {
    pub mac: Mac,
    /// Interface the entry was learned on.
    pub iface: usize,
}

/// ARP caches of hosts and routers, MAC tables of switches.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetState {
    pub arp: BTreeMap<DeviceId, BTreeMap<Ipv4Addr, ArpEntry>>,
    pub mac_tables: BTreeMap<DeviceId, BTreeMap<Mac, usize>>,
    seq: u16,
}

impl NetState {
    /// Empty one device's ARP cache, or every cache with `None`.
    pub fn clear_arp(&mut self, device: Option<DeviceId>) {
        match device {
            Some(d) => {
                self.arp.remove(&d);
            }
            None => self.arp.clear(),
        }
    }

    /// Empty one switch's MAC table, or every table with `None`.
    pub fn clear_mac_tables(&mut self, device: Option<DeviceId>) {
        match device {
            Some(d) => {
                self.mac_tables.remove(&d);
            }
            None => self.mac_tables.clear(),
        }
    }

    /// Forget entries the topology no longer backs: deleted devices, unplugged
    /// ports, addresses that were changed.
    pub fn prune(&mut self, topo: &Topology) {
        let mut pairs = HashSet::new();
        let mut macs = HashSet::new();
        for dev in &topo.devices {
            for (iface, i) in dev.interfaces.iter().enumerate() {
                let mac = Mac::of(Endpoint {
                    device: dev.id,
                    iface,
                });
                macs.insert(mac);
                if let Some(c) = i.ip {
                    pairs.insert((c.addr, mac));
                }
            }
        }
        let linked = |device, iface| topo.is_linked(Endpoint { device, iface });
        self.arp.retain(|&d, table| {
            table.retain(|ip, e| pairs.contains(&(*ip, e.mac)) && linked(d, e.iface));
            !table.is_empty() && topo.device(d).is_some_and(|x| !x.kind.is_switch())
        });
        self.mac_tables.retain(|&d, table| {
            table.retain(|mac, port| macs.contains(mac) && linked(d, *port));
            !table.is_empty() && topo.device(d).is_some_and(|x| x.kind.is_switch())
        });
    }
}

// ── Run record ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NoteKind {
    Info,
    Success,
    Failure,
}

/// Something a device did or decided, in plain words.
#[derive(Clone, Debug)]
pub struct Note {
    pub step: usize,
    /// Written while the source prepares the ping, before the first frame leaves.
    pub before_frames: bool,
    pub device: Option<DeviceId>,
    pub kind: NoteKind,
    pub text: String,
}

/// One frame crossing one cable during one step.
#[derive(Clone, Debug)]
pub struct Transmission {
    pub step: usize,
    pub from: Endpoint,
    pub to: Endpoint,
    pub frame: Frame,
    /// Why the sender put this frame on the wire.
    pub why: String,
}

#[derive(Clone, Debug)]
pub struct Run {
    pub source: DeviceId,
    pub header: String,
    pub transmissions: Vec<Transmission>,
    pub notes: Vec<Note>,
    /// Number of steps; step `s` holds the frames travelling at the same time.
    pub steps: usize,
    /// Caches and tables after each step's frames were received.
    pub states: Vec<NetState>,
    pub final_state: NetState,
    pub success: bool,
    pub summary: String,
}

impl Run {
    pub fn frames_at(&self, step: usize) -> impl Iterator<Item = (usize, &Transmission)> {
        self.transmissions
            .iter()
            .enumerate()
            .filter(move |(_, t)| t.step == step)
    }

    pub fn notes_at(&self, step: usize) -> impl Iterator<Item = &Note> {
        self.notes.iter().filter(move |n| n.step == step)
    }
}

/// Simulate one ping from `src` to `dst`, starting from `state` (not modified:
/// the caches after the run are in [`Run::final_state`]).
pub fn ping(topo: &Topology, state: &NetState, src: DeviceId, dst: Ipv4Addr) -> Run {
    let mut state = state.clone();
    state.prune(topo);
    state.seq = state.seq.wrapping_add(1);
    let seq = state.seq;
    let src_name = topo.device(src).map_or("?", |d| d.name.as_str());
    let header = tf!(
        "Ping vers {dst} depuis {src_name}",
        "Pinging {dst} from {src_name}"
    );

    let mut e = Engine {
        topo,
        state,
        tx: Vec::new(),
        notes: Vec::new(),
        out_step: 0,
        note_step: 0,
        pending: BTreeMap::new(),
        source: src,
        seq,
        reply: None,
        failure: None,
        originating: true,
    };

    match topo.device(src) {
        None => e.note(
            None,
            NoteKind::Failure,
            tr("Appareil source inconnu.", "Unknown source device.").into(),
        ),
        Some(d) if d.kind.is_switch() => e.note(
            Some(src),
            NoteKind::Failure,
            tr(
                "Un switch n'a pas d'adresse IP. Lancez le ping depuis un PC, un serveur ou un routeur.",
                "A switch has no IP address. Ping from a PC, server or router.",
            )
            .into(),
        ),
        Some(d) if d.owns_ip(dst) => {
            e.note(
                Some(src),
                NoteKind::Success,
                {
                    let name = &d.name;
                    tf!(
                        "{name} possède {dst} : il se répond à lui-même, rien ne passe sur le câble.",
                        "{name} owns {dst}: it answers itself, nothing goes on the wire."
                    )
                },
            );
            e.reply = Some(Packet {
                src: dst,
                dst,
                ttl: INITIAL_TTL,
                icmp: Icmp::EchoReply { seq },
            });
        }
        Some(d) => match next_hop(topo, src, dst) {
            Err(m) => e.note(Some(src), NoteKind::Failure, m),
            Ok((iface, _)) => {
                let src_ip = d.interfaces[iface]
                    .ip
                    .expect("next_hop checks the address")
                    .addr;
                e.note(
                    Some(src),
                    NoteKind::Info,
                    {
                        let name = &d.name;
                        tf!(
                            "{name} construit une requête d'écho ICMP de {src_ip} vers {dst}.",
                            "{name} builds an ICMP echo request from {src_ip} to {dst}."
                        )
                    },
                );
                e.send_ip(
                    src,
                    Packet {
                        src: src_ip,
                        dst,
                        ttl: INITIAL_TTL,
                        icmp: Icmp::EchoRequest { seq },
                    },
                );
            }
        },
    }

    e.originating = false;
    let mut states = Vec::new();
    let mut step = 0;
    loop {
        let batch: Vec<Transmission> = e.tx.iter().filter(|t| t.step == step).cloned().collect();
        if batch.is_empty() {
            break;
        }
        e.out_step = step + 1;
        e.note_step = step;
        for t in &batch {
            e.receive(t);
        }
        states.push(e.state.clone());
        step += 1;
        if e.tx.len() > MAX_FRAMES || step >= MAX_STEPS {
            e.tx.retain(|t| t.step < step);
            e.note(None, NoteKind::Failure, {
                let n = e.tx.len();
                tf!(
                    "Arrêt après {n} trames : elles tournent en rond. Une boucle entre \
                         switchs fait se multiplier les broadcasts à l'infini (tempête de \
                         broadcast), car il n'y a pas de spanning tree ici.",
                    "Stopped after {n} frames: they keep circulating. A loop between \
                         switches makes broadcasts multiply forever (a broadcast storm), since \
                         there is no spanning tree here."
                )
            });
            break;
        }
    }

    let unanswered: Vec<(DeviceId, Ipv4Addr)> = e.pending.keys().copied().collect();
    for (dev, nh) in unanswered {
        let name = e.name(dev);
        let text = tf!(
            "{name} n'a jamais reçu de réponse ARP pour {nh} : personne sur ce réseau ne \
             possède cette adresse (hôte injoignable).",
            "{name} never got an ARP reply for {nh}: nobody on that network owns the address \
             (host unreachable)."
        );
        e.note(Some(dev), NoteKind::Failure, text);
    }

    let success = e.reply.is_some();
    let summary = match e.reply {
        Some(p) => {
            let (ttl, n) = (p.ttl, e.tx.len());
            tf!(
                "Réponse de {dst} : ttl={ttl} · {n} trames en {step} étapes",
                "Reply from {dst}: ttl={ttl} · {n} frames in {step} steps"
            )
        }
        None => e
            .failure
            .clone()
            .unwrap_or_else(|| tr("Délai d'attente dépassé.", "Request timed out.").into()),
    };
    Run {
        source: src,
        header,
        transmissions: e.tx,
        notes: e.notes,
        steps: step,
        states,
        final_state: e.state,
        success,
        summary,
    }
}

struct Engine<'a> {
    topo: &'a Topology,
    state: NetState,
    tx: Vec<Transmission>,
    notes: Vec<Note>,
    /// Step given to frames sent now.
    out_step: usize,
    /// Step given to notes written now.
    note_step: usize,
    /// IP packets waiting for an ARP reply, per (device, next hop).
    pending: BTreeMap<(DeviceId, Ipv4Addr), Vec<(usize, Packet)>>,
    source: DeviceId,
    seq: u16,
    reply: Option<Packet>,
    failure: Option<String>,
    originating: bool,
}

impl Engine<'_> {
    fn name(&self, id: DeviceId) -> String {
        self.topo
            .device(id)
            .map_or_else(|| "?".into(), |d| d.name.clone())
    }

    fn port(&self, ep: Endpoint) -> String {
        self.topo.iface_name(ep).to_owned()
    }

    fn note(&mut self, device: Option<DeviceId>, kind: NoteKind, text: String) {
        if kind == NoteKind::Failure && self.failure.is_none() {
            self.failure = Some(text.clone());
        }
        self.notes.push(Note {
            step: self.note_step,
            before_frames: self.originating,
            device,
            kind,
            text,
        });
    }

    fn emit(&mut self, from: Endpoint, frame: Frame, why: String) {
        match self.topo.link_at(from).and_then(|(_, l)| l.other(from)) {
            Some(to) => self.tx.push(Transmission {
                step: self.out_step,
                from,
                to,
                frame,
                why,
            }),
            None => {
                let (name, port) = (self.name(from.device), self.port(from));
                let text = tf!(
                    "{name} {port} n'est pas connecté.",
                    "{name} {port} is not connected."
                );
                self.note(Some(from.device), NoteKind::Failure, text);
            }
        }
    }

    fn learn(&mut self, dev: DeviceId, ip: Ipv4Addr, mac: Mac, iface: usize) {
        self.state
            .arp
            .entry(dev)
            .or_default()
            .insert(ip, ArpEntry { mac, iface });
    }

    /// Layer 3: route, then use the ARP cache or ask for the next hop's MAC.
    fn send_ip(&mut self, dev: DeviceId, pkt: Packet) {
        let (iface, nh) = match next_hop(self.topo, dev, pkt.dst) {
            Ok(x) => x,
            Err(m) => return self.note(Some(dev), NoteKind::Failure, m),
        };
        let ep = Endpoint { device: dev, iface };
        let (me, port, label) = (self.name(dev), self.port(ep), pkt.icmp.label());
        let dst = pkt.dst;
        let via = if nh == dst {
            String::new()
        } else {
            tf!(" via le prochain saut {nh}", " through next hop {nh}")
        };
        let cached = self.state.arp.get(&dev).and_then(|t| t.get(&nh)).copied();
        if let Some(entry) = cached {
            let mac = entry.mac;
            let why = tf!(
                "{me} envoie la {label} pour {dst}{via} par {port}. Son cache ARP indique déjà \
                 que {nh} est à {mac} : la trame est adressée à cette MAC.",
                "{me} sends the {label} for {dst}{via} out {port}. Its ARP cache already says \
                 {nh} is at {mac}, so it addresses the frame to that MAC."
            );
            let frame = Frame {
                src: Mac::of(ep),
                dst: entry.mac,
                payload: Payload::Ip(pkt),
            };
            return self.emit(ep, frame, why);
        }

        let queue = self.pending.entry((dev, nh)).or_default();
        let first = queue.is_empty();
        queue.push((iface, pkt));
        if !first {
            return;
        }
        let Some(my_ip) = self
            .topo
            .device(dev)
            .and_then(|d| d.interfaces[iface].ip)
            .map(|c| c.addr)
        else {
            return;
        };
        let why = tf!(
            "{me} doit envoyer la {label}{via} mais n'a pas d'entrée ARP pour {nh}. Il diffuse \
             une requête ARP (broadcast) par {port} pour apprendre l'adresse MAC de {nh} ; la \
             {label} patiente en attendant.",
            "{me} must send the {label}{via} but has no ARP entry for {nh}. It broadcasts an \
             ARP request out {port} to learn {nh}'s MAC address; the {label} waits meanwhile."
        );
        let frame = Frame {
            src: Mac::of(ep),
            dst: Mac::BROADCAST,
            payload: Payload::Arp {
                op: ArpOp::Request,
                sender_mac: Mac::of(ep),
                sender_ip: my_ip,
                target_mac: Mac::ZERO,
                target_ip: nh,
            },
        };
        self.emit(ep, frame, why);
    }

    fn receive(&mut self, t: &Transmission) {
        let Some(dev) = self.topo.device(t.to.device) else {
            return;
        };
        let (me, port) = (dev.name.clone(), self.port(t.to));
        let frame = t.frame;
        let (src, dst) = (frame.src, frame.dst);

        if dev.kind.is_switch() {
            let table = self.state.mac_tables.entry(dev.id).or_default();
            let learned = table.insert(frame.src, t.to.iface) != Some(t.to.iface);
            let known = if frame.dst.is_broadcast() {
                None
            } else {
                table.get(&frame.dst).copied()
            };
            if learned {
                self.note(
                    Some(dev.id),
                    NoteKind::Info,
                    tf!(
                        "{me} apprend que {src} est joignable par {port}.",
                        "{me} learns that {src} is reachable through {port}."
                    ),
                );
            }
            match known {
                Some(p) if p == t.to.iface => self.note(
                    Some(dev.id),
                    NoteKind::Info,
                    tf!(
                        "{me} jette la trame : {dst} se trouve sur le port d'où elle vient.",
                        "{me} drops the frame: {dst} is on the port it came from."
                    ),
                ),
                Some(p) => {
                    let out = Endpoint {
                        device: dev.id,
                        iface: p,
                    };
                    let out_port = self.port(out);
                    let why = tf!(
                        "{me} sait que {dst} est sur {out_port} : il ne transmet la trame que \
                         par ce port.",
                        "{me} knows {dst} is on {out_port}: it forwards the frame out that port only."
                    );
                    self.emit(out, frame, why);
                }
                None => {
                    let reason = if dst.is_broadcast() {
                        tr("c'est un broadcast", "it is a broadcast").to_owned()
                    } else {
                        tf!(
                            "{dst} n'est pas encore dans sa table MAC",
                            "{dst} is not in its MAC table yet"
                        )
                    };
                    let ports: Vec<Endpoint> = (0..dev.interfaces.len())
                        .filter(|&p| p != t.to.iface)
                        .map(|iface| Endpoint {
                            device: dev.id,
                            iface,
                        })
                        .filter(|ep| self.topo.is_linked(*ep))
                        .collect();
                    for out in ports {
                        let why = tf!(
                            "{me} inonde la trame sur tous les ports sauf {port}, car {reason}.",
                            "{me} floods the frame out every port except {port}, because {reason}."
                        );
                        self.emit(out, frame, why);
                    }
                }
            }
            return;
        }

        let my_mac = Mac::of(t.to);
        if frame.dst != my_mac && !frame.dst.is_broadcast() {
            return self.note(
                Some(dev.id),
                NoteKind::Info,
                tf!(
                    "{me} ignore une trame destinée à {dst} : ce n'est pas son adresse MAC.",
                    "{me} ignores a frame sent to {dst}: not its MAC address."
                ),
            );
        }
        let my_ip = dev.interfaces[t.to.iface].ip.map(|c| c.addr);
        match frame.payload {
            Payload::Arp {
                op: ArpOp::Request,
                sender_mac,
                sender_ip,
                target_ip,
                ..
            } => {
                if my_ip != Some(target_ip) {
                    return self.note(
                        Some(dev.id),
                        NoteKind::Info,
                        tf!(
                            "{me} ignore la requête ARP : {target_ip} n'est pas son adresse.",
                            "{me} ignores the ARP request: {target_ip} is not its address."
                        ),
                    );
                }
                self.learn(dev.id, sender_ip, sender_mac, t.to.iface);
                self.note(
                    Some(dev.id),
                    NoteKind::Info,
                    tf!(
                        "{me} possède {target_ip} : il enregistre {sender_ip} → {sender_mac} dans \
                         son cache ARP et répond.",
                        "{me} owns {target_ip}: it records {sender_ip} → {sender_mac} in its ARP \
                         cache and answers."
                    ),
                );
                let reply = Frame {
                    src: my_mac,
                    dst: sender_mac,
                    payload: Payload::Arp {
                        op: ArpOp::Reply,
                        sender_mac: my_mac,
                        sender_ip: target_ip,
                        target_mac: sender_mac,
                        target_ip: sender_ip,
                    },
                };
                let why = tf!(
                    "{me} répond directement à {sender_ip} (unicast) : {target_ip} est à {my_mac}.",
                    "{me} answers {sender_ip} directly (unicast): {target_ip} is at {my_mac}."
                );
                self.emit(t.to, reply, why);
            }
            Payload::Arp {
                op: ArpOp::Reply,
                sender_mac,
                sender_ip,
                ..
            } => {
                self.learn(dev.id, sender_ip, sender_mac, t.to.iface);
                self.note(
                    Some(dev.id),
                    NoteKind::Info,
                    tf!(
                        "{me} ajoute {sender_ip} → {sender_mac} à son cache ARP.",
                        "{me} adds {sender_ip} → {sender_mac} to its ARP cache."
                    ),
                );
                for (iface, pkt) in self
                    .pending
                    .remove(&(dev.id, sender_ip))
                    .unwrap_or_default()
                {
                    let ep = Endpoint {
                        device: dev.id,
                        iface,
                    };
                    let (label, to) = (pkt.icmp.label(), pkt.dst);
                    let why = tf!(
                        "{me} connaît maintenant la MAC de {sender_ip} : il envoie la {label} en \
                         attente pour {to}.",
                        "{me} now knows {sender_ip}'s MAC, so it sends the waiting {label} for {to}."
                    );
                    let frame = Frame {
                        src: Mac::of(ep),
                        dst: sender_mac,
                        payload: Payload::Ip(pkt),
                    };
                    self.emit(ep, frame, why);
                }
            }
            Payload::Ip(pkt) => {
                let (from, to) = (pkt.src, pkt.dst);
                if dev.owns_ip(pkt.dst) {
                    match pkt.icmp {
                        Icmp::EchoRequest { seq } => {
                            self.note(
                                Some(dev.id),
                                NoteKind::Info,
                                tf!(
                                    "{me} reçoit la requête d'écho de {from} et répond par une \
                                     réponse d'écho.",
                                    "{me} receives the echo request from {from} and answers with \
                                     an echo reply."
                                ),
                            );
                            let id = dev.id;
                            self.send_ip(
                                id,
                                Packet {
                                    src: pkt.dst,
                                    dst: pkt.src,
                                    ttl: INITIAL_TTL,
                                    icmp: Icmp::EchoReply { seq },
                                },
                            );
                        }
                        Icmp::EchoReply { seq } if dev.id == self.source && seq == self.seq => {
                            self.reply = Some(pkt);
                            self.note(
                                Some(dev.id),
                                NoteKind::Success,
                                tf!(
                                    "{me} reçoit la réponse d'écho de {from} : le ping a réussi.",
                                    "{me} receives the echo reply from {from}: the ping succeeded."
                                ),
                            );
                        }
                        Icmp::EchoReply { .. } => self.note(
                            Some(dev.id),
                            NoteKind::Info,
                            tf!(
                                "{me} reçoit une réponse d'écho qu'il n'attendait pas.",
                                "{me} receives an echo reply it was not waiting for."
                            ),
                        ),
                    }
                } else if dev.kind.is_host() {
                    self.note(
                        Some(dev.id),
                        NoteKind::Failure,
                        tf!(
                            "{me} reçoit un paquet pour {to} mais n'est pas un routeur : il le jette.",
                            "{me} receives a packet for {to} but is not a router: it drops it."
                        ),
                    );
                } else if pkt.ttl <= 1 {
                    self.note(
                        Some(dev.id),
                        NoteKind::Failure,
                        tf!(
                            "TTL expiré sur {me} : le paquet pour {to} est jeté (boucle de routage ?).",
                            "TTL expired at {me}: the packet for {to} is dropped (routing loop?)."
                        ),
                    );
                } else {
                    let ttl = pkt.ttl - 1;
                    self.note(
                        Some(dev.id),
                        NoteKind::Info,
                        {
                            let old = pkt.ttl;
                            tf!(
                                "{me} route le paquet pour {to} et décrémente son TTL ({old} → {ttl}).",
                                "{me} routes the packet for {to} and decrements its TTL ({old} → {ttl})."
                            )
                        },
                    );
                    let id = dev.id;
                    self.send_ip(id, Packet { ttl, ..pkt });
                }
            }
        }
    }
}

/// Layer-3 decision: which interface to send on and which address to ARP for.
fn next_hop(topo: &Topology, id: DeviceId, dst: Ipv4Addr) -> Result<(usize, Ipv4Addr), String> {
    let dev = topo
        .device(id)
        .ok_or(tr("appareil inconnu", "unknown device"))?;
    let name = &dev.name;
    let up = |i: usize| {
        topo.is_linked(Endpoint {
            device: id,
            iface: i,
        })
    };

    if dev.kind.is_host() {
        let nic = &dev.interfaces[0];
        let nic_name = &nic.name;
        let cfg = nic.ip.ok_or_else(|| {
            tf!(
                "{name} n'a pas d'adresse IP sur {nic_name}.",
                "{name} has no IP address on {nic_name}."
            )
        })?;
        if !up(0) {
            return Err(tf!(
                "{name} {nic_name} n'est pas connecté.",
                "{name} {nic_name} is not connected."
            ));
        }
        if cfg.contains(dst) {
            return Ok((0, dst));
        }
        let gw = dev.gateway.ok_or_else(|| {
            tf!(
                "{dst} est hors du sous-réseau de {name} et aucune passerelle par défaut n'est \
                 définie.",
                "{dst} is outside {name}'s subnet and no default gateway is set."
            )
        })?;
        if !cfg.contains(gw) {
            return Err(tf!(
                "La passerelle {gw} de {name} est hors de son propre sous-réseau.",
                "{name}'s gateway {gw} is outside its own subnet."
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
        return best.map(|(_, _, i, nh)| (i, nh)).ok_or_else(|| {
            tf!(
                "{name} n'a pas de route vers {dst}.",
                "{name} has no route to {dst}."
            )
        });
    }

    Err(tf!(
        "{name} ne peut pas router de paquets.",
        "{name} can't route packets."
    ))
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

    fn fresh(t: &Topology, src: DeviceId, dst: &str) -> Run {
        crate::i18n::set_lang(crate::i18n::Lang::En);
        ping(t, &NetState::default(), src, ip(dst))
    }

    fn arp_frames(r: &Run) -> usize {
        r.transmissions
            .iter()
            .filter(|t| t.frame.kind() == FrameKind::Arp)
            .count()
    }

    /// PC1 — SW1 — R1 — SW2 — PC2, networks 10.0.1.0/24 and 10.0.2.0/24.
    fn two_lans() -> (Topology, DeviceId, DeviceId, DeviceId) {
        crate::i18n::set_lang(crate::i18n::Lang::En);
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
    fn explanations_follow_the_language() {
        let (t, pc1, _, _) = two_lans();
        crate::i18n::set_lang(crate::i18n::Lang::Fr);
        let r = ping(&t, &NetState::default(), pc1, ip("10.0.2.10"));
        assert!(r.success);
        assert!(
            r.summary.starts_with("Réponse de 10.0.2.10"),
            "{}",
            r.summary
        );
        assert!(r.header.starts_with("Ping vers"), "{}", r.header);
        let first = &r.transmissions[0];
        assert!(
            first.why.contains("diffuse une requête ARP"),
            "{}",
            first.why
        );
        assert!(
            first
                .frame
                .summary()
                .starts_with("Requête ARP : qui a 10.0.1.1 ?")
        );
        let r = ping(&t, &NetState::default(), pc1, ip("10.0.1.99"));
        assert!(r.summary.contains("hôte injoignable"), "{}", r.summary);
        crate::i18n::set_lang(crate::i18n::Lang::En);
    }

    #[test]
    fn mac_format() {
        let m = Mac::of(Endpoint {
            device: 257,
            iface: 3,
        });
        assert_eq!(m.to_string(), "02:4e:00:01:02:03");
        assert!(Mac::BROADCAST.is_broadcast());
    }

    #[test]
    fn same_subnet_uses_arp_then_cache_and_switch_learning() {
        let (mut t, pc1, _, _) = two_lans();
        let sw1 = t.devices[1].id;
        let pc3 = t.add_device(DeviceKind::Pc, [0.0; 2]);
        t.connect(pc3, sw1).unwrap();
        set_ip(&mut t, pc3, 0, "10.0.1.20", 24);

        let first = fresh(&t, pc1, "10.0.1.20");
        assert!(first.success, "{}", first.summary);
        // PC1 → SW1, flooded to R1 and PC3, then the unicast reply PC3 → SW1 → PC1.
        assert_eq!(arp_frames(&first), 5, "{:#?}", first.transmissions);
        assert!(first.final_state.arp[&pc1].contains_key(&ip("10.0.1.20")));
        assert!(first.final_state.arp[&pc3].contains_key(&ip("10.0.1.10")));

        // Second ping: caches are warm, the switch forwards without flooding.
        let second = ping(&t, &first.final_state, pc1, ip("10.0.1.20"));
        assert!(second.success);
        assert_eq!(arp_frames(&second), 0);
        assert_eq!(second.transmissions.len(), 4); // 2 hops each way

        // Clearing PC1's cache brings the ARP exchange back.
        let mut cleared = second.final_state.clone();
        cleared.clear_arp(Some(pc1));
        assert!(arp_frames(&ping(&t, &cleared, pc1, ip("10.0.1.20"))) > 0);
    }

    #[test]
    fn across_a_router() {
        let (t, pc1, r1, pc2) = two_lans();
        let r = fresh(&t, pc1, "10.0.2.10");
        assert!(r.success, "{}", r.summary);
        assert!(r.summary.contains("ttl=63"), "{}", r.summary);
        assert!(r.transmissions.iter().any(|x| x.to.device == r1));
        assert!(r.transmissions.iter().any(|x| x.to.device == pc2));
        assert_eq!(r.states.len(), r.steps);
        // Every frame has an explanation.
        assert!(r.transmissions.iter().all(|x| !x.why.is_empty()));
    }

    #[test]
    fn missing_gateway() {
        let (mut t, pc1, _, _) = two_lans();
        t.device_mut(pc1).unwrap().gateway = None;
        let r = fresh(&t, pc1, "10.0.2.10");
        assert!(!r.success);
        assert!(r.summary.contains("no default gateway"), "{}", r.summary);
        assert_eq!(r.steps, 0);
    }

    #[test]
    fn reply_lost_without_remote_gateway() {
        let (mut t, pc1, _, pc2) = two_lans();
        t.device_mut(pc2).unwrap().gateway = None;
        let r = fresh(&t, pc1, "10.0.2.10");
        assert!(!r.success);
        assert!(r.summary.contains("PC2"), "{}", r.summary);
        assert!(r.transmissions.iter().any(|x| x.to.device == pc2));
    }

    #[test]
    fn wrong_mask_breaks_reachability() {
        let (mut t, pc1, _, _) = two_lans();
        set_ip(&mut t, pc1, 0, "10.0.1.10", 30);
        let r = fresh(&t, pc1, "10.0.2.10");
        assert!(!r.success);
        assert!(
            r.summary.contains("outside its own subnet"),
            "{}",
            r.summary
        );
    }

    #[test]
    fn unknown_host_gets_no_arp_reply() {
        let (t, pc1, _, _) = two_lans();
        let r = fresh(&t, pc1, "10.0.1.99");
        assert!(!r.success);
        assert!(
            r.summary.contains("never got an ARP reply"),
            "{}",
            r.summary
        );
    }

    #[test]
    fn static_routes_and_ttl_expiry() {
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

        assert!(!fresh(&t, pc, "172.16.0.2").success, "R2 has no way back");
        t.device_mut(r2).unwrap().routes.push(Route {
            dest: ip("0.0.0.0"),
            prefix: 0,
            next_hop: ip("172.16.0.1"),
        });
        assert!(fresh(&t, pc, "172.16.0.2").success);

        let r = fresh(&t, pc, "192.168.1.1");
        assert!(!r.success);
        assert!(r.summary.contains("TTL expired"), "{}", r.summary);
    }

    #[test]
    fn switch_cannot_ping_and_loops_are_stopped() {
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
        assert!(!fresh(&t, a, "10.0.0.1").success);
        let r = fresh(&t, pc, "10.0.0.2");
        assert!(!r.success);
        assert!(r.summary.contains("broadcast storm"), "{}", r.summary);
    }

    #[test]
    fn prune_forgets_changed_addresses() {
        let (mut t, pc1, r1, _) = two_lans();
        let state = fresh(&t, pc1, "10.0.2.10").final_state;
        assert!(state.arp[&pc1].contains_key(&ip("10.0.1.1")));
        set_ip(&mut t, r1, 0, "10.0.1.254", 24);
        let mut pruned = state.clone();
        pruned.prune(&t);
        assert!(
            !pruned
                .arp
                .get(&pc1)
                .is_some_and(|m| m.contains_key(&ip("10.0.1.1")))
        );
    }

    #[test]
    fn frame_layers_describe_headers() {
        let (t, pc1, _, _) = two_lans();
        let r = fresh(&t, pc1, "10.0.2.10");
        let arp = &r.transmissions[0].frame;
        assert_eq!(arp.dst, Mac::BROADCAST);
        assert!(arp.summary().starts_with("ARP request: who has 10.0.1.1?"));
        let icmp = r
            .transmissions
            .iter()
            .find(|x| x.frame.kind() == FrameKind::Request)
            .unwrap();
        let layers = icmp.frame.layers();
        assert_eq!(
            layers.iter().map(|l| l.0).collect::<Vec<_>>(),
            ["Ethernet", "IPv4", "ICMP"]
        );
    }
}
