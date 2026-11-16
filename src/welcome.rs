//! First-launch screen: choose the interface language and the theme, then start.
//!
//! Both choices apply live, so the screen itself previews them. They stay available
//! later under View (Affichage), and the screen is not shown again once completed.

use crate::app::App;
use crate::i18n::{self, Lang, tr};
use crate::theme::{self, DARK, LIGHT, Palette, pal};
use eframe::egui::{
    self, Align, Align2, CornerRadius, FontId, Key, Layout, Painter, Pos2, Rect, Response,
    RichText, Role, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2, WidgetInfo, pos2, vec2,
};

const WIDTH: f32 = 560.0;
/// Approximate height of the content, to centre it vertically.
const HEIGHT: f32 = 520.0;
const GAP: f32 = 12.0;

pub fn show(app: &mut App, ui: &mut Ui) {
    let area = ui.max_rect();
    ui.painter()
        .rect_filled(area, CornerRadius::ZERO, pal().canvas);

    let width = WIDTH.min(area.width() - 48.0);
    let top = area.top() + ((area.height() - HEIGHT) / 2.0).max(24.0);
    let column = Rect::from_min_size(
        pos2(area.center().x - width / 2.0, top),
        vec2(width, HEIGHT),
    );

    ui.scope_builder(
        UiBuilder::new()
            .max_rect(column)
            .layout(Layout::top_down(Align::Min)),
        |ui| content(app, ui, width),
    );
}

fn content(app: &mut App, ui: &mut Ui, width: f32) {
    let ctx = ui.ctx().clone();
    let c = pal();
    ui.spacing_mut().item_spacing = vec2(GAP, 0.0);

    ui.label(theme::eyebrow(format!(
        "netlix {}",
        env!("CARGO_PKG_VERSION")
    )));
    ui.add_space(8.0);
    ui.label(
        RichText::new(tr("Bienvenue dans netlix", "Welcome to netlix"))
            .family(theme::display())
            .size(34.0)
            .color(c.text)
            .extra_letter_spacing(-0.8),
    );
    ui.add_space(10.0);
    ui.label(
        RichText::new(tr(
            "Dessinez un réseau, configurez les adresses IP et suivez chaque trame d'un ping. \
             Choisissez d'abord la langue et l'apparence.",
            "Draw a network, set IP addresses and follow every frame of a ping. \
             First, choose your language and how netlix looks.",
        ))
        .size(15.0)
        .color(c.body),
    );

    // Language. The label is bilingual: it is read before a language is chosen.
    ui.add_space(28.0);
    ui.label(theme::eyebrow("Langue · Language"));
    ui.add_space(8.0);
    let tile = vec2((width - GAP) / 2.0, 56.0);
    ui.horizontal(|ui| {
        for lang in Lang::ALL {
            let selected = i18n::lang() == lang;
            let resp = choice(ui, tile, selected, lang.native_name(), |p, r| {
                p.text(
                    r.left_center() + vec2(16.0, 0.0),
                    Align2::LEFT_CENTER,
                    lang.native_name(),
                    FontId::new(16.0, theme::medium()),
                    pal().text,
                );
            });
            if resp.clicked() && !selected {
                app.set_language(lang);
            }
        }
    });

    // Theme, each tile showing a small netlix window in its own colours.
    ui.add_space(20.0);
    ui.label(theme::eyebrow(tr("Apparence", "Appearance")));
    ui.add_space(8.0);
    let tile = vec2((width - GAP) / 2.0, 132.0);
    ui.horizontal(|ui| {
        for (dark, palette, name) in [
            (true, &DARK, tr("Sombre", "Dark")),
            (false, &LIGHT, tr("Clair", "Light")),
        ] {
            let selected = app.dark == dark;
            let resp = choice(ui, tile, selected, name, |p, r| {
                let preview = Rect::from_min_max(
                    r.min + vec2(12.0, 12.0),
                    pos2(r.right() - 12.0, r.bottom() - 40.0),
                );
                mini_window(p, preview, palette);
                p.text(
                    pos2(r.left() + 16.0, r.bottom() - 20.0),
                    Align2::LEFT_CENTER,
                    name,
                    FontId::new(15.0, theme::medium()),
                    pal().text,
                );
            });
            if resp.clicked() && !selected {
                app.dark = dark;
                theme::apply(&ctx, dark);
            }
        }
    });

    ui.add_space(32.0);
    let mut start = false;
    let mut example = false;
    ui.horizontal(|ui| {
        start = theme::primary_button(ui, tr("Commencer", "Get started")).clicked();
        // Only offered on an empty canvas: a file opened from the system stays as it is.
        if app.topo.devices.is_empty() {
            example = theme::ghost_button(
                ui,
                RichText::new(tr("Commencer avec un exemple", "Start with an example"))
                    .color(c.link),
            )
            .clicked();
        }
    });
    ui.add_space(14.0);
    ui.label(theme::muted(tr(
        "Ces réglages restent modifiables dans le menu Affichage.",
        "You can change these later in the View menu.",
    )));

    // Enter starts, unless a choice has the keyboard focus (Enter then selects it).
    let enter = ctx.input(|i| i.key_pressed(Key::Enter)) && ctx.memory(|m| m.focused().is_none());
    if example {
        app.load_example();
    }
    if start || example || enter {
        app.welcome = false;
    }
}

