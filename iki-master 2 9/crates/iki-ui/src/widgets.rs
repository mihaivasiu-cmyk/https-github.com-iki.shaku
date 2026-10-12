//! Hardware-style controls drawn with the egui painter (knobs, faders, pads,
//! rocker switches, segmented mode switches, modules).

use egui::{
    self,
    epaint::{Mesh, Vertex, WHITE_UV},
    Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
};

use crate::theme::{blend, Theme};
use iki_core::shaku::design::{motion, FINE_DRAG};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Accent {
    Moss,
    Iris,
}

impl Accent {
    pub fn color(self, t: &Theme) -> Color32 {
        match self {
            Accent::Moss => t.moss,
            Accent::Iris => t.iris,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KnobSize {
    Sm,
    Lg,
}

pub fn small(size: f32) -> FontId {
    FontId::proportional(size)
}

/// The calligraphic serif (kana, titles, the seal).
pub fn mincho(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Name("mincho".into()))
}

/// The knob stone: dark ink in both themes, with a paper-coloured pointer.
fn stone(t: &Theme) -> Color32 {
    if t.night {
        Color32::from_rgb(0x34, 0x30, 0x2a)
    } else {
        Color32::from_rgb(0x1e, 0x1b, 0x16)
    }
}
const PAPER: Color32 = Color32::from_rgb(0xf3, 0xf0, 0xe8);

/// Paper-coloured ink for marks drawn on the dark stone and on filled ink.
fn on_ink(t: &Theme) -> Color32 {
    if t.night {
        t.bg
    } else {
        t.surface
    }
}

/// Rotary knob: vertical drag (140 px = full sweep; hold Shift or Ctrl/Cmd for fine
/// adjust) and mouse wheel. The lit arc follows the value smoothly, and the number
/// pops up above the knob while it is held.
pub fn knob(
    ui: &mut Ui,
    t: &Theme,
    value: &mut f64,
    label: &str,
    hint: &str,
    size: KnobSize,
    accent: Accent,
) -> Response {
    knob_d(ui, t, value, None, label, hint, size, accent)
}

/// [`knob`] with a default value: double-click puts the knob back to it.
#[allow(clippy::too_many_arguments)]
pub fn knob_d(
    ui: &mut Ui,
    t: &Theme,
    value: &mut f64,
    default: Option<f64>,
    label: &str,
    hint: &str,
    size: KnobSize,
    accent: Accent,
) -> Response {
    let d = match size {
        KnobSize::Sm => 34.0,
        KnobSize::Lg => 52.0,
    };
    let w = (d + 22.0f32).max(62.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, d + 40.0), Sense::hover());
    let center = Pos2::new(rect.center().x, rect.top() + 6.0 + d / 2.0);
    let hit = Rect::from_center_size(center, Vec2::splat(d + 10.0));
    let id = ui.id().with(("knob", label, rect.min.x as i32, rect.min.y as i32));
    let mut resp = ui.interact(hit, id, Sense::click_and_drag());
    let before = *value;
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift || i.modifiers.command);
        let dy = resp.drag_delta().y as f64 * if fine { FINE_DRAG } else { 1.0 };
        *value = (*value - dy / 140.0).clamp(0.0, 1.0);
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            *value = (*value + scroll as f64 * 0.0024).clamp(0.0, 1.0);
            ui.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
        }
    }
    if resp.double_clicked() {
        if let Some(def) = default {
            *value = def.clamp(0.0, 1.0);
        }
    }
    if *value != before {
        resp.mark_changed();
    }
    let held = resp.dragged() || resp.is_pointer_button_down_on();
    let ctx = ui.ctx().clone();
    // The lit position glides to the value (a held knob follows quickly).
    let lit = ctx.animate_value_with_time(id.with("lit"), *value as f32, motion::KNOB);
    let glow = ctx.animate_bool_with_time(id.with("glow"), held || resp.hovered(), motion::HOVER);
    let bubble = ctx.animate_bool_with_time(id.with("bubble"), held, motion::BUBBLE);
    let p = ui.painter();
    let r = d / 2.0;
    let ac = accent.color(t);
    let shade = Color32::from_black_alpha(if t.night { 90 } else { 38 });
    // ticks, then the dim track and the lit arc just inside them
    for i in 0..11 {
        let a = (-135.0 + i as f32 * 27.0).to_radians();
        let dir = Vec2::new(a.sin(), -a.cos());
        p.line_segment([center + dir * (r + 7.0), center + dir * (r + 9.0)], Stroke::new(1.0_f32, t.subtle));
    }
    let arc = |from: f32, to: f32| -> Vec<Pos2> {
        let n = (((to - from).abs() * 270.0 / 6.0).ceil() as usize).max(2);
        (0..=n)
            .map(|i| {
                let a = (-135.0 + (from + (to - from) * i as f32 / n as f32) * 270.0).to_radians();
                center + Vec2::new(a.sin(), -a.cos()) * (r + 4.0)
            })
            .collect()
    };
    p.add(Shape::line(arc(0.0, 1.0), Stroke::new(2.0_f32, blend(t.border, t.surface, 0.3))));
    if lit > 0.004 {
        p.add(Shape::line(arc(0.0, lit), Stroke::new(2.4_f32 + glow, ac)));
    }
    // body: soft drop shadow, bezel, face
    // a river stone of sumi ink: soft shadow, a faint sheen at the top-left, a paper-coloured pointer
    p.circle_filled(center + Vec2::new(0.0, 2.5), r + 2.0, shade);
    p.circle_filled(center, r, stone(t));
    p.circle_filled(center + Vec2::new(0.0, r * 0.06), r * 0.86, blend(stone(t), Color32::BLACK, 0.25));
    p.circle_filled(
        center + Vec2::new(-r * 0.32, -r * 0.38),
        r * 0.26,
        Color32::from_white_alpha(if t.night { 16 } else { 26 }),
    );
    if glow > 0.01 {
        p.circle_stroke(center, r, Stroke::new(1.5_f32, ac.gamma_multiply(glow)));
    }
    let a = (-135.0 + lit * 270.0).to_radians();
    let dir = Vec2::new(a.sin(), -a.cos());
    p.line_segment([center + dir * (r * 0.3), center + dir * (r - 4.0)], Stroke::new(3.0_f32 + 0.6 * glow, PAPER));
    let ty = center.y + r + 11.0;
    p.text(Pos2::new(rect.center().x, ty), Align2::CENTER_TOP, label, small(11.0), t.fg);
    if !hint.is_empty() {
        p.text(Pos2::new(rect.center().x, ty + 14.0), Align2::CENTER_TOP, hint, small(9.5), t.muted);
    }
    if bubble > 0.01 {
        let text = if hint.is_empty() { format!("{:.0}%", *value * 100.0) } else { hint.to_string() };
        let g = p.layout_no_wrap(text, small(12.0), t.fg.gamma_multiply(bubble));
        let pad = Vec2::new(7.0, 3.0);
        let size = g.size() + pad * 2.0;
        let at = Pos2::new(center.x - size.x / 2.0, center.y - r - 12.0 - size.y - 4.0 * (1.0 - bubble));
        let box_rect = Rect::from_min_size(at, size);
        p.rect_filled(box_rect, 5.0, blend(t.surface, t.border, 0.2).gamma_multiply(bubble));
        p.rect_stroke(box_rect, 5.0, Stroke::new(1.0_f32, ac.gamma_multiply(bubble)), StrokeKind::Inside);
        p.galley(at + pad, g, t.fg);
    }
    resp.on_hover_text(format!(
        "{label}: {:.0}%  (drag ↕ or scroll · Shift = fine · double-click resets)",
        *value * 100.0
    ))
}

