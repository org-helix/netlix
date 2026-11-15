//! The topology canvas: pan/zoom, placing and dragging devices, drawing links,
//! and the packet animation.

use crate::app::{App, Selection, Tool};
use crate::model::{Device, DeviceId, DeviceKind};
use crate::theme::{self, pal};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontFamily, FontId, Galley, Painter, Pos2, Rect, Sense,
    Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};
use std::sync::Arc;

/// Device tile size in world units.
pub const TILE: f32 = 64.0;
const GRID: f32 = 32.0;
/// Seconds per link crossed by the animated packet.
const HOP_SECONDS: f64 = 0.32;
const MIN_ZOOM: f32 = 0.3;
const MAX_ZOOM: f32 = 3.0;

struct Xform {
    origin: Pos2,
    zoom: f32,
}

impl Xform {
    fn to_screen(&self, w: [f32; 2]) -> Pos2 {
        self.origin + vec2(w[0], w[1]) * self.zoom
    }
    fn to_world(&self, p: Pos2) -> [f32; 2] {
        let v = (p - self.origin) / self.zoom;
        [v.x, v.y]
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let rect = ui.max_rect();
    let bg = ui.allocate_rect(rect, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, pal().canvas);

    pan_and_zoom(app, ui, &bg, rect);
    let xf = Xform {
        origin: rect.min + app.view.offset,
        zoom: app.view.zoom,
    };

    draw_grid(&painter, rect, &xf);
    draw_links(app, &painter, &xf);

    // Devices: register interactions on top of the background.
    let mut responses = Vec::with_capacity(app.topo.devices.len());
    for dev in &app.topo.devices {
        let tile = Rect::from_center_size(xf.to_screen(dev.pos), Vec2::splat(TILE * xf.zoom));
        let resp = ui.interact(
            tile,
            ui.scope_id().with(("device", dev.id)),
            Sense::click_and_drag(),
        );
        responses.push((dev.id, resp));
    }
    for (id, resp) in &responses {
        draw_device(app, &painter, &xf, *id, resp.hovered() || resp.dragged());
    }
    for (id, resp) in responses {
        handle_device(app, &xf, id, &resp);
    }

    handle_background(app, &xf, &bg);
    draw_rubber_band(app, ui, &painter, &xf);
    draw_packet(app, ui, &painter, &xf);
    draw_overlays(app, ui, &painter, rect);
}

fn pan_and_zoom(app: &mut App, ui: &Ui, bg: &egui::Response, rect: Rect) {
    let pan_with_primary = app.tool == Tool::Select || app.tool == Tool::Link;
    if bg.dragged_by(egui::PointerButton::Middle)
        || (pan_with_primary && bg.dragged_by(egui::PointerButton::Primary))
    {
        app.view.offset += bg.drag_delta();
    }
    let Some(pointer) = ui
        .input(|i| i.pointer.hover_pos())
        .filter(|p| rect.contains(*p))
    else {
        return;
    };
    if !ui.rect_contains_pointer(rect) {
        return;
    }
    let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
    let factor = pinch * (scroll / 400.0).exp();
    if (factor - 1.0).abs() > f32::EPSILON {
        let old = app.view.zoom;
        let new = (old * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        // Keep the world point under the cursor fixed.
        let anchor = pointer - rect.min;
        app.view.offset = anchor - (anchor - app.view.offset) * (new / old);
        app.view.zoom = new;
    }
}

fn draw_grid(painter: &Painter, rect: Rect, xf: &Xform) {
    let step = GRID * xf.zoom;
    if step < 12.0 {
        return;
    }
    let start = xf.to_world(rect.min);
    let end = xf.to_world(rect.max);
    let first = |v: f32| (v / GRID).floor() * GRID;
    let radius = (1.2 * xf.zoom).clamp(0.8, 1.6);
    let mut x = first(start[0]);
    while x <= end[0] {
        let mut y = first(start[1]);
        while y <= end[1] {
            painter.circle_filled(xf.to_screen([x, y]), radius, pal().grid);
            y += GRID;
        }
        x += GRID;
    }
}

fn center(app: &App, xf: &Xform, id: DeviceId) -> Option<Pos2> {
    app.topo.device(id).map(|d| xf.to_screen(d.pos))
}

fn draw_links(app: &App, painter: &Painter, xf: &Xform) {
    let label_font = FontId::new((11.0 * xf.zoom).clamp(9.0, 15.0), FontFamily::Monospace);
    for (i, link) in app.topo.links.iter().enumerate() {
        let (Some(a), Some(b)) = (
            center(app, xf, link.a.device),
            center(app, xf, link.b.device),
        ) else {
            continue;
        };
        let selected = app.selection == Selection::Link(i);
        let stroke = if selected {
            Stroke::new(3.0, pal().accent)
        } else {
            Stroke::new(2.0, pal().wire)
        };
        painter.line_segment([a, b], stroke);

        if xf.zoom < 0.6 {
            continue;
        }
        let dir = (b - a).normalized();
        let mut labels = Vec::with_capacity(2);
        for (end, from, d) in [(link.a, a, dir), (link.b, b, -dir)] {
            let Some(dev) = app.topo.device(end.device) else {
                continue;
            };
            let galley = painter.layout_no_wrap(
                app.topo.iface_name(end).to_owned(),
                label_font.clone(),
                pal().muted,
            );
            // Put the port name just outside the device tile and its caption.
            let (tile, caption) = footprint(painter, dev, xf);
            let exit = ray_exit(from, d, tile, caption);
            let extent = d.x.abs() * galley.size().x / 2.0 + d.y.abs() * galley.size().y / 2.0;
            let pos = from + d * (exit + 6.0 + extent);
            labels.push((pos, galley, exit + 6.0 + 2.0 * extent));
        }
        // Skip labels when the link is too short to hold both.
        if labels.len() == 2 && labels[0].2 + labels[1].2 < (b - a).length() {
            for (pos, galley, _) in labels {
                let pill = Rect::from_center_size(pos, galley.size() + vec2(8.0, 2.0));
                painter.rect_filled(pill, CornerRadius::same(4), pal().canvas);
                painter.galley(pill.center() - galley.size() / 2.0, galley, pal().muted);
            }
        }
    }
}

/// Lines written under a device: name, then up to two addresses.
fn caption(painter: &Painter, dev: &Device, zoom: f32) -> Vec<Arc<Galley>> {
    let name_font = FontId::new((13.0 * zoom).clamp(9.0, 22.0), theme::medium());
    let mut out = vec![painter.layout_no_wrap(dev.name.clone(), name_font, pal().text)];
    if zoom >= 0.55 {
        let ip_font = FontId::new((11.0 * zoom).clamp(9.0, 16.0), FontFamily::Monospace);
        let ips: Vec<String> = dev
            .interfaces
            .iter()
            .filter_map(|i| i.ip.map(|c| format!("{}/{}", c.addr, c.prefix)))
            .collect();
        let mut lines: Vec<String> = ips.iter().take(2).cloned().collect();
        if ips.len() > 2 {
            lines.push(format!("+{} more", ips.len() - 2));
        }
        for line in lines {
            out.push(painter.layout_no_wrap(line, ip_font.clone(), pal().muted));
        }
    }
    out
}

/// Screen rects of a device's tile and of its caption block.
fn footprint(painter: &Painter, dev: &Device, xf: &Xform) -> (Rect, Rect) {
    let c = xf.to_screen(dev.pos);
    let tile = Rect::from_center_size(c, Vec2::splat(TILE * xf.zoom));
    let lines = caption(painter, dev, xf.zoom);
    let w = lines.iter().map(|g| g.size().x).fold(0.0, f32::max);
    let h: f32 = lines.iter().map(|g| g.size().y).sum();
    let top = tile.bottom() + 6.0 * xf.zoom;
    (
        tile,
        Rect::from_min_size(pos2(c.x - w / 2.0, top), vec2(w, h)).expand(2.0),
    )
}

/// Distance along `d` (unit) from `c` until the ray leaves the tile and, if it runs
/// into it right after, the caption.
fn ray_exit(c: Pos2, d: Vec2, tile: Rect, caption: Rect) -> f32 {
    let slab = |r: Rect| {
        let (mut t0, mut t1) = (f32::NEG_INFINITY, f32::INFINITY);
        for (o, dir, lo, hi) in [(c.x, d.x, r.min.x, r.max.x), (c.y, d.y, r.min.y, r.max.y)] {
            if dir.abs() < 1e-6 {
                if o < lo || o > hi {
                    return None;
                }
            } else {
                let (a, b) = ((lo - o) / dir, (hi - o) / dir);
                t0 = t0.max(a.min(b));
                t1 = t1.min(a.max(b));
            }
        }
        (t0 <= t1).then_some((t0, t1))
    };
    let t_tile = slab(tile).map_or(0.0, |(_, t1)| t1);
    match slab(caption) {
        Some((t0, t1)) if t0 <= t_tile + 8.0 && t1 > t_tile => t1,
        _ => t_tile,
    }
}

fn draw_device(app: &App, painter: &Painter, xf: &Xform, id: DeviceId, hovered: bool) {
    let Some(dev) = app.topo.device(id) else {
        return;
    };
    let center = xf.to_screen(dev.pos);
    let size = TILE * xf.zoom;
    let tile = Rect::from_center_size(center, Vec2::splat(size));
    let selected = app.selection == Selection::Device(id);
    let c = pal();
    let fill = if selected { c.selected } else { c.tile };
    let radius = CornerRadius::same((12.0 * xf.zoom).clamp(4.0, 24.0) as u8);
    painter.rect_filled(tile, radius, fill);
    if hovered && !selected {
        painter.rect_filled(tile, radius, c.fill);
    }
    if selected {
        painter.rect_stroke(
            tile.expand(2.0),
            radius,
            Stroke::new(2.0, c.ring),
            StrokeKind::Outside,
        );
    }
    if app.link_from == Some(id) {
        painter.rect_stroke(
            tile.expand(2.0),
            radius,
            Stroke::new(2.0, c.accent),
            StrokeKind::Outside,
        );
    }
    draw_icon(painter, center, size * 0.62, dev.kind, c.text, fill);

    // Caption on a canvas-coloured backing so links passing behind stay legible.
    let (_, block) = footprint(painter, dev, xf);
    painter.rect_filled(block, CornerRadius::same(4), c.canvas);
    let mut y = block.top() + 2.0;
    for galley in caption(painter, dev, xf.zoom) {
        let size = galley.size();
        painter.galley(pos2(center.x - size.x / 2.0, y), galley, c.text);
        y += size.y;
    }
}

fn handle_device(app: &mut App, xf: &Xform, id: DeviceId, resp: &egui::Response) {
    match app.tool {
        Tool::Select | Tool::Place(_) => {
            if resp.drag_started() || resp.clicked() {
                if app.selection != Selection::Device(id) {
                    app.edit.reset();
                }
                app.selection = Selection::Device(id);
                app.tool = Tool::Select;
            }
            if resp.dragged() && resp.drag_delta() != Vec2::ZERO {
                let d = resp.drag_delta() / xf.zoom;
                if let Some(dev) = app.topo.device_mut(id) {
                    dev.pos[0] += d.x;
                    dev.pos[1] += d.y;
                }
                app.mark_dirty();
            }
            if resp.drag_stopped() {
                snap(app, id);
            }
        }
        Tool::Link => {
            if resp.clicked() {
                match app.link_from.take() {
                    None => app.link_from = Some(id),
                    Some(from) if from == id => {}
                    Some(from) => match app.topo.connect(from, id) {
                        Ok(i) => {
                            let l = app.topo.links[i];
                            let text = format!(
                                "Linked {} {} ↔ {} {}",
                                dev_name(app, l.a.device),
                                app.topo.iface_name(l.a),
                                dev_name(app, l.b.device),
                                app.topo.iface_name(l.b)
                            );
                            app.set_status(text, false);
                            app.selection = Selection::Link(i);
                            app.mark_dirty();
                        }
                        Err(e) => app.set_status(e.to_string(), true),
                    },
                }
            }
        }
    }

    resp.context_menu(|ui| {
        if ui.button("Ping from here").clicked() {
            app.ping_src = Some(id);
            ui.close();
        }
        if ui.button("Link from here").clicked() {
            app.tool = Tool::Link;
            app.link_from = Some(id);
            ui.close();
        }
        ui.separator();
        if ui
            .button(egui::RichText::new("Delete").color(pal().error))
            .clicked()
        {
            app.selection = Selection::Device(id);
            app.delete_selection();
            ui.close();
        }
    });
}

/// Align a dropped device on the grid so diagrams stay tidy.
fn snap(app: &mut App, id: DeviceId) {
    if let Some(dev) = app.topo.device_mut(id) {
        let half = GRID / 2.0;
        dev.pos = dev.pos.map(|v| (v / half).round() * half);
    }
}

fn dev_name(app: &App, id: DeviceId) -> String {
    app.topo
        .device(id)
        .map_or_else(|| "?".into(), |d| d.name.clone())
}

fn handle_background(app: &mut App, xf: &Xform, bg: &egui::Response) {
    if !bg.clicked() {
        return;
    }
    let Some(p) = bg.interact_pointer_pos() else {
        return;
    };
    match app.tool {
        Tool::Place(kind) => {
            let w = xf.to_world(p);
            let half = GRID / 2.0;
            let id = app
                .topo
                .add_device(kind, w.map(|v| (v / half).round() * half));
            app.edit.reset();
            app.selection = Selection::Device(id);
            if !bg.ctx.input(|i| i.modifiers.shift) {
                app.tool = Tool::Select;
            }
            app.mark_dirty();
        }
        Tool::Link => app.link_from = None,
        Tool::Select => {
            let hit = link_at(app, xf, p);
            app.edit.reset();
            app.selection = hit.map_or(Selection::None, Selection::Link);
        }
    }
}

fn link_at(app: &App, xf: &Xform, p: Pos2) -> Option<usize> {
    app.topo
        .links
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            let a = center(app, xf, l.a.device)?;
            let b = center(app, xf, l.b.device)?;
            Some((i, distance_to_segment(p, a, b)))
        })
        .filter(|(_, d)| *d <= 6.0)
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(i, _)| i)
}

fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 == 0.0 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

fn draw_rubber_band(app: &App, ui: &Ui, painter: &Painter, xf: &Xform) {
    let (Some(from), Some(pointer)) = (app.link_from, ui.input(|i| i.pointer.hover_pos())) else {
        return;
    };
    if let Some(a) = center(app, xf, from) {
        painter.extend(Shape::dashed_line(
            &[a, pointer],
            Stroke::new(2.0, pal().accent),
            6.0,
            4.0,
        ));
    }
}

fn draw_packet(app: &mut App, ui: &Ui, painter: &Painter, xf: &Xform) {
    let Some(anim) = &app.anim else { return };
    let segments = anim.path.len().saturating_sub(1);
    let progress = (app.now - anim.started) / HOP_SECONDS;
    let lingering = progress - segments as f64;

    if lingering >= 0.0 {
        // Path done: show the outcome briefly on the last device, then stop.
        if lingering > 3.0 {
            app.anim = None;
            return;
        }
        if let Some(c) = anim.path.last().and_then(|id| center(app, xf, *id)) {
            let (glyph, color) = if anim.success {
                ("✓", pal().success)
            } else {
                ("✕", pal().error)
            };
            let at = c + vec2(TILE * 0.5, -TILE * 0.5) * xf.zoom;
            painter.circle_filled(at, 11.0, color);
            painter.text(
                at,
                Align2::CENTER_CENTER,
                glyph,
                FontId::proportional(14.0),
                pal().on_status,
            );
        }
        ui.ctx().request_repaint();
        return;
    }

    let idx = progress.floor() as usize;
    let t = progress.fract() as f32;
    let (Some(a), Some(b)) = (
        center(app, xf, anim.path[idx]),
        center(app, xf, anim.path[idx + 1]),
    ) else {
        app.anim = None;
        return;
    };
    let p = a + (b - a) * t;
    // Request in Red, reply in the text colour.
    let color = if idx + 1 < anim.request_len {
        pal().accent
    } else {
        pal().reply
    };
    painter.circle_filled(p, 8.0, color);
    painter.circle_filled(p, 3.0, pal().canvas);
    ui.ctx().request_repaint();
}

