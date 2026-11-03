// The macOS app: one window on the Quiet Split, the saved instance
// reopened at launch by the shared AppStore (LumiaKit), and the screens
// routed by what the session knows: no instance, not signed in, linking,
// or the app.
import LumiaKit
import Sparkle
import SwiftUI

@main
struct LumiaApp: App {
    @State private var store = AppStore()
    // Sparkle, for the direct-download build: checks the appcast in
    // Info.plist (SUFeedURL) once a day unless turned off below, and
    // installs what the user accepts. Nothing else in the app updates
    // itself.
    private let updater = SPUStandardUpdaterController(startingUpdater: true, updaterDelegate: nil, userDriverDelegate: nil)

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(store)
                .frame(minWidth: 1000, minHeight: 600)
                .background(LumiaTokens.bg.color)
                .task { await DevShot.run() }
        }
        .windowStyle(.hiddenTitleBar)
        .commands {
            CommandGroup(replacing: .newItem) {}
            CommandGroup(after: .appInfo) {
                UpdateCommands(updater: updater.updater)
            }
            CommandMenu("Account") {
                Button("Sign out") { Task { await store.signOut() } }
                    .disabled(store.session?.signedIn != true)
                Button("Change instance") { Task { await store.forgetInstance() } }
                    .disabled(store.session == nil)
            }
        }
    }
}

/// The update items in the application menu: a manual check, and the
/// switch for the daily automatic one (the only time the app contacts
/// anything but the user's instances, so it is the user's to turn off).
struct UpdateCommands: View {
    @State private var model: UpdateModel

    init(updater: SPUUpdater) {
        _model = State(initialValue: UpdateModel(updater: updater))
    }

    var body: some View {
        Button("Check for updates…") { model.updater.checkForUpdates() }
            .disabled(!model.canCheck)
        Toggle("Check for updates automatically", isOn: Binding(
            get: { model.updater.automaticallyChecksForUpdates },
            set: { model.updater.automaticallyChecksForUpdates = $0 }
        ))
    }
}

@MainActor
@Observable
final class UpdateModel {
    let updater: SPUUpdater
    private(set) var canCheck = false
    @ObservationIgnored private var observation: NSKeyValueObservation?

    init(updater: SPUUpdater) {
        self.updater = updater
        canCheck = updater.canCheckForUpdates
        observation = updater.observe(\.canCheckForUpdates, options: [.new]) { [weak self] _, change in
            let value = change.newValue ?? false
            Task { @MainActor in self?.canCheck = value }
        }
    }
}

/// A debug build writes the window as a PNG to LUMIA_DEV_SHOT a few
/// seconds after launch (scripts/smoke.sh), drawn from the view hierarchy
/// itself, which needs no screen-recording permission.
enum DevShot {
    static func run() async {
        #if DEBUG
        guard let path = ProcessInfo.processInfo.environment["LUMIA_DEV_SHOT"] else { return }
        let delay = Double(ProcessInfo.processInfo.environment["LUMIA_DEV_SHOT_AFTER"] ?? "") ?? 8
        try? await Task.sleep(for: .seconds(delay))
        guard let window = NSApp.windows.first(where: { $0.isVisible }), let view = window.contentView,
              let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return }
        view.cacheDisplay(in: view.bounds, to: rep)
        if let data = rep.representation(using: .png, properties: [:]) {
            try? data.write(to: URL(fileURLWithPath: path))
        }
        #endif
    }
}

struct RootView: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        Group {
            if store.opening {
                ProgressView().controlSize(.large)
            } else if store.route == .app, let session = store.session, session.signedIn {
                ShellView(session: session)
            } else if store.route == .link, let session = store.session {
                LinkView(session: session)
            } else {
                SignInView()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(LumiaTokens.bg.color)
    }
}
