//! The loop pedal (`loop-pedal.tsx`): clock, four track strips with zoomable
//! trim waveforms and punch rings, and the level/FX mixer.

use eframe::egui::{self, Align2, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::app::{App, LoopWaveUi};
use super::theme::{Theme, blend};
use super::widgets::{
    Accent, KnobSize, button_press, disabled_button_press, fader_value, fill_between, knob_d, module, paint_fader,
    pressed_now, ring, small,
};
use iki_core::shaku::looper::{LoopMode, LoopSnap};
use iki_core::shaku::timing::tempo_bpm;

fn mode_cap(m: LoopMode) -> &'static str {
    match m {
        LoopMode::Idle => "Rec",
        LoopMode::Rec => "Play",
        LoopMode::Play => "Dub",
        LoopMode::Dub => "Play",
        LoopMode::Stop => "Play",
    }
}

fn view_window(trim_in: f64, trim_out: f64) -> (f64, f64) {
    let span = (trim_out - trim_in).max(0.05);
    let pad = (span * 0.28).max(0.05);
    let (mut a, mut b) = (trim_in - pad, trim_out + pad);
    if a < 0.0 {
        b = (b - a).min(1.0);
        a = 0.0;
    }
    if b > 1.0 {
        a = (a - (b - 1.0)).max(0.0);
        b = 1.0;
    }
    (a, b)
}

fn sample_wave(wave: &[f32], t: f64) -> f32 {
    let n = wave.len();
    if n < 2 {
        return 0.0;
    }
    let x = t.clamp(0.0, 1.0) * (n - 1) as f64;
    let i = (x.floor() as usize).min(n - 2);
    let f = (x - i as f64) as f32;
    wave[i] * (1.0 - f) + wave[i + 1] * f
}

/// `LoopWave`: returns new trim bounds when the user dragged a handle.
fn loop_wave(ui: &mut Ui, t: &Theme, rect: Rect, snap: &LoopSnap, st: &mut LoopWaveUi) -> Option<(f64, f64)> {
    let empty = snap.mode == LoopMode::Idle && snap.wave.is_empty();
    let resp = ui.interact(rect, ui.id().with(("loop-wave", snap.id)), Sense::click_and_drag());
    let mut out = None;
    let w = rect.width().max(1.0) as f64;
    if !empty {
        if resp.drag_started()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            let u = ((pos.x - rect.left()) as f64 / w).clamp(0.0, 1.0);
            let p = st.view_a + u * (st.view_b - st.view_a);
            st.drag = Some((p - st.trim_in).abs() > (p - st.trim_out).abs());
            st.last_x = pos.x;
        }
        if resp.dragged()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            st.last_x = pos.x;
        }
        if resp.drag_stopped() {
            st.drag = None;
        }
    }
    if let Some(is_out) = st.drag {
        let u = ((st.last_x - rect.left()) as f64 / w).clamp(0.0, 1.0);
        let span = (st.view_b - st.view_a).max(1e-4);
        let edge = 0.14;
        if u < edge {
            let s = ((edge - u) / edge) * span * 0.08;
            st.view_a = (st.view_a - s).max(0.0);
            st.view_b = st.view_a + span;
        } else if u > 1.0 - edge {
            let s = ((u - (1.0 - edge)) / edge) * span * 0.08;
            st.view_b = (st.view_b + s).min(1.0);
            st.view_a = st.view_b - span;
        }
        let p = st.view_a + u * (st.view_b - st.view_a);
        let min = 0.012;
        if is_out {
            st.trim_out = p.max(st.trim_in + min).min(1.0);
        } else {
            st.trim_in = p.min(st.trim_out - min).max(0.0);
        }
        out = Some((st.trim_in, st.trim_out));
    } else {
        st.trim_in = snap.trim_in;
        st.trim_out = snap.trim_out;
        let (ta, tb) = view_window(st.trim_in, st.trim_out);
        st.view_a += (ta - st.view_a) * 0.28;
        st.view_b += (tb - st.view_b) * 0.28;
    }

    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, t.lcd);
    let mid = rect.center().y;
    let h2 = rect.height() / 2.0;
    let (a, b) = (st.view_a, st.view_b);
    let span = (b - a).max(1e-4);
    let to_x = |q: f64| rect.left() + (((q - a) / span) * w) as f32;
    let ink = if snap.id % 2 == 1 { t.iris } else { t.moss };
    if snap.wave.len() < 2 {
        p.line_segment(
            [Pos2::new(rect.left(), mid), Pos2::new(rect.right(), mid)],
            Stroke::new(1.0_f32, t.subtle.gamma_multiply(0.35)),
        );
        if empty {
            p.text(rect.center(), Align2::CENTER_CENTER, "empty", small(10.0), t.subtle);
        }
    } else {
        let steps = (rect.width() as usize).max(48);
        let mut top = Vec::with_capacity(steps + 1);
        let mut bot = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let q = a + (i as f64 / steps as f64) * span;
            let x = rect.left() + (i as f32 / steps as f32) * rect.width();
            let amp = (sample_wave(&snap.wave, q) * 1.6).min(1.0) * (h2 - 3.0);
            top.push(Pos2::new(x, mid - amp));
            bot.push(Pos2::new(x, mid + amp));
        }
        fill_between(&p, &top, &bot, ink.gamma_multiply(0.22));
        p.line(top.clone(), Stroke::new(1.1_f32, ink.gamma_multiply(0.9)));
        p.line(bot, Stroke::new(1.1_f32, ink.gamma_multiply(0.9)));
        let in_x = to_x(st.trim_in);
        let out_x = to_x(st.trim_out);
        let shade = egui::Color32::from_rgba_unmultiplied(42, 34, 20, 40);
        if in_x > rect.left() {
            p.rect_filled(Rect::from_min_max(rect.min, Pos2::new(in_x, rect.bottom())), 0.0, shade);
        }
        if out_x < rect.right() {
            p.rect_filled(Rect::from_min_max(Pos2::new(out_x, rect.top()), rect.max), 0.0, shade);
        }
        for (x, active) in [(in_x, st.drag == Some(false)), (out_x, st.drag == Some(true))] {
            let hr = Rect::from_center_size(Pos2::new(x, mid), Vec2::new(6.0, rect.height() - 4.0));
            p.rect_filled(hr, 2.0, if active { t.iris } else { t.fg.gamma_multiply(0.7) });
        }
    }
    if matches!(snap.mode, LoopMode::Play | LoopMode::Dub | LoopMode::Rec) {
        let x = to_x(snap.progress);
        if x >= rect.left() - 2.0 && x <= rect.right() + 2.0 {
            p.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0_f32, t.fg.gamma_multiply(0.85)),
            );
        }
    }
    out
}

