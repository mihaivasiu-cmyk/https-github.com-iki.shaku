//! Design tokens shared by every front end: the "Breath" (day) and "Temple"
//! (night) palettes, motion timings and easing. The egui theme and the generated
//! Android `Palette.kt` both come from here, so the platforms cannot drift.

/// One palette, as 0xRRGGBB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tokens {
    pub bg: u32,
    pub surface: u32,
    pub surface2: u32,
    pub fg: u32,
    pub muted: u32,
    pub subtle: u32,
    pub border: u32,
    pub moss: u32,
    pub moss_fg: u32,
    pub iris: u32,
    pub iris_fg: u32,
    pub danger: u32,
    pub hole: u32,
    pub lcd: u32,
    pub metal: u32,
}

pub const DAY: Tokens = Tokens {
    bg: 0xfbf8f0,
    surface: 0xffffff,
    surface2: 0xf4efe6,
    fg: 0x2a2214,
    muted: 0x6e5a38,
    subtle: 0x8a7450,
    border: 0xe4d8c0,
    moss: 0xe0b020,
    moss_fg: 0x1a1406,
    iris: 0x8b6bb5,
    iris_fg: 0xfaf7ff,
    danger: 0xc45c4a,
    hole: 0x3a3020,
    lcd: 0xfffaf2,
    metal: 0xefe8dc,
};

pub const NIGHT: Tokens = Tokens {
    bg: 0x16120c,
    surface: 0x1c1812,
    surface2: 0x241e16,
    fg: 0xf3ead8,
    muted: 0xc4b090,
    subtle: 0x9a8868,
    border: 0x3a3224,
    hole: 0x0e0c08,
    lcd: 0x14110c,
    metal: 0x2a2418,
    ..DAY
};

/// Motion timings, seconds. Short and purposeful: state changes should feel physical.
pub mod motion {
    /// Hover / press feedback on buttons and switches.
    pub const HOVER: f32 = 0.08;
    /// A switch or segmented selection moving to its new state.
    pub const SWITCH: f32 = 0.12;
    /// A knob's lit position catching up with the value.
    pub const KNOB: f32 = 0.09;
    /// A finger hole closing or opening on the flute.
    pub const HOLE: f32 = 0.05;
    /// The value bubble fading in and out while a knob is held.
    pub const BUBBLE: f32 = 0.10;
}

/// Fine adjust: dragging with Shift (or Ctrl/Cmd) held moves a knob this much slower.
pub const FINE_DRAG: f64 = 0.125;

