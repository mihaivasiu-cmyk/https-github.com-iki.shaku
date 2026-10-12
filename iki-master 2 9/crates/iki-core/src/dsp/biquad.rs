//! BiquadFilterNode with the coefficient formulas from the Web Audio spec
//! (note: lowpass/highpass Q is in dB there; shelves ignore Q).

use crate::shaku::env_shape::FiltType;

#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    pub kind: FiltType,
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
    last: (f32, f32, f32),
    sr: f64,
}

impl Biquad {
    pub fn new(kind: FiltType, sr: f32) -> Self {
        let mut b = Self {
            kind,
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
            last: (-1.0, -1.0, -1.0),
            sr: sr as f64,
        };
        b.set(350.0, 1.0, 0.0);
        b
    }

    pub fn set_kind(&mut self, kind: FiltType) {
        if self.kind != kind {
            self.kind = kind;
            self.last = (-1.0, -1.0, -1.0);
        }
    }

    /// Recompute coefficients if any parameter moved.
    pub fn set(&mut self, freq: f32, q: f32, gain_db: f32) {
        if self.last == (freq, q, gain_db) {
            return;
        }
        self.last = (freq, q, gain_db);
        let nyq = self.sr * 0.5;
        let f = (freq as f64).clamp(0.0, nyq) / nyq; // normalized 0..1
        let f = f.clamp(1e-5, 0.99999);
        let w0 = std::f64::consts::PI * f;
        let (sin, cos) = w0.sin_cos();
        let q = q as f64;
        let a = 10f64.powf(gain_db as f64 / 40.0);
        let (b0, b1, b2, a0, a1, a2);
        match self.kind {
            FiltType::Lowpass => {
                let alpha = sin / (2.0 * 10f64.powf(q / 20.0));
                b0 = (1.0 - cos) / 2.0;
                b1 = 1.0 - cos;
                b2 = (1.0 - cos) / 2.0;
                a0 = 1.0 + alpha;
                a1 = -2.0 * cos;
                a2 = 1.0 - alpha;
            }
            FiltType::Highpass => {
                let alpha = sin / (2.0 * 10f64.powf(q / 20.0));
                b0 = (1.0 + cos) / 2.0;
                b1 = -(1.0 + cos);
                b2 = (1.0 + cos) / 2.0;
                a0 = 1.0 + alpha;
                a1 = -2.0 * cos;
                a2 = 1.0 - alpha;
            }
            FiltType::Bandpass => {
                let alpha = sin / (2.0 * q.max(1e-4));
                b0 = alpha;
                b1 = 0.0;
                b2 = -alpha;
                a0 = 1.0 + alpha;
                a1 = -2.0 * cos;
                a2 = 1.0 - alpha;
            }
            FiltType::Notch => {
                let alpha = sin / (2.0 * q.max(1e-4));
                b0 = 1.0;
                b1 = -2.0 * cos;
                b2 = 1.0;
                a0 = 1.0 + alpha;
                a1 = -2.0 * cos;
                a2 = 1.0 - alpha;
            }
            FiltType::Allpass => {
                let alpha = sin / (2.0 * q.max(1e-4));
                b0 = 1.0 - alpha;
                b1 = -2.0 * cos;
                b2 = 1.0 + alpha;
                a0 = 1.0 + alpha;
                a1 = -2.0 * cos;
                a2 = 1.0 - alpha;
            }
            FiltType::Peaking => {
                let alpha = sin / (2.0 * q.max(1e-4));
                b0 = 1.0 + alpha * a;
                b1 = -2.0 * cos;
                b2 = 1.0 - alpha * a;
                a0 = 1.0 + alpha / a;
                a1 = -2.0 * cos;
                a2 = 1.0 - alpha / a;
            }
            FiltType::Lowshelf => {
                let alpha = sin / 2.0 * 2f64.sqrt();
                let k = 2.0 * a.sqrt() * alpha;
                b0 = a * ((a + 1.0) - (a - 1.0) * cos + k);
                b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos);
                b2 = a * ((a + 1.0) - (a - 1.0) * cos - k);
                a0 = (a + 1.0) + (a - 1.0) * cos + k;
                a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos);
                a2 = (a + 1.0) + (a - 1.0) * cos - k;
            }
            FiltType::Highshelf => {
                let alpha = sin / 2.0 * 2f64.sqrt();
                let k = 2.0 * a.sqrt() * alpha;
                b0 = a * ((a + 1.0) + (a - 1.0) * cos + k);
                b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos);
                b2 = a * ((a + 1.0) + (a - 1.0) * cos - k);
                a0 = (a + 1.0) - (a - 1.0) * cos + k;
                a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos);
                a2 = (a + 1.0) - (a - 1.0) * cos - k;
            }
        }
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let x = x as f64;
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        // Flush denormals / recover from blow-ups.
        if !y.is_finite() {
            self.z1 = 0.0;
            self.z2 = 0.0;
            return 0.0;
        }
        if self.z1.abs() < 1e-25 {
            self.z1 = 0.0;
        }
        if self.z2.abs() < 1e-25 {
            self.z2 = 0.0;
        }
        y as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gain_at(b: &mut Biquad, hz: f32, sr: f32) -> f32 {
        let mut peak = 0f32;
        for i in 0..(sr as usize) {
            let y = b.process((2.0 * std::f32::consts::PI * hz * i as f32 / sr).sin());
            if i > sr as usize / 2 {
                peak = peak.max(y.abs());
            }
        }
        peak
    }

    #[test]
    fn lowpass_attenuates_highs() {
        let sr = 48000.0;
        let mut b = Biquad::new(FiltType::Lowpass, sr);
        b.set(1000.0, 0.0, 0.0);
        assert!(gain_at(&mut b, 100.0, sr) > 0.95);
        let mut b = Biquad::new(FiltType::Lowpass, sr);
        b.set(1000.0, 0.0, 0.0);
        assert!(gain_at(&mut b, 8000.0, sr) < 0.05);
    }

    #[test]
    fn peaking_boosts() {
        let sr = 48000.0;
        let mut b = Biquad::new(FiltType::Peaking, sr);
        b.set(1000.0, 1.0, 6.0);
        let g = gain_at(&mut b, 1000.0, sr);
        assert!((g - 2.0).abs() < 0.05, "{g}");
    }
}
