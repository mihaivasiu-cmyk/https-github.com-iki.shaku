//! JNI bridge between the Iki kernel ([`iki_core`]) and the native Android
//! frontend (`org.iki.shakuhachi.Native`, Kotlin — see `kotlin/Native.kt`).
//!
//! The Kotlin side is presentation only: it forwards gestures, sets parameters
//! by numeric id and, once per display frame, calls `tick` and reads a packed
//! state array plus a few strings/curves. All behaviour lives in
//! [`iki_core::Controller`] — the same code the desktop app runs — and so do
//! the readout strings, so both fronts print exactly the same text.
//!
//! The crate also builds for the host (Linux/macOS), so the exact same JNI
//! surface can be exercised from a plain JVM (Robolectric tests).

mod bridge;
mod params;

use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JFloatArray, JObject, JString};
use jni::sys::{jboolean, jbyteArray, jdouble, jint, jlong, jstring};

use iki_core::engine::VoiceMode;
use iki_core::shaku::fingering::Voicing;
use iki_core::shaku::looper::TRACK_COUNT;
use iki_core::shaku::midi;

pub use bridge::*;
pub use params::*;

// ------------------------------------------------------------------ JNI glue
//
// Every export is `org.iki.shakuhachi.Native.<name>` (a Kotlin `object` with
// `@JvmStatic external` functions), taking the handle from `create`.

fn iki<'a>(h: jlong) -> Option<&'a mut Iki> {
    // SAFETY: `h` is a pointer returned by `create` and not yet destroyed; all
    // calls come from the UI thread.
    unsafe { (h as *mut Iki).as_mut() }
}

fn lane(meri: jboolean) -> Voicing {
    if meri != 0 { Voicing::Meri } else { Voicing::Kari }
}

fn jstr(env: &JNIEnv, s: String) -> jstring {
    env.new_string(s).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

#[cfg(target_os = "android")]
fn init_android_context(env: &mut JNIEnv, context: &JObject) {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let Ok(vm) = env.get_java_vm() else { return };
        let Ok(ctx) = env.new_global_ref(context) else { return };
        let raw_ctx = ctx.as_obj().as_raw();
        std::mem::forget(ctx); // lives for the process
        // SAFETY: valid JavaVM and a global reference to the application Context.
        unsafe { ndk_context::initialize_android_context(vm.get_java_vm_pointer().cast(), raw_ctx.cast()) };
    });
}

