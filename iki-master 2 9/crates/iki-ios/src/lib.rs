//! C ABI between the Iki kernel ([`iki_core`]) and the iOS app (SwiftUI, see
//! `ios/`). Same shape as the Android JNI bridge: the Swift side is presentation
//! only — it forwards gestures, sets parameters by numeric id and, once per
//! display frame, calls `iki_tick` and reads a packed state array plus a few
//! strings — while all behaviour lives in [`iki_core::Controller`].
//!
//! Audio: the app runs an `AVAudioEngine` with an `AVAudioSourceNode`; its render
//! block calls [`iki_render`] on the audio thread. Everything else is called from
//! the main thread. Destroy order: stop the audio engine, then `iki_destroy`.

#![allow(clippy::missing_safety_doc)]

#[allow(dead_code)]
#[path = "../../iki-android/src/bridge.rs"]
mod bridge;
#[allow(dead_code)]
#[path = "../../iki-android/src/params.rs"]
mod params;

use std::ffi::{CStr, c_char, c_int};
use std::time::Duration;

use iki_core::audio::{Renderer, external};
use iki_core::engine::VoiceMode;
use iki_core::shaku::fingering::Voicing;
use iki_core::shaku::looper::TRACK_COUNT;
use iki_core::shaku::midi;

use bridge::{Iki, SPEC_ROWS};
use params::*;

/// Everything the audio thread needs, allocated up front so rendering never allocates.
pub struct RenderCtx {
    renderer: Renderer,
    scratch: Vec<f32>,
}

/// The instrument plus the render context its audio callback drives.
pub struct Handle {
    iki: Iki,
    render: Box<RenderCtx>,
}

const MAX_BLOCK: usize = 4096;

fn h<'a>(p: *mut Handle) -> Option<&'a mut Iki> {
    // SAFETY: `p` comes from `iki_create` and is not yet destroyed; called from the main thread.
    unsafe { p.as_mut() }.map(|h| &mut h.iki)
}

fn lane(meri: c_int) -> Voicing {
    if meri != 0 { Voicing::Meri } else { Voicing::Kari }
}

/// Copy `s` into `buf` (capacity `cap`, NUL-terminated); returns the full length in bytes
/// (so a caller whose buffer was too small can retry with a bigger one).
unsafe fn put_str(s: &str, buf: *mut c_char, cap: c_int) -> c_int {
    if !buf.is_null() && cap > 0 {
        let n = s.len().min(cap as usize - 1);
        // Never cut a UTF-8 character in half.
        let mut n = n;
        while n > 0 && !s.is_char_boundary(n) {
            n -= 1;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(s.as_ptr(), buf as *mut u8, n);
            *buf.add(n) = 0;
        }
    }
    s.len() as c_int
}

unsafe fn cstr(p: *const c_char) -> String {
    if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
}

/// Create the instrument at the audio session's sample rate. `voice`: 0 flute, 1 bore, 2 both.
#[unsafe(no_mangle)]
pub extern "C" fn iki_create(sample_rate: f32, voice: c_int) -> *mut Handle {
    let ext = external(sample_rate.clamp(8000.0, 192000.0), format!("iPhone · {sample_rate:.0} Hz"));
    let mut audio = ext.handle;
    audio.scope = ext.scope; // the spectrogram reads what the audio callback produced
    let iki = Iki::with_audio(audio, VoiceMode::from_index(voice.max(0) as usize));
    let render = Box::new(RenderCtx { renderer: ext.renderer, scratch: vec![0.0; MAX_BLOCK * 2] });
    Box::into_raw(Box::new(Handle { iki, render }))
}

