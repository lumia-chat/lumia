// What the session does for quote replies, reactions, mentions and read
// state (architecture 6.6), the desktop shell's `session/reads.rs`: the
// counts are the core's (`readStates`), read again after every snapshot,
// applied event and mark; a conversation is marked read while its newest
// message is on screen and the app is in front, at most once a second each,
// and only when that moves something (`ReadMarks`); the Mentions pages are
// read a page at a time; a send carries the mentions the core finds in its
// body and what it quotes; finding a message for a jump reads further back
// in the store, up to `Replies.seekPages` pages.
import Foundation
import LumiaFFI

/// A message to show once its conversation is on screen.
public struct Reveal: Equatable, Sendable {
    public let channelID: String
    public let messageID: String
    public init(channelID: String, messageID: String) { self.channelID = channelID; self.messageID = messageID }
}

/// A Mentions page as far as it has been read.
public struct MentionsList: Equatable, Sendable {
    public var messages: [MessageSummary] = []
    /// The instance has older ones.
    public var more = false
    public var loading = false
    public var error: String?
    public init() {}
}

public extension LumiaSession {
    // MARK: Counts

    /// Reads the counts again from the core.
    func refreshReads() async {
        reads = ReadCounts(await client.readStates())
    }

    /// The app icon's badge: what is unread and for this account, everywhere.
    var badge: UInt32 { reads.badge }

    /// A conversation's unread messages and how many of those are for this account.
    func unread(_ channelID: String) -> (unread: UInt32, mentions: UInt32) { reads.unread(channelID) }

    /// Whether anything is unread in a server (its channels this account is
    /// in and its DMs), and how much of it is for this account.
    func serverUnread(_ serverID: String) -> (unread: Bool, mentions: UInt32) {
        let all = snapshot?.channels ?? []
        return reads.any(Membership.channels(all, serverID: serverID) + Membership.serverDMs(all, serverID: serverID))
    }

    /// The personal conversations' messages for this account that are unread: every one, in a DM.
    var personalMentions: UInt32 {
        (snapshot?.channels ?? []).filter { personalKinds.contains($0.kind) }.reduce(0) { $0 + unread($1.channelId).mentions }
    }

    /// Home's Mentions row: every server's mentions of this account.
    var homeMentions: UInt32 {
        reads.serverMentions(personal: (snapshot?.channels ?? []).filter { personalKinds.contains($0.kind) })
    }

    // MARK: Read

    /// A conversation is on screen: where it had been read to, while some of
    /// it is unread, places its "New messages" line until it is left.
    func opened(_ channelID: String) {
        newSince[channelID] = NewMessagesLine.since(reads.states[channelID])
    }

    /// The conversation is no longer on screen: its line goes.
    func left(_ channelID: String) {
        newSince[channelID] = nil
    }

    /// The message the "New messages" line goes above, in a server's channel
    /// or a personal conversation; nil when it has none.
    func newLine(in channelID: String) -> String? {
        guard let readTo = newSince[channelID] else { return nil }
        if let log = dmLogs[channelID] {
            return NewMessagesLine.at(log.messages.map { ReadEntry(id: $0.messageId, seq: $0.seq, counts: !$0.mine) }, readTo: readTo)
        }
        let list = messages[channelID] ?? []
        return NewMessagesLine.at(list.map { ReadEntry(id: $0.messageId, seq: $0.seq, counts: $0.notice.isEmpty && $0.authorAccountId != me) }, readTo: readTo)
    }

    /// This account has read a conversation through `seq`, its newest
    /// message on screen with the app in front. Nothing when that moves
    /// nothing, or when it was marked through `seq` less than a second ago.
    func markRead(channelID: String, seq: UInt64) async {
        guard readMarks.claim(channelID, seq: seq, state: reads.states[channelID], now: Date()) else { return }
        guard (try? await client.markRead(channelID: channelID, seq: seq)) != nil else { return }
        await refreshReads()
    }

    // MARK: Mentions

    /// Reads the first page of a Mentions list, or the next after what it
    /// holds. `serverID` nil is Home's, every server's.
    func loadMentions(serverID: String?, more: Bool) async {
        let key = serverID ?? ""
        var page = mentionsPages[key] ?? MentionsList()
        guard !page.loading, !more || page.more else { return }
        page.loading = true
        mentionsPages[key] = page
        let before = more ? page.messages.last?.messageId : nil
        do {
            let got = try await client.mentions(serverID: serverID, before: before)
            page.messages = (more ? page.messages : []) + got.messages
            page.more = got.hasMore
            page.error = nil
        } catch {
            page.error = error.lumiaMessage
        }
        page.loading = false
        mentionsPages[key] = page
        // Their authors' names, for those not in a server with this account.
        for m in page.messages { _ = name(of: m.authorAccountId) }
    }

