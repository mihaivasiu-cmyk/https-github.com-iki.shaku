import SwiftUI

struct ContentView: View {
    @EnvironmentObject var engine: IkiEngine
    @AppStorage("night") private var night = false
    @State private var tab = 0

    var body: some View {
        let pal = night ? Palette.night : Palette.day
        ZStack {
            pal.bg.ignoresSafeArea()
            if engine.loaded {
                VStack(spacing: 0) {
                    header(pal)
                    Group {
                        switch tab {
                        case 0: PlayTab()
                        case 1: SoundTab()
                        case 2: LoopTab()
                        default: MoreTab(night: $night)
                        }
                    }
                    tabBar(pal)
                }
            } else {
                VStack(spacing: 12) {
                    ProgressView()
                    Text("Loading shakuhachi samples…").font(.footnote).foregroundColor(pal.muted)
                }
            }
        }
        .environment(\.palette, pal)
        .preferredColorScheme(night ? .dark : .light)
    }

    private func header(_ pal: Palette) -> some View {
        HStack {
            Text("息").font(.system(size: 26, weight: .bold)).foregroundColor(pal.moss)
            Text("Iki").font(.system(size: 20, weight: .semibold, design: .rounded)).foregroundColor(pal.fg)
            Spacer()
            if engine.s(STATE_CLIP) > 0.5 {
                Text("CLIP").font(.system(size: 11, weight: .bold)).foregroundColor(pal.danger)
            }
            Text(engine.text(TEXT_BACKEND)).font(.system(size: 11)).foregroundColor(pal.subtle)
        }
        .padding(.horizontal, 16).padding(.vertical, 8)
    }

    private func tabBar(_ pal: Palette) -> some View {
        HStack(spacing: 0) {
            ForEach(Array(["Play", "Sound", "Loop", "More"].enumerated()), id: \.offset) { i, t in
                Button(action: { tab = i }) {
                    VStack(spacing: 2) {
                        Capsule().fill(tab == i ? pal.moss : Color.clear).frame(width: 28, height: 3)
                        Text(t).font(.system(size: 13, weight: tab == i ? .semibold : .regular, design: .rounded))
                            .foregroundColor(tab == i ? pal.fg : pal.subtle)
                    }
                    .frame(maxWidth: .infinity, minHeight: 44)
                }
                .buttonStyle(PlainButtonStyle())
            }
        }
        .background(pal.surface.shadow(color: Color.black.opacity(0.1), radius: 4, y: -1))
    }
}

// MARK: - Play

struct PlayTab: View {
    @EnvironmentObject var engine: IkiEngine
    @Environment(\.palette) private var pal
    @State private var presetName = ""
    @State private var saving = false
    private let kana = ["ロ", "ツ", "レ", "チ", "リ", "ヒ"]
    private let names = ["Ro", "Tsu", "Re", "Chi", "Ri", "Hi"]

