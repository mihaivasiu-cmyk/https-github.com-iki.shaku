//! Chassis colours — the "Breath" (day) and "Temple" (night) themes.

use egui::{self, Color32, FontData, FontDefinitions, FontFamily};
use iki_core::shaku::design::{Tokens, DAY, NIGHT};

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub night: bool,
    pub bg: Color32,
    pub surface: Color32,
    pub surface2: Color32,
    pub fg: Color32,
    pub muted: Color32,
    pub subtle: Color32,
    pub border: Color32,
    pub moss: Color32,
    pub moss_fg: Color32,
    pub iris: Color32,
    pub iris_fg: Color32,
    pub danger: Color32,
    pub hole: Color32,
    pub lcd: Color32,
    pub metal: Color32,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

impl Theme {
    fn from_tokens(night: bool, k: &Tokens) -> Self {
        Self {
            night,
            bg: hex(k.bg),
            surface: hex(k.surface),
            surface2: hex(k.surface2),
            fg: hex(k.fg),
            muted: hex(k.muted),
            subtle: hex(k.subtle),
            border: hex(k.border),
            moss: hex(k.moss),
            moss_fg: hex(k.moss_fg),
            iris: hex(k.iris),
            iris_fg: hex(k.iris_fg),
            danger: hex(k.danger),
            hole: hex(k.hole),
            lcd: hex(k.lcd),
            metal: hex(k.metal),
        }
    }

    pub fn day() -> Self {
        Self::from_tokens(false, &DAY)
    }

    pub fn night() -> Self {
        Self::from_tokens(true, &NIGHT)
    }

    pub fn get(night: bool) -> Self {
        if night {
            Self::night()
        } else {
            Self::day()
        }
    }

    pub fn apply(&self, ctx: &egui::Context) {
        let mut v = if self.night { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = self.bg;
        v.window_fill = self.surface;
        v.extreme_bg_color = self.lcd;
        v.faint_bg_color = self.surface2;
        v.override_text_color = Some(self.fg);
        v.selection.bg_fill = self.iris;
        v.selection.stroke.color = self.iris_fg;
        v.hyperlink_color = self.iris;
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.bg_fill = self.surface2;
            w.weak_bg_fill = self.surface2;
            w.fg_stroke.color = self.fg;
        }
        v.widgets.noninteractive.bg_stroke.color = self.border;
        v.widgets.noninteractive.fg_stroke.color = self.fg;
        v.widgets.inactive.bg_stroke.color = self.border;
        v.widgets.hovered.bg_stroke.color = self.moss;
        v.widgets.active.bg_stroke.color = self.iris;
        ctx.set_visuals(v);
    }
}

pub fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// Default egui fonts plus a Noto Sans JP subset (kana, 息, 風, …) as fallback.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "noto-jp".into(),
        std::sync::Arc::new(FontData::from_static(include_bytes!("../assets/fonts/NotoSansJP-Iki.otf"))),
    );
    for fam in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.entry(fam).or_default().push("noto-jp".into());
    }
    // A calligraphic serif for the pad kana and the Blow button (Noto Serif JP subset, OFL).
    fonts.font_data.insert(
        "noto-serif-jp".into(),
        std::sync::Arc::new(FontData::from_static(include_bytes!("../assets/fonts/NotoSerifJP-Iki.otf"))),
    );
    fonts.families.insert(FontFamily::Name("mincho".into()), vec!["noto-serif-jp".into(), "noto-jp".into()]);
    ctx.set_fonts(fonts);
}
