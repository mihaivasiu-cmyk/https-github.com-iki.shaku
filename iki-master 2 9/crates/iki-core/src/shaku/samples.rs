//! Ports of `samples.ts` and `loop-prep.ts`: the per-note shakuhachi recordings,
//! and the seamless-loop finder that picks the best crossfaded sustain region.

use std::sync::Arc;

pub struct SampleDef {
    pub name: &'static str,
    pub freq: f64,
    pub wav: &'static [u8],
}

/// Recordings are embedded so the binary is self-contained. `freq` is the
/// measured pitch of each recording. 36 looped notes from a studio shakuhachi
/// with every semitone recorded, in two dynamics: index `dynamic * 18 + layer * 6 + note`.
/// Dynamic 0 is the soft take, 1 the hard one (chosen by breath at note start).
/// Layer 0 is the kari note (Ro Tsu Re Chi Ri Hi), 1 the chu-meri note and 2
/// the dai-meri note (Re and Chi have no deeper step, so they reuse chu).
/// Built by `tools/sample-import/build_sp_bank.py`; the earlier banks are kept
/// in `assets/samples_riley` and `assets/samples_nbm`.
pub const SAMPLE_BANK: [SampleDef; 36] = [
    SampleDef { name: "ro", freq: 292.93, wav: include_bytes!("../../assets/samples/ro.wav") },
    SampleDef { name: "tsu", freq: 346.2, wav: include_bytes!("../../assets/samples/tsu.wav") },
    SampleDef { name: "re", freq: 390.42, wav: include_bytes!("../../assets/samples/re.wav") },
    SampleDef { name: "chi", freq: 437.04, wav: include_bytes!("../../assets/samples/chi.wav") },
    SampleDef { name: "ri", freq: 521.33, wav: include_bytes!("../../assets/samples/ri.wav") },
    SampleDef { name: "hi", freq: 582.74, wav: include_bytes!("../../assets/samples/hi.wav") },
    SampleDef { name: "ro_chu", freq: 278.78, wav: include_bytes!("../../assets/samples/ro_chu.wav") },
    SampleDef { name: "tsu_chu", freq: 329.12, wav: include_bytes!("../../assets/samples/tsu_chu.wav") },
    SampleDef { name: "re_chu", freq: 371.75, wav: include_bytes!("../../assets/samples/re_chu.wav") },
    SampleDef { name: "chi_chu", freq: 415.13, wav: include_bytes!("../../assets/samples/chi_chu.wav") },
    SampleDef { name: "ri_chu", freq: 494.33, wav: include_bytes!("../../assets/samples/ri_chu.wav") },
    SampleDef { name: "hi_chu", freq: 554.15, wav: include_bytes!("../../assets/samples/hi_chu.wav") },
    SampleDef { name: "ro_dai", freq: 262.08, wav: include_bytes!("../../assets/samples/ro_dai.wav") },
    SampleDef { name: "tsu_dai", freq: 313.69, wav: include_bytes!("../../assets/samples/tsu_dai.wav") },
    SampleDef { name: "re_dai", freq: 371.75, wav: include_bytes!("../../assets/samples/re_dai.wav") },
    SampleDef { name: "chi_dai", freq: 415.13, wav: include_bytes!("../../assets/samples/chi_dai.wav") },
    SampleDef { name: "ri_dai", freq: 466.2, wav: include_bytes!("../../assets/samples/ri_dai.wav") },
    SampleDef { name: "hi_dai", freq: 521.33, wav: include_bytes!("../../assets/samples/hi_dai.wav") },
    SampleDef { name: "ro_d1", freq: 293.1, wav: include_bytes!("../../assets/samples/ro_d1.wav") },
    SampleDef { name: "tsu_d1", freq: 345.88, wav: include_bytes!("../../assets/samples/tsu_d1.wav") },
    SampleDef { name: "re_d1", freq: 390.46, wav: include_bytes!("../../assets/samples/re_d1.wav") },
    SampleDef { name: "chi_d1", freq: 436.05, wav: include_bytes!("../../assets/samples/chi_d1.wav") },
    SampleDef { name: "ri_d1", freq: 520.87, wav: include_bytes!("../../assets/samples/ri_d1.wav") },
    SampleDef { name: "hi_d1", freq: 586.69, wav: include_bytes!("../../assets/samples/hi_d1.wav") },
    SampleDef { name: "ro_chu_d1", freq: 278.8, wav: include_bytes!("../../assets/samples/ro_chu_d1.wav") },
    SampleDef { name: "tsu_chu_d1", freq: 329.64, wav: include_bytes!("../../assets/samples/tsu_chu_d1.wav") },
    SampleDef { name: "re_chu_d1", freq: 370.91, wav: include_bytes!("../../assets/samples/re_chu_d1.wav") },
    SampleDef { name: "chi_chu_d1", freq: 414.37, wav: include_bytes!("../../assets/samples/chi_chu_d1.wav") },
    SampleDef { name: "ri_chu_d1", freq: 491.24, wav: include_bytes!("../../assets/samples/ri_chu_d1.wav") },
    SampleDef { name: "hi_chu_d1", freq: 552.21, wav: include_bytes!("../../assets/samples/hi_chu_d1.wav") },
    SampleDef { name: "ro_dai_d1", freq: 261.37, wav: include_bytes!("../../assets/samples/ro_dai_d1.wav") },
    SampleDef { name: "tsu_dai_d1", freq: 312.04, wav: include_bytes!("../../assets/samples/tsu_dai_d1.wav") },
    SampleDef { name: "re_dai_d1", freq: 370.91, wav: include_bytes!("../../assets/samples/re_dai_d1.wav") },
    SampleDef { name: "chi_dai_d1", freq: 414.37, wav: include_bytes!("../../assets/samples/chi_dai_d1.wav") },
    SampleDef { name: "ri_dai_d1", freq: 466.84, wav: include_bytes!("../../assets/samples/ri_dai_d1.wav") },
    SampleDef { name: "hi_dai_d1", freq: 520.87, wav: include_bytes!("../../assets/samples/hi_dai_d1.wav") },
];