/// Vertical fader geometry shared by the lean / note / mix faders.
pub fn fader_value(rect: Rect, y: f32, pad: f32) -> f64 {
    ((rect.bottom() - pad - y) / (rect.height() - pad * 2.0).max(1.0)).clamp(0.0, 1.0) as f64
}

pub fn paint_fader(p: &egui::Painter, t: &Theme, rect: Rect, value: f64, pad: f32, fill: Color32, live: bool) {
    let x = rect.center().x;
    let top = rect.top() + pad;
    let bot = rect.bottom() - pad;
    let slot = Rect::from_min_max(Pos2::new(x - 3.0, top), Pos2::new(x + 3.0, bot));
    p.rect_filled(slot, 3.0, t.surface2);
    p.rect_stroke(slot, 3.0, Stroke::new(1.0_f32, t.border), StrokeKind::Inside);
    let y = bot - (bot - top) * value as f32;
    p.rect_filled(
        Rect::from_min_max(Pos2::new(x - 3.0, y), Pos2::new(x + 3.0, bot)),
        3.0,
        blend(fill, t.surface, 0.35),
    );
    let cap = Rect::from_center_size(Pos2::new(x, y), Vec2::new(22.0, 12.0));
    p.rect_filled(cap, 4.0, if live { fill } else { t.metal });
    p.rect_stroke(cap, 4.0, Stroke::new(1.0_f32, blend(t.subtle, t.border, 0.3)), StrokeKind::Inside);
    p.line_segment([Pos2::new(cap.left() + 4.0, y), Pos2::new(cap.right() - 4.0, y)], Stroke::new(1.0_f32, t.fg));
}

