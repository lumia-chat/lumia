// The app's one session and where the app is: the saved instance
// reopened at launch (state under the app's Application Support), and
// the route the session's state picks.
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

    /// The app group the iOS app and its notification extension share
    /// (architecture 15.5): the stores live in its container so both can
    /// open them, under the single-writer lock.
    public static let appGroup = "group.chat.lumia"

    public nonisolated static let dataDirectory: URL = {
        #if os(iOS)
        if let group = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: appGroup) {
            return group.appendingPathComponent("Lumia", isDirectory: true)
        }
        #endif
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return base.appendingPathComponent("Lumia", isDirectory: true)
    }()

    /// The base URL an instance was opened with, by host, for the
    /// notification extension (a push names the instance's host only).
    public nonisolated static func baseURL(forHost host: String) -> URL? {
        let defaults = UserDefaults(suiteName: appGroup) ?? .standard
        let map = defaults.dictionary(forKey: "instances") as? [String: String] ?? [:]
        return map[host].flatMap(URL.init(string:))
    }

    private static func remember(host: String, baseURL: URL) {
        let defaults = UserDefaults(suiteName: appGroup) ?? .standard
        var map = defaults.dictionary(forKey: "instances") as? [String: String] ?? [:]
        map[host] = baseURL.absoluteString
        defaults.set(map, forKey: "instances")
    }

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
        // (apps/apple/scripts/smoke-ios.sh): LUMIA_DEV_INSTANCE, _HANDLE,
        // _PASSWORD.
        let env = ProcessInfo.processInfo.environment
        if let instance = env["LUMIA_DEV_INSTANCE"], let url = URL(string: instance),
           let handle = env["LUMIA_DEV_HANDLE"], let password = env["LUMIA_DEV_PASSWORD"] {
            do {
                let s = try await open(url)
                if !s.signedIn { try await s.signIn(handle: handle, password: password, totpCode: nil) }
                // LUMIA_DEV_LINK=1 links this device through files in the data
                // directory: the request goes to link-request.txt, and the code
                // the approving device gives is read from link-code.txt. The
                // app is on screen meanwhile, as it is for someone who signed
                // in with a password and links from Devices: the gateway loop
                // is running on the password's session when the claim signs
                // in by identity proof and that session is revoked.
                if env["LUMIA_DEV_LINK"] == "1", !s.hasIdentity {
                    route = .app
                    opening = false
                    let request = try await s.identityLinkRequest(label: "phone")
                    let dir = Self.dataDirectory
                    try request.write(to: dir.appendingPathComponent("link-request.txt"), atomically: true, encoding: .utf8)
                    let codeFile = dir.appendingPathComponent("link-code.txt")
                    for _ in 0..<120 {
                        if let code = try? String(contentsOf: codeFile, encoding: .utf8).trimmingCharacters(in: .whitespacesAndNewlines), !code.isEmpty {
                            try await s.identityClaim(code: code)
                            break
                        }
                        try await Task.sleep(for: .milliseconds(500))
                    }
                }
                // LUMIA_DEV_SCREEN=notify stands in for the notification
                // extension, which the simulator's push injection never
                // launches: the writer lock is released as in the background,
                // the extension's call runs here, and its answer goes to
                // notification.txt in the data directory ("none" for the
                // generic notice, "nothing" for no notice at all).
                if env["LUMIA_DEV_SCREEN"] == "notify" {
                    await s.client.background()
                    let client = s.client
                    let notice = await Task.detached { client.notice(timeoutMs: 3000) }.value
                    let text = switch notice {
                    case let .message(c): "\(c.title)\n\(c.body)\n\(c.channelId)\n\(c.kind)"
                    case .generic: "none"
                    case .nothing: "nothing"
                    }
                    try? text.write(to: Self.dataDirectory.appendingPathComponent("notification.txt"), atomically: true, encoding: .utf8)
                }
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
        Self.remember(host: s.host, baseURL: url)
        return s
    }

    /// A sign-out the person chose is on its way: the app shows nothing of
    /// the account meanwhile, so nothing can be asked of a session that is ending.
    public private(set) var signingOut = false

    /// Signs out (`LumiaSession.signOut`) and shows the sign-in screen. The
    /// saved instance stays, so that screen opens on it. A second tap while
    /// the first is on its way does nothing.
    public func signOut() async {
        guard !signingOut else { return }
        signingOut = true
        defer { signingOut = false }
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
