//! The plugin window: the desktop front panel (header, CRT · stage · blow
//! column, note pads, Shape and Space racks) drawn with the shared `iki-ui`
//! pieces. Knobs and switches edit host parameters (so Live records and shows
//! them as automation); pads, faders and buttons send gestures to the audio
//! thread; readouts come from the same kernel code as the apps, via a
//! detached controller that mirrors the audio thread's state.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, Align2, Pos2, Rect, RichText, Sense, Ui, Vec2};
use nih_plug_egui::resizable_window::ResizableWindow;
use nih_plug_egui::{EguiSettings, create_egui_editor};

use iki_core::controller::Controller;
use iki_core::dsp::Wave;
use iki_core::engine::FiltAuto;
use iki_core::params::*;
use iki_core::shaku::env_shape::{ENV_SHAPES, FILT_TYPES, env_seconds, filt_cutoff};
use iki_core::shaku::fingering::{FINGERINGS, NOTE_MAX, Voicing};
use iki_core::shaku::timing::DELAY_DIVS;
use iki_ui::keys::shortcut;
use iki_ui::panels;
use iki_ui::scopes::{Spectrogram, env_scope, filt_scope};
use iki_ui::theme::{Theme, install_fonts};
use iki_ui::widgets::{
    Accent, KnobSize, PadLook, button, fader_value, knob, led, module, pad, paint_fader, press_edges, rocker, segmented,
};

use crate::Shared;
use crate::instrument::Gesture;
use crate::params::{IkiParams, Kind};

const KARI_KEYS: [&str; 6] = ["A", "S", "D", "F", "G", "H"];
const MERI_KEYS: [&str; 6] = ["Z", "X", "C", "V", "B", "N"];

struct Gui {
    params: Arc<IkiParams>,
    shared: Arc<Shared>,
    /// Holds the audio thread's state for drawing and formats the readouts.
    mirror: Controller,
    theme: Theme,
    applied_night: Option<bool>,
    spectro: Spectrogram,
    last_frame: Instant,
    frame_ms: f32,
    pad_down: [[bool; 6]; 2],
    note_fader_down: bool,
    lean_fader_down: bool,
    muraiki_down: bool,
    bend_down: bool,
    show_help: bool,
    keys_down: Vec<egui::Key>,
    foli_mask: [bool; 12],
}

pub fn create(params: Arc<IkiParams>, shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
    let gui = Gui {
        params: params.clone(),
        shared,
        mirror: Controller::detached(),
        theme: Theme::get(false),
        applied_night: None,
        spectro: Spectrogram::default(),
        last_frame: Instant::now(),
        frame_ms: 16.7,
        pad_down: [[false; 6]; 2],
        note_fader_down: false,
        lean_fader_down: false,
        muraiki_down: false,
        bend_down: false,
        show_help: false,
        keys_down: Vec::new(),
        foli_mask: [false; 12],
    };
    // egui draws gamma-space colours itself and never turns on sRGB
    // conversion, so an sRGB-capable framebuffer isn't needed; not asking for
    // one lets the window open on more GPUs, drivers and VMs.
    let mut settings = EguiSettings::default();
    settings.gl_config.srgb = false;
    create_egui_editor(
        params.editor_state.clone(),
        gui,
        settings,
        |ctx, _queue, _gui| install_fonts(ctx),
        |ctx, setter, _queue, gui| gui.frame(ctx, setter),
    )
}

// ------------------------------------------------------------------ parameter helpers

fn float(params: &IkiParams, id: i32) -> &FloatParam {
    match params.get(id).map(|p| &p.kind) {
        Some(Kind::Float(p)) => p,
        _ => panic!("parameter {id} is not continuous"),
    }
}

fn boolean(params: &IkiParams, id: i32) -> &BoolParam {
    match params.get(id).map(|p| &p.kind) {
        Some(Kind::Bool(p)) => p,
        _ => panic!("parameter {id} is not a switch"),
    }
}

fn int(params: &IkiParams, id: i32) -> &IntParam {
    match params.get(id).map(|p| &p.kind) {
        Some(Kind::Int(p)) => p,
        _ => panic!("parameter {id} is not a choice"),
    }
}

/// One complete host edit (a click, a wheel step).
fn set_once<P: Param>(setter: &ParamSetter, p: &P, v: P::Plain) {
    setter.begin_set_parameter(p);
    setter.set_parameter(p, v);
    setter.end_set_parameter(p);
}

impl Gui {
    fn pv(&self, id: i32) -> f64 {
        self.params.get(id).map(|p| p.value()).unwrap_or(0.0)
    }

    fn toggle(&self, setter: &ParamSetter, id: i32) {
        let p = boolean(&self.params, id);
        set_once(setter, p, !p.value());
    }

    fn choose(&self, setter: &ParamSetter, id: i32, i: usize) {
        set_once(setter, int(&self.params, id), i as i32);
    }

