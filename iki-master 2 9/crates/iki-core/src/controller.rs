//! The UI-agnostic instrument controller — the behaviour half of the web
//! `iki-app.tsx` (and the live-MIDI half of `midi-dock.tsx`).
//!
//! Frontends (egui desktop, Android Views, …) own only presentation. They
//! forward gestures here (`on_hit_start`, `toggle_gate`, `key_down`, …), edit
//! the public parameter fields directly (knobs/faders), and call [`Controller::tick`]
//! once per frame, which pushes every changed value to the audio engine in the
//! same order the React effects did.

use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use crate::audio::AudioHandle;
use crate::engine::{Cmd, Fx, FxSet, Snapshot, VoiceMode};
use crate::shaku::fingering::{Holes, Voicing, clamp_note, holes_from_index, meri_depth, name_from_holes, played_hz};
use crate::shaku::foli::FoliFeel;
use crate::shaku::looper::{LoopMode, TRACK_COUNT};
use crate::shaku::midi::{self, LiveEvent, Sustain};
use crate::shaku::samples::{Bank, load_bank};

/// Platform-neutral keyboard shortcuts (the web `KEY_HITS` + control keys).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A S D F G H → kari Ro…Hi, Z X C V B N → meri Ro…Hi.
    Hit(usize, Voicing),
    Space,
    Octave,
    Muraiki,
    Track(usize),
    Punch,
    Stop,
    Undo,
    NoteUp,
    NoteDown,
}

