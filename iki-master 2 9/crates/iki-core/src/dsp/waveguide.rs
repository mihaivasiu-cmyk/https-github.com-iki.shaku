//! Port of `public/shaku-processor.js` — the shakuhachi waveguide
//! (bore delay + short jet delay + cubic nonlinearity, Cook/STK flute family).
//!
//! In the web app this AudioWorklet was shipped but never loaded; the live
//! instrument is the sampler in `engine.rs`. Here it is available as the
//! optional "Bore" voice so the kernel is not lost.

use super::Rng;

pub struct Waveguide {
    bore: Vec<f32>,
    jet: Vec<f32>,
    b_w: usize,
    j_w: usize,
    lp: f32,
    lp2: f32,
    dc_x: f32,
    dc_y: f32,
    env: f32,
    noise_lp: f32,
    noise_bp: f32,
    wander: f32,
    vib_phase: f32,
    last_blow: f32,
    chiff: f32,
    smooth_freq: f32,
    smooth_meri: f32,
    smooth_iki: f32,
    form_x1: f32,
    form_y1: f32,
    form_x2: f32,
    form_y2: f32,
    form_b0: f32,
    form_a1: f32,
    form_a2: f32,
    pub blowing: f32,
    pub freq: f32,
    pub meri: f32,
    pub iki: f32,
    pub volume: f32,
    rng: Rng,
    sr: f32,
    block_pos: usize,
    // per-block derived values
    delay_samp: f32,
    jet_samp: f32,
    noise_amt: f32,
    jet_gain: f32,
    feedback: f32,
    cut: f32,
    vol: f32,
    env_atk: f32,
}

/// The worklet ran per 128-frame render quantum; parameter smoothing is per block.
const QUANTUM: usize = 128;

impl Waveguide {
    pub fn new(sr: f32) -> Self {
        Self {
            bore: vec![0.0; 8192],
            jet: vec![0.0; 256],
            b_w: 0,
            j_w: 0,
            lp: 0.0,
            lp2: 0.0,
            dc_x: 0.0,
            dc_y: 0.0,
            env: 0.0,
            noise_lp: 0.0,
            noise_bp: 0.0,
            wander: 0.0,
            vib_phase: 0.0,
            last_blow: 0.0,
            chiff: 0.0,
            smooth_freq: 293.66,
            smooth_meri: 0.12,
            smooth_iki: 0.52,
            form_x1: 0.0,
            form_y1: 0.0,
            form_x2: 0.0,
            form_y2: 0.0,
            form_b0: 0.0,
            form_a1: 0.0,
            form_a2: 0.0,
            blowing: 0.0,
            freq: 293.66,
            meri: 0.12,
            iki: 0.52,
            volume: 0.78,
            rng: Rng::new(0x5eed_b0e5),
            sr,
            block_pos: 0,
            delay_samp: 150.0,
            jet_samp: 12.0,
            noise_amt: 0.0,
            jet_gain: 0.0,
            feedback: 0.0,
            cut: 0.1,
            vol: 0.0,
            env_atk: 0.001,
        }
    }

