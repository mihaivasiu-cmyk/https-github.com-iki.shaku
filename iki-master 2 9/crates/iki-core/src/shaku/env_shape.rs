//! Port of `src/lib/shaku/env-shape.ts` — envelope shapes, filter types and ranges.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvShape {
    Lin,
    Exp,
    Punch,
    Trap,
    Fold,
}

pub const ENV_SHAPES: [(EnvShape, &str); 5] = [
    (EnvShape::Lin, "Lin"),
    (EnvShape::Exp, "Exp"),
    (EnvShape::Punch, "Punch"),
    (EnvShape::Trap, "Trap"),
    (EnvShape::Fold, "Fold"),
];

/// Web Audio `BiquadFilterType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FiltType {
    Lowpass,
    Highpass,
    Bandpass,
    Notch,
    Peaking,
    Lowshelf,
    Highshelf,
    Allpass,
}

pub const FILT_TYPES: [(FiltType, &str); 8] = [
    (FiltType::Lowpass, "Lp"),
    (FiltType::Highpass, "Hp"),
    (FiltType::Bandpass, "Bp"),
    (FiltType::Notch, "Notch"),
    (FiltType::Peaking, "Peak"),
    (FiltType::Lowshelf, "LoSh"),
    (FiltType::Highshelf, "HiSh"),
    (FiltType::Allpass, "All"),
];

impl FiltType {
    pub fn name(self) -> &'static str {
        match self {
            FiltType::Lowpass => "lowpass",
            FiltType::Highpass => "highpass",
            FiltType::Bandpass => "bandpass",
            FiltType::Notch => "notch",
            FiltType::Peaking => "peaking",
            FiltType::Lowshelf => "lowshelf",
            FiltType::Highshelf => "highshelf",
            FiltType::Allpass => "allpass",
        }
    }
}

pub struct Limits {
    pub q: [f64; 2],
    pub gain: [f64; 2],
}

pub fn filt_limits(t: FiltType) -> Limits {
    match t {
        FiltType::Lowshelf | FiltType::Highshelf => Limits { q: [0.3, 1.1], gain: [-9.0, 9.0] },
        FiltType::Peaking => Limits { q: [0.3, 6.0], gain: [-8.0, 10.0] },
        FiltType::Notch => Limits { q: [0.4, 8.0], gain: [0.0, 0.0] },
        FiltType::Allpass => Limits { q: [0.3, 1.6], gain: [0.0, 0.0] },
        FiltType::Bandpass => Limits { q: [0.4, 7.0], gain: [0.0, 0.0] },
        _ => Limits { q: [0.3, 4.2], gain: [0.0, 0.0] },
    }
}

/// Boost (dB) for the peaking and shelf filters, driven by the Res knob:
/// 0 dB at the bottom up to the type's limit, so those filters always have an
/// audible effect (the old curve sat at ~0 dB around the default Res).
pub fn filt_gain_db(res: f64) -> f64 {
    res.clamp(0.0, 1.0) * 30.0
}

pub fn filt_cutoff(t: FiltType, cut01: f64) -> f64 {
    let x = cut01.clamp(0.0, 1.0);
    let log = |lo: f64, hi: f64| lo * (hi / lo).powf(x);
    match t {
        FiltType::Highpass => log(20.0, 4000.0),
        FiltType::Bandpass | FiltType::Notch | FiltType::Peaking => log(60.0, 10000.0),
        FiltType::Lowshelf => log(40.0, 2500.0),
        FiltType::Highshelf => log(300.0, 12000.0),
        FiltType::Allpass => log(60.0, 8000.0),
        FiltType::Lowpass => log(60.0, 16000.0),
    }
}

