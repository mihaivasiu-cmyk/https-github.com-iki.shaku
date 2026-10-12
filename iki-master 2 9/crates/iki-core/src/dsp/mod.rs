//! DSP building blocks mirroring the Web Audio nodes the original graph uses.

pub mod biquad;
pub mod param;
pub mod waveguide;

use std::f32::consts::PI;

/// Tiny xorshift PRNG — deterministic, allocation-free `Math.random()` stand-in.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    /// Uniform in [0, 1).
    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        ((x >> 40) as f32) / ((1u64 << 24) as f32)
    }
    #[inline]
    pub fn bipolar(&mut self) -> f32 {
        self.next_f32() * 2.0 - 1.0
    }
}

/// DelayNode: fractional (linear-interpolated) delay line.
#[derive(Clone, Debug)]
pub struct DelayLine {
    buf: Vec<f32>,
    w: usize,
}

impl DelayLine {
    pub fn new(max_sec: f64, sr: f32) -> Self {
        let n = (max_sec * sr as f64).ceil() as usize + 4;
        Self { buf: vec![0.0; n], w: 0 }
    }

    /// Read `d` samples behind the next write position (d >= 1 for feedback use).
    #[inline]
    pub fn read(&self, d: f32) -> f32 {
        let n = self.buf.len();
        let d = d.clamp(1.0, (n - 2) as f32);
        let di = d as usize;
        let f = d - di as f32;
        let i0 = (self.w + n - di) % n;
        let i1 = (i0 + n - 1) % n;
        self.buf[i0] + (self.buf[i1] - self.buf[i0]) * f
    }

    #[inline]
    pub fn write(&mut self, x: f32) {
        self.buf[self.w] = x;
        self.w += 1;
        if self.w >= self.buf.len() {
            self.w = 0;
        }
    }

