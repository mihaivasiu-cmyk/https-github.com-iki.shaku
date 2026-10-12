//! The plugin's host parameters, built from the kernel's parameter table
//! (`iki_core::params`): every knob, switch and choice of the panel except the
//! looper's and the tempo knob (the host's tempo is used). Ids are stable
//! strings, so Live's automation and saved sets keep working across versions.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;

use iki_core::controller::Controller;
use iki_core::params::*;

/// (id, stable string id, display name, group)
const TABLE: &[(i32, &str, &str, &str)] = &[
    (P_VOICE, "voice", "Voice", "Voice"),
    (P_MERI, "meri", "Meri", "Voice"),
    (P_LEAN, "lean", "Lean", "Voice"),
    (P_IKI, "iki", "Iki (breath)", "Voice"),
    (P_OTSU, "otsu", "Otsu (low octave)", "Voice"),
    (P_GAIN, "gain", "Gain", "Voice"),
    (P_FLUTE_LEVEL, "flute_level", "Flute level", "Voice"),
    (P_BORE_LEVEL, "bore_level", "Bore level", "Voice"),
    (P_BEND, "bend", "Pitch bend", "Expression"),
    (P_BEND_RANGE, "bend_range", "Bend range", "Expression"),
    (P_YURI, "yuri", "Yuri depth", "Expression"),
    (P_YURI_RATE, "yuri_rate", "Yuri rate", "Expression"),
    (P_GLIDE, "glide", "Glide", "Expression"),
    (P_TUNE, "tune", "Tuning (A ref)", "Voice"),
    (P_ENV_A, "env_a", "Amp attack", "Env"),
    (P_ENV_D, "env_d", "Amp decay", "Env"),
    (P_ENV_S, "env_s", "Amp sustain", "Env"),
    (P_ENV_R, "env_r", "Amp release", "Env"),
    (P_ENV_SHAPE, "env_shape", "Amp shape", "Env"),
    (P_ENV_LOOP, "env_loop", "Amp loop", "Env"),
    (P_FENV_A, "fenv_a", "Filter attack", "Env"),
    (P_FENV_D, "fenv_d", "Filter decay", "Env"),
    (P_FENV_S, "fenv_s", "Filter sustain", "Env"),
    (P_FENV_R, "fenv_r", "Filter release", "Env"),
    (P_FENV_SHAPE, "fenv_shape", "Filter shape", "Env"),
    (P_FENV_LOOP, "fenv_loop", "Filter loop", "Env"),
    (P_FILT_TYPE, "filt_type", "Filter type", "Filter"),
    (P_FILT_AUTO, "filt_auto", "Filter motion source", "Filter"),
    (P_FILT_CUT, "filt_cut", "Cutoff", "Filter"),
    (P_FILT_RES, "filt_res", "Resonance", "Filter"),
    (P_FILT_MOTION, "filt_motion", "Motion", "Filter"),
    (P_FILT_RATE, "filt_rate", "Sweep rate", "Filter"),
    (P_LFO_SYNC, "lfo_sync", "LFO sync", "LFO"),
    (P_LFO_DIV, "lfo_div", "LFO division", "LFO"),
    (P_LFO_RATE, "lfo_rate", "LFO rate", "LFO"),
    (P_LFO_WAVE, "lfo_wave", "LFO wave", "LFO"),
    (P_LFO_PITCH, "lfo_pitch", "LFO → pitch", "LFO"),
    (P_LFO_PITCH_AMT, "lfo_pitch_amt", "LFO pitch depth", "LFO"),
    (P_LFO_FILT, "lfo_filt", "LFO → filter", "LFO"),
    (P_LFO_FILT_AMT, "lfo_filt_amt", "LFO filter depth", "LFO"),
    (P_LFO_AMP, "lfo_amp", "LFO → amp", "LFO"),
    (P_LFO_AMP_AMT, "lfo_amp_amt", "LFO amp depth", "LFO"),
    (P_DELAY_PING, "delay_ping", "Delay ping-pong", "Delay"),
    (P_DELAY_SYNC, "delay_sync", "Delay sync", "Delay"),
    (P_DELAY_DIV, "delay_div", "Delay division", "Delay"),
    (P_DELAY_TIME, "delay_time", "Delay time", "Delay"),
    (P_DELAY_FEEDBACK, "delay_feedback", "Delay feedback", "Delay"),
    (P_DELAY_MIX, "delay_mix", "Delay mix", "Delay"),
    (P_DELAY_TONE, "delay_tone", "Delay tone", "Delay"),
    (P_DELAY_LOWCUT, "delay_lowcut", "Delay low cut", "Delay"),
    (P_REVERB, "reverb", "Reverb mix", "Space"),
    (P_REVERB_DECAY, "reverb_decay", "Reverb decay", "Space"),
    (P_REVERB_SIZE, "reverb_size", "Reverb size", "Space"),
    (P_REVERB_PRE, "reverb_pre", "Reverb pre-delay", "Space"),
    (P_REVERB_DAMP, "reverb_damp", "Reverb damping", "Space"),
    (P_REVERB_TYPE, "reverb_type", "Reverb room", "Space"),
    (P_WIDTH, "width", "Width", "Space"),
    (P_TAPE, "tape", "Tape", "Color"),
    (P_PHASER, "phaser", "Phaser", "Color"),
    (P_PHASER_DEPTH, "phaser_depth", "Phaser depth", "Color"),
    (P_FOLI_ON, "foli_on", "Foli drums", "Foli"),
    (P_FOLI_FEEL, "foli_feel", "Foli feel", "Foli"),
    (P_FOLI_KIT, "foli_kit", "Foli kit", "Foli"),
    (P_FOLI_LEVEL, "foli_level", "Foli level", "Foli"),
];

