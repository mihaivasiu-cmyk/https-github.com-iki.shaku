//! Iki as an instrument plugin — VST3 (Ableton Live, Cubase, …) and CLAP
//! (Bitwig, Reaper, …) — on [nih-plug](https://github.com/robbert-vdh/nih-plug).
//!
//! The same kernel and controller as the desktop and Android apps, played from
//! the host's MIDI (notes, pitch wheel → bend, mod wheel → yuri, volume / expression / aftertouch →
//! iki, sustain) or from the panel. Every panel control is a host parameter.
//! The looper is left out — the host does looping — and the synced delay,
//! LFO and Foli drums follow the host's tempo and bars.

mod editor;
pub mod instrument;
pub mod params;

use std::num::NonZeroU32;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use nih_plug::midi::MidiResult;
use nih_plug::prelude::*;

use instrument::{Gesture, Instrument, Perf};
use params::IkiParams;

/// State shared between the audio thread and the editor. The audio thread only
/// ever `try_lock`s, so the editor can never stall it.
pub struct Shared {
    pub perf: Mutex<Perf>,
    /// The current engine's snapshot (replaced when the host re-initialises).
    pub snapshot: Mutex<Option<iki_core::engine::SharedSnapshot>>,
    /// Output samples for the editor's spectrogram (consumer end).
    pub scope: Mutex<Option<rtrb::Consumer<(f32, f32)>>>,
    /// Editor → audio gestures (producer end).
    pub gestures: Mutex<rtrb::Producer<Gesture>>,
    pub sample_rate: Mutex<f32>,
}

pub struct IkiPlugin {
    params: Arc<IkiParams>,
    shared: Arc<Shared>,
    gestures: rtrb::Consumer<Gesture>,
    inst: Option<Instrument>,
    applied_mask: u32,
}

impl Default for IkiPlugin {
    fn default() -> Self {
        let (tx, rx) = rtrb::RingBuffer::new(1024);
        Self {
            params: Arc::new(IkiParams::default()),
            shared: Arc::new(Shared {
                perf: Mutex::new(Perf::default()),
                snapshot: Mutex::new(None),
                scope: Mutex::new(None),
                gestures: Mutex::new(tx),
                sample_rate: Mutex::new(48000.0),
            }),
            gestures: rx,
            inst: None,
            applied_mask: u32::MAX,
        }
    }
}

impl Plugin for IkiPlugin {
    const NAME: &'static str = "Iki";
    const VENDOR: &'static str = "Iki";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone(), self.shared.clone())
    }

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        let (inst, scope) = Instrument::new(config.sample_rate, config.max_buffer_size as usize);
        if let Ok(mut s) = self.shared.snapshot.lock() {
            *s = Some(inst.snapshot.clone());
        }
        if let Ok(mut s) = self.shared.scope.lock() {
            *s = Some(scope);
        }
        if let Ok(mut s) = self.shared.sample_rate.lock() {
            *s = config.sample_rate;
        }
        self.inst = Some(inst);
        self.applied_mask = u32::MAX;
        true
    }

    fn reset(&mut self) {
        if let Some(inst) = &mut self.inst {
            inst.release_all();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let Some(inst) = &mut self.inst else { return ProcessStatus::Normal };

        // Parameters (applied when they change), a restored Foli pattern,
        // the editor's gestures, then the host clock.
        for p in &self.params.list {
            inst.param(p.id, p.value());
        }
        let mask = self.params.foli_mask.load(Ordering::Relaxed);
        if mask != self.applied_mask && mask != u32::MAX {
            inst.ctl.foli(iki_core::engine::Cmd::FoliMask(mask as u16));
            self.applied_mask = mask;
        }
        while let Ok(g) = self.gestures.pop() {
            inst.gesture(g);
        }
        let t = context.transport();
        inst.host_clock(t.tempo, t.pos_beats(), t.playing);

        // Render between events so every note lands on its sample.
        let frames = buffer.samples();
        let out = buffer.as_slice();
        let (left, rest) = out.split_at_mut(1);
        let (left, right) = (&mut *left[0], &mut *rest[0]);
        let mut pos = 0;
        while let Some(ev) = context.next_event() {
            let at = (ev.timing() as usize).min(frames);
            if at > pos {
                inst.render(&mut left[pos..at], &mut right[pos..at]);
                pos = at;
            }
            if let Some(MidiResult::Basic(bytes)) = ev.as_midi() {
                inst.midi(&bytes);
            }
        }
        if pos < frames {
            inst.render(&mut left[pos..], &mut right[pos..]);
        }
        inst.end_block();

        if let Ok(mut p) = self.shared.perf.try_lock() {
            *p = inst.perf();
        }
        // Reverb and delay tails ring on after the last note.
        ProcessStatus::KeepAlive
    }
}

impl ClapPlugin for IkiPlugin {
    const CLAP_ID: &'static str = "org.iki.shakuhachi";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A playable 1.8 shakuhachi");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] =
        &[ClapFeature::Instrument, ClapFeature::Synthesizer, ClapFeature::Stereo];
}

impl Vst3Plugin for IkiPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"IkiShakuhachi1.8";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nih_export_clap!(IkiPlugin);
nih_export_vst3!(IkiPlugin);
