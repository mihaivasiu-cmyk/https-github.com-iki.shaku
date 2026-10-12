#!/usr/bin/env python3
"""Turn recordings of a shakuhachi into an Iki sample pack.

    import_samples.py INPUT_DIR OUT_DIR [--bank-dir DIR]

* Reads every audio file in INPUT_DIR (anything ffmpeg can decode).
* Splits each into notes, measures each note's pitch and loudness.
* The articulation comes from the file name: kari, meri, muraiki, atari,
  koro, yuri, tongue (anything else is "sustain"). Name your files like
  `meri_ro.wav` or `Muraiki 03.wav`; a note name in the file name (ro tsu re
  chi ri hi) overrides the detected one.
* Writes OUT_DIR/<articulation>_<note>_v<layer>.wav (mono, 16-bit, 44.1 kHz)
  and OUT_DIR/manifest.json with pitch (Hz, cents off equal temperament),
  loudness, duration and velocity layer (1 soft … 3 hard) per note.
* With --bank-dir it also writes the six sustained notes the engine plays
  today (ro.wav … hi.wav) and prints the SAMPLE_BANK lines to paste into
  crates/iki-core/src/shaku/samples.rs.

Only import recordings you own or are licensed to redistribute.
"""
import argparse, json, math, re, subprocess, sys, wave
from pathlib import Path

import numpy as np

SR = 44100
NOTES = {"ro": 293.66, "tsu": 349.23, "re": 392.0, "chi": 440.0, "ri": 523.25, "hi": 587.33}
ARTICS = ["muraiki", "atari", "koro", "yuri", "tongue", "meri", "kari"]


def decode(path):
    out = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", str(path), "-ac", "1", "-ar", str(SR), "-f", "f32le", "-"],
        capture_output=True,
    )
    if out.returncode != 0:
        raise RuntimeError(out.stderr.decode(errors="replace"))
    return np.frombuffer(out.stdout, dtype="<f4").astype(np.float64)


def envelope(x, hop=441):
    n = len(x) // hop
    return np.sqrt((x[: n * hop].reshape(n, hop) ** 2).mean(axis=1) + 1e-12)


def segments(x, min_len=0.25, gap=0.12):
    """Return (start, end) sample ranges of sounded passages."""
    hop = 441
    env = envelope(x, hop)
    if len(env) == 0:
        return []
    thr = max(np.percentile(env, 95) * 0.2, 1e-4)
    on = env > thr
    segs, i = [], 0
    while i < len(on):
        if on[i]:
            j = i
            while j < len(on) and on[j]:
                j += 1
            segs.append([i, j])
            i = j
        else:
            i += 1
    merged = []
    for s in segs:
        if merged and (s[0] - merged[-1][1]) * hop / SR < gap:
            merged[-1][1] = s[1]
        else:
            merged.append(s)
    return [(a * hop, b * hop) for a, b in merged if (b - a) * hop / SR >= min_len]


