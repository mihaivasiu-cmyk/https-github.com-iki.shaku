import SwiftUI

@main
struct IkiApp: App {
    @StateObject private var engine = IkiEngine()
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(engine)
                .onAppear { engine.start() }
        }
        .onChange(of: phase) { p in
            // Stop the audio graph and the display link in the background; resume on return.
            engine.setPaused(p != .active)
        }
    }
}
