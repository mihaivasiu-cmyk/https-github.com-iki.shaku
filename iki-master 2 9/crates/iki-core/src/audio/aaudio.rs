//! Native AAudio output (Android), set up the way Oboe recommends for an
//! instrument — cpal's generic AAudio path does not, and it crackles:
//!
//! * low-latency performance mode, exclusive (MMAP) sharing when the device
//!   grants it, else shared; float stereo at the device's native rate;
//! * callbacks of whatever size the device bursts in, with the buffer starting
//!   at two bursts and growing one burst per underrun (never shrinking);
//! * the stream is reopened when AAudio disconnects it — headphones, Bluetooth
//!   or USB route changes, audio-server restarts — instead of going silent;
//! * no locks, allocation or logging on the realtime thread (the engine lives
//!   behind a mutex that only a reopen ever contends, and then we play silence).

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use ndk::audio::{
    AudioCallbackResult, AudioDirection, AudioError, AudioFormat, AudioPerformanceMode, AudioSharingMode, AudioStream,
    AudioStreamBuilder, Clockid,
};

use super::{AudioHandle, AudioStatus, Backend, Parts, Runner, runner};

/// `AudioStream` is a plain handle; AAudio allows using it from any thread.
struct Stream(AudioStream);
// SAFETY: AAudio stream handles are thread-safe for the calls we make
// (start/pause/stop/close from a non-callback thread).
unsafe impl Send for Stream {}

struct Shared {
    /// The engine; `None` until the first stream told us the sample rate.
    run: Arc<Mutex<Option<Runner>>>,
    stream: Mutex<Option<Stream>>,
    /// Engine sample rate; every reopen asks for it (AAudio resamples if the new route differs).
    rate: AtomicI32,
    paused: AtomicBool,
    closed: AtomicBool,
    reopening: AtomicBool,
    /// A disconnect arrived (possibly while a reopen was already running).
    again: AtomicBool,
    status: Arc<Mutex<AudioStatus>>,
    xruns: Arc<AtomicU64>,
    latency_us: Arc<AtomicU64>,
}

struct AAudioBackend(Arc<Shared>);

impl Backend for AAudioBackend {
    fn set_paused(&self, paused: bool) {
        let sh = &self.0;
        sh.paused.store(paused, Ordering::Relaxed);
        if let Ok(g) = sh.stream.lock()
            && let Some(s) = g.as_ref()
        {
            let _ = if paused { s.0.request_pause() } else { s.0.request_start() };
        }
    }
}

impl Drop for AAudioBackend {
    fn drop(&mut self) {
        self.0.closed.store(true, Ordering::Relaxed);
        let old = self.0.stream.lock().ok().and_then(|mut g| g.take());
        if let Some(s) = old {
            let _ = s.0.request_stop();
        }
    }
}

fn describe(s: &AudioStream) -> String {
    let mode = match s.performance_mode() {
        AudioPerformanceMode::LowLatency => "low latency",
        AudioPerformanceMode::PowerSaving => "power saving",
        _ => "normal",
    };
    let share = if s.sharing_mode() == AudioSharingMode::Exclusive { "exclusive" } else { "shared" };
    format!(
        "AAudio · {mode} · {share} · {} ch · {} Hz · burst {}",
        s.channel_count(),
        s.sample_rate(),
        s.frames_per_burst()
    )
}

