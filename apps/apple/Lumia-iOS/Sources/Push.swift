// Push on the phone (architecture 15.5): the app registers with APNs,
// hands the device token to the instance through the core as the
// `apns` subscription, and the notification extension (Notifications/)
// turns the generic notice into the message. The token and the tapped
// notification arrive through the application delegate, which SwiftUI
// adapts here; the shell opens the conversation a tap names.
import LumiaKit
import SwiftUI
import UserNotifications
import os

/// Where a tapped notification points: the extension put the channel and
/// the kind (`dm` or `mention`) in the content; the generic notice has
/// neither and opens the app where it was.
struct PushTarget: Equatable {
    let host: String
    let channelID: String
    let kind: String

    init?(userInfo: [AnyHashable: Any]) {
        guard let channel = userInfo["channel_id"] as? String, !channel.isEmpty else { return nil }
        host = userInfo["instance"] as? String ?? ""
        channelID = channel
        kind = userInfo["kind"] as? String ?? "dm"
    }
}

@MainActor
@Observable
final class PushRegistrar {
    static let shared = PushRegistrar()
    /// The APNs device token as hex, once iOS has one.
    private(set) var token: String?
    /// The notification the person tapped last, until the shell opens it.
    var target: PushTarget?
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

final class PushDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        // Set before launch finishes, so a tap that started the app is delivered too.
        UNUserNotificationCenter.current().delegate = self
        return true
    }

    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        Task { @MainActor in PushRegistrar.shared.received(deviceToken) }
    }

    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
        // No token (no entitlement in a debug build, or no network): the
        // instance keeps no subscription and the app shows messages itself.
    }

    /// In the foreground the banner still shows: the person may be in
    /// another conversation, and the extension's rewrite is what it says.
    func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        Self.log.info("presenting \(notification.request.content.title, privacy: .public) in the foreground")
        return [.banner, .list, .sound]
    }

    private static let log = Logger(subsystem: "chat.lumia", category: "push")

    func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        guard response.actionIdentifier == UNNotificationDefaultActionIdentifier else { return }
        let target = PushTarget(userInfo: response.notification.request.content.userInfo)
        await MainActor.run { PushRegistrar.shared.target = target }
    }
}
