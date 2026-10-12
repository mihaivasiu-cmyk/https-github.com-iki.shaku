import SwiftUI
import UIKit

/// Palette is passed through the environment so every component follows the Day/Night switch.
private struct PaletteKey: EnvironmentKey { static let defaultValue = Palette.day }
extension EnvironmentValues {
    var palette: Palette {
        get { self[PaletteKey.self] }
        set { self[PaletteKey.self] = newValue }
    }
}

/// Serif face for the pad kana and the Blow button (Hiragino Mincho on every iPhone).
func mincho(_ size: CGFloat) -> Font { .custom("HiraMinProN-W6", size: size) }

/// Knob stone: dark ink in both themes, with a paper-coloured pointer.
private func stone(_ pal: Palette) -> Color { pal.night ? Color(hex: 0x34302a) : Color(hex: 0x1e1b16) }
private let paperMark = Color(hex: 0xf3f0e8)

enum Haptic {
    static func tick() { UIImpactFeedbackGenerator(style: .light).impactOccurred() }
    static func thud() { UIImpactFeedbackGenerator(style: .medium).impactOccurred() }
}

/// Section card — the "module" of the other front-ends.
struct Module<Content: View>: View {
    @Environment(\.palette) private var pal
    let title: String
    let content: Content
    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title.uppercased())
                .font(.system(size: 11, weight: .semibold, design: .rounded))
                .tracking(1.4)
                .foregroundColor(pal.subtle)
            content
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 14).fill(pal.surface))
        .overlay(RoundedRectangle(cornerRadius: 14).stroke(pal.border, lineWidth: 1))
        .shadow(color: Color.black.opacity(pal.night ? 0.4 : 0.07), radius: 5, x: 0, y: 2)
    }
}

/// Rotary knob. Drag vertically; double-tap resets to the default; the value bubble shows while dragging.
struct Knob: View {
    @EnvironmentObject var engine: IkiEngine
    @Environment(\.palette) private var pal
    let id: Int32
    let label: String
    var iris = false
    var size: CGFloat = 58
    @State private var dragging = false
    @State private var startValue = 0.0

    var body: some View {
        let _ = engine.frame
        let v = engine.get(id)
        VStack(spacing: 4) {
            ZStack {
                Circle().trim(from: 0, to: 0.75).stroke(pal.border, style: StrokeStyle(lineWidth: 2.4, lineCap: .round))
                    .rotationEffect(.degrees(135))
                Circle().trim(from: 0, to: CGFloat(v) * 0.75)
                    .stroke(pal.accent(iris), style: StrokeStyle(lineWidth: 2.8, lineCap: .round))
                    .rotationEffect(.degrees(135))
                Circle().fill(stone(pal)).padding(6)
                Ellipse().fill(Color.white.opacity(0.1)).frame(width: size * 0.22, height: size * 0.12)
                    .offset(x: -size * 0.12, y: -size * 0.16).rotationEffect(.degrees(-30))
                Capsule().fill(paperMark)
                    .frame(width: 3, height: size * 0.2)
                    .offset(y: -size * 0.26)
                    .rotationEffect(.degrees(-135 + v * 270))
                if dragging {
                    Text(engine.hint(id))
                        .font(.system(size: 12, weight: .semibold, design: .monospaced))
                        .padding(.horizontal, 7).padding(.vertical, 3)
                        .background(Capsule().fill(pal.lcd))
                        .overlay(Capsule().stroke(pal.border))
                        .foregroundColor(pal.fg)
                        .fixedSize()
                        .offset(y: -size * 0.85)
                        .transition(.opacity)
                }
            }
            .frame(width: size, height: size)
            .contentShape(Circle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { g in
                        if !dragging {
                            dragging = true
                            startValue = engine.get(id)
                        }
                        engine.set(id, min(1, max(0, startValue - Double(g.translation.height) / 180)))
                    }
                    .onEnded { _ in dragging = false }
            )
            .simultaneousGesture(
                TapGesture(count: 2).onEnded {
                    engine.set(id, engine.defaultValue(id))
                    Haptic.tick()
                }
            )
            .animation(.easeOut(duration: Motion.bubble), value: dragging)
            Text(label)
                .font(.system(size: 11, weight: .medium, design: .rounded))
                .foregroundColor(pal.muted)
        }
        .frame(minWidth: size)
        .zIndex(dragging ? 1 : 0)
    }
}

/// A button that reports touch-down and touch-up separately (pads, blow).
struct TouchArea<Content: View>: View {
    var onDown: () -> Void
    var onUp: () -> Void
    let content: (Bool) -> Content
    @State private var down = false
    var body: some View {
        content(down)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { _ in
                        if !down { down = true; Haptic.tick(); onDown() }
                    }
                    .onEnded { _ in
                        down = false
                        onUp()
                    }
            )
    }
}

