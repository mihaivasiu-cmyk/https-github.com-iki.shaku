//! Port of `src/lib/shaku/fingering.ts` — 1.8 shakuhachi fingerings, meri/kari pitch maths.

/// Hole order: bottom 1, 2, 3, 4, thumb. `true` = covered.
pub type Holes = [bool; 5];

#[derive(Clone, Copy, Debug)]
pub struct NamedPitch {
    pub name: &'static str,
    pub kana: &'static str,
    pub freq: f64,
    pub holes: Holes,
}

/// D4, 1.8 otsu Ro.
pub const RO_FREQ: f64 = 293.6648;

/// Kari (otsu) pentatonic — 1.8 shakuhachi in D.
pub const FINGERINGS: [NamedPitch; 6] = [
    NamedPitch { name: "Ro", kana: "ロ", freq: 293.6648, holes: [true, true, true, true, true] },
    NamedPitch { name: "Tsu", kana: "ツ", freq: 349.2282, holes: [false, true, true, true, true] },
    NamedPitch { name: "Re", kana: "レ", freq: 391.9954, holes: [false, false, true, true, true] },
    NamedPitch { name: "Chi", kana: "チ", freq: 440.0, holes: [false, false, false, true, true] },
    NamedPitch { name: "Ri", kana: "リ", freq: 523.2511, holes: [false, false, false, false, true] },
    NamedPitch { name: "Hi", kana: "ヒ", freq: 587.3295, holes: [false, false, false, false, false] },
];

/// Chu-meri (otsu): C♯4, E4, F♯4, A♭4, B4, C♯5
pub const MERI_CHU: [f64; 6] = [277.1826, 329.6276, 369.9944, 415.3047, 493.8833, 554.3653];
/// Dai-meri (otsu): C4, E♭4, F♯4, A♭4, B♭4, C5
pub const MERI_DAI: [f64; 6] = [261.6256, 311.127, 369.9944, 415.3047, 466.1638, 523.2511];

pub const NOTE_MAX: usize = FINGERINGS.len() - 1;

pub const DEFAULT_HOLES: Holes = FINGERINGS[0].holes;

const OPEN_WEIGHT: [f64; 5] = [0.159, 0.091, 0.083, 0.106, 0.061];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Voicing {
    Kari,
    Meri,
}

/// Extra flatten when the meri lane is selected (~chu-meri at rest).
pub const MERI_LANE: f64 = 0.7;

pub fn clamp_note(index: i32) -> usize {
    index.clamp(0, NOTE_MAX as i32) as usize
}

pub fn holes_from_index(index: usize) -> Holes {
    FINGERINGS[index.min(NOTE_MAX)].holes
}

pub fn index_from_holes(holes: &Holes) -> usize {
    FINGERINGS.iter().position(|f| f.holes == *holes).unwrap_or(0)
}

pub fn freq_from_holes(holes: &Holes) -> f64 {
    if let Some(f) = FINGERINGS.iter().find(|f| f.holes == *holes) {
        return f.freq;
    }
    let mut open = 0.0;
    for (h, w) in holes.iter().zip(OPEN_WEIGHT) {
        if !h {
            open += w;
        }
    }
    RO_FREQ / (1.0 - open).max(0.48)
}

pub fn played_meri(voicing: Voicing, meri: f64) -> f64 {
    let v = meri.clamp(0.0, 1.0);
    if voicing == Voicing::Meri { (MERI_LANE + v * 0.3).min(1.0) } else { v }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// Depth 0 = kari, 0.5 = chu-meri, 1 = dai-meri.
pub fn meri_depth(voicing: Voicing, meri: f64, lean: f64) -> f64 {
    let hole = lean.clamp(0.0, 1.0);
    let knob = meri.clamp(0.0, 1.0);
    let lane = if voicing == Voicing::Meri { 0.5 } else { 0.0 };
    (lane + hole * (if lane > 0.0 { 0.5 } else { 1.0 }) + knob * 0.22).clamp(0.0, 1.0)
}

pub fn hz_at(index: usize, otsu: bool, depth: f64) -> f64 {
    let i = index.min(NOTE_MAX);
    let kari = FINGERINGS[i].freq;
    let t = depth.clamp(0.0, 1.0);
    let mut hz = kari;
    if t > 0.02 {
        let chu = MERI_CHU[i];
        let dai = MERI_DAI[i];
        hz = if t <= 0.55 { lerp(kari, chu, t / 0.55) } else { lerp(chu, dai, (t - 0.55) / 0.45) };
    }
    hz * if otsu { 1.0 } else { 2.0 }
}

pub fn played_hz(index: usize, otsu: bool, meri: f64, lean: f64, voicing: Voicing) -> f64 {
    hz_at(index, otsu, meri_depth(voicing, meri, lean))
}

#[derive(Clone, Debug, PartialEq)]
pub struct NoteName {
    pub name: String,
    pub kana: String,
    pub meri: bool,
    pub kan: bool,
}

pub fn name_from_holes(holes: &Holes, meri: f64, iki: f64, voicing: Voicing, otsu: bool, lean: f64) -> NoteName {
    let best = &FINGERINGS[index_from_holes(holes)];
    let t = meri_depth(voicing, meri, lean);
    let is_meri = t > 0.22;
    let dai = t > 0.78;
    let kan = !otsu || iki > 0.9;
    NoteName {
        name: if is_meri {
            format!("{} {}", best.name, if dai { "dai-meri" } else { "meri" })
        } else {
            best.name.to_string()
        },
        kana: if is_meri {
            format!("{}{}", best.kana, if dai { "大メ" } else { "メ" })
        } else {
            best.kana.to_string()
        },
        meri: is_meri,
        kan,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kari_pitches() {
        assert!((hz_at(0, true, 0.0) - 293.6648).abs() < 1e-9);
        assert!((hz_at(3, false, 0.0) - 880.0).abs() < 1e-9);
    }

    #[test]
    fn meri_lane_rest_is_chu_meri_ish() {
        // meri lane, knob 0.14, no lean → depth 0.5 + 0.0308
        let d = meri_depth(Voicing::Meri, 0.14, 0.0);
        assert!((d - 0.5308).abs() < 1e-9);
        let hz = hz_at(0, true, d);
        assert!(hz < 293.66 && hz > 277.0);
    }

    #[test]
    fn names() {
        let n = name_from_holes(&holes_from_index(1), 0.14, 0.52, Voicing::Kari, true, 0.0);
        assert_eq!(n.name, "Tsu");
        let n = name_from_holes(&holes_from_index(1), 0.14, 0.52, Voicing::Meri, true, 1.0);
        assert_eq!(n.name, "Tsu dai-meri");
        assert_eq!(n.kana, "ツ大メ");
        assert!(name_from_holes(&holes_from_index(0), 0.0, 0.95, Voicing::Kari, true, 0.0).kan);
    }

    #[test]
    fn holes_roundtrip() {
        for (i, f) in FINGERINGS.iter().enumerate() {
            assert_eq!(index_from_holes(&holes_from_index(i)), i);
            assert_eq!(freq_from_holes(&holes_from_index(i)), f.freq);
        }
    }
}