/// Ease-out cubic, 0…1 → 0…1.
pub fn ease_out(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

/// Ease-in-out (smoothstep), 0…1 → 0…1.
pub fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Frame-rate independent exponential approach: where `from` ends up after `dt`
/// seconds heading for `to` with the given time constant.
pub fn approach(from: f32, to: f32, dt: f32, tau: f32) -> f32 {
    if tau <= 0.0 {
        return to;
    }
    to + (from - to) * (-dt.max(0.0) / tau).exp()
}

/// `0xRRGGBB` → Kotlin ARGB literal, e.g. `0xffe0b020.toInt()`.
fn kt(v: u32) -> String {
    format!("0xff{v:06x}.toInt()")
}

/// Source of `crates/iki-android/kotlin/Palette.kt`.
pub fn kotlin_palette_source() -> String {
    let mut s = String::from(
        "// GENERATED from crates/iki-core/src/shaku/design.rs — do not edit.\n\
         // Regenerate: cargo run -p iki-android --example gen_ids\n\
         package org.iki.shakuhachi\n\nimport kotlin.math.roundToInt\n\n\
         /** Chassis colours — \"Breath\" (day) and \"Temple\" (night); the same tokens as the desktop and plugin themes. */\n\
         class Palette private constructor(val night: Boolean) {\n",
    );
    let fields: [(&str, fn(&Tokens) -> u32); 15] = [
        ("moss", |t| t.moss),
        ("mossFg", |t| t.moss_fg),
        ("iris", |t| t.iris),
        ("irisFg", |t| t.iris_fg),
        ("danger", |t| t.danger),
        ("bg", |t| t.bg),
        ("surface", |t| t.surface),
        ("surface2", |t| t.surface2),
        ("fg", |t| t.fg),
        ("muted", |t| t.muted),
        ("subtle", |t| t.subtle),
        ("border", |t| t.border),
        ("hole", |t| t.hole),
        ("lcd", |t| t.lcd),
        ("metal", |t| t.metal),
    ];
    for (name, f) in fields {
        let (d, n) = (f(&DAY), f(&NIGHT));
        if d == n {
            s.push_str(&format!("    val {name} = {}\n", kt(d)));
        } else {
            s.push_str(&format!("    val {name} = if (night) {} else {}\n", kt(n), kt(d)));
        }
    }
    s.push_str(
        "\n    fun accent(iris: Boolean) = if (iris) this.iris else moss\n\n\
         \x20   companion object {\n\
         \x20       val DAY = Palette(false)\n\
         \x20       val NIGHT = Palette(true)\n\n\
         \x20       /** Linear blend of two ARGB colours, like the desktop `blend`. */\n\
         \x20       fun blend(a: Int, b: Int, t: Float): Int {\n\
         \x20           val k = t.coerceIn(0f, 1f)\n\
         \x20           fun ch(shift: Int): Int {\n\
         \x20               val x = (a ushr shift) and 0xff\n\
         \x20               val y = (b ushr shift) and 0xff\n\
         \x20               return (x + (y - x) * k).roundToInt() shl shift\n\
         \x20           }\n\
         \x20           return ch(24) or ch(16) or ch(8) or ch(0)\n\
         \x20       }\n\n\
         \x20       /** Scale the alpha (egui `gamma_multiply` on an opaque colour). */\n\
         \x20       fun alpha(c: Int, a: Float): Int = ((((c ushr 24) and 0xff) * a).roundToInt() shl 24) or (c and 0xffffff)\n\n\
         \x20       /** Motion timings in milliseconds, from the shared design tokens. */\n",
    );
    s.push_str(&format!(
        "        const val HOVER_MS = {}\n        const val SWITCH_MS = {}\n        const val KNOB_MS = {}\n        const val HOLE_MS = {}\n    }}\n}}\n",
        (motion::HOVER * 1000.0).round() as i32,
        (motion::SWITCH * 1000.0).round() as i32,
        (motion::KNOB * 1000.0).round() as i32,
        (motion::HOLE * 1000.0).round() as i32,
    ));
    s
}

/// Source of `ios/Iki/Generated/Theme.swift`.
pub fn swift_theme_source() -> String {
    let mut s = String::from(
        "// GENERATED from crates/iki-core/src/shaku/design.rs — do not edit.\n\
         // Regenerate: cargo run -p iki-android --example gen_ids\n\
         import SwiftUI\n\n\
         extension Color {\n\
         \x20   init(hex: UInt32) {\n\
         \x20       self.init(.sRGB, red: Double((hex >> 16) & 0xff) / 255, green: Double((hex >> 8) & 0xff) / 255, blue: Double(hex & 0xff) / 255, opacity: 1)\n\
         \x20   }\n\
         }\n\n\
         /// Chassis colours — \"Breath\" (day) and \"Temple\" (night); the same tokens as the desktop, plugin and Android themes.\n\
         struct Palette {\n\
         \x20   let night: Bool\n",
    );
    let fields: [(&str, fn(&Tokens) -> u32); 15] = [
        ("bg", |t| t.bg),
        ("surface", |t| t.surface),
        ("surface2", |t| t.surface2),
        ("fg", |t| t.fg),
        ("muted", |t| t.muted),
        ("subtle", |t| t.subtle),
        ("border", |t| t.border),
        ("moss", |t| t.moss),
        ("mossFg", |t| t.moss_fg),
        ("iris", |t| t.iris),
        ("irisFg", |t| t.iris_fg),
        ("danger", |t| t.danger),
        ("hole", |t| t.hole),
        ("lcd", |t| t.lcd),
        ("metal", |t| t.metal),
    ];
    for (name, _) in fields {
        s.push_str(&format!("    let {name}: Color\n"));
    }
    for (var, tok) in [("day", &DAY), ("night", &NIGHT)] {
        s.push_str(&format!("\n    static let {var} = Palette(\n        night: {},\n", var == "night"));
        let parts: Vec<String> =
            fields.iter().map(|(name, f)| format!("        {name}: Color(hex: 0x{:06x})", f(tok))).collect();
        s.push_str(&parts.join(",\n"));
        s.push_str("\n    )\n");
    }
    s.push_str("\n    func accent(_ iris: Bool) -> Color { iris ? self.iris : moss }\n}\n\n");
    s.push_str(&format!(
        "/// Motion timings in seconds, from the shared design tokens.\nenum Motion {{\n    static let hover = {:.2}\n    static let sw = {:.2}\n    static let knob = {:.2}\n    static let hole = {:.2}\n    static let bubble = {:.2}\n}}\n",
        motion::HOVER,
        motion::SWITCH,
        motion::KNOB,
        motion::HOLE,
        motion::BUBBLE
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_hits_its_endpoints_and_is_monotonic() {
        for f in [ease_out, ease_in_out] {
            assert_eq!((f(0.0), f(1.0)), (0.0, 1.0));
            let mut last = 0.0;
            for i in 0..=20 {
                let v = f(i as f32 / 20.0);
                assert!(v >= last);
                last = v;
            }
        }
        assert!(ease_out(0.5) > 0.5);
    }

    #[test]
    fn approach_converges_without_overshoot_at_any_frame_rate() {
        for dt in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 144.0] {
            let (mut v, mut t) = (0.0f32, 0.0f32);
            while t < 0.09 * 5.0 {
                v = approach(v, 1.0, dt, 0.09);
                assert!(v <= 1.0);
                t += dt;
            }
            assert!(v > 0.99, "{v}");
        }
        assert_eq!(approach(0.3, 0.9, 0.1, 0.0), 0.9);
    }

    #[test]
    fn motion_stays_short() {
        for t in [motion::HOVER, motion::SWITCH, motion::KNOB, motion::HOLE, motion::BUBBLE] {
            assert!((0.04..=0.15).contains(&t), "{t}");
        }
    }

    #[test]
    fn android_palette_is_generated_from_the_tokens() {
        let file = include_str!("../../../iki-android/kotlin/Palette.kt");
        assert_eq!(file, kotlin_palette_source(), "run: cargo run -p iki-android --example gen_ids");
    }

    #[test]
    fn ios_theme_is_generated_from_the_tokens() {
        let file = include_str!("../../../../ios/Iki/Generated/Theme.swift");
        assert_eq!(file, swift_theme_source(), "run: cargo run -p iki-android --example gen_ids");
    }
}
