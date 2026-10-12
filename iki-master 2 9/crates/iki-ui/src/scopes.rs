//! CRT stereo spectrogram (`crt-wave.tsx`), envelope scope (`env-scope.tsx`)
//! and filter response scope (`filt-scope.tsx`).

use egui::{self, Color32, ColorImage, Pos2, Rect, Stroke, TextureHandle, TextureOptions, Vec2};

use crate::theme::{Theme, blend};
use crate::widgets::fill_between;
use iki_core::shaku::env_shape::{EnvShape, EnvTimes, FiltType, env_level, filt_mag_curve};
use iki_core::spectrum::{Analyser, BINS, FFT_SIZE};

const ROWS: usize = 72;
const COLS: usize = 360;
const GAP: usize = 2;

pub struct Spectrogram {
    ring_l: Vec<f32>,
    ring_r: Vec<f32>,
    w: usize,
    an_l: Analyser,
    an_r: Analyser,
    hist_l: Vec<f32>,
    hist_r: Vec<f32>,
    col: usize,
    acc: f64,
    tex: Option<TextureHandle>,
    pub level: f32,
}

impl Default for Spectrogram {
    fn default() -> Self {
        Self {
            ring_l: vec![0.0; FFT_SIZE],
            ring_r: vec![0.0; FFT_SIZE],
            w: 0,
            an_l: Analyser::new(0.72),
            an_r: Analyser::new(0.72),
            hist_l: vec![0.0; COLS * ROWS],
            hist_r: vec![0.0; COLS * ROWS],
            col: 0,
            acc: 0.0,
            tex: None,
            level: 0.0,
        }
    }
}

fn log_bin(t: f32, rate: f32) -> usize {
    let f0 = 55.0;
    let f1 = (rate * 0.48).min(12000.0);
    let f = f0 * (f1 / f0).powf(t);
    ((f / (rate * 0.5)) * BINS as f32).round().clamp(0.0, (BINS - 1) as f32) as usize
}

fn paint_col(src: &[u8], dest: &mut [f32], col: usize, rate: f32) {
    let base = col * ROWS;
    for y in 0..ROWS {
        let t = 1.0 - y as f32 / (ROWS - 1) as f32;
        let a = log_bin((t - 0.012).max(0.0), rate);
        let b = log_bin((t + 0.012).min(1.0), rate);
        let m = src[a..=b.max(a)].iter().copied().max().unwrap_or(0);
        dest[base + y] = m as f32 / 255.0;
    }
}

impl Spectrogram {
    /// Feed the latest output samples from the audio thread.
    pub fn push(&mut self, l: f32, r: f32) {
        self.ring_l[self.w] = l;
        self.ring_r[self.w] = r;
        self.w = (self.w + 1) % FFT_SIZE;
    }