def yin_pitch(x, lo=200.0, hi=1500.0):
    """Median pitch (Hz) and confidence over the steady middle of a note."""
    n = len(x)
    a, b = int(n * 0.3), int(n * 0.8)
    win = 2048
    f0s, conf = [], []
    tau_min, tau_max = int(SR / hi), int(SR / lo)
    for s in range(a, max(a + 1, b - win), win // 2):
        f = x[s : s + win + tau_max]
        if len(f) < win + tau_max:
            break
        d = np.array([np.sum((f[:win] - f[t : t + win]) ** 2) for t in range(tau_max + 1)])
        cm = np.cumsum(d[1:])
        dn = np.ones_like(d)
        dn[1:] = d[1:] * np.arange(1, len(d)) / np.maximum(cm, 1e-12)
        t = None
        for k in range(tau_min, tau_max):
            if dn[k] < 0.15 and dn[k] <= dn[k + 1]:
                t = k
                break
        if t is None:
            t = int(np.argmin(dn[tau_min:tau_max])) + tau_min
        if 1 <= t < tau_max:
            y0, y1, y2 = dn[t - 1], dn[t], dn[t + 1]
            den = y0 - 2 * y1 + y2
            t = t + (0.5 * (y0 - y2) / den if abs(den) > 1e-12 else 0.0)
        f0s.append(SR / t)
        conf.append(1.0 - min(1.0, dn[int(round(t))] if int(round(t)) < len(dn) else 1.0))
    if not f0s:
        return 0.0, 0.0
    return float(np.median(f0s)), float(np.median(conf))


def nearest_note(hz):
    best = min(NOTES, key=lambda k: abs(1200 * math.log2(hz / NOTES[k])) if hz > 0 else 1e9)
    cents = 1200 * math.log2(hz / NOTES[best])
    return best, cents


def articulation(stem):
    s = stem.lower()
    for a in ARTICS:
        if a in s:
            return a
    return "sustain"


def note_from_name(stem):
    for n in NOTES:
        if re.search(rf"(^|[^a-z]){n}([^a-z]|$)", stem.lower()):
            return n
    return None


def fade(x, ms=8):
    n = min(int(SR * ms / 1000), len(x) // 4)
    y = x.copy()
    if n:
        ramp = np.linspace(0, 1, n)
        y[:n] *= ramp
        y[-n:] *= ramp[::-1]
    return y


def write_wav(path, x):
    pcm = (np.clip(x, -1, 1) * 32767).astype("<i2")
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(pcm.tobytes())


def process(indir, outdir):
    outdir.mkdir(parents=True, exist_ok=True)
    entries = []
    files = sorted(p for p in Path(indir).iterdir() if p.is_file() and p.suffix.lower() in
                   {".wav", ".mp3", ".flac", ".ogg", ".aif", ".aiff", ".m4a"})
    for f in files:
        try:
            x = decode(f)
        except Exception as e:  # keep going; report at the end
            print(f"skip {f.name}: {e}", file=sys.stderr)
            continue
        art = articulation(f.stem)
        forced = note_from_name(f.stem)
        for k, (a, b) in enumerate(segments(x)):
            seg = x[a:b]
            hz, conf = yin_pitch(seg)
            if hz <= 0 and not forced:
                continue
            if forced:
                note, cents = forced, (1200 * math.log2(hz / NOTES[forced]) if hz > 0 else 0.0)
            else:
                note, cents = nearest_note(hz)
            rms_db = 20 * math.log10(float(np.sqrt(np.mean(seg**2))) + 1e-9)
            entries.append(dict(src=f.name, index=k, art=art, note=note, hz=round(hz, 2), cents=round(cents, 1),
                                conf=round(conf, 3), rms_db=round(rms_db, 1), dur=round(len(seg) / SR, 3), _x=seg))
    # velocity layers: loudness terciles within each (articulation, note)
    groups = {}
    for e in entries:
        groups.setdefault((e["art"], e["note"]), []).append(e)
    for g in groups.values():
        g.sort(key=lambda e: e["rms_db"])
        for i, e in enumerate(g):
            e["layer"] = 1 + min(2, 3 * i // len(g)) if len(g) >= 3 else (3 if i == len(g) - 1 else 1) if len(g) > 1 else 2
    manifest = []
    for e in entries:
        x = e.pop("_x")
        peak = float(np.max(np.abs(x))) or 1.0
        name = f"{e['art']}_{e['note']}_v{e['layer']}_{e['src'].rsplit('.', 1)[0][:20]}_{e['index']}.wav".replace(" ", "_")
        write_wav(outdir / name, fade(x * (0.7 / peak)))
        e["file"] = name
        manifest.append(e)
        e["_x"] = x
    for e in manifest:
        e.pop("_x", None)
    (outdir / "manifest.json").write_text(json.dumps(manifest, indent=2))
    return manifest, entries


def bank(manifest, outdir, bankdir):
    bankdir.mkdir(parents=True, exist_ok=True)
    lines = []
    for n in NOTES:
        cands = [e for e in manifest if e["note"] == n and e["art"] in ("kari", "sustain") and e["dur"] >= 1.0
                 and e["conf"] > 0.6]
        if not cands:
            print(f"no sustained {n} found; keep the old sample", file=sys.stderr)
            continue
        cands.sort(key=lambda e: (-e["conf"], abs(e["layer"] - 2)))
        best = cands[0]
        data = (outdir / best["file"]).read_bytes()
        (bankdir / f"{n}.wav").write_bytes(data)
        lines.append(f'    SampleDef {{ name: "{n}", freq: {best["hz"]}, wav: include_bytes!("../../assets/samples/{n}.wav") }},')
    print("\n".join(lines))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("input")
    ap.add_argument("out")
    ap.add_argument("--bank-dir")
    a = ap.parse_args()
    manifest, _ = process(a.input, Path(a.out))
    print(f"{len(manifest)} notes → {a.out}/manifest.json", file=sys.stderr)
    if a.bank_dir:
        bank(manifest, Path(a.out), Path(a.bank_dir))


if __name__ == "__main__":
    main()
