#!/usr/bin/env python3
"""Build Iki's flute bank (18 looped notes) from the Samplephonics "Shakuhachi" pack.

    build_sp_bank.py PACK_DIR OUT_DIR [--takes 2,4]

The pack has every semitone, so the six notes come as real kari recordings and
the two meri depths come as the real flattened notes (no pitch-shifting):

    kari   D3 F3 G3 A3 C4 D4          (sounding Ro Tsu Re Chi Ri Hi)
    chu    C#3 E3 F#3 G#3 B3 C#4
    dai    C3 D#3 F#3 G#3 A#3 C4      (Re and Chi have no deeper step: chu is reused)

Index in the bank = dynamic * 18 + layer * 6 + note (files `ro.wav`, `ro_chu.wav`, `ro_d1.wav` …).
Each is cut to its first 4 s, levelled to the same sustain loudness, and
written as mono 16-bit 44.1 kHz, plus `bank.txt` with the measured pitch to
paste into crates/iki-core/src/shaku/samples.rs.
The pack's licence forbids redistributing the samples in their original form:
read its Terms and Conditions before shipping a build that embeds them.
"""
import argparse, math, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
import numpy as np
import import_samples as I

NAMES = ["ro", "tsu", "re", "chi", "ri", "hi"]
KARI = ["D3", "F3", "G3", "A3", "C4", "D4"]
CHU = ["C#3", "E3", "F#3", "G#3", "B3", "C#4"]
DAI = ["C3", "D#3", "F#3", "G#3", "A#3", "C4"]
CUT = 4.0
TARGET_RMS = 0.11


def take_audio(pack, key, take):
    f = Path(pack) / "Samples" / "Sustained" / f"{key}_ShakuhaciLong_{take:02d}_SP.wav"
    return I.decode(f)[: int(CUT * I.SR)]


def build(pack, out, takes, ref_take=3):
    """`takes` = recorded dynamics to write, soft first. Levels keep the recorded
    spread: every take of a note gets the gain that sets the reference take to TARGET_RMS."""
    out.mkdir(parents=True, exist_ok=True)
    lines = []
    for dyn, take in enumerate(takes):
      for layer, (tag, keys) in enumerate((("", KARI), ("_chu", CHU), ("_dai", DAI))):
        for n, key in zip(NAMES, keys):
            x = take_audio(pack, key, take)
            hz, conf = I.yin_pitch(x)
            ref = take_audio(pack, key, ref_take)[int(0.5 * I.SR) : int(3.5 * I.SR)]
            g = TARGET_RMS / (float(np.sqrt(np.mean(ref**2))) + 1e-9)
            y = x * g
            peak = float(np.max(np.abs(y)))
            if peak > 0.92:
                y *= 0.92 / peak
            n_fade = int(0.01 * I.SR)
            y[-n_fade:] *= np.linspace(1, 0, n_fade)
            name = f"{n}{tag}" + ("" if dyn == 0 else f"_d{dyn}")
            I.write_wav(out / f"{name}.wav", y)
            lines.append((name, round(hz, 2), round(conf, 2), key, round(g, 2)))
            print(f"{name:11} {key:4} {hz:7.2f} Hz conf {conf:.2f} gain x{g:.1f}")
    (out / "bank.txt").write_text(
        "\n".join(f'    SampleDef {{ name: "{n}", freq: {hz}, wav: include_bytes!("../../assets/samples/{n}.wav") }},' for n, hz, *_ in lines) + "\n"
    )


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pack")
    ap.add_argument("out")
    ap.add_argument("--takes", default="2,4", help="recorded dynamics to use, soft first (1 soft … 4 hard)")
    a = ap.parse_args()
    build(a.pack, Path(a.out), [int(t) for t in a.takes.split(",")])