    var body: some View {
        let _ = engine.frame
        ScrollView {
            VStack(spacing: 10) {
                Choice(id: P_VOICE, labels: P_VOICE_LABELS)
                presetRow

                Module("Now playing") {
                    HStack(spacing: 8) {
                        Text(engine.text(TEXT_KANA)).font(.system(size: 30, weight: .bold)).foregroundColor(pal.moss)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(engine.text(TEXT_NAME)).font(.system(size: 15, weight: .semibold))
                            Text(engine.text(TEXT_PITCH)).font(.system(size: 12, design: .monospaced)).foregroundColor(pal.muted)
                        }
                        Spacer()
                    }
                    .foregroundColor(pal.fg)
                    FluteView()
                    Readout(text: engine.text(TEXT_HOLES), size: 12)
                }

                Module("Fingering — kari (top) · meri (bottom)") {
                    HStack(spacing: 6) {
                        ForEach(0..<6, id: \.self) { i in Pad(index: i, meri: false, kana: kana[i], name: names[i]) }
                    }
                    HStack(spacing: 6) {
                        ForEach(0..<6, id: \.self) { i in Pad(index: i, meri: true, kana: kana[i] + "メ", name: names[i] + " meri") }
                    }
                }

                Module("Breath") {
                    HStack(spacing: 8) {
                        TouchArea(onDown: { engine.gesture(G_TOGGLE_GATE) }, onUp: {}) { down in
                            Text(engine.blowing ? "吹 GATE" : "吹 BLOW")
                                .font(mincho(18)).tracking(3)
                                .frame(maxWidth: .infinity, minHeight: 48)
                                .foregroundColor(engine.blowing ? pal.mossFg : (pal.night ? pal.bg : pal.surface))
                                .background(RoundedRectangle(cornerRadius: 3).fill(engine.blowing ? pal.moss : pal.fg))
                                .scaleEffect(down ? 0.97 : 1)
                        }
                        TouchArea(onDown: { engine.gesture(G_MURAIKI) }, onUp: {}) { down in
                            Text("Muraiki")
                                .font(.system(size: 15, weight: .semibold, design: .rounded))
                                .frame(maxWidth: .infinity, minHeight: 48)
                                .foregroundColor(pal.fg)
                                .background(RoundedRectangle(cornerRadius: 12).fill(down ? pal.iris.opacity(0.5) : pal.surface2))
                                .overlay(RoundedRectangle(cornerRadius: 12).stroke(pal.border))
                        }
                    }
                    HStack(spacing: 8) {
                        TapButton(title: engine.get(P_OTSU) > 0.5 ? "Octave: Otsu" : "Octave: Kan") {
                            engine.set(P_OTSU, engine.get(P_OTSU) > 0.5 ? 0 : 1)
                        }
                        TapButton(title: "Phrase", iris: true) { engine.gesture(G_PHRASE) }
                    }
                    Text("Bend").font(.system(size: 11)).foregroundColor(pal.subtle)
                    BendStrip()
                }

                Module("Expression") {
                    HStack(alignment: .top) {
                        Knob(id: P_MERI, label: "Meri")
                        Knob(id: P_IKI, label: "Iki")
                        Knob(id: P_GAIN, label: "Gain")
                        Knob(id: P_LEAN, label: "Lean", iris: true)
                    }
                    .frame(maxWidth: .infinity)
                    HStack(alignment: .top) {
                        Knob(id: P_YURI, label: "Yuri", iris: true)
                        Knob(id: P_YURI_RATE, label: "Rate", iris: true)
                        Knob(id: P_GLIDE, label: "Glide", iris: true)
                        Knob(id: P_BEND_RANGE, label: "Range", iris: true)
                    }
                    .frame(maxWidth: .infinity)
                }
            }
            .padding(.horizontal, 12).padding(.bottom, 16)
        }
    }

    private var presetRow: some View {
        HStack(spacing: 8) {
            Menu {
                Section {
                    ForEach(Array(engine.factoryNames.enumerated()), id: \.offset) { i, n in
                        Button(n) { engine.loadFactory(i) }
                    }
                }
                if !engine.userPresets.isEmpty {
                    Section {
                        ForEach(engine.userPresets, id: \.self) { n in Button(n) { engine.loadPreset(n) } }
                    }
                }
            } label: {
                Text("Presets ▾")
                    .font(.system(size: 13, weight: .semibold, design: .rounded))
                    .frame(maxWidth: .infinity, minHeight: 40)
                    .foregroundColor(pal.fg)
                    .background(RoundedRectangle(cornerRadius: 10).fill(pal.surface2))
                    .overlay(RoundedRectangle(cornerRadius: 10).stroke(pal.border))
            }
            TapButton(title: "Save…") { saving = true }
        }
        .alert(isPresented: $saving, TextFieldAlert(title: "Save preset", text: $presetName) {
            engine.savePreset(presetName)
        })
    }
}

// MARK: - Sound

struct SoundTab: View {
    @EnvironmentObject var engine: IkiEngine

