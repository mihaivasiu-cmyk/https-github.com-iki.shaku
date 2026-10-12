//! Numeric ids shared with Kotlin. The parameter ids come from the kernel
//! ([`iki_core::params`]); gestures, looper ops, texts and the packed per-frame
//! state are Android's own. `kotlin/Ids.kt` is generated from both
//! (`cargo run -p iki-android --example gen_ids`) and a test fails if the
//! checked-in copy drifts.

use iki_core::ids;
pub use iki_core::params::*;

// Gestures (`nativeGesture`).
ids!(GESTURES: G_HIT_START = 0, G_SLIDE_START = 1, G_SLIDE_MOVE = 2, G_SLIDE_END = 3, G_TOGGLE_GATE = 4,
     G_MURAIKI = 5, G_RELEASE_ALL = 6, G_FOLI_STEP = 7, G_PHRASE = 8);

// Looper ops (`nativeLoop`).
ids!(LOOP_OPS: L_PUNCH = 0, L_PLAY = 1, L_STOP = 2, L_STOP_ALL = 3, L_UNDO = 4, L_FREEZE = 5, L_LEVEL = 6,
     L_FX = 7, L_SELECT = 8, L_TRIM_IN = 9, L_TRIM_OUT = 10);

// Strings (`nativeText`) and curves (`nativeCurve`).
ids!(TEXTS: TEXT_KANA = 0, TEXT_NAME = 1, TEXT_PITCH = 2, TEXT_BACKEND = 3, TEXT_PROBLEM = 4, TEXT_STATUS = 5,
     TEXT_HOLES = 6, TEXT_PLAY = 7, TEXT_ENGINE = 8, TEXT_CLOCK = 9, TEXT_DELAY = 10, TEXT_LFO_HZ = 11,
     TEXT_BPM = 12, TEXT_CLIP = 13);
ids!(CURVES: CURVE_AMP_ENV = 0, CURVE_FILT_ENV = 1, CURVE_FILTER = 2);

// Packed state (`nativeState`).
ids!(STATE:
    STATE_BLOWING = 0, STATE_LATCHED = 1, STATE_NOTE = 2, STATE_MERI_LANE = 3, STATE_KARI_NOTE = 4,
    STATE_MERI_NOTE = 5, STATE_OTSU = 6, STATE_HZ = 7, STATE_BREATH = 8, STATE_PEAK_L = 9, STATE_PEAK_R = 10,
    STATE_COMP_GR = 11, STATE_CUTOFF = 12, STATE_LATENCY_MS = 13, STATE_LOADED = 14, STATE_ORN_LANE = 15,
    STATE_ORN_INDEX = 16, STATE_ORN_KORO = 17, STATE_MURAIKI = 18, STATE_LOOP_TRACK = 19, STATE_BPM = 20,
    STATE_BAR_PHASE = 21, STATE_CYCLE_BARS = 22, STATE_PAD_ARMED = 23, STATE_FOLI_ON = 24, STATE_FOLI_STEP = 25,
    STATE_FOLI_MASK = 26, STATE_BORE = 27, STATE_PROBLEM = 28, STATE_PHASER_ROUTED = 29, STATE_CLOCK_LATE = 30,
    STATE_CLIP = 31,
    // per track: mode(0 idle,1 rec,2 play,3 dub,4 stop), progress, length s, level, pad, pending, fx, canUndo,
    // trim in, trim out
    STATE_TRACKS = 32, TRACK_STRIDE = 10, STATE_LEN_I = 80,
);
pub const STATE_LEN: usize = STATE_LEN_I as usize;

/// Source of `kotlin/Ids.kt`.
pub fn kotlin_ids_source() -> String {
    let mut s = String::from(
        "// GENERATED from crates/iki-android/src/params.rs — do not edit.\n\
         // Regenerate: cargo run -p iki-android --example gen_ids\n\
         @file:Suppress(\"unused\")\n\npackage org.iki.shakuhachi\n",
    );
    for (title, group) in [
        ("Gestures", GESTURES),
        ("Looper ops", LOOP_OPS),
        ("Texts", TEXTS),
        ("Curves", CURVES),
        ("Continuous parameters (0…1)", CONTINUOUS),
        ("Switches (0/1)", SWITCHES),
        ("Choices (index)", CHOICES),
        ("Packed state indices", STATE),
    ] {
        s.push_str(&format!("\n// {title}\n"));
        for (name, v) in group {
            let name = if *name == "STATE_LEN_I" { "STATE_LEN" } else { name };
            s.push_str(&format!("const val {name} = {v}\n"));
        }
    }
    s.push_str("\n// Choice labels\n");
    for (name, id) in CHOICES {
        let labels: Vec<String> = choice_labels(*id).iter().map(|l| format!("\"{l}\"")).collect();
        s.push_str(&format!("val {name}_LABELS = arrayOf({})\n", labels.join(", ")));
    }
    s
}

/// Source of `ios/Iki/Generated/Ids.swift`.
pub fn swift_ids_source() -> String {
    let mut s = String::from(
        "// GENERATED from crates/iki-android/src/params.rs — do not edit.\n\
         // Regenerate: cargo run -p iki-android --example gen_ids\n\
         // Ids are Int32 to match the C ABI; STATE_* are Int (array indices).\n",
    );
    for (title, group, int) in [
        ("Gestures", GESTURES, false),
        ("Looper ops", LOOP_OPS, false),
        ("Texts", TEXTS, false),
        ("Curves", CURVES, false),
        ("Continuous parameters (0…1)", CONTINUOUS, false),
        ("Switches (0/1)", SWITCHES, false),
        ("Choices (index)", CHOICES, false),
        ("Packed state indices", STATE, true),
    ] {
        s.push_str(&format!("\n// {title}\n"));
        for (name, v) in group {
            let name = if *name == "STATE_LEN_I" { "STATE_LEN" } else { name };
            if int {
                s.push_str(&format!("let {name}: Int = {v}\n"));
            } else {
                s.push_str(&format!("let {name}: Int32 = {v}\n"));
            }
        }
    }
    s.push_str("\n// Choice labels\n");
    for (name, id) in CHOICES {
        let labels: Vec<String> = choice_labels(*id).iter().map(|l| format!("\"{l}\"")).collect();
        s.push_str(&format!("let {name}_LABELS: [String] = [{}]\n", labels.join(", ")));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kotlin_ids_file_is_current() {
        let on_disk = include_str!("../kotlin/Ids.kt");
        assert_eq!(on_disk, kotlin_ids_source(), "run: cargo run -p iki-android --example gen_ids");
    }

    #[test]
    fn swift_ids_file_is_current() {
        let on_disk = include_str!("../../../ios/Iki/Generated/Ids.swift");
        assert_eq!(on_disk, swift_ids_source(), "run: cargo run -p iki-android --example gen_ids");
    }
}
