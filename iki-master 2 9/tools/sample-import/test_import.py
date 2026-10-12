import json, subprocess, sys, tempfile, unittest, wave
from pathlib import Path
import numpy as np

HERE = Path(__file__).parent
SR = 44100


def tone(hz, secs, amp):
    t = np.arange(int(SR * secs)) / SR
    env = np.minimum(1, np.minimum(t / 0.05, (secs - t) / 0.08))
    x = sum(a * np.sin(2 * np.pi * hz * k * t) for k, a in ((1, 1), (2, 0.3), (3, 0.1)))
    x += 0.01 * np.random.default_rng(1).standard_normal(len(t))
    return amp * env * x / 1.4


def save(path, x):
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(SR)
        w.writeframes((np.clip(x, -1, 1) * 32767).astype("<i2").tobytes())


class Importer(unittest.TestCase):
    def test_splits_notes_and_measures_pitch(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d); (d / "in").mkdir()
            gap = np.zeros(int(SR * 0.4))
            seq = [tone(293.66, 1.5, 0.3), gap, tone(392.0, 1.5, 0.5), gap, tone(523.25, 1.5, 0.8), gap]
            save(d / "in" / "kari_run.wav", np.concatenate(seq))
            save(d / "in" / "meri_ro.wav", tone(277.0, 1.5, 0.4))
            r = subprocess.run([sys.executable, str(HERE / "import_samples.py"), str(d / "in"), str(d / "out"),
                                "--bank-dir", str(d / "bank")], capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)
            m = json.loads((d / "out" / "manifest.json").read_text())
            kari = sorted([e for e in m if e["art"] == "kari"], key=lambda e: e["index"])
            self.assertEqual([e["note"] for e in kari], ["ro", "re", "ri"])
            for e, hz in zip(kari, (293.66, 392.0, 523.25)):
                self.assertAlmostEqual(e["hz"], hz, delta=hz * 0.01)
            meri = [e for e in m if e["art"] == "meri"]
            self.assertEqual(len(meri), 1)
            self.assertEqual(meri[0]["note"], "ro")
            self.assertLess(meri[0]["cents"], -50)  # flattened
            self.assertTrue((d / "out" / kari[0]["file"]).exists())
            self.assertTrue((d / "bank" / "ro.wav").exists() and (d / "bank" / "ri.wav").exists())
            self.assertIn('name: "re"', r.stdout)


if __name__ == "__main__":
    unittest.main()
