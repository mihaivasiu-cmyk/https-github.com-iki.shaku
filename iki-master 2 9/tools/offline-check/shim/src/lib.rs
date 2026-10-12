//! Offline harness: compiles the real iki-core sources that need no external
//! crates (DSP, engine, looper, Foli, samples, MIDI parsing) against a tiny
//! stand-in for `hound`, so the end-to-end audio tests run with no network and
//! no audio device. Not used by the app; see TESTING.md.

#[path = "../../../../crates/iki-core/src/dsp/mod.rs"]
pub mod dsp;
#[path = "../../../../crates/iki-core/src/shaku/mod.rs"]
pub mod shaku;
#[path = "../../../../crates/iki-core/src/engine.rs"]
pub mod engine;

/// The offline renderer from `audio.rs` (the rest of that file needs cpal / rtrb).
pub mod audio {
    use crate::engine::Engine;

    pub fn render_offline(engine: &mut Engine, seconds: f64, out: &mut Vec<f32>) {
        let frames = (seconds * engine.sample_rate() as f64) as usize;
        let mut buf = vec![0f32; 256 * 2];
        let mut done = 0;
        while done < frames {
            let n = 256.min(frames - done);
            engine.render(&mut buf[..n * 2], 2, |_, _| {});
            out.extend_from_slice(&buf[..n * 2]);
            done += n;
        }
    }
}