/// Press/release tracking for momentary controls (pads, faders).
/// Returns (pressed_now, released_now, is_down).
pub fn press_edges(resp: &Response, was_down: &mut bool) -> (bool, bool, bool) {
    let down = resp.is_pointer_button_down_on() || resp.dragged();
    let pressed = down && !*was_down;
    let released = !down && *was_down;
    *was_down = down;
    (pressed, released, down)
}

pub struct PadLook<'a> {
    pub kana: &'a str,
    pub name: &'a str,
    pub key: &'a str,
    pub on: bool,
    pub live: bool,
    pub iris: bool,
    pub flash: Option<bool>, // Some(false) = atari, Some(true) = koro
}

pub fn pad(ui: &mut Ui, t: &Theme, size: Vec2, look: &PadLook, id: egui::Id) -> Response {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    let p = ui.painter();
    let accent = if look.iris { t.iris } else { t.moss };
    // outlined squares in ink; a pressed or sounding pad is filled solid (vermilion in the meri lane)
    let ink_on = if look.iris { t.moss } else { t.fg };
    let (fill, fg) = if look.on { (ink_on, if look.iris { t.moss_fg } else { on_ink(t) }) } else { (t.surface, t.fg) };
    let fill = if resp.is_pointer_button_down_on() && !look.on { blend(fill, t.fg, 0.12) } else { fill };
    p.rect_filled(rect, 3.0, fill);
    let mut stroke = if look.iris { Stroke::new(1.2_f32, t.subtle) } else { Stroke::new(1.6_f32, t.fg) };
    if look.live {
        stroke = Stroke::new(2.2_f32, t.moss);
    }
    if let Some(koro) = look.flash {
        stroke = Stroke::new(3.0_f32, if koro { t.danger } else { t.moss });
    }
    p.rect_stroke(rect, 3.0, stroke, StrokeKind::Inside);
    let led_c = if look.on { fg } else { t.border };
    p.circle_filled(Pos2::new(rect.center().x, rect.top() + 8.0), 3.0, led_c);
    p.text(Pos2::new(rect.center().x, rect.top() + 14.0), Align2::CENTER_TOP, look.kana, mincho(23.0), fg);
    p.text(
        Pos2::new(rect.center().x, rect.bottom() - 19.0),
        Align2::CENTER_BOTTOM,
        look.name,
        small(10.5),
        if look.on { fg } else { t.muted },
    );
    p.text(
        Pos2::new(rect.center().x, rect.bottom() - 5.0),
        Align2::CENTER_BOTTOM,
        look.key,
        small(9.5),
        if look.on { fg } else { t.subtle },
    );
    resp
}

/// Small rocker switch with a caption above/below; returns true when clicked.
pub fn rocker(
    ui: &mut Ui,
    t: &Theme,
    on: bool,
    top: &str,
    bottom: &str,
    id: egui::Id,
    bottom_lit_when_on: bool,
) -> bool {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(56.0, 64.0), Sense::hover());
    let body = Rect::from_center_size(rect.center(), Vec2::new(24.0, 30.0));
    let resp = ui.interact(body.expand(4.0), id, Sense::click());
    let p = ui.painter();
    let lit_top = if bottom_lit_when_on { !on } else { on };
    if !top.is_empty() {
        p.text(
            Pos2::new(rect.center().x, rect.top()),
            Align2::CENTER_TOP,
            top,
            small(10.5),
            if lit_top && bottom_lit_when_on {
                t.fg
            } else if lit_top {
                t.moss
            } else {
                t.subtle
            },
        );
    }
    if !bottom.is_empty() {
        p.text(
            Pos2::new(rect.center().x, rect.bottom()),
            Align2::CENTER_BOTTOM,
            bottom,
            small(10.5),
            if on { t.moss } else { t.subtle },
        );
    }
    p.rect_filled(body, 5.0, t.surface2);
    p.rect_stroke(body, 5.0, Stroke::new(1.0_f32, t.border), StrokeKind::Inside);
    let f = ui.ctx().animate_bool_with_time(id.with("lever"), on, motion::SWITCH);
    let mid = body.center().y;
    let lever = Rect::from_min_max(
        Pos2::new(body.left() + 3.0, body.top() + 3.0 + (mid - body.top() - 3.0) * f),
        Pos2::new(body.right() - 3.0, mid + (body.bottom() - 3.0 - mid) * f),
    );
    p.rect_filled(lever, 3.0, blend(t.metal, t.moss, f));
    p.rect_stroke(lever, 3.0, Stroke::new(1.0_f32, t.subtle), StrokeKind::Inside);
    resp.clicked()
}

