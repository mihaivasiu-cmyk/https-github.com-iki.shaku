# Iki on iPhone 7 (iOS 15)

The iPhone 7 can run up to **iOS 15.8**, so this build targets iOS 15.0 and uses only iOS 15-safe SwiftUI (no `NavigationStack`, `ShareLink`, `Grid`…). It is iPhone-only, portrait.

## How it is built

- The sound engine is the same Rust kernel as the desktop/Android apps, compiled as a static library (`crates/iki-ios`, C interface in `crates/iki-ios/include/iki.h`).
- Swift owns the audio: `AVAudioEngine` + `AVAudioSourceNode` calls `iki_render` for each block (`ios/Iki/IkiEngine.swift`).
- The screens are SwiftUI (`ContentView.swift`, `Components.swift`) with the same colours and motion timings as the other front ends (generated into `Generated/Theme.swift`).
- The 36 shakuhachi samples are embedded in the library, so there is nothing to download.

## What you need

- A **Mac with Xcode 14 or later** (iOS apps can only be built on macOS — this cannot be done on Windows/Linux).
- Rust (`rustup`) and Homebrew (the script installs `xcodegen`).
- Your iPhone 7 and a Lightning cable.
- A free Apple ID is enough for your own phone (the app expires after **7 days** and you re-run it from Xcode); a paid Apple Developer account ($99/yr) gives 1-year signing and TestFlight.

## Steps

1. `scripts/build-ios.sh --open` — builds the kernel for device + simulator, packs `target/ios/IkiCore.xcframework`, generates `ios/Iki.xcodeproj` and opens it.
2. In Xcode, select the **Iki** target → *Signing & Capabilities* → tick *Automatically manage signing* and choose your **Team** (your Apple ID). If the bundle id is taken, change `org.iki.shakuhachi.ios` to something unique (also in `ios/project.yml`).
3. Plug in the iPhone, unlock it, tap *Trust* if asked, and pick it as the run destination. (iOS 16+ needs *Developer Mode*; the iPhone 7 on iOS 15 does not.)
4. Press **Run**. First launch: on the phone, *Settings → General → VPN & Device Management → your Apple ID → Trust*, then open Iki.

Without your own Mac: the `ios` job in `.github/workflows/build.yml` compiles everything on a GitHub macOS runner (it checks that the app builds; it does not produce a signed app). To get an installable `.ipa` without a Mac you would extend that job to archive it, then sideload with AltStore or Sideloadly (they re-sign with your Apple ID).

## Using it

Tabs: **Play** (pads, blow, muraiki, bend strip, expression knobs, presets), **Sound** (filter, envelopes, LFO, delay, reverb with live curves), **Loop** (4 tracks, Foli rhythm, Export WAV → share sheet), **More** (Day/Night, status).
Double-tap any knob to reset it; drag to see its value. A USB/Lightning MIDI keyboard is picked up automatically. Presets are saved to *Files → On My iPhone → Iki → Presets*.

## Status — read this

None of the iOS code (Rust `iki-ios`, Swift, `project.yml`) has been compiled here; there was no Mac or network. The DSP core it wraps is covered by the offline test suite. Expect a handful of compile errors on first build: send them (or a screenshot) and they get fixed quickly. `ios/Iki/Generated/Ids.swift` was produced by a Python mirror of the generator; `build-ios.sh` regenerates it with the real one.

Performance note: the iPhone 7 (A10) renders the full engine comfortably in release builds, but never run a Debug build for listening — it is far too slow. Use *Product → Scheme → Edit Scheme → Run → Release*.

## Before publishing

The sample pack's licence (Samplephonics) must be checked for commercial redistribution, and App Store release needs a paid account, an icon set and a privacy label ("data not collected").

## Uploading to TestFlight

- Create the app in App Store Connect first (same bundle id), otherwise Xcode says `DistributionAppRecordProviderError`.
- Archive with the destination **Any iOS Device (arm64)** — never "My Mac" or a Mac/Vision destination, or Apple validates it as a Mac app (errors 90242 / 90236 about ICNS and LSApplicationCategoryType).
- The project is iPhone-only (`supportedDestinations: [iOS]`), has a 1024 px app icon (`ios/Iki/Assets.xcassets`) and the music category set.
