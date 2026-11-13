// Channel membership and server DMs (architecture 6.4 and 6.8), as values
// the tests cover: which of a server's channels the sidebar lists and
// where, the notices a channel's list draws, the words the screens say,
// who the add-people and message pickers offer, the default-channel
// choices, and the words for what the instance refuses. The same rules
// and words as the web's, the desktop shell's and the Android app's.
import Foundation
import LumiaFFI

public let serverDMKind = "SERVER_DM"
public let textKind = "TEXT"

public enum Membership {
    /// A server's channels as its sidebar lists them under Channels: the
    /// ones this account is in, which is what the snapshot holds, but its
    /// server DMs and any personal conversation.
    public static func channels(_ all: [ChannelSummary], serverID: String) -> [ChannelSummary] {
        all.filter { $0.serverId == serverID && $0.kind != serverDMKind && !personalKinds.contains($0.kind) }
    }

    /// A server's DMs, as its sidebar lists them under Direct messages.
    public static func serverDMs(_ all: [ChannelSummary], serverID: String) -> [ChannelSummary] {
        all.filter { $0.serverId == serverID && $0.kind == serverDMKind }
    }

    /// The invitations into a server's channels that wait for this account.
    public static func invitations(_ all: [ChannelInvitationSummary], serverID: String) -> [ChannelInvitationSummary] {
        all.filter { $0.channel.serverId == serverID }
    }

    /// Who a server DM is with: its other participant (the account itself
    /// for a conversation with nobody else in it).
    public static func other(in channel: ChannelSummary, me: String) -> String {
        channel.participantAccountIds.first { $0 != me } ?? me
    }

    /// The members of a server a picker offers, without this account,
    /// those `excluding` names, and anyone `query` does not find by name,
    /// nickname or handle; by the name `name` gives each.
    public static func pick(_ members: [MemberSummary], me: String, excluding: Set<String> = [], query: String, name: (MemberSummary) -> String) -> [MemberSummary] {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        return members
            .filter { $0.accountId != me && !excluding.contains($0.accountId) }
            .filter { m in q.isEmpty || [m.displayName, m.nickname, m.handle].contains { $0.lowercased().contains(q) } }
            .sorted { name($0).localizedCaseInsensitiveCompare(name($1)) == .orderedAscending }
    }

    /// A server's public channels for its default-channel choice: the ones
    /// this account is in that are public text channels, then those Browse
    /// lists, each once.
    public static func publicChannels(joined: [ChannelSummary], browsed: [ChannelSummary], serverID: String) -> [ChannelSummary] {
        var seen = Set<String>()
        return (joined.filter { $0.kind == textKind && !$0.private } + browsed)
            .filter { $0.serverId == serverID && seen.insert($0.channelId).inserted }
    }

    /// How many are in a channel.
    public static func memberCount(_ n: UInt32) -> String {
        n == 1 ? "1 member" : "\(n) members"
    }

    /// The line an invitation reads as.
    public static func invitationLine(channel: String, inviter: String) -> String {
        "#\(channel) · \(inviter) invited you"
    }

    /// The bar in place of the composer of a channel this account is not in.
    public static func notIn(_ channel: String) -> String {
        "You’re not in #\(channel)"
    }

    /// What a server DM says once, at its top (architecture 6.4).
    public static func serverDMNote(server: String) -> String {
        "Messages here are kept by \(server) and follow its message history settings. For an encrypted conversation, message privately."
    }

    /// What a profile card says for someone who turned server DMs off there.
    public static let takesNoMessages = "Doesn’t take messages here"
}

/// A notice the instance posted in a channel (architecture 6.8), as its
/// one quiet line: `kind` as the message carries it, `actor` the name of
/// who did it (the author), `subject` of who it is about. Nil for a kind
/// this build does not know, which draws nothing.
public func noticeText(kind: String, actor: String, subject: String) -> String? {
    switch kind {
    case "joined": "\(subject) joined"
    case "left": "\(subject) left"
    case "added": "\(actor) added \(subject)"
    case "removed": "\(actor) removed \(subject)"
    default: nil
    }
}

/// A message's stamp for its block: a notice is a block of its own, and
/// the message after it opens one.
public func messageStamp(_ m: MessageSummary) -> MessageStamp {
    MessageStamp(authorID: m.notice.isEmpty ? m.authorAccountId : "notice:\(m.messageId)", atMicros: m.createdAtMicros, quotes: m.reply != nil)
}

/// The channels the gateway loop is subscribed to. The loop starts on
/// the channels of the snapshot it started with; a channel gained after
/// that (created, joined, added, a server DM begun) is subscribed to as a
/// snapshot first holds it, as the desktop shell does, or nothing of it
/// would arrive live until the loop started again.
public struct ChannelSubscriptions: Equatable, Sendable {
    private var ids: Set<String> = []

    public init() {}

    /// The loop started on these.
    public mutating func started(_ channelIDs: [String]) { ids = Set(channelIDs) }

    /// Of `channels`, the server ones not subscribed to yet, which count as
    /// subscribed from here.
    public mutating func gained(_ channels: [ChannelSummary]) -> [String] {
        channels.filter { !personalKinds.contains($0.kind) }.map(\.channelId).filter { ids.insert($0).inserted }
    }

    /// One subscribed to outside a snapshot (a channel previewed).
    public mutating func add(_ channelID: String) -> Bool { ids.insert(channelID).inserted }

    /// The loop stopped: nothing is subscribed.
    public mutating func clear() { ids = [] }
}

/// The default channels chosen in Server settings: what the server has,
/// and what is chosen now.
public struct DefaultChannels: Equatable, Sendable {
    public private(set) var chosen: Set<String>
    public let current: Set<String>

    public init(_ channels: [ChannelSummary]) {
        current = Set(channels.filter(\.isDefault).map(\.channelId))
        chosen = current
    }

    public func isChosen(_ channelID: String) -> Bool { chosen.contains(channelID) }

    public mutating func toggle(_ channelID: String) {
        if chosen.remove(channelID) == nil { chosen.insert(channelID) }
    }

    /// Whether Save does anything.
    public var changed: Bool { chosen != current }

    /// What is sent, in the order the card lists them.
    public func ids(in order: [ChannelSummary]) -> [String] {
        order.map(\.channelId).filter(chosen.contains)
    }
}

/// What the screens say when the instance refuses a membership call:
/// `code` and `details` as the error carries them, `message` its own words.
public enum MembershipRefusal {
    public static func add(code: String?, message: String) -> String {
        switch code {
        case "PERMISSION_DENIED", "NOT_FOUND": "You cannot add people to this channel."
        case "INVALID_ARGUMENT": "Someone chosen cannot be added to this channel."
        default: message
        }
    }

    public static func remove(code: String?, message: String) -> String {
        code == "PERMISSION_DENIED" ? "You cannot remove people from this channel." : message
    }

    /// Opening a server DM with `name`.
    public static func serverDM(code: String?, name: String, message: String) -> String {
        switch code {
        case "FAILED_PRECONDITION": "\(name) doesn’t take messages here."
        case "PERMISSION_DENIED": "You cannot start conversations in this server."
        case "NOT_FOUND": "\(name) is not in this server."
        default: message
        }
    }

    public static func createChannel(code: String?, message: String) -> String {
        code == "PERMISSION_DENIED" ? "You cannot add channels to this server." : message
    }
}
