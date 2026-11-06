// A sign-in through the system browser (architecture 6.3): the instance's
// OIDC provider, or its web sign-in page for a passkey, in an
// ASWebAuthenticationSession that comes back to the app on the `lumia:`
// scheme with a one-time token. Platform passkeys through
// AuthenticationServices would need the instance's host among the app's
// associated domains, which an app for any instance cannot carry; the page
// in the sheet is Safari's own WebAuthn, with no such tie.
import AuthenticationServices
import Foundation

public enum BrowserSheetError: Error {
    /// The person dismissed the sheet.
    case cancelled
    case badURL
}

@MainActor
public final class BrowserSheet: NSObject {
    public static let scheme = "lumia"
    /// Where a sign-in made in the sheet returns to, with `magic={token}`.
    public static let signInReturn = "lumia://sign-in"
    /// Where the web page returns to once done with a handed-off session.
    public static let doneReturn = "lumia://done"

    private var session: ASWebAuthenticationSession?
    // The sheet holds its provider weakly; this keeps the running one alive.
    private static var current: BrowserSheet?

    /// Opens `url` and resolves with the URL the sheet came back on.
    /// `ephemeral` keeps nothing of the page once the sheet closes (the
    /// web sign-in page); the provider's own session is kept otherwise.
    public func run(_ url: URL, ephemeral: Bool) async throws -> URL {
        Self.current = self
        defer { Self.current = nil }
        return try await withCheckedThrowingContinuation { continuation in
            let s = ASWebAuthenticationSession(url: url, callbackURLScheme: Self.scheme) { back, error in
                if let back {
                    continuation.resume(returning: back)
                } else if let error = error as? ASWebAuthenticationSessionError, error.code == .canceledLogin {
                    continuation.resume(throwing: BrowserSheetError.cancelled)
                } else {
                    continuation.resume(throwing: error ?? BrowserSheetError.cancelled)
                }
            }
            s.prefersEphemeralWebBrowserSession = ephemeral
            s.presentationContextProvider = self
            session = s
            s.start()
        }
    }

    private func anchor() -> ASPresentationAnchor {
        #if os(macOS)
        return NSApplication.shared.keyWindow ?? NSApplication.shared.windows.first ?? ASPresentationAnchor()
        #else
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        return scenes.flatMap(\.windows).first { $0.isKeyWindow } ?? scenes.first?.windows.first ?? ASPresentationAnchor()
        #endif
    }
}

extension BrowserSheet: ASWebAuthenticationPresentationContextProviding {
    public nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        MainActor.assumeIsolated { anchor() }
    }
}