/// Segmented switch (the web `ModeSwitch` / `delay-divs` rows). Returns the clicked index.
/// The highlight slides to the chosen segment.
pub fn segmented(ui: &mut Ui, t: &Theme, labels: &[&str], selected: usize, width: f32) -> Option<usize> {
    let mut out = None;
    let n = labels.len().max(1) as f32;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 20.0), Sense::hover());
    let w = rect.width() / n;
    let group = ui.id().with((
        "segpill",
        labels.first().copied().unwrap_or(""),
        labels.len(),
        rect.min.x as i32,
        rect.min.y as i32,
    ));
    let pos = ui.ctx().animate_value_with_time(group, selected as f32, motion::SWITCH);
    let p = ui.painter();
    p.rect_filled(rect, 4.0, t.surface2);
    let pill = Rect::from_min_size(Pos2::new(rect.left() + w * pos, rect.top()), Vec2::new(w, rect.height()));
    p.rect_filled(pill.shrink(1.0), 3.0, t.moss);
    for (i, l) in labels.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(rect.left() + w * i as f32, rect.top()), Vec2::new(w, rect.height()));
        let resp = ui.interact(
            r,
            ui.id().with((
                "seg",
                labels.first().copied().unwrap_or(""),
                labels.len(),
                i,
                rect.min.x as i32,
                rect.min.y as i32,
            )),
            Sense::click(),
        );
        let near = (1.0 - (pos - i as f32).abs()).clamp(0.0, 1.0);
        if resp.hovered() && near < 0.5 {
            p.rect_filled(r.shrink(1.0), 3.0, blend(t.surface2, t.moss, 0.18));
        }
        p.text(r.center(), Align2::CENTER_CENTER, *l, small(10.0), blend(t.muted, t.moss_fg, near));
        if resp.clicked() {
            out = Some(i);
        }
    }
    p.rect_stroke(rect, 4.0, Stroke::new(1.0_f32, t.border), StrokeKind::Inside);
    out
}

/// A labelled push/toggle button; returns the response. Hover and toggling fade in and out.
pub fn button(ui: &mut Ui, t: &Theme, label: &str, on: bool, size: Vec2) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let ctx = ui.ctx().clone();
    let hover = ctx.animate_bool_with_time(resp.id.with("hover"), resp.hovered(), motion::HOVER);
    let lit = ctx.animate_bool_with_time(resp.id.with("on"), on, motion::SWITCH);
    let down = if resp.is_pointer_button_down_on() { 0.35 } else { 0.15 * hover };
    let p = ui.painter();
    let fill = blend(blend(t.surface2, t.moss, down), t.moss, lit);
    p.rect_filled(rect, 4.0, fill);
    p.rect_stroke(rect, 4.0, Stroke::new(1.0_f32, t.border), StrokeKind::Inside);
    draw_label(p, rect, label, blend(t.fg, t.moss_fg, lit));
    resp
}

/// True on the frame the pointer goes down on `resp` — for controls that must
/// act the instant they are pressed, not when the button comes back up.
pub fn pressed_now(ui: &Ui, resp: &Response) -> bool {
    let down = resp.is_pointer_button_down_on();
    let key = resp.id.with("pressed-now");
    let was = ui.data(|d| d.get_temp::<bool>(key)).unwrap_or(false);
    ui.data_mut(|d| d.insert_temp(key, down));
    // A press and release inside one frame never shows as "down": fall back to
    // the click so a quick tap still fires, exactly once.
    (down && !was) || (resp.clicked() && !was && !down)
}

