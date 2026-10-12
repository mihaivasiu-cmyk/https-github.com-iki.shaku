//! Port of `src/lib/shaku/delay.ts` — tempo, delay-time and LFO-rate maths.

pub const DELAY_MAX: f64 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DelayDiv {
    pub id: &'static str,
    pub beats: f64,
}

pub const DELAY_DIVS: [DelayDiv; 9] = [
    DelayDiv { id: "1/16", beats: 0.25 },
    DelayDiv { id: "1/8", beats: 0.5 },
    DelayDiv { id: "1/4", beats: 1.0 },
    DelayDiv { id: "1/2", beats: 2.0 },
    DelayDiv { id: "1", beats: 4.0 },
    DelayDiv { id: "2", beats: 8.0 },
    DelayDiv { id: "4", beats: 16.0 },
    DelayDiv { id: "6", beats: 24.0 },
    DelayDiv { id: "8", beats: 32.0 },
];

/// Index into [`DELAY_DIVS`]; the default is "1/4".
pub const DIV_QUARTER: usize = 2;

pub fn tempo_bpm(knob: f64) -> f64 {
    (48.0 + knob.clamp(0.0, 1.0) * 96.0).round()
}

pub fn free_delay_sec(knob: f64) -> f64 {
    let v = knob.clamp(0.0, 1.0);
    0.03 + v * v * 1.97
}

pub fn synced_delay_sec(div: usize, bpm: f64) -> f64 {
    let d = DELAY_DIVS.get(div).unwrap_or(&DELAY_DIVS[DIV_QUARTER]);
    let sec = (60.0 / bpm.max(40.0)) * d.beats;
    sec.clamp(0.02, DELAY_MAX)
}

pub fn delay_sec(sync: bool, time: f64, div: usize, tempo: f64) -> f64 {
    delay_sec_bpm(sync, time, div, tempo_bpm(tempo))
}

/// [`delay_sec`] at an exact tempo (a plugin host's) instead of the clock knob.
pub fn delay_sec_bpm(sync: bool, time: f64, div: usize, bpm: f64) -> f64 {
    if sync { synced_delay_sec(div, bpm) } else { free_delay_sec(time).min(DELAY_MAX) }
}

pub fn synced_lfo_hz(div: usize, bpm: f64) -> f64 {
    1.0 / synced_delay_sec(div, bpm).max(0.05)
}

pub fn lfo_hz(sync: bool, rate: f64, div: usize, tempo: f64) -> f64 {
    lfo_hz_bpm(sync, rate, div, tempo_bpm(tempo))
}

/// [`lfo_hz`] at an exact tempo.
pub fn lfo_hz_bpm(sync: bool, rate: f64, div: usize, bpm: f64) -> f64 {
    if sync { synced_lfo_hz(div, bpm) } else { log_hz(rate, 0.05, 12.0) }
}

pub fn log_hz(knob: f64, min: f64, max: f64) -> f64 {
    let v = knob.clamp(0.0, 1.0);
    min * (max / min).powf(v)
}

pub fn format_hz(hz: f64) -> String {
    if hz < 1.0 { format!("{hz:.3} Hz") } else { format!("{hz:.2} Hz") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        assert_eq!(tempo_bpm(0.25), 72.0);
        assert!((delay_sec(true, 0.32, DIV_QUARTER, 0.25) - 60.0 / 72.0).abs() < 1e-12);
        assert!((free_delay_sec(1.0) - 2.0).abs() < 1e-12);
        assert_eq!(synced_delay_sec(8, 48.0), DELAY_MAX);
        assert_eq!(format_hz(0.5), "0.500 Hz");
    }
}
