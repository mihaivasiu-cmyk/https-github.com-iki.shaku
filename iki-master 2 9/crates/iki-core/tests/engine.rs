//! End-to-end checks of the sound kernel: render real performances offline and
//! measure what comes out.

use iki_core::audio::render_offline;
use iki_core::engine::{Cmd, Engine, FxSet, VoiceMode};
use iki_core::shaku::fingering::{FINGERINGS, Voicing, holes_from_index, meri_depth};
use iki_core::shaku::samples::load_bank;

const SR: f32 = 48000.0;

fn engine() -> Engine {
    // The loop search is slow in debug builds, so every test shares one decoded bank.
    static BANK: std::sync::OnceLock<iki_core::shaku::samples::Bank> = std::sync::OnceLock::new();
    let mut e = Engine::new(SR);
    e.handle(Cmd::Bank(BANK.get_or_init(|| load_bank().unwrap()).clone()));
    // Dry the output a little so pitch detection sees the voice, not the reverb tail.
    e.handle(Cmd::Fx(FxSet::Reverb(0.0)));
    e
}

fn mono(buf: &[f32]) -> Vec<f32> {
    buf.chunks(2).map(|c| c[0]).collect()
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

/// Normalised autocorrelation pitch estimate in [120, 1400] Hz.
fn pitch(x: &[f32]) -> f32 {
    let min_lag = (SR / 1400.0) as usize;
    let max_lag = (SR / 120.0) as usize;
    let n = x.len() - max_lag;
    let mut best = (0usize, -1f32);
    let mut corr = vec![0f32; max_lag + 1];
    for (lag, c) in corr.iter_mut().enumerate().take(max_lag + 1).skip(min_lag) {
        let (mut s, mut e0, mut e1) = (0f32, 0f32, 0f32);
        for i in 0..n {
            s += x[i] * x[i + lag];
            e0 += x[i] * x[i];
            e1 += x[i + lag] * x[i + lag];
        }
        *c = s / (e0 * e1).sqrt().max(1e-9);
    }
    // Prefer the first strong peak (avoids octave-down errors).
    let top = corr.iter().cloned().fold(-1f32, f32::max);
    for lag in min_lag + 1..max_lag {
        if corr[lag] > corr[lag - 1] && corr[lag] >= corr[lag + 1] && corr[lag] > top * 0.9 {
            best = (lag, corr[lag]);
            break;
        }
    }
    // Parabolic refinement.
    let l = best.0;
    let (a, b, c) = (corr[l - 1], corr[l], corr[l + 1]);
    let off = 0.5 * (a - c) / (a - 2.0 * b + c);
    SR / (l as f32 + if off.is_finite() { off } else { 0.0 })
}

fn play(e: &mut Engine, secs: f64) -> Vec<f32> {
    let mut out = Vec::new();
    render_offline(e, secs, &mut out);
    mono(&out)
}

fn cents(a: f32, b: f32) -> f32 {
    1200.0 * (a / b).log2()
}

#[test]
fn silent_until_blown_then_sounds() {
    let mut e = engine();
    let quiet = play(&mut e, 0.5);
    assert!(rms(&quiet) < 1e-4, "idle output {}", rms(&quiet));
    e.handle(Cmd::Blow(true));
    let on = play(&mut e, 1.0);
    assert!(rms(&on[24000..]) > 0.02, "blowing rms {}", rms(&on[24000..]));
    e.handle(Cmd::Blow(false));
    let _ = play(&mut e, 1.5);
    let tail = play(&mut e, 0.5);
    assert!(rms(&tail) < 2e-3, "after release {}", rms(&tail));
}

#[test]
fn every_kari_note_is_in_tune() {
    let mut e = engine();
    e.handle(Cmd::Meri(meri_depth(Voicing::Kari, 0.0, 0.0)));
    e.handle(Cmd::Blow(true));
    for (i, f) in FINGERINGS.iter().enumerate() {
        e.handle(Cmd::Holes(holes_from_index(i)));
        let x = play(&mut e, 0.9);
        let hz = pitch(&x[24000..]);
        let dev = cents(hz, f.freq as f32);
        println!("{:>4} expected {:7.2} Hz got {:7.2} Hz ({dev:+.1} c)", f.name, f.freq, hz);
        assert!(dev.abs() < 35.0, "{} off by {dev:.1} cents ({hz} Hz)", f.name);
    }
}

#[test]
fn meri_flattens_and_kan_doubles() {
    let mut e = engine();
    e.handle(Cmd::Meri(0.0));
    e.handle(Cmd::Blow(true));
    let kari = pitch(&play(&mut e, 0.9)[24000..]);
    // meri lane at rest ≈ chu-meri (one semitone down)
    e.handle(Cmd::Meri(meri_depth(Voicing::Meri, 0.0, 0.0)));
    let meri = pitch(&play(&mut e, 0.9)[24000..]);
    let drop = cents(meri, kari);
    println!("kari {kari:.2} Hz, chu-meri {meri:.2} Hz ({drop:+.1} c)");
    assert!((-140.0..-60.0).contains(&drop), "meri drop {drop}");
    e.handle(Cmd::Meri(0.0));
    e.handle(Cmd::Otsu(false));
    let kan = pitch(&play(&mut e, 0.9)[24000..]);
    let up = cents(kan, kari);
    println!("kan {kan:.2} Hz ({up:+.1} c)");
    assert!((up - 1200.0).abs() < 40.0, "kan interval {up}");
}

#[test]
fn looper_records_and_plays_back() {
    let mut e = engine();
    e.handle(Cmd::LoopPunch(0));
    e.handle(Cmd::Blow(true));
    let _ = play(&mut e, 3.0);
    e.handle(Cmd::Blow(false));
    let _ = play(&mut e, 0.2);
    e.handle(Cmd::LoopPunch(0)); // commits immediately; take snaps to one bar (3.33 s)
    let _ = play(&mut e, 0.5);
    let mut snap = iki_core::engine::Snapshot::default();
    e.snapshot(&mut snap);
    assert_eq!(snap.loop_bank.tracks[0].mode, iki_core::shaku::looper::LoopMode::Play);
    // Voice is released; what we hear now is the loop.
    let x = play(&mut e, 1.5);
    assert!(rms(&x) > 0.01, "loop playback rms {}", rms(&x));
    e.handle(Cmd::LoopStop(None));
    let _ = play(&mut e, 1.0);
    let x = play(&mut e, 0.5);
    assert!(rms(&x) < 5e-3, "stopped loop rms {}", rms(&x));
}

#[test]
fn ornaments_and_fx_do_not_blow_up() {
    let mut e = engine();
    e.handle(Cmd::Blow(true));
    let _ = play(&mut e, 0.3);
    for s in [
        FxSet::Tape(1.0),
        FxSet::Phaser(1.0),
        FxSet::DelayMix(1.0),
        FxSet::DelayFeedback(1.0),
        FxSet::DelayPing(true),
        FxSet::Reverb(1.0),
        FxSet::ReverbDecay(1.0),
        FxSet::LfoPitch(true),
        FxSet::LfoFilt(true),
        FxSet::LfoAmp(true),
        FxSet::FiltRes(1.0),
        FxSet::EnvLoop(true),
        FxSet::FenvLoop(true),
    ] {
        e.handle(Cmd::Fx(s));
    }
    for t in iki_core::shaku::env_shape::FILT_TYPES {
        e.handle(Cmd::Fx(FxSet::FiltType(t.0)));
        e.handle(Cmd::Atari);
        let _ = play(&mut e, 0.1);
        e.handle(Cmd::Koro);
        let _ = play(&mut e, 0.1);
        e.handle(Cmd::Muraiki);
        let x = play(&mut e, 0.3);
        assert!(x.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        assert!(rms(&x) > 1e-3, "{:?} went silent", t.0);
    }
}

#[test]
fn bore_waveguide_voice_speaks() {
    let mut e = engine();
    e.handle(Cmd::VoiceMode(VoiceMode::Bore));
    e.handle(Cmd::Meri(0.0));
    e.handle(Cmd::Blow(true));
    let x = play(&mut e, 1.5);
    let tail = &x[36000..];
    assert!(rms(tail) > 0.005, "bore rms {}", rms(tail));
    let hz = pitch(tail);
    println!("bore Ro {hz:.2} Hz");
    // A physical-model voice: allow a wide window around D4 (or its octave).
    let c = cents(hz, 293.66).rem_euclid(1200.0);
    assert!(!(150.0..1050.0).contains(&c), "bore pitch {hz}");
}

#[test]
fn voice_mode_switch_keeps_sounding() {
    for sr in [44100.0f32, 48000.0] {
        let mut e = Engine::new(sr);
        e.handle(Cmd::Bank(load_bank().unwrap()));
        let n = (sr * 0.5) as usize;
        let step = |e: &mut Engine, label: &str| {
            let mut out = Vec::new();
            render_offline(e, 1.0, &mut out);
            let x = mono(&out);
            let r = rms(&x[n..]);
            println!("{sr} {label}: rms {r:.4}");
            r
        };
        e.handle(Cmd::Blow(true));
        assert!(step(&mut e, "sampler") > 0.01);
        e.handle(Cmd::VoiceMode(VoiceMode::Bore));
        assert!(step(&mut e, "bore (switched while blowing)") > 0.005);
        e.handle(Cmd::VoiceMode(VoiceMode::Sampler));
        assert!(step(&mut e, "sampler again") > 0.01);
        e.handle(Cmd::Blow(false));
        step(&mut e, "released");
        e.handle(Cmd::VoiceMode(VoiceMode::Bore));
        e.handle(Cmd::Blow(true));
        assert!(step(&mut e, "bore fresh") > 0.005);
        e.handle(Cmd::Blow(false));
        step(&mut e, "released");
        e.handle(Cmd::VoiceMode(VoiceMode::Sampler));
        e.handle(Cmd::Blow(true));
        assert!(step(&mut e, "sampler fresh") > 0.01);
        // every mode, in every order, while blowing
        use VoiceMode::*;
        for (a, b) in [
            (Sampler, Both),
            (Both, Sampler),
            (Bore, Both),
            (Both, Bore),
            (Sampler, Bore),
            (Bore, Sampler),
            (Both, Both),
        ] {
            e.handle(Cmd::VoiceMode(a));
            assert!(step(&mut e, &format!("{a:?}")) > 0.005, "{a:?} silent");
            e.handle(Cmd::VoiceMode(b));
            assert!(step(&mut e, &format!("{a:?} -> {b:?}")) > 0.005, "{a:?} -> {b:?} silent");
        }
    }
}

// --- flute / bore levels, filter modes ---------------------------------------

fn level_rms(mode: VoiceMode, set: impl Fn(&mut Engine)) -> f32 {
    let mut e = engine();
    e.handle(Cmd::VoiceMode(mode));
    set(&mut e);
    e.handle(Cmd::Blow(true));
    let x = play(&mut e, 1.5);
    rms(&x[24000..])
}

#[test]
fn flute_and_bore_level_knobs_set_their_loudness() {
    let loud = level_rms(VoiceMode::Sampler, |e| e.handle(Cmd::Fx(FxSet::FluteLevel(0.8))));
    let soft = level_rms(VoiceMode::Sampler, |e| e.handle(Cmd::Fx(FxSet::FluteLevel(0.2))));
    assert!(loud > soft * 2.5, "flute level: {loud} vs {soft}");
    let loud = level_rms(VoiceMode::Bore, |e| e.handle(Cmd::Fx(FxSet::BoreLevel(0.8))));
    let soft = level_rms(VoiceMode::Bore, |e| e.handle(Cmd::Fx(FxSet::BoreLevel(0.2))));
    assert!(loud > soft * 2.5, "bore level: {loud} vs {soft}");
    // the flute knob must not touch the bore voice, nor the other way round
    let a = level_rms(VoiceMode::Bore, |e| e.handle(Cmd::Fx(FxSet::FluteLevel(0.0))));
    let b = level_rms(VoiceMode::Bore, |e| e.handle(Cmd::Fx(FxSet::FluteLevel(1.0))));
    assert!((a - b).abs() < a * 0.05, "flute knob leaks into bore: {a} vs {b}");
    // Both = the two layers together, louder than either alone
    let both = level_rms(VoiceMode::Both, |_| {});
    let flute = level_rms(VoiceMode::Sampler, |_| {});
    assert!(both > flute * 0.9, "both {both} vs flute {flute}");
}

#[test]
fn filter_envelope_only_acts_in_env_mode() {
    use iki_core::engine::FiltAuto;
    let env_after = |auto: FiltAuto| {
        let mut e = engine();
        e.handle(Cmd::Fx(FxSet::FiltAuto(auto)));
        e.handle(Cmd::Blow(true));
        let _ = play(&mut e, 0.12);
        let mut snap = iki_core::engine::Snapshot::default();
        e.snapshot(&mut snap);
        snap.filt_env
    };
    assert!(env_after(FiltAuto::Env) > 50.0);
    assert!(env_after(FiltAuto::Off) < 1.0, "Off still opens the filter");
    assert!(env_after(FiltAuto::Sweep) < 1.0, "Sweep still runs the envelope");
}

#[test]
fn every_filter_type_audibly_shapes_the_sound() {
    use iki_core::shaku::env_shape::{FILT_TYPES, FiltType};
    // Brightness: energy of the first difference relative to the signal. A
    // plain RMS would miss filters that only reshape the upper harmonics.
    let level = |ty: FiltType, res: f64| {
        let mut e = engine();
        e.handle(Cmd::Fx(FxSet::FiltType(ty)));
        e.handle(Cmd::Fx(FxSet::FiltRes(res)));
        e.handle(Cmd::Fx(FxSet::FiltCut(0.5)));
        e.handle(Cmd::Blow(true));
        let x = mono(&play(&mut e, 1.2));
        let x = &x[18000..];
        let d: Vec<f32> = x.windows(2).map(|w| w[1] - w[0]).collect();
        rms(&d) / rms(x)
    };
    let reference = level(FiltType::Lowpass, 0.22);
    for (ty, name) in FILT_TYPES.into_iter().filter(|(t, _)| *t != FiltType::Lowpass) {
        let r = level(ty, 0.22);
        let moved = (r / reference - 1.0).abs();
        assert!(moved > 0.03, "{name} sounds the same as Lp ({r} vs {reference})");
    }
}

fn settled_pitch(e: &mut Engine) -> f32 {
    pitch(&play(e, 0.9)[24000..])
}

#[test]
fn pitch_bend_moves_pitch_by_its_range() {
    for mode in [VoiceMode::Sampler, VoiceMode::Bore] {
        let mut e = engine();
        e.handle(Cmd::VoiceMode(mode));
        e.handle(Cmd::Meri(0.0));
        e.handle(Cmd::Blow(true));
        let base = settled_pitch(&mut e);
        // default range ≈ 2 semitones either way
        e.handle(Cmd::Fx(FxSet::Bend(1.0)));
        let up = cents(settled_pitch(&mut e), base);
        e.handle(Cmd::Fx(FxSet::Bend(0.0)));
        let down = cents(settled_pitch(&mut e), base);
        println!("{mode:?}: +{up:.0} c / {down:.0} c");
        assert!((up - 199.0).abs() < 40.0, "{mode:?} bend up {up}");
        assert!((down + 199.0).abs() < 40.0, "{mode:?} bend down {down}");
        // widest range: a full octave up
        e.handle(Cmd::Fx(FxSet::BendRange(1.0)));
        e.handle(Cmd::Fx(FxSet::Bend(1.0)));
        let oct = cents(settled_pitch(&mut e), base);
        assert!((oct - 1200.0).abs() < 60.0, "{mode:?} octave bend {oct}");
        // back at centre: back on pitch
        e.handle(Cmd::Fx(FxSet::Bend(0.5)));
        let home = cents(settled_pitch(&mut e), base);
        assert!(home.abs() < 45.0, "{mode:?} centre {home}");
    }
}

/// Spread (max − min) of short-window pitch estimates over a held note.
fn pitch_wobble(e: &mut Engine) -> f32 {
    play(e, 0.6);
    let x = play(e, 2.0);
    let w = 4800;
    let ps: Vec<f32> = (0..x.len() / w - 1).map(|i| pitch(&x[i * w..(i + 1) * w + 400])).collect();
    let lo = ps.iter().cloned().fold(f32::MAX, f32::min);
    let hi = ps.iter().cloned().fold(0f32, f32::max);
    cents(hi, lo)
}

#[test]
fn yuri_adds_vibrato_only_when_asked() {
    for mode in [VoiceMode::Sampler, VoiceMode::Bore] {
        let mut e = engine();
        e.handle(Cmd::VoiceMode(mode));
        e.handle(Cmd::Meri(0.0));
        e.handle(Cmd::Blow(true));
        let flat = pitch_wobble(&mut e);
        e.handle(Cmd::Fx(FxSet::Yuri(1.0)));
        let wob = pitch_wobble(&mut e);
        println!("{mode:?}: wobble {flat:.0} c → {wob:.0} c");
        assert!(wob > flat + 30.0, "{mode:?} yuri wobble {wob} vs {flat}");
    }
}

#[test]
fn glide_slows_note_changes() {
    let early = |glide: f64| {
        let mut e = engine();
        e.handle(Cmd::Meri(0.0));
        e.handle(Cmd::Fx(FxSet::Glide(glide)));
        e.handle(Cmd::Blow(true));
        e.handle(Cmd::Holes(holes_from_index(0)));
        play(&mut e, 0.8);
        let target = FINGERINGS[1].freq as f32;
        e.handle(Cmd::Holes(holes_from_index(1)));
        let x = play(&mut e, 0.14);
        cents(pitch(&x[2400..]), target)
    };
    let (quick, slow) = (early(0.0), early(1.0));
    println!("glide 0: {quick:+.0} c, glide 1: {slow:+.0} c from target");
    assert!(quick.abs() < 90.0, "quick glide {quick}");
    assert!(slow < -120.0, "slow glide should still be climbing: {slow}");
}

#[test]
fn master_limiter_holds_the_ceiling_and_meters_loudness() {
    let mut e = engine();
    e.handle(Cmd::VoiceMode(VoiceMode::Both));
    for s in [FxSet::Gain(1.0), FxSet::FluteLevel(1.0), FxSet::BoreLevel(1.0), FxSet::Reverb(1.0), FxSet::DelayMix(0.8)]
    {
        e.handle(Cmd::Fx(s));
    }
    e.handle(Cmd::FoliLevel(1.0));
    e.handle(Cmd::FoliOn(true));
    e.handle(Cmd::Meri(0.0));
    e.handle(Cmd::Blow(true));
    let mut out = Vec::new();
    render_offline(&mut e, 3.0, &mut out);
    let peak = out.iter().fold(0f32, |m, v| m.max(v.abs()));
    let mut s = iki_core::engine::Snapshot::default();
    e.snapshot(&mut s);
    println!("peak {peak:.3}, loudness {:.1} dBFS, limiter {:.1} dB", s.loud_db, s.limit_gr);
    assert!(peak <= 0.96, "peak {peak}");
    assert!(s.loud_db > -60.0 && s.loud_db < 0.0, "loudness {}", s.loud_db);
    assert!(s.limit_gr >= 0.0);
}

#[test]
fn recorded_loops_export_as_wav() {
    use iki_core::engine::ExportSlot;
    let mut e = engine();
    // nothing recorded yet → empty export
    let slot: ExportSlot = Default::default();
    e.handle(Cmd::ExportLoops(slot.clone()));
    assert!(slot.lock().unwrap().take().unwrap().tracks.is_empty());

    e.handle(Cmd::LoopPunch(1));
    e.handle(Cmd::Blow(true));
    let _ = play(&mut e, 3.0);
    e.handle(Cmd::Blow(false));
    let _ = play(&mut e, 0.2);
    e.handle(Cmd::LoopPunch(1));
    let _ = play(&mut e, 0.3);
    e.handle(Cmd::ExportLoops(slot.clone()));
    let ex = slot.lock().unwrap().take().unwrap();
    assert_eq!(ex.tracks.len(), 1);
    let (idx, data) = &ex.tracks[0];
    assert_eq!(*idx, 1);
    assert!(data.len() as f32 > ex.sample_rate * 2.0, "{} samples", data.len());
    assert!(rms(data) > 0.005, "exported loop is silent");
    let wav = iki_core::shaku::wav::encode_i16(data, 1, ex.sample_rate as u32);
    let (back, ch, sr) = iki_core::shaku::wav::decode_i16(&wav).unwrap();
    assert_eq!((ch, sr as f32, back.len()), (1, ex.sample_rate, data.len()));
}

// ---- audio-thread safety: the render path must not touch the allocator ----

mod rt {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        pub static WATCH: Cell<bool> = const { Cell::new(false) };
        pub static HITS: Cell<usize> = const { Cell::new(0) };
    }

    pub struct Counting;
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            if WATCH.try_with(|w| w.get()).unwrap_or(false) {
                let _ = HITS.try_with(|h| h.set(h.get() + 1));
            }
            unsafe { System.alloc(l) }
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            unsafe { System.dealloc(p, l) }
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            if WATCH.try_with(|w| w.get()).unwrap_or(false) {
                let _ = HITS.try_with(|h| h.set(h.get() + 1));
            }
            unsafe { System.realloc(p, l, n) }
        }
    }
}

