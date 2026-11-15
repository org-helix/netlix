//! Side panels: toolbox (left), inspector (right), ping console (bottom).

use crate::app::{App, LogKind, Selection, Tool};
use crate::canvas::draw_icon;
use crate::model::{
    Device, DeviceId, DeviceKind, Endpoint, IpConfig, Route, network, parse_mask, prefix_to_mask,
};
use crate::theme::{self, pal};
use eframe::egui::{
    self, Align2, ComboBox, CornerRadius, FontId, RichText, ScrollArea, Sense, Stroke, TextEdit,
    Ui, vec2,
};
use std::net::Ipv4Addr;

// ── Toolbox ────────────────────────────────────────────────────────────────

enum RowIcon {
    Device(DeviceKind),
    Select,
    Link,
}

pub fn toolbox(app: &mut App, ui: &mut Ui) {
    ui.label(theme::eyebrow("Devices"));
    for (n, kind) in DeviceKind::ALL.into_iter().enumerate() {
        let active = app.tool == Tool::Place(kind);
        if tool_row(
            ui,
            RowIcon::Device(kind),
            kind.label(),
            &(n + 1).to_string(),
            active,
        )
        .clicked()
        {
            app.tool = if active {
                Tool::Select
            } else {
                Tool::Place(kind)
            };
            app.link_from = None;
        }
    }
    ui.add_space(16.0);
    ui.label(theme::eyebrow("Tools"));
    if tool_row(ui, RowIcon::Select, "Select", "V", app.tool == Tool::Select).clicked() {
        app.tool = Tool::Select;
        app.link_from = None;
    }
    if tool_row(ui, RowIcon::Link, "Link", "L", app.tool == Tool::Link).clicked() {
        app.tool = Tool::Link;
    }
}

fn tool_row(ui: &mut Ui, icon: RowIcon, label: &str, key: &str, active: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    let p = ui.painter();
    let c = pal();
    let radius = CornerRadius::same(8);
    if active {
        p.rect_filled(rect, radius, c.selected);
    } else if resp.hovered() {
        p.rect_filled(rect, radius, c.fill);
    }
    if resp.has_focus() {
        p.rect_stroke(
            rect,
            radius,
            Stroke::new(2.0, c.ring),
            egui::StrokeKind::Inside,
        );
    }
    let bg = if active { c.selected } else { c.panel };
    let ic = rect.left_center() + vec2(22.0, 0.0);
    match icon {
        RowIcon::Device(kind) => draw_icon(p, ic, 24.0, kind, c.text, bg),
        RowIcon::Select => {
            let pts = [
                ic + vec2(-5.0, -8.0),
                ic + vec2(6.0, 2.0),
                ic + vec2(0.5, 2.5),
                ic + vec2(-2.0, 8.0),
            ];
            p.add(egui::Shape::closed_line(
                pts.to_vec(),
                Stroke::new(2.0, c.text),
            ));
        }
        RowIcon::Link => {
            let (a, b) = (ic + vec2(-7.0, 6.0), ic + vec2(7.0, -6.0));
            p.line_segment([a, b], Stroke::new(2.0, c.text));
            p.circle_filled(a, 3.0, c.text);
            p.circle_filled(b, 3.0, c.text);
        }
    }
    p.text(
        rect.left_center() + vec2(44.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::new(14.0, theme::medium()),
        c.text,
    );
    p.text(
        rect.right_center() - vec2(10.0, 0.0),
        Align2::RIGHT_CENTER,
        key,
        FontId::monospace(12.0),
        c.muted,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// ── Inspector ──────────────────────────────────────────────────────────────

/// Text being typed in the inspector. The model only changes when a value parses.
#[derive(Default)]
pub struct EditBuffers {
    device: Option<DeviceId>,
    ifaces: Vec<(String, String)>,
    gateway: String,
    route: [String; 3],
    route_error: Option<String>,
}

impl EditBuffers {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn sync(&mut self, dev: &Device) {
        if self.device == Some(dev.id) && self.ifaces.len() == dev.interfaces.len() {
            return;
        }
        self.reset();
        self.device = Some(dev.id);
        self.ifaces = dev
            .interfaces
            .iter()
            .map(|i| match i.ip {
                Some(c) => (c.addr.to_string(), prefix_to_mask(c.prefix).to_string()),
                None => (String::new(), String::new()),
            })
            .collect();
        self.gateway = dev.gateway.map(|g| g.to_string()).unwrap_or_default();
    }
}

pub fn inspector(app: &mut App, ui: &mut Ui) {
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let dups = app.topo.duplicate_ips();
            if !dups.is_empty() {
                let list = dups
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                theme::group_frame().show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("Duplicate IP address: {list}")).color(pal().error),
                    );
                });
                ui.add_space(8.0);
            }
            match app.selection {
                Selection::Device(id) if app.topo.device(id).is_some() => {
                    device_inspector(app, ui, id)
                }
                Selection::Link(i) if i < app.topo.links.len() => link_inspector(app, ui, i),
                _ => overview(app, ui),
            }
        });
}