/// The readout under each knob, from the kernel (a detached controller holds
/// the value being formatted; the cutoff reads as for the default low-pass).
fn hint(id: i32, v: f32) -> String {
    static FMT: OnceLock<Mutex<Controller>> = OnceLock::new();
    let m = FMT.get_or_init(|| {
        // The time / rate knobs describe the free-running values (the synced
        // ones follow the division and the host tempo, not the knob).
        let mut c = Controller::detached();
        c.fx.delay_sync = false;
        c.fx.lfo_sync = false;
        Mutex::new(c)
    });
    let Ok(mut c) = m.lock() else { return format!("{v:.2}") };
    let old = get_param(&c, id);
    set_param(&mut c, id, v as f64);
    let s = param_hint(&c, id);
    set_param(&mut c, id, old);
    // The panel labels meri / iki "Flatten" / "Breath"; the host needs a number.
    if s.is_empty() || readout_number(&s).is_none() { format!("{:.0}", v * 100.0) } else { s }
}

/// The number in a readout, in base units ("64 ms" → 0.064, "1.20 Hz" → 1.2,
/// "±58 c" → 58, "lean 55" → 55).
fn readout_number(s: &str) -> Option<f64> {
    let start = s.find(|c: char| c.is_ascii_digit() || c == '.' || c == '-')?;
    let rest = &s[start..];
    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).unwrap_or(rest.len());
    let v: f64 = rest[..end].parse().ok()?;
    let unit = rest[end..].trim_start();
    Some(if unit.starts_with("ms") { v / 1000.0 } else { v })
}

/// Invert a readout (typed into the host's parameter field): every knob's
/// readout grows with the knob, so bisect for the value that shows it.
fn value_for(id: i32, s: &str) -> Option<f32> {
    let target = readout_number(s)?;
    let at = |v: f32| readout_number(&hint(id, v));
    let (lo_v, hi_v) = (at(0.0)?, at(1.0)?);
    if lo_v >= hi_v || lo_v.is_nan() || hi_v.is_nan() {
        return None;
    }
    if target <= lo_v {
        return Some(0.0);
    }
    if target >= hi_v {
        return Some(1.0);
    }
    let (mut lo, mut hi) = (0f32, 1f32);
    for _ in 0..32 {
        let mid = 0.5 * (lo + hi);
        if at(mid)? < target { lo = mid } else { hi = mid }
    }
    Some(hi)
}

pub enum Kind {
    Float(FloatParam),
    Bool(BoolParam),
    Int(IntParam),
}

pub struct HostParam {
    pub id: i32,
    pub key: &'static str,
    pub group: &'static str,
    pub kind: Kind,
}

impl HostParam {
    /// The value in the kernel's units (0…1, 0/1, or a choice index).
    pub fn value(&self) -> f64 {
        match &self.kind {
            Kind::Float(p) => p.value() as f64,
            Kind::Bool(p) => p.value() as u8 as f64,
            Kind::Int(p) => p.value() as f64,
        }
    }

    pub fn ptr(&self) -> ParamPtr {
        match &self.kind {
            Kind::Float(p) => p.as_ptr(),
            Kind::Bool(p) => p.as_ptr(),
            Kind::Int(p) => p.as_ptr(),
        }
    }
}

pub struct IkiParams {
    /// Never resized after construction: the host keeps pointers into it.
    pub list: Vec<HostParam>,
    /// Editor window size.
    pub editor_state: Arc<EguiState>,
    /// Breath (day) or Temple (night) theme.
    pub night: AtomicBool,
    /// Foli step pattern (bit per step), restored with the set.
    pub foli_mask: AtomicU32,
}

