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
    /// Whether the app holds a signed-in session now: a push that arrives
    /// in the foreground without one is for an account the phone does not
    /// hold, and is not shown (`PushDelegate`'s `willPresent`).
    var signedIn = false

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

    /// Offers the token to the session, which sends it to the instance once
    /// per value and forgets that at sign-out (`PushLedger` in LumiaKit), so
    /// this may be asked whenever there is a token and a session.
    func subscribe(_ session: LumiaSession) async {
        guard let token else { return }
        await session.pushSubscribe(token: token)
    }

    /// The account has left this phone, signed out or sent back to sign-in.
    /// The session has taken the subscription back from the instance where
    /// it still could (`LumiaSession.signOut`); here the phone stops taking
    /// pushes at all, which also covers the times it could not (no network,
    /// or a session the instance ended), as the web drops the browser's own
    /// subscription beside the instance's. The token is forgotten with it:
    /// the next sign-in registers again and subscribes with what the system
    /// then gives. A notification tapped for the account opens nothing for
    /// whoever signs in next, and those already delivered go from
    /// Notification Center: each names a conversation, and after the
    /// extension's rewrite who wrote and what, of an account this phone no
    /// longer holds. All of them, since the phone holds one account and
    /// every notification the app has is that account's; the app schedules
    /// none of its own, so there are none waiting to be removed.
    func signedOut() {
        target = nil
        token = nil
        session = nil
        UIApplication.shared.unregisterForRemoteNotifications()
        UNUserNotificationCenter.current().removeAllDeliveredNotifications()
        UNUserNotificationCenter.current().setBadgeCount(0)
    }

    // MARK: Read state (architecture 6.6 and 15.5)

    /// The signed-in session, while the shell is on screen: a read push asks it for the counts.
    weak var session: LumiaSession?

    /// The counts changed in the app (a snapshot, a message, a mark here or
    /// another device's read through the account log): the badge follows
    /// them, and the notifications of the conversations now read go.
    func readsChanged(before: ReadCounts, after: ReadCounts) {
        let center = UNUserNotificationCenter.current()
        center.setBadgeCount(Int(after.badge))
        let read = ReadNotifications.newlyRead(before: before, after: after)
        guard !read.isEmpty else { return }
        Task {
            let ids = await center.deliveredNotifications().filter { read.contains(Self.channel(of: $0) ?? "") }.map(\.request.identifier)
            if !ids.isEmpty {
                Self.log.info("removing \(ids.count) delivered notifications of conversations read")
                center.removeDeliveredNotifications(withIdentifiers: ids)
            }
        }
    }

    /// A read push (`"kind": "read"`, silent, content-available): the
    /// account read something on another device. The badge becomes what it
    /// carries, and the notifications already delivered of conversations
    /// with nothing unread now go, by the counts a snapshot gives; every one
    /// of them when the badge is 0, since then nothing that notifies is unread.
    func readPush(badge: Int?, host: String?) async {
        let center = UNUserNotificationCenter.current()
        if let badge { try? await center.setBadgeCount(badge) }
        let states = await counts(host: host)
        let delivered = await center.deliveredNotifications().map { (id: $0.request.identifier, channelID: Self.channel(of: $0)) }
        // No badge in the push: only the counts decide.
        let ids = ReadNotifications.toRemove(delivered: delivered, states: states, badge: badge.map { UInt32(max(0, $0)) } ?? 1)
        Self.log.info("read push: badge \(badge ?? -1), \(ids.count) of \(delivered.count) delivered notifications removed")
        if !ids.isEmpty { center.removeDeliveredNotifications(withIdentifiers: ids) }
    }

    /// The account's counts as the instance has them now: through the
    /// session the app holds, or, woken in the background with none, a
    /// client opened on the same stores as the notification extension does.
    private func counts(host: String?) async -> ReadCounts? {
        if let session, session.signedIn {
            await session.refreshSnapshot()
            return session.reads
        }
        guard let host,
              let url = AppStore.baseURL(forHost: host) ?? AppStore.baseURL(forHost: host.replacingOccurrences(of: ":", with: "_")),
              let client = try? LumiaClient(baseURL: url, dataDirectory: AppStore.dataDirectory),
              await client.signedIn,
              let snap = try? await client.snapshot() else { return nil }
        return ReadCounts(snap.readStates)
    }

    /// The conversation a delivered notification is of, as the extension wrote it.
    private static func channel(of n: UNNotification) -> String? {
        let content = n.request.content
        if let id = content.userInfo["channel_id"] as? String, !id.isEmpty { return id }
        return content.threadIdentifier.isEmpty ? nil : content.threadIdentifier
    }

    private static let log = Logger(subsystem: "chat.lumia", category: "push")
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

    /// A background push: the read wake (architecture 15.5), which shows nothing.
    func application(_ application: UIApplication, didReceiveRemoteNotification userInfo: [AnyHashable: Any]) async -> UIBackgroundFetchResult {
        guard userInfo["kind"] as? String == "read" else { return .noData }
        let badge = (userInfo["aps"] as? [String: Any])?["badge"] as? Int
        let host = userInfo["instance"] as? String
        await PushRegistrar.shared.readPush(badge: badge, host: host)
        return .newData
    }

    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
        // No token (no entitlement in a debug build, or no network): the
        // instance keeps no subscription and the app shows messages itself.
    }

    /// In the foreground the banner still shows: the person may be in
    /// another conversation, and the extension's rewrite is what it says.
    /// Not while signed out (on the sign-in screen, or signing out): the
    /// push is for an account the phone no longer holds, so nothing is
    /// shown, listed or sounded, as the extension posts nothing then.
    func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        guard await PushRegistrar.shared.signedIn else {
            Self.log.info("not presenting a notification: signed out")
            return []
        }
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