fn overview(app: &App, ui: &mut Ui) {
    ui.label(theme::title("Topology"));
    let count = |f: fn(DeviceKind) -> bool| app.topo.devices.iter().filter(|d| f(d.kind)).count();
    ui.label(theme::muted(format!(
        "{} hosts · {} switches · {} routers · {} links",
        count(DeviceKind::is_host),
        count(DeviceKind::is_switch),
        count(DeviceKind::is_router),
        app.topo.links.len()
    )));
    ui.add_space(16.0);
    ui.label(theme::eyebrow("How it works"));
    for tip in [
        "Place devices from the left panel, then link them with the Link tool (L).",
        "Select a device to set its IP addresses, mask and gateway.",
        "Routers forward between subnets; add static routes for networks they don't touch.",
        "Use the ping console below to test reachability.",
    ] {
        ui.label(RichText::new(tip).color(pal().body));
    }
}

fn link_inspector(app: &mut App, ui: &mut Ui, i: usize) {
    let link = app.topo.links[i];
    ui.label(theme::eyebrow("Link"));
    let name = |ep: Endpoint| {
        let dev = app.topo.device(ep.device).map_or("?", |d| d.name.as_str());
        format!("{dev} · {}", app.topo.iface_name(ep))
    };
    ui.label(theme::title(format!("{} ↔ {}", name(link.a), name(link.b))));
    ui.add_space(8.0);
    ui.label(theme::muted(
        "Copper Ethernet. Delete it to free both ports.",
    ));
    ui.add_space(12.0);
    if theme::ghost_button(ui, RichText::new("Delete link").color(pal().error)).clicked() {
        app.delete_selection();
    }
}