/// A selectable tile, used like a radio button: Red ring and filled dot when chosen.
fn choice(
    ui: &mut Ui,
    size: Vec2,
    selected: bool,
    label: &str,
    paint: impl FnOnce(&Painter, Rect),
) -> Response {
    let c = pal();
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| WidgetInfo::selected(Role::RadioButton, true, selected, label));
    let radius = CornerRadius::same(12);
    let fill = if selected {
        c.selected
    } else if resp.hovered() {
        c.fill_hover
    } else {
        c.fill
    };
    let p = ui.painter();
    p.rect_filled(rect, radius, fill);
    if selected {
        p.rect_stroke(rect, radius, Stroke::new(2.0, c.accent), StrokeKind::Inside);
    }
    if resp.has_focus() {
        p.rect_stroke(
            rect.expand(3.0),
            CornerRadius::same(15),
            Stroke::new(2.0, c.ring),
            StrokeKind::Outside,
        );
    }
    paint(p, rect);

    // Radio dot, top right for tall tiles, centred on the right for short ones.
    let dot = if rect.height() > 80.0 {
        pos2(rect.right() - 24.0, rect.bottom() - 20.0)
    } else {
        pos2(rect.right() - 24.0, rect.center().y)
    };
    if selected {
        p.circle_filled(dot, 9.0, c.accent);
        p.circle_filled(dot, 3.5, c.on_accent);
    } else {
        p.circle_stroke(dot, 8.0, Stroke::new(1.5, c.muted));
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// A netlix window in miniature: title bar, toolbox, canvas with a small topology.
fn mini_window(p: &Painter, r: Rect, c: &Palette) {
    p.rect_filled(r, CornerRadius::same(8), c.canvas);
    let bar = Rect::from_min_max(r.min, pos2(r.right(), r.top() + 12.0));
    p.rect_filled(
        bar,
        CornerRadius {
            nw: 8,
            ne: 8,
            sw: 0,
            se: 0,
        },
        c.panel,
    );
    let side = Rect::from_min_max(
        pos2(r.left(), bar.bottom()),
        pos2(r.left() + 34.0, r.bottom()),
    );
    p.rect_filled(
        side,
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: 8,
            se: 0,
        },
        c.panel,
    );
    for i in 0..3 {
        let y = side.top() + 10.0 + i as f32 * 12.0;
        p.rect_filled(
            Rect::from_min_size(pos2(side.left() + 7.0, y), vec2(20.0, 6.0)),
            CornerRadius::same(2),
            c.fill_hover,
        );
    }

    let stage = Rect::from_min_max(pos2(side.right(), bar.bottom()), r.max);
    let at = |x: f32, y: f32| -> Pos2 { stage.lerp_inside(vec2(x, y)) };
    let (a, b, router, d) = (
        at(0.18, 0.32),
        at(0.18, 0.72),
        at(0.5, 0.52),
        at(0.82, 0.52),
    );
    for end in [a, b, d] {
        p.line_segment([router, end], Stroke::new(1.5, c.wire));
    }
    for host in [a, b, d] {
        p.rect_filled(
            Rect::from_center_size(host, Vec2::splat(14.0)),
            CornerRadius::same(4),
            c.tile,
        );
        p.rect_stroke(
            Rect::from_center_size(host, Vec2::splat(14.0)),
            CornerRadius::same(4),
            Stroke::new(1.0, c.hairline),
            StrokeKind::Inside,
        );
    }
    p.circle_filled(router, 9.0, c.accent);
}
