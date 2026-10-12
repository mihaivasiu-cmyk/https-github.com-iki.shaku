//! Writes the generated front-end constants: kotlin/Ids.kt (parameter / gesture ids),
//! kotlin/Palette.kt (design tokens), and the iOS app's Ids.swift and Theme.swift.
fn main() {
    let kt = concat!(env!("CARGO_MANIFEST_DIR"), "/kotlin");
    let ios = concat!(env!("CARGO_MANIFEST_DIR"), "/../../ios/Iki/Generated");
    std::fs::create_dir_all(ios).expect("create ios/Iki/Generated");
    let files = [
        (format!("{kt}/Ids.kt"), iki::kotlin_ids_source()),
        (format!("{kt}/Palette.kt"), iki_core::shaku::design::kotlin_palette_source()),
        (format!("{ios}/Ids.swift"), iki::swift_ids_source()),
        (format!("{ios}/Theme.swift"), iki_core::shaku::design::swift_theme_source()),
    ];
    for (path, text) in files {
        std::fs::write(&path, text).unwrap_or_else(|e| panic!("write {path}: {e}"));
        println!("wrote {path}");
    }
}