    var body: some View {
        ScrollView {
            VStack(spacing: 10) {
                Module("Layers") {
                    knobs([(P_FLUTE_LEVEL, "Flute"), (P_BORE_LEVEL, "Bore"), (P_TUNE, "Tune"), (P_FOLI_LEVEL, "Foli")])
                }
                Module("Filter") {
                    Choice(id: P_FILT_TYPE, labels: Array(P_FILT_TYPE_LABELS.prefix(4)))
                    CurveView(which: CURVE_FILTER)
                    knobs([(P_FILT_CUT, "Cutoff"), (P_FILT_RES, "Reso"), (P_FILT_MOTION, "Motion"), (P_FILT_RATE, "Rate")])
                    Choice(id: P_FILT_AUTO, labels: P_FILT_AUTO_LABELS, iris: true)
                }
                Module("Amp envelope") {
                    CurveView(which: CURVE_AMP_ENV)
                    knobs([(P_ENV_A, "A"), (P_ENV_D, "D"), (P_ENV_S, "S"), (P_ENV_R, "R")])
                    Choice(id: P_ENV_SHAPE, labels: P_ENV_SHAPE_LABELS)
                }
                Module("Filter envelope") {
                    CurveView(which: CURVE_FILT_ENV, iris: true)
                    knobs([(P_FENV_A, "A"), (P_FENV_D, "D"), (P_FENV_S, "S"), (P_FENV_R, "R")], iris: true)
                    Choice(id: P_FENV_SHAPE, labels: P_FENV_SHAPE_LABELS, iris: true)
                }
                Module("LFO") {
                    Choice(id: P_LFO_WAVE, labels: P_LFO_WAVE_LABELS)
                    knobs([(P_LFO_RATE, "Rate"), (P_LFO_PITCH_AMT, "Pitch"), (P_LFO_FILT_AMT, "Filter"), (P_LFO_AMP_AMT, "Amp")])
                    HStack(spacing: 6) {
                        Toggle1(id: P_LFO_PITCH, title: "→Pitch")
                        Toggle1(id: P_LFO_FILT, title: "→Filter")
                        Toggle1(id: P_LFO_AMP, title: "→Amp")
                    }
                }
                Module("Delay") {
                    knobs([(P_DELAY_TIME, "Time"), (P_DELAY_FEEDBACK, "Feedback"), (P_DELAY_MIX, "Mix"), (P_TEMPO, "Tempo")])
                    knobs([(P_DELAY_TONE, "Tone"), (P_DELAY_LOWCUT, "Low cut")])
                    HStack(spacing: 6) {
                        Toggle1(id: P_DELAY_PING, title: "Ping-pong")
                        Toggle1(id: P_DELAY_SYNC, title: "Sync")
                    }
                }
                Module("Space") {
                    Choice(id: P_REVERB_TYPE, labels: P_REVERB_TYPE_LABELS, iris: true)
                    knobs([(P_REVERB, "Reverb"), (P_REVERB_DECAY, "Decay"), (P_REVERB_SIZE, "Size"), (P_REVERB_PRE, "Pre")], iris: true)
                    knobs([(P_REVERB_DAMP, "Damp"), (P_WIDTH, "Width"), (P_TAPE, "Tape")], iris: true)
                    knobs([(P_PHASER, "Phaser"), (P_PHASER_DEPTH, "Depth")], iris: true)
                }
            }
            .padding(.horizontal, 12).padding(.bottom, 16)
        }
    }

    private func knobs(_ items: [(Int32, String)], iris: Bool = false) -> some View {
        HStack(alignment: .top) {
            ForEach(Array(items.enumerated()), id: \.offset) { _, it in Knob(id: it.0, label: it.1, iris: iris) }
        }
        .frame(maxWidth: .infinity)
    }
}

// MARK: - Loop

struct LoopTab: View {
    @EnvironmentObject var engine: IkiEngine
    @Environment(\.palette) private var pal
    @State private var showShare = false

