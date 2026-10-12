//! Port of `src/lib/shaku/engine.ts` — the Iki sound kernel.
//!
//! The web version builds a Web Audio node graph and pokes `AudioParam`s from
//! the UI thread. Here the same graph is rendered sample by sample on the
//! audio thread, with the UI sending [`Cmd`]s over a lock-free queue. Every
//! `setTargetAtTime`/ramp/curve call maps onto [`Param`], and every
//! `setTimeout`/`setInterval` becomes a sample-accurate timer.
//!
//! Signal flow (identical to the web graph):
//!
//! ```text
//! sample voice(s) ─┐                                         ┌─ dry ×0.82 → sum ×0.9 ─┬──────────────► L
//!   (rate + LFO)   ├ ×(1+LFO·amp) → hp80 → peak → formant → lp → breath(env) → tape → FILTER ┤                        └ width delay ─► R
//! pink → bp1800 ───┘                                          ├─ phaser ×mix ───────────────────────────► L only (as on the web)
//!                                                             ├─ hp160 → damp → ping-pong delay ×mix ───► L / R
//!                                                             └─ 4 comb reverb ×mix ────────────────────► L + R
//! looper: records `sum`, plays into the compressor (L+R), FX sends feed phaser/delay/reverb.
//! L/R → compressor → out gain → device + analysers
//! ```

use std::sync::{Arc, Mutex};

use crate::dsp::biquad::Biquad;
use crate::dsp::param::Param;
use crate::dsp::waveguide::Waveguide;
use crate::dsp::{Compressor, DelayLine, Osc, Rng, Shaper, Wave, pink_buffer};
use crate::shaku::env_shape::{EnvShape, FiltType, env_curve, env_seconds, filt_cutoff, filt_gain_db, filt_limits};
use crate::shaku::fingering::{DEFAULT_HOLES, Holes, hz_at, index_from_holes};
use crate::shaku::foli::{Foli, FoliFeel};
use crate::shaku::looper::{LoopBankSnap, Looper};
use crate::shaku::samples::{Bank, SAMPLE_BANK};
use crate::shaku::timing::{DELAY_MAX, DIV_QUARTER, delay_sec_bpm, lfo_hz_bpm, log_hz, tempo_bpm};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FiltAuto {
    Off,
    Sweep,
    Env,
}

/// Which sound source feeds the chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceMode {
    /// The shipping web instrument: looped recordings, pitch-shifted.
    Sampler,
    /// The (unused on the web) waveguide AudioWorklet kernel.
    Bore,
    /// Sampler and waveguide layered together.
    Both,
}

impl VoiceMode {
    pub const ALL: [VoiceMode; 3] = [VoiceMode::Sampler, VoiceMode::Bore, VoiceMode::Both];

    /// Does this mode run the waveguide?
    pub fn has_bore(self) -> bool {
        matches!(self, VoiceMode::Bore | VoiceMode::Both)
    }

    /// Does this mode play the sample loops?
    pub fn has_sampler(self) -> bool {
        matches!(self, VoiceMode::Sampler | VoiceMode::Both)
    }

    pub fn index(self) -> usize {
        match self {
            VoiceMode::Sampler => 0,
            VoiceMode::Bore => 1,
            VoiceMode::Both => 2,
        }
    }

    pub fn from_index(i: usize) -> Self {
        Self::ALL[i.min(2)]
    }

    pub fn label(self) -> &'static str {
        ["flute", "bore", "both"][self.index()]
    }
}

/// The web engine's `fx` object.
#[derive(Clone, Debug, PartialEq)]
pub struct Fx {
    pub delay_time: f64,
    pub delay_feedback: f64,
    pub delay_mix: f64,
    pub delay_ping: bool,
    pub delay_sync: bool,
    pub delay_div: usize,
    pub delay_tempo: f64,
    pub reverb: f64,
    pub reverb_decay: f64,
    /// Delay repeats' brightness: 0 → dark (1.2 kHz), 1 → bright (12 kHz).
    pub delay_tone: f64,
    /// Delay low cut: 0 → 40 Hz, 1 → 800 Hz.
    pub delay_lowcut: f64,
    /// Reverb room size: scales the comb lengths ×0.5 … ×2.
    pub reverb_size: f64,
    /// Reverb pre-delay: 0 → 0 ms, 1 → 120 ms.
    pub reverb_pre: f64,
    /// Reverb damping: 0 → bright (9 kHz), 1 → dark (800 Hz).
    pub reverb_damp: f64,
    pub phaser: f64,
    pub phaser_depth: f64,
    pub tape: f64,
    pub width: f64,
    pub mod_rate: f64,
    pub mod_amt: f64,
    pub lfo_sync: bool,
    pub lfo_div: usize,
    pub lfo_wave: Wave,
    pub lfo_pitch: bool,
    pub lfo_filt: bool,
    pub lfo_amp: bool,
    pub lfo_pitch_amt: f64,
    pub lfo_filt_amt: f64,
    pub lfo_amp_amt: f64,
    pub filt_cut: f64,
    pub filt_res: f64,
    pub filt_motion: f64,
    pub filt_rate: f64,
    pub filt_type: FiltType,
    pub filt_auto: FiltAuto,
    pub env_a: f64,
    pub env_d: f64,
    pub env_s: f64,
    pub env_r: f64,
    pub env_shape: EnvShape,
    pub env_loop: bool,
    pub fenv_a: f64,
    pub fenv_d: f64,
    pub fenv_s: f64,
    pub fenv_r: f64,
    pub fenv_shape: EnvShape,
    pub fenv_loop: bool,
    pub gain: f64,
    /// Level of the sampled flute voice (0.8 = unity).
    pub flute_level: f64,
    /// Level of the waveguide bore voice (0.8 = unity).
    pub bore_level: f64,
    /// Pitch bend, 0.5 = centre.
    pub bend: f64,
    /// Bend range: 0 → 1 semitone, 1 → 12 semitones.
    pub bend_range: f64,
    /// Yuri (jaw vibrato) depth.
    pub yuri: f64,
    /// Yuri rate: 0 → 3 Hz, 1 → 8 Hz.
    pub yuri_rate: f64,
    /// Glide time between notes: 0 → 12 ms, 1 → 260 ms.
    pub glide: f64,
    /// Concert pitch: 0.5 = A 440 Hz, ±1 semitone across the knob (A 415 … 466).
    pub tune: f64,
}

impl Default for Fx {
    fn default() -> Self {
        Self {
            delay_time: 0.32,
            delay_feedback: 0.35,
            delay_mix: 0.0,
            delay_ping: false,
            delay_sync: true,
            delay_div: DIV_QUARTER,
            delay_tempo: 0.25,
            reverb: 0.22,
            reverb_decay: 0.48,
            delay_tone: 0.57,
            delay_lowcut: 0.46,
            reverb_size: 0.5,
            reverb_pre: 0.0,
            reverb_damp: 0.30,
            phaser: 0.0,
            phaser_depth: 0.4,
            tape: 0.0,
            width: 0.12,
            mod_rate: 0.7,
            mod_amt: 0.42,
            lfo_sync: true,
            lfo_div: DIV_QUARTER,
            lfo_wave: Wave::Sine,
            lfo_pitch: false,
            lfo_filt: false,
            lfo_amp: false,
            lfo_pitch_amt: 0.48,
            lfo_filt_amt: 0.42,
            lfo_amp_amt: 0.4,
            filt_cut: 0.434,
            filt_res: 0.22,
            filt_motion: 0.72,
            filt_rate: 0.22,
            filt_type: FiltType::Lowpass,
            filt_auto: FiltAuto::Env,
            env_a: 0.16,
            env_d: 0.22,
            env_s: 1.0,
            env_r: 0.28,
            env_shape: EnvShape::Lin,
            env_loop: false,
            fenv_a: 0.12,
            fenv_d: 0.28,
            fenv_s: 0.55,
            fenv_r: 0.32,
            fenv_shape: EnvShape::Exp,
            fenv_loop: false,
            gain: 0.72,
            flute_level: 0.8,
            bore_level: 0.8,
            bend: 0.5,
            bend_range: 0.09,
            yuri: 0.0,
            yuri_rate: 0.4,
            glide: 0.133,
            tune: 0.5,
        }
    }
}

