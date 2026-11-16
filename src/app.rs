//! Application state, menus, file handling and keyboard shortcuts.

use crate::i18n::{self, Lang, tr};
use crate::model::{DeviceId, DeviceKind, Topology};
use crate::panels::EditBuffers;
use crate::{canvas, examples, io, panels, sim, tf, theme, titlebar, welcome};
use eframe::egui::{
    self, CentralPanel, Frame, Key, KeyboardShortcut, Margin, Modifiers, Panel, Ui, Vec2,
    ViewportCommand,
};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Select,
    Place(DeviceKind),
    Link,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Selection {
    None,
    Device(DeviceId),
    Link(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pending {
    New,
    Open,
    Example,
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Info,
    Success,
    Failure,
}

impl From<sim::NoteKind> for LogKind {
    fn from(k: sim::NoteKind) -> Self {
        match k {
            sim::NoteKind::Info => LogKind::Info,
            sim::NoteKind::Success => LogKind::Success,
            sim::NoteKind::Failure => LogKind::Failure,
        }
    }
}

pub enum LogEntry {
    Line {
        text: String,
        kind: LogKind,
    },
    /// A transmitted frame with its explanation (shown when "Explain every frame" is on).
    Frame(sim::Transmission),
}

/// How a ping is played.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SendMode {
    /// The whole exchange at once, animated quickly.
    Realtime,
    /// One step at a time, to inspect every frame.
    StepByStep,
}

/// A ping being shown on the canvas.
pub struct Playback {
    pub run: sim::Run,
    pub mode: SendMode,
    /// Step currently shown.
    pub step: usize,
    pub step_started: f64,
    pub playing: bool,
    /// Transmission highlighted on the canvas (index into `run.transmissions`).
    pub selected: Option<usize>,
    outcome_logged: bool,
    /// What the run was computed from, to compute it again in another language.
    initial: sim::NetState,
    dst: std::net::Ipv4Addr,
}

impl Playback {
    pub fn step_seconds(&self) -> f64 {
        match self.mode {
            SendMode::Realtime => 0.32,
            SendMode::StepByStep => 0.7,
        }
    }

    pub fn at_end(&self) -> bool {
        self.step + 1 >= self.run.steps
    }

    pub fn go_to(&mut self, step: usize, now: f64) {
        let step = step.min(self.run.steps.saturating_sub(1));
        if step != self.step {
            self.step = step;
            self.step_started = now;
            self.selected = None;
        }
    }
}

pub struct Status {
    pub text: String,
    pub error: bool,
    pub at: f64,
}

pub struct View {
    pub offset: Vec2,
    pub zoom: f32,
}

impl Default for View {
    fn default() -> Self {
        Self {
            offset: Vec2::new(120.0, 120.0),
            zoom: 1.0,
        }
    }
}

pub struct App {
    pub topo: Topology,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub tool: Tool,
    pub selection: Selection,
    pub view: View,
    pub link_from: Option<DeviceId>,
    pub status: Option<Status>,
    pub ping_src: Option<DeviceId>,
    pub ping_dst: String,
    pub log: Vec<LogEntry>,
    pub playback: Option<Playback>,
    pub mode: SendMode,
    /// Log every frame with its explanation in real-time mode.
    pub explain: bool,
    /// ARP caches and switch MAC tables, kept between pings.
    pub net: sim::NetState,
    pub animate: bool,
    pub dark: bool,
    pub edit: EditBuffers,
    /// Device whose configuration window is open.
    pub config: Option<DeviceId>,
    /// Where that window appears (next to the device, near the top).
    pub config_pos: egui::Pos2,
    /// Move the window to `config_pos` on its next frame.
    pub config_reposition: bool,
    /// Cable under the last right-click on the canvas.
    pub context_link: Option<usize>,
    /// The first-launch screen (language and theme) is showing.
    pub welcome: bool,
    pub now: f64,
    pending: Option<Pending>,
    allow_close: bool,
    last_title: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        // Settings from earlier sessions. On the very first launch the language starts
        // from the system's, and the welcome screen lets the user confirm both choices.
        let stored: Option<Prefs> = cc.storage.and_then(|s| eframe::get_value(s, PREFS_KEY));
        let first_launch = stored.is_none();
        let mut prefs = stored.unwrap_or_default();
        if first_launch {
            prefs.lang = i18n::system_lang();
        }
        i18n::set_lang(prefs.lang);
        theme::install(&cc.egui_ctx, prefs.dark);
        let mut app = Self {
            topo: Topology::default(),
            path: None,
            dirty: false,
            tool: Tool::Select,
            selection: Selection::None,
            view: View::default(),
            link_from: None,
            status: None,
            ping_src: None,
            ping_dst: String::new(),
            log: Vec::new(),
            playback: None,
            mode: prefs.mode(),
            explain: prefs.explain,
            net: sim::NetState::default(),
            animate: prefs.animate,
            dark: prefs.dark,
            edit: EditBuffers::default(),
            config: None,
            config_pos: egui::pos2(320.0, 120.0),
            config_reposition: false,
            context_link: None,
            welcome: !prefs.welcomed,
            now: 0.0,
            pending: None,
            allow_close: false,
            last_title: String::new(),
        };
        if let Some(path) = open {
            app.open_path(path);
        }
        app
    }

    // ── State helpers ──────────────────────────────────────────────────────

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn set_status(&mut self, text: impl Into<String>, error: bool) {
        self.status = Some(Status {
            text: text.into(),
            error,
            at: self.now,
        });
    }

    fn replace_topology(&mut self, topo: Topology, path: Option<PathBuf>) {
        self.topo = topo;
        self.path = path;
        self.dirty = false;
        self.tool = Tool::Select;
        self.selection = Selection::None;
        self.view = View::default();
        self.link_from = None;
        self.ping_src = None;
        self.playback = None;
        self.net = sim::NetState::default();
        self.config = None;
        self.edit.reset();
    }

    pub fn delete_selection(&mut self) {
        match self.selection {
            Selection::Device(id) => {
                self.topo.remove_device(id);
                if self.config == Some(id) {
                    self.config = None;
                }
                if self.ping_src == Some(id) {
                    self.ping_src = None;
                }
                self.playback = None;
            }
            Selection::Link(i) if i < self.topo.links.len() => {
                self.topo.links.remove(i);
                self.playback = None;
            }
            _ => return,
        }
        self.selection = Selection::None;
        self.edit.reset();
        self.mark_dirty();
    }

    pub fn send_ping(&mut self) {
        let Some(src) = self.ping_src else {
            self.set_status(
                tr(
                    "Choisissez d'abord un appareil source.",
                    "Choose a source device first.",
                ),
                true,
            );
            return;
        };
        let Ok(dst) = self.ping_dst.trim().parse() else {
            self.set_status(
                tr(
                    "La destination doit être une adresse IPv4, par exemple 10.0.0.2.",
                    "The destination must be an IPv4 address, e.g. 10.0.0.2.",
                ),
                true,
            );
            return;
        };
        let initial = self.net.clone();
        let run = sim::ping(&self.topo, &initial, src, dst);
        self.net = run.final_state.clone();
        self.log.push(LogEntry::Line {
            text: run.header.clone(),
            kind: LogKind::Info,
        });
        match self.mode {
            SendMode::Realtime => {
                if self.explain {
                    self.log_steps(&run);
                }
                self.log_outcome(&run);
            }
            SendMode::StepByStep => self.log.push(LogEntry::Line {
                text: tr(
                    "Pas à pas : utilisez Suivant (→) pour suivre chaque trame.",
                    "Step by step: use Next (→) to follow each frame.",
                )
                .into(),
                kind: LogKind::Info,
            }),
        }
        let animated = self.mode == SendMode::StepByStep || self.animate;
        self.playback = (animated && run.steps > 0).then(|| Playback {
            playing: self.mode == SendMode::Realtime,
            mode: self.mode,
            step: 0,
            step_started: self.now,
            selected: None,
            outcome_logged: self.mode == SendMode::Realtime,
            run,
            initial,
            dst,
        });
    }

    /// Switch the interface language. The ping being shown is simulated again so its
    /// explanations follow; lines already in the log keep their language.
    pub fn set_language(&mut self, lang: Lang) {
        i18n::set_lang(lang);
        if let Some(pb) = &mut self.playback {
            pb.run = sim::ping(&self.topo, &pb.initial, pb.run.source, pb.dst);
            pb.step = pb.step.min(pb.run.steps.saturating_sub(1));
            pb.selected = None;
        }
    }

    /// Every frame and device decision of a run, in order.
    fn log_steps(&mut self, run: &sim::Run) {
        for step in 0..run.steps.max(1) {
            for n in run.notes_at(step).filter(|n| n.before_frames) {
                self.log.push(LogEntry::Line {
                    text: n.text.clone(),
                    kind: n.kind.into(),
                });
            }
            for (_, t) in run.frames_at(step) {
                self.log.push(LogEntry::Frame(t.clone()));
            }
            for n in run.notes_at(step).filter(|n| !n.before_frames) {
                self.log.push(LogEntry::Line {
                    text: n.text.clone(),
                    kind: n.kind.into(),
                });
            }
        }
    }

    fn log_outcome(&mut self, run: &sim::Run) {
        self.log.push(LogEntry::Line {
            text: run.summary.clone(),
            kind: if run.success {
                LogKind::Success
            } else {
                LogKind::Failure
            },
        });
    }

    /// Advance the playback clock; log the outcome when a step-by-step run reaches its end.
    fn tick(&mut self, ctx: &egui::Context) {
        self.net.prune(&self.topo);
        let now = self.now;
        let mut finished = false;
        let mut outcome = None;
        if let Some(pb) = &mut self.playback {
            let dur = pb.step_seconds();
            let elapsed = now - pb.step_started;
            if pb.playing && elapsed >= dur {
                if !pb.at_end() {
                    pb.step += 1;
                    pb.step_started = now;
                } else if pb.mode == SendMode::Realtime {
                    // Keep the outcome badge up for a moment, then clear the canvas.
                    finished = elapsed >= dur + 2.5;
                } else {
                    pb.playing = false;
                }
            }
            if !pb.outcome_logged && pb.at_end() {
                pb.outcome_logged = true;
                outcome = Some((pb.run.summary.clone(), pb.run.success));
            }
            if pb.playing || elapsed < dur + 0.1 {
                ctx.request_repaint();
            }
        }
        if finished {
            self.playback = None;
        }
        if let Some((text, ok)) = outcome {
            self.log.push(LogEntry::Line {
                text,
                kind: if ok {
                    LogKind::Success
                } else {
                    LogKind::Failure
                },
            });
        }
    }

    pub fn step_by(&mut self, delta: isize) {
        let now = self.now;
        if let Some(pb) = &mut self.playback {
            pb.playing = false;
            let target = pb.step.saturating_add_signed(delta);
            pb.go_to(target, now);
        }
    }

    /// Caches and tables as they are at the step being shown (live state otherwise).
    pub fn view_state(&self) -> &sim::NetState {
        match &self.playback {
            Some(pb) if pb.mode == SendMode::StepByStep && !pb.run.states.is_empty() => {
                &pb.run.states[pb.step.min(pb.run.states.len() - 1)]
            }
            _ => &self.net,
        }
    }

    /// Open the configuration window of a device (and select it).
    pub fn open_config(&mut self, id: DeviceId, near: egui::Pos2) {
        if self.config != Some(id) {
            self.edit.reset();
        }
        self.selection = Selection::Device(id);
        self.config = Some(id);
        self.config_pos = near;
        self.config_reposition = true;
    }

    pub fn clear_arp(&mut self, device: Option<DeviceId>) {
        self.net.clear_arp(device);
        self.playback = None;
        self.set_status(
            if device.is_some() {
                tr("Cache ARP vidé.", "ARP cache cleared.")
            } else {
                tr(
                    "Tous les caches ARP ont été vidés.",
                    "All ARP caches cleared.",
                )
            },
            false,
        );
    }

    pub fn clear_mac_tables(&mut self, device: Option<DeviceId>) {
        self.net.clear_mac_tables(device);
        self.playback = None;
        self.set_status(
            if device.is_some() {
                tr("Table MAC vidée.", "MAC table cleared.")
            } else {
                tr(
                    "Toutes les tables MAC des switchs ont été vidées.",
                    "All switch MAC tables cleared.",
                )
            },
            false,
        );
    }

    pub fn file_name(&self) -> String {
        self.path.as_ref().and_then(|p| p.file_name()).map_or_else(
            || tr("Sans titre", "Untitled").into(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    // ── Files ──────────────────────────────────────────────────────────────

    fn request(&mut self, action: Pending, ctx: &egui::Context) {
        if self.dirty {
            self.pending = Some(action);
        } else {
            self.perform(action, ctx);
        }
    }

    fn perform(&mut self, action: Pending, ctx: &egui::Context) {
        match action {
            Pending::New => self.replace_topology(Topology::default(), None),
            Pending::Open => {
                if let Some(path) = file_dialog().pick_file() {
                    self.open_path(path);
                }
            }
            Pending::Example => self.load_example(),
            Pending::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    /// Replace the canvas with the office example, ready to ping the Internet.
    pub fn load_example(&mut self) {
        self.replace_topology(examples::office(), None);
        self.ping_src = self.topo.devices.first().map(|d| d.id);
        self.ping_dst = "8.8.8.8".into();
        self.set_status(
            tr(
                "Exemple chargé. Cliquez sur « Envoyer le ping » pour l'essayer.",
                "Example loaded. Press \"Send ping\" to try it.",
            ),
            false,
        );
    }

    fn open_path(&mut self, path: PathBuf) {
        match io::load(&path) {
            Ok(topo) => {
                self.replace_topology(topo, Some(path));
                let name = self.file_name();
                self.set_status(tf!("Ouvert : {name}", "Opened {name}"), false);
            }
            Err(e) => self.set_status(e, true),
        }
    }

    /// Returns true when the topology ended up on disk.
    fn save(&mut self) -> bool {
        match self.path.clone() {
            Some(path) => self.save_to(path),
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) -> bool {
        let name = format!(
            "{}.{}",
            self.file_name().trim_end_matches(".netlix"),
            io::EXTENSION
        );
        match file_dialog().set_file_name(name).save_file() {
            Some(mut path) => {
                if path.extension().is_none() {
                    path.set_extension(io::EXTENSION);
                }
                self.save_to(path)
            }
            None => false,
        }
    }

    fn save_to(&mut self, path: PathBuf) -> bool {
        match io::save(&path, &self.topo) {
            Ok(()) => {
                self.path = Some(path);
                self.dirty = false;
                let name = self.file_name();
                self.set_status(tf!("Enregistré : {name}", "Saved {name}"), false);
                true
            }
            Err(e) => {
                self.set_status(e, true);
                false
            }
        }
    }

    // ── Frame ──────────────────────────────────────────────────────────────

    fn handle_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.allow_close {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = |key| KeyboardShortcut::new(Modifiers::COMMAND, key);
        let [save_as, save, open, new, reset] = ctx.input_mut(|i| {
            [
                i.consume_shortcut(&KeyboardShortcut::new(
                    Modifiers::COMMAND | Modifiers::SHIFT,
                    Key::S,
                )),
                i.consume_shortcut(&cmd(Key::S)),
                i.consume_shortcut(&cmd(Key::O)),
                i.consume_shortcut(&cmd(Key::N)),
                i.consume_shortcut(&cmd(Key::Num0)),
            ]
        });
        if save_as {
            self.save_as();
        } else if save {
            self.save();
        }
        if open {
            self.request(Pending::Open, ctx);
        }
        if new {
            self.request(Pending::New, ctx);
        }
        if reset {
            self.view = View::default();
        }

        // Single-key shortcuts only when no text field has focus.
        if ctx.egui_wants_keyboard_input() || self.pending.is_some() {
            return;
        }
        let pressed = |k| ctx.input(|i| i.key_pressed(k) && i.modifiers.is_none());
        if pressed(Key::Escape) && self.config.is_some() {
            self.config = None;
        } else if pressed(Key::Escape) {
            self.tool = Tool::Select;
            self.link_from = None;
        }
        if pressed(Key::Delete) || pressed(Key::Backspace) {
            self.delete_selection();
        }
        if pressed(Key::V) {
            self.tool = Tool::Select;
            self.link_from = None;
        }
        if pressed(Key::L) {
            self.tool = Tool::Link;
        }
        if self
            .playback
            .as_ref()
            .is_some_and(|pb| pb.mode == SendMode::StepByStep)
        {
            if pressed(Key::ArrowRight) || pressed(Key::Space) {
                self.step_by(1);
            }
            if pressed(Key::ArrowLeft) {
                self.step_by(-1);
            }
        }
        let digits = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5];
        for (key, kind) in digits.into_iter().zip(DeviceKind::ALL) {
            if pressed(key) {
                self.tool = Tool::Place(kind);
                self.link_from = None;
            }
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = format!(
            "{}{} — netlix",
            self.file_name(),
            if self.dirty { " •" } else { "" }
        );
        if title != self.last_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }

    /// The File / Edit / View / Help menus (laid out by the title bar).
    pub fn menus(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        {
            ui.menu_button(tr("Fichier", "File"), |ui| {
                if ui
                    .add(egui::Button::new(tr("Nouveau", "New")).shortcut_text(
                        ctx.format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::N)),
                    ))
                    .clicked()
                {
                    self.request(Pending::New, &ctx);
                }
                if ui
                    .add(egui::Button::new(tr("Ouvrir…", "Open…")).shortcut_text(
                        ctx.format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::O)),
                    ))
                    .clicked()
                {
                    self.request(Pending::Open, &ctx);
                }
                if ui.button(tr("Charger l'exemple", "Load example")).clicked() {
                    self.request(Pending::Example, &ctx);
                }
                ui.separator();
                if ui
                    .add(egui::Button::new(tr("Enregistrer", "Save")).shortcut_text(
                        ctx.format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::S)),
                    ))
                    .clicked()
                {
                    self.save();
                }
                if ui
                    .add(
                        egui::Button::new(tr("Enregistrer sous…", "Save as…")).shortcut_text(
                            ctx.format_shortcut(&KeyboardShortcut::new(
                                Modifiers::COMMAND | Modifiers::SHIFT,
                                Key::S,
                            )),
                        ),
                    )
                    .clicked()
                {
                    self.save_as();
                }
                ui.separator();
                if ui.button(tr("Quitter", "Quit")).clicked() {
                    self.request(Pending::Quit, &ctx);
                }
            });
            ui.menu_button(tr("Édition", "Edit"), |ui| {
                let has_selection = self.selection != Selection::None;
                if ui
                    .add_enabled(
                        has_selection,
                        egui::Button::new(tr("Supprimer la sélection", "Delete selection"))
                            .shortcut_text(tr("Suppr", "Del")),
                    )
                    .clicked()
                {
                    self.delete_selection();
                }
                if ui
                    .button(tr("Effacer le journal du ping", "Clear ping log"))
                    .clicked()
                {
                    self.log.clear();
                }
                ui.separator();
                if ui
                    .button(tr("Vider tous les caches ARP", "Clear all ARP caches"))
                    .clicked()
                {
                    self.clear_arp(None);
                }
                if ui
                    .button(tr("Vider toutes les tables MAC", "Clear all MAC tables"))
                    .clicked()
                {
                    self.clear_mac_tables(None);
                }
            });
            ui.menu_button(tr("Affichage", "View"), |ui| {
                if ui
                    .add(
                        egui::Button::new(tr("Réinitialiser la vue", "Reset view")).shortcut_text(
                            ctx.format_shortcut(&KeyboardShortcut::new(
                                Modifiers::COMMAND,
                                Key::Num0,
                            )),
                        ),
                    )
                    .clicked()
                {
                    self.view = View::default();
                }
                ui.checkbox(
                    &mut self.animate,
                    tr("Animer les paquets", "Animate packets"),
                );
                if ui
                    .checkbox(&mut self.dark, tr("Mode sombre", "Dark mode"))
                    .changed()
                {
                    theme::apply(&ctx, self.dark);
                }
                ui.separator();
                ui.menu_button(tr("Langue", "Language"), |ui| {
                    for lang in Lang::ALL {
                        if ui.radio(i18n::lang() == lang, lang.native_name()).clicked() {
                            self.set_language(lang);
                            ui.close();
                        }
                    }
                });
            });
            ui.menu_button(tr("Aide", "Help"), |ui| {
                ui.label(theme::muted(tr(
                    "netlix — simulateur de topologie réseau",
                    "netlix — network topology simulator",
                )));
                ui.label(theme::muted(format!(
                    "Version {}",
                    env!("CARGO_PKG_VERSION")
                )));
                ui.separator();
                ui.label(tr("1–5  placer un appareil", "1–5  place a device"));
                ui.label(tr(
                    "L  outil lien · V  sélection · Échap  annuler",
                    "L  link tool · V  select · Esc  cancel",
                ));
                ui.label(tr(
                    "Molette pour zoomer · glisser le fond pour déplacer la vue",
                    "Scroll to zoom · drag the background to pan",
                ));
                ui.label(tr(
                    "Pas à pas : → ou Espace étape suivante, ← précédente",
                    "Step by step: → or Space next, ← back",
                ));
                ui.separator();
                ui.label(theme::eyebrow(tr("Fonctionnement", "How it works")));
                ui.set_max_width(420.0);
                for tip in panels::tips() {
                    ui.label(theme::muted(tip));
                }
            });
        }
    }

    fn pending_modal(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending else { return };
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("netlix-unsaved")).show(ctx, |ui| {
            ui.set_max_width(360.0);
            ui.label(theme::title(tr(
                "Enregistrer les modifications ?",
                "Save changes?",
            )));
            ui.add_space(4.0);
            let name = self.file_name();
            ui.label(theme::muted(tf!(
                "{name} contient des modifications non enregistrées. Elles seront perdues si \
                 vous ne les enregistrez pas.",
                "{name} has unsaved changes. They will be lost if you don't save them."
            )));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if theme::primary_button(ui, tr("Enregistrer", "Save")).clicked() {
                    choice = Some(true);
                }
                if ui
                    .add(
                        egui::Button::new(tr("Ne pas enregistrer", "Don't save"))
                            .min_size(egui::vec2(0.0, 36.0)),
                    )
                    .clicked()
                {
                    choice = Some(false);
                }
                if theme::ghost_button(ui, tr("Annuler", "Cancel")).clicked()
                    || ui.input(|i| i.key_pressed(Key::Escape))
                {
                    self.pending = None;
                }
            });
        });
        match choice {
            Some(true) => {
                self.pending = None;
                if self.save() {
                    self.perform(action, ctx);
                }
            }
            Some(false) => {
                self.pending = None;
                self.dirty = false;
                self.perform(action, ctx);
            }
            None => {}
        }
    }
}

