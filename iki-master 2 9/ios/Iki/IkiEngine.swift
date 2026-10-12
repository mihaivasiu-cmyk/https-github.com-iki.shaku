import AVFoundation
import Combine
import CoreMIDI
import QuartzCore
import SwiftUI

/// Owns the Rust kernel, the audio engine and the per-frame state. Every view reads from here.
/// Main-thread only, except for the audio render block (which only touches `iki_render`).
final class IkiEngine: ObservableObject {
    private var handle: OpaquePointer?
    private var renderCtx: OpaquePointer?
    private let audio = AVAudioEngine()
    private var source: AVAudioSourceNode?
    private var ticker: Ticker?
    private var link: CADisplayLink?
    private var state: [Float]
    private var defaults: [Int32: Double] = [:]
    private var midiClient = MIDIClientRef()
    private var midiPort = MIDIPortRef()

    /// Bumped every display tick: views that read the engine re-evaluate.
    @Published private(set) var frame = 0
    @Published var message = ""

    init() {
        state = [Float](repeating: 0, count: Int(iki_state_len()))
    }

    // MARK: lifecycle

    func start() {
        guard handle == nil else { return }
        let session = AVAudioSession.sharedInstance()
        try? session.setCategory(.playback, mode: .default, options: [])
        try? session.setPreferredIOBufferDuration(0.005)
        try? session.setActive(true)
        let sr = session.sampleRate > 0 ? session.sampleRate : 48000
        let voice = Int32(UserDefaults.standard.integer(forKey: "voice"))
        guard let h = iki_create(Float(sr), voice) else { return }
        handle = h
        renderCtx = iki_render_ctx(h)

        let ctx = renderCtx
        let format = AVAudioFormat(standardFormatWithSampleRate: sr, channels: 2)!
        let node = AVAudioSourceNode(format: format) { _, _, frames, abl -> OSStatus in
            let list = UnsafeMutableAudioBufferListPointer(abl)
            if list.count >= 2, let l = list[0].mData, let r = list[1].mData {
                iki_render(ctx, l.assumingMemoryBound(to: Float.self), r.assumingMemoryBound(to: Float.self), Int32(frames))
            }
            return noErr
        }
        source = node
        audio.attach(node)
        audio.connect(node, to: audio.mainMixerNode, format: format)
        do { try audio.start() } catch { message = "Audio failed to start: \(error.localizedDescription)" }

        let t = Ticker { [weak self] in self?.tick() }
        ticker = t
        let l = CADisplayLink(target: t, selector: #selector(Ticker.fire))
        l.preferredFramesPerSecond = 30
        l.add(to: .main, forMode: .common)
        link = l

        NotificationCenter.default.addObserver(
            forName: AVAudioSession.interruptionNotification, object: nil, queue: .main
        ) { [weak self] note in
            guard let self = self,
                  let raw = note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
                  let kind = AVAudioSession.InterruptionType(rawValue: raw) else { return }
            if kind == .began {
                self.setPaused(true)
            } else {
                try? AVAudioSession.sharedInstance().setActive(true)
                try? self.audio.start()
                self.setPaused(false)
            }
        }
        startMIDI()
    }

    func stop() {
        link?.invalidate()
        link = nil
        audio.stop()
        if let h = handle { iki_destroy(h) }
        handle = nil
        renderCtx = nil
    }

    func setPaused(_ paused: Bool) {
        guard let h = handle else { return }
        iki_pause(h, paused)
        if !paused { try? audio.start() }
    }

    private func tick() {
        guard let h = handle else { return }
        iki_tick(h)
        state.withUnsafeMutableBufferPointer { iki_state(h, $0.baseAddress, Int32($0.count)) }
        frame &+= 1
    }

    // MARK: reading

    func s(_ index: Int) -> Float { index < state.count ? state[index] : 0 }
    var loaded: Bool { s(STATE_LOADED) >= 36 }
    var blowing: Bool { s(STATE_BLOWING) > 0.5 }

    func get(_ id: Int32) -> Double { handle.map { iki_get($0, id) } ?? 0 }

    func defaultValue(_ id: Int32) -> Double {
        if let d = defaults[id] { return d }
        let d = iki_default(id)
        defaults[id] = d
        return d
    }

    func hint(_ id: Int32) -> String { read { iki_hint(self.handle, id, $0, $1) } }
    func text(_ which: Int32) -> String { read { iki_text(self.handle, which, $0, $1) } }

    private func read(_ f: (UnsafeMutablePointer<CChar>, Int32) -> Int32) -> String {
        guard handle != nil else { return "" }
        var buf = [CChar](repeating: 0, count: 160)
        var need = buf.withUnsafeMutableBufferPointer { Int(f($0.baseAddress!, Int32($0.count))) }
        if need >= buf.count {
            buf = [CChar](repeating: 0, count: need + 1)
            need = buf.withUnsafeMutableBufferPointer { Int(f($0.baseAddress!, Int32($0.count))) }
        }
        return String(cString: buf)
    }

    func curve(_ which: Int32, points: Int) -> [Float] {
        guard let h = handle else { return [] }
        var out = [Float](repeating: 0, count: points + 1)
        out.withUnsafeMutableBufferPointer { iki_curve(h, which, $0.baseAddress, Int32($0.count)) }
        return out
    }

    func loopWave(_ track: Int) -> [Float] {
        guard let h = handle else { return [] }
        var out = [Float](repeating: 0, count: 384)
        let n = out.withUnsafeMutableBufferPointer { Int(iki_loop_wave(h, Int32(track), $0.baseAddress, Int32($0.count))) }
        return Array(out.prefix(n))
    }

    // MARK: acting

    func set(_ id: Int32, _ value: Double) {
        guard let h = handle else { return }
        iki_set(h, id, min(1, max(0, value)))
        if id == P_VOICE { UserDefaults.standard.set(Int(value.rounded()), forKey: "voice") }
    }

    func gesture(_ g: Int32, _ index: Int = 0, meri: Bool = false) {
        if let h = handle { iki_gesture(h, g, Int32(index), meri ? 1 : 0) }
    }

    func loop(_ op: Int32, _ track: Int, _ value: Double = 0) {
        if let h = handle { iki_loop(h, op, Int32(track), value) }
    }

    // MARK: patches

    var factoryNames: [String] {
        (0..<Int(iki_factory_count())).map { i in
            var buf = [CChar](repeating: 0, count: 64)
            _ = buf.withUnsafeMutableBufferPointer { iki_factory_name(Int32(i), $0.baseAddress, Int32($0.count)) }
            return String(cString: buf)
        }
    }

    func loadFactory(_ i: Int) {
        guard let h = handle else { return }
        message = iki_load_factory(h, Int32(i)) == 1 ? "Loaded \(factoryNames[i])" : "Could not load patch"
    }

    private var presetDir: URL {
        let d = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("Presets")
        try? FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
        return d
    }

    var userPresets: [String] {
        let files = (try? FileManager.default.contentsOfDirectory(atPath: presetDir.path)) ?? []
        return files.filter { $0.hasSuffix(".ikipreset") }.map { String($0.dropLast(".ikipreset".count)) }.sorted()
    }

    func savePreset(_ rawName: String) {
        let name = rawName.filter { $0.isLetter || $0.isNumber || $0 == " " || $0 == "-" || $0 == "_" }
            .trimmingCharacters(in: .whitespaces)
        guard let h = handle, !name.isEmpty else { message = "Type a name first"; return }
        var buf = [CChar](repeating: 0, count: 4096)
        let n = buf.withUnsafeMutableBufferPointer { Int(iki_preset_text(h, name, $0.baseAddress, Int32($0.count))) }
        if n >= buf.count {
            buf = [CChar](repeating: 0, count: n + 1)
            _ = buf.withUnsafeMutableBufferPointer { iki_preset_text(h, name, $0.baseAddress, Int32($0.count)) }
        }
        do {
            try String(cString: buf).write(to: presetDir.appendingPathComponent("\(name).ikipreset"), atomically: true, encoding: .utf8)
            message = "Saved “\(name)”"
        } catch { message = "Could not save: \(error.localizedDescription)" }
    }

    func loadPreset(_ name: String) {
        guard let h = handle,
              let text = try? String(contentsOf: presetDir.appendingPathComponent("\(name).ikipreset"), encoding: .utf8)
        else { message = "Could not read that patch"; return }
        message = iki_load_preset(h, text) == 1 ? "Loaded “\(name)”" : "Not an Iki patch"
    }

    // MARK: looper export

    @Published var exportURL: URL?

    func exportLoops() {
        guard let h = handle else { return }
        iki_export_request(h)
        // The audio thread answers within a few blocks; poll for ~1 s.
        var tries = 0
        Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] timer in
            guard let self = self, let h = self.handle else { timer.invalidate(); return }
            tries += 1
            var cap = 1 << 22
            var buf = [UInt8](repeating: 0, count: cap)
            var n = Int(buf.withUnsafeMutableBufferPointer { iki_export_take(h, $0.baseAddress, Int32($0.count)) })
            if n < 0 {
                cap = -n
                buf = [UInt8](repeating: 0, count: cap)
                n = Int(buf.withUnsafeMutableBufferPointer { iki_export_take(h, $0.baseAddress, Int32($0.count)) })
            }
            if n > 0 {
                timer.invalidate()
                let url = FileManager.default.temporaryDirectory.appendingPathComponent("iki-loops.wav")
                try? Data(buf.prefix(n)).write(to: url)
                self.exportURL = url
            } else if tries > 12 {
                timer.invalidate()
                self.message = "Nothing recorded yet"
            }
        }
    }

    // MARK: MIDI in (USB / Bluetooth keyboards)

    private func startMIDI() {
        guard MIDIClientCreateWithBlock("Iki" as CFString, &midiClient, nil) == noErr else { return }
        let status = MIDIInputPortCreateWithBlock(midiClient, "Iki in" as CFString, &midiPort) { [weak self] list, _ in
            var bytes: [UInt8] = []
            for packet in list.unsafeSequence() {
                let n = Int(packet.pointee.length)
                withUnsafeBytes(of: packet.pointee.data) { raw in bytes.append(contentsOf: raw.prefix(n)) }
            }
            guard !bytes.isEmpty else { return }
            DispatchQueue.main.async {
                guard let self = self, let h = self.handle else { return }
                bytes.withUnsafeBufferPointer { iki_midi_in(h, $0.baseAddress, Int32($0.count)) }
            }
        }
        guard status == noErr else { return }
        for i in 0..<MIDIGetNumberOfSources() { MIDIPortConnectSource(midiPort, MIDIGetSource(i), nil) }
    }
}

/// CADisplayLink needs an Objective-C target.
private final class Ticker: NSObject {
    let block: () -> Void
    init(_ block: @escaping () -> Void) { self.block = block }
    @objc func fire() { block() }
}
