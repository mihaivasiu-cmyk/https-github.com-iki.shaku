//! Audio devices through cpal (desktop, and the Android fallback). Behind the
//! `device` feature: a plugin build has its host's audio thread instead.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::engine::{Cmd, SharedSnapshot};

use super::{AudioHandle, AudioOptions, AudioStatus, Backend, DEFAULT_BUFFER, Parts, Runner, runner};

pub(super) struct CpalBackend(cpal::Stream);

impl Backend for CpalBackend {
    fn set_paused(&self, paused: bool) {
        let _ = if paused { self.0.pause() } else { self.0.play() };
    }
}

impl Runner {
    fn note_timing(&self, info: &cpal::OutputCallbackInfo, frames: usize) {
        let ts = info.timestamp();
        let lat = ts.playback.duration_since(ts.callback);
        self.latency_us.store(lat.as_micros() as u64, Ordering::Relaxed);
        self.period.store(frames as u64, Ordering::Relaxed);
    }

    fn render_as<T: cpal::SizedSample + cpal::FromSample<f32>>(&mut self, out: &mut [T], channels: usize) {
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.resize(out.len(), 0.0);
        self.render(&mut scratch, channels);
        for (o, s) in out.iter_mut().zip(scratch.iter()) {
            *o = T::from_sample(*s);
        }
        self.scratch = scratch;
    }
}

fn device_name(d: &cpal::Device) -> String {
    d.description().map(|x| x.name().to_string()).unwrap_or_else(|_| "unknown".into())
}

/// Hosts to try, best first: an explicit `--host`, else cpal's default
/// (PulseAudio/PipeWire when a server is reachable, else ALSA), then the rest.
fn host_order(req: Option<&str>) -> Result<Vec<cpal::HostId>, String> {
    let avail = cpal::available_hosts();
    if let Some(r) = req {
        let r = r.to_lowercase();
        let hit = avail.iter().find(|h| h.name().to_lowercase().starts_with(&r));
        return match hit {
            Some(h) => Ok(vec![*h]),
            None => Err(format!(
                "unknown audio host '{r}' (available: {})",
                avail.iter().map(|h| h.name().to_lowercase()).collect::<Vec<_>>().join(", ")
            )),
        };
    }
    let mut order = vec![cpal::default_host().id()];
    for h in avail {
        if !order.contains(&h) {
            order.push(h);
        }
    }
    Ok(order)
}

/// `iki --list-devices`.
pub fn list_devices() -> String {
    let mut out = String::new();
    let default_id = cpal::default_host().id();
    for id in cpal::available_hosts() {
        let mark = if id == default_id { " (default host)" } else { "" };
        out.push_str(&format!("{}{mark}\n", id.name().to_lowercase()));
        let Ok(host) = cpal::host_from_id(id) else {
            out.push_str("    (unavailable)\n");
            continue;
        };
        let def = host.default_output_device().map(|d| device_name(&d));
        match host.output_devices() {
            Ok(devs) => {
                for d in devs {
                    let name = device_name(&d);
                    let cfg = d
                        .default_output_config()
                        .map(|c| format!("{} ch, {} Hz, {:?}", c.channels(), c.sample_rate(), c.sample_format()))
                        .unwrap_or_else(|e| format!("no output config: {e}"));
                    let star = if def.as_deref() == Some(name.as_str()) { "*" } else { " " };
                    out.push_str(&format!("  {star} {name}  [{cfg}]\n"));
                }
            }
            Err(e) => out.push_str(&format!("    cannot list devices: {e}\n")),
        }
    }
    out
}

/// Open the best output (see [`host_order`]); `None` = nothing usable.
pub(super) fn start(opts: &AudioOptions, status: Arc<Mutex<AudioStatus>>) -> Option<AudioHandle> {
    let order = match host_order(opts.host.as_deref()) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("iki: {e}");
            status.lock().unwrap().skipped.push(e);
            return None;
        }
    };
    for id in order {
        match start_cpal(id, opts.device.as_deref(), opts.buffer.unwrap_or(DEFAULT_BUFFER), status.clone()) {
            Ok(h) => {
                eprintln!("iki: audio → {}", h.backend);
                return Some(h);
            }
            Err(e) => {
                let msg = format!("{}: {e}", id.name());
                eprintln!("iki: audio host skipped — {msg}");
                status.lock().unwrap().skipped.push(msg);
            }
        }
    }
    eprintln!("iki: no audio output could be opened; running silent (see `iki --list-devices`)");
    None
}