fn file_dialog() -> rfd::FileDialog {
    rfd::FileDialog::new()
        .add_filter(tr("Topologie netlix", "netlix topology"), &[io::EXTENSION])
        .add_filter(tr("Tous les fichiers", "All files"), &["*"])
}

const PREFS_KEY: &str = "netlix-prefs";

/// Settings kept between sessions (eframe's app storage).
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Prefs {
    lang: Lang,
    dark: bool,
    animate: bool,
    step_by_step: bool,
    explain: bool,
    /// The first-launch screen has been completed.
    welcomed: bool,
}

impl Prefs {
    fn mode(&self) -> SendMode {
        if self.step_by_step {
            SendMode::StepByStep
        } else {
            SendMode::Realtime
        }
    }
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            lang: Lang::Fr,
            dark: true,
            animate: true,
            step_by_step: false,
            explain: false,
            welcomed: false,
        }
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let prefs = Prefs {
            lang: i18n::lang(),
            dark: self.dark,
            animate: self.animate,
            step_by_step: self.mode == SendMode::StepByStep,
            explain: self.explain,
            welcomed: !self.welcome,
        };
        eframe::set_value(storage, PREFS_KEY, &prefs);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.now = ctx.input(|i| i.time);
        self.handle_close(&ctx);
        if !self.welcome {
            self.shortcuts(&ctx);
            self.tick(&ctx);
        }
        self.update_title(&ctx);

        let window = ui.max_rect();
        Panel::top("titlebar")
            .exact_size(titlebar::HEIGHT)
            .frame(Frame::new().fill(theme::pal().panel))
            .show(ui, |ui| titlebar::show(self, ui));
        if self.welcome {
            CentralPanel::no_frame().show(ui, |ui| welcome::show(self, ui));
            titlebar::resize_edges(ui, window);
            self.pending_modal(&ctx);
            return;
        }
        Panel::bottom("console")
            .resizable(true)
            .default_size(330.0)
            .min_size(140.0)
            .frame(
                Frame::new()
                    .fill(theme::pal().panel)
                    .inner_margin(Margin::same(16)),
            )
            .show(ui, |ui| panels::console(self, ui));
        Panel::left("toolbox")
            .resizable(false)
            .exact_size(188.0)
            .frame(
                Frame::new()
                    .fill(theme::pal().panel)
                    .inner_margin(Margin::same(12)),
            )
            .show(ui, |ui| panels::toolbox(self, ui));
        CentralPanel::no_frame().show(ui, |ui| canvas::show(self, ui));
        titlebar::resize_edges(ui, window);

        panels::config_window(self, &ctx);
        self.pending_modal(&ctx);
    }
}
