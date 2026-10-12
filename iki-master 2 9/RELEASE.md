# Release checklist

1. **Clear the samples.** `crates/iki-core/assets/samples/NOTICE.txt` — the bank is
   cut from a third-party pack whose terms forbid redistributing it in its
   original form. Get written permission, or record/commission your own and
   rebuild the bank (`tools/sample-import/`). Do the same for the Foli loops in
   `assets/drums` and for `samples_riley` / `samples_nbm`, which should not ship.
2. **Build and test on a networked machine:** `scripts/check.sh` (format, clippy,
   all crate tests, plugin + Android bridge). The UI crates have only ever been
   compiled in CI-style environments, never run by hand with these changes.
3. **Play-test every control** on desktop, in a DAW (VST3/CLAP) and on a phone.
   Watch the limiter meter, the Engine panel and MIDI bend/CC mapping.
4. **Validate plugins** (clap-validator / pluginval) and the Android build
   (`scripts/build-android.sh`).
5. **Sign and notarise** the macOS app, sign the Windows installer, sign the
   Android bundle. Set up an update channel.
6. **Prepare the store/site assets:** icon, screenshots, a short demo video, the
   five factory patches' audio demos.
