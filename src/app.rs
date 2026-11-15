//! Application state, menus, file handling and keyboard shortcuts.

use crate::model::{DeviceId, DeviceKind, Topology};
use crate::panels::EditBuffers;
use crate::{canvas, examples, io, panels, sim, theme};
use eframe::egui::{
    self, CentralPanel, Frame, Key, KeyboardShortcut, Margin, Modifiers, Panel, RichText, Ui, Vec2,
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

pub struct LogLine {
    pub text: String,
    pub kind: LogKind,
}

pub struct Animation {
    pub path: Vec<DeviceId>,
    pub request_len: usize,
    pub started: f64,
    pub success: bool,
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
    pub log: Vec<LogLine>,
    pub anim: Option<Animation>,
    pub animate: bool,
    pub dark: bool,
    pub edit: EditBuffers,
    pub now: f64,
    pending: Option<Pending>,
    allow_close: bool,
    last_title: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        // Dark unless the user switched to light in an earlier session.
        let prefs: Prefs = cc
            .storage
            .and_then(|s| eframe::get_value(s, PREFS_KEY))
            .unwrap_or_default();
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
            anim: None,
            animate: prefs.animate,
            dark: prefs.dark,
            edit: EditBuffers::default(),
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
        self.anim = None;
        self.edit.reset();
    }

    pub fn delete_selection(&mut self) {
        match self.selection {
            Selection::Device(id) => {
                self.topo.remove_device(id);
                if self.ping_src == Some(id) {
                    self.ping_src = None;
                }
                self.anim = None;
            }
            Selection::Link(i) if i < self.topo.links.len() => {
                self.topo.links.remove(i);
                self.anim = None;
            }
            _ => return,
        }
        self.selection = Selection::None;
        self.edit.reset();
        self.mark_dirty();
    }

    pub fn send_ping(&mut self) {
        let Some(src) = self.ping_src else {
            self.set_status("Choose a source device first.", true);
            return;
        };
        let Ok(dst) = self.ping_dst.trim().parse() else {
            self.set_status(
                "The destination must be an IPv4 address, e.g. 10.0.0.2.",
                true,
            );
            return;
        };
        let result = sim::ping(&self.topo, src, dst);
        let outcome = if result.success {
            LogKind::Success
        } else {
            LogKind::Failure
        };
        for (i, text) in result.lines.into_iter().enumerate() {
            let kind = if i == 0 { LogKind::Info } else { outcome };
            self.log.push(LogLine { text, kind });
        }
        self.anim = (self.animate && result.path.len() > 1).then_some(Animation {
            path: result.path,
            request_len: result.request_len,
            started: self.now,
            success: result.success,
        });
    }

    fn file_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or_else(|| "Untitled".into(), |n| n.to_string_lossy().into_owned())
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
            Pending::Example => {
                self.replace_topology(examples::office(), None);
                self.ping_src = self.topo.devices.first().map(|d| d.id);
                self.ping_dst = "8.8.8.8".into();
                self.set_status("Example loaded. Press \"Send ping\" to try it.", false);
            }
            Pending::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        match io::load(&path) {
            Ok(topo) => {
                self.replace_topology(topo, Some(path));
                self.set_status(format!("Opened {}", self.file_name()), false);
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
                self.set_status(format!("Saved {}", self.file_name()), false);
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
        if pressed(Key::Escape) {
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

    fn menu_bar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.label(
                RichText::new("netlix")
                    .family(theme::display())
                    .size(20.0)
                    .color(theme::pal().text)
                    .extra_letter_spacing(-0.4),
            );
            ui.add_space(12.0);
            ui.menu_button("File", |ui| {
                if ui
                    .add(egui::Button::new("New").shortcut_text(
                        ctx.format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::N)),
                    ))
                    .clicked()
                {
                    self.request(Pending::New, &ctx);
                }
                if ui
                    .add(egui::Button::new("Open…").shortcut_text(
                        ctx.format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::O)),
                    ))
                    .clicked()
                {
                    self.request(Pending::Open, &ctx);
                }
                if ui.button("Load example").clicked() {
                    self.request(Pending::Example, &ctx);
                }
                ui.separator();
                if ui
                    .add(egui::Button::new("Save").shortcut_text(
                        ctx.format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::S)),
                    ))
                    .clicked()
                {
                    self.save();
                }
                if ui
                    .add(
                        egui::Button::new("Save as…").shortcut_text(ctx.format_shortcut(
                            &KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::S),
                        )),
                    )
                    .clicked()
                {
                    self.save_as();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    self.request(Pending::Quit, &ctx);
                }
            });
            ui.menu_button("Edit", |ui| {
                let has_selection = self.selection != Selection::None;
                if ui
                    .add_enabled(
                        has_selection,
                        egui::Button::new("Delete selection").shortcut_text("Del"),
                    )
                    .clicked()
                {
                    self.delete_selection();
                }
                if ui.button("Clear ping log").clicked() {
                    self.log.clear();
                }
            });
            ui.menu_button("View", |ui| {
                if ui
                    .add(egui::Button::new("Reset view").shortcut_text(
                        ctx.format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Num0)),
                    ))
                    .clicked()
                {
                    self.view = View::default();
                }
                ui.checkbox(&mut self.animate, "Animate packets");
                if ui.checkbox(&mut self.dark, "Dark mode").changed() {
                    theme::apply(&ctx, self.dark);
                }
            });
            ui.menu_button("Help", |ui| {
                ui.label(theme::muted("netlix — network topology simulator"));
                ui.label(theme::muted(format!(
                    "Version {}",
                    env!("CARGO_PKG_VERSION")
                )));
                ui.separator();
                ui.label("1–5  place a device");
                ui.label("L  link tool · V  select · Esc  cancel");
                ui.label("Scroll to zoom · drag the background to pan");
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let name = self.file_name();
                ui.label(theme::muted(if self.dirty {
                    format!("{name} · unsaved")
                } else {
                    name
                }));
            });
        });
    }

    fn pending_modal(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending else { return };
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("netlix-unsaved")).show(ctx, |ui| {
            ui.set_max_width(360.0);
            ui.label(theme::title("Save changes?"));
            ui.add_space(4.0);
            ui.label(theme::muted(format!(
                "{} has unsaved changes. They will be lost if you don't save them.",
                self.file_name()
            )));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if theme::primary_button(ui, "Save").clicked() {
                    choice = Some(true);
                }
                if ui
                    .add(egui::Button::new("Don't save").min_size(egui::vec2(0.0, 36.0)))
                    .clicked()
                {
                    choice = Some(false);
                }
                if theme::ghost_button(ui, "Cancel").clicked()
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
        .add_filter("netlix topology", &[io::EXTENSION])
        .add_filter("All files", &["*"])
}

const PREFS_KEY: &str = "netlix-prefs";

/// Settings kept between sessions (eframe's app storage).
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Prefs {
    dark: bool,
    animate: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            dark: true,
            animate: true,
        }
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let prefs = Prefs {
            dark: self.dark,
            animate: self.animate,
        };
        eframe::set_value(storage, PREFS_KEY, &prefs);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.now = ctx.input(|i| i.time);
        self.handle_close(&ctx);
        self.shortcuts(&ctx);
        self.update_title(&ctx);

        Panel::top("menu")
            .frame(
                Frame::new()
                    .fill(theme::pal().panel)
                    .inner_margin(Margin::symmetric(16, 8)),
            )
            .show(ui, |ui| self.menu_bar(ui));
        Panel::bottom("console")
            .resizable(true)
            .default_size(176.0)
            .min_size(120.0)
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
        Panel::right("inspector")
            .resizable(true)
            .default_size(304.0)
            .min_size(260.0)
            .frame(
                Frame::new()
                    .fill(theme::pal().panel)
                    .inner_margin(Margin::same(16)),
            )
            .show(ui, |ui| panels::inspector(self, ui));
        CentralPanel::no_frame().show(ui, |ui| canvas::show(self, ui));

        self.pending_modal(&ctx);
    }
}