/// Reverb room presets: (name, size, decay, damp, pre-delay). Index 0 is "Custom" (no preset).
pub const REVERB_TYPES: [&str; 5] = ["Custom", "Room", "Hall", "Plate", "Slap"];
const REVERB_VALUES: [[f64; 4]; 4] = [
    [0.22, 0.38, 0.45, 0.02], // Room: small, short, a little dark
    [0.88, 0.84, 0.42, 0.14], // Hall: big, long, pre-delay
    [0.40, 0.72, 0.12, 0.00], // Plate: dense, bright, no pre-delay
    [0.05, 0.20, 0.35, 0.06], // Slap: tiny and quick
];

impl Fx {
    /// Which room preset the reverb knobs currently match (0 = custom).
    pub fn reverb_type(&self) -> usize {
        let cur = [self.reverb_size, self.reverb_decay, self.reverb_damp, self.reverb_pre];
        REVERB_VALUES.iter().position(|v| v.iter().zip(cur).all(|(a, b)| (a - b).abs() < 0.004)).map_or(0, |i| i + 1)
    }

    /// Apply a room preset (0 = custom leaves the knobs as they are).
    pub fn set_reverb_type(&mut self, i: usize) {
        if let Some(v) = i.checked_sub(1).and_then(|k| REVERB_VALUES.get(k)) {
            self.reverb_size = v[0];
            self.reverb_decay = v[1];
            self.reverb_damp = v[2];
            self.reverb_pre = v[3];
        }
    }
}

/// One engine setter call (`setDelayTime`, `setFiltType`, …).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FxSet {
    DelayTime(f64),
    DelayFeedback(f64),
    DelayMix(f64),
    DelayPing(bool),
    DelaySync(bool),
    DelayDiv(usize),
    DelayTempo(f64),
    Reverb(f64),
    ReverbDecay(f64),
    DelayTone(f64),
    DelayLowcut(f64),
    ReverbSize(f64),
    ReverbPre(f64),
    ReverbDamp(f64),
    Phaser(f64),
    PhaserDepth(f64),
    Tape(f64),
    Width(f64),
    ModRate(f64),
    ModAmt(f64),
    LfoSync(bool),
    LfoDiv(usize),
    LfoWave(Wave),
    LfoPitch(bool),
    LfoFilt(bool),
    LfoAmp(bool),
    LfoPitchAmt(f64),
    LfoFiltAmt(f64),
    LfoAmpAmt(f64),
    FiltCut(f64),
    FiltRes(f64),
    FiltMotion(f64),
    FiltRate(f64),
    FiltType(FiltType),
    FiltAuto(FiltAuto),
    EnvA(f64),
    EnvD(f64),
    EnvS(f64),
    EnvR(f64),
    EnvShape(EnvShape),
    EnvLoop(bool),
    FenvA(f64),
    FenvD(f64),
    FenvS(f64),
    FenvR(f64),
    FenvShape(EnvShape),
    FenvLoop(bool),
    Gain(f64),
    FluteLevel(f64),
    BoreLevel(f64),
    Bend(f64),
    BendRange(f64),
    Yuri(f64),
    YuriRate(f64),
    Glide(f64),
    Tune(f64),
}

/// Loop audio handed to the UI for saving.
pub struct LoopExport {
    pub sample_rate: f32,
    pub tracks: Vec<(usize, Vec<f32>)>,
}

/// Mailbox the engine drops an export into.
pub type ExportSlot = Arc<Mutex<Option<LoopExport>>>;

/// UI → audio thread commands.
pub enum Cmd {
    Bank(Bank),
    /// Copy every recorded loop into the mailbox (one allocation, on user request only).
    ExportLoops(ExportSlot),
    Blow(bool),
    Holes(Holes),
    /// Meri *depth* (0 kari … 1 dai-meri), as the web UI passes it.
    Meri(f64),
    Lean(f64),
    Iki(f64),
    Otsu(bool),
    Muraiki,
    Atari,
    /// `setTimeout(fireAtari, ms)` after a note jump.
    AtariAfter(f64),
    Koro,
    Fx(FxSet),
    VoiceMode(VoiceMode),
    LoopPunch(usize),
    LoopPlay(usize),
    LoopStop(Option<usize>),
    LoopUndo(usize),
    LoopClear(Option<usize>),
    LoopFreeze,
    LoopFirePad(bool),
    LoopLevel(usize, f64),
    LoopFx(usize, bool),
    LoopBounds(usize, f64, f64),
    FoliOn(bool),
    FoliFeel(FoliFeel),
    /// Drum kit: 0–9 sampled loops, 10 = synthesised.
    FoliKit(usize),
    FoliLevel(f64),
    FoliToggleStep(usize),
    /// Restore a step pattern (bit per step): steps that differ are toggled.
    FoliMask(u16),
    /// A plugin host's transport, once per block: its tempo replaces the clock
    /// knob (delay/LFO sync, Foli) and `beat` (quarter notes, `None` when
    /// stopped) locks the Foli pattern to its bars.
    HostClock {
        bpm: f64,
        beat: Option<f64>,
    },
}

/// Audio → UI state, published a few hundred times per second.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub now: f64,
    pub sample_rate: f32,
    pub loaded: usize,
    pub blowing: bool,
    pub breath: f32,
    pub filt_env: f32,
    pub rate: f32,
    pub voices: usize,
    pub loop_bank: LoopBankSnap,
    pub peak_l: f32,
    pub peak_r: f32,
    pub comp_gr: f32,
    /// Master limiter gain reduction, dB (≥ 0).
    pub limit_gr: f32,
    /// Momentary loudness of the output (400 ms RMS), dBFS.
    pub loud_db: f32,
    pub phaser_routed: bool,
    pub cutoff_hz: f32,
    pub voice_mode_bore: bool,
    pub voice_mode: u8,
    pub foli_on: bool,
    pub foli_step: Option<usize>,
    pub foli_mask: [bool; 12],
}

pub type SharedSnapshot = Arc<Mutex<Snapshot>>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Which {
    Amp = 0,
    Filt = 1,
}

#[derive(Clone, Copy, Debug)]
enum TimerKind {
    StopIfReleased,
    EnvLoop(Which, u32),
    Atari,
}

#[derive(Clone, Copy, Debug)]
struct Timer {
    at: f64,
    kind: TimerKind,
}

#[derive(Clone, Copy, Debug)]
struct Koro {
    n: u32,
    home: f32,
    lp_home: f32,
    voice: u64,
    next: f64,
}

struct Voice {
    id: u64,
    note: usize,
    pos: f64,
    gain: Param,
    rate: Param,
    rate_mod: bool,
    stop_at: Option<f64>,
    bore: bool,
    sample: bool,
}

impl Voice {
    fn new(sr: f32) -> Self {
        Self {
            id: 0,
            note: 0,
            pos: 0.0,
            gain: Param::new(0.0, sr),
            rate: Param::new(1.0, sr),
            rate_mod: true,
            stop_at: None,
            bore: false,
            sample: true,
        }
    }
}

/// Voices sounding at once (the newest plus fading ones); older ones are cut.
const MAX_VOICES: usize = 12;

const SUB: usize = 16;
/// Master limiter ceiling (about -0.4 dBFS).
const LIMIT_CEIL: f32 = 0.955;
const REVERB_TIMES: [f32; 4] = [0.0299, 0.0371, 0.0433, 0.0531];
const PHASER_BASES: [f32; 4] = [920.0, 1380.0, 1960.0, 2680.0];

