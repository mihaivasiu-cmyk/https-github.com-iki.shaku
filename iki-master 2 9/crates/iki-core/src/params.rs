//! The instrument's parameter table: numeric ids for every knob, switch and
//! choice, with get/set on a [`Controller`] and the readout under each knob.
//! Shared by the frontends that address parameters by number — the Android
//! JNI bridge and the plugin (host automation).

use crate::controller::Controller;
use crate::dsp::Wave;
use crate::engine::{FiltAuto, VoiceMode};
use crate::shaku::env_shape::{ENV_SHAPES, EnvShape, FILT_TYPES, env_seconds, filt_cutoff};
use crate::shaku::foli::FoliFeel;
use crate::shaku::timing::{DELAY_DIVS, delay_sec_bpm, format_hz, lfo_hz_bpm, log_hz};

/// `ids!(GROUP: NAME = value, …)`: one `pub const` per id plus a `GROUP` table of (name, id).
#[macro_export]
macro_rules! ids {
    ($group:ident: $($name:ident = $v:expr),* $(,)?) => {
        $(pub const $name: i32 = $v;)*
        pub const $group: &[(&str, i32)] = &[$((stringify!($name), $v)),*];
    };
}

// Continuous parameters, 0…1 (knobs and faders).
ids!(CONTINUOUS:
    P_MERI = 0, P_LEAN = 1, P_IKI = 2, P_GAIN = 3,
    P_DELAY_TIME = 4, P_DELAY_FEEDBACK = 5, P_DELAY_MIX = 6, P_TEMPO = 7,
    P_REVERB = 8, P_REVERB_DECAY = 9, P_PHASER = 10, P_PHASER_DEPTH = 11, P_TAPE = 12, P_WIDTH = 13,
    P_LFO_RATE = 14, P_LFO_PITCH_AMT = 15, P_LFO_FILT_AMT = 16, P_LFO_AMP_AMT = 17,
    P_FILT_CUT = 18, P_FILT_RES = 19, P_FILT_MOTION = 20, P_FILT_RATE = 21,
    P_ENV_A = 22, P_ENV_D = 23, P_ENV_S = 24, P_ENV_R = 25,
    P_FENV_A = 26, P_FENV_D = 27, P_FENV_S = 28, P_FENV_R = 29,
    P_FOLI_LEVEL = 30, P_FLUTE_LEVEL = 31, P_BORE_LEVEL = 32,
    P_BEND = 33, P_BEND_RANGE = 34, P_YURI = 35, P_YURI_RATE = 36, P_GLIDE = 37, P_TUNE = 38,
    P_DELAY_TONE = 50, P_DELAY_LOWCUT = 51, P_REVERB_SIZE = 52, P_REVERB_PRE = 53, P_REVERB_DAMP = 54,
);

// On/off switches (value 0 or 1).
ids!(SWITCHES:
    P_OTSU = 40, P_DELAY_PING = 41, P_DELAY_SYNC = 42, P_LFO_SYNC = 43, P_LFO_PITCH = 44, P_LFO_FILT = 45,
    P_LFO_AMP = 46, P_ENV_LOOP = 47, P_FENV_LOOP = 48, P_FOLI_ON = 49,
);

// Choices (value = index into the matching `*_LABELS` list).
ids!(CHOICES:
    P_FILT_TYPE = 60, P_FILT_AUTO = 61, P_ENV_SHAPE = 62, P_FENV_SHAPE = 63, P_LFO_WAVE = 64,
    P_DELAY_DIV = 65, P_LFO_DIV = 66, P_VOICE = 67, P_FOLI_FEEL = 68, P_FOLI_KIT = 69, P_REVERB_TYPE = 70,
);

