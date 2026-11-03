// Push on the phone (architecture 15.5): the app registers with APNs,
// hands the device token to the instance through the core as the
// `apns` subscription, and the notification extension (Notifications/)
// turns the generic notice into the message. The token arrives through
// the application delegate, which SwiftUI adapts here.
import LumiaKit
import SwiftUI
import UserNotifications

@MainActor
@Observable
final class PushRegistrar {
    static let shared = PushRegistrar()
    /// The APNs device token as hex, once iOS has one.
    private(set) var token: String?
    private var subscribed: String?

    /// Asks for permission and a token; both are best effort.
    func register() {
        #if DEBUG
        // The screen smoke (scripts/smoke-ios.sh) screenshots screens; the
        // permission prompt would cover them.
        if ProcessInfo.processInfo.environment["LUMIA_DEV_NO_PUSH"] == "1" { return }
        #endif
        Task {
            let center = UNUserNotificationCenter.current()
            _ = try? await center.requestAuthorization(options: [.alert, .sound, .badge])
            UIApplication.shared.registerForRemoteNotifications()
        }
    }

    func received(_ deviceToken: Data) {
        token = deviceToken.map { String(format: "%02x", $0) }.joined()
    }

    /// Sends the token to the instance once per token and session.
    func subscribe(_ session: LumiaSession) async {
        guard let token, subscribed != token + session.host else { return }
        subscribed = token + session.host
        await session.pushSubscribe(platform: "apns", token: token)
    }
}

final class PushDelegate: NSObject, UIApplicationDelegate {
    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        Task { @MainActor in PushRegistrar.shared.received(deviceToken) }
    }

    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
        // No token (no entitlement in a debug build, or no network): the
        // instance keeps no subscription and the app shows messages itself.
    }
}