pub struct Engine {
    sr: f32,
    frame: u64,
    // --- instrument state ---
    blowing: bool,
    pending_blow: bool,
    yuri_phase: f64,
    blow_t0: f64,
    /// Which recorded meri depth is sounding: 0 kari, 1 chu, 2 dai.
    layer: usize,
    /// Recorded dynamic in use: 0 soft, 1 hard.
    dynamic: usize,
    meri: f64,
    lean: f64,
    iki: f64,
    holes: Holes,
    otsu: bool,
    fx: Fx,
    phaser_routed: bool,
    muraiki_until: f64,
    bank: Option<Bank>,
    voices: Vec<Voice>,
    /// Finished voices kept for reuse, so starting a note never allocates on
    /// the audio thread.
    spare: Vec<Voice>,
    current: Option<u64>,
    next_voice: u64,
    voice_mode: VoiceMode,
    /// Tempo from a plugin host ([`Cmd::HostClock`]); `None` = the clock knob.
    host_bpm: Option<f64>,
    wg: Waveguide,
    timers: Vec<Timer>,
    env_loop_gen: [u32; 2],
    koro: Option<Koro>,
    // --- voice bus ---
    breath: Param,
    noise: Vec<f32>,
    noise_pos: usize,
    noise_bp: Biquad,
    noise_gain: Param,
    hp: Biquad,
    peak: Biquad,
    peak_freq: Param,
    formant: Biquad,
    formant_freq: Param,
    lp: Biquad,
    lp_freq: Param,
    tape_in: Param,
    tape_dry: Param,
    tape_wet: Param,
    shaper: Shaper,
    tape_lp: Biquad,
    // --- filter ---
    filt: Biquad,
    filt_freq: Param,
    filt_q: Param,
    filt_gain: Param,
    filt_lfo: Osc,
    filt_lfo_freq: Param,
    filt_lfo_amt: Param,
    filt_env: Param,
    // --- LFO ---
    lfo: Osc,
    lfo_freq: Param,
    lfo_pitch_amt: Param,
    lfo_filt_amt: Param,
    lfo_amp_amt: Param,
    // --- phaser ---
    phaser: [Biquad; 4],
    phaser_lfo: Osc,
    phaser_depth: Param,
    phaser_mix: Param,
    // --- delay ---
    delay_l: DelayLine,
    delay_r: DelayLine,
    delay_time: Param,
    delay_hp: Biquad,
    delay_damp: Biquad,
    delay_damp_freq: Param,
    delay_hp_freq: Param,
    delay_src_r: Param,
    delay_fb_l: Param,
    delay_fb_r: Param,
    delay_cross_lr: Param,
    delay_cross_rl: Param,
    delay_mix_l: Param,
    delay_mix_r: Param,
    // --- reverb ---
    combs: [DelayLine; 4],
    comb_damp: [Biquad; 4],
    reverb_damp_freq: Param,
    reverb_scale: Param,
    reverb_pre: DelayLine,
    reverb_pre_time: Param,
    reverb_fb: Param,
    reverb_mix: Param,
    // --- out ---
    width: DelayLine,
    width_time: Param,
    comp: Compressor,
    out_gain: Param,
    flute_gain: Param,
    bore_gain: Param,
    looper: Looper,
    foli: Foli,
    // meters
    peak_l: f32,
    limit_g: f32,
    loud_ms: f32,
    peak_r: f32,
    last_cutoff: f32,
    filt_mod: f32,
    phaser_lfo_value: f32,
}

fn lp(sr: f32, f: f32, q: f32) -> Biquad {
    let mut b = Biquad::new(FiltType::Lowpass, sr);
    b.set(f, q, 0.0);
    b
}

fn biq(kind: FiltType, sr: f32, f: f32, q: f32, g: f32) -> Biquad {
    let mut b = Biquad::new(kind, sr);
    b.set(f, q, g);
    b
}

impl Engine {
    /// `boot()`: build the graph with the web defaults, then `applyVoice` + `applyFx`.
    pub fn new(sr: f32) -> Self {
        let mut rng = Rng::new(0x1c1_5eed);
        let noise = pink_buffer(sr, &mut rng);
        let p = |v: f32| Param::new(v, sr);
        let mut e = Self {
            sr,
            frame: 0,
            blowing: false,
            yuri_phase: 0.0,
            blow_t0: 0.0,
            layer: 0,
            dynamic: 0,
            pending_blow: false,
            meri: 0.14,
            lean: 0.0,
            iki: 0.52,
            holes: DEFAULT_HOLES,
            otsu: true,
            fx: Fx::default(),
            phaser_routed: false,
            muraiki_until: 0.0,
            bank: None,
            voices: Vec::with_capacity(MAX_VOICES + 1),
            spare: (0..=MAX_VOICES).map(|_| Voice::new(sr)).collect(),
            current: None,
            next_voice: 1,
            voice_mode: VoiceMode::Sampler,
            host_bpm: None,
            wg: Waveguide::new(sr),
            timers: Vec::with_capacity(32),
            env_loop_gen: [0, 0],
            koro: None,
            breath: p(0.0),
            noise,
            noise_pos: 0,
            noise_bp: biq(FiltType::Bandpass, sr, 1800.0, 0.9, 0.0),
            noise_gain: p(0.0),
            hp: biq(FiltType::Highpass, sr, 80.0, 0.4, 0.0),
            peak: biq(FiltType::Peaking, sr, 820.0, 0.9, 2.2),
            peak_freq: p(820.0),
            formant: biq(FiltType::Peaking, sr, 2150.0, 1.1, 1.6),
            formant_freq: p(2150.0),
            lp: lp(sr, 4200.0, 0.5),
            lp_freq: p(4200.0),
            tape_in: p(1.0),
            tape_dry: p(1.0),
            tape_wet: p(0.0),
            shaper: Shaper::tape(),
            tape_lp: lp(sr, 7200.0, 0.5),
            filt: lp(sr, 3200.0, 0.7),
            filt_freq: p(3200.0),
            filt_q: p(0.7),
            filt_gain: p(0.0),
            filt_lfo: Osc::new(Wave::Triangle),
            filt_lfo_freq: p(0.35),
            filt_lfo_amt: p(0.0),
            filt_env: p(0.0),
            lfo: Osc::new(Wave::Sine),
            lfo_freq: p(0.4),
            lfo_pitch_amt: p(0.0),
            lfo_filt_amt: p(0.0),
            lfo_amp_amt: p(0.0),
            phaser: PHASER_BASES.map(|f| biq(FiltType::Allpass, sr, f, 0.55, 0.0)),
            phaser_lfo: Osc::new(Wave::Sine),
            phaser_depth: p(180.0),
            phaser_mix: p(0.0),
            delay_l: DelayLine::new(DELAY_MAX, sr),
            delay_r: DelayLine::new(DELAY_MAX, sr),
            delay_time: p(0.5),
            delay_hp: biq(FiltType::Highpass, sr, 160.0, 1.0, 0.0),
            delay_damp: lp(sr, 4200.0, 0.5),
            delay_damp_freq: p(4200.0),
            delay_hp_freq: p(160.0),
            delay_src_r: p(1.0),
            delay_fb_l: p(0.0),
            delay_fb_r: p(0.0),
            delay_cross_lr: p(0.0),
            delay_cross_rl: p(0.0),
            delay_mix_l: p(0.0),
            delay_mix_r: p(0.0),
            combs: std::array::from_fn(|_| DelayLine::new(0.2, sr)),
            comb_damp: std::array::from_fn(|_| lp(sr, 4200.0, 1.0)),
            reverb_damp_freq: p(4200.0),
            reverb_scale: p(1.0),
            reverb_pre: DelayLine::new(0.15, sr),
            reverb_pre_time: p(0.0),
            reverb_fb: p(0.0),
            reverb_mix: p(0.0),
            width: DelayLine::new(0.03, sr),
            width_time: p(0.0),
            comp: Compressor::new(sr, -18.0, 10.0, 2.0, 0.01, 0.2),
            out_gain: p(0.85),
            flute_gain: p(1.0),
            bore_gain: p(1.0),
            looper: Looper::new(sr),
            foli: Foli::new(sr),
            peak_l: 0.0,
            limit_g: 1.0,
            loud_ms: 0.0,
            peak_r: 0.0,
            last_cutoff: 3200.0,
            filt_mod: 0.0,
            phaser_lfo_value: 0.0,
        };
        e.apply_voice(0.0);
        e.apply_fx(0.0);
        e
    }

    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    /// The tempo everything synced follows: the host's, else the clock knob.
    pub fn bpm(&self) -> f64 {
        self.host_bpm.unwrap_or_else(|| tempo_bpm(self.fx.delay_tempo))
    }

    #[inline]
    fn now(&self) -> f64 {
        self.frame as f64 / self.sr as f64
    }

    fn note_index(&self) -> usize {
        index_from_holes(&self.holes)
    }

    /// Index into the sample bank for the current fingering and meri depth. The
    /// layer has a little hysteresis so a wobbling meri doesn't flip samples.
    fn sample_index(&mut self) -> usize {
        let d = self.meri;
        self.layer = if !self.voice_mode.has_sampler() {
            0
        } else {
            match self.layer {
                0 if d > 0.33 => 1,
                1 if d < 0.22 => 0,
                1 if d > 0.83 => 2,
                2 if d < 0.72 => 1,
                l => l,
            }
        };
        self.dynamic * 18 + self.layer * 6 + self.note_index().min(5)
    }

    /// Current pitch bend in semitones.
    fn bend_semitones(&self) -> f64 {
        (self.fx.bend - 0.5) * 2.0 * (1.0 + 11.0 * self.fx.bend_range)
    }

    fn pitch_hz(&self) -> f64 {
        hz_at(self.note_index(), self.otsu, self.meri)
            * ((self.bend_semitones() + (self.fx.tune - 0.5) * 2.0) / 12.0).exp2()
    }

