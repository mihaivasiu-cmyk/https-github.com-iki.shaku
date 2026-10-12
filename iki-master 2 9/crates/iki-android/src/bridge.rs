//! The platform-independent instrument shell shared by the Android (JNI) and iOS
//! (C ABI) bridges: one [`Iki`] per engine, ticked once per display frame, with a
//! packed state array, readout strings, curves and a spectrogram column. All
//! behaviour lives in [`iki_core::Controller`]; this only packages it.

use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use iki_core::controller::{Controller, Key};
use iki_core::engine::VoiceMode;
use iki_core::shaku::env_shape::{env_level, env_seconds, filt_cutoff, filt_mag_curve};
use iki_core::shaku::fingering::Voicing;
use iki_core::shaku::looper::{LoopMode, TRACK_COUNT};
use iki_core::shaku::midi::{self, ClipPlayer, LiveEvent};
use iki_core::spectrum::{Analyser, BINS, FFT_SIZE};

use crate::params::*;

/// Rows per channel in one spectrogram column (matches the desktop CRT).
pub const SPEC_ROWS: usize = 72;

/// A playing .mid clip: events for the instrument, raw bytes for MIDI out.
struct Clip {
    _player: ClipPlayer,
    rx: Receiver<(LiveEvent, Option<[u8; 3]>)>,
    name: String,
    until: Instant,
}

pub struct Iki {
    pub ctl: Controller,
    ring_l: Vec<f32>,
    ring_r: Vec<f32>,
    w: usize,
    an_l: Analyser,
    an_r: Analyser,
    clip: Option<Clip>,
    /// Clip bytes waiting for `midi_tx` (sent out even when Tx echo is off, like the desktop bus).
    clip_out: Vec<u8>,
    last_tick: Instant,
    frame_ms: f32,
}

impl Iki {
    /// An instrument on a device-backed audio stream (Android, desktop tests).
    #[cfg(not(target_os = "ios"))]
    pub fn new(silent: bool, voice: VoiceMode) -> Self {
        let audio = iki_core::audio::start(&iki_core::audio::AudioOptions { silent, ..Default::default() });
        Self::with_audio(audio, voice)
    }

    /// An instrument on any audio handle, e.g. [`iki_core::audio::external`] where the
    /// platform owns the audio callback (iOS).
    pub fn with_audio(audio: iki_core::audio::AudioHandle, voice: VoiceMode) -> Self {
        Self {
            ctl: Controller::new(audio, voice, || {}),
            ring_l: vec![0.0; FFT_SIZE],
            ring_r: vec![0.0; FFT_SIZE],
            w: 0,
            an_l: Analyser::new(0.72),
            an_r: Analyser::new(0.72),
            clip: None,
            clip_out: Vec::new(),
            last_tick: Instant::now(),
            frame_ms: 16.7,
        }
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32().min(0.25);
        self.last_tick = now;
        self.frame_ms = self.frame_ms * 0.9 + dt * 1000.0 * 0.1;
        if let Some(c) = &self.clip {
            while let Ok((ev, bytes)) = c.rx.try_recv() {
                if let Some(b) = bytes {
                    self.clip_out.extend_from_slice(&b);
                }
                self.ctl.live(ev);
            }
            if now > c.until {
                self.clip = None;
            }
        }
        let (rl, rr, w) = (&mut self.ring_l, &mut self.ring_r, &mut self.w);
        self.ctl.tick(|l, r| {
            rl[*w] = l;
            rr[*w] = r;
            *w = (*w + 1) % FFT_SIZE;
        });
        let w = self.w;
        let (l, r) = (&self.ring_l, &self.ring_r);
        self.an_l.update((0..FFT_SIZE).map(|i| l[(w + i) % FFT_SIZE]));
        self.an_r.update((0..FFT_SIZE).map(|i| r[(w + i) % FFT_SIZE]));
    }

    /// One spectrogram column: `SPEC_ROWS` values for L then R, top = high.
    pub fn spectrum_column(&self, out: &mut [f32]) {
        let rate = self.ctl.audio.sample_rate;
        let log_bin = |t: f32| {
            let f0 = 55.0;
            let f1 = (rate * 0.48).min(12000.0);
            let f = f0 * (f1 / f0).powf(t);
            ((f / (rate * 0.5)) * BINS as f32).round().clamp(0.0, (BINS - 1) as f32) as usize
        };
        for (ch, bytes) in [&self.an_l.bytes, &self.an_r.bytes].into_iter().enumerate() {
            for y in 0..SPEC_ROWS {
                let t = 1.0 - y as f32 / (SPEC_ROWS - 1) as f32;
                let a = log_bin((t - 0.012).max(0.0));
                let b = log_bin((t + 0.012).min(1.0)).max(a);
                let m = bytes[a..=b].iter().copied().max().unwrap_or(0);
                if let Some(o) = out.get_mut(ch * SPEC_ROWS + y) {
                    *o = m as f32 / 255.0;
                }
            }
        }
    }

