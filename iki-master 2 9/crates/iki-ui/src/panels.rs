//! The larger painted pieces of the front panel — CRT, flute, blow / muraiki
//! buttons, LFO wave picker, engine bars, Foli steps, keyboard help — drawn
//! from plain data so every frontend shows exactly the same instrument.

use egui::{self, Align2, Color32, Pos2, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::scopes::Spectrogram;
use crate::theme::{blend, Theme};
use crate::widgets::{mincho, small};
use iki_core::dsp::Wave;
use iki_core::shaku::design::motion;
use iki_core::shaku::fingering::Holes;

/// Everything the CRT shows besides the spectrogram.
pub struct CrtData<'a> {
    pub kana: &'a str,
    pub name: &'a str,
    /// "293 Hz · Ro otsu env"
    pub sub: &'a str,
    pub blowing: bool,
    pub breath: f32,
    pub peak_l: f32,
    pub peak_r: f32,
    pub cutoff_hz: f32,
    pub comp_gr: f32,
    /// Bottom caption (keyboard hints).
    pub footer: &'a str,
}

/// The CRT: kana, note name, pitch readout, level meters and the stereo spectrogram.
pub fn crt(ui: &mut Ui, t: &Theme, d: &CrtData, spectro: &mut Spectrogram, size: Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 8.0, t.lcd);
    p.rect_stroke(rect, 8.0, Stroke::new(1.0_f32, t.border), StrokeKind::Inside);
    let kana_c = if d.blowing { t.moss } else { t.iris };
    let kr = p.text(rect.min + Vec2::new(16.0, 8.0), Align2::LEFT_TOP, d.kana, small(46.0), kana_c);
    let x = kr.right() + 14.0;
    p.text(Pos2::new(x, rect.top() + 18.0), Align2::LEFT_TOP, d.name, small(19.0), t.fg);
    p.text(Pos2::new(x, rect.top() + 44.0), Align2::LEFT_TOP, d.sub, small(11.5), t.muted);
    // Breath / level readouts on the right of the head.
    let meter = |p: &egui::Painter, y: f32, label: &str, v: f32, c: Color32| {
        let r = Rect::from_min_size(Pos2::new(rect.right() - 118.0, y), Vec2::new(84.0, 6.0));
        p.text(Pos2::new(r.left() - 6.0, r.center().y), Align2::RIGHT_CENTER, label, small(9.0), t.subtle);
        p.rect_filled(r, 2.0, t.surface2);
        p.rect_filled(Rect::from_min_size(r.min, Vec2::new(84.0 * v.clamp(0.0, 1.0), 6.0)), 2.0, c);
    };
    meter(p, rect.top() + 16.0, "breath", d.breath, t.moss);
    meter(p, rect.top() + 28.0, "out L", d.peak_l, t.moss);
    meter(p, rect.top() + 40.0, "out R", d.peak_r, t.iris);
    p.text(
        Pos2::new(rect.right() - 12.0, rect.top() + 56.0),
        Align2::RIGHT_TOP,
        format!("cut {:.0} Hz · gr {:.1} dB", d.cutoff_hz, d.comp_gr),
        small(9.0),
        t.subtle,
    );
    let spec = Rect::from_min_max(rect.min + Vec2::new(12.0, 76.0), rect.max - Vec2::new(12.0, 26.0));
    spectro.show(ui, t, spec);
    ui.painter().text(
        Pos2::new(rect.left() + 12.0, rect.bottom() - 8.0),
        Align2::LEFT_BOTTOM,
        d.footer,
        small(9.5),
        t.subtle,
    );
}

/// Front holes (index, height on the body) bottom to top; the thumb hole is at 0.08.
const FRONT: [(usize, f32); 4] = [(3, 0.31), (2, 0.45), (1, 0.59), (0, 0.73)];

/// What the living flute shows, from plain data.
pub struct FluteLive<'a> {
    pub holes: &'a Holes,
    pub blowing: bool,
    /// 0…1 breath level (the engine's breath meter).
    pub breath: f32,
    /// Pitch bend in semitones (± range).
    pub bend_st: f32,
    /// Meri depth 0…1.
    pub meri: f32,
}