    /// Glide time constant in seconds.
    fn glide_tau(&self) -> f32 {
        (0.012 + 0.248 * self.fx.glide) as f32
    }

    /// Move the sounding voice's pitch to the current target with time constant `tau`.
    fn retarget_pitch(&mut self, at: f64, tau: f32) {
        let t = at.max(self.now());
        if let Some(note) = self.current_voice().map(|v| v.note) {
            let r = self.play_rate(note);
            if let Some(v) = self.current_voice() {
                v.rate.set_target_at(r, t, tau);
            }
        }
    }

    fn play_rate(&self, note: usize) -> f32 {
        let native = SAMPLE_BANK.get(note).map(|s| s.freq).unwrap_or(293.66);
        let r = self.pitch_hz() / native;
        if r.is_finite() && r > 0.25 && r < 4.0 { r as f32 } else { 1.0 }
    }

    fn current_voice(&mut self) -> Option<&mut Voice> {
        let id = self.current?;
        self.voices.iter_mut().find(|v| v.id == id)
    }

    // ------------------------------------------------------------------ voice

    fn stop_voice(&mut self, immediate: bool) {
        let now = self.now();
        let Some(v) = self.current_voice() else {
            self.current = None;
            return;
        };
        v.rate_mod = false;
        v.gain.cancel(now);
        let cur = v.gain.value().max(0.0001);
        v.gain.set_value_at(cur, now);
        if immediate {
            v.gain.set_value_at(0.0, now);
            v.stop_at = Some(now);
        } else {
            v.gain.linear_ramp_to(0.0001, now + 0.08);
            v.stop_at = Some(now + 0.1);
        }
        self.current = None;
    }

    fn start_voice(&mut self, attack: bool) {
        let Some(bank) = self.bank.clone() else { return };
        // Breath picks the recorded dynamic for each new note (with hysteresis).
        self.dynamic = match self.dynamic {
            0 if self.iki > 0.6 => 1,
            1 if self.iki < 0.45 => 0,
            d => d,
        };
        let note = self.sample_index();
        let Some(prep) = bank.get(note) else { return };
        let now = self.now();
        let bore = self.voice_mode.has_bore();
        let sample = self.voice_mode.has_sampler();
        if let Some(v) = self.current_voice()
            && v.bore == bore
            && v.sample == sample
            && (v.note == note || (v.bore && !v.sample))
            && !attack
        {
            v.note = note;
            self.apply_voice(now);
            return;
        }
        // Legato: remember where the pitch was so the new note can glide in from it.
        let prev_hz = if attack {
            None
        } else {
            self.current_voice()
                .map(|v| SAMPLE_BANK.get(v.note).map(|s| s.freq).unwrap_or(293.66) * v.rate.value() as f64)
        };
        self.stop_voice(false);
        // Keep the realtime path bounded: recycle the oldest fading voices.
        while self.voices.len() >= MAX_VOICES {
            let old = self.voices.remove(0);
            self.spare.push(old);
        }
        let rate = self.play_rate(note);
        let id = self.next_voice;
        self.next_voice += 1;
        let mut v = self.spare.pop().unwrap_or_else(|| Voice::new(self.sr));
        v.id = id;
        v.note = note;
        v.pos = if attack { 0.0 } else { prep.loop_start };
        v.gain.reset(0.0001, now);
        v.gain.linear_ramp_to(1.0, now + if attack { 0.045 } else { 0.03 });
        let native = SAMPLE_BANK.get(note).map(|s| s.freq).unwrap_or(293.66);
        let from = prev_hz.map(|h| (h / native) as f32).filter(|r| r.is_finite() && *r > 0.25 && *r < 4.0);
        match from {
            Some(r0) if (r0 / rate - 1.0).abs() > 0.002 => {
                v.rate.reset(r0, now);
                v.rate.set_target_at(rate, now, self.glide_tau());
            }
            _ => v.rate.reset(rate, now),
        }
        v.rate_mod = true;
        v.stop_at = None;
        v.bore = bore;
        v.sample = sample;
        self.voices.push(v);
        self.current = Some(id);
    }

    fn apply_voice(&mut self, at: f64) {
        let t = at.max(self.now());
        let meri_dark = (self.meri + self.lean * 0.7).min(1.0) as f32;
        let iki = self.iki as f32;
        let otsu = self.otsu;
        if t >= self.muraiki_until {
            self.noise_gain.set_target_at(0.0012, t, 0.08);
        }
        let cut = (if otsu { 2400.0 } else { 4600.0 }) - meri_dark * 1600.0 + iki * 900.0;
        self.lp_freq.set_target_at(cut.max(if otsu { 900.0 } else { 1400.0 }), t, 0.07);
        self.peak_freq.set_target_at((if otsu { 480.0 } else { 780.0 }) - meri_dark * 160.0, t, 0.07);
        self.formant_freq.set_target_at(
            (if otsu { 1500.0 } else { 2150.0 }) - meri_dark * 280.0 + iki * 180.0,
            t,
            0.08,
        );
        // A deeper meri may need a different recording: crossfade to it.
        if self.blowing && self.voice_mode.has_sampler() && self.bank.is_some() {
            let want = self.sample_index();
            if self.current_voice().is_some_and(|v| v.note != want && v.sample) {
                self.start_voice(false);
                return;
            }
        }
        let tau = self.glide_tau();
        self.retarget_pitch(t, tau);
    }

    // ------------------------------------------------------------ envelopes

    fn stop_env_loop(&mut self, which: Option<Which>) {
        for w in [Which::Amp, Which::Filt] {
            if which.is_none() || which == Some(w) {
                self.env_loop_gen[w as usize] += 1;
                self.timers.retain(|t| !matches!(t.kind, TimerKind::EnvLoop(x, _) if x == w));
            }
        }
    }

    fn filter_env_peak(&self) -> f64 {
        // Only the "Env" mode lets the filter envelope open the filter; Off and
        // Sweep must leave the cutoff exactly where the knob says.
        if self.fx.filt_auto != FiltAuto::Env {
            return 0.0;
        }
        let cut = self.cutoff_hz();
        let motion = self.fx.filt_motion;
        let mut amt = (motion * (cut - 80.0)).min((cut - 90.0).max(0.0));
        let t = self.fx.filt_type;
        if t == FiltType::Highpass {
            amt = (motion * (cut - 30.0)).min((cut - 36.0).max(0.0));
        }
        if t == FiltType::Notch || t == FiltType::Allpass {
            amt *= 0.4;
        }
        if t == FiltType::Peaking || t == FiltType::Bandpass {
            amt *= 0.65;
        }
        amt.max(0.0)
    }

    fn cutoff_hz(&self) -> f64 {
        filt_cutoff(self.fx.filt_type, self.fx.filt_cut)
    }

    /// `fireEnv`: returns the curve duration (on) or the release time (off).
    fn fire_env(&mut self, which: Which, on: bool) -> f64 {
        self.stop_env_loop(Some(which));
        let fx = &self.fx;
        let times = match which {
            Which::Amp => env_seconds(fx.env_a, fx.env_d, fx.env_s, fx.env_r),
            Which::Filt => env_seconds(fx.fenv_a, fx.fenv_d, fx.fenv_s, fx.fenv_r),
        };
        let shape = if which == Which::Amp { fx.env_shape } else { fx.fenv_shape };
        let lp = on && if which == Which::Amp { fx.env_loop } else { fx.fenv_loop };
        let peak = if which == Which::Amp { 1.0 } else { self.filter_env_peak() };
        let now = self.now();
        let param = if which == Which::Amp { &mut self.breath } else { &mut self.filt_env };
        let raw = param.value();
        let cur = if raw.is_finite() { raw.max(0.0001) } else { 0.0001 };
        param.cancel(now);
        param.set_value_at(cur, now);
        if !on {
            param.linear_ramp_to(0.0001, now + times.r.max(0.03));
            return times.r;
        }
        let (curve, dur) = env_curve(times, shape, lp);
        let mut scaled = [0f32; crate::shaku::env_shape::ENV_CURVE_N];
        let mut ok = dur.is_finite() && dur > 0.01;
        let pk = peak.max(0.0001) as f32;
        for (s, c) in scaled.iter_mut().zip(curve.iter()) {
            let v = c * pk;
            *s = if v.is_finite() { v.max(0.0001) } else { 0.0001 };
            ok &= s.is_finite();
        }
        if ok {
            param.set_value_curve_at(&scaled, now + 0.003, dur.max(0.04));
        } else {
            param.linear_ramp_to(pk, now + times.a);
            param.linear_ramp_to((peak * times.s).max(0.0001) as f32, now + times.a + times.d);
        }
        if lp {
            self.env_loop_gen[which as usize] += 1;
            let gen_ = self.env_loop_gen[which as usize];
            let wait = (dur * 1000.0 - 16.0).max(48.0) / 1000.0;
            self.timers.push(Timer { at: now + wait, kind: TimerKind::EnvLoop(which, gen_) });
        }
        dur
    }

