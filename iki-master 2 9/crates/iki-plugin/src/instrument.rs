//! The instrument inside the plugin, independent of the plugin API: the
//! kernel's engine rendered from the host's audio thread, and the same
//! [`Controller`] the apps use — so pads, gate, ornaments, legato, sustain and
//! MIDI mapping behave exactly as in the app — driven by host MIDI, parameter
//! changes and gestures from the editor.
//!
//! Everything called per block is allocation-free once the samples are loaded.

use std::time::Duration;

use iki_core::audio::{Renderer, external};
use iki_core::controller::{Controller, Key, Ornament};
use iki_core::engine::{Cmd, SharedSnapshot, VoiceMode};
use iki_core::params::set_param;
use iki_core::shaku::fingering::Voicing;

/// Editor → audio thread.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    /// Pad pressed: atari/koro when already blowing, otherwise start blowing.
    HitStart(usize, Voicing),
    /// Note fader pressed.
    SlideStart(usize, Voicing),
    /// Note fader moved.
    ParkNote(usize, Voicing),
    /// Pad / note fader released.
    SlideEnd,
    ToggleGate,
    Muraiki,
    FoliStep(usize),
    KeyDown(Key),
    KeyUp(Key),
    /// Editor lost focus: release held keys.
    ReleaseKeys,
}

/// Audio thread → editor: the performance state the panel shows, copied
/// every block. Meri, iki and the octave are here (not just in the
/// parameters) because MIDI moves them: pitch bend, velocity, note range.
#[derive(Clone, Copy, Debug, Default)]
pub struct Perf {
    pub kari_note: usize,
    pub meri_note: usize,
    pub meri_lane: bool,
    pub blowing: bool,
    pub latched: bool,
    pub otsu: bool,
    pub meri: f64,
    pub iki: f64,
    pub lean: f64,
    pub ornament: Option<Ornament>,
    pub muraiki: bool,
    pub loop_track: usize,
    /// Host tempo (0 before the first block).
    pub bpm: f64,
    pub playing: bool,
    /// Frames per host block.
    pub block: usize,
}

/// Parameter ids fit in this many slots (see `iki_core::params`).
pub const PARAM_SLOTS: usize = 80;

pub struct Instrument {
    pub ctl: Controller,
    renderer: Renderer,
    /// Interleaved stereo scratch for one render call.
    scratch: Vec<f32>,
    /// The last value applied per parameter id (NaN: never).
    applied: [f64; PARAM_SLOTS],
    /// Engine state for the editor (meters, Foli steps, …).
    pub snapshot: SharedSnapshot,
    playing: bool,
    block: usize,
}

impl Instrument {
    /// A new engine at the host's rate. Blocks until the samples are decoded,
    /// so an offline render (freeze, export) starts with sound. Returns the
    /// output-sample stream for the editor's spectrogram too.
    pub fn new(sample_rate: f32, max_block: usize) -> (Self, rtrb::Consumer<(f32, f32)>) {
        let ext = external(sample_rate, format!("host · {sample_rate:.0} Hz"));
        let snapshot = ext.handle.snapshot.clone();
        let mut ctl = Controller::new(ext.handle, VoiceMode::Sampler, || {});
        ctl.wait_for_samples(Duration::from_secs(20));
        let inst = Self {
            ctl,
            renderer: ext.renderer,
            scratch: vec![0.0; max_block.clamp(64, 1 << 14) * 2],
            applied: [f64::NAN; PARAM_SLOTS],
            snapshot,
            playing: false,
            block: 0,
        };
        (inst, ext.scope)
    }

    /// A parameter's host value. Applied only when it changed, so a meri bend,
    /// velocity-driven iki or the octave a MIDI note chose stay until the knob
    /// (or its automation) moves.
    pub fn param(&mut self, id: i32, value: f64) {
        let Some(slot) = self.applied.get_mut(id as usize) else { return };
        if *slot != value {
            *slot = value;
            set_param(&mut self.ctl, id, value);
        }
    }

