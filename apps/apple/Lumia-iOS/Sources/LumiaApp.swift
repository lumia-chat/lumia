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

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(store)
                .background(LumiaTokens.bg.color)
        }
    }
}

struct RootView: View {
    @Environment(AppStore.self) private var store

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
    }
}