fn draw_overlays(app: &mut App, ui: &Ui, painter: &Painter, rect: Rect) {
    // Empty state.
    if app.topo.devices.is_empty() {
        let c = rect.center();
        painter.text(
            c - vec2(0.0, 14.0),
            Align2::CENTER_BOTTOM,
            "Start a topology",
            FontId::new(26.0, theme::display()),
            pal().text,
        );
        painter.text(
            c,
            Align2::CENTER_TOP,
            "Pick a device on the left and click here to place it,\nor use File → Load example.",
            FontId::proportional(15.0),
            pal().muted,
        );
    }

    // Tool hint, top-left.
    let hint = match app.tool {
        Tool::Select => None,
        Tool::Place(kind) => Some(format!(
            "Click to place a {} · Shift keeps the tool · Esc cancels",
            kind_name(kind)
        )),
        Tool::Link if app.link_from.is_some() => {
            Some("Click the second device · Esc cancels".to_owned())
        }
        Tool::Link => Some("Click the first device to link · Esc cancels".to_owned()),
    };
    if let Some(hint) = hint {
        pill(
            painter,
            rect.left_top() + vec2(16.0, 16.0),
            Align2::LEFT_TOP,
            &hint,
            pal().inverse,
            pal().on_inverse,
        );
    }

    // Status message, bottom-left, fades after a few seconds.
    if let Some(status) = &app.status {
        let age = app.now - status.at;
        if age < 4.0 {
            let (bg, fg) = if status.error {
                (pal().error, pal().on_status)
            } else {
                (pal().inverse, pal().on_inverse)
            };
            pill(
                painter,
                rect.left_bottom() + vec2(16.0, -16.0),
                Align2::LEFT_BOTTOM,
                &status.text,
                bg,
                fg,
            );
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        } else {
            app.status = None;
        }
    }

    // Zoom level, bottom-right.
    painter.text(
        rect.right_bottom() + vec2(-16.0, -16.0),
        Align2::RIGHT_BOTTOM,
        format!("{:.0} %", app.view.zoom * 100.0),
        FontId::monospace(12.0),
        pal().muted,
    );
}