/// Labels for each choice parameter, in value order.
pub fn choice_labels(id: i32) -> Vec<&'static str> {
    match id {
        P_FILT_TYPE => FILT_TYPES.iter().map(|f| f.1).collect(),
        P_FILT_AUTO => vec!["Off", "Env", "Sweep"],
        P_ENV_SHAPE | P_FENV_SHAPE => ENV_SHAPES.iter().map(|s| s.1).collect(),
        P_LFO_WAVE => Wave::ALL.iter().map(|w| w.1).collect(),
        P_DELAY_DIV | P_LFO_DIV => DELAY_DIVS.iter().map(|d| d.id).collect(),
        P_VOICE => vec!["Flute", "Bore", "Both"],
        P_FOLI_FEEL => FoliFeel::ALL.iter().map(|f| f.1).collect(),
        P_FOLI_KIT => crate::shaku::foli::KIT_NAMES.to_vec(),
        P_REVERB_TYPE => crate::engine::REVERB_TYPES.to_vec(),
        _ => Vec::new(),
    }
}

fn b(v: f64) -> bool {
    v > 0.5
}

fn idx(v: f64, n: usize) -> usize {
    (v.round().max(0.0) as usize).min(n.saturating_sub(1))
}

const SHAPES: [EnvShape; 5] = [EnvShape::Lin, EnvShape::Exp, EnvShape::Punch, EnvShape::Trap, EnvShape::Fold];
const AUTOS: [FiltAuto; 3] = [FiltAuto::Off, FiltAuto::Env, FiltAuto::Sweep];
const FEELS: [FoliFeel; 3] = [FoliFeel::Work, FoliFeel::Dance, FoliFeel::Pulse];

pub fn set_param(c: &mut Controller, id: i32, v: f64) {
    let u = v.clamp(0.0, 1.0);
    let fx = &mut c.fx;
    match id {
        P_MERI => c.meri = u,
        P_LEAN => c.lean = u,
        P_IKI => c.iki = u,
        P_GAIN => fx.gain = u,
        P_FLUTE_LEVEL => fx.flute_level = u,
        P_BORE_LEVEL => fx.bore_level = u,
        P_BEND => fx.bend = u,
        P_BEND_RANGE => fx.bend_range = u,
        P_YURI => fx.yuri = u,
        P_YURI_RATE => fx.yuri_rate = u,
        P_GLIDE => fx.glide = u,
        P_TUNE => fx.tune = u,
        P_DELAY_TIME => fx.delay_time = u,
        P_DELAY_FEEDBACK => fx.delay_feedback = u,
        P_DELAY_MIX => fx.delay_mix = u,
        P_TEMPO => fx.delay_tempo = u,
        P_REVERB => fx.reverb = u,
        P_REVERB_DECAY => fx.reverb_decay = u,
        P_DELAY_TONE => fx.delay_tone = u,
        P_DELAY_LOWCUT => fx.delay_lowcut = u,
        P_REVERB_SIZE => fx.reverb_size = u,
        P_REVERB_PRE => fx.reverb_pre = u,
        P_REVERB_DAMP => fx.reverb_damp = u,
        P_PHASER => fx.phaser = u,
        P_PHASER_DEPTH => fx.phaser_depth = u,
        P_TAPE => fx.tape = u,
        P_WIDTH => fx.width = u,
        P_LFO_RATE => fx.mod_rate = u,
        P_LFO_PITCH_AMT => fx.lfo_pitch_amt = u,
        P_LFO_FILT_AMT => fx.lfo_filt_amt = u,
        P_LFO_AMP_AMT => fx.lfo_amp_amt = u,
        P_FILT_CUT => fx.filt_cut = u,
        P_FILT_RES => fx.filt_res = u,
        P_FILT_MOTION => fx.filt_motion = u,
        P_FILT_RATE => fx.filt_rate = u,
        P_ENV_A => fx.env_a = u,
        P_ENV_D => fx.env_d = u,
        P_ENV_S => fx.env_s = u,
        P_ENV_R => fx.env_r = u,
        P_FENV_A => fx.fenv_a = u,
        P_FENV_D => fx.fenv_d = u,
        P_FENV_S => fx.fenv_s = u,
        P_FENV_R => fx.fenv_r = u,
        P_FOLI_LEVEL => c.set_foli_level(u),
        P_OTSU => c.otsu = b(v),
        P_DELAY_PING => fx.delay_ping = b(v),
        P_DELAY_SYNC => fx.delay_sync = b(v),
        P_LFO_SYNC => fx.lfo_sync = b(v),
        P_LFO_PITCH => fx.lfo_pitch = b(v),
        P_LFO_FILT => fx.lfo_filt = b(v),
        P_LFO_AMP => fx.lfo_amp = b(v),
        P_ENV_LOOP => fx.env_loop = b(v),
        P_FENV_LOOP => fx.fenv_loop = b(v),
        P_FOLI_ON => {
            if b(v) != c.snap.foli_on {
                c.foli(crate::engine::Cmd::FoliOn(b(v)));
                c.snap.foli_on = b(v);
            }
        }
        P_FILT_TYPE => fx.filt_type = FILT_TYPES[idx(v, FILT_TYPES.len())].0,
        P_FILT_AUTO => fx.filt_auto = AUTOS[idx(v, 3)],
        P_ENV_SHAPE => fx.env_shape = SHAPES[idx(v, 5)],
        P_FENV_SHAPE => fx.fenv_shape = SHAPES[idx(v, 5)],
        P_LFO_WAVE => fx.lfo_wave = Wave::ALL[idx(v, 4)].0,
        P_DELAY_DIV => fx.delay_div = idx(v, DELAY_DIVS.len()),
        P_LFO_DIV => fx.lfo_div = idx(v, DELAY_DIVS.len()),
        P_VOICE => c.set_voice_mode(VoiceMode::from_index(idx(v, 3))),
        P_FOLI_FEEL => c.set_foli_feel(FEELS[idx(v, 3)]),
        P_FOLI_KIT => c.set_foli_kit(idx(v, crate::shaku::foli::KIT_NAMES.len())),
        P_REVERB_TYPE => fx.set_reverb_type(idx(v, crate::engine::REVERB_TYPES.len())),
        _ => {}
    }
}

