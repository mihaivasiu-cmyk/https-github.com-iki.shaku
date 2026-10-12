//! `iki` — launch the instrument, or render a demo performance offline.

mod midi_bus;
mod ui;

use iki_core::audio;
use iki_core::engine::{Cmd, Engine, FxSet};
use iki_core::shaku::fingering::{Voicing, holes_from_index, meri_depth};
use iki_core::shaku::samples::load_bank;

const USAGE: &str = "\
iki — a playable 1.8 shakuhachi (native port)

USAGE:
    iki [OPTIONS]                       open the instrument window
    iki --list-devices                  show audio hosts and output devices
    iki render <out.wav> [--sr 48000]   render a short demo phrase offline
    iki --help

OPTIONS:
    --host <name>     audio host: pulseaudio (also PipeWire via pipewire-pulse) or alsa
    --device <text>   output device whose name contains <text> (see --list-devices)
    --buffer <frames> audio period in frames (default 512 ≈ 11 ms; raise if it crackles)
    --silent          do not open an audio device (UI + engine still run, silently)
    --night           start in the Temple (dark) theme; --day for Breath (light)

Audio defaults to the PulseAudio protocol when a server (PipeWire or PulseAudio)
is running, else ALSA's default device. The chosen output is printed at start
and shown in the window header.
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return;
    }
    if args.iter().any(|a| a == "--list-devices") {
        print!("{}", audio::list_devices());
        return;
    }
    if args.first().map(String::as_str) == Some("render") {
        let Some(out) = args.get(1) else {
            eprint!("{USAGE}");
            std::process::exit(2);
        };
        let sr = args
            .iter()
            .position(|a| a == "--sr")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(48000.0);
        if let Err(e) = render_demo(out, sr) {
            eprintln!("iki: {e}");
            std::process::exit(1);
        }
        return;
    }
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let audio_opts = audio::AudioOptions {
        silent: args.iter().any(|a| a == "--silent"),
        host: value("--host"),
        device: value("--device"),
        buffer: value("--buffer").and_then(|v| v.parse().ok()),
    };
    let night = if args.iter().any(|a| a == "--night") {
        Some(true)
    } else if args.iter().any(|a| a == "--day") {
        Some(false)
    } else {
        None
    };

    // The desktop has the memory for long takes (phones keep the 24 s default).
    iki_core::shaku::looper::set_max_seconds(120.0);
    let handle = audio::start(&audio_opts);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Iki — 1.8 shakuhachi")
            .with_app_id("iki")
            .with_inner_size([1380.0, 1000.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    let mut handle = Some(handle);
    if let Err(e) = eframe::run_native(
        "Iki",
        options,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, handle.take().expect("audio handle"), night)))),
    ) {
        eprintln!("iki: could not open a window: {e}");
        std::process::exit(1);
    }
}

/// A scripted phrase that exercises the kernel: kari scale, meri, kan,
/// ornaments, filter/LFO and the looper.
fn render_demo(path: &str, sr: f32) -> Result<(), String> {
    let mut e = Engine::new(sr);
    e.handle(Cmd::Bank(load_bank()?));
    let depth = |v: Voicing, lean: f64| meri_depth(v, 0.14, lean);
    let mut script: Vec<(f64, Cmd)> =
        vec![(0.0, Cmd::Meri(depth(Voicing::Kari, 0.0))), (0.0, Cmd::LoopPunch(0)), (0.05, Cmd::Blow(true))];
    let mut t = 0.05;
    for i in 0..6 {
        script.push((t, Cmd::Holes(holes_from_index(i))));
        t += 0.55;
    }
    script.push((t, Cmd::Atari));
    t += 0.3;
    script.push((t, Cmd::Koro));
    t += 0.5;
    script.push((t, Cmd::Holes(holes_from_index(2))));
    script.push((t, Cmd::Meri(depth(Voicing::Meri, 0.0))));
    t += 0.6;
    script.push((t, Cmd::Meri(depth(Voicing::Meri, 1.0))));
    t += 0.6;
    script.push((t, Cmd::Meri(depth(Voicing::Kari, 0.0))));
    script.push((t, Cmd::Otsu(false)));
    script.push((t, Cmd::Holes(holes_from_index(0))));
    t += 0.7;
    script.push((t, Cmd::Muraiki));
    t += 0.7;
    script.push((t, Cmd::Otsu(true)));
    script.push((t, Cmd::Fx(FxSet::LfoPitch(true))));
    script.push((t, Cmd::Fx(FxSet::DelayMix(0.5))));
    t += 1.0;
    script.push((t, Cmd::Blow(false)));
    script.push((t + 0.4, Cmd::LoopPunch(0))); // commits at once → loop plays
    let total = t + 6.0;
    script.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    let mut out = Vec::new();
    let mut now = 0.0;
    for (at, cmd) in script {
        if at > now {
            audio::render_offline(&mut e, at - now, &mut out);
            now = at;
        }
        e.handle(cmd);
    }
    audio::render_offline(&mut e, total - now, &mut out);

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    let mut peak = 0f32;
    let mut sum = 0f64;
    for s in &out {
        peak = peak.max(s.abs());
        sum += (*s as f64) * (*s as f64);
        w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16).map_err(|e| e.to_string())?;
    }
    w.finalize().map_err(|e| e.to_string())?;
    let rms = (sum / out.len().max(1) as f64).sqrt();
    println!("wrote {path}: {:.1} s stereo @ {} Hz, peak {:.3}, rms {:.4}", total, sr as u32, peak, rms);
    Ok(())
}