    /// Host transport, once per block: tempo for the synced delay / LFO / Foli,
    /// and the beat position (while playing) to lock Foli to the bars.
    pub fn host_clock(&mut self, bpm: Option<f64>, beat: Option<f64>, playing: bool) {
        self.playing = playing;
        let Some(bpm) = bpm.filter(|b| b.is_finite() && *b > 1.0) else { return };
        self.ctl.host_bpm = Some(bpm);
        self.ctl.send(Cmd::HostClock { bpm, beat: if playing { beat } else { None } });
    }

    pub fn gesture(&mut self, g: Gesture) {
        let c = &mut self.ctl;
        match g {
            Gesture::HitStart(i, lane) => c.on_hit_start(i.min(5), lane),
            Gesture::SlideStart(i, lane) => c.on_slide_start(i.min(5), lane),
            Gesture::ParkNote(i, lane) => c.park_note(i.min(5), lane),
            Gesture::SlideEnd => c.on_slide_end(),
            Gesture::ToggleGate => c.toggle_gate(),
            Gesture::Muraiki => c.shoot_muraiki(),
            Gesture::FoliStep(i) => c.foli(Cmd::FoliToggleStep(i.min(11))),
            Gesture::KeyDown(k) => c.key_down(k),
            Gesture::KeyUp(k) => c.key_up(k),
            Gesture::ReleaseKeys => c.release_keys(),
        }
    }

    /// One raw MIDI message from the host (notes, pitch wheel → bend, CC 1 → yuri, CC 7/11/aftertouch → iki, CC 74 → meri, sustain).
    pub fn midi(&mut self, bytes: &[u8]) {
        self.ctl.midi_in(bytes);
    }

    /// Stop everything (transport stop / reset): no held note, no latched drone.
    pub fn release_all(&mut self) {
        self.ctl.release_keys();
        if self.ctl.blowing {
            self.ctl.blow(Some(false));
        }
    }

