//! Side panels: toolbox (left), inspector (right), ping console (bottom).

use crate::app::{App, LogEntry, LogKind, Selection, SendMode, Tool};
use crate::canvas::{draw_icon, frame_dot};
use crate::i18n::tr;
use crate::model::{
    Device, DeviceId, DeviceKind, Endpoint, IpConfig, Route, network, parse_mask, prefix_to_mask,
};
use crate::sim::{self, Mac};
use crate::tf;
use crate::theme::{self, pal};
use eframe::egui::{
    self, Align2, ComboBox, CornerRadius, FontId, Margin, RichText, ScrollArea, Sense, Stroke,
    TextEdit, Ui, vec2,
};
use std::net::Ipv4Addr;

// ── Toolbox ────────────────────────────────────────────────────────────────

enum RowIcon {
    Device(DeviceKind),
    Select,
    Link,
}

pub fn toolbox(app: &mut App, ui: &mut Ui) {
    ui.label(theme::eyebrow(tr("Appareils", "Devices")));
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
    ui.label(theme::eyebrow(tr("Outils", "Tools")));
    if tool_row(
        ui,
        RowIcon::Select,
        tr("Sélection", "Select"),
        "V",
        app.tool == Tool::Select,
    )
    .clicked()
    {
        app.tool = Tool::Select;
        app.link_from = None;
    }
    if tool_row(
        ui,
        RowIcon::Link,
        tr("Lien", "Link"),
        "L",
        app.tool == Tool::Link,
    )
    .clicked()
    {
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

/// "How it works" hints, shown in the Help menu.
pub fn tips() -> [&'static str; 6] {
    [
        tr(
            "Placez des appareils depuis le panneau de gauche, puis reliez-les avec l'outil Lien (L).",
            "Place devices from the left panel, then link them with the Link tool (L).",
        ),
        tr(
            "Clic droit sur un appareil → Configurer… (ou double-clic) pour régler ses adresses IP, \
             son masque, sa passerelle et ses routes.",
            "Right-click a device → Configure… (or double-click it) to set its IP addresses, mask, \
             gateway and routes.",
        ),
        tr(
            "Les routeurs relient les sous-réseaux ; ajoutez des routes statiques pour les réseaux \
             qu'ils ne touchent pas directement.",
            "Routers forward between subnets; add static routes for networks they don't touch.",
        ),
        tr(
            "Testez la connectivité avec la console de ping en bas.",
            "Use the ping console below to test reachability.",
        ),
        tr(
            "Passez la console en Pas à pas pour suivre chaque trame : ARP, commutation, routage.",
            "Switch the console to Step by step to follow every frame: ARP, switching, routing.",
        ),
        tr(
            "Les caches ARP et les tables MAC des switchs sont conservés d'un ping à l'autre. \
             Videz-les depuis la console ou d'un clic droit sur un appareil pour revoir la \
             résolution d'adresses.",
            "ARP caches and switch MAC tables are kept between pings. Clear them from the \
             console or by right-clicking a device to watch address resolution again.",
        ),
    ]
}

/// "3 hosts · 2 switches · 2 routers · 6 links".
pub fn topology_summary(app: &App) -> String {
    let count = |f: fn(DeviceKind) -> bool| app.topo.devices.iter().filter(|d| f(d.kind)).count();
    let (hosts, switches, routers, links) = (
        count(DeviceKind::is_host),
        count(DeviceKind::is_switch),
        count(DeviceKind::is_router),
        app.topo.links.len(),
    );
    tf!(
        "{hosts} hôtes · {switches} switchs · {routers} routeurs · {links} liens",
        "{hosts} hosts · {switches} switches · {routers} routers · {links} links"
    )
}

/// Floating window with the configuration of the device picked with "Configure…".
pub fn config_window(app: &mut App, ctx: &egui::Context) {
    let Some(id) = app.config else {
        return;
    };
    let Some(dev) = app.topo.device(id) else {
        app.config = None;
        return;
    };
    let name = dev.name.clone();
    let mut open = true;
    let screen = ctx.content_rect();
    let max_height = (screen.height() - 100.0).max(240.0);
    let window_id = egui::Id::unique("netlix-config");
    let mut window = egui::Window::new(tf!("Configurer {name}", "Configure {name}"));
    if app.config_reposition {
        // Beside the device, but near the top so the window gets the full height, and on screen.
        let x = app
            .config_pos
            .x
            .min(screen.right() - 440.0)
            .max(screen.left() + 8.0);
        let pos = egui::pos2(x, screen.top() + 64.0);
        // `Window::current_pos` is overridden by the remembered position when the window
        // moves by its title bar (egui 0.37), so move the remembered state itself.
        match egui::AreaState::load(ctx, window_id) {
            Some(mut state) => {
                state.set_left_top_pos(pos);
                ctx.memory_mut(|m| {
                    m.areas_mut()
                        .set_state(egui::LayerId::new(egui::Order::Middle, window_id), state);
                });
            }
            None => window = window.default_pos(pos),
        }
        app.config_reposition = false;
    }
    window
        .id(window_id)
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(420.0)
        .default_height(max_height)
        .max_height(max_height)
        .show(ctx, |ui| {
            ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| device_form(app, ui, id));
        });
    if !open {
        app.config = None;
    }
}

/// Everything that can be set on one device.
fn device_form(app: &mut App, ui: &mut Ui, id: DeviceId) {
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
    let stepping = app
        .playback
        .as_ref()
        .filter(|pb| pb.mode == SendMode::StepByStep)
        .map(|pb| pb.step + 1);
    let state = app.view_state();
    let arp_rows: Vec<(Ipv4Addr, sim::ArpEntry)> = state
        .arp
        .get(&id)
        .map(|t| t.iter().map(|(k, v)| (*k, *v)).collect())
        .unwrap_or_default();
    let mac_rows: Vec<(Mac, usize)> = state
        .mac_tables
        .get(&id)
        .map(|t| t.iter().map(|(k, v)| (*k, *v)).collect())
        .unwrap_or_default();
    let mut clear_table = false;
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
        tr("Ports", "Ports")
    } else {
        tr("Interfaces", "Interfaces")
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
                        None => ui.label(
                            RichText::new(tr("non connecté", "not connected")).color(pal().muted),
                        ),
                    },
                );
            });
            let mac = Mac::of(Endpoint {
                device: id,
                iface: i,
            });
            ui.label(
                RichText::new(format!("MAC {mac}"))
                    .monospace()
                    .size(12.0)
                    .color(pal().muted),
            );
            if dev.kind.is_switch() {
                return;
            }
            let (ip_text, mask_text) = &mut buffers.ifaces[i];
            let ip_edit = field(ui, tr("Adresse IP", "IP address"), ip_text, "192.168.1.10");
            let mask_edit = field(ui, tr("Masque", "Subnet mask"), mask_text, "255.255.255.0");
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
                error(
                    ui,
                    tr("Adresse IPv4 invalide.", "Not a valid IPv4 address."),
                );
            } else if !mask_text.trim().is_empty() && parse_mask(mask_text).is_none() {
                error(
                    ui,
                    tr(
                        "Utilisez un masque comme 255.255.255.0 ou /24.",
                        "Use a mask like 255.255.255.0 or /24.",
                    ),
                );
            } else if let Some(c) = iface.ip.filter(|c| c.prefix < 31) {
                let broadcast =
                    Ipv4Addr::from(u32::from(c.network()) | !u32::from(prefix_to_mask(c.prefix)));
                if c.addr == c.network() {
                    error(
                        ui,
                        tr(
                            "C'est l'adresse du réseau : choisissez une adresse d'hôte.",
                            "This is the network address; pick a host address.",
                        ),
                    );
                } else if c.addr == broadcast {
                    error(
                        ui,
                        tr(
                            "C'est l'adresse de broadcast : choisissez une adresse d'hôte.",
                            "This is the broadcast address; pick a host address.",
                        ),
                    );
                }
            }
        });
        ui.add_space(4.0);
    }

    if dev.kind.is_host() {
        ui.add_space(8.0);
        ui.label(theme::eyebrow(tr(
            "Passerelle par défaut",
            "Default gateway",
        )));
        if field(
            ui,
            tr("Passerelle", "Gateway"),
            &mut buffers.gateway,
            "192.168.1.1",
        ) {
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
            error(
                ui,
                tr("Adresse IPv4 invalide.", "Not a valid IPv4 address."),
            );
        } else if let (Some(gw), Some(cfg)) = (dev.gateway, dev.interfaces[0].ip)
            && !cfg.contains(gw)
        {
            error(
                ui,
                tr(
                    "La passerelle doit être dans le même sous-réseau que eth0.",
                    "The gateway must be in the same subnet as eth0.",
                ),
            );
        }
    }

    if dev.kind.is_router() {
        ui.add_space(8.0);
        ui.label(theme::eyebrow(tr("Routes statiques", "Static routes")));
        if dev.routes.is_empty() {
            ui.label(theme::muted(tr(
                "Aucune. Les réseaux directement connectés sont routés automatiquement.",
                "None. Directly connected networks are routed automatically.",
            )));
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
                    if theme::ghost_button(
                        ui,
                        RichText::new(tr("Retirer", "Remove")).color(pal().error),
                    )
                    .clicked()
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
            field(ui, tr("Réseau", "Network"), dest, "0.0.0.0");
            field(ui, tr("Masque", "Mask"), mask, "0.0.0.0");
            field(ui, tr("Prochain saut", "Next hop"), hop, "203.0.113.1");
            if ui
                .add(
                    egui::Button::new(tr("Ajouter la route", "Add route"))
                        .min_size(vec2(0.0, 32.0)),
                )
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

    // ARP cache (hosts, routers) or MAC address table (switches).
    ui.add_space(8.0);
    let iface_name = |i: usize| dev.interfaces.get(i).map_or("?", |x| x.name.as_str());
    let (title, empty, rows): (&str, &str, Vec<(String, String)>) = if dev.kind.is_switch() {
        (
            tr("Table d'adresses MAC", "MAC address table"),
            tr(
                "Vide. Le switch apprend quel port mène à chaque MAC grâce aux trames qu'il reçoit.",
                "Empty. The switch learns which port leads to each MAC from the frames it receives.",
            ),
            mac_rows
                .iter()
                .map(|(mac, port)| (mac.to_string(), iface_name(*port).to_owned()))
                .collect(),
        )
    } else {
        (
            tr("Cache ARP", "ARP cache"),
            tr(
                "Vide. Des entrées apparaissent quand cet appareil résout une adresse ou répond à une requête ARP.",
                "Empty. Entries appear when this device resolves an address or answers an ARP request.",
            ),
            arp_rows
                .iter()
                .map(|(ip, e)| {
                    (
                        ip.to_string(),
                        format!("{}  {}", e.mac, iface_name(e.iface)),
                    )
                })
                .collect(),
        )
    };
    ui.label(theme::eyebrow(title));
    if let Some(step) = stepping {
        ui.label(
            RichText::new(tf!(
                "État à l'étape {step} du ping en cours d'examen.",
                "As of step {step} of the ping being stepped through."
            ))
            .small()
            .color(pal().muted),
        );
    }
    if rows.is_empty() {
        ui.label(theme::muted(empty));
    } else {
        theme::group_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::Grid::new(("net-table", id))
                .num_columns(2)
                .spacing(vec2(16.0, 4.0))
                .show(ui, |ui| {
                    for (key, value) in &rows {
                        ui.label(RichText::new(key).monospace().color(pal().text));
                        ui.label(RichText::new(value).monospace().color(pal().muted));
                        ui.end_row();
                    }
                });
        });
        let label = if dev.kind.is_switch() {
            tr("Vider la table MAC", "Clear MAC table")
        } else {
            tr("Vider le cache ARP", "Clear ARP cache")
        };
        if theme::ghost_button(ui, label).clicked() {
            clear_table = true;
        }
    }
    let is_switch = dev.kind.is_switch();

    ui.add_space(16.0);
    ui.horizontal(|ui| {
        if !dev.kind.is_switch()
            && theme::ghost_button(
                ui,
                tr("Pinger depuis cet appareil", "Ping from this device"),
            )
            .clicked()
        {
            ping_here = true;
        }
        if theme::ghost_button(
            ui,
            RichText::new(tr("Supprimer l'appareil", "Delete device")).color(pal().error),
        )
        .clicked()
        {
            delete = true;
        }
    });

    if changed {
        app.mark_dirty();
    }
    if ping_here {
        app.ping_src = Some(id);
    }
    if clear_table {
        if is_switch {
            app.clear_mac_tables(Some(id));
        } else {
            app.clear_arp(Some(id));
        }
    }
    if delete {
        app.selection = Selection::Device(id);
        app.delete_selection();
        app.config = None;
    }
}