    fn steer(param: &mut Param, t: f64, value: f32, tau: f32) {
        let cur = if param.value().is_finite() { param.value() } else { value };
        param.cancel(t);
        param.set_value_at(cur, t);
        param.set_target_at(value, t, tau);
    }

    // --------------------------------------------------------------- filter

    fn apply_filt(&mut self, at: f64) {
        let t = at.max(self.now());
        let cut = self.cutoff_hz().clamp(25.0, (self.sr * 0.42).min(18000.0) as f64);
        let ty = self.fx.filt_type;
        let lim = filt_limits(ty);
        let q = (0.35 + self.fx.filt_res * 11.5).clamp(lim.q[0], lim.q[1]) as f32;
        let g_db = filt_gain_db(self.fx.filt_res).clamp(lim.gain[0], lim.gain[1]) as f32;
        let env_amt = self.filter_env_peak();
        let base = (cut - env_amt).max(40.0);
        self.filt_q.cancel(t);
        self.filt_gain.cancel(t);
        let qv = self.filt_q.value();
        let qv = if qv.is_finite() && qv != 0.0 { qv } else { 1.0 };
        self.filt_q.set_value_at(qv.clamp(0.3, 1.05), t);
        self.filt_gain.set_value_at(0.0, t);
        self.filt.set_kind(ty);
        self.filt_freq.set_target_at(base as f32, t, 0.04);
        self.filt_q.set_target_at(q, t, 0.05);
        self.filt_gain.set_target_at(g_db, t, 0.05);

        let headroom = (base - 50.0).max(40.0);
        let mut sweep = if self.fx.filt_auto == FiltAuto::Sweep {
            (self.fx.filt_motion * cut * 0.45).min(headroom * 0.45)
        } else {
            0.0
        };
        let mut lfo = if self.fx.lfo_filt { (self.fx.lfo_filt_amt * 900.0).min(headroom * 0.45) } else { 0.0 };
        if sweep + lfo > headroom {
            let s = headroom / (sweep + lfo);
            sweep *= s;
            lfo *= s;
        }
        self.filt_lfo_freq.set_target_at(log_hz(self.fx.filt_rate, 0.03, 8.0) as f32, t, 0.06);
        self.filt_lfo_amt.set_target_at(sweep as f32, t, 0.05);
        self.lfo_filt_amt.set_target_at(lfo as f32, t, 0.05);
    }

    fn apply_fx(&mut self, at: f64) {
        let t = at.max(self.now());
        let fx = self.fx.clone();
        let bpm = self.bpm();
        let ping = if fx.delay_ping { 1.0 } else { 0.0 };
        let delay_fb = (fx.delay_feedback * 0.78) as f32;
        let time = delay_sec_bpm(fx.delay_sync, fx.delay_time, fx.delay_div, bpm) as f32;
        self.delay_time.set_target_at(time, t, 0.03);
        self.delay_src_r.set_target_at(1.0 - ping, t, 0.04);
        self.delay_fb_l.set_target_at(delay_fb * (1.0 - ping), t, 0.04);
        self.delay_fb_r.set_target_at(delay_fb * (1.0 - ping), t, 0.04);
        self.delay_cross_lr.set_target_at(ping, t, 0.04);
        self.delay_cross_rl.set_target_at(delay_fb * ping, t, 0.04);
        let mix = (fx.delay_mix * 0.62) as f32;
        self.delay_mix_l.set_target_at(mix, t, 0.04);
        self.delay_mix_r.set_target_at(mix, t, 0.04);
        self.delay_damp_freq.set_target_at((1200.0 * 10f64.powf(fx.delay_tone)) as f32, t, 0.08);
        self.delay_hp_freq.set_target_at((40.0 * 20f64.powf(fx.delay_lowcut)) as f32, t, 0.08);

        self.reverb_mix.set_target_at((fx.reverb * 0.42) as f32, t, 0.06);
        self.reverb_fb.set_target_at((0.35 + fx.reverb_decay * 0.5) as f32, t, 0.08);
        self.reverb_damp_freq.set_target_at((9000.0 * (800.0f64 / 9000.0).powf(fx.reverb_damp)) as f32, t, 0.1);
        self.reverb_scale.set_target_at((0.5 * 4f64.powf(fx.reverb_size)) as f32, t, 0.25);
        self.reverb_pre_time.set_target_at((fx.reverb_pre * 0.12) as f32, t, 0.1);

        self.phaser_mix.set_target_at((fx.phaser * 0.55) as f32, t, 0.05);
        self.phaser_depth.set_target_at((40.0 + fx.phaser_depth * 240.0) as f32, t, 0.08);
        self.phaser_routed = fx.phaser > 0.01;

        self.tape_in.set_target_at((1.0 + fx.tape * 3.4) as f32, t, 0.05);
        self.tape_wet.set_target_at((fx.tape * 0.62) as f32, t, 0.05);
        self.tape_dry.set_target_at((1.0 - fx.tape * 0.38) as f32, t, 0.05);

        self.width_time.set_target_at((fx.width * 0.014) as f32, t, 0.04);

        self.lfo.wave = fx.lfo_wave;
        let rate = lfo_hz_bpm(fx.lfo_sync, fx.mod_rate, fx.lfo_div, bpm) as f32;
        self.lfo_freq.set_target_at(rate, t, 0.06);
        self.lfo_pitch_amt.set_target_at(if fx.lfo_pitch { (fx.lfo_pitch_amt * 0.058) as f32 } else { 0.0 }, t, 0.05);
        self.lfo_amp_amt.set_target_at(if fx.lfo_amp { (fx.lfo_amp_amt * 0.48) as f32 } else { 0.0 }, t, 0.05);
        let gain = (0.15 + fx.gain * fx.gain * 1.15) as f32;
        self.out_gain.set_target_at(gain, t, 0.03);
        self.flute_gain.set_target_at((fx.flute_level * 1.25) as f32, t, 0.03);
        self.bore_gain.set_target_at((fx.bore_level * 1.25) as f32, t, 0.03);
        self.apply_filt(t);
    }

    // ---------------------------------------------------------- performance

    fn run_blow(&mut self, on: bool) {
        if self.bank.is_none() {
            // `await loadBuffers()` — resume once samples arrive.
            self.pending_blow = true;
            return;
        }
        let now = self.now();
        if on {
            self.wg.blowing = 1.0;
            self.start_voice(true);
            self.apply_voice(now);
            self.apply_filt(now);
            self.fire_env(Which::Amp, true);
            self.fire_env(Which::Filt, true);
        } else {
            self.wg.blowing = 0.0;
            let rel = self.fire_env(Which::Amp, false);
            self.fire_env(Which::Filt, false);
            let wait = ((rel * 1000.0).round() + 40.0) / 1000.0;
            self.timers.push(Timer { at: now + wait, kind: TimerKind::StopIfReleased });
        }
    }

    fn fire_muraiki(&mut self) {
        let now = self.now();
        self.muraiki_until = now + 0.62;
        let ng = &mut self.noise_gain;
        let ncur = if ng.value().is_finite() { ng.value().max(0.0001) } else { 0.001 };
        ng.cancel(now);
        ng.set_value_at(ncur, now);
        ng.linear_ramp_to(0.46, now + 0.035);
        ng.linear_ramp_to(0.2, now + 0.16);
        ng.linear_ramp_to(0.001, now + 0.55);
        let Some(note) = self.current_voice().map(|v| v.note) else { return };
        let rate = self.play_rate(note);
        let Some(v) = self.current_voice() else { return };
        let cur = if v.gain.value().is_finite() { v.gain.value().max(0.15) } else { 1.0 };
        v.gain.cancel(now);
        v.gain.set_value_at(cur, now);
        v.gain.linear_ramp_to(0.28, now + 0.05);
        v.gain.linear_ramp_to(1.0, now + 0.42);
        v.rate.cancel(now);
        v.rate.set_value_at(rate, now);
        v.rate.linear_ramp_to(rate * 0.985, now + 0.06);
        v.rate.linear_ramp_to(rate, now + 0.28);
    }

