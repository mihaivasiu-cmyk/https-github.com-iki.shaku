// GENERATED from crates/iki-core/src/shaku/design.rs — do not edit.
// Regenerate: cargo run -p iki-android --example gen_ids
import SwiftUI

extension Color {
    init(hex: UInt32) {
        self.init(.sRGB, red: Double((hex >> 16) & 0xff) / 255, green: Double((hex >> 8) & 0xff) / 255, blue: Double(hex & 0xff) / 255, opacity: 1)
    }
}

/// Chassis colours — "Breath" (day) and "Temple" (night); the same tokens as the desktop, plugin and Android themes.
struct Palette {
    let night: Bool
    let bg: Color
    let surface: Color
    let surface2: Color
    let fg: Color
    let muted: Color
    let subtle: Color
    let border: Color
    let moss: Color
    let mossFg: Color
    let iris: Color
    let irisFg: Color
    let danger: Color
    let hole: Color
    let lcd: Color
    let metal: Color

    static let day = Palette(
        night: false,
        bg: Color(hex: 0xfbf8f0),
        surface: Color(hex: 0xffffff),
        surface2: Color(hex: 0xf4efe6),
        fg: Color(hex: 0x2a2214),
        muted: Color(hex: 0x6e5a38),
        subtle: Color(hex: 0x8a7450),
        border: Color(hex: 0xe4d8c0),
        moss: Color(hex: 0xe0b020),
        mossFg: Color(hex: 0x1a1406),
        iris: Color(hex: 0x8b6bb5),
        irisFg: Color(hex: 0xfaf7ff),
        danger: Color(hex: 0xc45c4a),
        hole: Color(hex: 0x3a3020),
        lcd: Color(hex: 0xfffaf2),
        metal: Color(hex: 0xefe8dc)
    )

    static let night = Palette(
        night: true,
        bg: Color(hex: 0x16120c),
        surface: Color(hex: 0x1c1812),
        surface2: Color(hex: 0x241e16),
        fg: Color(hex: 0xf3ead8),
        muted: Color(hex: 0xc4b090),
        subtle: Color(hex: 0x9a8868),
        border: Color(hex: 0x3a3224),
        moss: Color(hex: 0xe0b020),
        mossFg: Color(hex: 0x1a1406),
        iris: Color(hex: 0x8b6bb5),
        irisFg: Color(hex: 0xfaf7ff),
        danger: Color(hex: 0xc45c4a),
        hole: Color(hex: 0x0e0c08),
        lcd: Color(hex: 0x14110c),
        metal: Color(hex: 0x2a2418)
    )

    func accent(_ iris: Bool) -> Color { iris ? self.iris : moss }
}

/// Motion timings in seconds, from the shared design tokens.
enum Motion {
    static let hover = 0.08
    static let sw = 0.12
    static let knob = 0.09
    static let hole = 0.05
    static let bubble = 0.10
}