fn field(ui: &mut Ui, label: &str, text: &mut String, hint: &str) -> bool {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            vec2(112.0, 28.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(112.0);
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
    let dest: Ipv4Addr = dest.trim().parse().map_err(|_| {
        tr(
            "Réseau : adresse IPv4 invalide.",
            "Network: not a valid IPv4 address.",
        )
    })?;
    let prefix = parse_mask(mask).ok_or(tr(
        "Masque : utilisez 255.255.0.0 ou /16.",
        "Mask: use 255.255.0.0 or /16.",
    ))?;
    let next_hop = hop.trim().parse().map_err(|_| {
        tr(
            "Prochain saut : adresse IPv4 invalide.",
            "Next hop: not a valid IPv4 address.",
        )
    })?;
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
            .selected_text(
                src_name.unwrap_or_else(|| tr("Appareil source", "Source device").into()),
            )
            .width(160.0)
            .show_ui(ui, |ui| {
                for d in app.topo.devices.iter().filter(|d| !d.kind.is_switch()) {
                    ui.selectable_value(&mut app.ping_src, Some(d.id), &d.name);
                }
            });
        ui.label(theme::muted(tr("vers", "to")));
        let resp = ui.add(
            TextEdit::singleline(&mut app.ping_dst)
                .hint_text(tr("IP de destination", "Destination IP"))
                .font(egui::TextStyle::Monospace)
                .desired_width(150.0)
                .margin(vec2(8.0, 6.0)),
        );
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            send = true;
        }
        ComboBox::from_id_salt("ping-dst-pick")
            .selected_text(tr("Choisir un appareil", "Pick a device"))
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
        send |= theme::primary_button(ui, tr("Envoyer le ping", "Send ping")).clicked();
        if !app.log.is_empty()
            && theme::ghost_button(ui, tr("Effacer le journal", "Clear log")).clicked()
        {
            app.log.clear();
        }
    });

    ui.horizontal(|ui| {
        ui.label(theme::eyebrow("Mode"));
        let before = app.mode;
        ui.selectable_value(
            &mut app.mode,
            SendMode::Realtime,
            tr("Temps réel", "Real time"),
        );
        ui.selectable_value(
            &mut app.mode,
            SendMode::StepByStep,
            tr("Pas à pas", "Step by step"),
        );
        if app.mode != before {
            app.playback = None;
        }
        if app.mode == SendMode::Realtime {
            ui.add_space(8.0);
            ui.checkbox(
                &mut app.explain,
                tr("Expliquer chaque trame", "Explain every frame"),
            )
            .on_hover_text(tr(
                "Affiche dans le journal chaque trame, la raison de son envoi et ses en-têtes.",
                "Log each frame with the reason it was sent and its headers.",
            ));
        }
        ui.separator();
        if theme::ghost_button(ui, tr("Vider les caches ARP", "Clear ARP caches")).clicked() {
            app.clear_arp(None);
        }
        if theme::ghost_button(ui, tr("Vider les tables MAC", "Clear MAC tables")).clicked() {
            app.clear_mac_tables(None);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            legend(ui);
        });
    });
    ui.add_space(4.0);

    let stepping = app
        .playback
        .as_ref()
        .is_some_and(|pb| pb.mode == SendMode::StepByStep);
    if stepping {
        step_view(app, ui);
    } else {
        log_view(app, ui);
    }

    if send {
        app.send_ping();
    }
}