fn device_inspector(app: &mut App, ui: &mut Ui, id: DeviceId) {
    let peers: Vec<Option<String>> = {
        let dev = app.topo.device(id).expect("checked by caller");
        (0..dev.interfaces.len())
            .map(|iface| {
                let ep = Endpoint { device: id, iface };
                let (_, link) = app.topo.link_at(ep)?;
                let other = link.other(ep)?;
                let dev = app.topo.device(other.device)?;
                Some(format!("{} · {}", dev.name, app.topo.iface_name(other)))
            })
            .collect()
    };
    app.edit
        .sync(app.topo.device(id).expect("checked by caller"));
    let mut changed = false;
    let mut delete = false;
    let mut ping_here = false;
    let buffers = &mut app.edit;
    let dev = app.topo.device_mut(id).expect("checked by caller");

    ui.label(theme::eyebrow(dev.kind.label()));
    changed |= ui
        .add(
            TextEdit::singleline(&mut dev.name)
                .font(egui::TextStyle::Heading)
                .desired_width(f32::INFINITY)
                .char_limit(32),
        )
        .changed();
    ui.add_space(12.0);

    ui.label(theme::eyebrow(if dev.kind.is_switch() {
        "Ports"
    } else {
        "Interfaces"
    }));
    for (i, iface) in dev.interfaces.iter_mut().enumerate() {
        theme::group_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(&iface.name).monospace().color(pal().text));
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| match &peers[i] {
                        Some(peer) => {
                            ui.label(RichText::new(format!("→ {peer}")).color(pal().muted))
                        }
                        None => ui.label(RichText::new("not connected").color(pal().muted)),
                    },
                );
            });
            if dev.kind.is_switch() {
                return;
            }
            let (ip_text, mask_text) = &mut buffers.ifaces[i];
            let ip_edit = field(ui, "IP address", ip_text, "192.168.1.10");
            let mask_edit = field(ui, "Subnet mask", mask_text, "255.255.255.0");
            if ip_edit || mask_edit {
                // Fill a classful mask the first time a valid address is typed.
                if let (Ok(ip), true) = (
                    ip_text.trim().parse::<Ipv4Addr>(),
                    mask_text.trim().is_empty(),
                ) {
                    *mask_text = prefix_to_mask(classful_prefix(ip)).to_string();
                }
                let new = match (ip_text.trim().parse::<Ipv4Addr>(), parse_mask(mask_text)) {
                    _ if ip_text.trim().is_empty() => Some(None),
                    (Ok(ip), Some(prefix)) => Some(Some(IpConfig::new(ip, prefix))),
                    _ => None, // keep the last valid value while typing
                };
                if let Some(new) = new.filter(|n| *n != iface.ip) {
                    iface.ip = new;
                    changed = true;
                }
            }
            if !ip_text.trim().is_empty() && ip_text.trim().parse::<Ipv4Addr>().is_err() {
                error(ui, "Not a valid IPv4 address.");
            } else if !mask_text.trim().is_empty() && parse_mask(mask_text).is_none() {
                error(ui, "Use a mask like 255.255.255.0 or /24.");
            } else if let Some(c) = iface.ip.filter(|c| c.prefix < 31) {
                let broadcast =
                    Ipv4Addr::from(u32::from(c.network()) | !u32::from(prefix_to_mask(c.prefix)));
                if c.addr == c.network() {
                    error(ui, "This is the network address; pick a host address.");
                } else if c.addr == broadcast {
                    error(ui, "This is the broadcast address; pick a host address.");
                }
            }
        });
        ui.add_space(4.0);
    }

    if dev.kind.is_host() {
        ui.add_space(8.0);
        ui.label(theme::eyebrow("Default gateway"));
        if field(ui, "Gateway", &mut buffers.gateway, "192.168.1.1") {
            let text = buffers.gateway.trim();
            let new = if text.is_empty() {
                Some(None)
            } else {
                text.parse().ok().map(Some)
            };
            if let Some(new) = new.filter(|n| *n != dev.gateway) {
                dev.gateway = new;
                changed = true;
            }
        }
        let text = buffers.gateway.trim();
        if !text.is_empty() && text.parse::<Ipv4Addr>().is_err() {
            error(ui, "Not a valid IPv4 address.");
        } else if let (Some(gw), Some(cfg)) = (dev.gateway, dev.interfaces[0].ip)
            && !cfg.contains(gw)
        {
            error(ui, "The gateway must be in the same subnet as eth0.");
        }
    }

    if dev.kind.is_router() {
        ui.add_space(8.0);
        ui.label(theme::eyebrow("Static routes"));
        if dev.routes.is_empty() {
            ui.label(theme::muted(
                "None. Directly connected networks are routed automatically.",
            ));
        }
        let mut remove = None;
        for (i, r) in dev.routes.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{}/{} via {}", r.dest, r.prefix, r.next_hop))
                        .monospace()
                        .color(pal().text),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::ghost_button(ui, RichText::new("Remove").color(pal().error)).clicked()
                    {
                        remove = Some(i);
                    }
                });
            });
        }
        if let Some(i) = remove {
            dev.routes.remove(i);
            changed = true;
        }
        theme::group_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let [dest, mask, hop] = &mut buffers.route;
            field(ui, "Network", dest, "0.0.0.0");
            field(ui, "Mask", mask, "0.0.0.0");
            field(ui, "Next hop", hop, "203.0.113.1");
            if ui
                .add(egui::Button::new("Add route").min_size(vec2(0.0, 32.0)))
                .clicked()
            {
                match parse_route(dest, mask, hop) {
                    Ok(route) => {
                        dev.routes.push(route);
                        buffers.route = Default::default();
                        buffers.route_error = None;
                        changed = true;
                    }
                    Err(e) => buffers.route_error = Some(e),
                }
            }
            if let Some(e) = &buffers.route_error {
                error(ui, e);
            }
        });
    }

    ui.add_space(16.0);
    ui.horizontal(|ui| {
        if !dev.kind.is_switch() && theme::ghost_button(ui, "Ping from this device").clicked() {
            ping_here = true;
        }
        if theme::ghost_button(ui, RichText::new("Delete device").color(pal().error)).clicked() {
            delete = true;
        }
    });

    if changed {
        app.mark_dirty();
    }
    if ping_here {
        app.ping_src = Some(id);
    }
    if delete {
        app.delete_selection();
    }
}