impl Key {
    /// Map a US-layout key name ("a", "space", "1", "up", …) to a shortcut.
    pub fn from_name(name: &str) -> Option<Key> {
        const KARI: &str = "asdfgh";
        const MERI: &str = "zxcvbn";
        let n = name.to_ascii_lowercase();
        if n.len() == 1 {
            let c = n.as_bytes()[0] as char;
            if let Some(i) = KARI.find(c) {
                return Some(Key::Hit(i, Voicing::Kari));
            }
            if let Some(i) = MERI.find(c) {
                return Some(Key::Hit(i, Voicing::Meri));
            }
            return match c {
                'o' => Some(Key::Octave),
                'm' => Some(Key::Muraiki),
                'r' => Some(Key::Punch),
                't' => Some(Key::Stop),
                'u' => Some(Key::Undo),
                '1'..='4' => Some(Key::Track(c as usize - '1' as usize)),
                ' ' => Some(Key::Space),
                _ => None,
            };
        }
        match n.as_str() {
            "space" => Some(Key::Space),
            "up" | "right" => Some(Key::NoteUp),
            "down" | "left" => Some(Key::NoteDown),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Ornament {
    pub lane: Voicing,
    pub index: usize,
    pub koro: bool,
    pub until: Instant,
}

#[derive(Clone, PartialEq)]
struct Sent {
    holes: Holes,
    depth: f64,
    lean: f64,
    iki: f64,
    otsu: bool,
    fx: Fx,
}

/// Live-MIDI bookkeeping (sustain, legato stack, Tx echo suppression).
struct MidiLive {
    sustain: Sustain,
    /// Scratch for `midi_in` (kept to stay allocation-free on an audio thread).
    evs: Vec<LiveEvent>,
    held: Vec<u8>,
    from_hw: bool,
    last_tx: (u8, bool),
    last_sig: Option<(bool, usize, bool, u64)>,
}

pub struct Controller {
    pub audio: AudioHandle,
    /// A plugin host's tempo; replaces the clock knob in the readouts.
    pub host_bpm: Option<f64>,
    bank_rx: Option<Receiver<Result<Bank, String>>>,
    pub bank_status: String,
    pub voice_mode: VoiceMode,

    pub kari_note: usize,
    pub meri_note: usize,
    pub voicing: Voicing,
    pub meri: f64,
    pub lean: f64,
    pub iki: f64,
    pub otsu: bool,
    pub blowing: bool,
    pub latched: bool,
    export_slot: crate::engine::ExportSlot,
    pub fx: Fx,
    pub loop_levels: [f64; TRACK_COUNT],
    pub loop_track: usize,
    pub foli_feel: FoliFeel,
    pub foli_kit: usize,
    pub foli_level: f64,
    hold_play: bool,
    gated: bool,
    held_keys: Vec<Key>,
    last_tap: Option<(Instant, usize, Voicing)>,
    skip_pad_end: bool,
    pub ornament: Option<Ornament>,
    pub muraiki_flash: Option<Instant>,

    sent: Option<Sent>,
    pub snap: Snapshot,
    midi: MidiLive,
    trace: bool,
    started: Instant,
    cb_last: u64,
    cb_seen: Instant,
}

impl Controller {
    /// `wake` is called (from a loader thread) once the samples are ready.
    pub fn new(audio: AudioHandle, voice_mode: VoiceMode, wake: impl FnOnce() + Send + 'static) -> Self {
        let (btx, brx) = mpsc::channel();
        std::thread::Builder::new()
            .name("iki-samples".into())
            .spawn(move || {
                let _ = btx.send(load_bank());
                wake();
            })
            .ok();
        Self::build(audio, voice_mode, Some(brx))
    }

    /// A controller with no engine behind it (its commands go nowhere). A
    /// frontend whose engine lives elsewhere — a plugin editor — keeps one to
    /// hold the state it shows and format the same readouts as the app.
    pub fn detached() -> Self {
        Self::build(crate::audio::external(48000.0, String::new()).handle, VoiceMode::Sampler, None)
    }

    fn build(audio: AudioHandle, voice_mode: VoiceMode, bank_rx: Option<Receiver<Result<Bank, String>>>) -> Self {
        let mut c = Self {
            audio,
            bank_status: if bank_rx.is_some() { "loading samples…".into() } else { String::new() },
            bank_rx,
            host_bpm: None,
            voice_mode,
            kari_note: 0,
            meri_note: 0,
            voicing: Voicing::Kari,
            meri: 0.14,
            lean: 0.0,
            iki: 0.52,
            otsu: true,
            blowing: false,
            latched: false,
            export_slot: Default::default(),
            fx: Fx::default(),
            loop_levels: [0.9; TRACK_COUNT],
            loop_track: 0,
            foli_feel: FoliFeel::Work,
            foli_kit: 0,
            foli_level: 0.55,
            hold_play: false,
            gated: false,
            held_keys: Vec::with_capacity(32),
            last_tap: None,
            skip_pad_end: false,
            ornament: None,
            muraiki_flash: None,
            sent: None,
            snap: Snapshot::default(),
            midi: MidiLive {
                sustain: Sustain { on: false, held: Vec::with_capacity(128) },
                held: Vec::with_capacity(128),
                evs: Vec::with_capacity(8),
                from_hw: false,
                last_tx: (62, false),
                last_sig: None,
            },
            trace: std::env::var_os("IKI_TRACE").is_some(),
            started: Instant::now(),
            cb_last: 0,
            cb_seen: Instant::now(),
        };
        if voice_mode != VoiceMode::Sampler {
            c.audio.send(Cmd::VoiceMode(voice_mode));
        }
        c.sync_engine();
        c
    }

    // ------------------------------------------------------------ derived

    pub fn note(&self) -> usize {
        if self.voicing == Voicing::Meri { self.meri_note } else { self.kari_note }
    }

    pub fn holes(&self) -> Holes {
        holes_from_index(self.note())
    }

    pub fn depth(&self) -> f64 {
        meri_depth(self.voicing, self.meri, self.lean)
    }

    pub fn heard_hz(&self) -> f64 {
        played_hz(self.note(), self.otsu, self.meri, self.lean, self.voicing)
    }

    pub fn note_name(&self) -> crate::shaku::fingering::NoteName {
        name_from_holes(&self.holes(), self.meri, self.iki, self.voicing, self.otsu, self.lean)
    }

    /// The CRT subtitle: "Ro otsu env sine 1:play".
    pub fn pitch_label(&self) -> String {
        let named = self.note_name();
        let mut parts: Vec<String> = vec![named.name.clone()];
        if self.otsu {
            parts.push("otsu".into());
        }
        if named.kan {
            parts.push("kan".into());
        }
        match self.fx.filt_auto {
            crate::engine::FiltAuto::Env => parts.push("env".into()),
            crate::engine::FiltAuto::Sweep => parts.push("sweep".into()),
            crate::engine::FiltAuto::Off => {}
        }
        if self.fx.lfo_pitch || self.fx.lfo_filt || self.fx.lfo_amp {
            parts.push(self.fx.lfo_wave.name().into());
        }
        let loops: Vec<String> = self
            .snap
            .loop_bank
            .tracks
            .iter()
            .filter(|t| t.mode != LoopMode::Idle)
            .map(|t| format!("{}:{}", t.id + 1, t.mode.name()))
            .collect();
        if !loops.is_empty() {
            parts.push(loops.join(" "));
        }
        parts.join(" ")
    }

    // ------------------------------------------------------------ readouts
    // Text shown by every frontend, formatted once here so they read the same.

    /// CRT subtitle under the note name: "293 Hz · Ro otsu env".
    pub fn pitch_readout(&self) -> String {
        format!("{} Hz · {}", self.heard_hz().round(), self.pitch_label())
    }

    /// "holes ○●●●●  ·  depth 0.14  ·  lean 0%  ·  otsu".
    pub fn holes_readout(&self) -> String {
        let dots: String = self.holes().iter().map(|h| if *h { '●' } else { '○' }).collect();
        format!(
            "holes {dots}  ·  depth {:.2}  ·  lean {:.0}%  ·  {}",
            self.depth(),
            self.lean * 100.0,
            if self.otsu { "otsu" } else { "kan" }
        )
    }

    /// "blowing  ·  lane kari  ·  loop track 1".
    pub fn play_readout(&self) -> String {
        format!(
            "{}  ·  lane {}  ·  loop track {}",
            if self.blowing { "blowing" } else { "silent" },
            if self.voicing == Voicing::Meri { "meri" } else { "kari" },
            self.loop_track + 1
        )
    }

    /// Header status: "AAudio · … · 6 samples  ·  voices 1  ·  60 fps".
    pub fn status_readout(&self, fps: f32) -> String {
        format!("{}  ·  {}  ·  voices {}  ·  {:.0} fps", self.audio.backend, self.bank_status, self.snap.voices, fps)
    }

    /// Engine module, first line: "48000 Hz · rate 1.000 · cut 676 Hz · sampler".
    pub fn engine_readout(&self) -> String {
        let s = &self.snap;
        format!(
            "{:.0} Hz · rate {:.3} · cut {:.0} Hz · {}",
            s.sample_rate,
            s.rate,
            s.cutoff_hz,
            VoiceMode::from_index(s.voice_mode as usize).label()
        )
    }

    /// Engine module, second line, and whether the latency is worryingly high.
    pub fn clock_readout(&self) -> (String, bool) {
        use std::sync::atomic::Ordering::Relaxed;
        let lat = self.audio.latency_us.load(Relaxed) as f64 / 1000.0;
        let per = self.audio.period.load(Relaxed);
        let xruns = self.audio.xruns.load(Relaxed);
        let lat_txt = if lat > 0.0 { format!("{lat:.0} ms") } else { "n/a".into() };
        let under = if xruns > 0 { format!(" · {xruns} underruns") } else { String::new() };
        (format!("audio clock {:.1} s · latency {lat_txt} · {per} frames/callback{under}", self.snap.now), lat > 150.0)
    }

    /// Delay time under the knob / below the synced delay: "1/4 · 500 ms".
    pub fn delay_readout(&self) -> String {
        use crate::shaku::timing::{DELAY_DIVS, delay_sec_bpm};
        let fx = &self.fx;
        let sec = delay_sec_bpm(fx.delay_sync, fx.delay_time, fx.delay_div, self.bpm());
        let t = if sec >= 1.0 { format!("{sec:.2} s") } else { format!("{} ms", (sec * 1000.0).round()) };
        if fx.delay_sync {
            format!("{} · {t}", DELAY_DIVS[fx.delay_div].id)
        } else {
            format!("{} ms", (sec * 1000.0).round())
        }
    }

    /// LFO rate: "1.20 Hz".
    pub fn lfo_readout(&self) -> String {
        use crate::shaku::timing::{format_hz, lfo_hz_bpm};
        let fx = &self.fx;
        format_hz(lfo_hz_bpm(fx.lfo_sync, fx.mod_rate, fx.lfo_div, self.bpm()))
    }

    /// Loop tempo: "72 bpm".
    pub fn bpm_readout(&self) -> String {
        let bpm = self.bpm();
        if bpm.fract().abs() < 0.005 { format!("{bpm:.0} bpm") } else { format!("{bpm:.2} bpm") }
    }

    /// The tempo synced things follow: the host's, else the clock knob.
    pub fn bpm(&self) -> f64 {
        self.host_bpm.unwrap_or_else(|| crate::shaku::timing::tempo_bpm(self.fx.delay_tempo))
    }

    pub fn is_gated(&self) -> bool {
        self.gated
    }

    // ------------------------------------------------------------ engine

    pub fn send(&mut self, cmd: Cmd) {
        if self.trace {
            let what = match &cmd {
                Cmd::Holes(h) => {
                    format!("holes {:?} note {}", h.map(|c| c as u8), crate::shaku::fingering::index_from_holes(h))
                }
                Cmd::Blow(b) => format!("blow {b}"),
                Cmd::Otsu(o) => format!("otsu {o}"),
                Cmd::Meri(d) => format!("meri depth {d:.3}"),
                Cmd::Atari => "atari".into(),
                Cmd::AtariAfter(s) => format!("atari after {s}s"),
                Cmd::Koro => "koro".into(),
                Cmd::Muraiki => "muraiki".into(),
                Cmd::LoopPunch(t) => format!("loop punch {t}"),
                Cmd::LoopStop(t) => format!("loop stop {t:?}"),
                Cmd::LoopUndo(t) => format!("loop undo {t}"),
                Cmd::Fx(f) => format!("fx {f:?}"),
                _ => String::new(),
            };
            if !what.is_empty() {
                eprintln!("[{:8.3}] {what}", self.started.elapsed().as_secs_f64());
            }
        }
        self.audio.send(cmd);
    }

    /// The React `useEffect([value])` → `engineX(value)` bridge: send whatever changed.
    pub fn sync_engine(&mut self) {
        let now = Sent {
            holes: self.holes(),
            depth: self.depth(),
            lean: self.lean,
            iki: self.iki,
            otsu: self.otsu,
            fx: self.fx.clone(),
        };
        if self.sent.as_ref() == Some(&now) {
            return;
        }
        let prev = self.sent.take();
        let changed = |f: &dyn Fn(&Sent) -> bool| prev.as_ref().is_none_or(f);
        if changed(&|p| p.holes != now.holes) {
            self.send(Cmd::Holes(now.holes));
        }
        if changed(&|p| p.depth != now.depth) {
            self.send(Cmd::Meri(now.depth));
        }
        if changed(&|p| p.lean != now.lean) {
            self.send(Cmd::Lean(now.lean));
        }
        if changed(&|p| p.iki != now.iki) {
            self.send(Cmd::Iki(now.iki));
        }
        if changed(&|p| p.otsu != now.otsu) {
            self.send(Cmd::Otsu(now.otsu));
        }
        fx_diff(prev.as_ref().map(|p| &p.fx), &now.fx, &mut |s| self.send(Cmd::Fx(s)));
        self.sent = Some(now);
    }

    /// Once per UI frame: load samples when ready, pull the engine snapshot,
    /// hand scope samples to `scope`, expire flashes, push changed parameters.
    pub fn tick(&mut self, mut scope: impl FnMut(f32, f32)) {
        if let Ok(s) = self.audio.snapshot.try_lock() {
            self.snap.clone_from(&s);
        }
        while let Ok((l, r)) = self.audio.scope.pop() {
            scope(l, r);
        }
        self.poll();
    }

    /// Ask the engine for a copy of every recorded loop; pick it up with [`Self::take_loop_export`].
    pub fn request_loop_export(&mut self) {
        let slot = self.export_slot.clone();
        self.audio.send(Cmd::ExportLoops(slot));
    }

    /// The loops requested by [`Self::request_loop_export`], once the audio thread has delivered them.
    pub fn take_loop_export(&mut self) -> Option<crate::engine::LoopExport> {
        self.export_slot.try_lock().ok()?.take()
    }

    /// [`Controller::tick`] without the snapshot and scope pulls — what a plugin
    /// calls from its audio thread each block. Doesn't allocate (once the
    /// samples are in).
    pub fn poll(&mut self) {
        self.poll_bank();
        let now = Instant::now();
        if self.ornament.is_some_and(|o| now > o.until) {
            self.ornament = None;
        }
        if self.muraiki_flash.is_some_and(|u| now > u) {
            self.muraiki_flash = None;
        }
        self.sync_engine();
    }

    fn poll_bank(&mut self) {
        let Some(rx) = &self.bank_rx else { return };
        match rx.try_recv() {
            Ok(Ok(bank)) => {
                self.bank_status = format!("{} samples", bank.len());
                self.audio.send(Cmd::Bank(bank));
                self.bank_rx = None;
            }
            Ok(Err(e)) => {
                self.bank_status = format!("sample error: {e}");
                self.bank_rx = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => self.bank_rx = None,
        }
    }

    /// Release the audio device while the app is in the background (and take
    /// it back): stops any held note first so nothing resumes mid-breath.
    pub fn set_audio_paused(&mut self, paused: bool) {
        if paused {
            self.release_keys();
            if self.blowing {
                self.blow(Some(false));
            }
            self.send(Cmd::LoopStop(None));
        }
        self.audio.set_paused(paused);
        self.cb_seen = Instant::now(); // don't report the pause as a stall
    }

    /// A user-facing description of anything wrong with audio output, if so.
    pub fn audio_problem(&mut self) -> Option<String> {
        let n = self.audio.callbacks.load(std::sync::atomic::Ordering::Relaxed);
        if n != self.cb_last {
            self.cb_last = n;
            self.cb_seen = Instant::now();
        }
        let st = self.audio.status.lock().map(|s| s.clone()).unwrap_or_default();
        if self.audio.silent && self.audio.backend.contains("--silent") {
            return None; // silence was asked for
        }
        if self.audio.silent {
            let why = if st.skipped.is_empty() { String::new() } else { format!(": {}", st.skipped.join("; ")) };
            return Some(format!("No sound — {}{why}", self.audio.backend));
        }
        if let Some(e) = st.stream_error {
            return Some(format!("Audio stream error: {e}"));
        }
        let idle = self.cb_seen.elapsed();
        if idle > Duration::from_millis(1500) {
            return Some(format!(
                "Audio stalled: no callbacks from {} for {:.0} s (device held by another program?)",
                self.audio.backend,
                idle.as_secs_f32()
            ));
        }
        None
    }

    // ------------------------------------------------------------ gestures

    pub fn blow(&mut self, on: Option<bool>) {
        let next = on.unwrap_or(!self.blowing);
        self.blowing = next;
        if !next {
            self.gated = false;
            self.latched = false;
        }
        self.sync_engine();
        self.send(Cmd::Blow(next));
    }

    pub fn toggle_gate(&mut self) {
        self.gated = !self.gated;
        self.hold_play = false;
        self.latched = self.gated;
        self.blow(Some(self.gated));
    }

    pub fn park_note(&mut self, index: usize, lane: Voicing) {
        let n = index.min(5);
        match lane {
            Voicing::Meri => self.meri_note = n,
            Voicing::Kari => self.kari_note = n,
        }
        self.voicing = lane;
    }

    fn flash_ornament(&mut self, index: usize, lane: Voicing, koro: bool) {
        let ms = if koro { 340 } else { 160 };
        self.ornament = Some(Ornament { lane, index, koro, until: Instant::now() + Duration::from_millis(ms) });
    }

    fn play_ornament(&mut self, index: usize, lane: Voicing) -> bool {
        if !self.blowing {
            return false;
        }
        let now = Instant::now();
        let cur = lane == self.voicing && index == self.note();
        let same = cur
            && self
                .last_tap
                .is_some_and(|(t, i, l)| i == index && l == lane && now.duration_since(t) < Duration::from_millis(260));
        if same {
            self.sync_engine();
            self.send(Cmd::Koro);
            self.flash_ornament(index, lane, true);
        } else if cur {
            self.sync_engine();
            self.send(Cmd::Atari);
            self.flash_ornament(index, lane, false);
        } else {
            self.park_note(index, lane);
            self.sync_engine();
            self.send(Cmd::AtariAfter(0.028));
            self.flash_ornament(index, lane, false);
        }
        self.last_tap = Some((now, index, lane));
        self.skip_pad_end = self.gated;
        true
    }

    /// Finger/pointer down on a note fader (or a pad when not blowing).
    pub fn on_slide_start(&mut self, index: usize, lane: Voicing) {
        self.park_note(index, lane);
        if self.gated {
            return;
        }
        self.hold_play = true;
        if !self.blowing {
            self.blow(Some(true));
        }
    }

    /// Finger/pointer released from a pad or note fader.
    pub fn on_slide_end(&mut self) {
        if self.skip_pad_end {
            self.skip_pad_end = false;
            return;
        }
        if !self.hold_play {
            return;
        }
        self.hold_play = false;
        if !self.gated {
            self.blow(Some(false));
        }
    }

    /// Pad pressed: atari/koro when already blowing, otherwise start blowing.
    pub fn on_hit_start(&mut self, index: usize, lane: Voicing) {
        if self.play_ornament(index, lane) {
            return;
        }
        self.on_slide_start(index, lane);
    }

    pub fn shoot_muraiki(&mut self) {
        self.send(Cmd::Muraiki);
        self.muraiki_flash = Some(Instant::now() + Duration::from_millis(420));
    }

    pub fn on_punch(&mut self, track: usize) {
        self.loop_track = track;
        self.send(Cmd::LoopPunch(track));
    }
    pub fn on_loop_play(&mut self, track: usize) {
        self.loop_track = track;
        self.send(Cmd::LoopPlay(track));
    }
    pub fn on_loop_stop(&mut self, track: usize) {
        self.loop_track = track;
        self.send(Cmd::LoopStop(Some(track)));
    }
    pub fn on_loop_stop_all(&mut self) {
        self.send(Cmd::LoopStop(None));
    }
    pub fn on_loop_undo(&mut self, track: usize) {
        self.loop_track = track;
        self.send(Cmd::LoopUndo(track));
    }
    pub fn on_freeze(&mut self) {
        self.send(Cmd::LoopFreeze);
        self.loop_track = 3;
    }
    pub fn set_loop_level(&mut self, track: usize, v: f64) {
        self.loop_levels[track] = v;
        self.send(Cmd::LoopLevel(track, v));
    }
    pub fn set_loop_fx(&mut self, track: usize, on: bool) {
        self.send(Cmd::LoopFx(track, on));
    }
    pub fn set_loop_bounds(&mut self, track: usize, a: f64, b: f64) {
        self.send(Cmd::LoopBounds(track, a, b));
    }
    pub fn foli(&mut self, cmd: Cmd) {
        self.send(cmd);
    }
    pub fn set_foli_feel(&mut self, f: FoliFeel) {
        self.foli_feel = f;
        self.send(Cmd::FoliFeel(f));
    }
    pub fn set_foli_kit(&mut self, k: usize) {
        self.foli_kit = k.min(crate::shaku::foli::KIT_COUNT - 1);
        self.send(Cmd::FoliKit(self.foli_kit));
    }
    pub fn set_foli_level(&mut self, v: f64) {
        self.foli_level = v.clamp(0.0, 1.0);
        self.send(Cmd::FoliLevel(self.foli_level));
    }

    pub fn set_voice_mode(&mut self, m: VoiceMode) {
        if self.voice_mode != m {
            self.voice_mode = m;
            self.send(Cmd::VoiceMode(m));
        }
    }

    // ----------------------------------------------------------- keyboard

    /// A key went down (callers filter auto-repeat).
    pub fn key_down(&mut self, k: Key) {
        match k {
            Key::Space => self.toggle_gate(),
            Key::Octave => self.otsu = !self.otsu,
            Key::Muraiki => self.shoot_muraiki(),
            Key::Track(t) => self.loop_track = t.min(TRACK_COUNT - 1),
            Key::Punch => self.on_punch(self.loop_track),
            Key::Stop => self.on_loop_stop(self.loop_track),
            Key::Undo => self.on_loop_undo(self.loop_track),
            Key::NoteUp | Key::NoteDown => {
                let d = if k == Key::NoteUp { 1 } else { -1 };
                match self.voicing {
                    Voicing::Meri => self.meri_note = clamp_note(self.meri_note as i32 + d),
                    Voicing::Kari => self.kari_note = clamp_note(self.kari_note as i32 + d),
                }
            }
            Key::Hit(index, lane) => {
                if !self.held_keys.contains(&k) {
                    self.held_keys.push(k);
                }
                self.on_hit_start(index, lane);
            }
        }
    }

    /// A key came up. Releasing one of several held note keys is legato.
    pub fn key_up(&mut self, k: Key) {
        if !matches!(k, Key::Hit(..)) {
            return;
        }
        self.held_keys.retain(|h| *h != k);
        if self.held_keys.is_empty() {
            self.on_slide_end();
            return;
        }
        if let Some(Key::Hit(i, lane)) = self.held_keys.last().copied() {
            self.park_note(i, lane);
        }
    }

    /// Window lost focus: release every held note key.
    pub fn release_keys(&mut self) {
        if !self.held_keys.is_empty() {
            self.held_keys.clear();
            self.on_slide_end();
        }
    }

    // --------------------------------------------------------------- MIDI

    /// Raw bytes from a MIDI input (notes, bend → meri, CC → iki/punch, sustain).
    pub fn midi_in(&mut self, bytes: &[u8]) {
        let mut evs = std::mem::take(&mut self.midi.evs);
        midi::handle_bytes(bytes, &mut self.midi.sustain, &mut evs);
        for e in evs.drain(..) {
            self.live(e);
        }
        self.midi.evs = evs;
    }

    /// Block until the samples are loaded (a plugin's `initialize`, so offline
    /// renders start with sound). False on timeout or a load error.
    pub fn wait_for_samples(&mut self, timeout: Duration) -> bool {
        let Some(rx) = &self.bank_rx else { return self.bank_status.ends_with(" samples") };
        match rx.recv_timeout(timeout) {
            Ok(Ok(bank)) => {
                self.bank_status = format!("{} samples", bank.len());
                self.audio.send(Cmd::Bank(bank));
                self.bank_rx = None;
                true
            }
            Ok(Err(e)) => {
                self.bank_status = format!("sample error: {e}");
                self.bank_rx = None;
                false
            }
            Err(_) => false,
        }
    }

    /// `MidiLive` callbacks.
    pub fn live(&mut self, ev: LiveEvent) {
        match ev {
            LiveEvent::NoteOn { midi: m, vel } => {
                self.midi.from_hw = true;
                self.midi.held.retain(|n| *n != m);
                self.midi.held.push(m);
                let hit = midi::midi_to_shaku(m);
                self.park_note(hit.index, Voicing::Kari);
                self.otsu = hit.otsu;
                self.iki = midi::vel_to_iki(vel);
                self.blow(Some(true));
            }
            LiveEvent::NoteOff { midi: m } => {
                self.midi.from_hw = true;
                self.midi.held.retain(|n| *n != m);
                if let Some(&last) = self.midi.held.last() {
                    let hit = midi::midi_to_shaku(last);
                    self.park_note(hit.index, Voicing::Kari);
                    self.otsu = hit.otsu;
                } else {
                    self.blow(Some(false));
                }
            }
            LiveEvent::Meri(v) => self.meri = v.clamp(0.0, 1.0),
            LiveEvent::Iki(v) => self.iki = v.clamp(0.0, 1.0),
            LiveEvent::Bend(n) => self.fx.bend = 0.5 + 0.5 * n.clamp(-1.0, 1.0),
            LiveEvent::Yuri(v) => self.fx.yuri = v.clamp(0.0, 1.0),
            LiveEvent::Punch(t) => self.on_punch(t.min(TRACK_COUNT - 1)),
        }
    }

    /// The Tx effect: MIDI bytes to echo the played note to an output,
    /// unless the change came from MIDI itself. Call once per frame.
    pub fn midi_tx(&mut self) -> Vec<[u8; 3]> {
        let mut out = Vec::new();
        let sig = (self.blowing, self.note(), self.otsu, self.iki.to_bits());
        if self.midi.last_sig == Some(sig) {
            self.midi.from_hw = false;
            return out;
        }
        self.midi.last_sig = Some(sig);
        if std::mem::take(&mut self.midi.from_hw) {
            return out;
        }
        let m = midi::shaku_to_midi(self.note(), self.otsu);
        let vel = (28.0 + self.iki * 99.0).round() as u8;
        let (last_m, last_on) = self.midi.last_tx;
        if last_on && (last_m != m || !self.blowing) {
            out.push(midi::note_off_bytes(last_m));
        }
        if self.blowing && (!last_on || last_m != m) {
            out.push(midi::note_on_bytes(m, vel));
        }
        self.midi.last_tx = (m, self.blowing);
        out
    }
}

/// Map every changed `fx` field to its engine setter, in the web effect order.
fn fx_diff(old: Option<&Fx>, n: &Fx, out: &mut impl FnMut(FxSet)) {
    macro_rules! d {
        ($field:ident, $variant:ident) => {
            if old.is_none_or(|o| o.$field != n.$field) {
                out(FxSet::$variant(n.$field));
            }
        };
    }
    d!(delay_time, DelayTime);
    d!(delay_feedback, DelayFeedback);
    d!(delay_mix, DelayMix);
    d!(delay_ping, DelayPing);
    d!(delay_sync, DelaySync);
    d!(delay_div, DelayDiv);
    d!(delay_tempo, DelayTempo);
    d!(reverb, Reverb);
    d!(reverb_decay, ReverbDecay);
    d!(delay_tone, DelayTone);
    d!(delay_lowcut, DelayLowcut);
    d!(reverb_size, ReverbSize);
    d!(reverb_pre, ReverbPre);
    d!(reverb_damp, ReverbDamp);
    d!(phaser, Phaser);
    d!(phaser_depth, PhaserDepth);
    d!(tape, Tape);
    d!(width, Width);
    d!(mod_rate, ModRate);
    d!(lfo_sync, LfoSync);
    d!(lfo_div, LfoDiv);
    d!(lfo_wave, LfoWave);
    d!(lfo_pitch, LfoPitch);
    d!(lfo_filt, LfoFilt);
    d!(lfo_amp, LfoAmp);
    d!(lfo_pitch_amt, LfoPitchAmt);
    d!(lfo_filt_amt, LfoFiltAmt);
    d!(lfo_amp_amt, LfoAmpAmt);
    d!(filt_cut, FiltCut);
    d!(filt_res, FiltRes);
    d!(filt_motion, FiltMotion);
    d!(filt_rate, FiltRate);
    d!(filt_type, FiltType);
    d!(filt_auto, FiltAuto);
    d!(env_a, EnvA);
    d!(env_d, EnvD);
    d!(env_s, EnvS);
    d!(env_r, EnvR);
    d!(env_shape, EnvShape);
    d!(env_loop, EnvLoop);
    d!(fenv_a, FenvA);
    d!(fenv_d, FenvD);
    d!(fenv_s, FenvS);
    d!(fenv_r, FenvR);
    d!(fenv_shape, FenvShape);
    d!(fenv_loop, FenvLoop);
    d!(gain, Gain);
    d!(flute_level, FluteLevel);
    d!(bore_level, BoreLevel);
    d!(bend, Bend);
    d!(bend_range, BendRange);
    d!(yuri, Yuri);
    d!(yuri_rate, YuriRate);
    d!(glide, Glide);
    d!(tune, Tune);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioOptions, start};

    fn ctl() -> Controller {
        Controller::new(start(&AudioOptions { silent: true, ..Default::default() }), VoiceMode::Sampler, || {})
    }

    #[test]
    fn key_names() {
        assert_eq!(Key::from_name("A"), Some(Key::Hit(0, Voicing::Kari)));
        assert_eq!(Key::from_name("n"), Some(Key::Hit(5, Voicing::Meri)));
        assert_eq!(Key::from_name("3"), Some(Key::Track(2)));
        assert_eq!(Key::from_name("space"), Some(Key::Space));
        assert_eq!(Key::from_name("q"), None);
    }

    #[test]
    fn hold_legato_and_ornaments() {
        let mut c = ctl();
        c.key_down(Key::Hit(4, Voicing::Kari));
        assert!(c.blowing);
        assert_eq!(c.note(), 4);
        c.key_down(Key::Hit(5, Voicing::Kari)); // second key while blowing → atari jump
        assert_eq!(c.note(), 5);
        assert!(c.ornament.is_some_and(|o| !o.koro));
        c.key_up(Key::Hit(5, Voicing::Kari)); // legato back to Ri
        assert_eq!(c.note(), 4);
        assert!(c.blowing);
        c.key_up(Key::Hit(4, Voicing::Kari));
        assert!(!c.blowing);
    }

    #[test]
    fn gate_and_midi_tx() {
        let mut c = ctl();
        c.toggle_gate();
        assert!(c.blowing && c.latched);
        let tx = c.midi_tx();
        assert_eq!(tx, vec![midi::note_on_bytes(62, (28.0f64 + 0.52 * 99.0).round() as u8)]);
        c.midi_in(&[0x80, 62, 0]); // note-off from hardware stops the sound, no echo
        assert!(!c.blowing && !c.latched);
        assert!(c.midi_tx().is_empty());
    }
}