fn kind_name(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Cloud => "Internet cloud",
        k => k.label(),
    }
}

fn pill(painter: &Painter, at: Pos2, align: Align2, text: &str, bg: Color32, fg: Color32) {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(13.0), fg);
    let size = galley.size() + vec2(24.0, 12.0);
    let rect = align.anchor_size(at, size);
    painter.rect_filled(rect, CornerRadius::same(255), bg);
    painter.galley(rect.min + vec2(12.0, 6.0), galley, fg);
}

// ── Icons (drawn, no image assets): 2 px-equivalent strokes in currentColor ──

pub fn draw_icon(p: &Painter, c: Pos2, s: f32, kind: DeviceKind, color: Color32, bg: Color32) {
    let w = (s * 0.07).max(1.4);
    let st = Stroke::new(w, color);
    let r = CornerRadius::same((s * 0.08).clamp(1.0, 12.0) as u8);
    match kind {
        DeviceKind::Pc => {
            let screen = Rect::from_center_size(c + vec2(0.0, -0.1 * s), vec2(0.8 * s, 0.52 * s));
            p.rect_stroke(screen, r, st, StrokeKind::Middle);
            let foot = screen.bottom() + 0.16 * s;
            p.line_segment([pos2(c.x, screen.bottom()), pos2(c.x, foot)], st);
            p.line_segment([pos2(c.x - 0.2 * s, foot), pos2(c.x + 0.2 * s, foot)], st);
        }
        DeviceKind::Server => {
            let body = Rect::from_center_size(c, vec2(0.54 * s, 0.84 * s));
            p.rect_stroke(body, r, st, StrokeKind::Middle);
            for k in [-1.0, 1.0] {
                let y = c.y + k * 0.14 * s;
                p.line_segment([pos2(body.left(), y), pos2(body.right(), y)], st);
            }
            for k in [-0.28, 0.0, 0.28] {
                p.circle_filled(pos2(body.right() - 0.12 * s, c.y + k * s), w * 0.8, color);
            }
        }
        DeviceKind::Switch => {
            let body = Rect::from_center_size(c, vec2(0.92 * s, 0.5 * s));
            p.rect_stroke(body, r, st, StrokeKind::Middle);
            let dy = 0.09 * s;
            let dx = 0.25 * s;
            arrow(p, pos2(c.x - dx, c.y - dy), pos2(c.x + dx, c.y - dy), st);
            arrow(p, pos2(c.x + dx, c.y + dy), pos2(c.x - dx, c.y + dy), st);
        }
        DeviceKind::Router => {
            p.circle_stroke(c, 0.44 * s, st);
            let d = 0.24 * s;
            arrow(p, pos2(c.x - d, c.y), pos2(c.x + d, c.y), st);
            arrow(p, pos2(c.x, c.y + d), pos2(c.x, c.y - d), st);
        }
        DeviceKind::Cloud => {
            cloud(p, c, s, color, 0.0);
            cloud(p, c, s, bg, w);
        }
    }
}