/// The bamboo body with its holes; breath wisps above it while blowing.
pub fn flute(p: &egui::Painter, t: &Theme, body: Rect, holes: &Holes, blowing: bool, time: f64) {
    let covers = holes.map(|c| if c { 1.0 } else { 0.0 });
    draw_flute(p, t, body, covers, if blowing { 1.0 } else { 0.0 }, 0.0, 0.0, time);
}

/// The flute, alive: holes close and open in 50 ms, the glow and wisps follow the
/// breath, wisps lean with the bend, and the blowing end darkens with meri.
pub fn flute_live(ctx: &egui::Context, p: &egui::Painter, t: &Theme, body: Rect, d: &FluteLive, time: f64) {
    let base = egui::Id::new(("flute-live", body.min.x as i32, body.min.y as i32));
    let mut covers = [0f32; 5];
    for (i, c) in covers.iter_mut().enumerate() {
        *c = ctx.animate_bool_with_time(base.with(i), d.holes[i], motion::HOLE);
    }
    let blow = ctx.animate_bool_with_time(base.with("blow"), d.blowing, motion::SWITCH);
    let breath = ctx.animate_value_with_time(base.with("breath"), d.breath.clamp(0.0, 1.0), motion::KNOB);
    let bend = ctx.animate_value_with_time(base.with("bend"), d.bend_st, motion::KNOB);
    draw_flute(p, t, body, covers, blow, breath * blow, bend, time);
    if d.meri > 0.01 {
        // the chin covers more of the blowing edge as meri deepens
        let h = body.height() * 0.07 * d.meri.clamp(0.0, 1.0);
        p.rect_filled(
            Rect::from_min_size(body.min + Vec2::new(4.0, 0.0), Vec2::new(body.width() - 8.0, h)),
            6.0,
            t.hole.gamma_multiply(0.55),
        );
    }
    if d.blowing || blow > 0.01 {
        ctx.request_repaint();
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_flute(
    p: &egui::Painter,
    t: &Theme,
    body: Rect,
    covers: [f32; 5],
    blow: f32,
    breath: f32,
    bend: f32,
    time: f64,
) {
    let n = 12;
    let dark = if t.night { Color32::from_rgb(0x4a, 0x36, 0x10) } else { Color32::from_rgb(0x8a, 0x6a, 0x18) };
    let lit = if t.night { Color32::from_rgb(0xb0, 0x8c, 0x3a) } else { Color32::from_rgb(0xf0, 0xc8, 0x5c) };
    let edge = if t.night { Color32::from_rgb(0x30, 0x22, 0x08) } else { Color32::from_rgb(0x5a, 0x40, 0x10) };
    if blow > 0.01 {
        let glow = 0.18 + 0.3 * breath.clamp(0.0, 1.0);
        p.rect_filled(body.expand(4.0 + 5.0 * breath), 24.0, t.moss.gamma_multiply(glow * blow));
    }
    p.rect_filled(body, 20.0, dark);
    for i in 0..n {
        let u = i as f32 / (n - 1) as f32;
        let c = if u < 0.42 {
            blend(dark, lit, u / 0.42)
        } else if u < 0.58 {
            blend(lit, blend(lit, dark, 0.2), (u - 0.42) / 0.16)
        } else {
            blend(blend(lit, dark, 0.2), edge, (u - 0.58) / 0.42)
        };
        let x0 = body.left() + 6.0 + (body.width() - 12.0) * i as f32 / n as f32;
        let x1 = body.left() + 6.0 + (body.width() - 12.0) * (i + 1) as f32 / n as f32;
        p.rect_filled(Rect::from_min_max(Pos2::new(x0, body.top() + 3.0), Pos2::new(x1, body.bottom() - 3.0)), 0.0, c);
    }
    p.rect_stroke(
        body,
        20.0,
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(20, 16, 12, 90)),
        StrokeKind::Inside,
    );
    for y in [0.18, 0.40, 0.62, 0.84] {
        let cy = body.top() + body.height() * y;
        p.rect_filled(
            Rect::from_center_size(Pos2::new(body.center().x, cy), Vec2::new(body.width() + 6.0, 7.0)),
            4.0,
            Color32::from_rgba_unmultiplied(40, 32, 24, 115),
        );
    }
    let hole = |p: &egui::Painter, y: f32, cover: f32, r: f32| {
        let c = Pos2::new(body.center().x, body.top() + body.height() * y);
        p.circle_filled(c, r + 1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 60));
        // an open hole glows with the breath; a closing finger darkens it
        p.circle_filled(c, r, blend(t.moss, t.hole, cover));
        if cover < 0.99 {
            p.circle_stroke(
                c,
                r - 3.0,
                Stroke::new(1.0_f32, blend(t.moss, Color32::WHITE, 0.4).gamma_multiply(1.0 - cover)),
            );
        }
    };
    hole(p, 0.08, covers[4], 13.0);
    for (i, y) in FRONT {
        hole(p, y, covers[i], 15.0);
    }
    if blow > 0.01 {
        let tt = time as f32;
        // wisps lean with the bend: sharp to one side, flat to the other
        let lean = (bend / 4.0).clamp(-1.0, 1.0) * 12.0;
        let speed = 1.6 - 0.7 * breath.clamp(0.0, 1.0);
        for (k, dx) in [(0, 0.0f32), (1, -8.0), (2, 7.0)] {
            let ph = ((tt / speed) + k as f32 * 0.125) % 1.0;
            let y = body.top() - 4.0 - ph * (14.0 + 14.0 * breath);
            let a = (1.0 - ph) * (0.3 + 0.4 * breath) * blow;
            p.rect_filled(
                Rect::from_center_size(
                    Pos2::new(body.center().x + dx + lean * ph, y),
                    Vec2::new(5.0, 12.0 + 10.0 * breath),
                ),
                3.0,
                t.iris.gamma_multiply(a),
            );
        }
    }
}

