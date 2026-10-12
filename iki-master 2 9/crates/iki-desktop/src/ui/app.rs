//! Desktop (egui) frontend state: presentation-only bits (theme, widget drag
//! state, spectrogram, MIDI ports) around the shared [`Controller`], which
//! holds all instrument behaviour. `App` derefs to the controller, so views
//! read/write `self.fx`, `self.kari_note`, … directly.

use std::ops::{Deref, DerefMut};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui::{self, Event, Key};

use super::scopes::Spectrogram;
use super::theme::{Theme, install_fonts};
use crate::midi_bus::{MidiBus, MidiBusSnap};
use iki_core::audio::AudioHandle;
use iki_core::controller::Controller;
use iki_core::engine::VoiceMode;
use iki_core::shaku::looper::TRACK_COUNT;
use iki_core::shaku::midi::{self, ClipPlayer, LiveEvent};

pub use iki_ui::keys::shortcut;

#[derive(Clone, Copy, Debug)]
enum KeyAction {
    Down(Key),
    Up(Key),
    FocusLost,
}

#[derive(Default, Clone, Copy)]
pub struct LoopWaveUi {
    pub view_a: f64,
    pub view_b: f64,
    pub trim_in: f64,
    pub trim_out: f64,
    /// Some(false) = dragging the in-handle, Some(true) = out-handle.
    pub drag: Option<bool>,
    pub last_x: f32,
}

pub enum MidiMsg {
    Raw(Vec<u8>),
    Clip(LiveEvent),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MidiStatus {
    Off,
    On,
    Err,
}

pub struct MidiUi {
    pub bus: MidiBus,
    pub open: bool,
    pub status: MidiStatus,
    pub device: String,
    pub clip_name: Option<(String, Instant)>,
    pub rx: Receiver<MidiMsg>,
    pub tx: Sender<MidiMsg>,
    pub clip: Option<ClipPlayer>,
    pub path: String,
    pub snap: MidiBusSnap,
    pub snap_at: Instant,
}

pub struct App {
    pub ctl: Controller,
    pub theme: Theme,
    applied_night: Option<bool>,
    pub night: bool,
    pub spectro: Spectrogram,
    last_frame: Instant,
    pub pad_down: [[bool; 6]; 2],
    pub note_fader_down: bool,
    pub lean_fader_down: bool,
    pub blow_down: bool,
    pub loop_ui: [LoopWaveUi; TRACK_COUNT],
    key_actions: Vec<KeyAction>,
    keys_down: Vec<Key>,
    pub midi: MidiUi,
    pub show_help: bool,
    pub export_pending: bool,
    pub export_msg: String,
    pub frame_ms: f32,
}

impl Deref for App {
    type Target = Controller;
    fn deref(&self) -> &Controller {
        &self.ctl
    }
}

impl DerefMut for App {
    fn deref_mut(&mut self) -> &mut Controller {
        &mut self.ctl
    }
}

#[cfg(target_os = "macos")]
fn prefs_path() -> Option<std::path::PathBuf> {
    let home = std::path::PathBuf::from(std::env::var_os("HOME")?);
    Some(home.join("Library").join("Application Support").join("Iki").join("prefs"))
}

#[cfg(not(target_os = "macos"))]
fn prefs_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))?;
    Some(base.join("iki").join("prefs"))
}

/// The patch the app was left on, restored at the next start.
fn last_patch_path() -> Option<std::path::PathBuf> {
    Some(prefs_path()?.parent()?.join("last.ikipreset"))
}

/// Where user patches live: `<config>/iki/presets/*.ikipreset`.
pub fn presets_dir() -> Option<std::path::PathBuf> {
    Some(prefs_path()?.parent()?.join("presets"))
}

fn load_prefs() -> (bool, VoiceMode) {
    let txt = prefs_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    let night = txt.lines().any(|l| l.trim() == "temple=1");
    let mode = if txt.lines().any(|l| l.trim() == "voice=both") {
        VoiceMode::Both
    } else if txt.lines().any(|l| l.trim() == "voice=bore") {
        VoiceMode::Bore
    } else {
        VoiceMode::Sampler
    };
    (night, mode)
}

fn save_prefs(night: bool, mode: VoiceMode) {
    if let Some(p) = prefs_path() {
        let _ = std::fs::create_dir_all(p.parent().unwrap_or(&p));
        let voice = mode.label();
        let _ = std::fs::write(p, format!("temple={}\nvoice={voice}\n", night as u8));
    }
}