/// RBJ cookbook magnitude (display only), exactly as the web scope draws it.
pub fn filt_mag_curve(t: FiltType, f0: f64, q_raw: f64, gain_db_raw: f64, n: usize) -> Vec<f64> {
    let lim = filt_limits(t);
    let q = q_raw.clamp(lim.q[0], lim.q[1]);
    let gain_db = gain_db_raw.clamp(lim.gain[0], lim.gain[1]);
    let fs = 44100.0;
    let a = 10f64.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f64::consts::PI * f0.clamp(40.0, 14000.0) / fs;
    let (sin, cos) = w0.sin_cos();
    let alpha = sin / (2.0 * q);
    let (b0, b1, b2, a0, a1, a2) = match t {
        FiltType::Highpass => {
            ((1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
        }
        FiltType::Bandpass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
        FiltType::Notch => (1.0, -2.0 * cos, 1.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
        FiltType::Allpass => (1.0 - alpha, -2.0 * cos, 1.0 + alpha, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
        FiltType::Peaking => {
            (1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a)
        }
        FiltType::Lowshelf => {
            let two = 2.0 * a.sqrt() * alpha;
            (
                a * (a + 1.0 - (a - 1.0) * cos + two),
                2.0 * a * (a - 1.0 - (a + 1.0) * cos),
                a * (a + 1.0 - (a - 1.0) * cos - two),
                a + 1.0 + (a - 1.0) * cos + two,
                -2.0 * (a - 1.0 + (a + 1.0) * cos),
                a + 1.0 + (a - 1.0) * cos - two,
            )
        }
        FiltType::Highshelf => {
            let two = 2.0 * a.sqrt() * alpha;
            (
                a * (a + 1.0 + (a - 1.0) * cos + two),
                -2.0 * a * (a - 1.0 + (a + 1.0) * cos),
                a * (a + 1.0 + (a - 1.0) * cos - two),
                a + 1.0 - (a - 1.0) * cos + two,
                2.0 * (a - 1.0 - (a + 1.0) * cos),
                a + 1.0 - (a - 1.0) * cos - two,
            )
        }
        FiltType::Lowpass => ((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
    };
    let (c0, c1, c2, d1, d2) = (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0);
    (0..n)
        .map(|i| {
            let hz = 40.0 * (12000.0f64 / 40.0).powf(i as f64 / (n.max(2) - 1) as f64);
            let w = 2.0 * std::f64::consts::PI * hz / fs;
            let (z1i, z1r) = (-w).sin_cos();
            let (z2i, z2r) = (-2.0 * w).sin_cos();
            let nr = c0 + c1 * z1r + c2 * z2r;
            let ni = c1 * z1i + c2 * z2i;
            let dr = 1.0 + d1 * z1r + d2 * z2r;
            let di = d1 * z1i + d2 * z2i;
            let den = dr * dr + di * di;
            let hr = if den > 1e-12 { (nr * dr + ni * di) / den } else { 1.0 };
            let hi = if den > 1e-12 { (ni * dr - nr * di) / den } else { 0.0 };
            let m = (hr * hr + hi * hi).sqrt();
            if m.is_finite() { m } else { 1.0 }
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct EnvTimes {
    pub a: f64,
    pub d: f64,
    pub s: f64,
    pub r: f64,
}

pub fn env_seconds(a: f64, d: f64, s: f64, r: f64) -> EnvTimes {
    EnvTimes { a: 0.008 + a * a * 2.2, d: 0.02 + d * d * 2.4, s: s.clamp(0.0, 1.0), r: 0.03 + r * r * 3.5 }
}

fn rise(u: f64, shape: EnvShape) -> f64 {
    let t = u.clamp(0.0, 1.0);
    match shape {
        EnvShape::Exp => 1.0 - (-3.4 * t).exp(),
        EnvShape::Punch => (t.powf(0.32) * 1.08).min(1.0),
        EnvShape::Trap => {
            if t < 0.82 {
                t / 0.82
            } else {
                1.0
            }
        }
        EnvShape::Fold => (std::f64::consts::PI * t * 1.15).sin().abs().powf(0.72),
        EnvShape::Lin => t,
    }
}

fn fall(u: f64, shape: EnvShape) -> f64 {
    let t = u.clamp(0.0, 1.0);
    match shape {
        EnvShape::Exp => (-3.1 * t).exp(),
        EnvShape::Punch => (1.0 - t).powf(1.7),
        EnvShape::Trap => {
            if t < 0.18 {
                1.0
            } else {
                1.0 - (t - 0.18) / 0.82
            }
        }
        EnvShape::Fold => 0.5 + 0.5 * (std::f64::consts::PI * t).cos() * (1.0 - t * 0.35),
        EnvShape::Lin => 1.0 - t,
    }
}

pub fn env_level(t: f64, e: EnvTimes, shape: EnvShape, lp: bool) -> f64 {
    let EnvTimes { a, d, s, r } = e;
    let retr = if lp { (r * 0.4).max(0.03) } else { 0.0 };
    let hold = if lp { (r * 0.12).max(0.012) } else { 0.05 };
    if t <= a {
        return rise(t / a.max(0.001), shape);
    }
    if t <= a + d {
        let k = fall((t - a) / d.max(0.001), shape);
        return s + (1.0 - s) * k;
    }
    if t <= a + d + hold {
        return s;
    }
    if lp {
        let u = (t - a - d - hold) / retr.max(0.001);
        return (s * (1.0 - rise(u, shape))).max(0.0);
    }
    s
}

pub fn env_duration(e: EnvTimes, lp: bool) -> f64 {
    if !lp {
        return e.a + e.d + 0.05;
    }
    e.a + e.d + (e.r * 0.12).max(0.012) + (e.r * 0.4).max(0.03)
}

pub const ENV_CURVE_N: usize = 96;

pub fn env_curve(e: EnvTimes, shape: EnvShape, lp: bool) -> ([f32; ENV_CURVE_N], f64) {
    let n = ENV_CURVE_N;
    let dur = env_duration(e, lp).max(0.04);
    let mut curve = [0f32; ENV_CURVE_N];
    for (i, c) in curve.iter_mut().enumerate() {
        let t = (i as f64 / (n - 1) as f64) * dur;
        *c = env_level(t, e, shape, lp).clamp(0.0001, 1.2) as f32;
    }
    (curve, dur)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// At the default Res knob, the peaking and shelf filters must visibly bend
    /// the response (they once sat at ~0 dB and the buttons looked dead).
    #[test]
    fn gain_filters_are_audible_at_default_res() {
        let res: f64 = 0.22;
        for t in [FiltType::Peaking, FiltType::Lowshelf, FiltType::Highshelf] {
            let lim = filt_limits(t);
            let q = (0.35 + res * 11.5).clamp(lim.q[0], lim.q[1]);
            let g = filt_gain_db(res).clamp(lim.gain[0], lim.gain[1]);
            let mag = filt_mag_curve(t, filt_cutoff(t, 0.434), q, g, 96);
            let peak_db = mag.iter().map(|m| 20.0 * m.max(1e-6).log10().abs()).fold(0.0, f64::max);
            assert!(peak_db > 3.0, "{t:?} only moves the response by {peak_db:.1} dB at default Res");
        }
    }

    #[test]
    fn env_defaults() {
        let e = env_seconds(0.16, 0.22, 1.0, 0.28);
        assert!((e.a - (0.008 + 0.0256 * 2.2)).abs() < 1e-12);
        let (c, dur) = env_curve(e, EnvShape::Lin, false);
        assert!((c[0] - 0.0001).abs() < 1e-6);
        assert!((c[ENV_CURVE_N - 1] - 1.0).abs() < 1e-6);
        assert!((dur - (e.a + e.d + 0.05)).abs() < 1e-12);
    }

    #[test]
    fn lowpass_mag_passes_dc() {
        let m = filt_mag_curve(FiltType::Lowpass, 1000.0, 0.7, 0.0, 96);
        assert!((m[0] - 1.0).abs() < 0.05);
        assert!(m[95] < 0.1);
    }
}