    /// A knob bound to a host parameter. `shown` is what to draw (MIDI can move
    /// meri / iki away from the parameter's value).
    #[allow(clippy::too_many_arguments)]
    fn knob(
        &self,
        ui: &mut Ui,
        setter: &ParamSetter,
        id: i32,
        shown: f64,
        label: &str,
        size: KnobSize,
        accent: Accent,
    ) {
        let t = self.theme;
        let hint = self.hint(id);
        let mut v = shown;
        let resp = knob(ui, &t, &mut v, label, &hint, size, accent);
        let p = float(&self.params, id);
        if resp.drag_started() {
            setter.begin_set_parameter(p);
        }
        if v != shown {
            if resp.dragged() {
                setter.set_parameter(p, v as f32);
            } else {
                set_once(setter, p, v as f32); // mouse wheel
            }
        }
        if resp.drag_stopped() {
            setter.end_set_parameter(p);
        }
        if resp.double_clicked() {
            set_once(setter, p, p.default_plain_value());
        }
    }

    fn hint(&self, id: i32) -> String {
        match id {
            P_MERI => "Flatten".into(),
            P_IKI => "Breath".into(),
            _ => param_hint(&self.mirror, id),
        }
    }

    fn gesture(&self, g: Gesture) {
        if let Ok(mut tx) = self.shared.gestures.lock() {
            let _ = tx.push(g);
        }
    }

    // ------------------------------------------------------------------ frame

    fn frame(&mut self, ctx: &egui::Context, setter: &ParamSetter) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f64().min(0.25);
        self.frame_ms = self.frame_ms * 0.9 + (dt * 1000.0) as f32 * 0.1;
        self.last_frame = now;

        let night = self.params.night.load(Ordering::Relaxed);
        if self.applied_night != Some(night) {
            self.theme = Theme::get(night);
            self.theme.apply(ctx);
            self.applied_night = Some(night);
        }
        self.sync_mirror();
        let sr = self.shared.sample_rate.lock().map(|s| *s).unwrap_or(48000.0);
        if let Ok(mut scope) = self.shared.scope.lock()
            && let Some(scope) = scope.as_mut()
        {
            while let Ok((l, r)) = scope.pop() {
                self.spectro.push(l, r);
            }
        }
        self.spectro.frame(dt, sr);
        self.keyboard(ctx);