/// The Blow / Gate disc; a click toggles the latched drone.
pub fn blow_button(ui: &mut Ui, t: &Theme, on: bool) -> Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::new(118.0, 70.0), Sense::click());
    let p = ui.painter();
    let fill = if on { t.moss } else { t.fg };
    let fill = if resp.hovered() && !on { blend(fill, t.subtle, 0.2) } else { fill };
    p.rect_filled(r, 3.0, fill);
    let ink = if on {
        t.moss_fg
    } else if t.night {
        t.bg
    } else {
        t.surface
    };
    let label = if on { "吹 GATE" } else { "吹 BLOW" };
    p.text(r.center(), Align2::CENTER_CENTER, label, mincho(18.0), ink);
    resp
}

/// 風 Muraiki; the caller fires it on press (see `widgets::press_edges`).
pub fn muraiki_button(ui: &mut Ui, t: &Theme, lit: bool) -> Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::new(70.0, 70.0), Sense::click_and_drag());
    let p = ui.painter();
    if lit {
        p.rect_filled(r.expand(5.0), 8.0, t.iris.gamma_multiply(0.3));
    }
    p.rect_filled(r, 4.0, if lit { t.iris } else { t.surface });
    p.rect_stroke(r, 4.0, Stroke::new(1.0_f32, t.border), StrokeKind::Inside);
    let ink = if lit { t.iris_fg } else { t.fg };
    p.text(r.center() - Vec2::new(0.0, 8.0), Align2::CENTER_CENTER, "風", small(24.0), ink);
    p.text(Pos2::new(r.center().x, r.bottom() - 8.0), Align2::CENTER_BOTTOM, "Muraiki", small(10.0), ink);
    resp
}