#[global_allocator]
static ALLOC: rt::Counting = rt::Counting;

#[test]
fn render_path_never_allocates() {
    let mut e = engine();
    e.handle(Cmd::VoiceMode(VoiceMode::Both));
    e.handle(Cmd::Fx(FxSet::DelayMix(0.4)));
    e.handle(Cmd::Fx(FxSet::Yuri(0.6)));
    e.handle(Cmd::FoliOn(true));
    e.handle(Cmd::LoopPunch(0));
    e.handle(Cmd::Blow(true));
    let mut buf = vec![0f32; 512 * 2];
    // warm up (first-use buffers), then watch
    for _ in 0..40 {
        e.render(&mut buf, 2, |_, _| {});
    }
    rt::WATCH.with(|w| w.set(true));
    rt::HITS.with(|h| h.set(0));
    let mut last = 0;
    for k in 0..400 {
        if k == 50 {
            e.handle(Cmd::Holes(holes_from_index(2)));
        }
        if k == 100 {
            e.handle(Cmd::Fx(FxSet::Bend(0.9)));
            e.handle(Cmd::Fx(FxSet::Reverb(0.5)));
        }
        if k == 150 {
            e.handle(Cmd::LoopPunch(0)); // commit the take → playback
        }
        if k == 250 {
            e.handle(Cmd::Blow(false));
        }
        e.render(&mut buf, 2, |_, _| {});
        let now = rt::HITS.with(|h| h.get());
        if now != last {
            println!("allocation(s) at block {k}: +{}", now - last);
            last = now;
        }
    }
    rt::WATCH.with(|w| w.set(false));
    let hits = rt::HITS.with(|h| h.get());
    assert_eq!(hits, 0, "{hits} allocations on the render path");
}

