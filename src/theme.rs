//! Helix look for egui: Helix colour palette on Dusk DS structure
//! (Bricolage Grotesque titles, Geist text, 4 px grid, radii 8/12/16, no drop shadows,
//! depth from translucent fills instead of borders).
//!
//! Two variants share every rule and differ only in colour:
//! - dark (default): the "dark Helix sky" — deepened ink canvas,
//!   Pale Amber text at a few opacities, Red accent, Blush focus;
//! - light: Pale Amber canvas, ink text, Red accent.

use eframe::egui::{
    self, Button, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin,
    Response, RichText, Shadow, Stroke, TextStyle, Ui, vec2,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

// ── Helix brand palette ────────────────────────────────────────────────────
const RED: Color32 = Color32::from_rgb(0xB2, 0x4E, 0x30); // the only action accent
const RUST: Color32 = Color32::from_rgb(0x86, 0x3B, 0x24); // Red under the pointer
const INK: Color32 = Color32::from_rgb(0x32, 0x29, 0x21);
const AMBER: Color32 = Color32::from_rgb(0xFA, 0xF6, 0xED);
const PAPER: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
const DUSTY: Color32 = Color32::from_rgb(0x63, 0x52, 0x42);
const PALE_ORANGE: Color32 = Color32::from_rgb(0xEC, 0xD3, 0xCB);
const BLUSH: Color32 = Color32::from_rgb(0xDC, 0xAF, 0xA2);

const fn ink(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied_const(0x32, 0x29, 0x21, alpha)
}
const fn amber(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied_const(0xFA, 0xF6, 0xED, alpha)
}

/// Semantic colours. UI code only ever reads these, through [`pal`].
pub struct Palette {
    pub dark: bool,
    /// Behind the topology.
    pub canvas: Color32,
    /// Side panels, menus, dialogs.
    pub panel: Color32,
    /// Device tiles on the canvas.
    pub tile: Color32,
    /// Text fields.
    pub field: Color32,
    /// Titles, labels, icons.
    pub text: Color32,
    /// Running text.
    pub body: Color32,
    /// Secondary text (meta, hints, port names).
    pub muted: Color32,
    pub faint: Color32,
    pub fill: Color32,
    pub fill_hover: Color32,
    pub fill_active: Color32,
    pub hairline: Color32,
    pub grid: Color32,
    /// Cables between devices.
    pub wire: Color32,
    /// Selected tile, tool row, list item.
    pub selected: Color32,
    /// Selection and keyboard-focus ring.
    pub ring: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub on_accent: Color32,
    pub link: Color32,
    pub error: Color32,
    pub success: Color32,
    /// Text on an `error` / `success` badge or toast.
    pub on_status: Color32,
    /// The echo reply dot (the request uses `accent`).
    pub reply: Color32,
    /// Floating hints and toasts: the opposite of the canvas.
    pub inverse: Color32,
    pub on_inverse: Color32,
}

pub const LIGHT: Palette = Palette {
    dark: false,
    canvas: AMBER,
    panel: PAPER,
    tile: PAPER,
    field: AMBER,
    text: INK,
    body: ink(217),
    muted: DUSTY,
    faint: ink(10),
    fill: ink(15),
    fill_hover: ink(26),
    fill_active: ink(31),
    hairline: ink(31),
    grid: ink(26),
    wire: ink(115),
    selected: PALE_ORANGE,
    ring: INK,
    accent: RED,
    accent_hover: RUST,
    on_accent: PAPER,
    link: RED,
    error: RUST,
    // Helix: extrapolated — the palette has no success colour. Deep green, AA on Paper and Amber.
    success: Color32::from_rgb(0x3E, 0x6B, 0x48),
    on_status: PAPER,
    reply: INK,
    inverse: INK,
    on_inverse: AMBER,
};

pub const DARK: Palette = Palette {
    dark: true,
    canvas: Color32::from_rgb(0x1E, 0x19, 0x14), // ink, deepened (top of the Helix sky)
    panel: Color32::from_rgb(0x28, 0x20, 0x1A),  // floating plate, opaque
    tile: Color32::from_rgb(0x3C, 0x33, 0x2B),   // ink lifted toward Pale Amber
    field: Color32::from_rgb(0x1E, 0x19, 0x14),
    text: AMBER,
    body: amber(204),
    muted: amber(166),
    faint: amber(10),
    fill: amber(20),
    fill_hover: amber(36),
    fill_active: amber(31),
    hairline: amber(20),
    grid: amber(22),
    wire: amber(90),
    selected: Color32::from_rgb(0x5C, 0x32, 0x23), // ink/rust step of the Helix sky
    ring: BLUSH,
    accent: RED,
    accent_hover: RUST,
    on_accent: PAPER,
    link: BLUSH,
    // Helix: extrapolated — the palette has no status colours; these are AA on the dark canvas.
    error: Color32::from_rgb(0xFF, 0x8A, 0x8A),
    success: Color32::from_rgb(0x4A, 0xDE, 0x80),
    on_status: INK,
    reply: AMBER,
    inverse: AMBER,
    on_inverse: INK,
};

static IS_DARK: AtomicBool = AtomicBool::new(true);

/// The palette in use.
pub fn pal() -> &'static Palette {
    if IS_DARK.load(Ordering::Relaxed) {
        &DARK
    } else {
        &LIGHT
    }
}

