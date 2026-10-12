//! Port of `src/lib/shaku/looper.ts` — a 4-track bar-quantised looper.
//!
//! Track 4 doubles as the "freeze" pad: `freeze_pad` bounces tracks 1–3 into
//! one cycle that fires as a one-shot. Everything runs on the audio thread;
//! the web version's `setTimeout` bar scheduling becomes a sample-accurate
//! pending operation.

use crate::dsp::param::Param;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LoopMode {
    #[default]
    Idle,
    Rec,
    Play,
    Dub,
    Stop,
}

impl LoopMode {
    pub fn name(self) -> &'static str {
        match self {
            LoopMode::Idle => "idle",
            LoopMode::Rec => "rec",
            LoopMode::Play => "play",
            LoopMode::Dub => "dub",
            LoopMode::Stop => "stop",
        }
    }
}

pub const TRACK_COUNT: usize = 4;
pub const PAD_TRACK: usize = 3;
pub const WAVE_BINS: usize = 384;
/// Default longest recording per track. Front ends with the memory to spare (the desktop app)
/// raise it with [`set_max_seconds`] before the engine is built; phones keep this.
const MAX_SEC: f64 = 24.0;
static MAX_SEC_BITS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Raise (or lower) the recording length for engines created from now on. Call before `audio::start`.
pub fn set_max_seconds(secs: f64) {
    MAX_SEC_BITS.store((secs.clamp(MAX_SEC, 600.0) as f32).to_bits(), std::sync::atomic::Ordering::Relaxed);
}

fn configured_max_seconds() -> f64 {
    match MAX_SEC_BITS.load(std::sync::atomic::Ordering::Relaxed) {
        0 => MAX_SEC,
        b => f32::from_bits(b) as f64,
    }
}
const MIN_SEC: f64 = 0.35;

#[derive(Clone, Debug)]
pub struct LoopSnap {
    pub id: usize,
    pub mode: LoopMode,
    pub progress: f64,
    pub length: f64,
    pub layers: u32,
    pub can_undo: bool,
    pub level: f64,
    pub trim_in: f64,
    pub trim_out: f64,
    /// Empty when the track holds no audio.
    pub wave: Vec<f32>,
    pub pad: bool,
    pub pending: bool,
    pub fx: bool,
}