fn arrow(p: &Painter, from: Pos2, to: Pos2, st: Stroke) {
    p.line_segment([from, to], st);
    let dir = (to - from).normalized();
    let normal = vec2(-dir.y, dir.x);
    let h = st.width * 2.6;
    p.line_segment([to, to - dir * h + normal * h], st);
    p.line_segment([to, to - dir * h - normal * h], st);
}

fn cloud(p: &Painter, c: Pos2, s: f32, color: Color32, inset: f32) {
    for (x, y, r) in [(-0.22, 0.06, 0.18), (0.0, -0.06, 0.26), (0.22, 0.06, 0.18)] {
        p.circle_filled(c + vec2(x * s, y * s), r * s - inset, color);
    }
    let base = Rect::from_min_max(c + vec2(-0.22 * s, 0.06 * s), c + vec2(0.22 * s, 0.24 * s))
        .shrink(inset);
    p.rect_filled(base, 0.0, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_distance() {
        let a = pos2(0.0, 0.0);
        let b = pos2(10.0, 0.0);
        assert_eq!(distance_to_segment(pos2(5.0, 3.0), a, b), 3.0);
        assert_eq!(distance_to_segment(pos2(-4.0, 3.0), a, b), 5.0);
        assert_eq!(distance_to_segment(pos2(1.0, 1.0), a, a), 2f32.sqrt());
    }
}