fn field(ui: &mut Ui, label: &str, text: &mut String, hint: &str) -> bool {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            vec2(96.0, 28.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(96.0);
                ui.label(RichText::new(label).color(pal().muted));
            },
        );
        ui.add(
            TextEdit::singleline(text)
                .hint_text(hint)
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY)
                .margin(vec2(8.0, 6.0)),
        )
        .changed()
    })
    .inner
}

fn error(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().color(pal().error));
}

fn classful_prefix(ip: Ipv4Addr) -> u8 {
    match ip.octets()[0] {
        0..=127 => 8,
        128..=191 => 16,
        _ => 24,
    }
}

fn parse_route(dest: &str, mask: &str, hop: &str) -> Result<Route, String> {
    let dest: Ipv4Addr = dest
        .trim()
        .parse()
        .map_err(|_| "Network: not a valid IPv4 address.")?;
    let prefix = parse_mask(mask).ok_or("Mask: use 255.255.0.0 or /16.")?;
    let next_hop = hop
        .trim()
        .parse()
        .map_err(|_| "Next hop: not a valid IPv4 address.")?;
    Ok(Route {
        dest: network(dest, prefix),
        prefix,
        next_hop,
    })
}

// ── Ping console ───────────────────────────────────────────────────────────

pub fn console(app: &mut App, ui: &mut Ui) {
    let mut send = false;
    ui.horizontal(|ui| {
        ui.label(theme::title("Ping"));
        ui.add_space(8.0);

        let src_name = app
            .ping_src
            .and_then(|id| app.topo.device(id))
            .map(|d| d.name.clone());
        ComboBox::from_id_salt("ping-src")
            .selected_text(src_name.unwrap_or_else(|| "Source device".into()))
            .width(160.0)
            .show_ui(ui, |ui| {
                for d in app.topo.devices.iter().filter(|d| !d.kind.is_switch()) {
                    ui.selectable_value(&mut app.ping_src, Some(d.id), &d.name);
                }
            });
        ui.label(theme::muted("to"));
        let resp = ui.add(
            TextEdit::singleline(&mut app.ping_dst)
                .hint_text("Destination IP")
                .font(egui::TextStyle::Monospace)
                .desired_width(150.0)
                .margin(vec2(8.0, 6.0)),
        );
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            send = true;
        }
        ComboBox::from_id_salt("ping-dst-pick")
            .selected_text("Pick a device")
            .width(150.0)
            .show_ui(ui, |ui| {
                for d in &app.topo.devices {
                    for iface in d.interfaces.iter().filter_map(|i| i.ip) {
                        let label = format!("{} · {}", d.name, iface.addr);
                        if ui.selectable_label(false, label).clicked() {
                            app.ping_dst = iface.addr.to_string();
                        }
                    }
                }
            });
        send |= theme::primary_button(ui, "Send ping").clicked();
        if !app.log.is_empty() && theme::ghost_button(ui, "Clear").clicked() {
            app.log.clear();
        }
    });
    ui.add_space(4.0);

    theme::group_frame().show(ui, |ui| {
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if app.log.is_empty() {
                    ui.label(theme::muted(
                        "Results appear here. Pick a source, type a destination IP, then send.",
                    ));
                }
                for line in &app.log {
                    let (prefix, color) = match line.kind {
                        LogKind::Info => ("›", pal().muted),
                        LogKind::Success => ("✓", pal().success),
                        LogKind::Failure => ("✕", pal().error),
                    };
                    ui.label(
                        RichText::new(format!("{prefix} {}", line.text))
                            .monospace()
                            .color(color),
                    );
                }
            });
    });

    if send {
        app.send_ping();
    }
}