/// Colour key of the frame markers.
fn legend(ui: &mut Ui) {
    for (kind, label) in [
        (sim::FrameKind::Reply, tr("réponse d'écho", "echo reply")),
        (
            sim::FrameKind::Request,
            tr("requête d'écho", "echo request"),
        ),
        (sim::FrameKind::Arp, "ARP"),
    ] {
        ui.label(theme::muted(label));
        let (rect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
        frame_dot(ui.painter(), rect.center(), kind, 6.0);
        ui.add_space(4.0);
    }
}

fn log_view(app: &App, ui: &mut Ui) {
    theme::group_frame().show(ui, |ui| {
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if app.log.is_empty() {
                    ui.label(theme::muted(
                        tr("Les résultats s'affichent ici. Choisissez une source, saisissez une IP de destination, puis envoyez.", "Results appear here. Pick a source, type a destination IP, then send."),
                    ));
                }
                for (i, entry) in app.log.iter().enumerate() {
                    match entry {
                        LogEntry::Line { text, kind } => log_line(ui, text, *kind),
                        LogEntry::Frame(t) => {
                            frame_card(ui, app, t, false, i);
                        }
                    }
                }
            });
    });
}

fn log_line(ui: &mut Ui, text: &str, kind: LogKind) {
    let (prefix, color) = match kind {
        LogKind::Info => ("›", pal().muted),
        LogKind::Success => ("✓", pal().success),
        LogKind::Failure => ("✕", pal().error),
    };
    ui.label(
        RichText::new(format!("{prefix} {text}"))
            .monospace()
            .color(color),
    );
}