    var body: some View {
        let _ = engine.frame
        ScrollView {
            VStack(spacing: 10) {
                Module("Clock") {
                    Readout(text: engine.text(TEXT_CLOCK))
                    HStack(spacing: 8) {
                        TapButton(title: "Stop all", danger: true) { engine.loop(L_STOP_ALL, 0) }
                        TapButton(title: "Freeze", iris: true) { engine.loop(L_FREEZE, 0) }
                        TapButton(title: "Export WAV") { engine.exportLoops() }
                    }
                    if !engine.message.isEmpty {
                        Text(engine.message).font(.system(size: 12)).foregroundColor(pal.muted)
                    }
                }
                ForEach(0..<4, id: \.self) { t in track(t) }
                Module("Foli (rhythm)") {
                    HStack(spacing: 8) {
                        Toggle1(id: P_FOLI_ON, title: "Foli on", iris: true)
                        Menu {
                            ForEach(Array(P_FOLI_KIT_LABELS.enumerated()), id: \.offset) { i, n in
                                Button(n) { engine.set(P_FOLI_KIT, Double(i)) }
                            }
                        } label: {
                            Text("Kit: " + label(P_FOLI_KIT_LABELS, P_FOLI_KIT))
                                .font(.system(size: 13, weight: .semibold, design: .rounded))
                                .frame(maxWidth: .infinity, minHeight: 40)
                                .foregroundColor(pal.fg)
                                .background(RoundedRectangle(cornerRadius: 10).fill(pal.surface2))
                                .overlay(RoundedRectangle(cornerRadius: 10).stroke(pal.border))
                        }
                    }
                    Choice(id: P_FOLI_FEEL, labels: P_FOLI_FEEL_LABELS)
                    HStack(spacing: 6) {
                        ForEach(0..<8, id: \.self) { i in
                            let mask = Int(engine.s(STATE_FOLI_MASK))
                            let on = (mask >> i) & 1 == 1
                            let cur = Int(engine.s(STATE_FOLI_STEP)) == i && engine.s(STATE_FOLI_ON) > 0.5
                            Button(action: { Haptic.tick(); engine.gesture(G_FOLI_STEP, i) }) {
                                RoundedRectangle(cornerRadius: 6)
                                    .fill(on ? pal.accent(true) : pal.surface2)
                                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(cur ? pal.moss : pal.border, lineWidth: cur ? 2 : 1))
                                    .frame(height: 34)
                            }
                            .buttonStyle(PlainButtonStyle())
                        }
                    }
                    HStack(alignment: .top) {
                        Knob(id: P_FOLI_LEVEL, label: "Level", iris: true)
                        Knob(id: P_TEMPO, label: "Tempo")
                    }
                    .frame(maxWidth: .infinity)
                }
            }
            .padding(.horizontal, 12).padding(.bottom, 16)
        }
        .sheet(isPresented: Binding(
            get: { engine.exportURL != nil },
            set: { if !$0 { engine.exportURL = nil } }
        )) {
            if let u = engine.exportURL { ShareSheet(url: u) }
        }
    }

    private func label(_ labels: [String], _ id: Int32) -> String {
        let i = Int(engine.get(id).rounded())
        return i >= 0 && i < labels.count ? labels[i] : "—"
    }

    private func track(_ t: Int) -> some View {
        let o = STATE_TRACKS + t * TRACK_STRIDE
        let mode = Int(engine.s(o))
        let selected = Int(engine.s(STATE_LOOP_TRACK)) == t
        let names = ["Idle", "Rec", "Play", "Dub", "Stop"]
        return VStack(spacing: 6) {
            HStack {
                Text("Track \(t + 1)").font(.system(size: 13, weight: .semibold))
                Text(names[min(max(mode, 0), 4)]).font(.system(size: 11)).foregroundColor(mode == 1 ? pal.danger : pal.subtle)
                Spacer()
                Text(String(format: "%.1fs", engine.s(o + 2))).font(.system(size: 11, design: .monospaced)).foregroundColor(pal.subtle)
            }
            .foregroundColor(pal.fg)
            WaveStrip(wave: engine.loopWave(t), progress: Double(engine.s(o + 1)), live: mode == 2 || mode == 3)
            HStack(spacing: 6) {
                TapButton(title: mode == 1 ? "● Rec" : (mode == 3 ? "● Dub" : "Rec/Dub"), on: mode == 1 || mode == 3, danger: false) {
                    engine.loop(L_SELECT, t)
                    engine.loop(L_PUNCH, t)
                }
                TapButton(title: "Play") { engine.loop(L_PLAY, t) }
                TapButton(title: "Stop") { engine.loop(L_STOP, t) }
                TapButton(title: "Undo") { engine.loop(L_UNDO, t) }
            }
            HStack {
                Text("Level").font(.system(size: 11)).foregroundColor(pal.subtle)
                Slider(value: Binding(
                    get: { Double(engine.s(o + 3)) },
                    set: { engine.loop(L_LEVEL, t, $0) }
                ), in: 0...1)
                TapButton(title: "FX", on: engine.s(o + 6) > 0.5, iris: true) {
                    engine.loop(L_FX, t, engine.s(o + 6) > 0.5 ? 0 : 1)
                }
                .frame(width: 56)
            }
        }
        .padding(12)
        .background(RoundedRectangle(cornerRadius: 14).fill(pal.surface))
        .overlay(RoundedRectangle(cornerRadius: 14).stroke(selected ? pal.moss : pal.border, lineWidth: selected ? 2 : 1))
    }
}