    /// Write-then-read so a zero delay passes the input straight through.
    #[inline]
    pub fn process(&mut self, x: f32, d: f32) -> f32 {
        if d < 1.0 {
            let prev = self.read(1.0);
            self.write(x);
            return x + (prev - x) * d.max(0.0);
        }
        let y = self.read(d);
        self.write(x);
        y
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wave {
    Sine,
    Triangle,
    Sawtooth,
    Square,
}

impl Wave {
    pub const ALL: [(Wave, &'static str); 4] =
        [(Wave::Sine, "Sine"), (Wave::Triangle, "Tri"), (Wave::Sawtooth, "Saw"), (Wave::Square, "Sqr")];
    pub fn name(self) -> &'static str {
        match self {
            Wave::Sine => "sine",
            Wave::Triangle => "triangle",
            Wave::Sawtooth => "sawtooth",
            Wave::Square => "square",
        }
    }
}

/// OscillatorNode used as an LFO (naive waveforms are fine at sub-audio rates).
/// Phase conventions follow the Web Audio periodic waves (all start at 0, rising).
#[derive(Clone, Debug)]
pub struct Osc {
    pub wave: Wave,
    phase: f64,
}

impl Osc {
    pub fn new(wave: Wave) -> Self {
        Self { wave, phase: 0.0 }
    }

    #[inline]
    pub fn next(&mut self, hz: f32, sr: f32) -> f32 {
        let p = self.phase as f32;
        let y = match self.wave {
            Wave::Sine => (2.0 * PI * p).sin(),
            Wave::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Wave::Sawtooth => {
                if p < 0.5 {
                    2.0 * p
                } else {
                    2.0 * p - 2.0
                }
            }
            Wave::Triangle => {
                if p < 0.25 {
                    4.0 * p
                } else if p < 0.75 {
                    2.0 - 4.0 * p
                } else {
                    4.0 * p - 4.0
                }
            }
        };
        self.phase += hz as f64 / sr as f64;
        self.phase -= self.phase.floor();
        y
    }
}

/// DynamicsCompressorNode approximation: soft knee, attack/release smoothing on
/// the gain reduction, and Chromium-style automatic makeup gain.
#[derive(Clone, Debug)]
pub struct Compressor {
    threshold: f32,
    knee: f32,
    ratio: f32,
    att: f32,
    rel: f32,
    makeup: f32,
    gr_db: f32,
}

impl Compressor {
    pub fn new(sr: f32, threshold: f32, knee: f32, ratio: f32, attack: f32, release: f32) -> Self {
        let mut c = Self {
            threshold,
            knee,
            ratio,
            att: 1.0 - (-1.0 / (attack * sr)).exp(),
            rel: 1.0 - (-1.0 / (release * sr)).exp(),
            makeup: 1.0,
            gr_db: 0.0,
        };
        let full = c.curve_db(0.0);
        c.makeup = db_to_lin(-full * 0.6);
        c
    }

    fn curve_db(&self, x: f32) -> f32 {
        let (t, k, r) = (self.threshold, self.knee, self.ratio);
        if x < t - k / 2.0 {
            x
        } else if x <= t + k / 2.0 {
            let d = x - t + k / 2.0;
            x + (1.0 / r - 1.0) * d * d / (2.0 * k)
        } else {
            t + (x - t) / r
        }
    }

    /// Current gain reduction in dB (positive number).
    pub fn reduction_db(&self) -> f32 {
        self.gr_db
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let peak = l.abs().max(r.abs()).max(1e-9);
        let x_db = 20.0 * peak.log10();
        let want = (x_db - self.curve_db(x_db)).max(0.0);
        let k = if want > self.gr_db { self.att } else { self.rel };
        self.gr_db += (want - self.gr_db) * k;
        let g = db_to_lin(-self.gr_db) * self.makeup;
        (l * g, r * g)
    }
}

#[inline]
pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// WaveShaperNode with a fixed curve (linear interpolation, no oversampling).
#[derive(Clone, Debug)]
pub struct Shaper {
    curve: Vec<f32>,
}

impl Shaper {
    /// The tape curve from `engine.ts`.
    pub fn tape() -> Self {
        let n = 1024;
        let curve = (0..n)
            .map(|i| {
                let x = (i as f32 / (n - 1) as f32) * 2.0 - 1.0;
                ((x * 1.85 + 0.14).tanh() - 0.14f32.tanh()) * 0.9
            })
            .collect();
        Self { curve }
    }

    #[inline]
    pub fn process(&self, x: f32) -> f32 {
        let n = self.curve.len();
        let v = (n - 1) as f32 * (x + 1.0) * 0.5;
        if v <= 0.0 {
            return self.curve[0];
        }
        let k = v as usize;
        if k >= n - 1 {
            return self.curve[n - 1];
        }
        let f = v - k as f32;
        self.curve[k] + (self.curve[k + 1] - self.curve[k]) * f
    }
}

/// The 2.4 s looped pink-noise buffer from `engine.ts` (Paul Kellet filter).
pub fn pink_buffer(sr: f32, rng: &mut Rng) -> Vec<f32> {
    let len = (sr * 2.4) as usize;
    let (mut b0, mut b1, mut b2, mut b3, mut b4, mut b5) = (0f32, 0f32, 0f32, 0f32, 0f32, 0f32);
    (0..len)
        .map(|_| {
            let w = rng.bipolar();
            b0 = 0.99886 * b0 + w * 0.0555179;
            b1 = 0.99332 * b1 + w * 0.0750759;
            b2 = 0.969 * b2 + w * 0.153852;
            b3 = 0.8665 * b3 + w * 0.3104856;
            b4 = 0.55 * b4 + w * 0.5329522;
            b5 = -0.7616 * b5 - w * 0.016898;
            let b6 = w * 0.115926;
            (b0 + b1 + b2 + b3 + b4 + b5 + b6 + w * 0.5362) * 0.055
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_integer() {
        let mut d = DelayLine::new(0.01, 1000.0);
        for i in 0..5 {
            d.write(i as f32);
        }
        assert_eq!(d.read(1.0), 4.0);
        assert_eq!(d.read(3.0), 2.0);
        assert!((d.read(2.5) - 2.5).abs() < 1e-6);
        assert_eq!(d.process(9.0, 0.0), 9.0);
    }

    #[test]
    fn compressor_makeup() {
        let c = Compressor::new(48000.0, -18.0, 10.0, 2.0, 0.01, 0.2);
        let db = 20.0 * c.makeup.log10();
        assert!((db - 5.4).abs() < 0.1, "{db}");
    }

    #[test]
    fn lfo_shapes_start_at_zero() {
        for (w, _) in Wave::ALL {
            let mut o = Osc::new(w);
            let y = o.next(1.0, 100.0);
            if w != Wave::Square {
                assert!(y.abs() < 1e-6);
            }
        }
    }
}