fn start_cpal(
    id: cpal::HostId,
    want: Option<&str>,
    buffer: u32,
    status: Arc<Mutex<AudioStatus>>,
) -> Result<AudioHandle, String> {
    let host = cpal::host_from_id(id).map_err(|e| e.to_string())?;
    let device = match want {
        Some(w) => {
            let w = w.to_lowercase();
            host.output_devices()
                .map_err(|e| e.to_string())?
                .find(|d| device_name(d).to_lowercase().contains(&w))
                .ok_or_else(|| format!("no output device matching '{w}'"))?
        }
        None => host.default_output_device().ok_or("no default output device")?,
    };
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let fixed = match supported.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => buffer.clamp(*min, (*max).max(*min)),
        cpal::SupportedBufferSize::Unknown => buffer,
    };
    // A small fixed period keeps the instrument playable; fall back to the
    // backend default if the device refuses it.
    let first = open_stream(&device, &supported, cpal::BufferSize::Fixed(fixed), status.clone());
    let (stream, parts, config) = match first {
        Ok(ok) => ok,
        Err(e) => {
            if let Ok(mut s) = status.lock() {
                s.skipped.push(format!("{}: {fixed}-frame buffer refused ({e}); using the backend default", id.name()));
            }
            open_stream(&device, &supported, cpal::BufferSize::Default, status.clone())?
        }
    };
    let buf = match config.buffer_size {
        cpal::BufferSize::Fixed(n) => format!("{n}-frame buffer"),
        cpal::BufferSize::Default => "default buffer".into(),
    };
    Ok(AudioHandle {
        cmds: parts.cmds,
        scope: parts.scope,
        snapshot: parts.snapshot,
        backend: format!(
            "{} · {} · {} ch · {} Hz · {buf}",
            id.name(),
            device_name(&device),
            config.channels,
            config.sample_rate
        ),
        silent: false,
        status,
        callbacks: parts.callbacks,
        latency_us: parts.latency_us,
        period: parts.period,
        xruns: parts.xruns,
        sample_rate: config.sample_rate as f32,
        stream: Some(Box::new(CpalBackend(stream))),
        null_stop: None,
    })
}

struct Opened {
    cmds: rtrb::Producer<Cmd>,
    scope: rtrb::Consumer<(f32, f32)>,
    snapshot: SharedSnapshot,
    callbacks: Arc<AtomicU64>,
    latency_us: Arc<AtomicU64>,
    period: Arc<AtomicU64>,
    xruns: Arc<AtomicU64>,
}

fn open_stream(
    device: &cpal::Device,
    supported: &cpal::SupportedStreamConfig,
    buffer_size: cpal::BufferSize,
    status: Arc<Mutex<AudioStatus>>,
) -> Result<(cpal::Stream, Opened, cpal::StreamConfig), String> {
    let format = supported.sample_format();
    let mut config = supported.config();
    config.buffer_size = buffer_size;
    let sr = config.sample_rate as f32;
    let channels = config.channels as usize;
    let Parts { mut run, cmds, scope, snapshot, callbacks, latency_us, period, xruns } = runner(sr);
    let xr = xruns.clone();
    let err = move |e: cpal::Error| {
        // Underruns and route changes are glitches the stream survives: count, don't latch.
        match e.kind() {
            cpal::ErrorKind::Xrun => {
                xr.fetch_add(1, Ordering::Relaxed);
                return;
            }
            cpal::ErrorKind::DeviceChanged | cpal::ErrorKind::RealtimeDenied => return,
            _ => {}
        }
        eprintln!("iki: audio stream error: {e}");
        if let Ok(mut s) = status.lock() {
            s.stream_error = Some(e.to_string());
        }
    };
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_output_stream::<f32, _, _>(
            config,
            move |out, info| {
                run.note_timing(info, out.len() / channels.max(1));
                run.render(out, channels)
            },
            err,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_output_stream::<i16, _, _>(
            config,
            move |out, info| {
                run.note_timing(info, out.len() / channels.max(1));
                run.render_as(out, channels)
            },
            err,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_output_stream::<u16, _, _>(
            config,
            move |out, info| {
                run.note_timing(info, out.len() / channels.max(1));
                run.render_as(out, channels)
            },
            err,
            None,
        ),
        cpal::SampleFormat::I32 => device.build_output_stream::<i32, _, _>(
            config,
            move |out, info| {
                run.note_timing(info, out.len() / channels.max(1));
                run.render_as(out, channels)
            },
            err,
            None,
        ),
        other => return Err(format!("unsupported sample format {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, Opened { cmds, scope, snapshot, callbacks, latency_us, period, xruns }, config))
}
