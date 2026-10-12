//! Port of `src/lib/shaku/midi.ts`: note ↔ fingering mapping, incoming message
//! decoding (notes, pitch wheel → bend, CC → yuri / iki / meri / loop punch, sustain), a
//! Standard MIDI File parser and a clip player.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShakuHit {
    pub index: usize,
    pub otsu: bool,
}

/// 1.8 shakuhachi: otsu D4, kan D5.
const PITCHES: [(u8, usize, bool); 26] = [
    (60, 0, true),
    (61, 0, true),
    (62, 0, true),
    (63, 1, true),
    (64, 1, true),
    (65, 1, true),
    (66, 2, true),
    (67, 2, true),
    (68, 3, true),
    (69, 3, true),
    (70, 4, true),
    (71, 4, true),
    (72, 4, true),
    (73, 5, true),
    (74, 0, false),
    (75, 5, true),
    (76, 1, false),
    (77, 1, false),
    (78, 2, false),
    (79, 2, false),
    (80, 3, false),
    (81, 3, false),
    (82, 4, false),
    (83, 4, false),
    (84, 4, false),
    (86, 5, false),
];

/// The bundled demo clip (Ro → Tsu → Re → Chi → Ri → Ro kan).
pub const PHRASE_MID: &[u8] = include_bytes!("../../assets/samples/phrase.mid");

pub fn shaku_to_midi(index: usize, otsu: bool) -> u8 {
    const OTSU: [u8; 6] = [62, 65, 67, 69, 72, 74];
    const KAN: [u8; 6] = [74, 77, 79, 81, 84, 86];
    let i = index.min(5);
    if otsu { OTSU[i] } else { KAN[i] }
}

pub fn midi_to_shaku(note: u8) -> ShakuHit {
    let mut best = PITCHES[0];
    let mut dist = 99i32;
    for p in PITCHES {
        let d = (p.0 as i32 - note as i32).abs();
        if d < dist {
            dist = d;
            best = p;
        }
    }
    ShakuHit { index: best.1, otsu: best.2 }
}

pub fn vel_to_iki(vel: u8) -> f64 {
    0.28 + (vel.clamp(1, 127) as f64 / 127.0) * 0.72
}

pub fn bend_to_meri(value14: u16) -> f64 {
    let n = (value14 as f64 - 8192.0) / 8192.0;
    if n >= 0.0 { 0.14 * (1.0 - n) } else { 0.14 + -n * 0.82 }
}

/// What the instrument should do in response to MIDI (the web `MidiLive` callbacks).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LiveEvent {
    NoteOn {
        midi: u8,
        vel: u8,
    },
    NoteOff {
        midi: u8,
    },
    Meri(f64),
    Iki(f64),
    /// Pitch wheel, -1…1.
    Bend(f64),
    /// Yuri (vibrato) depth, 0…1.
    Yuri(f64),
    Punch(usize),
}

#[derive(Default, Debug)]
pub struct Sustain {
    pub on: bool,
    pub held: Vec<u8>,
}

/// `handleBytes`: decode one message into instrument events.
pub fn handle_bytes(data: &[u8], sustain: &mut Sustain, out: &mut Vec<LiveEvent>) {
    let Some(&st) = data.first() else { return };
    let cmd = st & 0xf0;
    let a = data.get(1).copied().unwrap_or(0);
    let b = data.get(2).copied().unwrap_or(0);
    match cmd {
        0x90 => {
            if b == 0 {
                if sustain.on {
                    sustain.held.push(a);
                } else {
                    out.push(LiveEvent::NoteOff { midi: a });
                }
            } else {
                out.push(LiveEvent::NoteOn { midi: a, vel: b });
            }
        }
        0x80 => {
            if sustain.on {
                sustain.held.push(a);
            } else {
                out.push(LiveEvent::NoteOff { midi: a });
            }
        }
        0xe0 => out.push(LiveEvent::Bend((((b as i32) << 7 | a as i32) - 8192) as f64 / 8192.0)),
        0xd0 => out.push(LiveEvent::Iki(a as f64 / 127.0)),
        0xb0 => {
            match a {
                1 => out.push(LiveEvent::Yuri(b as f64 / 127.0)),
                2 | 7 | 11 => out.push(LiveEvent::Iki(b as f64 / 127.0)),
                74 => out.push(LiveEvent::Meri(b as f64 / 127.0)),
                _ => {}
            }
            if a == 64 {
                sustain.on = b >= 64;
                if !sustain.on {
                    for n in sustain.held.drain(..) {
                        out.push(LiveEvent::NoteOff { midi: n });
                    }
                }
            }
            if (20..=23).contains(&a) {
                out.push(LiveEvent::Punch((a - 20) as usize));
            }
        }
        _ => {}
    }
}