/// [`button`], but returns true on press rather than on release.
pub fn button_press(ui: &mut Ui, t: &Theme, label: &str, on: bool, size: Vec2) -> bool {
    let resp = button(ui, t, label, on, size);
    pressed_now(ui, &resp)
}

/// [`disabled_button`], firing on press.
pub fn disabled_button_press(ui: &mut Ui, t: &Theme, label: &str, on: bool, enabled: bool, size: Vec2) -> bool {
    if enabled {
        return button_press(ui, t, label, on, size);
    }
    disabled_button(ui, t, label, on, false, size)
}

/// Text label, or a vector icon for glyphs our fonts lack (↺ undo).
fn draw_label(p: &egui::Painter, rect: Rect, label: &str, color: Color32) {
    if label == "↺" {
        let c = rect.center();
        let r = 5.0;
        let pts: Vec<Pos2> = (0..=20)
            .map(|i| {
                let a = (-60.0f32 + 280.0 * i as f32 / 20.0).to_radians();
                c + Vec2::new(a.cos(), a.sin()) * r
            })
            .collect();
        let tip = pts[0];
        p.add(Shape::line(pts, Stroke::new(1.3_f32, color)));
        p.add(Shape::convex_polygon(
            vec![tip + Vec2::new(-3.5, -1.0), tip + Vec2::new(1.5, -3.5), tip + Vec2::new(1.5, 2.0)],
            color,
            Stroke::NONE,
        ));
        return;
    }
    p.text(rect.center(), Align2::CENTER_CENTER, label, small(10.5), color);
}

pub fn disabled_button(ui: &mut Ui, t: &Theme, label: &str, on: bool, enabled: bool, size: Vec2) -> bool {
    if enabled {
        return button(ui, t, label, on, size).clicked();
    }
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, t.surface2);
    p.rect_stroke(rect, 4.0, Stroke::new(1.0_f32, t.border), StrokeKind::Inside);
    draw_label(p, rect, label, blend(t.subtle, t.surface2, 0.5));
    false
}

pub fn led(ui: &mut Ui, t: &Theme, on: bool, color: Color32) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
    let f = ui.ctx().animate_bool_with_time(resp.id.with("led"), on, motion::HOVER);
    let p = ui.painter();
    if f > 0.01 {
        p.circle_filled(rect.center(), 7.0, color.gamma_multiply(0.3 * f));
    }
    p.circle_filled(rect.center(), 4.5, blend(t.border, color, f));
}

/// A titled module panel of fixed width (the web `Module`), with a faint bamboo grain.
pub fn module<R>(ui: &mut Ui, t: &Theme, title: &str, width: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let backdrop = ui.painter().add(Shape::Noop);
    let out = egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(width);
            ui.label(egui::RichText::new(title.to_uppercase()).size(10.5).color(t.muted).strong());
            ui.add_space(2.0);
            add(ui)
        })
        .inner
    });
    let rect = out.response.rect;
    let mut shapes = vec![Shape::rect_filled(rect, CornerRadius::same(8), t.surface)];
    shapes.extend(grain(rect, t, title));
    shapes.push(Shape::rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0_f32, t.border), StrokeKind::Inside));
    ui.painter().set(backdrop, Shape::Vec(shapes));
    out.inner
}

/// Faint vertical bamboo fibres, the same every frame for a given panel.
pub fn grain(rect: Rect, t: &Theme, seed_text: &str) -> Vec<Shape> {
    let mut seed = seed_text.bytes().fold(0x9e37_79b9u32, |h, b| h.rotate_left(5) ^ b as u32).wrapping_mul(2654435761);
    let mut next = move || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / (1u32 << 24) as f32
    };
    let count = ((rect.width() * rect.height()) / 900.0).clamp(12.0, 90.0) as usize;
    let alpha = if t.night { 16 } else { 14 };
    let ink = Color32::from_rgba_unmultiplied(t.subtle.r(), t.subtle.g(), t.subtle.b(), alpha);
    (0..count)
        .map(|_| {
            let x = rect.left() + 4.0 + next() * (rect.width() - 8.0);
            let len = 10.0 + next() * rect.height() * 0.5;
            let y0 = rect.top() + 3.0 + next() * (rect.height() - 6.0 - len).max(1.0);
            let sway = (next() - 0.5) * 2.0;
            Shape::line_segment(
                [Pos2::new(x, y0), Pos2::new(x + sway, (y0 + len).min(rect.bottom() - 3.0))],
                Stroke::new(0.7_f32, ink),
            )
        })
        .collect()
}

