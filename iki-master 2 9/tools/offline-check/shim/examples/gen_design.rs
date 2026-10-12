//! Writes the generated palette files from the real design.rs (offline stand-in for
//! `cargo run -p iki-android --example gen_ids`, which needs the whole workspace).
fn main() {
    let root = std::env::args().nth(1).expect("repo root");
    std::fs::write(
        format!("{root}/crates/iki-android/kotlin/Palette.kt"),
        iki_core::shaku::design::kotlin_palette_source(),
    )
    .unwrap();
    std::fs::write(format!("{root}/ios/Iki/Generated/Theme.swift"), iki_core::shaku::design::swift_theme_source())
        .unwrap();
}
