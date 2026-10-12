//! Keyboard shortcuts (US layout positions, like the web `KeyboardEvent.code`),
//! shared by every egui frontend.

use egui::Key;
use iki_core::controller;
use iki_core::shaku::fingering::Voicing;

/// Physical key → instrument shortcut (US layout positions, like `KeyboardEvent.code`).
pub fn shortcut(k: Key) -> Option<controller::Key> {
    use controller::Key as K;
    Some(match k {
        Key::A => K::Hit(0, Voicing::Kari),
        Key::S => K::Hit(1, Voicing::Kari),
        Key::D => K::Hit(2, Voicing::Kari),
        Key::F => K::Hit(3, Voicing::Kari),
        Key::G => K::Hit(4, Voicing::Kari),
        Key::H => K::Hit(5, Voicing::Kari),
        Key::Z => K::Hit(0, Voicing::Meri),
        Key::X => K::Hit(1, Voicing::Meri),
        Key::C => K::Hit(2, Voicing::Meri),
        Key::V => K::Hit(3, Voicing::Meri),
        Key::B => K::Hit(4, Voicing::Meri),
        Key::N => K::Hit(5, Voicing::Meri),
        Key::Space => K::Space,
        Key::O => K::Octave,
        Key::M => K::Muraiki,
        Key::Num1 => K::Track(0),
        Key::Num2 => K::Track(1),
        Key::Num3 => K::Track(2),
        Key::Num4 => K::Track(3),
        Key::R => K::Punch,
        Key::T => K::Stop,
        Key::U => K::Undo,
        Key::ArrowUp | Key::ArrowRight => K::NoteUp,
        Key::ArrowDown | Key::ArrowLeft => K::NoteDown,
        _ => return None,
    })
}
