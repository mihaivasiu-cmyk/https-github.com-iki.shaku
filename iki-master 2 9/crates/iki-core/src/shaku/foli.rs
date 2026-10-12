//! Port of `src/lib/shaku/foli.ts` — a 12-step (4 × triplet) hand-drum and
//! bell accompaniment with three feels. On the web this module was written
//! but never attached to the UI; here it is mixed into the output bus and
//! driven from the Foli panel, following the loop clock.
//!
//! The kit selector picks one of ten sampled one-bar loops (125 bpm, 12 triplet
//! steps each). Every step of the pattern that is switched on plays its slice
//! of the loop, resampled to the current tempo, so the step pads edit the
//! loop. Kit 11 ("Syn") is the original synthesised kit.
//!
//! The synthesised hits are made like the web version: `burst` (band-passed noise),
//! `skin` (pitch-swept sine) and `ken` (three inharmonic bell partials), each
//! with the same exponential attack/decay envelopes.

use crate::dsp::Rng;
use crate::dsp::biquad::Biquad;
use crate::shaku::env_shape::FiltType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoliFeel {
    Work,
    Dance,
    Pulse,
}

impl FoliFeel {
    pub const ALL: [(FoliFeel, &'static str); 3] =
        [(FoliFeel::Work, "Work"), (FoliFeel::Dance, "Dance"), (FoliFeel::Pulse, "Pulse")];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    Dun,
    Sang,
    Slap,
    Ken,
    Shake,
}

type Step = &'static [(Hit, f32)];
use Hit::*;

const WORK: [Step; 12] = [
    &[(Dun, 1.0), (Ken, 0.72), (Shake, 0.45)],
    &[(Shake, 0.12)],
    &[(Shake, 0.28)],
    &[(Sang, 0.7), (Ken, 0.42), (Shake, 0.35)],
    &[(Shake, 0.14)],
    &[(Slap, 0.38), (Shake, 0.2)],
    &[(Dun, 0.88), (Ken, 0.7), (Shake, 0.42)],
    &[(Shake, 0.12)],
    &[(Sang, 0.5), (Shake, 0.3)],
    &[(Ken, 0.5), (Shake, 0.22)],
    &[(Slap, 0.55), (Shake, 0.32)],
    &[(Shake, 0.16)],
];

const DANCE: [Step; 12] = [
    &[(Dun, 1.0), (Ken, 0.8), (Shake, 0.55)],
    &[(Slap, 0.35), (Shake, 0.22)],
    &[(Sang, 0.45), (Shake, 0.4)],
    &[(Ken, 0.65), (Shake, 0.28)],
    &[(Dun, 0.55), (Slap, 0.5), (Shake, 0.38)],
    &[(Shake, 0.18)],
    &[(Dun, 0.92), (Ken, 0.78), (Shake, 0.5)],
    &[(Sang, 0.4), (Shake, 0.24)],
    &[(Slap, 0.62), (Ken, 0.4), (Shake, 0.42)],
    &[(Shake, 0.2)],
    &[(Sang, 0.7), (Ken, 0.55), (Shake, 0.36)],
    &[(Slap, 0.48), (Shake, 0.3)],
];

const PULSE: [Step; 12] = [
    &[(Dun, 0.9), (Ken, 0.55)],
    &[],
    &[(Shake, 0.18)],
    &[(Ken, 0.28)],
    &[(Sang, 0.4)],
    &[],
    &[(Dun, 0.7), (Ken, 0.5), (Shake, 0.22)],
    &[],
    &[(Ken, 0.32)],
    &[],
    &[(Sang, 0.35), (Shake, 0.16)],
    &[],
];

const BEAT_FILL: Step = &[(Dun, 0.85), (Ken, 0.5)];
const OFF_FILL: Step = &[(Shake, 0.4)];

fn feel_pattern(f: FoliFeel) -> [Step; 12] {
    match f {
        FoliFeel::Work => WORK,
        FoliFeel::Dance => DANCE,
        FoliFeel::Pulse => PULSE,
    }
}

/// `exponentialRampToValueAtTime` envelope: 1e-4 → peak over `atk`, → 1e-4 at `dur`.
#[derive(Clone, Copy, Debug)]
struct ExpEnv {
    t0: f64,
    atk: f64,
    dur: f64,
    peak: f32,
}

impl ExpEnv {
    #[inline]
    fn at(&self, t: f64) -> f32 {
        let dt = t - self.t0;
        if dt < 0.0 {
            return 0.0;
        }
        let floor = 1e-4f32;
        let peak = self.peak.max(0.001);
        if dt < self.atk {
            floor * (peak / floor).powf((dt / self.atk) as f32)
        } else if dt < self.dur {
            peak * (floor / peak).powf(((dt - self.atk) / (self.dur - self.atk)) as f32)
        } else {
            floor
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    Burst { pos: usize, bp: Biquad },
    Skin { phase: f64, f0: f32, f1: f32, sweep: f64 },
    Ken { phase: [f64; 3] },
}

#[derive(Clone, Copy, Debug)]
struct PercVoice {
    kind: Kind,
    env: ExpEnv,
    stop: f64,
}

pub struct Foli {
    sr: f32,
    pub on: bool,
    pub feel: FoliFeel,
    pub level: f64,
    /// Tempo in bpm (three steps per beat, twelve per bar).
    pub bpm: f64,
    step: usize,
    next: f64,
    pattern: [Step; 12],
    noise: Vec<f32>,
    voices: Vec<PercVoice>,
    slices: Vec<SliceVoice>,
    kits: Vec<Vec<i16>>,
    /// Which bar of the loop the pattern is on (0..bars of the kit).
    bar: usize,
    /// 0..KIT_COUNT = sampled loop, KIT_SYNTH = synthesised.
    pub kit: usize,
    rng: Rng,
    /// When each step last sounded (for the UI step light).
    step_times: [f64; 12],
    /// How far ahead steps are scheduled (the web version's 140 ms timer
    /// look-ahead); 0 while locked to a host transport, so a relocate is exact.
    lookahead: f64,
}

const LOOKAHEAD: f64 = 0.14;

/// Number of sampled presets; `kit == KIT_SYNTH` selects the synthesised drums.
pub const KIT_COUNT: usize = 7;
pub const KIT_SYNTH: usize = KIT_COUNT;
const KIT_SR: f64 = 44100.0;

/// (recorded bpm, bars in the loop, wav). Each loop is cut into `bars × 12`
/// triplet steps; the 12-step pattern plays one bar of it per pass, so a
/// whole loop runs over `bars` passes of the pattern.
const KITS: [(f64, usize, &[u8]); KIT_COUNT] = [
    (90.0, 4, include_bytes!("../../assets/drums/loop_090_gm.wav")),
    (110.0, 8, include_bytes!("../../assets/drums/loop_110_cm.wav")),
    (110.0, 8, include_bytes!("../../assets/drums/loop_110_em.wav")),
    (140.0, 8, include_bytes!("../../assets/drums/loop_140_cm.wav")),
    (140.0, 8, include_bytes!("../../assets/drums/loop_140_d.wav")),
    (160.0, 8, include_bytes!("../../assets/drums/loop_160_am.wav")),
    (160.0, 8, include_bytes!("../../assets/drums/loop_160_gm.wav")),
];

/// Preset names for the UI (bpm and key of each loop).
pub const KIT_NAMES: [&str; KIT_COUNT] = ["90 Gm", "110 Cm", "110 Em", "140 Cm", "140 D", "160 Am", "160 Gm"];

fn decode_kit(bytes: &[u8]) -> Vec<i16> {
    let Ok(mut r) = hound::WavReader::new(std::io::Cursor::new(bytes)) else { return Vec::new() };
    r.samples::<i16>().map(|s| s.unwrap_or(0)).collect()
}

/// One playing slice of a kit loop.
#[derive(Clone, Copy, Debug)]
struct SliceVoice {
    kit: usize,
    pos: f64,
    end: f64,
    inc: f64,
    t0: f64,
    vel: f32,
    played: usize,
}

impl Foli {
    pub fn new(sr: f32) -> Self {
        // `noiseBuf`: 0.6 s of lightly low-passed noise.
        let mut rng = Rng::new(0xf011_5eed);
        let n = (sr * 0.6) as usize;
        let mut b = 0f32;
        let noise = (0..n)
            .map(|_| {
                let w = rng.bipolar();
                b = 0.97 * b + 0.03 * w;
                (w * 0.35 + b) * 0.8
            })
            .collect();
        Self {
            sr,
            on: false,
            feel: FoliFeel::Work,
            level: 0.55,
            bpm: 72.0,
            step: 0,
            next: 0.0,
            pattern: WORK,
            noise,
            voices: Vec::with_capacity(96),
            slices: Vec::with_capacity(64),
            kits: KITS.iter().map(|k| decode_kit(k.2)).collect(),
            bar: 0,
            kit: 0,
            rng,
            step_times: [-1.0; 12],
            lookahead: LOOKAHEAD,
        }
    }

    /// Follow a host transport: `beat` is its position in quarter notes at
    /// `now`, `None` while it is stopped (then the pattern runs free, as in the
    /// app). While playing, the steps sit on the host's grid: step 0 on every
    /// bar line of 4/4, steps 3, 6, 9 on the other beats.
    pub fn follow(&mut self, beat: Option<f64>, now: f64) {
        let Some(beat) = beat else {
            self.lookahead = LOOKAHEAD;
            return;
        };
        self.lookahead = 0.0;
        if !self.on {
            return;
        }
        let ss = self.step_sec();
        // grid position of the next scheduled step
        let pos = beat * 3.0 + (self.next - now) / ss;
        let k = pos.round();
        let idx = (k as i64).rem_euclid(12) as usize;
        let nbars = self.bars() as i64;
        let bar = (k as i64).div_euclid(12).rem_euclid(nbars) as usize;
        if ((pos - k) * ss).abs() > 0.002 || idx != self.step || bar != self.bar || self.next < now - 0.01 {
            let first = (beat * 3.0 - 1e-9).ceil();
            self.next = now + (first - beat * 3.0).max(0.0) * ss;
            self.step = (first as i64).rem_euclid(12) as usize;
            self.bar = (first as i64).div_euclid(12).rem_euclid(nbars) as usize;
        }
    }

    pub fn set_on(&mut self, on: bool, now: f64) {
        self.on = on;
        if on {
            self.step = 0;
            self.bar = 0;
            self.next = now + 0.04;
        }
    }

    pub fn set_feel(&mut self, f: FoliFeel) {
        self.feel = f;
        self.pattern = feel_pattern(f);
    }

    pub fn set_kit(&mut self, k: usize) {
        self.kit = k.min(KIT_COUNT - 1);
        self.bar = 0;
        self.slices.clear();
    }

    /// Bars in the current preset's loop (1 for the synthesised kit).
    fn bars(&self) -> usize {
        KITS.get(self.kit).map(|k| k.1).unwrap_or(1).max(1)
    }

    /// Start step `s` of the sampled loop at time `t`: the slice covers exactly
    /// one triplet step at the current tempo.
    fn play_slice(&mut self, s: usize, t: f64, vel: f32) {
        let Some(data) = self.kits.get(self.kit) else { return };
        let len = data.len();
        let steps = self.bars() * 12;
        if len < steps {
            return;
        }
        let slice = len as f64 / steps as f64;
        let start = slice * (self.bar * 12 + s) as f64;
        // run a few ms into the next slice so neighbouring steps cross-fade
        let end = (start + slice + 0.004 * KIT_SR).min(len as f64 - 1.0);
        let inc = (self.bpm / KITS[self.kit].0) * KIT_SR / self.sr as f64;
        if self.slices.len() >= 64 {
            self.slices.remove(0);
        }
        self.slices.push(SliceVoice { kit: self.kit, pos: start, end, inc, t0: t, vel, played: 0 });
    }

    pub fn toggle_step(&mut self, i: usize) {
        if i > 11 {
            return;
        }
        self.pattern[i] = if !self.pattern[i].is_empty() {
            &[]
        } else if i.is_multiple_of(3) {
            BEAT_FILL
        } else {
            OFF_FILL
        };
    }

    pub fn mask(&self) -> [bool; 12] {
        std::array::from_fn(|i| !self.pattern[i].is_empty())
    }

    /// The step currently sounding, if running.
    pub fn current_step(&self, now: f64) -> Option<usize> {
        if !self.on {
            return None;
        }
        (0..12)
            .filter(|&i| self.step_times[i] <= now && self.step_times[i] >= 0.0)
            .max_by(|&a, &b| self.step_times[a].partial_cmp(&self.step_times[b]).unwrap_or(std::cmp::Ordering::Equal))
    }

    fn step_sec(&self) -> f64 {
        60.0 / self.bpm.max(20.0) / 3.0
    }

    fn push(&mut self, v: PercVoice) {
        if self.voices.len() >= 96 {
            self.voices.remove(0);
        }
        self.voices.push(v);
    }

    fn burst(&mut self, t: f64, vel: f32, freq: f32, q: f32, dur: f64) {
        let mut bp = Biquad::new(FiltType::Bandpass, self.sr);
        bp.set(freq, q, 0.0);
        self.push(PercVoice {
            kind: Kind::Burst { pos: 0, bp },
            env: ExpEnv { t0: t, atk: 0.004, dur, peak: vel },
            stop: t + dur + 0.02,
        });
    }

    fn skin(&mut self, t: f64, vel: f32, f0: f32, f1: f32, dur: f64) {
        self.push(PercVoice {
            kind: Kind::Skin { phase: 0.0, f0, f1, sweep: dur * 0.7 },
            env: ExpEnv { t0: t, atk: 0.006, dur, peak: vel },
            stop: t + dur + 0.02,
        });
    }

    fn ken(&mut self, t: f64, vel: f32) {
        self.push(PercVoice {
            kind: Kind::Ken { phase: [0.0; 3] },
            env: ExpEnv { t0: t, atk: 0.002, dur: 0.55, peak: vel * 0.22 },
            stop: t + 0.58,
        });
    }

    fn play_hit(&mut self, h: Hit, t: f64, vel: f32) {
        let v = vel * (0.86 + self.rng.next_f32() * 0.28);
        match h {
            Hit::Dun => {
                self.skin(t, v * 0.7, 68.0, 46.0, 0.32);
                self.burst(t, v * 0.22, 90.0, 1.1, 0.12);
            }
            Hit::Sang => {
                self.skin(t, v * 0.42, 196.0, 150.0, 0.16);
                self.burst(t, v * 0.28, 420.0, 2.2, 0.09);
            }
            Hit::Slap => {
                self.burst(t, v * 0.55, 2400.0, 0.8, 0.07);
                self.burst(t, v * 0.22, 6800.0, 0.6, 0.04);
            }
            Hit::Ken => self.ken(t, v),
            Hit::Shake => self.burst(t, v * 0.18, 5200.0, 0.7, 0.05),
        }
    }

    /// The 25 ms look-ahead scheduler (`schedule()`), then one output sample.
    #[inline]
    pub fn process(&mut self, now: f64) -> f32 {
        if self.on {
            while self.next < now + self.lookahead {
                let s = self.step;
                let t;
                if self.kit < KIT_COUNT {
                    // sampled loop: stay exactly on the grid
                    t = self.next;
                    let peak = self.pattern[s].iter().fold(0f32, |m, &(_, v)| m.max(v));
                    if !self.pattern[s].is_empty() {
                        self.play_slice(s, t, (0.55 + 0.45 * peak) * self.level as f32 * 1.6);
                    }
                } else {
                    let human = (self.rng.next_f32() as f64 - 0.5) * 0.012;
                    t = self.next + human;
                    for &(h, v) in self.pattern[s] {
                        self.play_hit(h, t, v * self.level as f32);
                    }
                }
                self.step_times[s] = t;
                self.step = (self.step + 1) % 12;
                if self.step == 0 {
                    self.bar = (self.bar + 1) % self.bars();
                }
                self.next += self.step_sec();
            }
        }
        if self.voices.is_empty() && self.slices.is_empty() {
            return 0.0;
        }
        let sr = self.sr as f64;
        let mut out = 0f32;
        let fade_in = (0.002 * sr) as usize + 1;
        let fade_out = 0.004 * KIT_SR;
        let kits = &self.kits;
        for v in self.slices.iter_mut() {
            if now < v.t0 {
                continue;
            }
            let Some(d) = kits.get(v.kit) else { continue };
            let i0 = v.pos as usize;
            if i0 + 1 >= d.len() {
                v.pos = v.end;
                continue;
            }
            let f = (v.pos - i0 as f64) as f32;
            let s = (d[i0] as f32 + (d[i0 + 1] as f32 - d[i0] as f32) * f) * (1.0 / 32768.0);
            let fi = (v.played as f32 / fade_in as f32).min(1.0);
            let fo = (((v.end - v.pos) / fade_out) as f32).clamp(0.0, 1.0);
            out += s * v.vel * fi * fo;
            v.pos += v.inc;
            v.played += 1;
        }
        self.slices.retain(|v| v.pos < v.end);
        let noise = &self.noise;
        for v in self.voices.iter_mut() {
            if now < v.env.t0 {
                continue;
            }
            let g = v.env.at(now);
            let x = match &mut v.kind {
                Kind::Burst { pos, bp } => {
                    let n = noise.get(*pos).copied().unwrap_or(0.0);
                    *pos += 1;
                    bp.process(n)
                }
                Kind::Skin { phase, f0, f1, sweep } => {
                    let u = ((now - v.env.t0) / *sweep).clamp(0.0, 1.0) as f32;
                    let f = *f0 * (*f1 / *f0).powf(u);
                    *phase = (*phase + f as f64 / sr).fract();
                    (*phase as f32 * std::f32::consts::TAU).sin()
                }
                Kind::Ken { phase } => {
                    let mut s = 0.0;
                    for (p, f) in phase.iter_mut().zip([1864.0, 2489.0, 3120.0]) {
                        *p = (*p + f / sr).fract();
                        s += (*p as f32 * std::f32::consts::TAU).sin();
                    }
                    s
                }
            };
            out += x * g;
        }
        self.voices.retain(|v| now < v.stop);
        out * 0.55
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_in_time_and_stops() {
        let sr = 16000.0;
        let mut f = Foli::new(sr);
        f.set_on(true, 0.0);
        let mut energy = 0f64;
        for i in 0..(sr as usize * 2) {
            let y = f.process(i as f64 / sr as f64);
            assert!(y.is_finite());
            energy += (y * y) as f64;
        }
        assert!(energy > 1.0, "foli silent: {energy}");
        // 72 bpm triplet steps = 0.2778 s; after 2 s we are on step 7.
        assert_eq!(f.current_step(2.0), Some(7));
        f.set_on(false, 2.0);
        for i in 0..(sr as usize) {
            f.process(2.0 + i as f64 / sr as f64);
        }
        assert!(f.voices.is_empty());
    }

    #[test]
    fn follows_the_host_grid() {
        let sr = 16000.0;
        let mut f = Foli::new(sr);
        f.bpm = 120.0; // step = 1/6 s
        f.set_on(true, 0.0);
        // host transport at beat 2.5 (step 7.5) at t = 1.0: next step is 8, at t = 1 + 1/12
        f.follow(Some(2.5), 1.0);
        let mut t = 1.0;
        while t < 1.2 {
            f.process(t);
            t += 1.0 / sr as f64;
        }
        let at = f.step_times[8];
        assert!((at - (1.0 + 1.0 / 12.0)).abs() < 0.008, "step 8 at {at}");
        assert_eq!(f.current_step(1.2), Some(8));
        // stopped transport: free-running again with the look-ahead
        f.follow(None, 1.2);
        assert_eq!(f.lookahead, LOOKAHEAD);
    }

    /// Render `secs` of a running Foli on the preset `kit` at its recorded tempo.
    fn render_kit(kit: usize, secs: f64, mutate: impl Fn(&mut Foli)) -> Vec<f32> {
        let sr = 16000.0;
        let mut f = Foli::new(sr);
        f.bpm = KITS[kit].0;
        f.set_kit(kit);
        mutate(&mut f);
        f.set_on(true, 0.0);
        (0..(sr as f64 * secs) as usize).map(|i| f.process(i as f64 / sr as f64)).collect()
    }

    fn energy(x: &[f32]) -> f64 {
        x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>()
    }

    #[test]
    fn every_preset_plays_and_walks_through_its_loop() {
        for kit in 0..KIT_COUNT {
            let bar = 240.0 / KITS[kit].0; // seconds in one 12-step bar
            let x = render_kit(kit, bar * 3.0 + 0.1, |_| {});
            assert!(x.iter().all(|v| v.is_finite()), "kit {kit} produced NaN");
            let n = (bar * 16000.0) as usize;
            let off = (0.04 * 16000.0) as usize; // Foli starts 40 ms after switching on
            let (b0, b1) = (&x[off..off + n], &x[off + n..off + 2 * n]);
            assert!(energy(b0) > 1.0 && energy(b1) > 1.0, "kit {kit} silent: {} {}", energy(b0), energy(b1));
            // successive bars play successive parts of the loop
            let dot: f64 = b0.iter().zip(b1).map(|(a, b)| (*a * *b) as f64).sum();
            let corr = dot / (energy(b0) * energy(b1)).sqrt();
            assert!(corr < 0.98, "kit {kit}: bar 2 repeats bar 1 (corr {corr:.3})");
        }
    }

    #[test]
    fn switched_off_steps_mute_their_slices() {
        let bar = 240.0 / KITS[0].0;
        let all = render_kit(0, bar, |_| {});
        let none = render_kit(0, bar, |f| {
            for i in 0..12 {
                if !f.pattern[i].is_empty() {
                    f.toggle_step(i);
                }
            }
        });
        assert!(energy(&all) > 1.0);
        assert!(energy(&none) < 1e-6, "muted pattern still sounds: {}", energy(&none));
    }

    #[test]
    fn toggle_step_fills_beats_and_offbeats() {
        let mut f = Foli::new(8000.0);
        f.set_feel(FoliFeel::Pulse);
        assert!(!f.mask()[1]);
        f.toggle_step(1);
        assert!(f.mask()[1]);
        f.toggle_step(0);
        assert!(!f.mask()[0]);
    }
}
