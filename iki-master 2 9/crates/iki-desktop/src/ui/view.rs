//! Drawing: header, play strip (CRT · flute · blow column), note pads, the
//! Shape rack (envelopes, filter, LFO) and the Space rack (delay, space,
//! colour, looper, MIDI). Mirrors the web layout top to bottom.

use eframe::egui::{self, Align2, Color32, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use super::app::App;
use super::scopes::{env_scope, filt_scope};
use super::theme::Theme;
use super::widgets::{
    Accent, KnobSize, PadLook, bend_strip, button, fader_value, knob, knob_d, led, module, pad, paint_fader,
    press_edges, rocker, segmented, small,
};
use iki_core::engine::{FiltAuto, VoiceMode};
use iki_core::shaku::env_shape::{ENV_SHAPES, FILT_TYPES, env_seconds, filt_cutoff};
use iki_core::shaku::fingering::{FINGERINGS, NOTE_MAX, Voicing};
use iki_core::shaku::timing::{DELAY_DIVS, format_hz, log_hz, tempo_bpm};
use iki_ui::panels;

const KARI_KEYS: [&str; 6] = ["A", "S", "D", "F", "G", "H"];
const MERI_KEYS: [&str; 6] = ["Z", "X", "C", "V", "B", "N"];

fn strip_title(ui: &mut Ui, t: &Theme, s: &str) {
    ui.label(RichText::new(s.to_uppercase()).size(12.0).color(t.muted).strong());
}

impl App {
    pub fn view(&mut self, ui: &mut Ui) {
        let t = self.theme;
        egui::Frame::new().fill(t.bg).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
                self.header(ui);
                self.play_strip(ui);
                self.keys_strip(ui);
                ui.add_space(4.0);
                strip_title(ui, &t, "Shape");
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(10.0, 10.0);
                    self.env_rack(ui);
                    self.filter_rack(ui);
                    self.lfo_rack(ui);
                });
                ui.add_space(4.0);
                strip_title(ui, &t, &format!("Space · {} bpm", tempo_bpm(self.fx.delay_tempo)));
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(10.0, 10.0);
                    self.fx_rack(ui);
                    self.output_module(ui);
                    self.foli_module(ui);
                });
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(10.0, 10.0);
                    self.loop_pedal(ui);
                    self.midi_dock(ui);
                });
                self.footer(ui);
            });
        });
    }

    fn header(&mut self, ui: &mut Ui) {
        let t = self.theme;
        ui.horizontal(|ui| {
            ui.label(RichText::new("息").size(34.0).color(t.moss));
            ui.vertical(|ui| {
                ui.label(RichText::new("Iki").size(24.0).color(t.fg));
                ui.label(RichText::new("1.8 shakuhachi").size(11.0).color(t.muted));
            });
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("VOICE").size(9.5).color(t.subtle));
                let sel = self.voice_mode.index();
                if let Some(i) = segmented(ui, &t, &["Flute", "Bore", "Both"], sel, 190.0) {
                    self.set_voice_mode(VoiceMode::from_index(i));
                }
            });
            ui.add_space(4.0);
            let fh = format!("{}", (self.fx.flute_level * 100.0).round());
            knob_d(ui, &t, &mut self.fx.flute_level, Some(iki_core::engine::Fx::default().flute_level), "Flute", &fh, KnobSize::Sm, Accent::Moss);
            let bh = format!("{}", (self.fx.bore_level * 100.0).round());
            knob_d(ui, &t, &mut self.fx.bore_level, Some(iki_core::engine::Fx::default().bore_level), "Bore", &bh, KnobSize::Sm, Accent::Iris);
            let th = format!("A {:.1}", 440.0 * ((self.fx.tune - 0.5) * 2.0 / 12.0).exp2());
            knob_d(ui, &t, &mut self.fx.tune, Some(iki_core::engine::Fx::default().tune), "Tune", &th, KnobSize::Sm, Accent::Moss);
            ui.add_space(12.0);
            let problem = self.audio_problem();
            let (status_rect, status_resp) =
                ui.allocate_exact_size(Vec2::new((ui.available_width() - 190.0).max(120.0), 40.0), Sense::hover());
            let p = ui.painter();
            let mid = status_rect.center().y;
            p.line_segment(
                [Pos2::new(status_rect.left(), mid + 14.0), Pos2::new(status_rect.right(), mid + 14.0)],
                Stroke::new(1.0_f32, t.border),
            );
            p.text(
                Pos2::new(status_rect.right(), mid - 6.0),
                Align2::RIGHT_CENTER,
                self.status_readout(1000.0 / self.frame_ms.max(1.0)),
                small(10.5),
                if problem.is_some() { t.danger } else { t.muted },
            );
            let skipped = self.audio.status.lock().map(|s| s.skipped.clone()).unwrap_or_default();
            status_resp.on_hover_text(format!(
                "Audio output: {}{}\n\nChoose another with --host pulseaudio|alsa and --device <name>; list them with --list-devices.",
                self.audio.backend,
                if skipped.is_empty() { String::new() } else { format!("\nSkipped: {}", skipped.join("\n  ")) }
            ));
            if button(ui, &t, if self.night { "Temple" } else { "Breath" }, self.night, Vec2::new(70.0, 24.0)).clicked()
            {
                self.set_night(!self.night);
            }
            if button(ui, &t, "Info", self.show_help, Vec2::new(46.0, 24.0)).clicked() {
                self.show_help = !self.show_help;
            }
            led(ui, &t, self.latched, t.moss);
        });
        if let Some(msg) = self.audio_problem() {
            ui.label(RichText::new(format!("⚠ {msg}")).size(11.5).color(t.danger).strong());
        }
        if self.show_help {
            self.help_card(ui);
        }
    }

    fn help_card(&mut self, ui: &mut Ui) {
        let t = self.theme;
        panels::help_card(ui, &t, &panels::KEY_HELP);
    }

    // ------------------------------------------------------------ play strip

    fn play_strip(&mut self, ui: &mut Ui) {
        ui.horizontal_top(|ui| {
            self.crt(ui);
            self.stage(ui);
            self.play_col(ui);
        });
    }

    fn crt(&mut self, ui: &mut Ui) {
        let t = self.theme;
        let named = self.note_name();
        let sub = self.pitch_readout();
        let d = panels::CrtData {
            kana: &named.kana,
            name: &named.name,
            sub: &sub,
            blowing: self.blowing,
            breath: self.snap.breath,
            peak_l: self.snap.peak_l,
            peak_r: self.snap.peak_r,
            cutoff_hz: self.snap.cutoff_hz,
            comp_gr: self.snap.comp_gr,
            footer: "A S D F G H · Z X C V B N · SPACE BLOW · O OCTAVE · M MURAIKI",
        };
        panels::crt(ui, &t, &d, &mut self.spectro, Vec2::new(470.0, 300.0));
    }

    fn stage(&mut self, ui: &mut Ui) {
        let t = self.theme;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(250.0, 300.0), Sense::hover());
        let h = rect.height() - 16.0;
        let top = rect.top() + 8.0;
        // --- lean fader (left, meri) ---
        let lean_rect = Rect::from_min_size(Pos2::new(rect.left() + 18.0, top), Vec2::new(40.0, h));
        let lr = ui.interact(lean_rect, ui.id().with("lean-fader"), Sense::click_and_drag());
        let lpad = (h * 0.08).min(28.0);
        let (_, _, down) = press_edges(&lr, &mut self.lean_fader_down);
        if down && let Some(pos) = lr.interact_pointer_pos() {
            self.lean = fader_value(lean_rect, pos.y, lpad);
        }
        let p = ui.painter();
        paint_fader(p, &t, lean_rect, self.lean, lpad, t.iris, self.lean > 0.0);
        p.text(
            Pos2::new(lean_rect.left() - 2.0, lean_rect.bottom() - lpad - (h - 2.0 * lpad) * 0.92),
            Align2::RIGHT_CENTER,
            "メリ",
            small(10.0),
            t.subtle,
        );
        p.text(
            Pos2::new(lean_rect.left() - 2.0, lean_rect.bottom() - lpad - (h - 2.0 * lpad) * 0.08),
            Align2::RIGHT_CENTER,
            "カリ",
            small(10.0),
            t.subtle,
        );

        // --- flute body ---
        let body = Rect::from_center_size(Pos2::new(rect.left() + 118.0, rect.center().y), Vec2::new(40.0, h));
        let time = ui.input(|i| i.time);
        let bend_st = ((self.fx.bend - 0.5) * 2.0 * (1.0 + 11.0 * self.fx.bend_range)) as f32;
        let live = panels::FluteLive {
            holes: &self.holes(),
            blowing: self.blowing,
            breath: self.snap.breath,
            bend_st,
            meri: self.meri as f32,
        };
        panels::flute_live(ui.ctx(), p, &t, body, &live, time);

        // --- note fader (right, kari) ---
        let nf = Rect::from_min_size(Pos2::new(rect.left() + 168.0, top), Vec2::new(36.0, h));
        let npad = (h * 0.08).min(28.0);
        let nr = ui.interact(nf, ui.id().with("note-fader"), Sense::click_and_drag());
        let (pressed, released, down) = press_edges(&nr, &mut self.note_fader_down);
        let to_note = |y: f32| ((fader_value(nf, y, npad) * NOTE_MAX as f64).round() as usize).min(NOTE_MAX);
        if let Some(pos) = nr.interact_pointer_pos() {
            if pressed {
                self.on_slide_start(to_note(pos.y), Voicing::Kari);
            } else if down {
                self.park_note(to_note(pos.y), Voicing::Kari);
            }
        }
        if released {
            self.on_slide_end();
        }
        let active = self.voicing == Voicing::Kari;
        let p = ui.painter();
        paint_fader(p, &t, nf, self.kari_note as f64 / NOTE_MAX as f64, npad, t.moss, active);
        for (i, f) in FINGERINGS.iter().enumerate() {
            let y = nf.bottom() - npad - (nf.height() - 2.0 * npad) * i as f32 / NOTE_MAX as f32;
            let c = if active && i == self.kari_note { t.fg } else { t.subtle };
            p.text(Pos2::new(nf.right() + 6.0, y), Align2::LEFT_CENTER, f.kana, small(12.0), c);
        }
    }

    fn play_col(&mut self, ui: &mut Ui) {
        let t = self.theme;
        ui.vertical(|ui| {
            ui.set_width(330.0);
            ui.horizontal(|ui| {
                // Blow / Gate
                if panels::blow_button(ui, &t, self.latched).clicked() {
                    self.toggle_gate();
                }
                ui.add_space(6.0);
                // Muraiki (fires on press)
                let resp = panels::muraiki_button(ui, &t, self.muraiki_flash.is_some());
                let (pressed, _, _) = press_edges(&resp, &mut self.blow_down);
                if pressed {
                    self.shoot_muraiki();
                }
                ui.add_space(6.0);
                // Octave rocker: lever down = otsu
                if rocker(ui, &t, self.otsu, "Kan", "Otsu", ui.id().with("octave"), true) {
                    self.otsu = !self.otsu;
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                knob_d(ui, &t, &mut self.meri, Some(0.14), "Meri", "Flatten", KnobSize::Lg, Accent::Moss);
                knob_d(ui, &t, &mut self.iki, Some(0.52), "Iki", "Breath", KnobSize::Lg, Accent::Iris);
                let hint = format!("{}", (self.fx.gain * 100.0).round());
                knob_d(
                    ui,
                    &t,
                    &mut self.fx.gain,
                    Some(iki_core::engine::Fx::default().gain),
                    "Gain",
                    &hint,
                    KnobSize::Lg,
                    Accent::Moss,
                );
            });
            // Expression: pitch bend (springs back), yuri vibrato, glide.
            let bend_hint = format!("bend {:+.1} st", (self.fx.bend - 0.5) * 2.0 * (1.0 + 11.0 * self.fx.bend_range));
            bend_strip(ui, &t, &mut self.fx.bend, 322.0, &bend_hint);
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let h = format!("±{:.0} st", 1.0 + 11.0 * self.fx.bend_range);
                knob_d(
                    ui,
                    &t,
                    &mut self.fx.bend_range,
                    Some(iki_core::engine::Fx::default().bend_range),
                    "Range",
                    &h,
                    KnobSize::Sm,
                    Accent::Iris,
                );
                let h = format!("{}", (self.fx.yuri * 100.0).round());
                knob_d(
                    ui,
                    &t,
                    &mut self.fx.yuri,
                    Some(iki_core::engine::Fx::default().yuri),
                    "Yuri",
                    &h,
                    KnobSize::Sm,
                    Accent::Moss,
                );
                let h = format_hz(3.0 + 5.0 * self.fx.yuri_rate);
                knob_d(
                    ui,
                    &t,
                    &mut self.fx.yuri_rate,
                    Some(iki_core::engine::Fx::default().yuri_rate),
                    "Rate",
                    &h,
                    KnobSize::Sm,
                    Accent::Moss,
                );
                let h = format!("{} ms", ((0.012 + 0.248 * self.fx.glide) * 1000.0).round());
                knob_d(
                    ui,
                    &t,
                    &mut self.fx.glide,
                    Some(iki_core::engine::Fx::default().glide),
                    "Glide",
                    &h,
                    KnobSize::Sm,
                    Accent::Iris,
                );
            });
            // Indicator block: holes + state flags.
            ui.label(RichText::new(self.holes_readout()).size(10.5).color(t.muted));
            ui.label(RichText::new(self.play_readout()).size(10.5).color(t.muted));
        });
    }

    // ------------------------------------------------------------ note pads

    fn keys_strip(&mut self, ui: &mut Ui) {
        let t = self.theme;
        for lane in [Voicing::Kari, Voicing::Meri] {
            let meri = lane == Voicing::Meri;
            ui.label(
                RichText::new(if meri {
                    "MERI · Z X C V B N · left fader leans"
                } else {
                    "KARI · A S D F G H · tap: atari · again: koro · M: muraiki"
                })
                .size(10.5)
                .color(t.muted),
            );
            let w = ((ui.available_width() - 5.0 * 8.0) / 6.0).clamp(90.0, 230.0);
            ui.horizontal(|ui| {
                let note = if meri { self.meri_note } else { self.kari_note };
                for (i, f) in FINGERINGS.iter().enumerate() {
                    let kana = if meri { format!("{}メ", f.kana) } else { f.kana.to_string() };
                    let name = if meri { format!("{} meri", f.name) } else { f.name.to_string() };
                    let flash = self.ornament.filter(|o| o.lane == lane && o.index == i).map(|o| o.koro);
                    let look = PadLook {
                        kana: &kana,
                        name: &name,
                        key: if meri { MERI_KEYS[i] } else { KARI_KEYS[i] },
                        on: i == note,
                        live: self.voicing == lane && i == note,
                        iris: meri || i % 2 == 1,
                        flash,
                    };
                    let resp = pad(ui, &t, Vec2::new(w, 66.0), &look, ui.id().with(("pad", meri, i)));
                    let (pressed, released, _) = press_edges(&resp, &mut self.pad_down[meri as usize][i]);
                    if pressed {
                        self.on_hit_start(i, lane);
                    }
                    if released {
                        self.on_slide_end();
                    }
                }
            });
        }
    }

    // ------------------------------------------------------------ shape rack

    fn env_rack(&mut self, ui: &mut Ui) {
        let t = self.theme;
        module(ui, &t, "Env", 560.0, |ui| {
            ui.horizontal_top(|ui| {
                for amp in [true, false] {
                    ui.vertical(|ui| {
                        ui.set_width(272.0);
                        let fx = &mut self.fx;
                        let (a, d, s, r, shape, lp) = if amp {
                            (
                                &mut fx.env_a,
                                &mut fx.env_d,
                                &mut fx.env_s,
                                &mut fx.env_r,
                                &mut fx.env_shape,
                                &mut fx.env_loop,
                            )
                        } else {
                            (
                                &mut fx.fenv_a,
                                &mut fx.fenv_d,
                                &mut fx.fenv_s,
                                &mut fx.fenv_r,
                                &mut fx.fenv_shape,
                                &mut fx.fenv_loop,
                            )
                        };
                        let accent = if amp { Accent::Moss } else { Accent::Iris };
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(if amp { "AMP" } else { "FILT" }).size(10.5).color(t.muted).strong(),
                            );
                            ui.add_space(ui.available_width() - 52.0);
                            if button(ui, &t, "Loop", *lp, Vec2::new(48.0, 18.0)).clicked() {
                                *lp = !*lp;
                            }
                        });
                        let times = env_seconds(*a, *d, *s, *r);
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(268.0, 64.0), Sense::hover());
                        env_scope(ui.painter(), &t, rect, times, *shape, *lp, accent.color(&t));
                        let labels: Vec<&str> = ENV_SHAPES.iter().map(|s| s.1).collect();
                        let sel = ENV_SHAPES.iter().position(|s| s.0 == *shape).unwrap_or(0);
                        if let Some(i) = segmented(ui, &t, &labels, sel, 268.0) {
                            *shape = ENV_SHAPES[i].0;
                        }
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let ms = |s: f64| format!("{} ms", (s * 1000.0).round());
                            knob(ui, &t, a, "Attack", &ms(times.a), KnobSize::Sm, accent);
                            knob(ui, &t, d, "Decay", &ms(times.d), KnobSize::Sm, accent);
                            let sh = format!("{}", (times.s * 100.0).round());
                            knob(ui, &t, s, "Sustain", &sh, KnobSize::Sm, accent);
                            knob(ui, &t, r, "Release", &ms(times.r), KnobSize::Sm, accent);
                        });
                    });
                }
            });
        });
    }

    fn filter_rack(&mut self, ui: &mut Ui) {
        let t = self.theme;
        module(ui, &t, "Filter", 300.0, |ui| {
            let fx = &mut self.fx;
            let cut_hz = filt_cutoff(fx.filt_type, fx.filt_cut);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(296.0, 64.0), Sense::hover());
            filt_scope(ui.painter(), &t, rect, fx.filt_type, cut_hz, fx.filt_res);
            let labels: Vec<&str> = FILT_TYPES.iter().map(|f| f.1).collect();
            let sel = FILT_TYPES.iter().position(|f| f.0 == fx.filt_type).unwrap_or(0);
            if let Some(i) = segmented(ui, &t, &labels, sel, 296.0) {
                fx.filt_type = FILT_TYPES[i].0;
            }
            let modes = [FiltAuto::Off, FiltAuto::Env, FiltAuto::Sweep];
            let sel = modes.iter().position(|m| *m == fx.filt_auto).unwrap_or(1);
            if let Some(i) = segmented(ui, &t, &["Off", "Env", "Sweep"], sel, 150.0) {
                fx.filt_auto = modes[i];
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let hz = format!("{} Hz", cut_hz.round());
                knob_d(
                    ui,
                    &t,
                    &mut fx.filt_cut,
                    Some(iki_core::engine::Fx::default().filt_cut),
                    "Cutoff",
                    &hz,
                    KnobSize::Lg,
                    Accent::Moss,
                );
                let rh = format!("{}", (fx.filt_res * 100.0).round());
                knob_d(
                    ui,
                    &t,
                    &mut fx.filt_res,
                    Some(iki_core::engine::Fx::default().filt_res),
                    "Res",
                    &rh,
                    KnobSize::Lg,
                    Accent::Moss,
                );
                let mh = format!("{}", (fx.filt_motion * 100.0).round());
                knob_d(
                    ui,
                    &t,
                    &mut fx.filt_motion,
                    Some(iki_core::engine::Fx::default().filt_motion),
                    "Motion",
                    &mh,
                    KnobSize::Sm,
                    Accent::Iris,
                );
                let rate = format_hz(log_hz(fx.filt_rate, 0.03, 8.0));
                knob_d(
                    ui,
                    &t,
                    &mut fx.filt_rate,
                    Some(iki_core::engine::Fx::default().filt_rate),
                    "Rate",
                    &rate,
                    KnobSize::Sm,
                    Accent::Iris,
                );
            });
        });
    }

    fn div_select(ui: &mut Ui, id: &str, value: &mut usize) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Div").size(10.5));
            egui::ComboBox::from_id_salt(id).width(64.0).selected_text(DELAY_DIVS[*value].id).show_ui(ui, |ui| {
                for (i, d) in DELAY_DIVS.iter().enumerate() {
                    ui.selectable_value(value, i, d.id);
                }
            });
        });
    }

    fn lfo_rack(&mut self, ui: &mut Ui) {
        let t = self.theme;
        let hz = self.lfo_readout();
        module(ui, &t, "Lfo", 330.0, |ui| {
            let fx = &mut self.fx;
            ui.horizontal(|ui| {
                if let Some(i) = segmented(ui, &t, &["Free", "Bars"], fx.lfo_sync as usize, 100.0) {
                    fx.lfo_sync = i == 1;
                }
                if fx.lfo_sync {
                    Self::div_select(ui, "lfo-div", &mut fx.lfo_div);
                    ui.label(RichText::new(&hz).size(10.5).color(t.muted));
                }
            });
            ui.horizontal(|ui| {
                if !fx.lfo_sync {
                    knob_d(
                        ui,
                        &t,
                        &mut fx.mod_rate,
                        Some(iki_core::engine::Fx::default().mod_rate),
                        "Rate",
                        &hz,
                        KnobSize::Sm,
                        Accent::Moss,
                    );
                }
                ui.vertical(|ui| {
                    ui.label(RichText::new("WAVE").size(9.5).color(t.subtle));
                    if let Some(w) = panels::wave_picker(ui, &t, fx.lfo_wave) {
                        fx.lfo_wave = w;
                    }
                });
            });
            ui.label(RichText::new("DEST").size(9.5).color(t.subtle));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let lanes: [(&str, bool, f64, Accent); 3] = [
                    ("Pitch", fx.lfo_pitch, fx.lfo_pitch_amt, Accent::Moss),
                    ("Filt", fx.lfo_filt, fx.lfo_filt_amt, Accent::Iris),
                    ("Amp", fx.lfo_amp, fx.lfo_amp_amt, Accent::Iris),
                ];
                for (k, (label, on, _amt, accent)) in lanes.into_iter().enumerate() {
                    ui.vertical(|ui| {
                        ui.set_width(100.0);
                        ui.horizontal(|ui| {
                            if rocker(ui, &t, on, label, "", ui.id().with(("lfo-dest", k)), false) {
                                match k {
                                    0 => fx.lfo_pitch = !on,
                                    1 => fx.lfo_filt = !on,
                                    _ => fx.lfo_amp = !on,
                                }
                            }
                            let hint = match k {
                                0 => format!("±{} c", (fx.lfo_pitch_amt * 120.0).round()),
                                1 => format!("{} Hz", (fx.lfo_filt_amt * 2400.0).round()),
                                _ => format!("{}", (fx.lfo_amp_amt * 100.0).round()),
                            };
                            let v = match k {
                                0 => &mut fx.lfo_pitch_amt,
                                1 => &mut fx.lfo_filt_amt,
                                _ => &mut fx.lfo_amp_amt,
                            };
                            ui.add_space(-14.0);
                            knob(ui, &t, v, "Depth", &hint, KnobSize::Sm, accent);
                        });
                    });
                }
            });
        });
    }

    // ------------------------------------------------------------ space rack

    fn fx_rack(&mut self, ui: &mut Ui) {
        let t = self.theme;
        let time_hint = self.delay_readout();
        module(ui, &t, "Delay", 236.0, |ui| {
            let fx = &mut self.fx;
            ui.horizontal(|ui| {
                if let Some(i) = segmented(ui, &t, &["Mono", "Ping"], fx.delay_ping as usize, 110.0) {
                    fx.delay_ping = i == 1;
                }
                if let Some(i) = segmented(ui, &t, &["Free", "Bars"], fx.delay_sync as usize, 110.0) {
                    fx.delay_sync = i == 1;
                }
            });
            if fx.delay_sync {
                Self::div_select(ui, "delay-div", &mut fx.delay_div);
            }
            ui.horizontal(|ui| {
                if !fx.delay_sync {
                    knob_d(
                        ui,
                        &t,
                        &mut fx.delay_time,
                        Some(iki_core::engine::Fx::default().delay_time),
                        "Time",
                        &time_hint,
                        KnobSize::Sm,
                        Accent::Iris,
                    );
                }
                let h = format!("{}", (fx.delay_feedback * 100.0).round());
                knob_d(
                    ui,
                    &t,
                    &mut fx.delay_feedback,
                    Some(iki_core::engine::Fx::default().delay_feedback),
                    "Feedback",
                    &h,
                    KnobSize::Sm,
                    Accent::Iris,
                );
                let h = format!("{}", (fx.delay_mix * 100.0).round());
                knob_d(
                    ui,
                    &t,
                    &mut fx.delay_mix,
                    Some(iki_core::engine::Fx::default().delay_mix),
                    "Mix",
                    &h,
                    KnobSize::Sm,
                    Accent::Iris,
                );
            });
            ui.horizontal(|ui| {
                let h = format!("{} Hz", (1200.0 * 10f64.powf(fx.delay_tone)).round());
                knob_d(
                    ui,
                    &t,
                    &mut fx.delay_tone,
                    Some(iki_core::engine::Fx::default().delay_tone),
                    "Tone",
                    &h,
                    KnobSize::Sm,
                    Accent::Iris,
                );
                let h = format!("{} Hz", (40.0 * 20f64.powf(fx.delay_lowcut)).round());
                knob_d(
                    ui,
                    &t,
                    &mut fx.delay_lowcut,
                    Some(iki_core::engine::Fx::default().delay_lowcut),
                    "Low cut",
                    &h,
                    KnobSize::Sm,
                    Accent::Iris,
                );
            });
            if fx.delay_sync {
                ui.label(RichText::new(&time_hint).size(10.5).color(t.muted));
            }
        });
        module(ui, &t, "Space", 250.0, |ui| {
            let fx = &mut self.fx;
            ui.label(RichText::new("ROOM").size(9.5).color(t.subtle));
            if let Some(i) = segmented(ui, &t, &iki_core::engine::REVERB_TYPES, fx.reverb_type(), 240.0) {
                fx.set_reverb_type(i);
            }
            let fx = &mut self.fx;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let h = format!("{}", (fx.reverb * 100.0).round());
                knob_d(
                    ui,
                    &t,
                    &mut fx.reverb,
                    Some(iki_core::engine::Fx::default().reverb),
                    "Mix",
                    &h,
                    KnobSize::Sm,
                    Accent::Iris,
                );
                let h = format!("{}", (fx.reverb_decay * 100.0).round());
                knob_d(
                    ui,
                    &t,
                    &mut fx.reverb_decay,
                    Some(iki_core::engine::Fx::default().reverb_decay),
                    "Decay",
                    &h,
                    KnobSize::Sm,
                    Accent::Iris,
                );
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let h = format!("×{:.2}", 0.5 * 4f64.powf(fx.reverb_size));
                knob_d(
                    ui,
                    &t,
                    &mut fx.reverb_size,
                    Some(iki_core::engine::Fx::default().reverb_size),
                    "Size",
                    &h,
                    KnobSize::Sm,
                    Accent::Iris,
                );
                let h = format!("{} ms", (fx.reverb_pre * 120.0).round());
                knob_d(
                    ui,
                    &t,
                    &mut fx.reverb_pre,
                    Some(iki_core::engine::Fx::default().reverb_pre),
                    "Pre",
                    &h,
                    KnobSize::Sm,
                    Accent::Iris,
                );
                let h = format!("{} Hz", (9000.0 * (800.0f64 / 9000.0).powf(fx.reverb_damp)).round());
                knob_d(
                    ui,
                    &t,
                    &mut fx.reverb_damp,
                    Some(iki_core::engine::Fx::default().reverb_damp),
                    "Damp",
                    &h,
                    KnobSize::Sm,
                    Accent::Iris,
                );
            });
            let wide = fx.width > 0.4;
            if let Some(i) = segmented(ui, &t, &["Tight", "Wide"], wide as usize, 150.0) {
                fx.width = if i == 1 { 0.72 } else { 0.1 };
            }
        });
        module(ui, &t, "Color", 206.0, |ui| {
            let fx = &mut self.fx;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let h = format!("{}", (fx.tape * 100.0).round());
                knob_d(
                    ui,
                    &t,
                    &mut fx.tape,
                    Some(iki_core::engine::Fx::default().tape),
                    "Tape",
                    &h,
                    KnobSize::Sm,
                    Accent::Moss,
                );
                let h = format!("{}", (fx.phaser * 100.0).round());
                knob_d(
                    ui,
                    &t,
                    &mut fx.phaser,
                    Some(iki_core::engine::Fx::default().phaser),
                    "Phaser",
                    &h,
                    KnobSize::Sm,
                    Accent::Iris,
                );
                let h = format!("{}", (fx.phaser_depth * 100.0).round());
                knob_d(
                    ui,
                    &t,
                    &mut fx.phaser_depth,
                    Some(iki_core::engine::Fx::default().phaser_depth),
                    "Depth",
                    &h,
                    KnobSize::Sm,
                    Accent::Iris,
                );
            });
            ui.label(
                RichText::new(if self.snap.phaser_routed { "phaser routed (L)" } else { "phaser idle" })
                    .size(9.5)
                    .color(t.subtle),
            );
        });
    }

    /// Extra desktop indicators: engine / device state at a glance.
    fn output_module(&mut self, ui: &mut Ui) {
        let t = self.theme;
        let s = self.snap.clone();
        let (engine_line, (clock_line, late)) = (self.engine_readout(), self.clock_readout());
        module(ui, &t, "Engine", 300.0, |ui| {
            let bar = |ui: &mut Ui, label: &str, v: f32, c: Color32, text: String| {
                panels::meter_bar(ui, &t, label, v, c, text)
            };
            bar(ui, "out L  ", s.peak_l, t.moss, format!("{:.2}", s.peak_l));
            bar(ui, "out R  ", s.peak_r, t.iris, format!("{:.2}", s.peak_r));
            bar(ui, "breath", s.breath, t.moss, format!("{:.2}", s.breath));
            bar(ui, "comp   ", s.comp_gr / 12.0, t.danger, format!("-{:.1} dB", s.comp_gr));
            bar(ui, "limiter", s.limit_gr / 12.0, t.danger, format!("-{:.1} dB", s.limit_gr));
            // Momentary loudness, -60 … 0 dBFS.
            bar(ui, "loud   ", (s.loud_db + 60.0) / 60.0, t.iris, format!("{:.0} dBFS", s.loud_db.max(-99.0)));
            ui.label(RichText::new(engine_line).size(10.0).color(t.subtle));
            ui.label(RichText::new(clock_line).size(10.0).color(if late { t.danger } else { t.subtle }));
        });
    }

    /// `foli.ts` — percussion accompaniment (triplet grid, follows the loop clock).
    fn foli_module(&mut self, ui: &mut Ui) {
        use iki_core::engine::Cmd;
        use iki_core::shaku::foli::FoliFeel;
        let t = self.theme;
        let s = self.snap.clone();
        module(ui, &t, "Foli · drums", 290.0, |ui| {
            ui.horizontal(|ui| {
                if button(ui, &t, if s.foli_on { "On" } else { "Off" }, s.foli_on, Vec2::new(44.0, 22.0)).clicked() {
                    self.foli(Cmd::FoliOn(!s.foli_on));
                }
                let labels: Vec<&str> = FoliFeel::ALL.iter().map(|f| f.1).collect();
                let sel = FoliFeel::ALL.iter().position(|f| f.0 == self.foli_feel).unwrap_or(0);
                if let Some(i) = segmented(ui, &t, &labels, sel, 150.0) {
                    self.ctl.set_foli_feel(FoliFeel::ALL[i].0);
                }
            });
            ui.label(RichText::new("PRESET").size(9.5).color(t.subtle));
            if let Some(i) = segmented(ui, &t, &iki_core::shaku::foli::KIT_NAMES, self.ctl.foli_kit, 282.0) {
                self.ctl.set_foli_kit(i);
            }
            ui.horizontal(|ui| {
                let before = self.foli_level;
                let h = format!("{}", (self.foli_level * 100.0).round());
                knob_d(ui, &t, &mut self.foli_level, Some(0.55), "Level", &h, KnobSize::Sm, Accent::Iris);
                if self.foli_level != before {
                    let v = self.foli_level;
                    self.ctl.set_foli_level(v);
                }
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(format!("{} bpm · 12/8", tempo_bpm(self.fx.delay_tempo)))
                            .size(10.0)
                            .color(t.muted),
                    );
                    if let Some(i) = panels::foli_steps(ui, &t, &s.foli_mask, s.foli_step) {
                        self.foli(Cmd::FoliToggleStep(i));
                    }
                });
            });
        });
    }

    fn footer(&mut self, ui: &mut Ui) {
        let t = self.theme;
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Space gate · A–H kari · Z–N meri · ↑↓←→ step note · O octave · M muraiki · 1–4 track · R punch · T stop · U undo",
            )
            .size(10.5)
            .color(t.subtle),
        );
    }
}
