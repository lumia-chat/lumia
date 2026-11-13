// What the Server settings screen decides before anything is sent
// (architecture 6.7), as values the tests cover: the name a server may
// have, the choices of how long it keeps its messages and when a choice
// deletes what it keeps now, and where a picture the instance names is.
// The same rules and words as the web's and the desktop shell's.
import Foundation

/// A server's name as the instance keeps it: trimmed, 1 to 100 characters.
public enum ServerName {
    public static let longest = 100

    /// What is sent: the name trimmed.
    public static func trimmed(_ name: String) -> String {
        name.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Whether the instance takes `name`. Characters as a person counts
    /// them (graphemes), as the instance does.
    public static func isValid(_ name: String) -> Bool {
        (1...longest).contains(trimmed(name).count)
    }

    /// Whether Save does anything: something there that is not the name the server has.
    public static func canSave(_ name: String, current: String) -> Bool {
        let t = trimmed(name)
        return !t.isEmpty && t != current
    }

    /// What is said when the instance would refuse it, or did.
    public static let refusal = "A server name is 1 to 100 characters."
}

/// How long a server keeps its messages, in days; 0 is for ever.
public enum Retention {
    /// The choices offered, longest first: Forever, 1 year, 90 days, 30 days.
    public static let standard: [UInt32] = [0, 365, 90, 30]

    /// The choices for a server that keeps its messages `current` days: the
    /// standard ones, and a value set elsewhere that is none of them after them.
    public static func choices(current: UInt32) -> [UInt32] {
        standard.contains(current) ? standard : standard + [current]
    }

    /// A choice as the screen names it.
    public static func label(_ days: UInt32) -> String {
        days == 0 ? "Forever" : span(days)
    }

    /// A number of days as the warning names it: "1 year", "90 days".
    public static func span(_ days: UInt32) -> String {
        switch days {
        case 365: "1 year"
        case 1: "1 day"
        default: "\(days) days"
        }
    }

    /// Whether `chosen` keeps messages for less time than `current`, so that
    /// saving it deletes some. Forever is the longest.
    public static func isShorter(_ chosen: UInt32, than current: UInt32) -> Bool {
        guard chosen != 0 else { return false }
        return current == 0 || chosen < current
    }

    /// What is said above Save when the choice deletes messages; nil when it deletes none.
    public static func warning(chosen: UInt32, current: UInt32) -> String? {
        guard isShorter(chosen, than: current) else { return nil }
        return "Messages older than \(span(chosen)) will be deleted for everyone. This cannot be undone."
    }
}

/// What the screen says when the instance refuses a server's settings:
/// `code` and `details` as the error carries them, `message` its own words.
public enum ServerSettingsRefusal {
    public static let notAllowed = "You cannot change this server's settings."
    public static let defaultChannels = "Default channels must be public channels of this server."

    /// A name or retention refused.
    public static func settings(code: String?, details: [String: String], message: String) -> String {
        switch code {
        case "PERMISSION_DENIED": notAllowed
        case "INVALID_ARGUMENT" where details["field"] == "name": ServerName.refusal
        case "INVALID_ARGUMENT" where details["field"] == "default_channels": defaultChannels
        default: message
        }
    }

    /// A picture refused, in the words the Profile's photo uses.
    public static func picture(code: String?, details: [String: String], message: String) -> String {
        switch (code, details["reason"]) {
        case ("PERMISSION_DENIED", _): notAllowed
        case ("INVALID_ARGUMENT", "too_large"): tooLarge
        case ("INVALID_ARGUMENT", "unsupported"): "That file is not a picture Lumia can read. Choose a PNG, JPEG, WebP or GIF."
        case ("INVALID_ARGUMENT", _): "That picture could not be read."
        case ("FAILED_PRECONDITION", _): "This instance does not keep pictures."
        case ("RATE_LIMITED", _): "The instance is busy. Try again in a moment."
        default: message
        }
    }

    /// The largest picture sent at all; a larger one is refused here, unsent.
    public static let largestPicture = 8 * 1024 * 1024
    public static let tooLarge = "That picture is too large. Choose one under 8 MB."
}

/// Where a picture the instance names (an account's or a server's) is:
/// only a path of the instance's own pictures, `/api/v1/avatars/` and 64
/// hexadecimal digits (the core's `is_own_avatar`), so nothing an instance
/// says can make the app fetch from anywhere else. Nil for none.
public func pictureURL(_ path: String, base: URL) -> URL? {
    let prefix = "/api/v1/avatars/"
    guard path.hasPrefix(prefix) else { return nil }
    let name = path.dropFirst(prefix.count)
    guard name.count == 64, name.allSatisfy({ ("0"..."9").contains($0) || ("a"..."f").contains($0) }) else { return nil }
    var root = base.absoluteString
    while root.hasSuffix("/") { root.removeLast() }
    return URL(string: root + path)
}
