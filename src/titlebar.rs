//! The window's own title bar, drawn in the app's colours instead of the system one.
//!
//! Left: wordmark and menus. Centre: document name, with a Red dot when unsaved.
//! Right (Windows, Linux): minimise, maximise/restore, close. The empty parts of the bar
//! move the window, a double-click maximises it, and thin edges around the window resize it.
//! On macOS the native traffic lights stay (the content just extends under them).

use crate::app::App;
use crate::i18n::tr;
use crate::theme::{self, pal};
use eframe::egui::{
    self, Align, CornerRadius, CursorIcon, FontId, Id, Layout, Painter, Pos2, Rect,
    ResizeDirection, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2, ViewportCommand,
    pos2, vec2,
};

pub const HEIGHT: f32 = 44.0;
const BUTTON_WIDTH: f32 = 46.0;
const MACOS: bool = cfg!(target_os = "macos");
/// Room left for the macOS traffic lights.
const MACOS_LIGHTS: f32 = 68.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum WindowButton {
    Minimize,
    Maximize,
    Close,
}

/// Window options that go with this title bar.
pub fn viewport(builder: egui::ViewportBuilder) -> egui::ViewportBuilder {
    if MACOS {
        builder
            .with_fullsize_content_view(true)
            .with_title_shown(false)
            .with_titlebar_shown(false)
    } else {
        builder.with_decorations(false)
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let c = pal();
    let bar = ui.max_rect();
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));

    // Background first: every widget added afterwards takes its clicks before it.
    let background = ui.interact(bar, Id::unique("netlix-titlebar"), Sense::click_and_drag());
    if background.drag_started_by(egui::PointerButton::Primary) {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if background.double_clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    }

    document_name(app, ui.painter(), bar);

    let buttons_width = if MACOS { 0.0 } else { 3.0 * BUTTON_WIDTH };
    let left = Rect::from_min_max(bar.min, pos2(bar.right() - buttons_width, bar.bottom()));
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(left.shrink2(vec2(12.0, 6.0)))
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            if MACOS {
                ui.add_space(MACOS_LIGHTS);
            }
            ui.add(
                egui::Label::new(
                    RichText::new("netlix")
                        .family(theme::display())
                        .size(19.0)
                        .color(c.text)
                        .extra_letter_spacing(-0.4),
                )
                .selectable(false),
            );
            ui.add_space(10.0);
            egui::MenuBar::new().ui(ui, |ui| app.menus(ui));
        },
    );

    if !MACOS {
        let mut right = bar.right();
        for button in [
            WindowButton::Close,
            WindowButton::Maximize,
            WindowButton::Minimize,
        ] {
            right -= BUTTON_WIDTH;
            let rect = Rect::from_min_max(
                pos2(right, bar.top()),
                pos2(right + BUTTON_WIDTH, bar.bottom()),
            );
            if window_button(ui, rect, button, maximized).clicked() {
                ctx.send_viewport_cmd(match button {
                    WindowButton::Minimize => ViewportCommand::Minimized(true),
                    WindowButton::Maximize => ViewportCommand::Maximized(!maximized),
                    // Goes through the unsaved-changes prompt like any close request.
                    WindowButton::Close => ViewportCommand::Close,
                });
            }
        }
    }
}

/// "office.netlix" centred in the bar, followed by a Red dot while unsaved.
fn document_name(app: &App, painter: &Painter, bar: Rect) {
    let c = pal();
    let galley =
        painter.layout_no_wrap(app.file_name(), FontId::new(13.0, theme::medium()), c.muted);
    let size = galley.size();
    let origin = bar.center() - size / 2.0;
    painter.galley(origin, galley, c.muted);
    if app.dirty {
        painter.circle_filled(
            pos2(origin.x + size.x + 10.0, bar.center().y),
            3.5,
            c.accent,
        );
    }
}