    fn begin_block(&mut self) {
        let n = QUANTUM as f32;
        self.smooth_freq += (self.freq - self.smooth_freq) * 0.0018 * n;
        self.smooth_meri += (self.meri - self.smooth_meri) * 0.003 * n;
        self.smooth_iki += (self.iki - self.smooth_iki) * 0.003 * n;
        let meri = self.smooth_meri;
        let iki = self.smooth_iki;
        // Embouchure flattens up to ~2.5 semitones and darkens.
        let meri_ratio = 2f32.powf((-meri * 2.55) / 12.0);
        // High iki overblows toward the kan (octave).
        let kan_blend = (iki - 0.78).max(0.0) / 0.22;
        let target_f = (self.smooth_freq * meri_ratio * (1.0 + kan_blend * 0.995)).max(90.0);
        self.delay_samp = (self.sr / target_f).max(12.0);
        self.jet_samp = (self.delay_samp * (0.08 + meri * 0.04)).clamp(2.5, 48.0);
        self.noise_amt = 0.18 + iki * 0.38 + meri * 0.12;
        self.jet_gain = 0.55 + iki * 1.15;
        self.feedback = 0.62 + iki * 0.16 - meri * 0.1;
        self.cut = (0.1 + iki * 0.28 - meri * 0.16).clamp(0.045, 0.55);
        self.vol = self.volume * self.volume;
        self.env_atk = if self.blowing > self.env { 0.0016 + iki * 0.001 } else { 0.0009 };
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
        if self.block_pos == 0 {
            self.begin_block();
        }
        self.block_pos = (self.block_pos + 1) % QUANTUM;
        let sr = self.sr;
        let meri = self.smooth_meri;
        let iki = self.smooth_iki;
        let dc_r = 0.995;

        self.env += (self.blowing - self.env) * self.env_atk;
        if self.blowing > 0.5 && self.last_blow < 0.5 {
            self.chiff = 1.0;
        }
        self.chiff *= 0.9992;
        self.last_blow = self.blowing;

        self.wander += (self.rng.next_f32() - 0.5) * 0.00012;
        self.wander *= 0.9994;
        self.vib_phase += (4.6 + iki * 1.4) / sr;
        if self.vib_phase > 1.0 {
            self.vib_phase -= 1.0;
        }
        let vib = (self.vib_phase * 2.0 * std::f32::consts::PI).sin() * (0.0018 + iki * 0.0012);
        let d_len = self.delay_samp * (1.0 + self.wander + vib);

        let nse = self.rng.bipolar();
        self.noise_lp += (nse - self.noise_lp) * 0.08;
        self.noise_bp = nse - self.noise_lp;
        let breath =
            self.env * (self.noise_lp * 0.55 + self.noise_bp * (0.35 + self.noise_amt * 0.5) + self.chiff * nse * 0.35);

        // Fractional bore read (cubic)
        let bl = self.bore.len();
        let mut r_pos = self.b_w as f32 - d_len;
        while r_pos < 0.0 {
            r_pos += bl as f32;
        }
        let i0 = (r_pos as usize) % bl;
        let c = r_pos - r_pos.floor();
        let i1 = if i0 + 1 < bl { i0 + 1 } else { 0 };
        let i2 = if i1 + 1 < bl { i1 + 1 } else { 0 };
        let ym1 = self.bore[if i0 == 0 { bl - 1 } else { i0 - 1 }];
        let y0 = self.bore[i0];
        let y1 = self.bore[i1];
        let y2 = self.bore[i2];
        let mut bore_out =
            y0 + 0.5 * c * (y1 - ym1 + c * (2.0 * ym1 - 5.0 * y0 + 4.0 * y1 - y2 + c * (3.0 * (y0 - y1) + y2 - ym1)));

        let hp = bore_out - self.dc_x + dc_r * self.dc_y;
        self.dc_x = bore_out;
        self.dc_y = hp;
        bore_out = hp;

        let jet_in = breath * self.jet_gain - 0.42 * bore_out;
        let jl = self.jet.len();
        self.jet[self.j_w] = jet_in;
        self.j_w = (self.j_w + 1) % jl;
        let mut j_pos = self.j_w as f32 - self.jet_samp;
        while j_pos < 0.0 {
            j_pos += jl as f32;
        }
        let ji = (j_pos as usize) % jl;
        let jf = j_pos - j_pos.floor();
        let ji1 = if ji + 1 < jl { ji + 1 } else { 0 };
        let mut jet = self.jet[ji] + (self.jet[ji1] - self.jet[ji]) * jf;

        // STK jet table
        jet *= 0.7 + iki * 0.55;
        jet = (jet - 0.333 * jet * jet * jet).clamp(-1.0, 1.0);

        let mut x = jet + bore_out * self.feedback;
        self.lp += (x - self.lp) * self.cut;
        self.lp2 += (self.lp - self.lp2) * (0.35 + (1.0 - meri) * 0.4);
        x = self.lp2;

        self.bore[self.b_w] = x * 0.995 + 1e-12;
        self.b_w = (self.b_w + 1) % bl;

        // Bamboo body formant ~900 Hz. The worklet's "one-pole resonator"
        // (`y += 0.12(x-y) + 0.92·y·f·8`) has a pole above 1 at every sample
        // rate and diverges to NaN within ~0.1 s, silencing the voice. This is
        // the intended resonance as a stable 2-pole band-pass (RBJ, Q 1.4).
        if self.block_pos == 1 {
            let f0 = (900.0 - meri * 220.0) / sr;
            let w0 = 2.0 * std::f32::consts::PI * f0;
            let alpha = w0.sin() / (2.0 * 1.4);
            let a0 = 1.0 + alpha;
            self.form_b0 = alpha / a0;
            self.form_a1 = -2.0 * w0.cos() / a0;
            self.form_a2 = (1.0 - alpha) / a0;
        }
        let form = self.form_b0 * (x - self.form_x2) - self.form_a1 * self.form_y1 - self.form_a2 * self.form_y2;
        self.form_x2 = self.form_x1;
        self.form_x1 = x;
        self.form_y2 = self.form_y1;
        self.form_y1 = form;

        let mut y = x * 0.72 + form * 0.22 + self.noise_bp * self.env * self.noise_amt * 0.16 + self.chiff * nse * 0.04;
        y *= self.env * (0.28 + self.vol * 0.85);
        let y = (y * 1.55).tanh();
        if y.is_finite() { y } else { 0.0 }
    }
}