    fn fire_atari(&mut self) {
        self.koro = None;
        let now = self.now();
        let air = 0.0012;
        let ng = &mut self.noise_gain;
        let ncur = if ng.value().is_finite() { ng.value() } else { air };
        ng.cancel(now);
        ng.set_value_at(ncur, now);
        ng.linear_ramp_to(0.16, now + 0.012);
        ng.linear_ramp_to(air, now + 0.1);
        let Some(note) = self.current_voice().map(|v| v.note) else { return };
        let rate = self.play_rate(note);
        let Some(v) = self.current_voice() else { return };
        v.rate.cancel(now);
        v.rate.set_value_at(rate, now);
        v.rate.linear_ramp_to(rate * 1.032, now + 0.016);
        v.rate.linear_ramp_to(rate, now + 0.072);
        let cur = if v.gain.value().is_finite() { v.gain.value().max(0.2) } else { 1.0 };
        v.gain.cancel(now);
        v.gain.set_value_at(cur, now);
        v.gain.linear_ramp_to(0.26, now + 0.01);
        v.gain.linear_ramp_to(1.06, now + 0.048);
        v.gain.linear_ramp_to(1.0, now + 0.1);
    }

    fn fire_koro(&mut self) {
        let Some((id, note)) = self.current_voice().map(|v| (v.id, v.note)) else {
            self.fire_atari();
            return;
        };
        let home = self.play_rate(note);
        let lp_home = if self.lp_freq.value().is_finite() { self.lp_freq.value() } else { 3200.0 };
        self.koro = Some(Koro { n: 0, home, lp_home, voice: id, next: self.now() + 0.038 });
    }

    fn koro_step(&mut self) {
        let Some(mut k) = self.koro else { return };
        let t = self.now();
        if t < k.next {
            return;
        }
        k.n += 1;
        k.next += 0.038;
        let open = k.n % 2 == 1;
        let done = k.n >= 8;
        if let Some(v) = self.voices.iter_mut().find(|v| v.id == k.voice) {
            if done {
                v.rate.set_target_at(k.home, t, 0.03);
            } else {
                v.rate.set_value_at(if open { k.home * 0.944 } else { k.home }, t);
            }
        }
        if done {
            self.lp_freq.set_target_at(k.lp_home, t, 0.04);
            self.koro = None;
        } else {
            self.lp_freq.set_value_at(if open { k.lp_home * 0.7 } else { k.lp_home }, t);
            self.koro = Some(k);
        }
    }

    // -------------------------------------------------------------- commands

    pub fn handle(&mut self, cmd: Cmd) {
        let now = self.now();
        self.looper.set_now(now);
        match cmd {
            Cmd::Bank(b) => {
                self.bank = Some(b);
                if self.pending_blow {
                    self.pending_blow = false;
                    let on = self.blowing;
                    self.run_blow(on);
                }
            }
            Cmd::ExportLoops(slot) => {
                if let Ok(mut g) = slot.try_lock() {
                    *g = Some(LoopExport { sample_rate: self.sr, tracks: self.looper.export() });
                }
            }
            Cmd::Blow(on) => {
                if on && !self.blowing {
                    self.blow_t0 = now;
                }
                self.blowing = on;
                self.run_blow(on);
            }
            Cmd::Holes(h) => {
                self.holes = h;
                if self.blowing {
                    self.start_voice(false);
                } else {
                    self.apply_voice(now);
                }
            }
            Cmd::Meri(v) => {
                self.meri = v.clamp(0.0, 1.0);
                self.apply_voice(now);
            }
            Cmd::Lean(v) => {
                self.lean = v.clamp(0.0, 1.0);
                self.apply_voice(now);
            }
            Cmd::Iki(v) => {
                self.iki = v.clamp(0.0, 1.0);
                self.apply_voice(now);
            }
            Cmd::Otsu(on) => {
                if self.otsu != on {
                    self.otsu = on;
                    self.apply_voice(now);
                }
            }
            Cmd::Muraiki => self.fire_muraiki(),
            Cmd::Atari => self.fire_atari(),
            Cmd::AtariAfter(sec) => self.timers.push(Timer { at: now + sec, kind: TimerKind::Atari }),
            Cmd::Koro => self.fire_koro(),
            Cmd::Fx(s) => self.set_fx(s),
            Cmd::VoiceMode(m) => {
                if self.voice_mode != m {
                    self.voice_mode = m;
                    self.stop_voice(true);
                    if self.blowing {
                        self.start_voice(true);
                    }
                }
            }
            Cmd::LoopPunch(t) => self.looper.punch(t),
            Cmd::LoopPlay(t) => self.looper.play(t),
            Cmd::LoopStop(t) => self.looper.stop(t),
            Cmd::LoopUndo(t) => self.looper.undo(t),
            Cmd::LoopClear(t) => self.looper.clear(t),
            Cmd::LoopFreeze => self.looper.freeze_pad(),
            Cmd::LoopFirePad(on) => self.looper.fire_pad(on),
            Cmd::LoopLevel(t, v) => self.looper.set_level(t, v),
            Cmd::LoopFx(t, on) => self.looper.set_fx(t, on),
            Cmd::LoopBounds(t, a, b) => self.looper.set_bounds(t, a, b),
            Cmd::FoliOn(on) => self.foli.set_on(on, now),
            Cmd::FoliFeel(f) => self.foli.set_feel(f),
            Cmd::FoliKit(k) => self.foli.set_kit(k),
            Cmd::FoliLevel(v) => self.foli.level = v.clamp(0.0, 1.0),
            Cmd::FoliToggleStep(i) => self.foli.toggle_step(i),
            Cmd::FoliMask(m) => {
                let now = self.foli.mask();
                for (i, on) in now.iter().enumerate() {
                    if *on != (m & (1 << i) != 0) {
                        self.foli.toggle_step(i);
                    }
                }
            }
            Cmd::HostClock { bpm, beat } => {
                let bpm = bpm.clamp(20.0, 999.0);
                if self.host_bpm != Some(bpm) {
                    self.host_bpm = Some(bpm);
                    self.looper.set_clock(bpm);
                    self.foli.bpm = bpm;
                    self.apply_fx(now);
                }
                self.foli.follow(beat, now);
            }
        }
    }

