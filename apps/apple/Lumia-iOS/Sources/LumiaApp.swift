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
            if store.opening {
                ProgressView().controlSize(.large)
            } else if devScreen == "signin" {
                SignInView()
            } else if devScreen == "link", let session = store.session {
                LinkView(session: session)
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
        // Light or dark for every screen, sign-in and linking included, and
        // for what the system draws over them (a sheet, a menu, the
        // keyboard); nil is the phone's own, followed as it changes.
        .preferredColorScheme(preferences.appearance.colorScheme)
    }
}