#[test]
fn tuning_knob_moves_concert_pitch() {
    let mut e = engine();
    e.handle(Cmd::Meri(0.0));
    e.handle(Cmd::Blow(true));
    let a440 = settled_pitch(&mut e);
    e.handle(Cmd::Fx(FxSet::Tune(1.0)));
    let up = cents(settled_pitch(&mut e), a440);
    e.handle(Cmd::Fx(FxSet::Tune(0.0)));
    let down = cents(settled_pitch(&mut e), a440);
    assert!((up - 100.0).abs() < 35.0, "A466 {up}");
    assert!((down + 100.0).abs() < 35.0, "A415 {down}");
}

#[test]
fn meri_sweep_uses_the_recorded_flat_notes_in_tune_and_without_gaps() {
    use iki_core::shaku::fingering::hz_at;
    for note in 0..6 {
        let mut e = engine();
        e.handle(Cmd::Holes(holes_from_index(note)));
        e.handle(Cmd::Meri(0.0));
        e.handle(Cmd::Blow(true));
        play(&mut e, 0.5);
        for depth in [0.0, 0.2, 0.4, 0.55, 0.7, 0.9, 1.0, 0.6, 0.3, 0.0] {
            e.handle(Cmd::Meri(depth));
            let x = play(&mut e, 0.9);
            assert!(rms(&x[24000..]) > 0.01, "note {note} depth {depth}: silent");
            let want = hz_at(note, true, depth) as f32;
            let got = pitch(&x[24000..]);
            let dev = cents(got, want);
            assert!(dev.abs() < 45.0, "note {note} depth {depth}: {got:.1} Hz vs {want:.1} Hz ({dev:+.0} c)");
        }
    }
}