/// One transmitted frame: where it goes, why, and its simplified headers.
fn frame_card(
    ui: &mut Ui,
    app: &App,
    t: &sim::Transmission,
    selected: bool,
    salt: usize,
) -> egui::Response {
    let c = pal();
    let end = |ep: Endpoint| {
        let name = app.topo.device(ep.device).map_or("?", |d| d.name.as_str());
        format!("{name} {}", app.topo.iface_name(ep))
    };
    egui::Frame::new()
        .fill(if selected { c.selected } else { c.faint })
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                frame_dot(ui.painter(), rect.center(), t.frame.kind(), 6.0);
                ui.label(
                    RichText::new({
                        let (n, from, to) = (t.step + 1, end(t.from), end(t.to));
                        tf!("Étape {n} · {from} → {to}", "Step {n} · {from} → {to}")
                    })
                    .family(theme::medium())
                    .color(c.text),
                );
                ui.label(RichText::new(t.frame.summary()).color(c.muted));
            });
            ui.label(RichText::new(&t.why).color(c.body));
            egui::Grid::new(("frame-card", salt))
                .num_columns(2)
                .spacing(vec2(12.0, 2.0))
                .show(ui, |ui| {
                    for (layer, fields) in t.frame.layers() {
                        ui.label(RichText::new(layer).monospace().color(c.muted));
                        ui.label(RichText::new(fields).monospace().color(c.text));
                        ui.end_row();
                    }
                });
        })
        .response
        .interact(Sense::click())
}

