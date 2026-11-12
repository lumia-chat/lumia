// The notification service extension (architecture 15.5): a push arrives
// as the instance's generic notice with mutable content; this process
// opens the same stores as the app (the app-group container), takes the
// single-writer lock, syncs and decrypts through the core, and rewrites
// the notice into the message. Anything that fails leaves the generic
// text, which is still a notification. A push for an account this phone
// holds no session of (signed out, or the session ended by the instance,
// with a subscription that outlived it) becomes no notification: the
// content handler is given empty content, which the system drops only for
// an extension with the `com.apple.developer.usernotifications.filtering`
// entitlement, which Apple grants on request; without it the generic text
// is shown, as before.
import LumiaFFI
import LumiaKit
import UserNotifications

final class NotificationService: UNNotificationServiceExtension {
    private var handler: ((UNNotificationContent) -> Void)?
    private var attempt: UNMutableNotificationContent?

    override func didReceive(_ request: UNNotificationRequest, withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void) {
        handler = contentHandler
        let attempt = (request.content.mutableCopy() as? UNMutableNotificationContent) ?? UNMutableNotificationContent()
        self.attempt = attempt
        guard let instance = request.content.userInfo["instance"] as? String, !instance.isEmpty else {
            contentHandler(attempt)
            return
        }
        Task.detached { [weak self] in
            let notice = Self.notice(forInstance: instance)
            await MainActor.run {
                guard let self, let handler = self.handler else { return }
                self.handler = nil
                switch notice {
                case let .message(content):
                    attempt.title = content.title
                    attempt.body = content.body
                    attempt.threadIdentifier = content.channelId
                    attempt.userInfo["channel_id"] = content.channelId
                    attempt.userInfo["kind"] = content.kind
                    handler(attempt)
                case .generic:
                    handler(attempt)
                case .nothing:
                    handler(UNNotificationContent())
                }
            }
        }
    }

    override func serviceExtensionTimeWillExpire() {
        // Out of time: the generic notice goes out as it came.
        if let handler, let attempt {
            self.handler = nil
            handler(attempt)
        }
    }

    /// What the push becomes (`LumiaClient.notice`): the message, the
    /// generic notice, or nothing for an account the phone is not signed in to.
    private static func notice(forInstance host: String) -> PushNotice {
        let url = AppStore.baseURL(forHost: host)
            ?? AppStore.baseURL(forHost: host.replacingOccurrences(of: ":", with: "_"))
            ?? URL(string: "https://\(host)")
        guard let url, let client = try? LumiaClient(baseURL: url, dataDirectory: AppStore.dataDirectory) else { return .generic }
        // The extension has about 30 seconds; the lock wait is a small part
        // of it, since an app holding it is live and shows the message itself.
        return client.notice(timeoutMs: 3000)
    }
}
