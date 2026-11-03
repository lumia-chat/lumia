// The macOS app: one window on the Quiet Split, the saved instance
// reopened at launch (state under Application Support/Lumia), and the
// screens routed by what the session knows: no instance, not signed in,
// linking, or the app.
import LumiaKit
import SwiftUI

@main
struct LumiaApp: App {
    @State private var store = AppStore()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(store)
                .frame(minWidth: 900, minHeight: 560)
                .background(LumiaTokens.bg.color)
        }
        .windowStyle(.hiddenTitleBar)
        .commands {
            CommandGroup(replacing: .newItem) {}
            CommandMenu("Account") {
                Button("Sign out") { Task { await store.signOut() } }
                    .disabled(store.session?.signedIn != true)
                Button("Change instance") { Task { await store.forgetInstance() } }
                    .disabled(store.session == nil)
            }
        }
    }
}

/// What the window shows.
enum Route: Equatable {
    case signIn, link, app
}

/// The app's one session and where it is.
@MainActor
@Observable
final class AppStore {
    private(set) var session: LumiaSession?
    var route: Route = .signIn
    private(set) var opening = true
    private(set) var openError: String?

    static let dataDirectory: URL = {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return base.appendingPathComponent("Lumia", isDirectory: true)
    }()

    var savedInstance: String? {
        get { UserDefaults.standard.string(forKey: "instance") }
        set { UserDefaults.standard.set(newValue, forKey: "instance") }
    }

    init() {
        Task { await reopen() }
    }

    /// Reopens the saved instance at launch, straight into the app when signed in.
    func reopen() async {
        defer { opening = false }
        #if DEBUG
        // A debug build signs in from the environment, for the smoke run
        // (apps/apple/scripts/smoke.sh): LUMIA_DEV_INSTANCE, _HANDLE, _PASSWORD.
        let env = ProcessInfo.processInfo.environment
        if let instance = env["LUMIA_DEV_INSTANCE"], let url = URL(string: instance),
           let handle = env["LUMIA_DEV_HANDLE"], let password = env["LUMIA_DEV_PASSWORD"] {
            do {
                let s = try await open(url)
                if !s.signedIn { try await s.signIn(handle: handle, password: password, totpCode: nil) }
                route = .app
            } catch {
                openError = error.lumiaMessage
            }
            return
        }
        #endif
        guard let saved = savedInstance, let url = URL(string: saved) else { return }
        do {
            let s = try await open(url)
            route = s.signedIn ? .app : .signIn
        } catch {
            openError = error.lumiaMessage
        }
    }

    @discardableResult
    func open(_ url: URL) async throws -> LumiaSession {
        session?.stop()
        let client = try LumiaClient(baseURL: url, dataDirectory: Self.dataDirectory)
        let s = await LumiaSession(client: client)
        session = s
        savedInstance = url.absoluteString
        return s
    }

    func signOut() async {
        await session?.signOut()
        route = .signIn
    }

    func forgetInstance() async {
        session?.stop()
        session = nil
        savedInstance = nil
        route = .signIn
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