impl Default for IkiParams {
    fn default() -> Self {
        let defaults = Controller::detached();
        let list = TABLE
            .iter()
            .map(|&(id, key, name, group)| {
                let d = get_param(&defaults, id);
                let kind = if SWITCHES.iter().any(|s| s.1 == id) {
                    Kind::Bool(
                        BoolParam::new(name, d > 0.5)
                            .with_value_to_string(Arc::new(|on| if on { "On".into() } else { "Off".into() })),
                    )
                } else if CHOICES.iter().any(|s| s.1 == id) {
                    let labels = choice_labels(id);
                    let n = labels.len() as i32;
                    let to_s = labels.clone();
                    Kind::Int(
                        IntParam::new(name, d.round() as i32, IntRange::Linear { min: 0, max: n - 1 })
                            .with_value_to_string(Arc::new(move |i| to_s.get(i as usize).unwrap_or(&"").to_string()))
                            .with_string_to_value(Arc::new(move |s| {
                                labels.iter().position(|l| l.eq_ignore_ascii_case(s.trim())).map(|i| i as i32)
                            })),
                    )
                } else {
                    Kind::Float(
                        FloatParam::new(name, d as f32, FloatRange::Linear { min: 0.0, max: 1.0 })
                            .with_value_to_string(Arc::new(move |v| hint(id, v)))
                            .with_string_to_value(Arc::new(move |s| value_for(id, s))),
                    )
                };
                HostParam { id, key, group, kind }
            })
            .collect();
        Self {
            list,
            editor_state: EguiState::from_size(1320, 820),
            night: AtomicBool::new(false),
            foli_mask: AtomicU32::new(u32::MAX),
        }
    }
}

impl IkiParams {
    pub fn get(&self, id: i32) -> Option<&HostParam> {
        self.list.iter().find(|p| p.id == id)
    }
}

unsafe impl Params for IkiParams {
    fn param_map(&self) -> Vec<(String, ParamPtr, String)> {
        self.list.iter().map(|p| (p.key.to_string(), p.ptr(), p.group.to_string())).collect()
    }

    fn serialize_fields(&self) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        if let Ok(s) = serde_json::to_string(&*self.editor_state) {
            m.insert("editor-state".into(), s);
        }
        m.insert("night".into(), self.night.load(Ordering::Relaxed).to_string());
        m.insert("foli-mask".into(), self.foli_mask.load(Ordering::Relaxed).to_string());
        m
    }

    fn deserialize_fields(&self, serialized: &BTreeMap<String, String>) {
        use nih_plug::params::persist::PersistentField;
        if let Some(s) = serialized.get("editor-state")
            && let Ok(st) = serde_json::from_str::<EguiState>(s)
        {
            self.editor_state.set(st);
        }
        if let Some(v) = serialized.get("night").and_then(|s| s.parse().ok()) {
            self.night.store(v, Ordering::Relaxed);
        }
        if let Some(v) = serialized.get("foli-mask").and_then(|s| s.parse().ok()) {
            self.foli_mask.store(v, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_complete_and_unique() {
        let p = IkiParams::default();
        let mut keys: Vec<_> = p.list.iter().map(|p| p.key).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), p.list.len());
        // every panel parameter except the looper-era tempo knob is exposed
        for (name, id) in CONTINUOUS.iter().chain(SWITCHES).chain(CHOICES) {
            assert!(*id == P_TEMPO || p.get(*id).is_some(), "{name} missing");
        }
        // defaults match a fresh instrument
        let c = Controller::detached();
        for q in &p.list {
            assert!((q.value() - get_param(&c, q.id)).abs() < 1e-6, "{} default", q.key);
        }
    }

    #[test]
    fn readouts_match_the_panel() {
        assert_eq!(hint(P_ENV_A, 0.25), {
            let mut c = Controller::detached();
            set_param(&mut c, P_ENV_A, 0.25);
            param_hint(&c, P_ENV_A)
        });
        assert!(hint(P_FILT_CUT, 1.0).ends_with("Hz"));
    }

    /// Host text fields: value → text → value → text is stable (clap-validator's check).
    #[test]
    fn readouts_round_trip() {
        let p = IkiParams::default();
        for q in &p.list {
            let Kind::Float(f) = &q.kind else { continue };
            for v in [0.0f32, 0.012_345_679, 0.25, 0.5, 0.777, 1.0] {
                let s1 = f.normalized_value_to_string(v, true);
                let back = f.string_to_normalized_value(&s1).unwrap_or_else(|| panic!("{} can't parse {s1:?}", q.key));
                let s2 = f.normalized_value_to_string(back, true);
                assert_eq!(s1, s2, "{} at {v}", q.key);
            }
        }
    }
}