impl App {
    /// Write a delivered loop export to `<config>/iki/exports/` (one WAV per track + a mixdown).
    pub fn finish_export(&mut self) {
        if !self.export_pending {
            return;
        }
        let Some(ex) = self.ctl.take_loop_export() else { return };
        self.export_pending = false;
        if ex.tracks.is_empty() {
            self.export_msg = "nothing recorded yet".into();
            return;
        }
        let Some(dir) = presets_dir().and_then(|d| Some(d.parent()?.join("exports"))) else {
            self.export_msg = "no config folder".into();
            return;
        };
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let sr = ex.sample_rate as u32;
        let mix_len = ex.tracks.iter().map(|t| t.1.len()).max().unwrap_or(0);
        let mut mix = vec![0f32; mix_len];
        let mut res = std::fs::create_dir_all(&dir);
        for (i, data) in &ex.tracks {
            for (k, m) in mix.iter_mut().enumerate() {
                *m += data[k % data.len()] * 0.7;
            }
            let wav = iki_core::shaku::wav::encode_i16(data, 1, sr);
            res = res.and_then(|_| std::fs::write(dir.join(format!("loop-{stamp}-track{}.wav", i + 1)), wav));
        }
        let wav = iki_core::shaku::wav::encode_i16(&mix, 1, sr);
        res = res.and_then(|_| std::fs::write(dir.join(format!("loop-{stamp}-mix.wav")), wav));
        self.export_msg = match res {
            Ok(()) => format!("saved to {}", dir.display()),
            Err(e) => format!("export failed: {e}"),
        };
    }

    pub fn new(cc: &eframe::CreationContext<'_>, audio: AudioHandle, night_override: Option<bool>) -> Self {
        install_fonts(&cc.egui_ctx);
        let (night_pref, voice_mode) = load_prefs();
        let night = night_override.unwrap_or(night_pref);
        let repaint = cc.egui_ctx.clone();
        let mut ctl = Controller::new(audio, voice_mode, move || repaint.request_repaint());
        // Pick up where the last session left off.
        if let Some(text) = last_patch_path().and_then(|p| std::fs::read_to_string(p).ok()) {
            let _ = iki_core::params::load_preset(&mut ctl, &text);
        }
        let (mtx, mrx) = mpsc::channel();
        let bus = MidiBus::new();
        let snap = bus.snap();
        Self {
            ctl,
            theme: Theme::get(night),
            applied_night: None,
            night,
            spectro: Spectrogram::default(),
            last_frame: Instant::now(),
            pad_down: [[false; 6]; 2],
            note_fader_down: false,
            lean_fader_down: false,
            blow_down: false,
            loop_ui: [LoopWaveUi { view_a: 0.0, view_b: 1.0, trim_in: 0.0, trim_out: 1.0, drag: None, last_x: 0.0 };
                TRACK_COUNT],
            key_actions: Vec::new(),
            keys_down: Vec::new(),
            midi: MidiUi {
                bus,
                open: false,
                status: MidiStatus::Off,
                device: "MIDI".into(),
                clip_name: None,
                rx: mrx,
                tx: mtx,
                clip: None,
                path: String::new(),
                snap,
                snap_at: Instant::now(),
            },
            show_help: false,
            export_pending: false,
            export_msg: String::new(),
            frame_ms: 0.0,
        }
    }

    pub fn set_voice_mode(&mut self, m: VoiceMode) {
        self.ctl.set_voice_mode(m);
        save_prefs(self.night, m);
    }

    pub fn set_night(&mut self, night: bool) {
        self.night = night;
        save_prefs(night, self.ctl.voice_mode);
    }

    /// Audio problems plus the desktop-specific hint.
    pub fn audio_problem(&mut self) -> Option<String> {
        self.ctl.audio_problem().map(|p| format!("{p}. Run `iki --list-devices`, then `--host` / `--device`."))
    }

    fn process_keys(&mut self) {
        let actions = std::mem::take(&mut self.key_actions);
        for a in actions {
            match a {
                KeyAction::Down(k) => {
                    if let Some(s) = shortcut(k) {
                        self.ctl.key_down(s);
                    }
                }
                KeyAction::Up(k) => {
                    if let Some(s) = shortcut(k) {
                        self.ctl.key_up(s);
                    }
                }
                KeyAction::FocusLost => self.ctl.release_keys(),
            }
        }
    }

    // --------------------------------------------------------------- MIDI

    pub fn midi_sink(&self, ctx: &egui::Context) -> crate::midi_bus::HwSink {
        let tx = self.midi.tx.clone();
        let ctx = ctx.clone();
        std::sync::Arc::new(move |bytes: Vec<u8>| {
            let _ = tx.send(MidiMsg::Raw(bytes));
            ctx.request_repaint();
        })
    }

    fn poll_midi(&mut self) {
        let mut evs = Vec::new();
        while let Ok(m) = self.midi.rx.try_recv() {
            match m {
                MidiMsg::Raw(b) => self.ctl.midi_in(&b),
                MidiMsg::Clip(e) => evs.push(e),
            }
        }
        for e in evs {
            self.ctl.live(e);
        }
        if let Some((_, until)) = &self.midi.clip_name
            && Instant::now() > *until
        {
            self.midi.clip_name = None;
        }
        if self.midi.snap_at.elapsed() > Duration::from_millis(400) {
            self.midi.snap = self.midi.bus.snap();
            self.midi.snap_at = Instant::now();
        }
    }

    fn midi_tx_effect(&mut self) {
        for b in self.ctl.midi_tx() {
            self.midi.bus.send_app(&b);
        }
    }