struct Pad: View {
    @EnvironmentObject var engine: IkiEngine
    @Environment(\.palette) private var pal
    let index: Int
    let meri: Bool
    let kana: String
    let name: String

    var body: some View {
        let _ = engine.frame
        let active = Int(engine.s(STATE_NOTE)) == index
            && (engine.s(STATE_MERI_LANE) > 0.5) == meri && engine.blowing
        TouchArea(
            onDown: { engine.gesture(G_HIT_START, index, meri: meri) },
            onUp: { engine.gesture(G_SLIDE_END) }
        ) { down in
            VStack(spacing: 1) {
                Text(kana).font(mincho(meri ? 17 : 23))
                Text(name).font(.system(size: 9, weight: .medium)).tracking(0.8).opacity(0.7)
            }
            .frame(maxWidth: .infinity, minHeight: meri ? 44 : 56)
            .foregroundColor(active || down ? (meri ? pal.mossFg : (pal.night ? pal.bg : pal.surface)) : (meri ? pal.subtle : pal.fg))
            .background(RoundedRectangle(cornerRadius: 3).fill(active || down ? (meri ? pal.moss : pal.fg) : Color.clear))
            .overlay(RoundedRectangle(cornerRadius: 3)
                .stroke(meri ? pal.subtle : pal.fg, style: StrokeStyle(lineWidth: meri ? 1.2 : 1.6, dash: meri ? [4, 3] : [])))
            .scaleEffect(down ? 0.96 : 1)
            .animation(.easeOut(duration: Motion.hole), value: down)
        }
    }
}

/// Plain tap button with a pressed state.
struct TapButton: View {
    @Environment(\.palette) private var pal
    let title: String
    var on = false
    var iris = false
    var danger = false
    let action: () -> Void
    var body: some View {
        Button(action: { Haptic.tick(); action() }) {
            Text(title)
                .font(.system(size: 13, weight: .semibold, design: .rounded))
                .lineLimit(1).minimumScaleFactor(0.7)
                .frame(maxWidth: .infinity, minHeight: 40)
                .foregroundColor(on ? pal.mossFg : (danger ? pal.danger : pal.fg))
                .background(RoundedRectangle(cornerRadius: 10).fill(on ? pal.accent(iris) : pal.surface2))
                .overlay(RoundedRectangle(cornerRadius: 10).stroke(pal.border))
        }
        .buttonStyle(PlainButtonStyle())
        .animation(.easeOut(duration: Motion.sw), value: on)
    }
}

/// Segmented choice bound to a choice parameter.
struct Choice: View {
    @EnvironmentObject var engine: IkiEngine
    @Environment(\.palette) private var pal
    let id: Int32
    let labels: [String]
    var iris = false
    var body: some View {
        let _ = engine.frame
        let cur = Int(engine.get(id).rounded())
        HStack(spacing: 4) {
            ForEach(0..<labels.count, id: \.self) { i in
                TapButton(title: labels[i], on: cur == i, iris: iris) { engine.set(id, Double(i)) }
            }
        }
    }
}

/// On/off switch parameter.
struct Toggle1: View {
    @EnvironmentObject var engine: IkiEngine
    let id: Int32
    let title: String
    var iris = false
    var body: some View {
        let _ = engine.frame
        TapButton(title: title, on: engine.get(id) > 0.5, iris: iris) {
            engine.set(id, engine.get(id) > 0.5 ? 0 : 1)
        }
    }
}

/// Pitch-bend strip: drag horizontally, springs back to centre on release.
struct BendStrip: View {
    @EnvironmentObject var engine: IkiEngine
    @Environment(\.palette) private var pal
    @State private var dragging = false
    var body: some View {
        let _ = engine.frame
        GeometryReader { geo in
            let w = geo.size.width
            let v = engine.get(P_BEND)
            ZStack(alignment: .leading) {
                RoundedRectangle(cornerRadius: 10).fill(pal.surface2)
                RoundedRectangle(cornerRadius: 10).stroke(pal.border)
                Rectangle().fill(pal.border).frame(width: 2).position(x: w / 2, y: geo.size.height / 2)
                Circle().fill(pal.accent(true))
                    .frame(width: 30, height: 30)
                    .shadow(color: Color.black.opacity(0.3), radius: 2, y: 1)
                    .position(x: 15 + CGFloat(v) * (w - 30), y: geo.size.height / 2)
            }
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { g in
                        dragging = true
                        engine.set(P_BEND, min(1, max(0, Double((g.location.x - 15) / (w - 30)))))
                    }
                    .onEnded { _ in
                        dragging = false
                        withAnimation(.interpolatingSpring(stiffness: 220, damping: 18)) {}
                        engine.set(P_BEND, 0.5)
                    }
            )
        }
        .frame(height: 40)
    }
}