fn window_button(ui: &mut Ui, rect: Rect, button: WindowButton, maximized: bool) -> egui::Response {
    let c = pal();
    let resp = ui.interact(
        rect,
        Id::unique(("netlix-window-button", button as u8)),
        Sense::click(),
    );
    let hot = resp.hovered() || resp.has_focus();
    let close = button == WindowButton::Close;
    if hot {
        let fill = if close { c.accent } else { c.fill_hover };
        ui.painter().rect_filled(rect, CornerRadius::ZERO, fill);
    }
    let color = if hot && close { c.on_accent } else { c.text };
    let p = ui.painter();
    let st = Stroke::new(1.2, color);
    let m = rect.center();
    match button {
        WindowButton::Minimize => {
            p.line_segment([m + vec2(-5.0, 0.0), m + vec2(5.0, 0.0)], st);
        }
        WindowButton::Maximize if maximized => {
            // Restore: two overlapping windows.
            let back = Rect::from_center_size(m + vec2(1.5, -1.5), Vec2::splat(8.0));
            p.line_segment([back.left_top(), back.right_top()], st);
            p.line_segment([back.right_top(), back.right_bottom()], st);
            let front = Rect::from_center_size(m + vec2(-1.0, 1.0), Vec2::splat(8.0));
            p.rect_stroke(front, CornerRadius::same(1), st, StrokeKind::Middle);
        }
        WindowButton::Maximize => {
            let r = Rect::from_center_size(m, Vec2::splat(10.0));
            p.rect_stroke(r, CornerRadius::same(1), st, StrokeKind::Middle);
        }
        WindowButton::Close => {
            p.line_segment([m + vec2(-5.0, -5.0), m + vec2(5.0, 5.0)], st);
            p.line_segment([m + vec2(-5.0, 5.0), m + vec2(5.0, -5.0)], st);
        }
    }
    let tip = match button {
        WindowButton::Minimize => tr("Réduire", "Minimise"),
        WindowButton::Maximize if maximized => tr("Restaurer", "Restore"),
        WindowButton::Maximize => tr("Agrandir", "Maximise"),
        WindowButton::Close => tr("Fermer", "Close"),
    };
    resp.on_hover_text(tip)
}

/// Thin areas along the window edges that resize it (the system frame is gone).
/// Call after everything else so they take precedence at the very edge.
pub fn resize_edges(ui: &mut Ui, window: Rect) {
    let ctx = ui.ctx().clone();
    let full = ctx.input(|i| {
        i.viewport().maximized.unwrap_or(false) || i.viewport().fullscreen.unwrap_or(false)
    });
    if MACOS || full {
        return;
    }
    let (t, k) = (5.0, 12.0); // edge thickness, corner size
    let r = window;
    let at = |x0: f32, y0: f32, x1: f32, y1: f32| {
        Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1))
    };
    let zones = [
        (
            at(r.left(), r.top(), r.left() + k, r.top() + k),
            ResizeDirection::NorthWest,
            CursorIcon::ResizeNorthWest,
        ),
        (
            at(r.right() - k, r.top(), r.right(), r.top() + k),
            ResizeDirection::NorthEast,
            CursorIcon::ResizeNorthEast,
        ),
        (
            at(r.left(), r.bottom() - k, r.left() + k, r.bottom()),
            ResizeDirection::SouthWest,
            CursorIcon::ResizeSouthWest,
        ),
        (
            at(r.right() - k, r.bottom() - k, r.right(), r.bottom()),
            ResizeDirection::SouthEast,
            CursorIcon::ResizeSouthEast,
        ),
        (
            at(r.left() + k, r.top(), r.right() - k, r.top() + t),
            ResizeDirection::North,
            CursorIcon::ResizeNorth,
        ),
        (
            at(r.left() + k, r.bottom() - t, r.right() - k, r.bottom()),
            ResizeDirection::South,
            CursorIcon::ResizeSouth,
        ),
        (
            at(r.left(), r.top() + k, r.left() + t, r.bottom() - k),
            ResizeDirection::West,
            CursorIcon::ResizeWest,
        ),
        (
            at(r.right() - t, r.top() + k, r.right(), r.bottom() - k),
            ResizeDirection::East,
            CursorIcon::ResizeEast,
        ),
    ];
    for (i, (rect, direction, cursor)) in zones.into_iter().enumerate() {
        let resp = ui.interact(rect, Id::unique(("netlix-resize", i)), Sense::drag());
        if resp.hovered() || resp.dragged() {
            ctx.set_cursor_icon(cursor);
        }
        if resp.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
        }
    }
}