    fn set_fx(&mut self, s: FxSet) {
        let c = |v: f64| v.clamp(0.0, 1.0);
        let now = self.now();
        let fx = &mut self.fx;
        match s {
            FxSet::DelayTime(v) => fx.delay_time = c(v),
            FxSet::DelayFeedback(v) => fx.delay_feedback = c(v),
            FxSet::DelayMix(v) => fx.delay_mix = c(v),
            FxSet::DelayPing(on) => fx.delay_ping = on,
            FxSet::DelaySync(on) => fx.delay_sync = on,
            FxSet::DelayDiv(d) => fx.delay_div = if d < 9 { d } else { DIV_QUARTER },
            FxSet::DelayTempo(v) => {
                fx.delay_tempo = c(v);
                if self.host_bpm.is_none() {
                    self.looper.set_clock(tempo_bpm(c(v)));
                    self.foli.bpm = tempo_bpm(c(v));
                }
            }
            FxSet::Reverb(v) => fx.reverb = c(v),
            FxSet::ReverbDecay(v) => fx.reverb_decay = c(v),
            FxSet::DelayTone(v) => fx.delay_tone = c(v),
            FxSet::DelayLowcut(v) => fx.delay_lowcut = c(v),
            FxSet::ReverbSize(v) => fx.reverb_size = c(v),
            FxSet::ReverbPre(v) => fx.reverb_pre = c(v),
            FxSet::ReverbDamp(v) => fx.reverb_damp = c(v),
            FxSet::Phaser(v) => fx.phaser = c(v),
            FxSet::PhaserDepth(v) => fx.phaser_depth = c(v),
            FxSet::Tape(v) => fx.tape = c(v),
            FxSet::Width(v) => fx.width = c(v),
            FxSet::ModRate(v) => fx.mod_rate = c(v),
            FxSet::ModAmt(v) => fx.mod_amt = c(v),
            FxSet::LfoSync(on) => fx.lfo_sync = on,
            FxSet::LfoDiv(d) => fx.lfo_div = if d < 9 { d } else { DIV_QUARTER },
            FxSet::LfoWave(w) => fx.lfo_wave = w,
            FxSet::LfoPitch(on) => fx.lfo_pitch = on,
            FxSet::LfoFilt(on) => fx.lfo_filt = on,
            FxSet::LfoAmp(on) => fx.lfo_amp = on,
            FxSet::LfoPitchAmt(v) => fx.lfo_pitch_amt = c(v),
            FxSet::LfoFiltAmt(v) => fx.lfo_filt_amt = c(v),
            FxSet::LfoAmpAmt(v) => fx.lfo_amp_amt = c(v),
            FxSet::FiltCut(v) => {
                fx.filt_cut = c(v);
                return self.apply_filt(now);
            }
            FxSet::FiltRes(v) => {
                fx.filt_res = c(v);
                return self.apply_filt(now);
            }
            FxSet::FiltMotion(v) => {
                fx.filt_motion = c(v);
                return self.apply_filt(now);
            }
            FxSet::FiltRate(v) => {
                fx.filt_rate = c(v);
                return self.apply_filt(now);
            }
            FxSet::FiltType(t) => {
                fx.filt_type = t;
                return self.apply_filt(now);
            }
            FxSet::FiltAuto(a) => {
                fx.filt_auto = a;
                self.apply_filt(now);
                if self.blowing {
                    // Env: (re)start the filter envelope. Off/Sweep: release it
                    // so it stops pushing the cutoff.
                    let on = self.fx.filt_auto == FiltAuto::Env;
                    self.fire_env(Which::Filt, on);
                } else {
                    self.stop_env_loop(Some(Which::Filt));
                }
                return;
            }
            FxSet::EnvA(v) => return fx.env_a = c(v),
            FxSet::EnvD(v) => return fx.env_d = c(v),
            FxSet::EnvS(v) => {
                fx.env_s = c(v);
                if self.blowing && !self.fx.env_loop {
                    let s = (self.fx.env_s as f32).max(0.0001);
                    Self::steer(&mut self.breath, now, s, 0.05);
                }
                return;
            }
            FxSet::EnvR(v) => return fx.env_r = c(v),
            FxSet::EnvShape(s) => {
                fx.env_shape = s;
                if self.blowing {
                    self.fire_env(Which::Amp, true);
                }
                return;
            }
            FxSet::EnvLoop(on) => {
                fx.env_loop = on;
                if self.blowing && on {
                    self.fire_env(Which::Amp, true);
                } else {
                    self.stop_env_loop(Some(Which::Amp));
                    if self.blowing {
                        // Loop switched off mid-note: settle on the sustain level.
                        let s = (self.fx.env_s as f32).max(0.0001);
                        Self::steer(&mut self.breath, now, s, 0.05);
                    }
                }
                return;
            }
            FxSet::FenvA(v) => return fx.fenv_a = c(v),
            FxSet::FenvD(v) => return fx.fenv_d = c(v),
            FxSet::FenvS(v) => {
                fx.fenv_s = c(v);
                if self.blowing && self.fx.filt_auto == FiltAuto::Env && !self.fx.fenv_loop {
                    let s = (self.filter_env_peak() * self.fx.fenv_s).max(0.0001) as f32;
                    Self::steer(&mut self.filt_env, now, s, 0.05);
                }
                return;
            }
            FxSet::FenvR(v) => return fx.fenv_r = c(v),
            FxSet::FenvShape(s) => {
                fx.fenv_shape = s;
                if self.blowing && self.fx.filt_auto == FiltAuto::Env {
                    self.fire_env(Which::Filt, true);
                }
                return;
            }
            FxSet::FenvLoop(on) => {
                fx.fenv_loop = on;
                if self.blowing && on && self.fx.filt_auto == FiltAuto::Env {
                    self.fire_env(Which::Filt, true);
                } else {
                    self.stop_env_loop(Some(Which::Filt));
                    if self.blowing && self.fx.filt_auto == FiltAuto::Env {
                        let s = (self.filter_env_peak() * self.fx.fenv_s).max(0.0001) as f32;
                        Self::steer(&mut self.filt_env, now, s, 0.05);
                    }
                }
                return;
            }
            FxSet::Gain(v) => fx.gain = c(v),
            FxSet::FluteLevel(v) => fx.flute_level = c(v),
            FxSet::BoreLevel(v) => fx.bore_level = c(v),
            FxSet::Bend(v) => {
                fx.bend = c(v);
                return self.retarget_pitch(now, 0.018);
            }
            FxSet::BendRange(v) => {
                fx.bend_range = c(v);
                return self.retarget_pitch(now, 0.03);
            }
            FxSet::Yuri(v) => fx.yuri = c(v),
            FxSet::YuriRate(v) => fx.yuri_rate = c(v),
            FxSet::Glide(v) => fx.glide = c(v),
            FxSet::Tune(v) => {
                fx.tune = c(v);
                return self.retarget_pitch(now, 0.03);
            }
        }
        self.apply_fx(now);
    }

    fn run_timers(&mut self) {
        let now = self.now();
        self.koro_step();
        let mut i = 0;
        while i < self.timers.len() {
            if self.timers[i].at > now {
                i += 1;
                continue;
            }
            let t = self.timers.swap_remove(i);
            match t.kind {
                TimerKind::StopIfReleased => {
                    if !self.blowing {
                        self.stop_voice(false);
                    }
                }
                TimerKind::EnvLoop(which, gen_) => {
                    if self.env_loop_gen[which as usize] == gen_ && self.blowing {
                        self.fire_env(which, true);
                    }
                }
                TimerKind::Atari => self.fire_atari(),
            }
        }
        // Reap finished voices (into the pool).
        let mut i = 0;
        while i < self.voices.len() {
            if self.voices[i].stop_at.is_some_and(|s| now >= s) {
                let v = self.voices.remove(i);
                self.spare.push(v);
            } else {
                i += 1;
            }
        }
    }

    // -------------------------------------------------------------- render

