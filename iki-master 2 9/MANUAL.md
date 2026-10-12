# Iki — manual

## Playing

| Key | Action |
| --- | --- |
| Space | toggle blow mode (latched drone) |
| A S D F G H · Z X C V B N | hold to blow kari / meri Ro … Hi; tap while blowing = atari, tap twice = koro |
| ↑ → / ↓ ← | next / previous note in the current lane |
| O | otsu ↔ kan (octave) |
| M | muraiki (breath burst) |
| 1 – 4 | select loop track |
| R / T / U | punch (rec → play → dub) · stop · undo on the selected track |
| mouse | drag knobs vertically or scroll (hold Shift or Ctrl/Cmd for fine adjust); double-click resets; drop a .mid file to play it |

## Voice

* **Flute / Bore / Both** — sampled flute, the waveguide bore model, or both.
  Each has its own level knob.
* **Meri** flattens the note through recorded flat notes; **Iki** is breath
  (loud, bright, and it picks the soft or hard recording when a note starts).
* **Bend** strip (springs back), **Range** (±1…12 semitones), **Yuri** vibrato
  (depth, **Rate**), **Glide** between notes, **Tune** (A 415…466 Hz).

## MIDI

Notes play (velocity → iki). Pitch wheel → bend · CC1 → yuri · CC2, 7, 11 and
channel aftertouch → iki · CC74 → meri · CC64 sustain · CC20–23 punch loop track 1–4.

## Sound patches and effects

The desktop header no longer has a Presets button. The last patch is still saved
when you quit and restored on start (`…/iki/last.ikipreset`); patch files are
small text files and can still be loaded by the plugin and mobile apps.

**Delay:** Time, Feedback, Mix, plus **Tone** (how dark the repeats get, 1.2–12 kHz)
and **Low cut** (40–800 Hz).
**Space (reverb):** pick a **Room** — Room, Hall, Plate or Slap — or shape it
yourself with **Decay**, **Size** (×0.5–×2), **Pre** (0–120 ms) and **Damp**
(9 kHz down to 800 Hz). Moving any of those knobs switches the room to Custom.
**Filter:** the cutoff now spans 60 Hz–16 kHz for the low-pass (the other filter
types are wider too); the default sound is unchanged.

## Looper

Four tracks; the first sets the loop length (snapped to bars). The desktop app records up to 120 s per track (the phone apps keep 24 s). Record, play,
stop and undo are instant. **Freeze 4** bakes tracks 1–3 into the pad,
**Export WAV** writes every recorded track plus a mixdown to `…/iki/exports/`.

## Foli

Seven drum-loop presets (90–160 BPM) sliced across the 12-step grid and
resampled to follow the clock, plus the built-in synth kit.

## What is not finished

* The new look (knob arcs, sliding switches, living flute, bamboo grain) was written
  without being able to run egui here: expect to tune sizes and colours once you see it.
  Android shares the palette and timings but not yet the animations. There has been no
  accessibility pass (touch targets, screen-reader labels, colour-blind-safe palette,
  keyboard navigation) and no calligraphic typeface.
* No MIDI-learn, undo/redo, scale/tuning tables other than the six shakuhachi
  notes, per-track pitch/reverse/half-speed in the looper, or convolution reverb.
* Foli presets are tempo-matched by resampling, so their pitch moves with the clock.
* Kan (high octave) plays the recorded notes at double speed.
* Installers, code signing and auto-update are not set up (see RELEASE.md).