pub fn get_param(c: &Controller, id: i32) -> f64 {
    let fx = &c.fx;
    let f = |x: bool| x as u8 as f64;
    match id {
        P_MERI => c.meri,
        P_LEAN => c.lean,
        P_IKI => c.iki,
        P_GAIN => fx.gain,
        P_FLUTE_LEVEL => fx.flute_level,
        P_BORE_LEVEL => fx.bore_level,
        P_BEND => fx.bend,
        P_BEND_RANGE => fx.bend_range,
        P_YURI => fx.yuri,
        P_YURI_RATE => fx.yuri_rate,
        P_GLIDE => fx.glide,
        P_TUNE => fx.tune,
        P_DELAY_TIME => fx.delay_time,
        P_DELAY_FEEDBACK => fx.delay_feedback,
        P_DELAY_MIX => fx.delay_mix,
        P_TEMPO => fx.delay_tempo,
        P_REVERB => fx.reverb,
        P_REVERB_DECAY => fx.reverb_decay,
        P_DELAY_TONE => fx.delay_tone,
        P_DELAY_LOWCUT => fx.delay_lowcut,
        P_REVERB_SIZE => fx.reverb_size,
        P_REVERB_PRE => fx.reverb_pre,
        P_REVERB_DAMP => fx.reverb_damp,
        P_PHASER => fx.phaser,
        P_PHASER_DEPTH => fx.phaser_depth,
        P_TAPE => fx.tape,
        P_WIDTH => fx.width,
        P_LFO_RATE => fx.mod_rate,
        P_LFO_PITCH_AMT => fx.lfo_pitch_amt,
        P_LFO_FILT_AMT => fx.lfo_filt_amt,
        P_LFO_AMP_AMT => fx.lfo_amp_amt,
        P_FILT_CUT => fx.filt_cut,
        P_FILT_RES => fx.filt_res,
        P_FILT_MOTION => fx.filt_motion,
        P_FILT_RATE => fx.filt_rate,
        P_ENV_A => fx.env_a,
        P_ENV_D => fx.env_d,
        P_ENV_S => fx.env_s,
        P_ENV_R => fx.env_r,
        P_FENV_A => fx.fenv_a,
        P_FENV_D => fx.fenv_d,
        P_FENV_S => fx.fenv_s,
        P_FENV_R => fx.fenv_r,
        P_FOLI_LEVEL => c.foli_level,
        P_OTSU => f(c.otsu),
        P_DELAY_PING => f(fx.delay_ping),
        P_DELAY_SYNC => f(fx.delay_sync),
        P_LFO_SYNC => f(fx.lfo_sync),
        P_LFO_PITCH => f(fx.lfo_pitch),
        P_LFO_FILT => f(fx.lfo_filt),
        P_LFO_AMP => f(fx.lfo_amp),
        P_ENV_LOOP => f(fx.env_loop),
        P_FENV_LOOP => f(fx.fenv_loop),
        P_FOLI_ON => f(c.snap.foli_on),
        P_FILT_TYPE => FILT_TYPES.iter().position(|t| t.0 == fx.filt_type).unwrap_or(0) as f64,
        P_FILT_AUTO => AUTOS.iter().position(|a| *a == fx.filt_auto).unwrap_or(1) as f64,
        P_ENV_SHAPE => SHAPES.iter().position(|s| *s == fx.env_shape).unwrap_or(0) as f64,
        P_FENV_SHAPE => SHAPES.iter().position(|s| *s == fx.fenv_shape).unwrap_or(0) as f64,
        P_LFO_WAVE => Wave::ALL.iter().position(|w| w.0 == fx.lfo_wave).unwrap_or(0) as f64,
        P_DELAY_DIV => fx.delay_div as f64,
        P_LFO_DIV => fx.lfo_div as f64,
        P_VOICE => c.voice_mode.index() as f64,
        P_FOLI_FEEL => FEELS.iter().position(|x| *x == c.foli_feel).unwrap_or(0) as f64,
        P_FOLI_KIT => c.foli_kit as f64,
        P_REVERB_TYPE => fx.reverb_type() as f64,
        _ => 0.0,
    }
}