    /// Render `frames` stereo frames into `out` (interleaved with `channels`).
    /// `scope` receives the post-gain L/R stream for the analysers.
    pub fn render(&mut self, out: &mut [f32], channels: usize, mut scope: impl FnMut(f32, f32)) {
        let channels = channels.max(1);
        let frames = out.len() / channels;
        let sr = self.sr;
        let bank = self.bank.clone();
        let mut i = 0;
        while i < frames {
            self.run_timers();
            let n = SUB.min(frames - i);
            // k-rate-ish coefficient refresh
            let cutoff = self.filt_freq.value() + self.filt_mod + self.filt_env.value();
            let cutoff = cutoff.clamp(0.0, sr * 0.45);
            self.last_cutoff = cutoff;
            self.filt.set(cutoff, self.filt_q.value(), self.filt_gain.value());
            self.lp.set(self.lp_freq.value(), 0.5, 0.0);
            self.peak.set(self.peak_freq.value(), 0.9, 2.2);
            self.formant.set(self.formant_freq.value(), 1.1, 1.6);
            self.delay_damp.set(self.delay_damp_freq.value(), 0.5, 0.0);
            self.delay_hp.set(self.delay_hp_freq.value(), 1.0, 0.0);
            let rd = self.reverb_damp_freq.value();
            for d in self.comb_damp.iter_mut() {
                d.set(rd, 1.0, 0.0);
            }
            let pd = self.phaser_depth.value();
            let plfo = self.phaser_lfo_value;
            for (k, ap) in self.phaser.iter_mut().enumerate() {
                ap.set(PHASER_BASES[k] + plfo * pd, 0.55, 0.0);
            }

            for j in 0..n {
                let t = self.frame as f64 / sr as f64;
                // LFOs
                let lfo_hz = self.lfo_freq.tick(t);
                let lfo_v = self.lfo.next(lfo_hz, sr);
                let pitch_amt = self.lfo_pitch_amt.tick(t);
                let amp_amt = self.lfo_amp_amt.tick(t);
                let lfo_filt = self.lfo_filt_amt.tick(t);
                let flfo_hz = self.filt_lfo_freq.tick(t);
                let flfo_v = self.filt_lfo.next(flfo_hz, sr);
                let flfo_amt = self.filt_lfo_amt.tick(t);
                self.filt_mod = flfo_v * flfo_amt + lfo_v * lfo_filt;
                self.filt_env.tick(t);
                self.filt_freq.tick(t);
                self.filt_q.tick(t);
                self.filt_gain.tick(t);
                self.lp_freq.tick(t);
                self.peak_freq.tick(t);
                self.formant_freq.tick(t);
                self.delay_damp_freq.tick(t);
                self.delay_hp_freq.tick(t);
                self.reverb_scale.tick(t);
                self.reverb_pre_time.tick(t);
                self.reverb_damp_freq.tick(t);
                self.phaser_depth.tick(t);
                self.phaser_lfo_value = self.phaser_lfo.next(0.38, sr);

                // Yuri: jaw vibrato, fades in after the note starts.
                let vib = if self.blowing && self.fx.yuri > 0.001 {
                    let hz = 3.0 + 5.0 * self.fx.yuri_rate;
                    self.yuri_phase += hz / sr as f64;
                    if self.yuri_phase >= 1.0 {
                        self.yuri_phase -= 1.0;
                    }
                    let onset = ((t - self.blow_t0 - 0.15) / 0.4).clamp(0.0, 1.0);
                    ((self.yuri_phase * std::f64::consts::TAU).sin() * onset * self.fx.yuri) as f32
                } else {
                    0.0
                };

                // Voices
                let mut vs = 0f32;
                let flute_g = self.flute_gain.tick(t);
                let bore_g = self.bore_gain.tick(t);
                for v in self.voices.iter_mut() {
                    let g = v.gain.tick(t);
                    let mut r = v.rate.tick(t);
                    if v.rate_mod {
                        r += lfo_v * pitch_amt;
                    }
                    if Some(v.id) == self.current {
                        r *= 1.0 + vib * 0.0353;
                    }
                    if v.bore {
                        if Some(v.id) == self.current {
                            let native = SAMPLE_BANK[v.note].freq as f32;
                            let meri_ratio = 2f32.powf((-(self.meri as f32) * 2.55) / 12.0);
                            self.wg.freq = native * r.max(0.0) / meri_ratio;
                            self.wg.meri = self.meri as f32;
                            self.wg.iki = self.iki as f32;
                        }
                        if !v.sample {
                            continue;
                        }
                    }
                    let Some(bank) = bank.as_ref() else { continue };
                    let s = &bank[v.note];
                    let len = s.data.len();
                    if len < 2 {
                        continue;
                    }
                    let p = v.pos;
                    let i0 = p as usize;
                    if i0 + 1 >= len {
                        v.pos = s.loop_start;
                        continue;
                    }
                    let f = (p - i0 as f64) as f32;
                    let y = s.data[i0] + (s.data[i0 + 1] - s.data[i0]) * f;
                    vs += y * g * flute_g;
                    v.pos += (r.max(0.0) * s.sr / sr) as f64;
                    if v.pos >= s.loop_end {
                        let span = (s.loop_end - s.loop_start).max(1.0);
                        v.pos = s.loop_start + (v.pos - s.loop_end) % span;
                    }
                }
                if self.voice_mode.has_bore() || self.voices.iter().any(|v| v.bore) {
                    let g = self.voices.iter().filter(|v| v.bore).map(|v| v.gain.value()).fold(0f32, f32::max);
                    vs += self.wg.tick() * g * bore_g;
                }

                let x = vs * (1.0 + lfo_v * amp_amt + vib * 0.12);
                let nz = self.noise[self.noise_pos];
                self.noise_pos += 1;
                if self.noise_pos >= self.noise.len() {
                    self.noise_pos = 0;
                }
                let nb = self.noise_bp.process(nz) * self.noise_gain.tick(t);
                let mut y = self.hp.process(x + nb);
                y = self.peak.process(y);
                y = self.formant.process(y);
                y = self.lp.process(y);
                y *= self.breath.tick(t);

                let tin = self.tape_in.tick(t);
                let td = self.tape_dry.tick(t);
                let tw = self.tape_wet.tick(t);
                let post = y * td + self.tape_lp.process(self.shaper.process(y * tin)) * tw;

                let mut f = self.filt.process(post);
                if self.filt.kind == FiltType::Allpass {
                    // An all-pass changes only phase, which is inaudible on its
                    // own; mixing it with the dry signal gives the phasey comb.
                    f = 0.5 * (f + post);
                }
                let sum = f * 0.82 * 0.9;
                let (loop_out, loop_fx) = self.looper.process(t, sum);
                let send = f + loop_fx;

                // phaser
                let mut ph = send;
                for ap in self.phaser.iter_mut() {
                    ph = ap.process(ph);
                }
                ph *= self.phaser_mix.tick(t);

                // ping-pong delay
                let din = self.delay_damp.process(self.delay_hp.process(send));
                let dt = self.delay_time.tick(t) * sr;
                let dl = self.delay_l.read(dt);
                let dr = self.delay_r.read(dt);
                let fb_l = self.delay_fb_l.tick(t);
                let fb_r = self.delay_fb_r.tick(t);
                let c_lr = self.delay_cross_lr.tick(t);
                let c_rl = self.delay_cross_rl.tick(t);
                let src_r = self.delay_src_r.tick(t);
                self.delay_l.write(din + dl * fb_l + dr * c_rl);
                self.delay_r.write(din * src_r + dr * fb_r + dl * c_lr);
                let mix_l = self.delay_mix_l.tick(t);
                let mix_r = self.delay_mix_r.tick(t);

                // comb reverb
                let rfb = self.reverb_fb.tick(t);
                let scale = self.reverb_scale.value();
                let pre_t = self.reverb_pre_time.value() * sr;
                let rin = if pre_t < 1.0 { send } else { self.reverb_pre.process(send, pre_t) };
                if pre_t < 1.0 {
                    self.reverb_pre.write(send);
                }
                let mut rev = 0f32;
                for ((comb, damp), time) in self.combs.iter_mut().zip(self.comb_damp.iter_mut()).zip(REVERB_TIMES) {
                    let o = damp.process(comb.read(time * scale * sr));
                    comb.write(rin + o * rfb);
                    rev += o;
                }
                rev *= self.reverb_mix.tick(t);

                let wr = self.width.process(sum, self.width_time.tick(t) * sr);
                let mut l = sum + dl * mix_l + rev + if self.phaser_routed { ph } else { 0.0 };
                let mut r = wr + dr * mix_r + rev;
                let perc = self.foli.process(t);
                l += loop_out + perc;
                r += loop_out + perc;
                let (cl, cr) = self.comp.process(l, r);
                let og = self.out_gain.tick(t);
                let mut l = cl * og;
                let mut r = cr * og;
                if !l.is_finite() {
                    l = 0.0;
                }
                if !r.is_finite() {
                    r = 0.0;
                }
                // Master limiter: instant-ish attack, 120 ms release, ceiling -0.4 dBFS.
                let need = (LIMIT_CEIL / l.abs().max(r.abs()).max(1e-9)).min(1.0);
                let coef = if need < self.limit_g { 0.9 } else { 1.0 - 1.0 / (0.12 * sr) };
                self.limit_g = need + (self.limit_g - need) * coef;
                let g = self.limit_g.min(need);
                let l = (l * g).clamp(-1.0, 1.0);
                let r = (r * g).clamp(-1.0, 1.0);
                self.loud_ms += (0.5 * (l * l + r * r) - self.loud_ms) * (1.0 / (0.4 * sr));
                self.peak_l = (self.peak_l * 0.99995).max(l.abs());
                self.peak_r = (self.peak_r * 0.99995).max(r.abs());
                scope(l, r);

                let base = (i + j) * channels;
                out[base] = l;
                if channels > 1 {
                    out[base + 1] = r;
                    for c in 2..channels {
                        out[base + c] = 0.0;
                    }
                }
                self.frame += 1;
            }
            i += n;
        }
    }

    /// Copy UI-visible state into `s`.
    pub fn snapshot(&mut self, s: &mut Snapshot) {
        s.now = self.now();
        s.sample_rate = self.sr;
        s.loaded = self.bank.as_ref().map(|b| b.len()).unwrap_or(0);
        s.blowing = self.blowing;
        s.breath = self.breath.value();
        s.filt_env = self.filt_env.value();
        let cur = self.current;
        s.rate = self.voices.iter().find(|v| Some(v.id) == cur).map(|v| v.rate.value()).unwrap_or(0.0);
        s.voices = self.voices.len();
        self.looper.set_now(self.now());
        self.looper.snapshot(&mut s.loop_bank);
        s.peak_l = self.peak_l;
        s.peak_r = self.peak_r;
        s.comp_gr = self.comp.reduction_db();
        s.limit_gr = -20.0 * self.limit_g.max(1e-4).log10();
        s.loud_db = 10.0 * self.loud_ms.max(1e-10).log10();
        s.phaser_routed = self.phaser_routed;
        s.cutoff_hz = self.last_cutoff;
        s.voice_mode_bore = self.voice_mode.has_bore();
        s.voice_mode = self.voice_mode.index() as u8;
        s.foli_on = self.foli.on;
        s.foli_step = self.foli.current_step(self.now());
        s.foli_mask = self.foli.mask();
    }
}