/// A decoded sample with its seamless loop region (in samples of `data`).
pub struct LoopedSample {
    pub data: Vec<f32>,
    pub sr: f32,
    pub loop_start: f64,
    pub loop_end: f64,
}

pub type Bank = Arc<Vec<LoopedSample>>;

pub fn decode_wav(bytes: &[u8]) -> Result<(Vec<f32>, f32), String> {
    let mut r = hound::WavReader::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().map(|s| s.unwrap_or(0) as f32 * scale).collect()
        }
    };
    // First channel only, like `getChannelData(0)`.
    let data = raw.chunks(ch).map(|c| c[0]).collect();
    Ok((data, spec.sample_rate as f32))
}

fn rms(data: &[f32], a: usize, b: usize) -> f32 {
    let n = b.saturating_sub(a).max(1);
    let s: f32 = data[a..b].iter().map(|x| x * x).sum();
    (s / n as f32).sqrt()
}

fn ncc(data: &[f32], a: usize, b: usize, n: usize) -> f32 {
    let (mut c, mut e1, mut e2) = (0f32, 0f32, 0f32);
    let mut i = 0;
    while i < n {
        let x = data[a + i];
        let y = data[b + i];
        c += x * y;
        e1 += x * x;
        e2 += y * y;
        i += 2;
    }
    let den = (e1 * e2).sqrt();
    if den > 1e-12 { c / den } else { 0.0 }
}

/// `seamLoop`: search start/length pairs for the best-correlated, level-balanced
/// loop, then equal-power crossfade the tail into the pre-start region.
pub fn seam_loop(src: &[f32], sr: f32) -> LoopedSample {
    let len = src.len();
    let fade = ((0.1 * sr) as usize).min(len / 8);
    let min_start = fade + (0.18 * sr) as usize;
    let max_end = len.saturating_sub((0.04 * sr) as usize);
    let min_len = (0.45 * sr) as usize;
    let max_len = ((1.15 * sr) as usize).min(max_end.saturating_sub(min_start));
    let step = ((0.012 * sr) as usize).max(64);
    let mut best = (min_start, min_start + min_len, -1e9f32);

    let mut start = min_start;
    while start + min_len <= max_end {
        let mut l = min_len;
        while l <= max_len && start + l <= max_end {
            let end = start + l;
            let r0 = rms(src, start, start + fade);
            let r1 = rms(src, end - fade, end);
            if r0 >= 0.03 && r1 >= 0.03 {
                let corr = ncc(src, start - fade, end - fade, fade);
                let bal = 1.0 - (r0 - r1).abs() / (r0 + r1 + 1e-6);
                let score = corr * 2.0 + bal;
                if score > best.2 {
                    best = (start, end, score);
                }
            }
            l += step;
        }
        start += step;
    }

    let mut d = src.to_vec();
    let last = fade.saturating_sub(1).max(1) as f32;
    let (bs, be) = (best.0, best.1.min(len));
    for i in 0..fade {
        let t = i as f32 / last;
        let a = (t * std::f32::consts::FRAC_PI_2).cos();
        let b = (t * std::f32::consts::FRAC_PI_2).sin();
        let ei = be - fade + i;
        let pi = bs - fade + i;
        if ei < len && pi < len {
            d[ei] = src[ei] * a + src[pi] * b;
        }
    }
    LoopedSample { data: d, sr, loop_start: bs as f64, loop_end: be as f64 }
}

/// Decode and loop-prep every note (in parallel — the search is O(n²)-ish).
pub fn load_bank() -> Result<Bank, String> {
    let results: Vec<Result<LoopedSample, String>> = std::thread::scope(|s| {
        let handles: Vec<_> = SAMPLE_BANK
            .iter()
            .map(|def| {
                s.spawn(move || {
                    let (data, sr) = decode_wav(def.wav).map_err(|e| format!("sample {}: {e}", def.name))?;
                    Ok(seam_loop(&data, sr))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("loader panicked".into()))).collect()
    });
    let bank = results.into_iter().collect::<Result<Vec<_>, _>>()?;
    Ok(Arc::new(bank))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bank_loads_with_sane_loops() {
        let bank = load_bank().unwrap();
        assert_eq!(bank.len(), 36);
        for s in bank.iter() {
            assert!(s.loop_end > s.loop_start);
            let span = (s.loop_end - s.loop_start) / s.sr as f64;
            assert!((0.44..=1.16).contains(&span), "loop span {span}");
        }
    }
}