struct WaveStrip: View {
    @Environment(\.palette) private var pal
    let wave: [Float]
    let progress: Double
    let live: Bool
    var body: some View {
        Canvas { ctx, size in
            let mid = size.height / 2
            if !wave.isEmpty {
                var p = Path()
                for (i, a) in wave.enumerated() {
                    let x = size.width * CGFloat(i) / CGFloat(max(1, wave.count - 1))
                    let h = CGFloat(min(1, abs(a))) * mid
                    p.move(to: CGPoint(x: x, y: mid - h))
                    p.addLine(to: CGPoint(x: x, y: mid + h))
                }
                ctx.stroke(p, with: .color(pal.accent(false)), lineWidth: 1)
            }
            if live {
                let x = size.width * CGFloat(min(1, max(0, progress)))
                ctx.fill(Path(CGRect(x: x - 1, y: 0, width: 2, height: size.height)), with: .color(pal.iris))
            }
        }
        .frame(height: 38)
        .background(RoundedRectangle(cornerRadius: 8).fill(pal.lcd))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(pal.border))
    }
}

// MARK: - More

struct MoreTab: View {
    @EnvironmentObject var engine: IkiEngine
    @Environment(\.palette) private var pal
    @Binding var night: Bool

    var body: some View {
        let _ = engine.frame
        ScrollView {
            VStack(spacing: 10) {
                Module("Look") {
                    HStack(spacing: 8) {
                        TapButton(title: "Day", on: !night) { night = false }
                        TapButton(title: "Night", on: night) { night = true }
                    }
                }
                Module("Engine") {
                    Readout(text: engine.text(TEXT_ENGINE), size: 12)
                    Readout(text: engine.text(TEXT_STATUS), size: 12)
                    let p = engine.text(TEXT_PROBLEM)
                    if !p.isEmpty { Text(p).font(.system(size: 12)).foregroundColor(pal.danger) }
                    Text(String(format: "Latency %.1f ms · limiter %.1f dB", engine.s(STATE_LATENCY_MS), engine.s(STATE_COMP_GR)))
                        .font(.system(size: 12, design: .monospaced)).foregroundColor(pal.muted)
                }
                Module("Tips") {
                    Text("""
                    • Tap a pad to sound a note; BLOW latches the breath.
                    • Meri row bends the pitch down the way a real player tilts the chin.
                    • Double-tap any knob to reset it. Drag it to see its value.
                    • Plug in a MIDI keyboard (Lightning/USB camera adapter) — it is picked up automatically.
                    • Loop → Export WAV opens the share sheet (Files, AirDrop, Voice Memos…).
                    • Use headphones or turn the volume up: the iPhone 7 speaker is small.
                    """)
                    .font(.system(size: 13)).foregroundColor(pal.muted)
                }
                Text("Iki — shakuhachi instrument").font(.system(size: 11)).foregroundColor(pal.subtle)
            }
            .padding(.horizontal, 12).padding(.bottom, 16)
        }
    }
}

/// Text-field alert for iOS 15 (SwiftUI has no native one before iOS 16). Presented through UIKit.
struct TextFieldAlert {
    let title: String
    let text: Binding<String>
    let onSave: () -> Void
}

extension View {
    func alert(isPresented: Binding<Bool>, _ a: TextFieldAlert) -> some View {
        background(AlertHost(isPresented: isPresented, alert: a))
    }
}

private struct AlertHost: UIViewControllerRepresentable {
    @Binding var isPresented: Bool
    let alert: TextFieldAlert

    func makeUIViewController(context: Context) -> UIViewController { UIViewController() }

    func updateUIViewController(_ vc: UIViewController, context: Context) {
        guard isPresented, vc.presentedViewController == nil else { return }
        let ac = UIAlertController(title: alert.title, message: nil, preferredStyle: .alert)
        ac.addTextField { f in
            f.placeholder = "Name"
            f.text = alert.text.wrappedValue
        }
        ac.addAction(UIAlertAction(title: "Cancel", style: .cancel) { _ in isPresented = false })
        ac.addAction(UIAlertAction(title: "Save", style: .default) { _ in
            alert.text.wrappedValue = ac.textFields?.first?.text ?? ""
            isPresented = false
            alert.onSave()
        })
        DispatchQueue.main.async { vc.present(ac, animated: true) }
    }
}