/// The four LFO wave buttons with little icons; returns the clicked wave.
pub fn wave_picker(ui: &mut Ui, t: &Theme, current: Wave) -> Option<Wave> {
    let mut out = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (w, label) in Wave::ALL {
            let on = current == w;
            let (r, resp) = ui.allocate_exact_size(Vec2::new(44.0, 38.0), Sense::click());
            let p = ui.painter();
            p.rect_filled(r, 4.0, if on { t.moss } else { t.surface2 });
            p.rect_stroke(r, 4.0, Stroke::new(1.0_f32, t.border), StrokeKind::Inside);
            let ink = if on { t.moss_fg } else { t.fg };
            let icon = Rect::from_min_size(r.min + Vec2::new(8.0, 4.0), Vec2::new(28.0, 16.0));
            let pts: Vec<Pos2> = (0..=28)
                .map(|i| {
                    let u = i as f32 / 28.0;
                    let y = match w {
                        Wave::Sine => (u * std::f32::consts::TAU).sin(),
                        Wave::Triangle => {
                            if u < 0.5 {
                                4.0 * u - 1.0
                            } else {
                                3.0 - 4.0 * u
                            }
                        }
                        Wave::Sawtooth => 2.0 * u - 1.0,
                        Wave::Square => {
                            if u < 0.5 {
                                1.0
                            } else {
                                -1.0
                            }
                        }
                    };
                    Pos2::new(icon.left() + u * icon.width(), icon.center().y - y * icon.height() / 2.0)
                })
                .collect();
            p.line(pts, Stroke::new(1.4_f32, ink));
            p.text(Pos2::new(r.center().x, r.bottom() - 3.0), Align2::CENTER_BOTTOM, label, small(9.5), ink);
            if resp.clicked() {
                out = Some(w);
            }
        }
    });
    out
}

/// Engine-module bar: "label ▮▮▮▯▯ text".
pub fn meter_bar(ui: &mut Ui, t: &Theme, label: &str, v: f32, c: Color32, text: String) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(10.0).color(t.subtle));
        let (r, _) = ui.allocate_exact_size(Vec2::new(120.0, 8.0), Sense::hover());
        ui.painter().rect_filled(r, 3.0, t.surface2);
        ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(120.0 * v.clamp(0.0, 1.0), 8.0)), 3.0, c);
        ui.label(RichText::new(text).size(10.0).color(t.muted));
    });
}

/// The Foli step grid (groups of three); returns the clicked step.
pub fn foli_steps(ui: &mut Ui, t: &Theme, mask: &[bool; 12], current: Option<usize>) -> Option<usize> {
    let mut out = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (i, &on) in mask.iter().enumerate() {
            if i % 3 == 0 && i > 0 {
                ui.add_space(3.0);
            }
            let (r, resp) = ui.allocate_exact_size(Vec2::new(13.0, 26.0), Sense::click());
            let p = ui.painter();
            let now = current == Some(i);
            let fill = if on {
                if i % 3 == 0 {
                    t.moss
                } else {
                    t.iris
                }
            } else {
                t.surface2
            };
            p.rect_filled(r, 3.0, fill);
            p.rect_stroke(
                r,
                3.0,
                Stroke::new(if now { 2.0_f32 } else { 1.0_f32 }, if now { t.fg } else { t.border }),
                StrokeKind::Inside,
            );
            if resp.clicked() {
                out = Some(i);
            }
        }
    });
    out
}

/// The keyboard shortcuts every frontend shares; frontends append their own rows.
pub const KEY_HELP: [(&str, &str); 8] = [
    ("Space", "toggle blow mode (latched drone)"),
    ("A S D F G H  ;  Z X C V B N", "hold to blow; tap while blowing = atari, tap twice = koro"),
    ("↑ → / ↓ ←", "next / previous note in the current lane"),
    ("O", "otsu ↔ kan (octave)"),
    ("M", "muraiki (breath burst)"),
    ("1 – 4", "select loop track"),
    ("R / T / U", "punch (rec → play → dub) · stop · undo on the selected track"),
    ("mouse", "drag knobs vertically or scroll; drop a .mid file on the window to play it"),
];

/// A framed two-column help card.
pub fn help_card(ui: &mut Ui, t: &Theme, rows: &[(&str, &str)]) {
    egui::Frame::new().fill(t.surface).stroke(Stroke::new(1.0_f32, t.border)).corner_radius(8).inner_margin(10).show(
        ui,
        |ui| {
            ui.set_width(ui.available_width());
            egui::Grid::new("help").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                for (k, v) in rows {
                    ui.label(RichText::new(*k).strong().color(t.moss));
                    ui.label(RichText::new(*v).color(t.fg));
                    ui.end_row();
                }
            });
        },
    );
}
