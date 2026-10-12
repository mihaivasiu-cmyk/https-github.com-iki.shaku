# Building and testing Iki

## Everyday

```
scripts/check.sh              # fmt + clippy + every test (needs crates.io)
cargo test --release          # just the tests
```

CI (`.github/workflows/build.yml`) runs the same on every push: format, lint
(advisory for now), the core / UI / desktop tests, the offline kernel tests, and
the plugin + Android-bridge tests, then builds the Linux and macOS binaries.

## Offline kernel tests (no network, no audio device)

```
scripts/offline-check.sh      # or: scripts/check.sh --offline
```

`tools/offline-check` compiles the real DSP, engine, looper, Foli drums, sample
loader and MIDI parser against a small stand-in for the `hound` WAV reader, then
runs their unit tests and `crates/iki-core/tests/engine.rs` (end-to-end renders:
pitch of every note, voice switching, flute / bore levels, filter modes, looper).
It exists so the sound kernel can be verified where crates.io or an audio device
isn't available. It does **not** cover the UI, audio I/O, the plugin or Android.

## What the tests pin down

| Behaviour | Test |
|---|---|
| Every note in tune with the sample bank | `every_kari_note_is_in_tune` |
| Flute / Bore / Both switch in any order, always sounding | `voice_mode_switch_keeps_sounding` |
| Flute and Bore level knobs scale only their own voice | `flute_and_bore_level_knobs_set_their_loudness` |
| Filter envelope acts only in Env mode | `filter_envelope_only_acts_in_env_mode` |
| Every filter type changes the sound; Peak / shelves audible at default Res | `every_filter_type_audibly_shapes_the_sound`, `gain_filters_are_audible_at_default_res` |
| Looper records, takes effect immediately, undo / dub | `looper_*`, `dub_and_undo` |
| Every Foli drum preset plays and walks through its bars; muted steps stay silent | `every_preset_plays_and_walks_through_its_loop`, `switched_off_steps_mute_their_slices` |

## Not yet automated

* UI clicks (every button / segmented control responds) — the next thing to add
  is an egui `kittest`-style test that presses each control.
* Real audio output and latency on each platform.
* Plugin validation (`pluginval`, CLAP validator).

## Sample importer

`python3 tools/sample-import/test_import.py` checks the pack importer on synthetic
recordings (needs ffmpeg and numpy). See `tools/sample-import/import_samples.py` for use.

## Sample bank

`tools/sample-import/build_sp_bank.py PACK_DIR crates/iki-core/assets/samples` rebuilds the
18-note flute bank from the Samplephonics pack (see `crates/iki-core/assets/samples/NOTICE.txt`
for the licence caveat). The engine tests check every note is in tune across the whole meri range.
