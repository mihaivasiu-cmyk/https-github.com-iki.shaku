# Changelog

## Unreleased — "final touches" pass

**Design**
- Breath (day) / Temple (night) are kept. Pads (outlined squares, dashed in the meri lane,
  serif kana), the rectangular BLOW / GATE button and the dark ink-stone knobs are new on
  every front end.

**Effects and desktop**
- Presets button removed from the desktop header (last session is still restored).
- Delay: new Tone and Low cut. Reverb: Room presets (Room / Hall / Plate / Slap) and
  Size, Pre-delay and Damp knobs. Also in the plugin and the iPhone Sound tab.
- Filter cutoff range widened (low-pass 60 Hz–16 kHz); default cutoff value remapped so
  the sound is unchanged. Saved patches made before this will sound brighter.
- Foli: the synthesised "Syn" kit is removed (seven sampled kits remain).
- Desktop loops record up to 120 s per track; iOS/Android keep 24 s.

**iOS**
- New iPhone version (iPhone 7 / iOS 15+): `crates/iki-ios` C ABI + SwiftUI app in `ios/`, built with `scripts/build-ios.sh`. See IOS.md. Untested on a device.

**Sound**
- Flute bank rebuilt from a studio shakuhachi pack: 36 looped notes — the six
  notes as kari, chu-meri and dai-meri recordings, each in a soft and a hard take.
  Meri moves between *recorded* flat notes instead of pitch-shifting one; breath
  picks the soft or hard take for each new note.
- Real pitch bend (±1…12 semitones), yuri vibrato (depth, rate, onset delay),
  glide between notes, and a tuning knob (A 415…466 Hz).
- Master limiter (ceiling −0.4 dBFS) with limiter and loudness meters.
- Flute, Bore and Both voice modes switch cleanly; separate Flute and Bore levels.
- Foli: seven loop presets sliced across the 12-bar grid, tempo-matched.

**Design**
- One set of design tokens in the core (`shaku/design.rs`): palettes, motion timings
  (50–150 ms) and easing feed the desktop and plugin theme, and generate Android's
  `Palette.kt` (a test fails if they drift).
- Knobs: soft shadow and highlight, a lit arc that glides to the value, the number
  pops up while held, Shift/Ctrl/Cmd for fine adjust, double-click resets (desktop).
- Switches, buttons, LEDs and rockers fade and slide instead of snapping; segmented
  rows slide their highlight.
- The flute is alive: holes close in 50 ms, glow and wisps follow the breath, wisps
  lean with the bend, the blowing end darkens with meri.
- Panels carry a faint bamboo grain.

**Playing**
- MIDI: pitch wheel → bend, CC1 → yuri, CC2/7/11 and aftertouch → iki, CC74 → meri,
  CC64 sustain, CC20–23 loop punch.
- Looper transport (record / play / stop) is instant on all four tracks; Export WAV
  writes each track and a mixdown.
- All filter, envelope and mixer buttons fixed (Peak and shelf filters were silent at
  the default resonance; FX / Freeze / Stop-all did not fire).

**Workflow**
- Presets: five factory patches, save/load your own, last session restored at start.
- Audio-thread safety: a test fails the build if the render path allocates (this
  found, and fixed, allocations when committing or overdubbing a loop).
- `scripts/offline-check.sh` runs the sound-kernel suite with no network or audio device.
- `tools/sample-import/` builds sample banks from your own recordings.

**Known gaps** — see MANUAL.md → "What is not finished".