impl LoopSnap {
    pub fn empty(id: usize) -> Self {
        Self {
            id,
            mode: LoopMode::Idle,
            progress: 0.0,
            length: 0.0,
            layers: 0,
            can_undo: false,
            level: 0.9,
            trim_in: 0.0,
            trim_out: 1.0,
            wave: Vec::new(),
            pad: false,
            pending: false,
            fx: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LoopBankSnap {
    pub tracks: Vec<LoopSnap>,
    pub bpm: f64,
    pub bar_phase: f64,
    pub cycle_bars: f64,
    pub pending_track: i32,
    pub pad_armed: bool,
}

impl Default for LoopBankSnap {
    fn default() -> Self {
        Self {
            tracks: (0..TRACK_COUNT).map(LoopSnap::empty).collect(),
            bpm: 72.0,
            bar_phase: 0.0,
            cycle_bars: 0.0,
            pending_track: -1,
            pad_armed: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Src {
    pos: f64,
    looping: bool,
}

struct Track {
    src: Option<Src>,
    gain: Param,
    mode: LoopMode,
    rec: Vec<f32>,
    data: Vec<f32>,
    has_buf: bool,
    undo: Vec<f32>,
    has_undo: bool,
    epoch: f64,
    layers: u32,
    level: f64,
    rec_started: f64,
    trim_in: f64,
    trim_out: f64,
    is_pad: bool,
    fx_send: Param,
    fx_on: bool,
    wave: [f32; WAVE_BINS],
    wave_len: usize,
    wave_dirty: bool,
}

impl Track {
    fn new(sr: f32, max_sec: f64) -> Self {
        let cap = (sr as f64 * (max_sec + 4.0)) as usize;
        Self {
            src: None,
            gain: Param::new(0.9, sr),
            mode: LoopMode::Idle,
            // Reserved up front so recording / overdub / freeze never allocate on the audio thread.
            // (Untouched pages cost no physical memory.)
            rec: Vec::with_capacity(cap),
            data: Vec::with_capacity(cap),
            has_buf: false,
            undo: Vec::with_capacity(cap),
            has_undo: false,
            epoch: 0.0,
            layers: 0,
            level: 0.9,
            rec_started: 0.0,
            trim_in: 0.0,
            trim_out: 1.0,
            is_pad: false,
            fx_send: Param::new(0.0, sr),
            fx_on: false,
            wave: [0.0; WAVE_BINS],
            wave_len: 0,
            wave_dirty: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Punch,
    Play,
    Stop,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
struct Pending {
    track: usize,
    op: Op,
    due: f64,
}

pub struct Looper {
    sr: f64,
    max_sec: f64,
    tracks: Vec<Track>,
    bpm: f64,
    clock_origin: Option<f64>,
    cycle_samples: usize,
    pending: Option<Pending>,
    now: f64,
    wave_clock: usize,
}

fn clamp_track(i: usize) -> usize {
    i.min(TRACK_COUNT - 1)
}

impl Looper {
    pub fn new(sr: f32) -> Self {
        let max_sec = configured_max_seconds();
        Self {
            sr: sr as f64,
            max_sec,
            tracks: (0..TRACK_COUNT).map(|_| Track::new(sr, max_sec)).collect(),
            bpm: 72.0,
            clock_origin: None,
            cycle_samples: 0,
            pending: None,
            now: 0.0,
            wave_clock: 0,
        }
    }

    fn max_samples(&self) -> usize {
        (self.sr * self.max_sec) as usize
    }
    fn min_samples(&self) -> usize {
        (self.sr * MIN_SEC) as usize
    }
    fn bar_sec(&self) -> f64 {
        (60.0 / self.bpm) * 4.0
    }
    fn bar_samples(&self) -> usize {
        ((self.sr * self.bar_sec()).round() as usize).max(1)
    }

    pub fn set_clock(&mut self, bpm: f64) {
        self.bpm = bpm.clamp(40.0, 200.0);
    }

    #[allow(dead_code)]
    fn next_bar_time(&mut self) -> f64 {
        let now = self.now;
        let b = self.bar_sec();
        let Some(origin) = self.clock_origin else {
            self.clock_origin = Some(now);
            return now;
        };
        let n = ((now - origin) / b - 1e-4).ceil();
        origin + n.max(0.0) * b
    }

    /// Rec / Play / Stop act the instant the button goes down. (The web
    /// version waited for the next bar; the recorded length still snaps to
    /// whole bars when a take is committed, and a restarted track re-joins the
    /// loop clock in phase — see `clock_offset`.)
    fn schedule(&mut self, track: usize, op: Op) {
        self.pending = None;
        self.run(track, op);
    }

    /// Seconds into the shared cycle right now, so a track started at an
    /// arbitrary moment lines up with the ones already playing.
    fn clock_offset(&self) -> f64 {
        match self.clock_origin {
            Some(origin) => (self.now - origin).max(0.0),
            None => 0.0,
        }
    }

    fn run(&mut self, i: usize, op: Op) {
        match op {
            Op::Punch => {
                for j in 0..TRACK_COUNT {
                    if j != i && self.tracks[j].mode == LoopMode::Rec {
                        self.commit_record(j, true);
                    }
                }
                self.punch_track(i);
            }
            Op::Play => {
                let tr = &mut self.tracks[i];
                if tr.mode == LoopMode::Rec {
                    self.commit_record(i, true);
                    return;
                }
                if tr.mode == LoopMode::Dub {
                    tr.mode = LoopMode::Play;
                    tr.layers += 1;
                    return;
                }
                if !tr.has_buf {
                    return;
                }
                tr.mode = LoopMode::Play;
                let off = self.clock_offset();
                self.start_src(i, off);
            }
            Op::Stop => self.stop_now(i),
        }
    }

    fn stop_now(&mut self, i: usize) {
        if self.tracks[i].mode == LoopMode::Rec {
            self.commit_record(i, false);
            return;
        }
        let t = &mut self.tracks[i];
        if t.mode == LoopMode::Dub {
            t.layers += 1;
        }
        t.src = None;
        t.mode = if t.has_buf { LoopMode::Stop } else { LoopMode::Idle };
    }

    fn snap_count(&self, samples: usize) -> usize {
        if self.cycle_samples > 0 {
            let n = ((samples as f64 / self.cycle_samples as f64).round() as usize).max(1);
            return n * self.cycle_samples;
        }
        let bar = self.bar_samples();
        let bars = ((samples as f64 / bar as f64).round() as usize).max(1);
        bars * bar
    }

    fn refresh_cycle(&mut self) {
        if !self.tracks.iter().any(|t| t.has_buf && !t.is_pad) {
            self.cycle_samples = 0;
            self.clock_origin = None;
        }
    }

    /// (start_sec, end_sec, span_sec, start_samp, end_samp)
    fn region(&self, i: usize) -> (f64, f64, f64, usize, usize) {
        let t = &self.tracks[i];
        if !t.has_buf || t.data.is_empty() {
            return (0.0, 0.0, 0.0, 0, 0);
        }
        let len = t.data.len();
        let dur = len as f64 / self.sr;
        let start = t.trim_in * dur;
        let end = (start + MIN_SEC).max(t.trim_out * dur);
        let span = (end - start).max(MIN_SEC);
        let s0 = ((start * self.sr) as usize).min(len.saturating_sub(1));
        let s1 = ((end * self.sr) as usize).min(len).max(s0 + 1);
        (start, end, span, s0, s1)
    }

    fn start_src(&mut self, i: usize, offset_in_region: f64) {
        if !self.tracks[i].has_buf {
            return;
        }
        let (start, _end, span, ..) = self.region(i);
        let off = start + ((offset_in_region % span) + span) % span;
        let now = self.now;
        let t = &mut self.tracks[i];
        t.src = Some(Src { pos: off * self.sr, looping: true });
        t.epoch = now - (off - start);
    }

    fn commit_record(&mut self, i: usize, play: bool) {
        let samples = self.tracks[i].rec.len();
        if samples < self.min_samples() {
            let t = &mut self.tracks[i];
            t.rec.clear();
            t.mode = if t.has_buf { LoopMode::Stop } else { LoopMode::Idle };
            t.wave_dirty = true;
            return;
        }
        let want = self.snap_count(samples);
        if self.cycle_samples == 0 {
            self.cycle_samples = want;
        }
        let now = self.now;
        let sr = self.sr;
        let t = &mut self.tracks[i];
        t.data.clear();
        let n = samples.min(want);
        t.data.extend_from_slice(&t.rec[..n]);
        t.data.resize(want, 0.0);
        t.rec.clear();
        t.has_buf = true;
        t.has_undo = false;
        t.layers = 1;
        t.trim_in = 0.0;
        t.trim_out = 1.0;
        t.is_pad = false;
        t.wave_dirty = true;
        if self.clock_origin.is_none() {
            self.clock_origin = Some(now - want as f64 / sr);
        }
        if play {
            self.tracks[i].mode = LoopMode::Play;
            self.start_src(i, 0.0);
        } else {
            let t = &mut self.tracks[i];
            t.mode = LoopMode::Stop;
            t.src = None;
        }
    }

    fn punch_track(&mut self, i: usize) {
        let now = self.now;
        if self.tracks[i].is_pad && self.tracks[i].has_buf {
            self.fire_pad(true);
            return;
        }
        match self.tracks[i].mode {
            LoopMode::Idle => {
                let cap = self.max_samples();
                let t = &mut self.tracks[i];
                t.rec.clear();
                t.rec.reserve(cap);
                t.rec_started = now;
                t.mode = LoopMode::Rec;
                if self.clock_origin.is_none() {
                    self.clock_origin = Some(now);
                }
            }
            LoopMode::Rec => self.commit_record(i, true),
            LoopMode::Play => {
                let t = &mut self.tracks[i];
                if t.has_buf {
                    t.undo.clear();
                    t.undo.extend_from_slice(&t.data);
                    t.has_undo = true;
                }
                t.mode = LoopMode::Dub;
            }
            LoopMode::Dub => {
                let t = &mut self.tracks[i];
                t.mode = LoopMode::Play;
                t.layers += 1;
            }
            LoopMode::Stop => {
                if self.tracks[i].has_buf {
                    self.tracks[i].mode = LoopMode::Play;
                    let off = self.clock_offset();
                    self.start_src(i, off);
                }
            }
        }
    }

    pub fn punch(&mut self, track: usize) {
        let i = clamp_track(track);
        if self.tracks[i].is_pad && self.tracks[i].has_buf {
            self.fire_pad(true);
            return;
        }
        self.schedule(i, Op::Punch);
    }

    pub fn play(&mut self, track: usize) {
        let i = clamp_track(track);
        let t = &self.tracks[i];
        if t.is_pad && t.has_buf {
            self.fire_pad(true);
            return;
        }
        if !t.has_buf && t.mode != LoopMode::Rec {
            return;
        }
        self.schedule(i, Op::Play);
    }

    pub fn stop(&mut self, track: Option<usize>) {
        match track {
            None => {
                self.pending = None;
                for i in 0..TRACK_COUNT {
                    self.stop_now(i);
                }
            }
            Some(t) => self.schedule(clamp_track(t), Op::Stop),
        }
    }

    pub fn undo(&mut self, track: usize) {
        let i = clamp_track(track);
        let t = &mut self.tracks[i];
        if t.has_undo && t.has_buf {
            let n = t.data.len().min(t.undo.len());
            t.data[..n].copy_from_slice(&t.undo[..n]);
            t.has_undo = false;
            if t.layers > 1 {
                t.layers -= 1;
            }
            if t.mode == LoopMode::Dub {
                t.mode = LoopMode::Play;
            }
            t.wave_dirty = true;
            return;
        }
        self.clear(Some(i));
    }

    pub fn clear(&mut self, track: Option<usize>) {
        let (lo, hi) = match track {
            None => (0, TRACK_COUNT),
            Some(t) => (clamp_track(t), clamp_track(t) + 1),
        };
        for i in lo..hi {
            let t = &mut self.tracks[i];
            t.src = None;
            t.rec.clear();
            t.data.clear();
            t.has_buf = false;
            t.undo.clear();
            t.has_undo = false;
            t.layers = 0;
            t.trim_in = 0.0;
            t.trim_out = 1.0;
            t.mode = LoopMode::Idle;
            t.is_pad = false;
            t.wave_dirty = true;
        }
        self.refresh_cycle();
    }

    pub fn freeze_pad(&mut self) {
        let cycle = if self.cycle_samples > 0 { self.cycle_samples } else { self.bar_samples() };
        // Build the mix in the pad track's own (pre-reserved) buffer.
        let mut mix = std::mem::take(&mut self.tracks[PAD_TRACK].data);
        mix.clear();
        mix.resize(cycle, 0.0);
        for src in self.tracks.iter().take(PAD_TRACK) {
            if !src.has_buf || src.data.is_empty() {
                continue;
            }
            let g = src.level as f32;
            let len = src.data.len();
            for (s, m) in mix.iter_mut().enumerate() {
                *m += src.data[s % len] * g;
            }
        }
        for m in mix.iter_mut() {
            *m = m.tanh();
        }
        let pad = &mut self.tracks[PAD_TRACK];
        pad.src = None;
        pad.data = mix;
        pad.has_buf = true;
        pad.has_undo = false;
        pad.layers = 1;
        pad.trim_in = 0.0;
        pad.trim_out = 1.0;
        pad.is_pad = true;
        pad.mode = LoopMode::Stop;
        pad.wave_dirty = true;
        if self.cycle_samples == 0 {
            self.cycle_samples = cycle;
        }
    }

    pub fn fire_pad(&mut self, on: bool) {
        let now = self.now;
        let pad = &mut self.tracks[PAD_TRACK];
        if !pad.has_buf {
            return;
        }
        if !on {
            pad.src = None;
            pad.mode = LoopMode::Stop;
            return;
        }
        pad.src = Some(Src { pos: 0.0, looping: false });
        pad.mode = LoopMode::Play;
        pad.epoch = now;
        pad.is_pad = true;
    }

    pub fn set_level(&mut self, track: usize, v: f64) {
        let now = self.now;
        let t = &mut self.tracks[clamp_track(track)];
        let n = v.clamp(0.0, 1.0);
        t.level = n;
        let g = &mut t.gain;
        g.cancel(now);
        g.set_value_at(g.value().max(0.0), now);
        if n <= 0.004 {
            g.linear_ramp_to(0.0, now + 0.02);
        } else {
            g.set_target_at(n.powf(1.45) as f32, now, 0.02);
        }
    }

    pub fn set_fx(&mut self, track: usize, on: bool) {
        let now = self.now;
        let t = &mut self.tracks[clamp_track(track)];
        t.fx_on = on;
        t.fx_send.set_target_at(if on { 1.0 } else { 0.0 }, now, 0.02);
    }

    pub fn set_bounds(&mut self, track: usize, start: f64, end: f64) {
        let i = clamp_track(track);
        let t = &mut self.tracks[i];
        let dur = if t.has_buf { t.data.len() as f64 / self.sr } else { 1.0 };
        let min = 0.04f64.min(MIN_SEC / dur.max(MIN_SEC));
        let mut a = start.clamp(0.0, 1.0);
        let mut b = end.clamp(0.0, 1.0);
        if b - a < min {
            if a + min <= 1.0 {
                b = a + min;
            } else {
                a = (b - min).max(0.0);
            }
        }
        t.trim_in = a;
        t.trim_out = b;
    }

    /// Set the clock time (seconds) before calling `process` / commands.
    pub fn set_now(&mut self, now: f64) {
        self.now = now;
    }

    /// One sample: `input` is the dry voice bus (`sum`). Returns (to-output, to-fx-send).
    #[inline]
    pub fn process(&mut self, now: f64, input: f32) -> (f32, f32) {
        self.now = now;
        if let Some(p) = self.pending
            && now >= p.due
        {
            self.pending = None;
            self.run(p.track, p.op);
        }
        let cap = if self.cycle_samples > 0 { self.cycle_samples } else { self.max_samples() };
        let mut out = 0f32;
        let mut fx = 0f32;
        for i in 0..TRACK_COUNT {
            if self.tracks[i].mode == LoopMode::Rec {
                self.tracks[i].rec.push(input);
                self.tracks[i].wave_dirty = true;
                if self.tracks[i].rec.len() >= cap {
                    self.commit_record(i, true);
                }
            }
            let (_, _, _, s0, s1) = self.region(i);
            let t = &mut self.tracks[i];
            let g = t.gain.tick(now);
            let fs = t.fx_send.tick(now);
            let Some(mut src) = t.src else { continue };
            let len = t.data.len();
            if len == 0 {
                t.src = None;
                continue;
            }
            let idx = (src.pos as usize).min(len - 1);
            let y = t.data[idx];
            if t.mode == LoopMode::Dub {
                t.data[idx] = (t.data[idx] + input).tanh();
                t.wave_dirty = true;
            }
            src.pos += 1.0;
            if src.looping {
                let (a, b) = (s0 as f64, s1 as f64);
                if src.pos >= b {
                    src.pos = a + (src.pos - b) % (b - a).max(1.0);
                }
                t.src = Some(src);
            } else if src.pos >= len as f64 {
                // one-shot pad ended (`onended`)
                t.src = None;
                t.mode = LoopMode::Stop;
            } else {
                t.src = Some(src);
            }
            let v = y * g;
            out += v;
            fx += v * fs;
        }
        self.wave_clock += 1;
        if self.wave_clock >= (self.sr * 0.1) as usize {
            self.wave_clock = 0;
            self.refresh_one_wave();
        }
        (out, fx)
    }

    fn refresh_one_wave(&mut self) {
        let Some(t) = self.tracks.iter_mut().find(|t| t.wave_dirty) else { return };
        t.wave_dirty = false;
        if t.mode == LoopMode::Rec {
            let total = t.rec.len();
            peaks_from_stream(&t.rec, total, &mut t.wave);
            t.wave_len = WAVE_BINS;
        } else if t.has_buf {
            peaks_from_data(&t.data, &mut t.wave);
            t.wave_len = WAVE_BINS;
        } else {
            t.wave_len = 0;
        }
    }

    /// The audible (trimmed) audio of every recorded track: (track, samples).
    pub fn export(&self) -> Vec<(usize, Vec<f32>)> {
        (0..self.tracks.len())
            .filter(|&i| self.tracks[i].has_buf && !self.tracks[i].is_pad)
            .map(|i| {
                let (.., s0, s1) = self.region(i);
                (i, self.tracks[i].data[s0..s1].to_vec())
            })
            .collect()
    }

    pub fn sample_rate(&self) -> f32 {
        self.sr as f32
    }

    pub fn snapshot(&self, out: &mut LoopBankSnap) {
        let now = self.now;
        for (i, t) in self.tracks.iter().enumerate() {
            let s = &mut out.tracks[i];
            s.id = i;
            s.mode = t.mode;
            s.layers = t.layers;
            s.can_undo = t.has_undo || t.layers > 0;
            s.level = t.level;
            s.trim_in = t.trim_in;
            s.trim_out = t.trim_out;
            s.pad = t.is_pad;
            s.pending = self.pending.map(|p| p.track) == Some(i);
            s.fx = t.fx_on;
            s.wave.clear();
            if t.wave_len > 0 {
                s.wave.extend_from_slice(&t.wave);
            }
            s.progress = 0.0;
            s.length = if t.has_buf { t.data.len() as f64 / self.sr } else { 0.0 };
            if t.mode == LoopMode::Rec {
                let elapsed = (now - t.rec_started).max(0.0);
                s.length = elapsed;
                s.progress = (elapsed / self.max_sec).min(1.0);
            } else if t.has_buf
                && matches!(t.mode, LoopMode::Play | LoopMode::Dub)
                && let Some(src) = t.src
                && !t.data.is_empty()
            {
                s.progress = (src.pos / t.data.len() as f64).clamp(0.0, 1.0);
            }
        }
        let b = self.bar_sec();
        out.bpm = self.bpm;
        out.bar_phase = match self.clock_origin {
            Some(o) if b > 0.0 => (((now - o) % b) + b) % b / b,
            _ => 0.0,
        };
        out.cycle_bars =
            if self.cycle_samples > 0 { self.cycle_samples as f64 / self.bar_samples() as f64 } else { 0.0 };
        out.pending_track = self.pending.map(|p| p.track as i32).unwrap_or(-1);
        out.pad_armed = self.tracks[PAD_TRACK].is_pad;
    }
}

/// `peaksFromChunks` — streaming bins while recording.
fn peaks_from_stream(data: &[f32], total: usize, out: &mut [f32; WAVE_BINS]) {
    out.fill(0.0);
    if total < 1 {
        return;
    }
    let bin = total as f64 / WAVE_BINS as f64;
    let (mut idx, mut acc, mut peak) = (0usize, 0f64, 0f32);
    for &x in data {
        let a = x.abs();
        if a > peak {
            peak = a;
        }
        acc += 1.0;
        if acc >= bin && idx < WAVE_BINS {
            out[idx] = peak;
            idx += 1;
            peak = 0.0;
            acc = 0.0;
        }
    }
    if idx < WAVE_BINS {
        out[idx] = peak;
    }
}

/// `peaksFromData`.
fn peaks_from_data(data: &[f32], out: &mut [f32; WAVE_BINS]) {
    out.fill(0.0);
    let len = data.len();
    if len < 1 {
        return;
    }
    let bin = len as f64 / WAVE_BINS as f64;
    for (i, o) in out.iter_mut().enumerate() {
        let a = (i as f64 * bin) as usize;
        let b = (((i + 1) as f64 * bin) as usize).max(a + 1).min(len);
        *o = data[a.min(len)..b].iter().fold(0f32, |m, v| m.max(v.abs()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 8000.0;

    fn run(l: &mut Looper, t: &mut f64, secs: f64, input: f32) -> f32 {
        let mut peak = 0f32;
        for _ in 0..(secs * SR as f64) as usize {
            let (o, _) = l.process(*t, input);
            peak = peak.max(o.abs());
            *t += 1.0 / SR as f64;
        }
        peak
    }

    #[test]
    fn record_snaps_to_bar_and_plays() {
        let mut l = Looper::new(SR);
        let mut t = 0.0;
        l.set_now(t);
        l.punch(0); // first punch: clock origin = now → immediate
        let mut snap = LoopBankSnap::default();
        l.snapshot(&mut snap);
        assert_eq!(snap.tracks[0].mode, LoopMode::Rec);
        run(&mut l, &mut t, 3.1, 0.5); // bar @72bpm = 3.333 s
        l.set_now(t);
        l.punch(0); // takes effect immediately; the take snaps to a whole bar
        l.snapshot(&mut snap);
        assert!(!snap.tracks[0].pending);
        assert_eq!(snap.tracks[0].mode, LoopMode::Play);
        assert!((snap.tracks[0].length - 10.0 / 3.0).abs() < 0.01, "len {}", snap.tracks[0].length);
        assert!((snap.cycle_bars - 1.0).abs() < 1e-9);
        // Plays back what was recorded (level 0.9 initial gain).
        let peak = run(&mut l, &mut t, 0.5, 0.0);
        assert!((peak - 0.45).abs() < 0.01, "{peak}");
    }

    #[test]
    fn dub_and_undo() {
        let mut l = Looper::new(SR);
        let mut t = 0.0;
        l.set_now(t);
        l.punch(0);
        run(&mut l, &mut t, 3.4, 0.2);
        l.set_now(t);
        l.punch(0);
        run(&mut l, &mut t, 3.4, 0.0);
        let mut snap = LoopBankSnap::default();
        l.snapshot(&mut snap);
        assert_eq!(snap.tracks[0].mode, LoopMode::Play);
        l.set_now(t);
        l.punch(0); // → dub, immediately
        run(&mut l, &mut t, 3.4, 0.3);
        l.snapshot(&mut snap);
        assert_eq!(snap.tracks[0].mode, LoopMode::Dub);
        l.undo(0);
        l.snapshot(&mut snap);
        assert_eq!(snap.tracks[0].mode, LoopMode::Play);
        l.undo(0); // nothing left to undo → clears
        l.snapshot(&mut snap);
        assert_eq!(snap.tracks[0].mode, LoopMode::Idle);
        assert_eq!(snap.cycle_bars, 0.0);
    }

    #[test]
    fn freeze_pad_is_one_shot() {
        let mut l = Looper::new(SR);
        let mut t = 0.0;
        l.set_now(t);
        l.punch(1);
        run(&mut l, &mut t, 3.4, 0.4);
        l.set_now(t);
        l.stop(None);
        l.freeze_pad();
        l.fire_pad(true);
        let mut snap = LoopBankSnap::default();
        l.snapshot(&mut snap);
        assert!(snap.pad_armed);
        assert_eq!(snap.tracks[PAD_TRACK].mode, LoopMode::Play);
        run(&mut l, &mut t, 3.5, 0.0);
        l.snapshot(&mut snap);
        assert_eq!(snap.tracks[PAD_TRACK].mode, LoopMode::Stop);
    }

    #[test]
    fn recording_length_can_be_raised_for_the_desktop() {
        assert_eq!(Looper::new(SR).max_samples(), (SR as f64 * 24.0) as usize);
        set_max_seconds(120.0);
        let l = Looper::new(SR);
        set_max_seconds(24.0);
        assert_eq!(l.max_samples(), (SR as f64 * 120.0) as usize);
        assert_eq!(Looper::new(SR).max_samples(), (SR as f64 * 24.0) as usize);
        set_max_seconds(1.0); // never below the default
        assert_eq!(Looper::new(SR).max_samples(), (SR as f64 * 24.0) as usize);
    }
}
