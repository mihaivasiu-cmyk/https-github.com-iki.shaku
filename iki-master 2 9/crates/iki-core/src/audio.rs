//! Audio I/O: runs the [`Engine`] inside a cpal output stream, or — when no
//! device is available — on a real-time paced "null" thread so the instrument
//! and all its indicators still work silently. On Android the output is a
//! native AAudio stream ([`aaudio`]); cpal is the fallback there.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::engine::{Cmd, Engine, SharedSnapshot, Snapshot};

#[cfg(target_os = "android")]
mod aaudio;
#[cfg(feature = "device")]
mod device;
#[cfg(feature = "device")]
pub use device::list_devices;

pub const SCOPE_CAP: usize = 1 << 15;

pub struct AudioHandle {
    pub cmds: rtrb::Producer<Cmd>,
    pub scope: rtrb::Consumer<(f32, f32)>,
    pub snapshot: SharedSnapshot,
    /// Human-readable "host · device · rate", or why we are silent.
    pub backend: String,
    /// True when no device could be opened (engine runs on a silent clock).
    pub silent: bool,
    /// Why earlier hosts/devices were skipped, and runtime stream errors.
    pub status: Arc<Mutex<AudioStatus>>,
    /// Incremented by every audio callback — lets the UI detect a stalled stream.
    pub callbacks: Arc<AtomicU64>,
    /// Output latency reported by the backend (callback → playback), µs; 0 = unknown.
    pub latency_us: Arc<AtomicU64>,
    /// Frames per callback of the last callback.
    pub period: Arc<AtomicU64>,
    /// Buffer underruns reported by the backend (glitches, not fatal).
    pub xruns: Arc<AtomicU64>,
    pub sample_rate: f32,
    stream: Option<Box<dyn Backend>>,
    null_stop: Option<Arc<AtomicBool>>,
}

/// A running output stream, kept alive by the [`AudioHandle`].
trait Backend: Send {
    /// Pause (app in the background) or resume the device stream.
    fn set_paused(&self, paused: bool);
}

#[derive(Default, Debug, Clone)]
pub struct AudioStatus {
    pub skipped: Vec<String>,
    pub stream_error: Option<String>,
}

/// Command-line audio preferences.
#[derive(Default, Debug, Clone)]
pub struct AudioOptions {
    pub silent: bool,
    /// Host name, e.g. "pulseaudio" or "alsa" (case-insensitive prefix).
    pub host: Option<String>,
    /// Output device: case-insensitive substring of its name.
    pub device: Option<String>,
    /// Frames per audio callback (default 512 ≈ 11 ms at 48 kHz). Without a fixed size
    /// PulseAudio/PipeWire picks its ~2 s default buffer, which makes playing impossible.
    pub buffer: Option<u32>,
}

pub const DEFAULT_BUFFER: u32 = 512;

impl AudioHandle {
    pub fn send(&mut self, cmd: Cmd) {
        // The queue is large; if the audio thread is wedged we drop rather than block the UI.
        let _ = self.cmds.push(cmd);
    }

    /// Release the device while the app is in the background (Android), and
    /// take it back afterwards. A no-op for the silent clock.
    pub fn set_paused(&self, paused: bool) {
        if let Some(s) = &self.stream {
            s.set_paused(paused);
        }
    }
}

impl Drop for AudioHandle {
    fn drop(&mut self) {
        if let Some(s) = &self.null_stop {
            s.store(true, Ordering::Relaxed);
        }
    }
}

/// Everything the realtime callback owns.
struct Runner {
    engine: Engine,
    cmds: rtrb::Consumer<Cmd>,
    scope: rtrb::Producer<(f32, f32)>,
    snapshot: SharedSnapshot,
    since_snap: usize,
    #[cfg_attr(not(feature = "device"), allow(dead_code))] // sample-format conversion (cpal)
    scratch: Vec<f32>,
    callbacks: Arc<AtomicU64>,
    #[cfg_attr(not(any(feature = "device", target_os = "android")), allow(dead_code))] // set by device backends
    latency_us: Arc<AtomicU64>,
    period: Arc<AtomicU64>,
}

impl Runner {
    fn render(&mut self, out: &mut [f32], channels: usize) {
        self.callbacks.fetch_add(1, Ordering::Relaxed);
        while let Ok(cmd) = self.cmds.pop() {
            self.engine.handle(cmd);
        }
        let scope = &mut self.scope;
        self.engine.render(out, channels, |l, r| {
            let _ = scope.push((l, r));
        });
        self.since_snap += out.len() / channels.max(1);
        if self.since_snap as f32 >= self.engine.sample_rate() * 0.004
            && let Ok(mut s) = self.snapshot.try_lock()
        {
            self.engine.snapshot(&mut s);
            self.since_snap = 0;
        }
    }
}

