//! `AnalyserNode.getByteFrequencyData` equivalent (Blackman window, FFT,
//! time smoothing, dB → byte mapping with minDecibels −100 / maxDecibels −30).

use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

pub const FFT_SIZE: usize = 2048;
pub const BINS: usize = FFT_SIZE / 2;

pub struct Analyser {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    smooth: Vec<f32>,
    buf: Vec<Complex<f32>>,
    pub smoothing: f32,
    pub bytes: Vec<u8>,
}

impl Analyser {
    pub fn new(smoothing: f32) -> Self {
        let fft = FftPlanner::new().plan_fft_forward(FFT_SIZE);
        let window = (0..FFT_SIZE)
            .map(|n| {
                let x = n as f32 / FFT_SIZE as f32;
                0.42 - 0.5 * (2.0 * std::f32::consts::PI * x).cos() + 0.08 * (4.0 * std::f32::consts::PI * x).cos()
            })
            .collect();
        Self {
            fft,
            window,
            smooth: vec![0.0; BINS],
            buf: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            smoothing,
            bytes: vec![0; BINS],
        }
    }

    /// `time` must hold the most recent `FFT_SIZE` samples, oldest first.
    pub fn update(&mut self, time: impl Iterator<Item = f32>) {
        for ((b, x), w) in self.buf.iter_mut().zip(time).zip(self.window.iter()) {
            *b = Complex::new(x * w, 0.0);
        }
        self.fft.process(&mut self.buf);
        let tau = self.smoothing;
        for k in 0..BINS {
            let m = self.buf[k].norm() / FFT_SIZE as f32;
            let s = tau * self.smooth[k] + (1.0 - tau) * m;
            self.smooth[k] = if s.is_finite() { s } else { 0.0 };
            let db = 20.0 * self.smooth[k].max(1e-12).log10();
            self.bytes[k] = (255.0 * (db + 100.0) / 70.0).clamp(0.0, 255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_peaks_at_its_bin() {
        let mut a = Analyser::new(0.0);
        let sr = 48000.0;
        let hz = 1500.0;
        a.update((0..FFT_SIZE).map(|i| (2.0 * std::f32::consts::PI * hz * i as f32 / sr).sin() * 0.5));
        let peak = a.bytes.iter().enumerate().max_by_key(|(_, b)| **b).unwrap().0;
        let expect = (hz / (sr / FFT_SIZE as f32)).round() as usize;
        assert!((peak as i32 - expect as i32).abs() <= 1);
        assert_eq!(a.bytes[peak], 255);
    }
}