#[test]
fn crossing_a_recording_boundary_does_not_dip_or_click() {
    let mut e = engine();
    e.handle(Cmd::Meri(0.2));
    e.handle(Cmd::Blow(true));
    play(&mut e, 0.8);
    let before = rms(&play(&mut e, 0.2));
    e.handle(Cmd::Meri(0.45)); // kari → chu recording
    let x = play(&mut e, 0.4);
    let worst = x.chunks(480).map(rms).fold(f32::MAX, f32::min);
    assert!(worst > before * 0.4, "level dipped to {worst} from {before}");
    assert!(x.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0f32, f32::max) < 0.5, "click");
}

#[test]
fn breath_selects_the_recorded_dynamic() {
    let level = |iki: f64| {
        let mut e = engine();
        e.handle(Cmd::Meri(0.0));
        e.handle(Cmd::Iki(iki));
        e.handle(Cmd::Blow(true));
        let x = play(&mut e, 1.5);
        let x = &x[30000..];
        let bright = x.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>() / rms(x);
        (rms(x), bright)
    };
    let (soft, soft_b) = level(0.2);
    let (hard, hard_b) = level(0.95);
    println!("soft rms {soft:.4} bright {soft_b:.1} / hard rms {hard:.4} bright {hard_b:.1}");
    assert!(hard > soft * 1.15, "hard {hard} vs soft {soft}");
    assert!(hard_b > soft_b, "hard take should be brighter ({hard_b} vs {soft_b})");
}