        let t = self.theme;
        let state = self.params.editor_state.clone();
        // Drag the bottom-right corner to resize; the racks re-flow.
        ResizableWindow::new("iki-window").min_size(Vec2::new(760.0, 480.0)).show(ctx, &state, |ui| {
            egui::Frame::new().fill(t.bg).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
                    self.header(ui, setter);
                    ui.horizontal_top(|ui| {
                        self.crt(ui);
                        self.stage(ui, setter);
                        self.play_col(ui, setter);
                    });
                    self.keys_strip(ui);
                    ui.add_space(4.0);
                    strip_title(ui, &t, "Shape");
                    self.rack(ui, setter, &[Rack::Env, Rack::Filter, Rack::Lfo]);
                    ui.add_space(4.0);
                    strip_title(ui, &t, &format!("Space · {}", self.mirror.bpm_readout()));
                    self.rack(ui, setter, &[Rack::Delay, Rack::Space, Rack::Color, Rack::Engine, Rack::Foli]);
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "MIDI: notes play (velocity → iki) · pitch bend → bend · mod wheel → yuri · CC 7 / 11 / aftertouch → iki · CC 74 → meri · sustain pedal · \
                             keys when the window has focus: Space gate · A–H kari · Z–N meri · ↑↓ note · O octave · M muraiki",
                        )
                        .size(10.5)
                        .color(t.subtle),
                    );
                });
            });
        });
    }

    /// Copy the audio thread's state into the mirror controller.
    fn sync_mirror(&mut self) {
        let perf = self.shared.perf.lock().map(|p| *p).unwrap_or_default();
        let m = &mut self.mirror;
        for p in &self.params.list {
            if !matches!(p.id, P_MERI | P_IKI | P_OTSU | P_LEAN) {
                set_param(m, p.id, p.value());
            }
        }
        m.meri = perf.meri;
        m.iki = perf.iki;
        m.otsu = perf.otsu;
        m.lean = perf.lean;
        m.kari_note = perf.kari_note;
        m.meri_note = perf.meri_note;
        m.voicing = if perf.meri_lane { Voicing::Meri } else { Voicing::Kari };
        m.blowing = perf.blowing;
        m.latched = perf.latched;
        m.ornament = perf.ornament;
        m.muraiki_flash = perf.muraiki.then(|| Instant::now() + Duration::from_millis(50));
        m.loop_track = perf.loop_track;
        m.host_bpm = (perf.bpm > 0.0).then_some(perf.bpm);
        let snap = self.shared.snapshot.lock().ok().and_then(|s| s.clone());
        if let Some(snap) = snap
            && let Ok(s) = snap.lock()
        {
            m.snap.clone_from(&s);
        }
        // remember the Foli pattern with the set
        if m.snap.foli_mask != self.foli_mask {
            self.foli_mask = m.snap.foli_mask;
            let bits = self.foli_mask.iter().enumerate().fold(0u32, |b, (i, on)| b | ((*on as u32) << i));
            self.params.foli_mask.store(bits, Ordering::Relaxed);
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            match ev {
                egui::Event::Key { key, physical_key, pressed, repeat, modifiers } => {
                    let k = physical_key.unwrap_or(key);
                    let Some(s) = shortcut(k) else { continue };
                    if modifiers.ctrl || modifiers.alt || modifiers.command {
                        continue;
                    }
                    if pressed {
                        if !repeat && !self.keys_down.contains(&k) {
                            self.keys_down.push(k);
                            self.gesture(Gesture::KeyDown(s));
                        }
                    } else {
                        self.keys_down.retain(|d| *d != k);
                        self.gesture(Gesture::KeyUp(s));
                    }
                }
                egui::Event::WindowFocused(false) => {
                    self.keys_down.clear();
                    self.gesture(Gesture::ReleaseKeys);
                }
                _ => {}
            }
        }
    }

    // ------------------------------------------------------------------ header

    fn header(&mut self, ui: &mut Ui, setter: &ParamSetter) {
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
                if let Some(i) = segmented(ui, &t, &["Flute", "Bore", "Both"], self.pv(P_VOICE) as usize, 190.0) {
                    self.choose(setter, P_VOICE, i);
                }
            });
            ui.add_space(4.0);
            self.knob(ui, setter, P_FLUTE_LEVEL, self.pv(P_FLUTE_LEVEL), "Flute", KnobSize::Sm, Accent::Moss);
            self.knob(ui, setter, P_BORE_LEVEL, self.pv(P_BORE_LEVEL), "Bore", KnobSize::Sm, Accent::Iris);
            self.knob(ui, setter, P_TUNE, self.pv(P_TUNE), "Tune", KnobSize::Sm, Accent::Moss);
            ui.add_space(12.0);
            let perf = self.shared.perf.lock().map(|p| *p).unwrap_or_default();
            let (status_rect, _) =
                ui.allocate_exact_size(Vec2::new((ui.available_width() - 190.0).max(120.0), 40.0), Sense::hover());
            let p = ui.painter();
            let mid = status_rect.center().y;
            p.line_segment(
                [Pos2::new(status_rect.left(), mid + 14.0), Pos2::new(status_rect.right(), mid + 14.0)],
                egui::Stroke::new(1.0_f32, t.border),
            );
            let loaded = self.mirror.snap.loaded;
            p.text(
                Pos2::new(status_rect.right(), mid - 6.0),
                Align2::RIGHT_CENTER,
                format!(
                    "{}  ·  {}  ·  voices {}  ·  {:.0} fps",
                    if perf.playing { "▶ host playing" } else { "host stopped" },
                    if loaded > 0 { format!("{loaded} samples") } else { "loading samples…".into() },
                    self.mirror.snap.voices,
                    1000.0 / self.frame_ms.max(1.0)
                ),
                iki_ui::widgets::small(10.5),
                t.muted,
            );
            let night = self.params.night.load(Ordering::Relaxed);
            if button(ui, &t, if night { "Temple" } else { "Breath" }, night, Vec2::new(70.0, 24.0)).clicked() {
                self.params.night.store(!night, Ordering::Relaxed);
            }
            if button(ui, &t, "Info", self.show_help, Vec2::new(46.0, 24.0)).clicked() {
                self.show_help = !self.show_help;
            }
            led(ui, &t, self.mirror.latched, t.moss);
        });
        if self.show_help {
            let mut rows = panels::KEY_HELP[..5].to_vec();
            rows.push(("MIDI", "notes play (velocity → iki), pitch wheel → bend, CC 1 → yuri, CC 7 / 11 / aftertouch → iki, CC 74 → meri, CC 64 sustain"));
            rows.push(("mouse", "drag knobs vertically or scroll; double-click resets"));
            panels::help_card(ui, &t, &rows);
        }
    }

    // ------------------------------------------------------------------ play strip

    fn crt(&mut self, ui: &mut Ui) {
        let t = self.theme;
        let named = self.mirror.note_name();
        let sub = self.mirror.pitch_readout();
        let s = &self.mirror.snap;
        let d = panels::CrtData {
            kana: &named.kana,
            name: &named.name,
            sub: &sub,
            blowing: self.mirror.blowing,
            breath: s.breath,
            peak_l: s.peak_l,
            peak_r: s.peak_r,
            cutoff_hz: s.cutoff_hz,
            comp_gr: s.comp_gr,
            footer: "MIDI IN · A S D F G H · Z X C V B N · SPACE BLOW · O OCTAVE · M MURAIKI",
        };
        panels::crt(ui, &t, &d, &mut self.spectro, Vec2::new(470.0, 300.0));
    }

    fn stage(&mut self, ui: &mut Ui, setter: &ParamSetter) {
        let t = self.theme;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(250.0, 300.0), Sense::hover());
        let h = rect.height() - 16.0;
        let top = rect.top() + 8.0;
        // lean fader (left, meri) → host parameter
        let lean_rect = Rect::from_min_size(Pos2::new(rect.left() + 18.0, top), Vec2::new(40.0, h));
        let lr = ui.interact(lean_rect, ui.id().with("lean-fader"), Sense::click_and_drag());
        let lpad = (h * 0.08).min(28.0);
        let (pressed, released, down) = press_edges(&lr, &mut self.lean_fader_down);
        let lean_p = float(&self.params, P_LEAN);
        if pressed {
            setter.begin_set_parameter(lean_p);
        }
        let mut lean = self.mirror.lean;
        if down && let Some(pos) = lr.interact_pointer_pos() {
            lean = fader_value(lean_rect, pos.y, lpad);
            setter.set_parameter(lean_p, lean as f32);
        }
        if released {
            setter.end_set_parameter(lean_p);
        }
        let p = ui.painter();
        paint_fader(p, &t, lean_rect, lean, lpad, t.iris, lean > 0.0);
        for (label, at) in [("メリ", 0.92), ("カリ", 0.08)] {
            p.text(
                Pos2::new(lean_rect.left() - 2.0, lean_rect.bottom() - lpad - (h - 2.0 * lpad) * at),
                Align2::RIGHT_CENTER,
                label,
                iki_ui::widgets::small(10.0),
                t.subtle,
            );
        }
        // flute body
        let body = Rect::from_center_size(Pos2::new(rect.left() + 118.0, rect.center().y), Vec2::new(40.0, h));
        let time = ui.input(|i| i.time);
        let bend_st = ((self.pv(P_BEND) - 0.5) * 2.0 * (1.0 + 11.0 * self.pv(P_BEND_RANGE))) as f32;
        let live = panels::FluteLive {
            holes: &self.mirror.holes(),
            blowing: self.mirror.blowing,
            breath: self.mirror.snap.breath,
            bend_st,
            meri: self.mirror.meri as f32,
        };
        panels::flute_live(ui.ctx(), p, &t, body, &live, time);
        // note fader (right, kari) → gestures
        let nf = Rect::from_min_size(Pos2::new(rect.left() + 168.0, top), Vec2::new(36.0, h));
        let npad = (h * 0.08).min(28.0);
        let nr = ui.interact(nf, ui.id().with("note-fader"), Sense::click_and_drag());
        let (pressed, released, down) = press_edges(&nr, &mut self.note_fader_down);
        let to_note = |y: f32| ((fader_value(nf, y, npad) * NOTE_MAX as f64).round() as usize).min(NOTE_MAX);
        if let Some(pos) = nr.interact_pointer_pos() {
            let n = to_note(pos.y);
            if pressed {
                self.gesture(Gesture::SlideStart(n, Voicing::Kari));
                self.mirror.kari_note = n;
            } else if down && n != self.mirror.kari_note {
                self.gesture(Gesture::ParkNote(n, Voicing::Kari));
                self.mirror.kari_note = n;
            }
        }
        if released {
            self.gesture(Gesture::SlideEnd);
        }
        let active = self.mirror.voicing == Voicing::Kari;
        let p = ui.painter();
        paint_fader(p, &t, nf, self.mirror.kari_note as f64 / NOTE_MAX as f64, npad, t.moss, active);
        for (i, f) in FINGERINGS.iter().enumerate() {
            let y = nf.bottom() - npad - (nf.height() - 2.0 * npad) * i as f32 / NOTE_MAX as f32;
            let c = if active && i == self.mirror.kari_note { t.fg } else { t.subtle };
            p.text(Pos2::new(nf.right() + 6.0, y), Align2::LEFT_CENTER, f.kana, iki_ui::widgets::small(12.0), c);
        }
    }

    fn play_col(&mut self, ui: &mut Ui, setter: &ParamSetter) {
        let t = self.theme;
        ui.vertical(|ui| {
            ui.set_width(330.0);
            ui.horizontal(|ui| {
                if panels::blow_button(ui, &t, self.mirror.latched).clicked() {
                    self.gesture(Gesture::ToggleGate);
                }
                ui.add_space(6.0);
                let resp = panels::muraiki_button(ui, &t, self.mirror.muraiki_flash.is_some());
                let (pressed, _, _) = press_edges(&resp, &mut self.muraiki_down);
                if pressed {
                    self.gesture(Gesture::Muraiki);
                }
                ui.add_space(6.0);
                if rocker(ui, &t, self.mirror.otsu, "Kan", "Otsu", ui.id().with("octave"), true) {
                    set_once(setter, boolean(&self.params, P_OTSU), !self.mirror.otsu);
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                self.knob(ui, setter, P_MERI, self.mirror.meri, "Meri", KnobSize::Lg, Accent::Moss);
                self.knob(ui, setter, P_IKI, self.mirror.iki, "Iki", KnobSize::Lg, Accent::Iris);
                self.knob(ui, setter, P_GAIN, self.pv(P_GAIN), "Gain", KnobSize::Lg, Accent::Moss);
            });
            // Expression: pitch bend strip (springs back on release) + yuri / glide.
            let bend_p = float(&self.params, P_BEND);
            let mut bv = self.pv(P_BEND);
            let bend_hint = self.hint(P_BEND);
            let moving = iki_ui::widgets::bend_strip(ui, &t, &mut bv, 322.0, &bend_hint);
            ui.add_space(12.0);
            if moving {
                if !self.bend_down {
                    setter.begin_set_parameter(bend_p);
                    self.bend_down = true;
                }
                setter.set_parameter(bend_p, bv as f32);
            } else if self.bend_down {
                setter.set_parameter(bend_p, 0.5);
                setter.end_set_parameter(bend_p);
                self.bend_down = false;
            }
            ui.horizontal(|ui| {
                self.knob(ui, setter, P_BEND_RANGE, self.pv(P_BEND_RANGE), "Range", KnobSize::Sm, Accent::Iris);
                self.knob(ui, setter, P_YURI, self.pv(P_YURI), "Yuri", KnobSize::Sm, Accent::Moss);
                self.knob(ui, setter, P_YURI_RATE, self.pv(P_YURI_RATE), "Rate", KnobSize::Sm, Accent::Moss);
                self.knob(ui, setter, P_GLIDE, self.pv(P_GLIDE), "Glide", KnobSize::Sm, Accent::Iris);
            });
            ui.label(RichText::new(self.mirror.holes_readout()).size(10.5).color(t.muted));
            ui.label(
                RichText::new(format!(
                    "{}  ·  lane {}",
                    if self.mirror.blowing { "blowing" } else { "silent" },
                    if self.mirror.voicing == Voicing::Meri { "meri" } else { "kari" },
                ))
                .size(10.5)
                .color(t.muted),
            );
        });
    }

    // ------------------------------------------------------------------ note pads

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
                let note = if meri { self.mirror.meri_note } else { self.mirror.kari_note };
                for (i, f) in FINGERINGS.iter().enumerate() {
                    let kana = if meri { format!("{}メ", f.kana) } else { f.kana.to_string() };
                    let name = if meri { format!("{} meri", f.name) } else { f.name.to_string() };
                    let flash = self.mirror.ornament.filter(|o| o.lane == lane && o.index == i).map(|o| o.koro);
                    let look = PadLook {
                        kana: &kana,
                        name: &name,
                        key: if meri { MERI_KEYS[i] } else { KARI_KEYS[i] },
                        on: i == note,
                        live: self.mirror.voicing == lane && i == note,
                        iris: meri || i % 2 == 1,
                        flash,
                    };
                    let resp = pad(ui, &t, Vec2::new(w, 66.0), &look, ui.id().with(("pad", meri, i)));
                    let (pressed, released, _) = press_edges(&resp, &mut self.pad_down[meri as usize][i]);
                    if pressed {
                        self.gesture(Gesture::HitStart(i, lane));
                    }
                    if released {
                        self.gesture(Gesture::SlideEnd);
                    }
                }
            });
        }
    }

    /// Modules side by side, wrapping onto a new row at the window edge
    /// (egui can't wrap containers by itself: their widths are known up front).
    fn rack(&mut self, ui: &mut Ui, setter: &ParamSetter, modules: &[Rack]) {
        const GAP: f32 = 10.0;
        let avail = ui.available_width();
        let mut rows: Vec<Vec<Rack>> = vec![Vec::new()];
        let mut used = 0.0;
        for &m in modules {
            let w = m.width() + 22.0; // frame margins + stroke
            if used > 0.0 && used + GAP + w > avail {
                rows.push(Vec::new());
                used = 0.0;
            }
            used += if used > 0.0 { GAP + w } else { w };
            if let Some(r) = rows.last_mut() {
                r.push(m);
            }
        }
        for row in rows {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(GAP, GAP);
                for m in row {
                    match m {
                        Rack::Env => self.env_rack(ui, setter),
                        Rack::Filter => self.filter_rack(ui, setter),
                        Rack::Lfo => self.lfo_rack(ui, setter),
                        Rack::Delay => self.delay_module(ui, setter),
                        Rack::Space => self.space_module(ui, setter),
                        Rack::Color => self.color_module(ui, setter),
                        Rack::Engine => self.engine_module(ui),
                        Rack::Foli => self.foli_module(ui, setter),
                    }
                }
            });
        }
    }

    // ------------------------------------------------------------------ shape rack

    fn env_rack(&mut self, ui: &mut Ui, setter: &ParamSetter) {
        let t = self.theme;
        module(ui, &t, "Env", Rack::Env.width(), |ui| {
            ui.horizontal_top(|ui| {
                for amp in [true, false] {
                    ui.vertical(|ui| {
                        ui.set_width(272.0);
                        let ids = if amp {
                            (P_ENV_A, P_ENV_D, P_ENV_S, P_ENV_R, P_ENV_SHAPE, P_ENV_LOOP)
                        } else {
                            (P_FENV_A, P_FENV_D, P_FENV_S, P_FENV_R, P_FENV_SHAPE, P_FENV_LOOP)
                        };
                        let accent = if amp { Accent::Moss } else { Accent::Iris };
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(if amp { "AMP" } else { "FILT" }).size(10.5).color(t.muted).strong(),
                            );
                            ui.add_space(ui.available_width() - 52.0);
                            if button(ui, &t, "Loop", self.pv(ids.5) > 0.5, Vec2::new(48.0, 18.0)).clicked() {
                                self.toggle(setter, ids.5);
                            }
                        });
                        let (a, d, s, r) = (self.pv(ids.0), self.pv(ids.1), self.pv(ids.2), self.pv(ids.3));
                        let shape = ENV_SHAPES[(self.pv(ids.4) as usize).min(ENV_SHAPES.len() - 1)].0;
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(268.0, 64.0), Sense::hover());
                        env_scope(
                            ui.painter(),
                            &t,
                            rect,
                            env_seconds(a, d, s, r),
                            shape,
                            self.pv(ids.5) > 0.5,
                            accent.color(&t),
                        );
                        let labels: Vec<&str> = ENV_SHAPES.iter().map(|s| s.1).collect();
                        if let Some(i) = segmented(ui, &t, &labels, self.pv(ids.4) as usize, 268.0) {
                            self.choose(setter, ids.4, i);
                        }
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            self.knob(ui, setter, ids.0, a, "Attack", KnobSize::Sm, accent);
                            self.knob(ui, setter, ids.1, d, "Decay", KnobSize::Sm, accent);
                            self.knob(ui, setter, ids.2, s, "Sustain", KnobSize::Sm, accent);
                            self.knob(ui, setter, ids.3, r, "Release", KnobSize::Sm, accent);
                        });
                    });
                }
            });
        });
    }

    fn filter_rack(&mut self, ui: &mut Ui, setter: &ParamSetter) {
        let t = self.theme;
        module(ui, &t, "Filter", Rack::Filter.width(), |ui| {
            let fx = self.mirror.fx.clone();
            let cut_hz = filt_cutoff(fx.filt_type, fx.filt_cut);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(296.0, 64.0), Sense::hover());
            filt_scope(ui.painter(), &t, rect, fx.filt_type, cut_hz, fx.filt_res);
            let labels: Vec<&str> = FILT_TYPES.iter().map(|f| f.1).collect();
            if let Some(i) = segmented(ui, &t, &labels, self.pv(P_FILT_TYPE) as usize, 296.0) {
                self.choose(setter, P_FILT_TYPE, i);
            }
            let modes = [FiltAuto::Off, FiltAuto::Env, FiltAuto::Sweep];
            let sel = modes.iter().position(|m| *m == fx.filt_auto).unwrap_or(1);
            if let Some(i) = segmented(ui, &t, &["Off", "Env", "Sweep"], sel, 150.0) {
                self.choose(setter, P_FILT_AUTO, i);
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                self.knob(ui, setter, P_FILT_CUT, fx.filt_cut, "Cutoff", KnobSize::Lg, Accent::Moss);
                self.knob(ui, setter, P_FILT_RES, fx.filt_res, "Res", KnobSize::Lg, Accent::Moss);
                self.knob(ui, setter, P_FILT_MOTION, fx.filt_motion, "Motion", KnobSize::Sm, Accent::Iris);
                self.knob(ui, setter, P_FILT_RATE, fx.filt_rate, "Rate", KnobSize::Sm, Accent::Iris);
            });
        });
    }

    fn div_select(&self, ui: &mut Ui, setter: &ParamSetter, id: i32) {
        let cur = self.pv(id) as usize;
        let mut v = cur;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Div").size(10.5));
            egui::ComboBox::from_id_salt(("div", id)).width(64.0).selected_text(DELAY_DIVS[cur.min(8)].id).show_ui(
                ui,
                |ui| {
                    for (i, d) in DELAY_DIVS.iter().enumerate() {
                        ui.selectable_value(&mut v, i, d.id);
                    }
                },
            );
        });
        if v != cur {
            self.choose(setter, id, v);
        }
    }

    fn lfo_rack(&mut self, ui: &mut Ui, setter: &ParamSetter) {
        let t = self.theme;
        module(ui, &t, "Lfo", Rack::Lfo.width(), |ui| {
            let fx = self.mirror.fx.clone();
            let hz = self.mirror.lfo_readout();
            ui.horizontal(|ui| {
                if let Some(i) = segmented(ui, &t, &["Free", "Bars"], fx.lfo_sync as usize, 100.0) {
                    set_once(setter, boolean(&self.params, P_LFO_SYNC), i == 1);
                }
                if fx.lfo_sync {
                    self.div_select(ui, setter, P_LFO_DIV);
                    ui.label(RichText::new(&hz).size(10.5).color(t.muted));
                }
            });
            ui.horizontal(|ui| {
                if !fx.lfo_sync {
                    self.knob(ui, setter, P_LFO_RATE, fx.mod_rate, "Rate", KnobSize::Sm, Accent::Moss);
                }
                ui.vertical(|ui| {
                    ui.label(RichText::new("WAVE").size(9.5).color(t.subtle));
                    if let Some(w) = panels::wave_picker(ui, &t, fx.lfo_wave) {
                        let i = Wave::ALL.iter().position(|x| x.0 == w).unwrap_or(0);
                        self.choose(setter, P_LFO_WAVE, i);
                    }
                });
            });
            ui.label(RichText::new("DEST").size(9.5).color(t.subtle));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let lanes = [
                    ("Pitch", P_LFO_PITCH, P_LFO_PITCH_AMT, fx.lfo_pitch, fx.lfo_pitch_amt, Accent::Moss),
                    ("Filt", P_LFO_FILT, P_LFO_FILT_AMT, fx.lfo_filt, fx.lfo_filt_amt, Accent::Iris),
                    ("Amp", P_LFO_AMP, P_LFO_AMP_AMT, fx.lfo_amp, fx.lfo_amp_amt, Accent::Iris),
                ];
                for (k, (label, sw, amt, on, v, accent)) in lanes.into_iter().enumerate() {
                    ui.vertical(|ui| {
                        ui.set_width(100.0);
                        ui.horizontal(|ui| {
                            if rocker(ui, &t, on, label, "", ui.id().with(("lfo-dest", k)), false) {
                                self.toggle(setter, sw);
                            }
                            ui.add_space(-14.0);
                            self.knob(ui, setter, amt, v, "Depth", KnobSize::Sm, accent);
                        });
                    });
                }
            });
        });
    }

    // ------------------------------------------------------------------ space rack

    fn delay_module(&mut self, ui: &mut Ui, setter: &ParamSetter) {
        let t = self.theme;
        let fx = self.mirror.fx.clone();
        let time_hint = self.mirror.delay_readout();
        module(ui, &t, "Delay", Rack::Delay.width(), |ui| {
            ui.horizontal(|ui| {
                if let Some(i) = segmented(ui, &t, &["Mono", "Ping"], fx.delay_ping as usize, 110.0) {
                    set_once(setter, boolean(&self.params, P_DELAY_PING), i == 1);
                }
                if let Some(i) = segmented(ui, &t, &["Free", "Bars"], fx.delay_sync as usize, 110.0) {
                    set_once(setter, boolean(&self.params, P_DELAY_SYNC), i == 1);
                }
            });
            if fx.delay_sync {
                self.div_select(ui, setter, P_DELAY_DIV);
            }
            ui.horizontal(|ui| {
                if !fx.delay_sync {
                    self.knob(ui, setter, P_DELAY_TIME, fx.delay_time, "Time", KnobSize::Sm, Accent::Iris);
                }
                self.knob(ui, setter, P_DELAY_FEEDBACK, fx.delay_feedback, "Feedback", KnobSize::Sm, Accent::Iris);
                self.knob(ui, setter, P_DELAY_MIX, fx.delay_mix, "Mix", KnobSize::Sm, Accent::Iris);
            });
            ui.horizontal(|ui| {
                self.knob(ui, setter, P_DELAY_TONE, fx.delay_tone, "Tone", KnobSize::Sm, Accent::Iris);
                self.knob(ui, setter, P_DELAY_LOWCUT, fx.delay_lowcut, "Low cut", KnobSize::Sm, Accent::Iris);
            });
            if fx.delay_sync {
                ui.label(RichText::new(&time_hint).size(10.5).color(t.muted));
            }
        });
    }

    fn space_module(&mut self, ui: &mut Ui, setter: &ParamSetter) {
        let t = self.theme;
        let fx = self.mirror.fx.clone();
        module(ui, &t, "Space", Rack::Space.width(), |ui| {
            ui.label(RichText::new("ROOM").size(9.5).color(t.subtle));
            if let Some(i) = segmented(ui, &t, &iki_core::engine::REVERB_TYPES, fx.reverb_type(), 240.0) {
                self.choose(setter, P_REVERB_TYPE, i);
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                self.knob(ui, setter, P_REVERB, fx.reverb, "Mix", KnobSize::Sm, Accent::Iris);
                self.knob(ui, setter, P_REVERB_DECAY, fx.reverb_decay, "Decay", KnobSize::Sm, Accent::Iris);
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                self.knob(ui, setter, P_REVERB_SIZE, fx.reverb_size, "Size", KnobSize::Sm, Accent::Iris);
                self.knob(ui, setter, P_REVERB_PRE, fx.reverb_pre, "Pre", KnobSize::Sm, Accent::Iris);
                self.knob(ui, setter, P_REVERB_DAMP, fx.reverb_damp, "Damp", KnobSize::Sm, Accent::Iris);
            });
            let wide = fx.width > 0.4;
            if let Some(i) = segmented(ui, &t, &["Tight", "Wide"], wide as usize, 150.0) {
                set_once(setter, float(&self.params, P_WIDTH), if i == 1 { 0.72 } else { 0.1 });
            }
        });
    }

    fn color_module(&mut self, ui: &mut Ui, setter: &ParamSetter) {
        let t = self.theme;
        let fx = self.mirror.fx.clone();
        module(ui, &t, "Color", Rack::Color.width(), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                self.knob(ui, setter, P_TAPE, fx.tape, "Tape", KnobSize::Sm, Accent::Moss);
                self.knob(ui, setter, P_PHASER, fx.phaser, "Phaser", KnobSize::Sm, Accent::Iris);
                self.knob(ui, setter, P_PHASER_DEPTH, fx.phaser_depth, "Depth", KnobSize::Sm, Accent::Iris);
            });
            ui.label(
                RichText::new(if self.mirror.snap.phaser_routed { "phaser routed (L)" } else { "phaser idle" })
                    .size(9.5)
                    .color(t.subtle),
            );
        });
    }

    fn engine_module(&mut self, ui: &mut Ui) {
        let t = self.theme;
        let s = self.mirror.snap.clone();
        let perf = self.shared.perf.lock().map(|p| *p).unwrap_or_default();
        let engine_line = self.mirror.engine_readout();
        module(ui, &t, "Engine", Rack::Engine.width(), |ui| {
            panels::meter_bar(ui, &t, "out L  ", s.peak_l, t.moss, format!("{:.2}", s.peak_l));
            panels::meter_bar(ui, &t, "out R  ", s.peak_r, t.iris, format!("{:.2}", s.peak_r));
            panels::meter_bar(ui, &t, "breath", s.breath, t.moss, format!("{:.2}", s.breath));
            panels::meter_bar(ui, &t, "comp   ", s.comp_gr / 12.0, t.danger, format!("-{:.1} dB", s.comp_gr));
            ui.label(RichText::new(engine_line).size(10.0).color(t.subtle));
            ui.label(
                RichText::new(format!("audio clock {:.1} s · host block {} frames", s.now, perf.block))
                    .size(10.0)
                    .color(t.subtle),
            );
        });
    }

    /// `foli.ts` — percussion accompaniment, on the host's bars while it plays.
    fn foli_module(&mut self, ui: &mut Ui, setter: &ParamSetter) {
        let t = self.theme;
        let s = self.mirror.snap.clone();
        let bpm = self.mirror.bpm_readout();
        module(ui, &t, "Foli · drums", Rack::Foli.width(), |ui| {
            ui.horizontal(|ui| {
                let on = self.pv(P_FOLI_ON) > 0.5;
                if button(ui, &t, if on { "On" } else { "Off" }, on, Vec2::new(44.0, 22.0)).clicked() {
                    self.toggle(setter, P_FOLI_ON);
                }
                if let Some(i) = segmented(ui, &t, &P_FOLI_FEEL_LABELS, self.pv(P_FOLI_FEEL) as usize, 150.0) {
                    self.choose(setter, P_FOLI_FEEL, i);
                }
            });
            ui.label(RichText::new("PRESET").size(9.5).color(t.subtle));
            if let Some(i) = segmented(ui, &t, &P_FOLI_KIT_LABELS, self.pv(P_FOLI_KIT) as usize, 282.0) {
                self.choose(setter, P_FOLI_KIT, i);
            }
            ui.horizontal(|ui| {
                self.knob(ui, setter, P_FOLI_LEVEL, self.pv(P_FOLI_LEVEL), "Level", KnobSize::Sm, Accent::Iris);
                ui.vertical(|ui| {
                    ui.label(RichText::new(format!("{bpm} · 12/8")).size(10.0).color(t.muted));
                    if let Some(i) = panels::foli_steps(ui, &t, &s.foli_mask, s.foli_step) {
                        self.gesture(Gesture::FoliStep(i));
                    }
                });
            });
        });
    }
}

/// The rack modules and their widths (as on the desktop).
#[derive(Clone, Copy)]
enum Rack {
    Env,
    Filter,
    Lfo,
    Delay,
    Space,
    Color,
    Engine,
    Foli,
}

impl Rack {
    fn width(self) -> f32 {
        match self {
            Rack::Env => 560.0,
            Rack::Filter => 300.0,
            Rack::Lfo => 330.0,
            Rack::Delay => 236.0,
            Rack::Space => 250.0,
            Rack::Color => 206.0,
            Rack::Engine => 300.0,
            Rack::Foli => 290.0,
        }
    }
}

const P_FOLI_FEEL_LABELS: [&str; 3] = ["Work", "Dance", "Pulse"];
const P_FOLI_KIT_LABELS: [&str; 7] = iki_core::shaku::foli::KIT_NAMES;

fn strip_title(ui: &mut Ui, t: &Theme, s: &str) {
    ui.label(RichText::new(s.to_uppercase()).size(12.0).color(t.muted).strong());
}