    /// Packed per-frame state (see `STATE_*` indices in `params.rs`).
    pub fn state(&mut self, out: &mut [f32]) {
        let problem = self.ctl.audio_problem().is_some();
        let c = &self.ctl;
        let s = &c.snap;
        let mut v = [0f32; STATE_LEN];
        v[STATE_BLOWING as usize] = c.blowing as u8 as f32;
        v[STATE_LATCHED as usize] = c.latched as u8 as f32;
        v[STATE_NOTE as usize] = c.note() as f32;
        v[STATE_MERI_LANE as usize] = (c.voicing == Voicing::Meri) as u8 as f32;
        v[STATE_KARI_NOTE as usize] = c.kari_note as f32;
        v[STATE_MERI_NOTE as usize] = c.meri_note as f32;
        v[STATE_OTSU as usize] = c.otsu as u8 as f32;
        v[STATE_HZ as usize] = c.heard_hz() as f32;
        v[STATE_BREATH as usize] = s.breath;
        v[STATE_PEAK_L as usize] = s.peak_l;
        v[STATE_PEAK_R as usize] = s.peak_r;
        v[STATE_COMP_GR as usize] = s.comp_gr;
        v[STATE_CUTOFF as usize] = s.cutoff_hz;
        v[STATE_LATENCY_MS as usize] = c.audio.latency_us.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0;
        v[STATE_LOADED as usize] = s.loaded as f32;
        match c.ornament {
            Some(o) => {
                v[STATE_ORN_LANE as usize] = (o.lane == Voicing::Meri) as u8 as f32;
                v[STATE_ORN_INDEX as usize] = o.index as f32;
                v[STATE_ORN_KORO as usize] = o.koro as u8 as f32;
            }
            None => v[STATE_ORN_INDEX as usize] = -1.0,
        }
        v[STATE_MURAIKI as usize] = c.muraiki_flash.is_some() as u8 as f32;
        v[STATE_LOOP_TRACK as usize] = c.loop_track as f32;
        let b = &s.loop_bank;
        v[STATE_BPM as usize] = b.bpm as f32;
        v[STATE_BAR_PHASE as usize] = b.bar_phase as f32;
        v[STATE_CYCLE_BARS as usize] = b.cycle_bars as f32;
        v[STATE_PAD_ARMED as usize] = b.pad_armed as u8 as f32;
        for (i, t) in b.tracks.iter().enumerate().take(TRACK_COUNT) {
            let o = STATE_TRACKS as usize + i * TRACK_STRIDE as usize;
            v[o] = match t.mode {
                LoopMode::Idle => 0.0,
                LoopMode::Rec => 1.0,
                LoopMode::Play => 2.0,
                LoopMode::Dub => 3.0,
                LoopMode::Stop => 4.0,
            };
            v[o + 1] = t.progress as f32;
            v[o + 2] = t.length as f32;
            v[o + 3] = c.loop_levels[i] as f32;
            v[o + 4] = t.pad as u8 as f32;
            v[o + 5] = t.pending as u8 as f32;
            v[o + 6] = t.fx as u8 as f32;
            v[o + 7] = t.can_undo as u8 as f32;
            v[o + 8] = t.trim_in as f32;
            v[o + 9] = t.trim_out as f32;
        }
        v[STATE_FOLI_ON as usize] = s.foli_on as u8 as f32;
        v[STATE_FOLI_STEP as usize] = s.foli_step.map(|x| x as f32).unwrap_or(-1.0);
        v[STATE_FOLI_MASK as usize] =
            s.foli_mask.iter().enumerate().fold(0u32, |m, (i, on)| m | ((*on as u32) << i)) as f32;
        v[STATE_BORE as usize] = s.voice_mode as f32;
        v[STATE_PROBLEM as usize] = problem as u8 as f32;
        v[STATE_PHASER_ROUTED as usize] = s.phaser_routed as u8 as f32;
        v[STATE_CLOCK_LATE as usize] = c.clock_readout().1 as u8 as f32;
        v[STATE_CLIP as usize] = self.clip.is_some() as u8 as f32;
        let n = out.len().min(STATE_LEN);
        out[..n].copy_from_slice(&v[..n]);
    }

    pub fn text(&mut self, which: i32) -> String {
        let c = &self.ctl;
        match which {
            TEXT_KANA => c.note_name().kana,
            TEXT_NAME => c.note_name().name,
            TEXT_PITCH => c.pitch_readout(),
            TEXT_STATUS => c.status_readout(1000.0 / self.frame_ms.max(1.0)),
            TEXT_PROBLEM => self.ctl.audio_problem().unwrap_or_default(),
            TEXT_HOLES => c.holes_readout(),
            TEXT_PLAY => c.play_readout(),
            TEXT_ENGINE => c.engine_readout(),
            TEXT_CLOCK => c.clock_readout().0,
            TEXT_DELAY => c.delay_readout(),
            TEXT_LFO_HZ => c.lfo_readout(),
            TEXT_BPM => c.bpm_readout(),
            TEXT_CLIP => self.clip.as_ref().map(|c| c.name.clone()).unwrap_or_default(),
            TEXT_BACKEND => {
                let skipped = c.audio.status.lock().map(|s| s.skipped.clone()).unwrap_or_default();
                let mut s = format!("Audio output: {}", c.audio.backend);
                if !skipped.is_empty() {
                    s.push_str(&format!("\nSkipped: {}", skipped.join("\n  ")));
                }
                s
            }
            _ => String::new(),
        }
    }