/// `create(context, silent, voice)`: starts audio and the controller.
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_create(
    mut _env: JNIEnv,
    _class: JClass,
    _context: JObject,
    silent: jboolean,
    voice: jint,
) -> jlong {
    #[cfg(target_os = "android")]
    if !_context.is_null() {
        init_android_context(&mut _env, &_context);
    }
    let voice = VoiceMode::from_index(voice.max(0) as usize);
    Box::into_raw(Box::new(Iki::new(silent != 0, voice))) as jlong
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_destroy(_env: JNIEnv, _class: JClass, h: jlong) {
    if h != 0 {
        // SAFETY: pointer from `create`; Kotlin guarantees a single destroy.
        drop(unsafe { Box::from_raw(h as *mut Iki) });
    }
}

/// App to the background (`true`) / foreground: releases held notes and the audio device.
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_pause(_env: JNIEnv, _class: JClass, h: jlong, paused: jboolean) {
    if let Some(i) = iki(h) {
        i.ctl.set_audio_paused(paused != 0);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_tick(_env: JNIEnv, _class: JClass, h: jlong) {
    if let Some(i) = iki(h) {
        i.tick();
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_gesture(
    _env: JNIEnv,
    _class: JClass,
    h: jlong,
    gesture: jint,
    index: jint,
    meri: jboolean,
) {
    let Some(i) = iki(h) else { return };
    if gesture == G_PHRASE {
        let _ = i.play_clip(midi::PHRASE_MID, "phrase");
        return;
    }
    let c = &mut i.ctl;
    let idx = index.clamp(0, 5) as usize;
    match gesture {
        G_HIT_START => c.on_hit_start(idx, lane(meri)),
        G_SLIDE_START => c.on_slide_start(idx, lane(meri)),
        G_SLIDE_MOVE => c.park_note(idx, lane(meri)),
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
pub extern "system" fn Java_org_iki_shakuhachi_Native_loop(
    _env: JNIEnv,
    _class: JClass,
    h: jlong,
    op: jint,
    track: jint,
    value: jdouble,
) {
    let Some(i) = iki(h) else { return };
    let c = &mut i.ctl;
    let t = track.clamp(0, TRACK_COUNT as i32 - 1) as usize;
    match op {
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
            let (a, b) = if op == L_TRIM_IN { (value, tr.trim_out) } else { (tr.trim_in, value) };
            c.set_loop_bounds(t, a, b);
        }
        _ => {}
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_set(
    _env: JNIEnv,
    _class: JClass,
    h: jlong,
    id: jint,
    value: jdouble,
) {
    if let Some(i) = iki(h) {
        set_param(&mut i.ctl, id, value);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_get(_env: JNIEnv, _class: JClass, h: jlong, id: jint) -> jdouble {
    iki(h).map(|i| get_param(&i.ctl, id)).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_hint<'l>(
    env: JNIEnv<'l>,
    _class: JClass,
    h: jlong,
    id: jint,
) -> jstring {
    let s = iki(h).map(|i| param_hint(&i.ctl, id)).unwrap_or_default();
    jstr(&env, s)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_text<'l>(
    env: JNIEnv<'l>,
    _class: JClass,
    h: jlong,
    which: jint,
) -> jstring {
    let s = iki(h).map(|i| i.text(which)).unwrap_or_default();
    jstr(&env, s)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_state(env: JNIEnv, _class: JClass, h: jlong, out: JFloatArray) {
    let Some(i) = iki(h) else { return };
    let mut v = [0f32; STATE_LEN];
    i.state(&mut v);
    let n = env.get_array_length(&out).unwrap_or(0).max(0) as usize;
    let _ = env.set_float_array_region(&out, 0, &v[..n.min(STATE_LEN)]);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_spectrum(
    env: JNIEnv,
    _class: JClass,
    h: jlong,
    out: JFloatArray,
) {
    let Some(i) = iki(h) else { return };
    let mut v = [0f32; SPEC_ROWS * 2];
    i.spectrum_column(&mut v);
    let n = env.get_array_length(&out).unwrap_or(0).max(0) as usize;
    let _ = env.set_float_array_region(&out, 0, &v[..n.min(v.len())]);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_curve(
    env: JNIEnv,
    _class: JClass,
    h: jlong,
    which: jint,
    out: JFloatArray,
) {
    let Some(i) = iki(h) else { return };
    let n = env.get_array_length(&out).unwrap_or(0).clamp(0, 1024) as usize;
    let mut v = vec![0f32; n];
    i.curve(which, &mut v);
    let _ = env.set_float_array_region(&out, 0, &v);
}

/// Loop waveform peaks for a track; returns how many bins were written (0 = empty).
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_loopWave(
    env: JNIEnv,
    _class: JClass,
    h: jlong,
    track: jint,
    out: JFloatArray,
) -> jint {
    let Some(i) = iki(h) else { return 0 };
    let Some(t) = i.ctl.snap.loop_bank.tracks.get(track.max(0) as usize) else { return 0 };
    let n = (env.get_array_length(&out).unwrap_or(0).max(0) as usize).min(t.wave.len());
    let _ = env.set_float_array_region(&out, 0, &t.wave[..n]);
    n as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_key(
    mut env: JNIEnv,
    _class: JClass,
    h: jlong,
    name: JString,
    down: jboolean,
) -> jboolean {
    let Some(i) = iki(h) else { return 0 };
    let name: String = env.get_string(&name).map(|s| s.into()).unwrap_or_default();
    i.key(&name, down != 0) as jboolean
}

/// Raw bytes from a MIDI input port (notes, bend → meri, CC → iki/punch, sustain).
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_midiIn(env: JNIEnv, _class: JClass, h: jlong, bytes: JByteArray) {
    let Some(i) = iki(h) else { return };
    if let Ok(b) = env.convert_byte_array(&bytes) {
        i.ctl.midi_in(&b);
    }
}

/// Bytes to send to the MIDI output this frame (empty when there is nothing).
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_midiOut(
    env: JNIEnv,
    _class: JClass,
    h: jlong,
    tx: jboolean,
) -> jbyteArray {
    let out = iki(h).map(|i| i.midi_out(tx != 0)).unwrap_or_default();
    env.byte_array_from_slice(&out).map(|a| a.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Play a .mid file's bytes; returns "" or the parse error.
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_playMidi<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass,
    h: jlong,
    bytes: JByteArray,
    name: JString,
) -> jstring {
    let Some(i) = iki(h) else { return std::ptr::null_mut() };
    let name: String = env.get_string(&name).map(|s| s.into()).unwrap_or_default();
    let err = match env.convert_byte_array(&bytes) {
        Ok(b) => i.play_clip(&b, &name).err().unwrap_or_default(),
        Err(e) => e.to_string(),
    };
    jstr(&env, err)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_iki_shakuhachi_Native_stateLen(_env: JNIEnv, _class: JClass) -> jint {
    STATE_LEN as jint
}