    // MARK: Reactions

    /// Adds this account's reaction to a message, or takes it away when it
    /// is already there; in a personal conversation through its encryption.
    func react(channelID: String, messageID: String, emoji: String) async throws {
        if let log = dmLogs[channelID] {
            let mine = log.messages.first { $0.messageId == messageID }?.reactions.first { $0.reaction == emoji }?.me ?? false
            try await client.dmReact(channelID: channelID, messageID: messageID, emoji: emoji, remove: mine)
            await refreshDmLog(channelID: channelID)
        } else {
            let mine = messages[channelID]?.first { $0.messageId == messageID }?.reactions.first { $0.reaction == emoji }?.me ?? false
            try await client.react(channelID: channelID, messageID: messageID, emoji: emoji, remove: mine)
        }
        FrequentEmoji.remember(emoji)
    }

    /// The refusal of a command the gateway carried, taken: shown once.
    func takeCommandError() -> String? {
        defer { commandError = nil }
        return commandError
    }

    // MARK: Sending

    /// Who is in a server's channel, for the @ list and the bar after a send.
    func loadChannelMembers(channelID: String) async {
        if let ids = try? await client.channelMembers(channelID: channelID) { channelMemberIDs[channelID] = Set(ids) }
    }

    /// Sends a message to a server's channel or DM with the mentions its body
    /// makes of the server's members (and of everyone, for one who may; none
    /// in a server DM, where both are told of every message) and what it
    /// quotes. The answer is who was mentioned and is not in the channel, so
    /// not notified.
    func send(channelID: String, body: String, quote: QuoteSummary?) async throws -> [String] {
        var mentions: [MentionSummary] = []
        if let channel = channel(channelID), channel.kind == textKind {
            let everyone = snapshot?.servers.first { $0.serverId == channel.serverId }?.canMentionEveryone ?? false
            mentions = await client.findMentions(body: body, members: serverMembers[channel.serverId] ?? [], everyone: everyone)
        }
        let outside = Replies.outside(mentions, inside: channelMemberIDs[channelID], me: me)
        _ = try await client.sendMessageWith(channelID: channelID, body: body, mentions: mentions, replyTo: quote)
        await refreshMessages(channelID: channelID)
        return outside
    }

    /// Sends a message in a personal conversation, quoting another or not.
    func dmSend(channelID: String, body: String, quote: QuoteSummary?) async throws {
        try await client.dmSendWith(channelID: channelID, body: body, quote: quote)
        await refreshDmLog(channelID: channelID)
    }

    /// "Add to #name" from the bar after a send: those added are in the
    /// channel; the answer says who was invited instead.
    func addOutside(channelID: String, accountIDs: [String]) async throws -> ChannelAddSummary {
        let added = try await addChannelMembers(channelID: channelID, accountIDs: accountIDs)
        await loadChannelMembers(channelID: channelID)
        return added
    }

    // MARK: Finding a message

    /// Finds a message of a conversation, reading further back in the store
    /// for it when it is not among those read, up to `Replies.seekPages`
    /// pages; true once it is there to scroll to.
    func reveal(channelID: String, messageID: String) async -> Bool {
        if let log = dmLogs[channelID] { return log.messages.contains { $0.messageId == messageID } }
        func has() -> Bool { messages[channelID]?.contains { $0.messageId == messageID } ?? false }
        if has() { return true }
        for _ in 0..<Replies.seekPages {
            let before = messages[channelID]?.count ?? 0
            messageLimits[channelID] = (messageLimits[channelID] ?? Replies.pageSize) + Replies.pageSize
            await refreshMessages(channelID: channelID)
            if has() { return true }
            // The store holds no more of it.
            if (messages[channelID]?.count ?? 0) == before { break }
        }
        return false
    }
}

public extension FrequentEmoji {
    private static let key = "recentEmoji"

    /// What this phone reacted with, newest first.
    static var recent: [String] { UserDefaults.standard.stringArray(forKey: key) ?? [] }

    /// Reacted with `emoji`: first in "Frequently used" from now.
    static func remember(_ emoji: String) {
        UserDefaults.standard.set(used(emoji, recent: recent), forKey: key)
    }
}