pub fn note_on_bytes(note: u8, vel: u8) -> [u8; 3] {
    [0x90, note & 127, vel.clamp(1, 127)]
}

pub fn note_off_bytes(note: u8) -> [u8; 3] {
    [0x80, note & 127, 0]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipEvent {
    pub time: f64,
    pub midi: u8,
    pub vel: u8,
    pub on: bool,
}

#[derive(Clone, Debug)]
pub struct MidiClip {
    pub name: String,
    pub duration: f64,
    pub events: Vec<ClipEvent>,
}

fn u32be(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

fn u16be(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(i..i + 2)?.try_into().ok()?))
}

fn vlq(b: &[u8], p: &mut usize) -> Option<u32> {
    let mut v = 0u32;
    for _ in 0..4 {
        let x = *b.get(*p)?;
        *p += 1;
        v = (v << 7) | (x & 0x7f) as u32;
        if x & 0x80 == 0 {
            return Some(v);
        }
    }
    Some(v)
}

/// `parseSmf`: note events of every track, tempo-mapped to seconds.
pub fn parse_smf(b: &[u8], name: &str) -> Result<MidiClip, String> {
    if b.len() < 14 || &b[0..4] != b"MThd" {
        return Err("Not a MIDI file".into());
    }
    let bad = || "Truncated MIDI file".to_string();
    let ntrks = u16be(b, 10).ok_or_else(bad)?;
    let division = u16be(b, 12).ok_or_else(bad)?;
    let tpq = if division & 0x8000 != 0 { 96.0 } else { division.max(1) as f64 };
    let mut i = 8 + u32be(b, 4).ok_or_else(bad)? as usize;
    struct Raw {
        tick: u64,
        midi: u8,
        vel: u8,
        on: bool,
    }
    let mut raw: Vec<Raw> = Vec::new();
    let mut tempos: Vec<(u64, f64)> = vec![(0, 500_000.0)];

    for _ in 0..ntrks {
        if i + 8 > b.len() || &b[i..i + 4] != b"MTrk" {
            break;
        }
        let len = u32be(b, i + 4).ok_or_else(bad)? as usize;
        let end = (i + 8 + len).min(b.len());
        let mut p = i + 8;
        let mut tick = 0u64;
        let mut run = 0u8;
        while p < end {
            tick += vlq(b, &mut p).ok_or_else(bad)? as u64;
            let Some(&first) = b.get(p) else { break };
            let st = if first < 0x80 {
                run
            } else {
                p += 1;
                run = first;
                first
            };
            if st == 0xff {
                let ty = *b.get(p).ok_or_else(bad)?;
                p += 1;
                let n = vlq(b, &mut p).ok_or_else(bad)? as usize;
                if ty == 0x51 && n == 3 && p + 3 <= b.len() {
                    let us = ((b[p] as u32) << 16) | ((b[p + 1] as u32) << 8) | b[p + 2] as u32;
                    tempos.push((tick, us as f64));
                }
                p += n;
                continue;
            }
            if st == 0xf0 || st == 0xf7 {
                let n = vlq(b, &mut p).ok_or_else(bad)? as usize;
                p += n;
                continue;
            }
            let cmd = st & 0xf0;
            if cmd == 0xc0 || cmd == 0xd0 {
                p += 1;
                continue;
            }
            let a = *b.get(p).ok_or_else(bad)?;
            let c = *b.get(p + 1).ok_or_else(bad)?;
            p += 2;
            if cmd == 0x90 {
                raw.push(Raw { tick, midi: a, vel: c, on: c > 0 });
            } else if cmd == 0x80 {
                raw.push(Raw { tick, midi: a, vel: 0, on: false });
            }
        }
        i = end;
    }

    tempos.sort_by_key(|t| t.0);
    let tick_to_sec = |tick: u64| {
        let (mut sec, mut last, mut us) = (0.0, 0u64, tempos[0].1);
        for &(tt, tus) in &tempos {
            if tt > tick {
                break;
            }
            sec += ((tt - last) as f64 * us) / tpq / 1e6;
            last = tt;
            us = tus;
        }
        sec + ((tick - last) as f64 * us) / tpq / 1e6
    };
    let mut events: Vec<ClipEvent> =
        raw.iter().map(|e| ClipEvent { time: tick_to_sec(e.tick), midi: e.midi, vel: e.vel, on: e.on }).collect();
    // Stable sort: by time, note-offs before note-ons at the same instant.
    events.sort_by(|a, c| a.time.partial_cmp(&c.time).unwrap_or(std::cmp::Ordering::Equal).then(a.on.cmp(&c.on)));
    let duration = events.iter().fold(0f64, |m, e| m.max(e.time));
    Ok(MidiClip { name: name.to_string(), duration, events })
}