/// The flute: a bamboo tube with five finger holes that follow the current fingering and glow with the breath.
struct FluteView: View {
    @EnvironmentObject var engine: IkiEngine
    @Environment(\.palette) private var pal
    var body: some View {
        let _ = engine.frame
        let holes = Self.parse(engine.text(TEXT_HOLES))
        let breath = CGFloat(min(1, max(0, engine.s(STATE_BREATH))))
        Canvas { ctx, size in
            let h: CGFloat = 26
            let r = CGRect(x: 6, y: (size.height - h) / 2, width: size.width - 12, height: h)
            let tube = Path(roundedRect: r, cornerRadius: h / 2)
            ctx.fill(tube, with: .linearGradient(
                Gradient(colors: [pal.metal, pal.surface2, pal.metal]),
                startPoint: CGPoint(x: 0, y: r.minY), endPoint: CGPoint(x: 0, y: r.maxY)))
            ctx.stroke(tube, with: .color(pal.border), lineWidth: 1)
            // utaguchi (blowing edge)
            ctx.fill(Path(ellipseIn: CGRect(x: r.minX + 8, y: r.midY - 5, width: 18, height: 10)),
                     with: .color(pal.hole.opacity(0.85)))
            // breath glow
            ctx.fill(tube, with: .color(pal.accent(false).opacity(0.28 * Double(breath))))
            for i in 0..<5 {
                let x = r.minX + r.width * (0.30 + 0.14 * CGFloat(i))
                let d: CGFloat = 13
                let c = CGRect(x: x - d / 2, y: r.midY - d / 2, width: d, height: d)
                let covered = i < holes.count ? holes[i] : false
                ctx.fill(Path(ellipseIn: c), with: .color(covered ? pal.hole : pal.bg))
                ctx.stroke(Path(ellipseIn: c), with: .color(pal.border), lineWidth: 1)
            }
            // joint bands
            for f in [0.22, 0.88] as [CGFloat] {
                ctx.fill(Path(CGRect(x: r.minX + r.width * f, y: r.minY, width: 3, height: h)),
                         with: .color(pal.border))
            }
        }
        .frame(height: 40)
        .animation(.easeOut(duration: Motion.hole), value: holes)
    }

    static func parse(_ text: String) -> [Bool] {
        text.filter { $0 == "●" || $0 == "○" }.prefix(5).map { $0 == "●" }
    }
}

/// Dark LCD-style readout.
struct Readout: View {
    @Environment(\.palette) private var pal
    let text: String
    var size: CGFloat = 13
    var body: some View {
        Text(text)
            .font(.system(size: size, weight: .medium, design: .monospaced))
            .foregroundColor(pal.fg)
            .lineLimit(1).minimumScaleFactor(0.6)
            .padding(.horizontal, 8).padding(.vertical, 5)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: 8).fill(pal.lcd))
            .overlay(RoundedRectangle(cornerRadius: 8).stroke(pal.border))
    }
}

/// A graph of a [0…1] curve (envelope / filter response).
struct CurveView: View {
    @EnvironmentObject var engine: IkiEngine
    @Environment(\.palette) private var pal
    let which: Int32
    var iris = false
    var body: some View {
        let _ = engine.frame
        let pts = engine.curve(which, points: 96)
        Canvas { ctx, size in
            guard pts.count > 1 else { return }
            var p = Path()
            for (i, y) in pts.enumerated() {
                let pt = CGPoint(x: size.width * CGFloat(i) / CGFloat(pts.count - 1),
                                 y: size.height * (1 - CGFloat(min(1, max(0, y)))))
                if i == 0 { p.move(to: pt) } else { p.addLine(to: pt) }
            }
            ctx.stroke(p, with: .color(pal.accent(iris)), style: StrokeStyle(lineWidth: 2, lineJoin: .round))
        }
        .frame(height: 54)
        .background(RoundedRectangle(cornerRadius: 8).fill(pal.lcd))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(pal.border))
    }
}

/// UIActivityViewController wrapper (share sheet) — ShareLink needs iOS 16.
struct ShareSheet: UIViewControllerRepresentable {
    let url: URL
    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: [url], applicationActivities: nil)
    }
    func updateUIViewController(_ vc: UIActivityViewController, context: Context) {}
}