    /// Envelope (amp/filter) or filter-response curve, as the desktop scopes
    /// draw them: `out[..n-1]` is the curve, `out[n-1]` the guide line.
    /// Envelopes: level 0…1.15 (guide = sustain). Filter: 0…1 of the dB range
    /// (guide = 0 dB).
    pub fn curve(&self, which: i32, out: &mut [f32]) {
        let Some((guide, pts)) = out.split_last_mut() else { return };
        let n = pts.len();
        if n < 2 {
            return;
        }
        let fx = &self.ctl.fx;
        match which {
            CURVE_AMP_ENV | CURVE_FILT_ENV => {
                let amp = which == CURVE_AMP_ENV;
                let e = if amp {
                    env_seconds(fx.env_a, fx.env_d, fx.env_s, fx.env_r)
                } else {
                    env_seconds(fx.fenv_a, fx.fenv_d, fx.fenv_s, fx.fenv_r)
                };
                let (shape, lp) = if amp { (fx.env_shape, fx.env_loop) } else { (fx.fenv_shape, fx.fenv_loop) };
                let span = if lp { (e.a + e.d + e.r * 0.52) * 2.0 } else { e.a + e.d + e.r.max(0.18) };
                for (i, o) in pts.iter_mut().enumerate() {
                    let mut t = i as f64 / (n - 1) as f64 * span;
                    let y = if lp {
                        let cycle = e.a + e.d + e.r * 0.52;
                        t -= (t / cycle.max(0.001)).floor() * cycle;
                        env_level(t, e, shape, true)
                    } else if t <= e.a + e.d + 0.08 {
                        env_level(t, e, shape, false)
                    } else {
                        e.s * (1.0 - ((t - e.a - e.d - 0.08) / e.r.max(0.001)).min(1.0)).max(0.0)
                    };
                    *o = y.clamp(0.0, 1.15) as f32;
                }
                *guide = e.s as f32;
            }
            CURVE_FILTER => {
                let cut = filt_cutoff(fx.filt_type, fx.filt_cut);
                let mag = filt_mag_curve(
                    fx.filt_type,
                    cut,
                    0.35 + fx.filt_res * 11.5,
                    iki_core::shaku::env_shape::filt_gain_db(fx.filt_res),
                    n,
                );
                let db: Vec<f64> = mag
                    .iter()
                    .map(|m| 20.0 * m.max(1e-6).log10())
                    .map(|v| if v.is_finite() { v } else { -120.0 })
                    .collect();
                let lo = (db.iter().cloned().fold(0f64, f64::min) - 2.0).min(-24.0);
                let hi = (db.iter().cloned().fold(0f64, f64::max) + 2.0).max(12.0);
                let span = (hi - lo).max(1.0);
                for (o, d) in pts.iter_mut().zip(db) {
                    *o = ((d - lo) / span) as f32;
                }
                *guide = ((0.0 - lo) / span) as f32;
            }
            _ => {}
        }
    }

    /// Parse a Standard MIDI File and play it through the instrument (and MIDI out).
    pub fn play_clip(&mut self, bytes: &[u8], name: &str) -> Result<(), String> {
        let clip = midi::parse_smf(bytes, name)?;
        let until = Instant::now() + Duration::from_secs_f64((clip.duration + 0.2).max(0.4));
        let name = clip.name.clone();
        let (tx, rx) = mpsc::channel();
        let player = ClipPlayer::play(clip, move |ev, bytes| {
            let _ = tx.send((ev, bytes));
        });
        self.clip = Some(Clip { _player: player, rx, name, until });
        Ok(())
    }

    /// Bytes for MIDI out this frame: the Tx echo of what is played, then clip notes.
    pub fn midi_out(&mut self, tx: bool) -> Vec<u8> {
        let echo = self.ctl.midi_tx();
        let mut out = Vec::new();
        if tx {
            for m in echo {
                out.extend_from_slice(&m);
            }
            out.append(&mut self.clip_out);
        } else {
            self.clip_out.clear();
        }
        out
    }

    pub fn key(&mut self, name: &str, down: bool) -> bool {
        let Some(k) = Key::from_name(name) else { return false };
        if down {
            self.ctl.key_down(k)
        } else {
            self.ctl.key_up(k)
        }
        true
    }
}