// ── Type ───────────────────────────────────────────────────────────────────
pub fn display() -> FontFamily {
    FontFamily::Name("display".into())
}
pub fn medium() -> FontFamily {
    FontFamily::Name("medium".into())
}
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn install(ctx: &egui::Context, dark: bool) {
    install_fonts(ctx);
    apply(ctx, dark);
}

/// Switch between the dark and light variants. The OS theme is ignored: the app's own
/// setting (dark unless changed in View) wins.
pub fn apply(ctx: &egui::Context, dark: bool) {
    IS_DARK.store(dark, Ordering::Relaxed);
    ctx.set_theme(if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
    ctx.all_styles_mut(style);
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let data = [
        (
            "bricolage",
            &include_bytes!("../assets/fonts/BricolageGrotesqueDisplay-SemiBold.ttf")[..],
        ),
        (
            "geist",
            &include_bytes!("../assets/fonts/Geist-Regular.ttf")[..],
        ),
        (
            "geist-medium",
            &include_bytes!("../assets/fonts/Geist-Medium.ttf")[..],
        ),
        (
            "geist-semibold",
            &include_bytes!("../assets/fonts/Geist-SemiBold.ttf")[..],
        ),
        (
            "geist-mono",
            &include_bytes!("../assets/fonts/GeistMono-Regular.ttf")[..],
        ),
    ];
    for (name, bytes) in data {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }

    // egui's bundled fonts stay as fallbacks for symbols Geist doesn't have (✓ ✕ …).
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let with_fallback = |first: &str| {
        let mut list = vec![first.to_owned()];
        list.extend(fallback.iter().cloned());
        list
    };
    fonts
        .families
        .insert(FontFamily::Proportional, with_fallback("geist"));
    fonts.families.insert(display(), with_fallback("bricolage"));
    fonts
        .families
        .insert(medium(), with_fallback("geist-medium"));
    fonts
        .families
        .insert(semibold(), with_fallback("geist-semibold"));
    if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
        mono.insert(0, "geist-mono".into());
    }
    ctx.set_fonts(fonts);
}