/// The small readout under each knob — same wording as the desktop/web UI.
pub fn param_hint(c: &Controller, id: i32) -> String {
    let fx = &c.fx;
    let pct = |v: f64| format!("{}", (v * 100.0).round());
    let ms = |s: f64| format!("{} ms", (s * 1000.0).round());
    let amp = env_seconds(fx.env_a, fx.env_d, fx.env_s, fx.env_r);
    let filt = env_seconds(fx.fenv_a, fx.fenv_d, fx.fenv_s, fx.fenv_r);
    match id {
        P_MERI => "Flatten".into(),
        P_IKI => "Breath".into(),
        P_LEAN => format!("lean {}", pct(c.lean)),
        P_TEMPO => c.bpm_readout(),
        P_DELAY_TIME => {
            let s = delay_sec_bpm(fx.delay_sync, fx.delay_time, fx.delay_div, c.bpm());
            if s >= 1.0 { format!("{s:.2} s") } else { ms(s) }
        }
        P_LFO_RATE => format_hz(lfo_hz_bpm(fx.lfo_sync, fx.mod_rate, fx.lfo_div, c.bpm())),
        P_LFO_PITCH_AMT => format!("±{} c", (fx.lfo_pitch_amt * 120.0).round()),
        P_LFO_FILT_AMT => format!("{} Hz", (fx.lfo_filt_amt * 2400.0).round()),
        P_DELAY_TONE => format!("{} Hz", (1200.0 * 10f64.powf(fx.delay_tone)).round()),
        P_DELAY_LOWCUT => format!("{} Hz", (40.0 * 20f64.powf(fx.delay_lowcut)).round()),
        P_REVERB_SIZE => format!("×{:.2}", 0.5 * 4f64.powf(fx.reverb_size)),
        P_REVERB_PRE => ms(fx.reverb_pre * 0.12),
        P_REVERB_DAMP => format!("{} Hz", (9000.0 * (800.0f64 / 9000.0).powf(fx.reverb_damp)).round()),
        P_FILT_CUT => format!("{} Hz", filt_cutoff(fx.filt_type, fx.filt_cut).round()),
        P_FILT_RATE => format_hz(log_hz(fx.filt_rate, 0.03, 8.0)),
        P_BEND => format!("{:+.1} st", (fx.bend - 0.5) * 2.0 * (1.0 + 11.0 * fx.bend_range)),
        P_BEND_RANGE => format!("±{:.0} st", 1.0 + 11.0 * fx.bend_range),
        P_YURI => pct(fx.yuri),
        P_YURI_RATE => format_hz(3.0 + 5.0 * fx.yuri_rate),
        P_TUNE => format!("A {:.1}", 440.0 * ((fx.tune - 0.5) * 2.0 / 12.0).exp2()),
        P_GLIDE => ms(0.012 + 0.248 * fx.glide),
        P_ENV_A => ms(amp.a),
        P_ENV_D => ms(amp.d),
        P_ENV_S => pct(amp.s),
        P_ENV_R => ms(amp.r),
        P_FENV_A => ms(filt.a),
        P_FENV_D => ms(filt.d),
        P_FENV_S => pct(filt.s),
        P_FENV_R => ms(filt.r),
        id if CONTINUOUS.iter().any(|p| p.1 == id) => pct(get_param(c, id)),
        _ => String::new(),
    }
}