/// The pointer the audio callback passes to [`iki_render`]. Valid until `iki_destroy`.
#[unsafe(no_mangle)]
pub extern "C" fn iki_render_ctx(p: *mut Handle) -> *mut RenderCtx {
    // SAFETY: see `h`.
    match unsafe { p.as_mut() } {
        Some(handle) => &mut *handle.render as *mut RenderCtx,
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn iki_destroy(p: *mut Handle) {
    if !p.is_null() {
        // SAFETY: pointer from `iki_create`; the caller destroys once, after stopping audio.
        drop(unsafe { Box::from_raw(p) });
    }
}

/// Audio thread: render `frames` frames into the two (non-interleaved) channel buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iki_render(ctx: *mut RenderCtx, left: *mut f32, right: *mut f32, frames: c_int) {
    let Some(ctx) = (unsafe { ctx.as_mut() }) else { return };
    if left.is_null() || right.is_null() || frames <= 0 {
        return;
    }
    let (l, r) = unsafe {
        (std::slice::from_raw_parts_mut(left, frames as usize), std::slice::from_raw_parts_mut(right, frames as usize))
    };
    ctx.renderer.set_period(frames as usize);
    let mut done = 0;
    while done < l.len() {
        let n = (l.len() - done).min(MAX_BLOCK);
        let buf = &mut ctx.scratch[..n * 2];
        ctx.renderer.render(buf, 2);
        for i in 0..n {
            l[done + i] = buf[i * 2];
            r[done + i] = buf[i * 2 + 1];
        }
        done += n;
    }
}

/// Block (off the main thread!) until the sample bank is decoded; false on timeout.
#[unsafe(no_mangle)]
pub extern "C" fn iki_wait_for_samples(p: *mut Handle, timeout_ms: c_int) -> bool {
    h(p).map(|i| i.ctl.wait_for_samples(Duration::from_millis(timeout_ms.max(0) as u64))).unwrap_or(false)
}

/// App to the background (`true`) / foreground: releases held notes and stops loops.
#[unsafe(no_mangle)]
pub extern "C" fn iki_pause(p: *mut Handle, paused: bool) {
    if let Some(i) = h(p) {
        i.ctl.set_audio_paused(paused);
    }
}

/// Once per display frame: pulls engine state, loads samples, pushes changed parameters.
#[unsafe(no_mangle)]
pub extern "C" fn iki_tick(p: *mut Handle) {
    if let Some(i) = h(p) {
        i.tick();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn iki_gesture(p: *mut Handle, gesture: c_int, index: c_int, meri_lane: c_int) {
    let Some(i) = h(p) else { return };
    if gesture as i32 == G_PHRASE {
        let _ = i.play_clip(midi::PHRASE_MID, "phrase");
        return;
    }
    let c = &mut i.ctl;
    let idx = index.clamp(0, 5) as usize;
    match gesture as i32 {
        G_HIT_START => c.on_hit_start(idx, lane(meri_lane)),
        G_SLIDE_START => c.on_slide_start(idx, lane(meri_lane)),
        G_SLIDE_MOVE => c.park_note(idx, lane(meri_lane)),
        G_SLIDE_END => c.on_slide_end(),
        G_TOGGLE_GATE => c.toggle_gate(),
        G_MURAIKI => c.shoot_muraiki(),
        G_FOLI_STEP => c.foli(iki_core::engine::Cmd::FoliToggleStep(index.clamp(0, 11) as usize)),
        G_RELEASE_ALL => {
            c.release_keys();
            if c.blowing {
                c.blow(Some(false));
            }
        }
        _ => {}
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn iki_loop(p: *mut Handle, op: c_int, track: c_int, value: f64) {
    let Some(i) = h(p) else { return };
    let c = &mut i.ctl;
    let t = track.clamp(0, TRACK_COUNT as c_int - 1) as usize;
    match op as i32 {
        L_PUNCH => c.on_punch(t),
        L_PLAY => c.on_loop_play(t),
        L_STOP => c.on_loop_stop(t),
        L_STOP_ALL => c.on_loop_stop_all(),
        L_UNDO => c.on_loop_undo(t),
        L_FREEZE => c.on_freeze(),
        L_LEVEL => c.set_loop_level(t, value.clamp(0.0, 1.0)),
        L_FX => c.set_loop_fx(t, value > 0.5),
        L_SELECT => c.loop_track = t,
        L_TRIM_IN | L_TRIM_OUT => {
            let Some(tr) = c.snap.loop_bank.tracks.get(t) else { return };
            let (a, b) = if op as i32 == L_TRIM_IN { (value, tr.trim_out) } else { (tr.trim_in, value) };
            c.set_loop_bounds(t, a, b);
        }
        _ => {}
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn iki_set(p: *mut Handle, id: c_int, value: f64) {
    if let Some(i) = h(p) {
        set_param(&mut i.ctl, id, value);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn iki_get(p: *mut Handle, id: c_int) -> f64 {
    h(p).map(|i| get_param(&i.ctl, id)).unwrap_or(0.0)
}

/// Readout under a knob ("64 ms", "A 440.0" …). Returns the string's length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iki_hint(p: *mut Handle, id: c_int, buf: *mut c_char, cap: c_int) -> c_int {
    let s = h(p).map(|i| param_hint(&i.ctl, id)).unwrap_or_default();
    unsafe { put_str(&s, buf, cap) }
}

/// A display string (`TEXT_*` in Ids.swift). Returns the string's length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iki_text(p: *mut Handle, which: c_int, buf: *mut c_char, cap: c_int) -> c_int {
    let s = h(p).map(|i| i.text(which)).unwrap_or_default();
    unsafe { put_str(&s, buf, cap) }
}

#[unsafe(no_mangle)]
pub extern "C" fn iki_state_len() -> c_int {
    STATE_LEN as c_int
}

/// Fill `out` (length `len`) with the packed per-frame state (`STATE_*` indices).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iki_state(p: *mut Handle, out: *mut f32, len: c_int) {
    let Some(i) = h(p) else { return };
    if out.is_null() || len <= 0 {
        return;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, len as usize) };
    i.state(out);
}

/// One spectrogram column: `SPEC_ROWS` (72) values for L then 72 for R, top = high, 0…1.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iki_spectrum(p: *mut Handle, out: *mut f32, len: c_int) {
    let Some(i) = h(p) else { return };
    if out.is_null() || len <= 0 {
        return;
    }
    let mut v = [0f32; SPEC_ROWS * 2];
    i.spectrum_column(&mut v);
    let n = (len as usize).min(v.len());
    unsafe { std::ptr::copy_nonoverlapping(v.as_ptr(), out, n) };
}

/// Envelope / filter curve (`CURVE_*`); the last element is the guide line.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iki_curve(p: *mut Handle, which: c_int, out: *mut f32, len: c_int) {
    let Some(i) = h(p) else { return };
    if out.is_null() || len < 2 {
        return;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, (len as usize).min(1024)) };
    i.curve(which, out);
}

/// Loop waveform peaks for a track; returns how many bins were written (0 = empty).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iki_loop_wave(p: *mut Handle, track: c_int, out: *mut f32, len: c_int) -> c_int {
    let Some(i) = h(p) else { return 0 };
    let Some(t) = i.ctl.snap.loop_bank.tracks.get(track.max(0) as usize) else { return 0 };
    if out.is_null() || len <= 0 {
        return 0;
    }
    let n = (len as usize).min(t.wave.len());
    unsafe { std::ptr::copy_nonoverlapping(t.wave.as_ptr(), out, n) };
    n as c_int
}

/// Raw MIDI bytes from an input (CoreMIDI): notes, pitch wheel → bend, CC → yuri/iki/meri/punch, sustain.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iki_midi_in(p: *mut Handle, bytes: *const u8, len: c_int) {
    let Some(i) = h(p) else { return };
    if bytes.is_null() || len <= 0 {
        return;
    }
    i.ctl.midi_in(unsafe { std::slice::from_raw_parts(bytes, len as usize) });
}