fn style(style: &mut egui::Style) {
    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new(12.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, medium())),
        (TextStyle::Heading, FontId::new(20.0, display())),
        (
            TextStyle::Monospace,
            FontId::new(13.0, FontFamily::Monospace),
        ),
    ]
    .into();

    let s = &mut style.spacing;
    s.item_spacing = vec2(8.0, 8.0);
    s.button_padding = vec2(12.0, 6.0);
    s.interact_size = vec2(32.0, 32.0);
    s.window_margin = Margin::same(16);
    s.menu_margin = Margin::same(8);
    s.combo_width = 160.0;
    s.text_edit_width = 160.0;

    let c = pal();
    let v = &mut style.visuals;
    *v = if c.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = c.panel;
    v.window_fill = c.panel;
    v.window_stroke = Stroke::new(1.0, c.hairline); // hairline: menus sit on same-colour panels
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.window_corner_radius = CornerRadius::same(16);
    v.menu_corner_radius = CornerRadius::same(12);
    v.extreme_bg_color = c.field;
    v.text_edit_bg_color = Some(c.field);
    v.faint_bg_color = c.faint;
    v.code_bg_color = c.fill;
    v.hyperlink_color = c.link;
    v.warn_fg_color = c.error;
    v.error_fg_color = c.error;
    v.selection.bg_fill = c.selected;
    v.selection.stroke = Stroke::new(2.0, c.ring); // also the focus outline of text fields
    v.text_cursor.stroke = Stroke::new(2.0, c.ring);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = c.panel;
    w.noninteractive.weak_bg_fill = c.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, c.hairline); // hairline separators
    w.noninteractive.fg_stroke = Stroke::new(1.0, c.body); // body text
    w.noninteractive.corner_radius = CornerRadius::same(8);
    for (wv, fill) in [
        (&mut w.inactive, c.fill),
        (&mut w.hovered, c.fill_hover),
        (&mut w.active, c.fill_active),
        (&mut w.open, c.fill_hover),
    ] {
        wv.bg_fill = fill;
        wv.weak_bg_fill = fill;
        wv.bg_stroke = Stroke::NONE; // no outlined controls
        wv.fg_stroke = Stroke::new(1.5, c.text);
        wv.corner_radius = CornerRadius::same(8);
        wv.expansion = 0.0;
    }
}

// ── Small building blocks ──────────────────────────────────────────────────

/// Section title: Bricolage 600, negative tracking.
pub fn title(text: impl Into<String>) -> RichText {
    RichText::new(text)
        .family(display())
        .size(20.0)
        .color(pal().text)
        .extra_letter_spacing(-0.4)
}

/// Small label above a group (sentence case, never forced capitals).
pub fn eyebrow(text: impl Into<String>) -> RichText {
    RichText::new(text)
        .family(medium())
        .size(12.0)
        .color(pal().muted)
}

pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).color(pal().muted)
}

/// The single primary action of a zone: Red fill, Paper text, Rust on hover.
pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    ui.scope(|ui| {
        let c = pal();
        let w = &mut ui.visuals_mut().widgets;
        for (wv, fill) in [
            (&mut w.inactive, c.accent),
            (&mut w.hovered, c.accent_hover),
            (&mut w.active, c.accent_hover),
        ] {
            wv.bg_fill = fill;
            wv.weak_bg_fill = fill;
            wv.fg_stroke.color = c.on_accent;
        }
        ui.add(Button::new(RichText::new(text).family(medium())).min_size(vec2(0.0, 36.0)))
    })
    .inner
}

/// Text-only action (Dusk "ghost").
pub fn ghost_button(ui: &mut Ui, text: impl Into<RichText>) -> Response {
    ui.add(
        Button::new(text.into())
            .frame(false)
            .min_size(vec2(0.0, 32.0)),
    )
}

/// Translucent fill block used for grouped content (no border, no shadow).
pub fn group_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(pal().faint)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(12))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG 2 relative luminance (alpha ignored: only opaque pairs are checked).
    fn luminance(c: Color32) -> f64 {
        let ch = |v: u8| {
            let v = f64::from(v) / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f64 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn text_pairs_meet_wcag_aa() {
        for p in [&LIGHT, &DARK] {
            let name = if p.dark { "dark" } else { "light" };
            for (fg, bg, what) in [
                (p.text, p.canvas, "text on canvas"),
                (p.text, p.panel, "text on panel"),
                (p.text, p.tile, "text on tile"),
                (p.text, p.selected, "text on selection"),
                (p.on_accent, p.accent, "primary button"),
                (p.on_accent, p.accent_hover, "hovered primary button"),
                (p.error, p.panel, "error text"),
                (p.success, p.panel, "success text"),
                (p.on_inverse, p.inverse, "hint pill"),
                (p.on_status, p.error, "error toast"),
                (p.ring, p.selected, "selected list item"),
            ] {
                let ratio = contrast(fg, bg);
                assert!(ratio >= 4.5, "{name}: {what} is {ratio:.2}:1");
            }
        }
    }
}