#[test]
fn reverb_rooms_are_presets_over_the_knobs() {
    use iki_core::engine::{Fx, REVERB_TYPES};
    let mut fx = Fx::default();
    assert_eq!(fx.reverb_type(), 0, "default is custom");
    for i in 1..REVERB_TYPES.len() {
        fx.set_reverb_type(i);
        assert_eq!(fx.reverb_type(), i, "{}", REVERB_TYPES[i]);
    }
    fx.reverb_size += 0.1;
    assert_eq!(fx.reverb_type(), 0, "moving a knob leaves the preset");
    let before = fx.clone();
    fx.set_reverb_type(0);
    assert_eq!(fx, before, "custom changes nothing");
}

/// RMS of the reverb tail between `from` and `to` seconds after a short note.
fn tail_rms(set: impl Fn(&mut Engine), from: f64, to: f64) -> f32 {
    let mut e = engine();
    e.handle(Cmd::Fx(FxSet::Reverb(0.9)));
    set(&mut e);
    e.handle(Cmd::Blow(true));
    let _ = play(&mut e, 0.6);
    e.handle(Cmd::Blow(false));
    let _ = play(&mut e, from);
    rms(&play(&mut e, to - from))
}

#[test]
fn hall_rings_longer_than_slap_and_size_matters() {
    use iki_core::engine::Fx;
    let room = |i: usize| {
        move |e: &mut Engine| {
            let mut fx = Fx::default();
            fx.set_reverb_type(i);
            e.handle(Cmd::Fx(FxSet::ReverbSize(fx.reverb_size)));
            e.handle(Cmd::Fx(FxSet::ReverbDecay(fx.reverb_decay)));
            e.handle(Cmd::Fx(FxSet::ReverbDamp(fx.reverb_damp)));
            e.handle(Cmd::Fx(FxSet::ReverbPre(fx.reverb_pre)));
        }
    };
    let hall = tail_rms(room(2), 1.2, 2.2);
    let slap = tail_rms(room(4), 1.2, 2.2);
    assert!(hall > slap * 2.0, "hall tail {hall} vs slap {slap}");
    let big = tail_rms(|e| e.handle(Cmd::Fx(FxSet::ReverbSize(1.0))), 1.0, 2.0);
    let small = tail_rms(|e| e.handle(Cmd::Fx(FxSet::ReverbSize(0.0))), 1.0, 2.0);
    assert!(big.is_finite() && small.is_finite() && (big - small).abs() > 1e-4, "size has no effect {big} {small}");
}