/// Play a Standard MIDI File through the instrument. Returns 0 on success, otherwise the
/// error text is in `err` and the return value is its length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iki_play_midi(
    p: *mut Handle,
    bytes: *const u8,
    len: c_int,
    name: *const c_char,
    err: *mut c_char,
    cap: c_int,
) -> c_int {
    let Some(i) = h(p) else { return 0 };
    if bytes.is_null() || len <= 0 {
        return 0;
    }
    let data = unsafe { std::slice::from_raw_parts(bytes, len as usize) };
    match i.play_clip(data, &unsafe { cstr(name) }) {
        Ok(()) => 0,
        Err(e) => unsafe { put_str(&e, err, cap) }.max(1),
    }
}

/// The current patch as text (see `shaku::preset`). Returns the full length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iki_preset_text(p: *mut Handle, name: *const c_char, buf: *mut c_char, cap: c_int) -> c_int {
    let Some(i) = h(p) else { return 0 };
    let s = preset_text(&i.ctl, &unsafe { cstr(name) });
    unsafe { put_str(&s, buf, cap) }
}

/// Load a patch from text. Returns 1 on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iki_load_preset(p: *mut Handle, text: *const c_char) -> c_int {
    let Some(i) = h(p) else { return 0 };
    load_preset(&mut i.ctl, &unsafe { cstr(text) }).is_ok() as c_int
}

#[unsafe(no_mangle)]
pub extern "C" fn iki_factory_count() -> c_int {
    factory_names().len() as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iki_factory_name(index: c_int, buf: *mut c_char, cap: c_int) -> c_int {
    let names = factory_names();
    match names.get(index.max(0) as usize) {
        Some(n) => unsafe { put_str(n, buf, cap) },
        None => 0,
    }
}

/// Load factory patch `index`. Returns 1 on success.
#[unsafe(no_mangle)]
pub extern "C" fn iki_load_factory(p: *mut Handle, index: c_int) -> c_int {
    h(p).map(|i| load_factory(&mut i.ctl, index.max(0) as usize).is_ok() as c_int).unwrap_or(0)
}

/// A parameter's default value (what double-tap resets to).
#[unsafe(no_mangle)]
pub extern "C" fn iki_default(id: c_int) -> f64 {
    get_param(&iki_core::controller::Controller::detached(), id)
}

/// Ask the engine to copy every recorded loop; poll with [`iki_export_take`].
#[unsafe(no_mangle)]
pub extern "C" fn iki_export_request(p: *mut Handle) {
    if let Some(i) = h(p) {
        i.ctl.request_loop_export();
    }
}

/// When the export is ready, writes a 16-bit WAV mixdown of the loops to `out` (capacity `cap` bytes)
/// and returns its size; 0 = not ready (or nothing recorded); negative = `cap` too small (−size).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iki_export_take(p: *mut Handle, out: *mut u8, cap: c_int) -> c_int {
    let Some(i) = h(p) else { return 0 };
    let Some(ex) = i.ctl.take_loop_export() else { return 0 };
    let len = ex.tracks.iter().map(|t| t.1.len()).max().unwrap_or(0);
    if len == 0 {
        return 0;
    }
    let mut mix = vec![0f32; len];
    for (_, data) in &ex.tracks {
        for (k, m) in mix.iter_mut().enumerate() {
            *m += data[k % data.len()] * 0.7;
        }
    }
    let wav = iki_core::shaku::wav::encode_i16(&mix, 1, ex.sample_rate as u32);
    if out.is_null() || (cap as usize) < wav.len() {
        return -(wav.len() as c_int);
    }
    unsafe { std::ptr::copy_nonoverlapping(wav.as_ptr(), out, wav.len()) };
    wav.len() as c_int
}