struct Parts {
    run: Runner,
    cmds: rtrb::Producer<Cmd>,
    scope: rtrb::Consumer<(f32, f32)>,
    snapshot: SharedSnapshot,
    callbacks: Arc<AtomicU64>,
    latency_us: Arc<AtomicU64>,
    period: Arc<AtomicU64>,
    xruns: Arc<AtomicU64>,
}

fn runner(sr: f32) -> Parts {
    let (ctx, crx) = rtrb::RingBuffer::<Cmd>::new(4096);
    let (stx, srx) = rtrb::RingBuffer::<(f32, f32)>::new(SCOPE_CAP);
    let snapshot: SharedSnapshot = Arc::new(Mutex::new(Snapshot::default()));
    let callbacks = Arc::new(AtomicU64::new(0));
    let latency_us = Arc::new(AtomicU64::new(0));
    let period = Arc::new(AtomicU64::new(0));
    let run = Runner {
        engine: Engine::new(sr),
        cmds: crx,
        scope: stx,
        snapshot: snapshot.clone(),
        since_snap: 0,
        scratch: Vec::with_capacity(1 << 15),
        callbacks: callbacks.clone(),
        latency_us: latency_us.clone(),
        period: period.clone(),
    };
    Parts { run, cmds: ctx, scope: srx, snapshot, callbacks, latency_us, period, xruns: Arc::new(AtomicU64::new(0)) }
}

pub fn start(opts: &AudioOptions) -> AudioHandle {
    let status = Arc::new(Mutex::new(AudioStatus::default()));
    if opts.silent {
        return start_null(48000.0, "silent (--silent)".into(), status);
    }
    #[cfg(target_os = "android")]
    match aaudio::start(status.clone()) {
        Ok(h) => return h,
        Err(e) => status.lock().unwrap().skipped.push(format!("AAudio: {e}")),
    }
    #[cfg(feature = "device")]
    if let Some(h) = device::start(opts, status.clone()) {
        return h;
    }
    #[cfg(not(feature = "device"))]
    status.lock().unwrap().skipped.push("built without device audio".into());
    start_null(48000.0, "silent (no audio device could be opened)".into(), status)
}

fn start_null(sr: f32, label: String, status: Arc<Mutex<AudioStatus>>) -> AudioHandle {
    let Parts { mut run, cmds, scope, snapshot, callbacks, latency_us, period, xruns } = runner(sr);
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::Builder::new()
        .name("iki-null-audio".into())
        .spawn(move || {
            const BLOCK: usize = 256;
            let mut buf = vec![0f32; BLOCK * 2];
            let period = Duration::from_secs_f64(BLOCK as f64 / sr as f64);
            let mut next = Instant::now();
            while !flag.load(Ordering::Relaxed) {
                run.render(&mut buf, 2);
                next += period;
                let now = Instant::now();
                if next > now {
                    std::thread::sleep(next - now);
                } else if now - next > Duration::from_millis(200) {
                    next = now; // fell far behind (suspended); resync
                }
            }
        })
        .expect("spawn null audio thread");
    AudioHandle {
        cmds,
        scope,
        snapshot,
        backend: label,
        silent: true,
        status,
        callbacks,
        latency_us,
        period,
        xruns,
        sample_rate: sr,
        stream: None,
        null_stop: Some(stop),
    }
}

/// The engine driven by someone else's audio thread — a plugin host's
/// `process()`. Commands from the [`AudioHandle`] are applied at the start of
/// every [`Renderer::render`] call, so splitting a host block at event offsets
/// makes them sample-accurate.
pub struct Renderer {
    run: Runner,
}

impl Renderer {
    /// Render `out` (interleaved, `channels` per frame).
    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        self.run.render(out, channels);
    }

    /// The host's block size, for the engine readout.
    pub fn set_period(&self, frames: usize) {
        self.run.period.store(frames as u64, Ordering::Relaxed);
    }
}

/// See [`external`].
pub struct External {
    /// For the [`crate::Controller`]; its `scope` is a dummy — the real one is [`External::scope`].
    pub handle: AudioHandle,
    pub renderer: Renderer,
    /// Output samples for a spectrogram, to hand to whichever thread draws it.
    pub scope: rtrb::Consumer<(f32, f32)>,
}

/// An engine without an audio device: the caller renders it from its own
/// audio callback (see [`Renderer`]).
pub fn external(sr: f32, label: String) -> External {
    let Parts { run, cmds, scope, snapshot, callbacks, latency_us, period, xruns } = runner(sr);
    let (_, dummy) = rtrb::RingBuffer::<(f32, f32)>::new(1);
    External {
        handle: AudioHandle {
            cmds,
            scope: dummy,
            snapshot,
            backend: label,
            silent: false,
            status: Arc::new(Mutex::new(AudioStatus::default())),
            callbacks,
            latency_us,
            period,
            xruns,
            sample_rate: sr,
            stream: None,
            null_stop: None,
        },
        renderer: Renderer { run },
        scope,
    }
}

/// Offline rendering for tests and `iki render`: returns an engine and a buffer sink.
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