/// Clip playback on its own thread (the web version used one `setTimeout` per event).
/// `emit(live_event, tx_bytes)` is called for each note as it becomes due.
pub struct ClipPlayer {
    stop: Arc<AtomicBool>,
}

impl ClipPlayer {
    pub fn play<F>(clip: MidiClip, mut emit: F) -> Self
    where
        F: FnMut(LiveEvent, Option<[u8; 3]>) + Send + 'static,
    {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        std::thread::Builder::new()
            .name("iki-clip".into())
            .spawn(move || {
                let t0 = Instant::now();
                let mut held: Vec<u8> = Vec::new();
                for ev in clip.events {
                    let due = t0 + Duration::from_secs_f64(ev.time.max(0.0));
                    loop {
                        if flag.load(Ordering::Relaxed) {
                            return;
                        }
                        let now = Instant::now();
                        if now >= due {
                            break;
                        }
                        std::thread::sleep((due - now).min(Duration::from_millis(5)));
                    }
                    if ev.on {
                        held.push(ev.midi);
                        emit(LiveEvent::NoteOn { midi: ev.midi, vel: ev.vel }, Some(note_on_bytes(ev.midi, ev.vel)));
                        continue;
                    }
                    if let Some(i) = held.iter().rposition(|&m| m == ev.midi) {
                        held.remove(i);
                    }
                    if held.is_empty() {
                        emit(LiveEvent::NoteOff { midi: ev.midi }, Some(note_off_bytes(ev.midi)));
                    }
                }
            })
            .ok();
        Self { stop }
    }
}

impl Drop for ClipPlayer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping() {
        assert_eq!(midi_to_shaku(62), ShakuHit { index: 0, otsu: true });
        assert_eq!(midi_to_shaku(74), ShakuHit { index: 0, otsu: false });
        assert_eq!(midi_to_shaku(85), ShakuHit { index: 4, otsu: false });
        assert_eq!(midi_to_shaku(20), ShakuHit { index: 0, otsu: true });
        for i in 0..5 {
            assert_eq!(midi_to_shaku(shaku_to_midi(i, true)).index, i);
        }
        // Same as the web table: otsu Hi and kan Ro are both D5 (74) and read back as Ro kan.
        assert_eq!(midi_to_shaku(shaku_to_midi(5, true)), ShakuHit { index: 0, otsu: false });
        let mut s = Sustain::default();
        let mut o = Vec::new();
        handle_bytes(&[0xe0, 0, 64], &mut s, &mut o);
        handle_bytes(&[0xe0, 127, 127], &mut s, &mut o);
        handle_bytes(&[0xe0, 0, 0], &mut s, &mut o);
        handle_bytes(&[0xb0, 1, 127], &mut s, &mut o);
        handle_bytes(&[0xb0, 74, 127], &mut s, &mut o);
        handle_bytes(&[0xd0, 127, 0], &mut s, &mut o);
        assert_eq!(
            o,
            vec![
                LiveEvent::Bend(0.0),
                LiveEvent::Bend(8191.0 / 8192.0),
                LiveEvent::Bend(-1.0),
                LiveEvent::Yuri(1.0),
                LiveEvent::Meri(1.0),
                LiveEvent::Iki(1.0),
            ]
        );
        assert!((bend_to_meri(8192) - 0.14).abs() < 1e-12);
        assert!((bend_to_meri(0) - 0.96).abs() < 1e-12);
        assert!((vel_to_iki(127) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn sustain_holds_note_offs() {
        let mut s = Sustain::default();
        let mut out = Vec::new();
        handle_bytes(&[0xb0, 64, 127], &mut s, &mut out);
        handle_bytes(&[0x80, 62, 0], &mut s, &mut out);
        assert!(out.is_empty());
        handle_bytes(&[0xb0, 64, 0], &mut s, &mut out);
        assert_eq!(out, vec![LiveEvent::NoteOff { midi: 62 }]);
        out.clear();
        handle_bytes(&[0xb1, 21, 100], &mut s, &mut out);
        assert_eq!(out, vec![LiveEvent::Punch(1)]);
    }

    #[test]
    fn parses_bundled_phrase() {
        let clip = parse_smf(include_bytes!("../../assets/samples/phrase.mid"), "phrase").unwrap();
        assert!(!clip.events.is_empty());
        assert!(clip.duration > 0.0);
        assert!(clip.events.windows(2).all(|w| w[0].time <= w[1].time));
    }
}