    /// One animation frame: analyse, scroll `dt × 60` columns (≈ one per 60 Hz frame like rAF).
    pub fn frame(&mut self, dt: f64, rate: f32) {
        let w = self.w;
        let l = &self.ring_l;
        let r = &self.ring_r;
        self.an_l.update((0..FFT_SIZE).map(|i| l[(w + i) % FFT_SIZE]));
        self.an_r.update((0..FFT_SIZE).map(|i| r[(w + i) % FFT_SIZE]));
        self.level = l.iter().chain(r.iter()).fold(0f32, |m, v| m.max(v.abs()));
        self.acc += dt * 60.0;
        let n = (self.acc.floor() as usize).min(8);
        self.acc -= self.acc.floor();
        for _ in 0..n {
            paint_col(&self.an_l.bytes, &mut self.hist_l, self.col, rate);
            paint_col(&self.an_r.bytes, &mut self.hist_r, self.col, rate);
            self.col = (self.col + 1) % COLS;
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, t: &Theme, rect: Rect) {
        let h = ROWS * 2 + GAP;
        let mut img = ColorImage::filled([COLS, h], t.lcd);
        for x in 0..COLS {
            let c = (self.col + x) % COLS;
            for y in 0..ROWS {
                let v = self.hist_l[c * ROWS + y];
                img[(x, y)] =
                    Color32::from_rgb((36.0 + v * 214.0) as u8, (26.0 + v * 168.0) as u8, (16.0 + v * 36.0) as u8);
                let v = self.hist_r[c * ROWS + y];
                img[(x, y + ROWS + GAP)] =
                    Color32::from_rgb((46.0 + v * 130.0) as u8, (22.0 + v * 48.0) as u8, (58.0 + v * 168.0) as u8);
            }
        }
        let tex = match &mut self.tex {
            Some(tex) => {
                tex.set(img, TextureOptions::LINEAR);
                tex.clone()
            }
            None => {
                let tex = ui.ctx().load_texture("crt-spec", img, TextureOptions::LINEAR);
                self.tex = Some(tex.clone());
                tex
            }
        };
        let p = ui.painter_at(rect);
        p.image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        let band = rect.height() / 2.0;
        p.text(
            rect.min + Vec2::new(8.0, 4.0),
            egui::Align2::LEFT_TOP,
            "L",
            egui::FontId::proportional(11.0),
            Color32::from_rgba_unmultiplied(240, 196, 58, 216),
        );
        p.text(
            rect.min + Vec2::new(8.0, band + 4.0),
            egui::Align2::LEFT_TOP,
            "R",
            egui::FontId::proportional(11.0),
            Color32::from_rgba_unmultiplied(196, 164, 232, 230),
        );
    }
}

/// `EnvScope`.
pub fn env_scope(p: &egui::Painter, t: &Theme, rect: Rect, e: EnvTimes, shape: EnvShape, lp: bool, ink: Color32) {
    p.rect_filled(rect, 4.0, t.lcd);
    let (a, d, s, r) = (e.a, e.d, e.s, e.r);
    let span = if lp { (a + d + r * 0.52) * 2.0 } else { a + d + r.max(0.18) };
    let (w, h) = (rect.width(), rect.height());
    let n = 160;
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let mut tt = (i as f64 / (n - 1) as f64) * span;
        let yv = if lp {
            let cycle = a + d + r * 0.52;
            let k = (tt / cycle.max(0.001)).floor();
            tt -= k * cycle;
            env_level(tt, e, shape, true)
        } else if tt <= a + d + 0.08 {
            env_level(tt, e, shape, false)
        } else {
            let rel = (tt - a - d - 0.08) / r.max(0.001);
            s * (1.0 - rel.min(1.0)).max(0.0)
        };
        let x = rect.left() + (i as f32 / (n - 1) as f32) * w;
        let y = rect.bottom() - 3.0 - (yv.clamp(0.0, 1.15) as f32) * (h - 6.0);
        pts.push(Pos2::new(x, y));
    }
    let bottom: Vec<Pos2> = pts.iter().map(|q| Pos2::new(q.x, rect.bottom())).collect();
    fill_between(p, &pts, &bottom, ink.gamma_multiply(0.16));
    p.line(pts, Stroke::new(1.6_f32, ink));
    let sy = rect.bottom() - 3.0 - s as f32 * (h - 6.0);
    p.line_segment(
        [Pos2::new(rect.left(), sy), Pos2::new(rect.right(), sy)],
        Stroke::new(1.0_f32, t.subtle.gamma_multiply(0.35)),
    );
}

/// `FiltScope`.
pub fn filt_scope(p: &egui::Painter, t: &Theme, rect: Rect, ty: FiltType, cut: f64, res: f64) {
    p.rect_filled(rect, 4.0, t.lcd);
    let q = 0.35 + res * 11.5;
    let gain_db = iki_core::shaku::env_shape::filt_gain_db(res);
    let mag = filt_mag_curve(ty, cut, q, gain_db, 96);
    let db = |m: f64| 20.0 * m.max(1e-6).log10();
    let (mut lo, mut hi) = (0f64, 0f64);
    for m in &mag {
        let v = db(*m);
        if v.is_finite() {
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    let lo = (lo - 2.0).min(-24.0);
    let hi = (hi + 2.0).max(12.0);
    let span = (hi - lo).max(1.0);
    let (w, h) = (rect.width(), rect.height());
    let pts: Vec<Pos2> = mag
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let x = rect.left() + (i as f32 / (mag.len() - 1) as f32) * w;
            let v = db(*m);
            let v = if v.is_finite() { v } else { lo };
            let y = rect.bottom() - (((v - lo) / span) as f32) * (h - 4.0) - 2.0;
            Pos2::new(x, y)
        })
        .collect();
    let bottom: Vec<Pos2> = pts.iter().map(|q| Pos2::new(q.x, rect.bottom())).collect();
    fill_between(p, &pts, &bottom, t.moss.gamma_multiply(0.18));
    p.line(pts, Stroke::new(1.7_f32, t.iris));
    // 0 dB guide
    let y0 = rect.bottom() - (((0.0 - lo) / span) as f32) * (h - 4.0) - 2.0;
    p.line_segment(
        [Pos2::new(rect.left(), y0), Pos2::new(rect.right(), y0)],
        Stroke::new(1.0_f32, blend(t.lcd, t.subtle, 0.3)),
    );
}