#[test]
fn delay_tone_darkens_the_repeats() {
    let bright = |tone: f64| {
        let mut e = engine();
        e.handle(Cmd::Fx(FxSet::DelaySync(false)));
        e.handle(Cmd::Fx(FxSet::DelayTime(0.3)));
        e.handle(Cmd::Fx(FxSet::DelayMix(1.0)));
        e.handle(Cmd::Fx(FxSet::DelayFeedback(0.6)));
        e.handle(Cmd::Fx(FxSet::DelayTone(tone)));
        e.handle(Cmd::Fx(FxSet::FiltCut(1.0))); // open filter: the repeats must have highs to lose
        e.handle(Cmd::Blow(true));
        let _ = play(&mut e, 0.4);
        e.handle(Cmd::Blow(false));
        let _ = play(&mut e, 0.9);
        let x = play(&mut e, 1.0);
        // Second difference: weights the highs far more than the fundamental does.
        let d: Vec<f32> = x.windows(3).map(|w| w[2] - 2.0 * w[1] + w[0]).collect();
        (rms(&d) / rms(&x).max(1e-9), rms(&x))
    };
    let (dark, dark_level) = bright(0.0);
    let (light, light_level) = bright(1.0);
    assert!(dark_level > 1e-4 && light_level > 1e-4, "repeats vanished {dark_level} {light_level}");
    assert!(light > dark * 1.08, "tone does nothing: dark {dark} bright {light}");
}

#[test]
fn filter_cutoff_covers_the_full_range() {
    use iki_core::shaku::env_shape::{FiltType, filt_cutoff};
    assert!(filt_cutoff(FiltType::Lowpass, 0.0) <= 70.0);
    assert!(filt_cutoff(FiltType::Lowpass, 1.0) >= 15000.0);
    let def = filt_cutoff(FiltType::Lowpass, iki_core::engine::Fx::default().filt_cut);
    assert!((600.0..760.0).contains(&def), "default cutoff {def} Hz moved");
    // and the extremes stay stable and audible/inaudible as expected
    let level = |cut: f64| {
        let mut e = engine();
        e.handle(Cmd::Fx(FxSet::FiltCut(cut)));
        e.handle(Cmd::Blow(true));
        let x = play(&mut e, 1.0);
        assert!(x.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        rms(&x[24000..])
    };
    assert!(level(1.0) > level(0.0), "fully closed should be quieter than fully open");
}

#[test]
fn foli_has_no_synth_kit_any_more() {
    assert_eq!(iki_core::shaku::foli::KIT_NAMES.len(), 7);
    assert!(!iki_core::shaku::foli::KIT_NAMES.contains(&"Syn"));
}
