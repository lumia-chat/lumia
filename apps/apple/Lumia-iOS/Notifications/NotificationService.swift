// The notification service extension (architecture 15.5): a push arrives
// as the instance's generic notice with mutable content; this process
// opens the same stores as the app (the app-group container), takes the
// single-writer lock, syncs and decrypts through the core, and rewrites
// the notice into the message. Anything that fails leaves the generic
// text, which is still a notification.
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
            let content = Self.content(forInstance: instance)
            await MainActor.run {
                guard let self, let handler = self.handler else { return }
                if let content {
                    attempt.title = content.title
                    attempt.body = content.body
                    attempt.threadIdentifier = content.channelId
                    attempt.userInfo["channel_id"] = content.channelId
                    attempt.userInfo["kind"] = content.kind
                }
                self.handler = nil
                handler(attempt)
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

    /// The message behind the push, or nil to keep the generic notice.
    private static func content(forInstance host: String) -> NotificationContent? {
        let url = AppStore.baseURL(forHost: host)
            ?? AppStore.baseURL(forHost: host.replacingOccurrences(of: ":", with: "_"))
            ?? URL(string: "https://\(host)")
        guard let url, let client = try? LumiaClient(baseURL: url, dataDirectory: AppStore.dataDirectory) else { return nil }
        // The extension has about 30 seconds; the lock wait is a small part
        // of it, since an app holding it is live and shows the message itself.
        return try? client.notificationContentSync(timeoutMs: 3000)
    }
}
