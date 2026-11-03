// The app's one session and where the app is, shared by the macOS and
// iOS layouts: the saved instance reopened at launch (state under the
// app's Application Support), and the route the session's state picks.
import Foundation
import Observation

/// What the app shows.
public enum Route: Equatable, Sendable {
    case signIn, link, app
}

@MainActor
@Observable
public final class AppStore {
    public private(set) var session: LumiaSession?
    public var route: Route = .signIn
    public private(set) var opening = true
    public private(set) var openError: String?

    public static let dataDirectory: URL = {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return base.appendingPathComponent("Lumia", isDirectory: true)
    }()

    public var savedInstance: String? {
        get { UserDefaults.standard.string(forKey: "instance") }
        set { UserDefaults.standard.set(newValue, forKey: "instance") }
    }

    public init() {
        Task { await reopen() }
    }

    /// Reopens the saved instance at launch, straight into the app when signed in.
    public func reopen() async {
        defer { opening = false }
        #if DEBUG
        // A debug build signs in from the environment, for the smoke runs
        // (apps/apple/scripts/smoke.sh, smoke-ios.sh): LUMIA_DEV_INSTANCE,
        // _HANDLE, _PASSWORD.
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
    public func open(_ url: URL) async throws -> LumiaSession {
        session?.stop()
        let client = try LumiaClient(baseURL: url, dataDirectory: Self.dataDirectory)
        let s = await LumiaSession(client: client)
        session = s
        savedInstance = url.absoluteString
        return s
    }

    public func signOut() async {
        await session?.signOut()
        route = .signIn
    }

    public func forgetInstance() async {
        session?.stop()
        session = nil
        savedInstance = nil
        route = .signIn
    }
}