/// Open (not start) an output stream. `rate` = None asks for the native rate.
fn open(sh: &Arc<Shared>, rate: Option<i32>, exclusive: bool) -> Result<AudioStream, AudioError> {
    let run = sh.run.clone();
    let xruns = sh.xruns.clone();
    let latency = sh.latency_us.clone();
    let mut last_xruns = 0;
    let mut calls = 0u32;
    let data = move |s: &AudioStream, data: *mut c_void, frames: i32| {
        let ch = s.channel_count().max(1) as usize;
        let n = frames.max(0) as usize * ch;
        // SAFETY: AAudio hands us `frames` frames of `ch` interleaved f32 (we asked for PCM_Float).
        let out = unsafe { std::slice::from_raw_parts_mut(data as *mut f32, n) };
        let Ok(mut guard) = run.try_lock() else {
            out.fill(0.0);
            return AudioCallbackResult::Continue;
        };
        let Some(r) = guard.as_mut() else {
            out.fill(0.0);
            return AudioCallbackResult::Continue;
        };
        r.render(out, ch);
        r.period.store(frames.max(0) as u64, Ordering::Relaxed);

        // Underrun → one more burst of headroom (Oboe's LatencyTuner).
        let x = s.x_run_count();
        if x > last_xruns {
            xruns.fetch_add((x - last_xruns) as u64, Ordering::Relaxed);
            last_xruns = x;
            let size = s.buffer_size_in_frames() + s.frames_per_burst().max(16);
            if size <= s.buffer_capacity_in_frames() {
                let _ = s.set_buffer_size_in_frames(size);
            }
        }
        calls = calls.wrapping_add(1);
        if calls % 64 == 1 {
            // Frames queued ahead of the DAC ≈ output latency.
            let rate = s.sample_rate().max(1) as i64;
            let queued = match s.timestamp(Clockid::Monotonic) {
                Ok(ts) => s.frames_written() - ts.frame_position,
                Err(_) => s.buffer_size_in_frames() as i64,
            };
            latency.store((queued.max(0) * 1_000_000 / rate) as u64, Ordering::Relaxed);
        }
        AudioCallbackResult::Continue
    };
    let weak: Weak<Shared> = Arc::downgrade(sh);
    let error = move |_: &AudioStream, e: AudioError| {
        // Called on an AAudio thread; the stream may only be closed elsewhere.
        if let Some(sh) = weak.upgrade() {
            if let Ok(mut st) = sh.status.lock() {
                st.stream_error = Some(format!("AAudio: {e}; reopening"));
            }
            sh.again.store(true, Ordering::Release);
            std::thread::Builder::new().name("iki-aaudio-reopen".into()).spawn(move || reopen(&sh)).ok();
        }
    };
    let mut b = AudioStreamBuilder::new()?
        .direction(AudioDirection::Output)
        .performance_mode(AudioPerformanceMode::LowLatency)
        .sharing_mode(if exclusive { AudioSharingMode::Exclusive } else { AudioSharingMode::Shared })
        .format(AudioFormat::PCM_Float)
        .channel_count(2) // usage/content type default to media/music
        .data_callback(Box::new(data))
        .error_callback(Box::new(error));
    if let Some(r) = rate {
        b = b.sample_rate(r);
    }
    let s = b.open_stream()?;
    if s.format() != AudioFormat::PCM_Float {
        return Err(AudioError::InvalidFormat);
    }
    // Start at two bursts; the callback adds more on underruns.
    let burst = s.frames_per_burst().max(16);
    let _ = s.set_buffer_size_in_frames((burst * 2).min(s.buffer_capacity_in_frames()));
    Ok(s)
}

fn open_any(sh: &Arc<Shared>, rate: Option<i32>) -> Result<AudioStream, AudioError> {
    open(sh, rate, true).or_else(|_| open(sh, rate, false))
}

/// Replace a disconnected stream (runs on its own thread; one at a time).
fn reopen(sh: &Arc<Shared>) {
    if sh.reopening.swap(true, Ordering::AcqRel) {
        return; // the running reopen sees `again` and goes round once more
    }
    while sh.again.swap(false, Ordering::AcqRel) && !sh.closed.load(Ordering::Relaxed) {
        reopen_once(sh);
    }
    sh.reopening.store(false, Ordering::Release);
}

fn reopen_once(sh: &Arc<Shared>) {
    let old = sh.stream.lock().ok().and_then(|mut g| g.take());
    if let Some(s) = old {
        let _ = s.0.request_stop();
        drop(s); // AAudioStream_close
    }
    let rate = sh.rate.load(Ordering::Relaxed);
    for attempt in 0..40 {
        if sh.closed.load(Ordering::Relaxed) {
            return;
        }
        match open_any(sh, Some(rate)) {
            Ok(s) => {
                if sh.paused.load(Ordering::Relaxed) || s.request_start().is_ok() {
                    if let Ok(mut st) = sh.status.lock() {
                        st.stream_error = None;
                    }
                    if let Ok(mut g) = sh.stream.lock() {
                        *g = Some(Stream(s));
                    }
                    return;
                }
            }
            Err(e) => {
                if let Ok(mut st) = sh.status.lock() {
                    st.stream_error = Some(format!("AAudio: {e} (retrying)"));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(if attempt < 5 { 100 } else { 500 }));
    }
}

pub(super) fn start(status: Arc<Mutex<AudioStatus>>) -> Result<AudioHandle, String> {
    let latency_us = Arc::new(AtomicU64::new(0));
    let xruns = Arc::new(AtomicU64::new(0));
    let sh = Arc::new(Shared {
        run: Arc::new(Mutex::new(None)),
        stream: Mutex::new(None),
        rate: AtomicI32::new(0),
        paused: AtomicBool::new(false),
        closed: AtomicBool::new(false),
        reopening: AtomicBool::new(false),
        again: AtomicBool::new(false),
        status: status.clone(),
        xruns: xruns.clone(),
        latency_us: latency_us.clone(),
    });
    let stream = open_any(&sh, None).map_err(|e| e.to_string())?;
    let rate = stream.sample_rate();
    sh.rate.store(rate, Ordering::Relaxed);
    let Parts { mut run, cmds, scope, snapshot, callbacks, period, .. } = runner(rate as f32);
    // The callback updates the shared latency/xrun counters directly.
    run.latency_us = latency_us.clone();
    *sh.run.lock().map_err(|e| e.to_string())? = Some(run);
    let backend = describe(&stream);
    stream.request_start().map_err(|e| e.to_string())?;
    *sh.stream.lock().map_err(|e| e.to_string())? = Some(Stream(stream));
    Ok(AudioHandle {
        cmds,
        scope,
        snapshot,
        backend,
        silent: false,
        status,
        callbacks,
        latency_us,
        period,
        xruns,
        sample_rate: rate as f32,
        stream: Some(Box::new(AAudioBackend(sh))),
        null_stop: None,
    })
}