fn step_view(app: &mut App, ui: &mut Ui) {
    let now = app.now;
    let Some(pb) = app.playback.as_mut() else {
        return;
    };
    let steps = pb.run.steps;
    let mut close = false;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(pb.step > 0, egui::Button::new(tr("Début", "First")))
            .clicked()
        {
            pb.playing = false;
            pb.go_to(0, now);
        }
        if ui
            .add_enabled(pb.step > 0, egui::Button::new(tr("← Précédent", "← Back")))
            .clicked()
        {
            pb.playing = false;
            pb.go_to(pb.step - 1, now);
        }
        if ui
            .add_enabled(!pb.at_end(), egui::Button::new(tr("Suivant →", "Next →")))
            .clicked()
        {
            pb.playing = false;
            pb.go_to(pb.step + 1, now);
        }
        let play_label = if pb.playing {
            "Pause"
        } else {
            tr("Lecture", "Play")
        };
        if ui
            .add_enabled(!pb.at_end() || pb.playing, egui::Button::new(play_label))
            .clicked()
        {
            pb.playing = !pb.playing;
            pb.step_started = now;
        }
        if ui
            .add_enabled(!pb.at_end(), egui::Button::new(tr("Fin", "Last")))
            .clicked()
        {
            pb.playing = false;
            pb.go_to(steps - 1, now);
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new({
                let n = pb.step + 1;
                tf!("Étape {n} sur {steps}", "Step {n} of {steps}")
            })
            .family(theme::medium())
            .color(pal().text),
        );
        let frames = pb.run.frames_at(pb.step).count();
        let (plural, header) = (if frames == 1 { "" } else { "s" }, &pb.run.header);
        ui.label(theme::muted(tf!(
            "· {frames} trame{plural} sur le câble · {header}",
            "· {frames} frame{plural} on the wire · {header}"
        )));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if theme::ghost_button(ui, tr("Fermer", "Close")).clicked() {
                close = true;
            }
        });
    });
    if close {
        app.playback = None;
        return;
    }

    let Some(pb) = app.playback.as_ref() else {
        return;
    };
    let step = pb.step;
    let mut clicked = None;
    theme::group_frame().show(ui, |ui| {
        ScrollArea::vertical()
            .id_salt(("step-view", step))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let run = &pb.run;
                for n in run.notes_at(step).filter(|n| n.before_frames) {
                    log_line(ui, &n.text, n.kind.into());
                }
                for (i, t) in run.frames_at(step) {
                    if frame_card(ui, app, t, pb.selected == Some(i), i).clicked() {
                        clicked = Some(i);
                    }
                    ui.add_space(4.0);
                }
                let after: Vec<_> = run.notes_at(step).filter(|n| !n.before_frames).collect();
                if !after.is_empty() {
                    ui.label(theme::eyebrow(tr(
                        "À l'arrivée de ces trames",
                        "When these frames arrive",
                    )));
                    for n in after {
                        log_line(ui, &n.text, n.kind.into());
                    }
                }
                if pb.at_end() {
                    ui.add_space(4.0);
                    let kind = if run.success {
                        LogKind::Success
                    } else {
                        LogKind::Failure
                    };
                    log_line(ui, &run.summary, kind);
                }
            });
    });
    if let (Some(i), Some(pb)) = (clicked, app.playback.as_mut()) {
        pb.selected = if pb.selected == Some(i) {
            None
        } else {
            Some(i)
        };
    }
}