/// The current patch as text.
pub fn preset_text(c: &Controller, name: &str) -> String {
    crate::shaku::preset::to_text(name, &[CONTINUOUS, SWITCHES, CHOICES], |id| get_param(c, id))
}

/// Load a patch (a file's text); returns its name. Gestures (meri, iki, bend) are left alone.
pub fn load_preset(c: &mut Controller, text: &str) -> Result<String, String> {
    let (name, values) = crate::shaku::preset::parse(text, &[CONTINUOUS, SWITCHES, CHOICES])?;
    for (id, v) in values {
        set_param(c, id, v);
    }
    c.sync_engine();
    Ok(name)
}

/// Number of factory patches, and their names.
pub fn factory_names() -> Vec<&'static str> {
    crate::shaku::preset::FACTORY.iter().map(|f| f.0).collect()
}

/// Load factory patch `i` on top of the defaults.
pub fn load_factory(c: &mut Controller, i: usize) -> Result<String, String> {
    let base = crate::shaku::preset::factory_text(i, &[CONTINUOUS, SWITCHES, CHOICES]).ok_or("no such patch")?;
    let d = Controller::detached();
    for (name, id) in CONTINUOUS.iter().chain(SWITCHES).chain(CHOICES) {
        if matches!(*name, "P_MERI" | "P_IKI" | "P_BEND" | "P_FOLI_ON") {
            continue;
        }
        set_param(c, *id, get_param(&d, *id));
    }
    load_preset(c, &base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let mut all: Vec<i32> = [CONTINUOUS, SWITCHES, CHOICES].iter().flat_map(|g| g.iter().map(|p| p.1)).collect();
        let n = all.len();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), n);
    }

    #[test]
    fn set_get_roundtrip() {
        let mut c = Controller::new(
            crate::audio::start(&crate::audio::AudioOptions { silent: true, ..Default::default() }),
            VoiceMode::Sampler,
            || {},
        );
        for (_, id) in CONTINUOUS {
            set_param(&mut c, *id, 0.37);
            assert!((get_param(&c, *id) - 0.37).abs() < 1e-9, "param {id}");
            assert!(!param_hint(&c, *id).is_empty() || *id == P_GAIN);
        }
        for (_, id) in CHOICES {
            let n = choice_labels(*id).len();
            set_param(&mut c, *id, (n - 1) as f64);
            assert_eq!(get_param(&c, *id), (n - 1) as f64, "choice {id}");
        }
        for (_, id) in SWITCHES {
            set_param(&mut c, *id, 1.0);
            assert_eq!(get_param(&c, *id), 1.0, "switch {id}");
        }
    }

    #[test]
    fn preset_roundtrip_and_factory() {
        let mut c = Controller::detached();
        set_param(&mut c, P_YURI, 0.77);
        set_param(&mut c, P_REVERB, 0.11);
        let text = preset_text(&c, "t");
        let mut d = Controller::detached();
        assert_eq!(load_preset(&mut d, &text).unwrap(), "t");
        assert!((get_param(&d, P_YURI) - 0.77).abs() < 1e-4);
        assert!((get_param(&d, P_REVERB) - 0.11).abs() < 1e-4);
        for i in 0..factory_names().len() {
            load_factory(&mut d, i).unwrap();
        }
        assert!(load_factory(&mut d, 99).is_err());
    }
}