    pub fn midi_connect(&mut self, ctx: &egui::Context) {
        let sink = self.midi_sink(ctx);
        match self.midi.bus.enable(sink) {
            Ok(name) => {
                self.midi.device = name;
                self.midi.status = MidiStatus::On;
            }
            Err(e) => {
                self.midi.status = MidiStatus::Err;
                self.midi.device = e;
            }
        }
        self.midi.snap = self.midi.bus.snap();
    }

    pub fn midi_disconnect(&mut self) {
        self.midi.bus.disable();
        self.midi.status = MidiStatus::Off;
        self.midi.device = "MIDI".into();
        self.midi.snap = self.midi.bus.snap();
    }

    /// `onFile`: parse a Standard MIDI File and play it through the instrument.
    pub fn play_midi_bytes(&mut self, ctx: &egui::Context, bytes: &[u8], name: &str) {
        match midi::parse_smf(bytes, name) {
            Ok(clip) => {
                let until = Instant::now() + Duration::from_secs_f64((clip.duration + 0.2).max(0.4));
                self.midi.clip_name = Some((clip.name.clone(), until));
                let tx = self.midi.tx.clone();
                let send = self.midi.bus.sender();
                let ctx = ctx.clone();
                self.midi.clip = Some(ClipPlayer::play(clip, move |ev, bytes| {
                    if let Some(b) = bytes {
                        send(&b);
                    }
                    let _ = tx.send(MidiMsg::Clip(ev));
                    ctx.request_repaint();
                }));
            }
            Err(e) => {
                self.midi.status = MidiStatus::Err;
                self.midi.device = e;
            }
        }
    }

    pub fn play_midi_file(&mut self, ctx: &egui::Context, path: &str) {
        let p = std::path::Path::new(path.trim());
        match std::fs::read(p) {
            Ok(bytes) => {
                let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "clip".into());
                self.play_midi_bytes(ctx, &bytes, &name);
            }
            Err(e) => {
                self.midi.status = MidiStatus::Err;
                self.midi.device = format!("{}: {e}", p.display());
            }
        }
    }

    // ------------------------------------------------------------- frame
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(p) = last_patch_path() {
            let _ = std::fs::create_dir_all(p.parent().unwrap_or(&p));
            let _ = std::fs::write(p, iki_core::params::preset_text(&self.ctl, "last session"));
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let typing = ctx.wants_keyboard_input();
        let mut keep = Vec::with_capacity(raw.events.len());
        for ev in raw.events.drain(..) {
            match &ev {
                Event::Key { key, physical_key, pressed, repeat, modifiers } if !typing => {
                    let k = physical_key.unwrap_or(*key);
                    if shortcut(k).is_some() && !modifiers.ctrl && !modifiers.alt && !modifiers.command {
                        // `e.repeat` on the web. Not every backend flags X11/Wayland
                        // auto-repeat, so a press of a key we already hold is a repeat too.
                        if *pressed {
                            if !*repeat && !self.keys_down.contains(&k) {
                                self.keys_down.push(k);
                                self.key_actions.push(KeyAction::Down(k));
                            }
                        } else {
                            self.keys_down.retain(|d| *d != k);
                            self.key_actions.push(KeyAction::Up(k));
                        }
                        continue; // swallow so focused widgets don't also react (e.g. Space)
                    }
                    keep.push(ev);
                }
                Event::WindowFocused(false) => {
                    self.keys_down.clear();
                    self.key_actions.push(KeyAction::FocusLost);
                    keep.push(ev);
                }
                _ => keep.push(ev),
            }
        }
        raw.events = keep;
        for f in &raw.dropped_files {
            if let Some(p) = &f.path {
                self.midi.path = p.display().to_string();
            }
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.finish_export();
        let ctx = ctx.clone();
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f64().min(0.25);
        self.frame_ms = self.frame_ms * 0.9 + (dt * 1000.0) as f32 * 0.1;
        self.last_frame = now;

        if self.applied_night != Some(self.night) {
            self.theme = Theme::get(self.night);
            self.theme.apply(&ctx);
            self.applied_night = Some(self.night);
        }
        let dropped: Vec<String> = ctx.input(|i| {
            i.raw.dropped_files.iter().filter_map(|f| f.path.as_ref().map(|p| p.display().to_string())).collect()
        });
        for p in dropped {
            if p.to_lowercase().ends_with(".mid") || p.to_lowercase().ends_with(".midi") {
                self.midi.open = true;
                self.play_midi_file(&ctx, &p);
            }
        }

        self.process_keys();
        self.poll_midi();
        let spectro = &mut self.spectro;
        self.ctl.tick(|l, r| spectro.push(l, r));
        self.spectro.frame(dt, self.ctl.audio.sample_rate);

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(&ctx, |ui| self.view(ui));

        self.sync_engine();
        self.midi_tx_effect();
        // ~60 fps animation. egui subtracts its predicted frame time from the
        // delay, so a bare 16 ms would mean "repaint immediately" (busy loop).
        let predicted = ctx.input(|i| i.predicted_dt).clamp(0.0, 0.1);
        ctx.request_repaint_after(Duration::from_secs_f32(1.0 / 60.0 + predicted));
    }
}