/// Filled area between `top` and `bottom` polylines (egui only fills convex paths).
pub fn fill_between(p: &egui::Painter, top: &[Pos2], bottom: &[Pos2], color: Color32) {
    let n = top.len().min(bottom.len());
    if n < 2 {
        return;
    }
    let mut mesh = Mesh::default();
    for i in 0..n {
        mesh.vertices.push(Vertex { pos: top[i], uv: WHITE_UV, color });
        mesh.vertices.push(Vertex { pos: bottom[i], uv: WHITE_UV, color });
    }
    for i in 0..n as u32 - 1 {
        let a = i * 2;
        mesh.add_triangle(a, a + 1, a + 2);
        mesh.add_triangle(a + 1, a + 3, a + 2);
    }
    p.add(Shape::mesh(mesh));
}

/// Progress ring for the loop punch buttons (starts at 12 o'clock, clockwise).
pub fn ring(p: &egui::Painter, center: Pos2, r: f32, frac: f32, stroke: Stroke, track: Stroke) {
    p.circle_stroke(center, r, track);
    let frac = frac.clamp(0.0, 1.0);
    if frac <= 0.001 {
        return;
    }
    let n = (64.0 * frac).ceil() as usize + 1;
    let pts: Vec<Pos2> = (0..n)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * frac * i as f32 / (n - 1) as f32;
            center + Vec2::new(a.cos(), a.sin()) * r
        })
        .collect();
    p.add(Shape::line(pts, stroke));
}

/// Horizontal pitch-bend strip: drag to bend, release and it springs back to the
/// centre (0.5), like a wheel. Returns true while the value is being moved.
pub fn bend_strip(ui: &mut Ui, t: &Theme, value: &mut f64, width: f32, readout: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 34.0), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width() - 16.0, 8.0));
    let mut moving = false;
    if resp.dragged() || resp.is_pointer_button_down_on() {
        if let Some(p) = resp.interact_pointer_pos() {
            *value = ((p.x - track.left()) / track.width().max(1.0)).clamp(0.0, 1.0) as f64;
            moving = true;
            ui.data_mut(|d| d.insert_temp(resp.id, true));
        }
    } else if ui.data(|d| d.get_temp::<bool>(resp.id)).unwrap_or(false) && (*value - 0.5).abs() > 0.0005 {
        // Only springs back from our own drag, so a MIDI wheel isn't fought.
        // Spring back, quick but not a click.
        let dt = ui.input(|i| i.stable_dt).min(0.05) as f64;
        *value += (0.5 - *value) * (1.0 - (-dt * 18.0).exp());
        if (*value - 0.5).abs() < 0.002 {
            *value = 0.5;
            ui.data_mut(|d| d.insert_temp(resp.id, false));
        }
        ui.ctx().request_repaint();
    }
    let p = ui.painter();
    p.rect_filled(track, 4.0, t.surface2);
    p.rect_stroke(track, 4.0, Stroke::new(1.0_f32, t.border), StrokeKind::Inside);
    let cx = track.center().x;
    p.line_segment(
        [Pos2::new(cx, track.top() - 3.0), Pos2::new(cx, track.bottom() + 3.0)],
        Stroke::new(1.0_f32, t.subtle),
    );
    let x = track.left() + track.width() * *value as f32;
    let (a, b) = if x < cx { (x, cx) } else { (cx, x) };
    p.rect_filled(
        Rect::from_min_max(Pos2::new(a, track.top()), Pos2::new(b, track.bottom())),
        4.0,
        blend(t.iris, t.surface, 0.35),
    );
    let cap = Rect::from_center_size(Pos2::new(x, track.center().y), Vec2::new(14.0, 24.0));
    p.rect_filled(cap, 4.0, if moving { t.iris } else { t.metal });
    p.rect_stroke(cap, 4.0, Stroke::new(1.0_f32, blend(t.subtle, t.border, 0.3)), StrokeKind::Inside);
    p.text(Pos2::new(rect.center().x, rect.bottom() + 1.0), Align2::CENTER_TOP, readout, small(9.5), t.subtle);
    moving
}