impl App {
    pub fn loop_pedal(&mut self, ui: &mut Ui) {
        let t = self.theme;
        let bank = self.snap.loop_bank.clone();
        module(ui, &t, "Loop", 820.0, |ui| {
            ui.horizontal_top(|ui| {
                // --- clock column ---
                ui.vertical(|ui| {
                    ui.set_width(108.0);
                    let bpm = tempo_bpm(self.fx.delay_tempo);
                    let hint = format!("{bpm} bpm");
                    knob_d(
                        ui,
                        &t,
                        &mut self.fx.delay_tempo,
                        Some(iki_core::engine::Fx::default().delay_tempo),
                        "Clock",
                        &hint,
                        KnobSize::Lg,
                        Accent::Moss,
                    );
                    let bars = (bank.cycle_bars.round() as i64).max(1);
                    ui.label(
                        RichText::new(format!("snap · {bars} bar{}", if bars == 1 { "" } else { "s" }))
                            .size(10.0)
                            .color(t.muted),
                    );
                    let (r, _) = ui.allocate_exact_size(Vec2::new(100.0, 6.0), Sense::hover());
                    ui.painter().rect_filled(r, 3.0, t.surface2);
                    ui.painter().rect_filled(
                        Rect::from_min_size(r.min, Vec2::new(100.0 * bank.bar_phase as f32, 6.0)),
                        3.0,
                        t.moss,
                    );
                    if button_press(ui, &t, "Freeze 4", bank.pad_armed, Vec2::new(100.0, 22.0)) {
                        self.on_freeze();
                    }
                    if button_press(ui, &t, "Stop all", false, Vec2::new(100.0, 22.0)) {
                        self.on_loop_stop_all();
                    }
                    if button_press(ui, &t, "Export WAV", self.export_pending, Vec2::new(100.0, 22.0)) {
                        self.export_pending = true;
                        self.request_loop_export();
                    }
                    if !self.export_msg.is_empty() {
                        ui.label(RichText::new(self.export_msg.clone()).size(9.5).color(t.muted));
                    }
                });
                // --- track strips ---
                ui.vertical(|ui| {
                    ui.set_width(500.0);
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for snap in bank.tracks.iter() {
                        self.loop_strip(ui, &t, snap);
                    }
                });
                // --- mixer ---
                ui.vertical(|ui| {
                    ui.label(RichText::new("MIX").size(10.0).color(t.muted));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for snap in bank.tracks.iter() {
                            self.mix_channel(ui, &t, snap);
                        }
                    });
                });
            });
        });
    }

    fn loop_strip(&mut self, ui: &mut Ui, t: &Theme, snap: &LoopSnap) {
        let i = snap.id;
        let selected = self.loop_track == i;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(500.0, 62.0), Sense::hover());
        if ui.input(|inp| inp.pointer.primary_pressed()) && ui.rect_contains_pointer(rect) {
            self.loop_track = i;
        }
        let p = ui.painter();
        p.rect_filled(rect, 6.0, if selected { blend(t.surface, t.moss, 0.08) } else { t.surface2 });
        if selected {
            p.rect_stroke(rect, 6.0, Stroke::new(1.0_f32, t.moss.gamma_multiply(0.6)), StrokeKind::Inside);
        }
        if snap.pending {
            let blink = (ui.input(|i| i.time) * 4.0).fract() < 0.5;
            p.rect_stroke(
                rect,
                6.0,
                Stroke::new(1.5_f32, if blink { t.iris } else { t.iris.gamma_multiply(0.3) }),
                StrokeKind::Inside,
            );
        }
        if snap.pad {
            p.rect_stroke(rect.shrink(1.0), 6.0, Stroke::new(1.0_f32, t.iris.gamma_multiply(0.45)), StrokeKind::Inside);
        }
        p.text(
            Pos2::new(rect.left() + 10.0, rect.center().y),
            Align2::CENTER_CENTER,
            if snap.pad { "P".to_string() } else { format!("{}", i + 1) },
            small(12.0),
            t.subtle,
        );
        let wave_rect =
            Rect::from_min_max(rect.min + Vec2::new(20.0, 4.0), Pos2::new(rect.right() - 96.0, rect.bottom() - 4.0));
        let mut st = self.loop_ui[i];
        if let Some((a, b)) = loop_wave(ui, t, wave_rect, snap, &mut st) {
            self.set_loop_bounds(i, a, b);
        }
        self.loop_ui[i] = st;

        // punch ring button
        let c = Pos2::new(rect.right() - 64.0, rect.center().y);
        let hit = Rect::from_center_size(c, Vec2::splat(54.0));
        let resp = ui.interact(hit, ui.id().with(("punch", i)), Sense::click());
        let p = ui.painter();
        let col = match snap.mode {
            LoopMode::Rec => t.danger,
            LoopMode::Dub => t.iris,
            _ => t.moss,
        };
        let frac = match snap.mode {
            LoopMode::Idle => 0.0,
            LoopMode::Rec => 0.92,
            _ => snap.progress as f32,
        };
        p.circle_filled(c, 25.0, if resp.hovered() { blend(t.surface, col, 0.12) } else { t.surface });
        ring(p, c, 23.0, frac, Stroke::new(3.0_f32, col), Stroke::new(3.0_f32, t.border));
        if snap.mode == LoopMode::Rec {
            p.circle_filled(c + Vec2::new(0.0, -10.0), 3.0, t.danger);
        }
        p.text(c - Vec2::new(0.0, 3.0), Align2::CENTER_CENTER, mode_cap(snap.mode), small(11.0), t.fg);
        let meta = if snap.mode == LoopMode::Idle { "—".to_string() } else { format!("{:.1}s", snap.length) };
        p.text(c + Vec2::new(0.0, 9.0), Align2::CENTER_CENTER, meta, small(9.0), t.muted);
        if pressed_now(ui, &resp) {
            self.on_punch(i);
        }
        // ▶ ■ ↺
        let x = rect.right() - 30.0;
        let y = rect.top() + 3.0;
        let bs = Vec2::new(26.0, 17.0);
        let playing = matches!(snap.mode, LoopMode::Play | LoopMode::Dub);
        let can_play = !(snap.mode == LoopMode::Idle && snap.wave.is_empty());
        let mut child =
            ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(Pos2::new(x, y), Vec2::new(26.0, 60.0))));
        child.spacing_mut().item_spacing.y = 2.0;
        if disabled_button_press(&mut child, t, "▶", playing, can_play, bs) {
            self.on_loop_play(i);
        }
        if button_press(&mut child, t, "■", false, bs) {
            self.on_loop_stop(i);
        }
        let can_undo = snap.can_undo || snap.mode != LoopMode::Idle;
        if disabled_button_press(&mut child, t, "↺", false, can_undo, bs) {
            self.on_loop_undo(i);
        }
    }

    fn mix_channel(&mut self, ui: &mut Ui, t: &Theme, snap: &LoopSnap) {
        let i = snap.id;
        ui.vertical(|ui| {
            ui.set_width(34.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            let live = matches!(snap.mode, LoopMode::Play | LoopMode::Dub | LoopMode::Rec);
            ui.label(
                RichText::new(if snap.pad { "P".to_string() } else { format!("{}", i + 1) })
                    .size(10.5)
                    .color(if live { t.fg } else { t.subtle }),
            );
            let (r, resp) = ui.allocate_exact_size(Vec2::new(34.0, 180.0), Sense::click_and_drag());
            if (resp.dragged() || resp.is_pointer_button_down_on())
                && let Some(pos) = resp.interact_pointer_pos()
            {
                let v = fader_value(r, pos.y, 10.0);
                if v != self.loop_levels[i] {
                    self.set_loop_level(i, v);
                }
            }
            let ink = if i % 2 == 1 { t.iris } else { t.moss };
            paint_fader(ui.painter(), t, r, self.loop_levels[i], 10.0, ink, live);
            let v = self.loop_levels[i];
            ui.label(
                RichText::new(if v <= 0.004 { "−∞".to_string() } else { format!("{}", (v * 100.0).round()) })
                    .size(10.0)
                    .color(t.muted),
            );
            if button_press(ui, t, "FX", snap.fx, Vec2::new(34.0, 18.0)) {
                self.set_loop_fx(i, !snap.fx);
            }
        });
    }
}