    /// Render `left`/`right` (same length). Changes made since the last call
    /// (parameters, MIDI, gestures) take effect at the first sample.
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.ctl.poll();
        let n = left.len().min(right.len());
        self.block = self.block.max(n);
        let mut done = 0;
        while done < n {
            let m = (n - done).min(self.scratch.len() / 2);
            let buf = &mut self.scratch[..m * 2];
            self.renderer.render(buf, 2);
            for (i, fr) in buf.as_chunks::<2>().0.iter().enumerate() {
                left[done + i] = fr[0];
                right[done + i] = fr[1];
            }
            done += m;
        }
    }

    /// Call at the end of each host block.
    pub fn end_block(&mut self) {
        self.renderer.set_period(self.block);
        self.block = 0;
    }

    pub fn perf(&self) -> Perf {
        let c = &self.ctl;
        Perf {
            kari_note: c.kari_note,
            meri_note: c.meri_note,
            meri_lane: c.voicing == Voicing::Meri,
            blowing: c.blowing,
            latched: c.latched,
            otsu: c.otsu,
            meri: c.meri,
            iki: c.iki,
            lean: c.lean,
            ornament: c.ornament,
            muraiki: c.muraiki_flash.is_some(),
            loop_track: c.loop_track,
            bpm: c.host_bpm.unwrap_or(0.0),
            playing: self.playing,
            block: self.renderer_period(),
        }
    }

    fn renderer_period(&self) -> usize {
        self.ctl.audio.period.load(std::sync::atomic::Ordering::Relaxed) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iki_core::params::*;

    const SR: f32 = 48000.0;

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    fn block(inst: &mut Instrument, frames: usize) -> (Vec<f32>, Vec<f32>) {
        let (mut l, mut r) = (vec![0f32; frames], vec![0f32; frames]);
        for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
            inst.render(a, b);
            inst.end_block();
        }
        (l, r)
    }

    /// Zero-crossing pitch estimate (the sampler voice is close to a sine at otsu).
    fn pitch(x: &[f32]) -> f32 {
        let min_lag = (SR / 1400.0) as usize;
        let max_lag = (SR / 120.0) as usize;
        let n = x.len() - max_lag;
        let mut best = (0, f32::MIN);
        for lag in min_lag..max_lag {
            let s: f32 = (0..n).map(|i| x[i] * x[i + lag]).sum();
            if s > best.1 {
                best = (lag, s);
            }
        }
        SR / best.0 as f32
    }

    #[test]
    fn midi_notes_play_and_stop() {
        let (mut inst, _scope) = Instrument::new(SR, 512);
        let (l, _) = block(&mut inst, 12000);
        assert!(rms(&l) < 1e-4, "silent before a note");
        inst.midi(&[0x90, 62, 100]); // D4 = Ro otsu
        let (l, r) = block(&mut inst, 48000);
        assert!(rms(&l[24000..]) > 0.02 && rms(&r[24000..]) > 0.02, "sounds: {}", rms(&l[24000..]));
        assert!(inst.ctl.blowing);
        let hz = pitch(&l[24000..36000]);
        assert!((hz / 293.66 - 1.0).abs() < 0.03 || (hz / 146.83 - 1.0).abs() < 0.03, "Ro at {hz} Hz");
        inst.midi(&[0x80, 62, 0]);
        let _ = block(&mut inst, 96000);
        let (l, _) = block(&mut inst, 24000);
        assert!(rms(&l) < 2e-3, "released: {}", rms(&l));
    }

    #[test]
    fn parameters_apply_on_change_only() {
        let (mut inst, _scope) = Instrument::new(SR, 256);
        inst.param(P_IKI, 0.2);
        assert_eq!(inst.ctl.iki, 0.2);
        inst.midi(&[0x90, 62, 127]); // velocity sets iki
        let v = inst.ctl.iki;
        assert!(v > 0.9);
        inst.param(P_IKI, 0.2); // unchanged host value: the MIDI-set breath stays
        assert_eq!(inst.ctl.iki, v);
        inst.param(P_IKI, 0.3); // the knob moved
        assert_eq!(inst.ctl.iki, 0.3);
    }

    #[test]
    fn voice_switch_and_gestures() {
        let (mut inst, _scope) = Instrument::new(SR, 512);
        for bore in [1.0, 2.0, 0.0, 2.0, 1.0, 0.0] {
            inst.param(P_VOICE, bore);
            inst.gesture(Gesture::HitStart(0, Voicing::Kari));
            let (l, _) = block(&mut inst, 48000);
            assert!(rms(&l[24000..]) > 0.005, "voice {bore}: {}", rms(&l[24000..]));
            inst.gesture(Gesture::SlideEnd);
            let _ = block(&mut inst, 96000);
        }
        inst.gesture(Gesture::ToggleGate);
        let _ = block(&mut inst, 4800);
        assert!(inst.perf().latched);
        inst.release_all();
        assert!(!inst.perf().blowing);
    }

    #[test]
    fn follows_host_tempo() {
        let (mut inst, _scope) = Instrument::new(SR, 512);
        inst.param(P_FOLI_ON, 1.0);
        inst.host_clock(Some(150.0), Some(0.0), true);
        let _ = block(&mut inst, 512);
        assert_eq!(inst.ctl.bpm_readout(), "150 bpm");
        // Foli plays on the host grid
        let mut steps = Vec::new();
        let mut beat = 0.0;
        for _ in 0..200 {
            inst.host_clock(Some(150.0), Some(beat), true);
            let _ = block(&mut inst, 480);
            beat += 480.0 / SR as f64 * 150.0 / 60.0;
            if let Ok(s) = inst.snapshot.lock() {
                steps.push(s.foli_step);
            }
        }
        assert!(steps.iter().any(|s| s.is_some()), "foli ran");
    }
}
