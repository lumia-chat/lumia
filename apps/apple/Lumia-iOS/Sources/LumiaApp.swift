// The iOS app: the Quiet Split folded for a phone (the design canvas's
// iOS boards). The saved instance is reopened at launch by the shared
// AppStore (LumiaKit), and the screens are routed by what the session
// knows: no instance, not signed in, linking, or the app.
import LumiaKit
import SwiftUI

@main
struct LumiaApp: App {
    @UIApplicationDelegateAdaptor(PushDelegate.self) private var pushDelegate
    @State private var store = AppStore()
    // This phone's choices (Preferences): outside the store, since they are
    // not the account's and signing out leaves them be.
    @State private var preferences = Preferences()

    init() {
        // What a run that ended with something playing or being shared left behind.
        MediaFiles.clear()
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(store)
                .environment(preferences)
                .background(LumiaTokens.bg.color)
        }
    }
}

struct RootView: View {
    @Environment(AppStore.self) private var store
    @Environment(Preferences.self) private var preferences

    #if DEBUG
    // The smoke run screenshots the sign-in and link screens past the
    // environment's sign-in (scripts/smoke-ios.sh, LUMIA_DEV_SCREEN).
    private var devScreen: String? { ProcessInfo.processInfo.environment["LUMIA_DEV_SCREEN"] }
    #else
    private var devScreen: String? { nil }
    #endif

    var body: some View {
        Group {
            #if DEBUG
            if let devScreen, devScreen.hasPrefix("bodies") {
                // No instance: message bodies alone (DevScenes.swift).
                BodiesScene(part: devScreen)
            } else if devScreen == "reactions" {
                ReactionsScene()
            } else if devScreen == "reactions-preferences" {
                PreferencesScreen()
            } else if devScreen == "profile-updated" {
                ProfileUpdatedScene()
            } else if devScreen == "tags" {
                // The "Tags" board's figures: every tone, and the longest a tag can be.
                TagsScene()
            } else if let devScreen, devScreen.hasPrefix("linking-") || devScreen.hasPrefix("linkprompt-") {
                // Linking by a prompt's every stage, drawn from values.
                LinkStagesScene(part: devScreen)
            } else if let devScreen, devScreen.hasPrefix("restore-") {
                RestoreStepsScene(part: devScreen)
            } else if devScreen == "removed" {
                SignInView(removed: true)
            } else if let devScreen, devScreen.hasPrefix("viewer") {
                // The viewer's picture, large and small, for its zoom.
                ViewerScene(part: devScreen)
            } else {
                screen
            }
            #else
            screen
            #endif
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(LumiaTokens.bg.color)
        // Light or dark for every screen, sign-in and linking included, and
        // for what the system draws over them (a sheet, a menu, the
        // keyboard); nil is the phone's own, followed as it changes.
        .preferredColorScheme(preferences.appearance.colorScheme)
        #if DEBUG
        // The simulator's `simctl push` refuses a silent push, so a smoke
        // stands in for the read wake with `lumia://dev/read-push?badge=N`:
        // the same handling, without the system's delivery.
        .onOpenURL { url in
            guard url.host() == "dev", url.path() == "/read-push" else { return }
            let badge = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?.first { $0.name == "badge" }?.value.flatMap(Int.init)
            Task {
                // Opened cold, as a wake would find it: once the saved instance is open.
                for _ in 0..<100 where store.opening { try? await Task.sleep(for: .milliseconds(100)) }
                await PushRegistrar.shared.readPush(badge: badge, host: store.session?.host)
            }
        }
        #endif
        .onChange(of: store.session?.signedIn) { _, signedIn in
            if signedIn == false { PushRegistrar.shared.signedOut() }
        }
        // What a push arriving in the foreground is shown by; from launch
        // on, so one that lands before any session is opened is not shown.
        .onChange(of: store.session?.signedIn == true && !store.signingOut, initial: true) { _, held in
            PushRegistrar.shared.signedIn = held
        }
    }

    @ViewBuilder private var screen: some View {
        // Opening, or signing out: once Sign out is tapped nothing of the
        // account is on screen, so there is no second tap and nothing to
        // ask of a session that is ending.
        if store.opening || store.signingOut {
            ProgressView().controlSize(.large)
        } else if devScreen == "signin" {
            SignInView()
        } else if devScreen == "link", let session = store.session, !session.hasIdentity {
            // Once linked it goes on into the app, as the route does.
            LinkView(session: session)
        } else if store.route == .app, let session = store.session, session.signedIn {
            ShellView(session: session)
        } else if store.route == .link, let session = store.session {
            LinkView(session: session)
        } else if store.route == .restore, let session = store.session {
            // From the sign-in screen: restoring signs in by the identity.
            RestoreScreen(session: session, back: "Sign in", close: { store.route = .signIn }, link: { store.route = .link }) { store.route = .app }
        } else {
            SignInView()
        }
    }
}
