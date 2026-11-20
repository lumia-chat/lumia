// The session: one instance's client with the state the screens show,
// kept fresh by the gateway's events the way the web's queries are
// (apps/web/src/client/state.ts). Main-actor and observable; every
// client call goes through the actor off the main thread.
import Foundation
import LumiaFFI
import Observation
#if os(iOS)
import UIKit
#endif
import os

public enum ConnectionState: Equatable, Sendable {
    case idle, connecting, connected
    case reconnecting(attempt: UInt32)
    case disconnected(reason: String)
    case closed(reason: String)

    public var label: String {
        switch self {
        case .idle: "not connected"
        case .connecting: "connecting"
        case .connected: "connected"
        case let .reconnecting(attempt): "reconnecting (attempt \(attempt))"
        case let .disconnected(reason): "disconnected: \(reason)"
        case let .closed(reason): "closed: \(reason)"
        }
    }
}

public let personalKinds: Set<String> = ["PERSONAL_DM", "PERSONAL_GROUP_DM"]

/// What a session has told its instance of this phone's push token. A
/// token goes once: the system hands the same one over at every launch
/// and every return to the shell. It goes again when it changes, when the
/// instance did not take it, and after signing out, which removes the
/// subscription there: whoever signs in next, the same account or
/// another, is subscribed again.
public struct PushLedger: Equatable, Sendable {
    private var token: String?

    public init() {}

    /// Whether `token` is still to be sent; it counts as sent from here, so
    /// that asking twice while it is on its way sends it once.
    public mutating func claim(_ token: String) -> Bool {
        guard self.token != token else { return false }
        self.token = token
        return true
    }

    /// The instance did not take `token`: it is still to be sent.
    public mutating func failed(_ token: String) {
        if self.token == token { self.token = nil }
    }

    /// Signing out: nothing is held as sent.
    public mutating func forget() {
        token = nil
    }
}

/// Why a session ended, and what the sign-in screen says of it (a sign-out
/// the person chose says nothing): the instance no longer taking the
/// session, or this device removed from the identity by another (identity
/// spec 4.4), which the core has erased.
public enum SessionEnd: Equatable, Sendable {
    case signedOut, removed

    /// The gateway loop's end: removed when the identity revoked this
    /// device; otherwise nothing is known yet, and the snapshot asked after
    /// it says whether the session is over.
    public static func closed(removed: Bool) -> SessionEnd? { removed ? .removed : nil }

    public var notice: String {
        switch self {
        case .signedOut: "You were signed out of this phone. Sign in again to carry on."
        case .removed: "This phone was removed from your account, and its private messages were erased from it."
        }
    }
}

/// How long a load that came short of the gateway loop waits before it is
/// asked again: a second, then twice as long each time, at most half a
/// minute, as the core's reconnects wait; a load that comes through starts
/// it over.
public struct LoadRetry: Equatable, Sendable {
    private var failures = 0

    public init() {}

    /// A load failed: the pause before the next.
    public mutating func pause() -> Duration {
        failures += 1
        return .seconds(min(30, 1 << min(failures - 1, 5)))
    }

    /// A load came through.
    public mutating func reset() {
        failures = 0
    }
}

/// The servers' member lists after a refresh, as a value the tests cover.
enum MemberLists {
    /// The lists of `serverIDs`: those `read` in place of what was held,
    /// the rest as held, and a server left gone. A list that could not be
    /// read is not an empty one: what was held of it stays, and a server
    /// with none held has none, so that it is asked for again at the next
    /// snapshot and nothing counts its members as zero meanwhile (a role's
    /// "Nobody holds this role." among them).
    static func after(held: [String: [MemberSummary]], serverIDs: [String], read: [String: [MemberSummary]]) -> [String: [MemberSummary]] {
        var lists = held.filter { serverIDs.contains($0.key) }
        for (id, list) in read where serverIDs.contains(id) { lists[id] = list }
        return lists
    }
}

@MainActor
@Observable
public final class LumiaSession {
    public let client: LumiaClient
    public let host: String

    public private(set) var signedIn: Bool
    public private(set) var hasIdentity: Bool
    public private(set) var snapshot: SnapshotSummary?
    public private(set) var connection: ConnectionState = .idle
    public private(set) var messages: [String: [MessageSummary]] = [:]
    public private(set) var dmChannels: [DmChannelSummary] = []
    public private(set) var dmLogs: [String: DmLogSummary] = [:]
    public private(set) var roster: RosterSummary?
    /// Where this phone stands with private messages (issue #20): found out
    /// and set up silently where it can be at each load.
    public private(set) var privateMessages: PrivateMessages = .unknown
    /// The recovery phrase this phone keeps still waits for three of its
    /// words: what the Home reminder and Devices' card follow.
    public private(set) var phraseUnconfirmed = false
    /// Setting up is being found out: a second asking meanwhile does nothing.
    @ObservationIgnored private var settling = false
    // Linking by a prompt (identity spec 4.2.1; `LinkByPrompt` has the rules).
    /// Linking this phone: where it stands, nil with none under way.
    public private(set) var linking: LinkByPrompt.Stage?
    /// What asking where it stands last failed with, until it next answers.
    public private(set) var linkingError: String?
    @ObservationIgnored private var linkStepping = false
    /// Other devices asking to be linked, oldest first, on a phone that
    /// holds the identity: session state, so the prompt shows over any screen.
    public private(set) var linkPrompts: [LinkPrompt] = []
    /// How the prompt answered here ended, said for a moment.
    public private(set) var linkEnding: LinkByPrompt.Ending?
    @ObservationIgnored private var promptsAsking = false
    /// What the prompt sheet shows, if anything.
    public var linkSheet: LinkByPrompt.Sheet? { LinkByPrompt.sheet(prompts: linkPrompts, ending: linkEnding) }
    /// Everyone this account shares a server with, by account id.
    public internal(set) var people: [String: MemberSummary] = [:]
    /// A server's members, by server id, for the members aside.
    public internal(set) var serverMembers: [String: [MemberSummary]] = [:]
    /// Who is present in the servers on screen (protocol 6.7), by server
    /// then account: `ONLINE`, `AWAY` or `DO_NOT_DISTURB`.
    public private(set) var serverPresence: [String: [String: String]] = [:]
    /// Custom status by server then account, beside the presence.
    public private(set) var serverCustomStatus: [String: [String: String]] = [:]
    private var watchingServerIDs: [String] = []
    /// Channels this account is not in, opened to read from Browse, and
    /// server DMs just begun, by id: what a screen finds that the snapshot
    /// does not hold (yet).
    public private(set) var previewed: [String: ChannelSummary] = [:]
    /// The channels the gateway loop has been told of.
    @ObservationIgnored private var subscriptions = ChannelSubscriptions()
    /// Who is typing in the servers' channels (protocol 6.7). A personal
    /// conversation never has anyone: the instance is not to learn who is
    /// writing there, and whether typing travels inside one is an open
    /// item of the E2EE spec (section 13).
    public private(set) var typing = TypingRoster()
    /// The wait for the first of those typing to run out, and the moment it waits for.
    private var typingExpiry: Task<Void, Never>?
    private var typingWake: Int64?
    /// This device's clock, in microseconds since the epoch; a test's own.
    var nowMicros: @MainActor () -> Int64 = { Int64(Date().timeIntervalSince1970 * 1_000_000) }
    /// Public profiles looked up one at a time; nil once the instance refused one.
    var profiles: [String: MemberSummary?] = [:]
    /// The lookups on their way. Not something a view watches: a view asks
    /// for a profile while drawing itself, and were the asking a change,
    /// two views naming the same stranger would redraw each other without
    /// end, the main thread never free to take the answer.
    @ObservationIgnored private var profilesInFlight: Set<String> = []
    public private(set) var lastError: String?

    // Quote replies, reactions, mentions and read state (architecture 6.6;
    // `Reads.swift` has what reads and changes these).
    /// Where this account has read each conversation, as the core counts it.
    public internal(set) var reads = ReadCounts()
    /// The Mentions pages read so far, by server id; "" is Home's, every server's.
    public internal(set) var mentionsPages: [String: MentionsList] = [:]
    /// Where a conversation open now had been read to when it was opened,
    /// while some of it was unread: its "New messages" line, until it is left.
    public internal(set) var newSince: [String: UInt64] = [:]
    /// Who is in a server's channel, as far as it has been asked, for the @
    /// list and the bar after a send.
    public internal(set) var channelMemberIDs: [String: Set<String>] = [:]
    /// What the instance refused of a command sent through the gateway (a
    /// reaction that is not one emoji, @everyone without the permission, a
    /// quote of a message that is gone), until it is read.
    public internal(set) var commandError: String?
    /// The edits sent through the gateway and not yet answered, by command
    /// id: a refusal names its command, and is said under the message.
    var editsSent = EditsSent()
    /// What the instance refused of a message's edit, by message: "Not saved: …".
    public internal(set) var editRefusals: [String: String] = [:]
    /// A message to show once its conversation is on screen: a mention
    /// opened from a Mentions page.
    public var pendingReveal: Reveal?
    /// How many of a channel's newest messages are read from the store: a
    /// page, and more as a jump to an older message asks.
    @ObservationIgnored var messageLimits: [String: UInt32] = [:]
    @ObservationIgnored var readMarks = ReadMarks()

    // Attachments and link cards (specs/media.md; `MediaSession.swift`).
    /// What the instance takes of attachments, once asked; nil while not
    /// known or when it keeps none, and the composer offers no way to attach.
    public internal(set) var uploadLimits: UploadLimits?
    @ObservationIgnored var uploadLimitsAsked = false
    /// Pictures decoded for drawing, by blob.
    @ObservationIgnored public let mediaImages = MediaImages()
    /// The servers' emoji pictures, by address (`Emoji.swift`): observable
    /// themselves, so a view drawing one is drawn again when it comes.
    @ObservationIgnored public let emojiPictures = EmojiPictures()

    /// What is typed in the conversations on screen and not sent, kept in
    /// the core's store off the main thread (issue #23; `Drafts.swift`).
    @ObservationIgnored public private(set) lazy var draftKeeper = DraftKeeper(store: queuedDrafts())

    // The rail's order and folders (architecture 6.9; `Rail.swift`).
    /// A rail edited here and not yet answered: drawn in place of the snapshot's.
    var railSending: [RailItem]?
    /// Which edit is the latest, so an answer to an older one changes nothing.
    @ObservationIgnored var railEdits = 0
    /// What the instance refused of a rail edited here, until it is read.
    public internal(set) var railError: String?

    private var events: Task<Void, Never>?
    /// A load that came short of the gateway loop, asked again after a
    /// pause until one comes through, the session ends or another load is
    /// asked for (`LoadRetry`).
    private var reload: Task<Void, Never>?
    @ObservationIgnored private var loadRetry = LoadRetry()
    private static let log = Logger(subsystem: "chat.lumia", category: "session")
    /// A gateway loop is being started: a second asking meanwhile (the
    /// shell's first load and a return to the foreground, say) starts none.
    private var starting = false

    /// The platform this phone's pushes come through: APNs, by the relay.
    public static let pushPlatform = "apns"
    /// What the instance has been told of this phone's token.
    private(set) var pushLedger = PushLedger()
    /// The token on its way to the instance, which signing out waits for.
    private var pushSending: Task<Void, Never>?
    /// The session is on its way out, chosen or ended: neither starts twice.
    private var leaving = false

    /// The calls whose order signing out is about.
    enum ClientCall: Hashable, Sendable {
        case pushSubscribe(token: String), pushUnsubscribe, signOut
    }

    /// Makes one of them on the client; a test's own.
    @ObservationIgnored var ask: @MainActor (ClientCall) async throws -> Void

    public convenience init(client: LumiaClient) async {
        await self.init(client: client, host: client.host, signedIn: client.signedIn, hasIdentity: client.hasIdentity)
    }

    /// A session as the client was found; a test names what it found.
    init(client: LumiaClient, host: String, signedIn: Bool, hasIdentity: Bool) {
        self.client = client
        self.host = host
        self.signedIn = signedIn
        self.hasIdentity = hasIdentity
        self.ask = { call in
            switch call {
            case let .pushSubscribe(token): try await client.pushSubscribe(platform: LumiaSession.pushPlatform, token: token)
            case .pushUnsubscribe: try await client.pushUnsubscribe(platform: LumiaSession.pushPlatform)
            case .signOut: try await client.signOut()
            }
        }
    }

    // MARK: Sign in

    public func instanceInfo() async throws -> InstanceSummary { try await client.instanceInfo() }

    /// Asks the instance to mail a sign-in link to `email`.
    public func requestMagicLink(email: String) async throws {
        try await client.requestMagicLink(email: email)
    }

    // A sign-in made while a session is held (linking a phone that signed
    // in with a password) replaces that session in the core, which keeps the
    // new pair and revokes the old one, and a gateway loop running on the
    // old one comes back on the new one by itself (`App::take_session`).

    /// Signs in with the token from a magic-link mail (the code, or the whole link).
    public func signInMagicLink(token: String) async throws {
        try await client.signInMagicLink(token: token)
        signedIn = true
    }

    public func signIn(handle: String, password: String, totpCode: String?) async throws {
        try await client.signIn(handle: handle, password: password, totpCode: totpCode)
        signedIn = true
    }

    /// With an invite link or code, the account joins its server at
    /// creation. Private messages are set up on this phone straight after,
    /// under the reauth the creation carried (architecture 5.4); the phrase
    /// comes later, through the reminders.
    public func createAccount(handle: String, password: String, invite: String? = nil) async throws {
        try await client.createAccount(handle: handle, password: password, invite: invite)
        signedIn = true
        await settlePrivateMessages()
    }

    // MARK: Invites (architecture 6.1)

    /// What an invite link or code leads to, before anything is sent.
    public func invitePreview(_ linkOrCode: String) async throws -> InvitePreviewSummary {
        try await client.invitePreview(linkOrCode)
    }

    /// An invite link for a server this account can invite to: unlimited
    /// unless `maxUses` is set, open-ended unless `expiresInHours` is.
    public func createInvite(serverID: String, maxUses: UInt32 = 0, expiresInHours: UInt32? = nil) async throws -> InviteSummary {
        try await client.createInvite(serverID: serverID, maxUses: maxUses, expiresInHours: expiresInHours)
    }

    /// A one-use invite good for a week, mailed to the address as a link (architecture 6.2).
    public func emailInvite(serverID: String, email: String) async throws -> InviteSummary {
        try await client.emailInvite(serverID: serverID, email: email)
    }

    /// The server's invites, newest first, for a member who may invite.
    public func invites(serverID: String) async throws -> [InviteSummary] {
        try await client.invites(serverID: serverID)
    }

    /// Revokes an invite this account made, or any with manage_server.
    public func revokeInvite(_ linkOrCode: String) async throws {
        try await client.revokeInvite(linkOrCode)
    }

    /// Joins the invite's server and refreshes the snapshot so it is listed; returns its id.
    public func acceptInvite(_ linkOrCode: String) async throws -> String {
        let serverID = try await client.acceptInvite(linkOrCode)
        await refreshSnapshot()
        return serverID
    }

    // MARK: Servers and channels

    /// A server this account owns, listed once the snapshot is refreshed; returns its id.
    public func createServer(name: String) async throws -> String {
        let server = try await client.createServer(name: name)
        await refreshSnapshot()
        return server.serverId
    }

    /// A text channel in the server, private when asked, listed once the
    /// snapshot is refreshed; returns its id.
    public func createChannel(serverID: String, name: String, isPrivate: Bool = false) async throws -> String {
        let channel = try await client.createChannel(serverID: serverID, name: name, isPrivate: isPrivate)
        await refreshSnapshot()
        return channel.channelId
    }

    // MARK: Channel membership (architecture 6.8)

    /// The public channels of a server this account can see and is not in.
    public func browseChannels(serverID: String) async throws -> [ChannelSummary] {
        try await client.browseChannels(serverID: serverID)
    }

    /// A channel this account is not in, opened to read (Browse): kept here
    /// for the screen to find, and subscribed to so it reads as any channel.
    public func preview(_ channel: ChannelSummary) {
        previewed[channel.channelId] = channel
        subscribe(channel.channelId)
    }

    /// A channel by id: the snapshot's, else one opened to read.
    public func channel(_ channelID: String) -> ChannelSummary? {
        snapshot?.channels.first { $0.channelId == channelID } ?? previewed[channelID]
    }

    /// Whether this account is in a channel: the snapshot holds the channels it is in.
    public func isIn(_ channelID: String) -> Bool {
        snapshot?.channels.contains { $0.channelId == channelID } ?? false
    }

    public func joinChannel(channelID: String) async throws {
        take(try await client.joinChannel(channelID: channelID))
    }

    public func leaveChannel(channelID: String) async throws {
        take(try await client.leaveChannel(channelID: channelID))
    }

    /// Who is in a channel, as account ids.
    public func channelMembers(channelID: String) async throws -> [String] {
        try await client.channelMembers(channelID: channelID)
    }

    public func addChannelMembers(channelID: String, accountIDs: [String]) async throws -> ChannelAddSummary {
        try await client.addChannelMembers(channelID: channelID, accountIDs: accountIDs)
    }

    public func removeChannelMember(channelID: String, accountID: String) async throws {
        try await client.removeChannelMember(channelID: channelID, accountID: accountID)
    }

    public func dismissChannelInvitation(channelID: String) async throws {
        take(try await client.dismissChannelInvitation(channelID: channelID))
    }

    /// This account's own switches in a server; nil leaves one as it is.
    public func updateMembership(serverID: String, channelAddsByInvitation: Bool? = nil, serverDmsOff: Bool? = nil) async throws {
        take(try await client.updateMembership(serverID: serverID, channelAddsByInvitation: channelAddsByInvitation, serverDmsOff: serverDmsOff))
    }

    /// Exactly these as the server's default channels.
    public func setDefaultChannels(serverID: String, channelIDs: [String]) async throws {
        take(try await client.setDefaultChannels(serverID: serverID, channelIDs: channelIDs))
    }

    /// The server DM with a member, started if there is none; returns its
    /// id. The account log brings it to the snapshot; until then it is held
    /// as a channel opened, so it opens at once.
    public func openServerDM(serverID: String, accountID: String) async throws -> String {
        let channel = try await client.openServerDM(serverID: serverID, accountID: accountID)
        previewed[channel.channelId] = channel
        await refreshSnapshot()
        return channel.channelId
    }

    /// A snapshot a call answered with: held, and the channels it gained subscribed to.
    func take(_ snap: SnapshotSummary) {
        hold(snap)
        subscribeGained()
    }

    /// A snapshot held in place of the last; the conversations of a server
    /// whose Show deleted messages it changes are read again under it.
    private func hold(_ snap: SnapshotSummary) {
        let shown = (snapshot?.servers ?? []).map { (serverID: $0.serverId, show: $0.showDeleted) }
        snapshot = snap
        rereadShown(was: shown)
    }

    /// A server whose Show deleted messages changed (here or, through the
    /// account log, elsewhere): the lists held of its channels were read
    /// under the old setting, so they are read again under the new one.
    private func rereadShown(was: [(serverID: String, show: Bool)]) {
        guard let snap = snapshot else { return }
        let changed = ShowDeleted.changed(from: was, to: snap.servers.map { (serverID: $0.serverId, show: $0.showDeleted) })
        guard !changed.isEmpty else { return }
        let channels = snap.channels + Array(previewed.values)
        let held = channels.filter { changed.contains($0.serverId) && messages[$0.channelId] != nil }.map(\.channelId)
        for id in Set(held) { Task { await refreshMessages(channelID: id) } }
    }

    /// Subscribes to the snapshot's channels the gateway loop has not been
    /// told of, while it runs (`ChannelSubscriptions`).
    private func subscribeGained() {
        guard events != nil, let snap = snapshot else { return }
        for id in subscriptions.gained(snap.channels) {
            Task { try? await client.subscribe(channelID: id) }
        }
    }

    private func subscribe(_ channelID: String) {
        guard events != nil, subscriptions.add(channelID) else { return }
        Task { try? await client.subscribe(channelID: channelID) }
    }

    // MARK: Server settings (architecture 6.7)

    /// Changes a server's name, how long it keeps messages (0 for ever), or
    /// both; nil leaves one as it is. The snapshot the instance answers with
    /// is held, so the rail and the screen show the change at once; other
    /// members hear of it through their account logs.
    public func updateServer(serverID: String, name: String?, retentionDays: UInt32?) async throws {
        hold(try await client.updateServer(serverID: serverID, name: name.map(ServerName.trimmed), retentionDays: retentionDays))
    }

    /// Makes a file's bytes, as chosen, the server's picture; the instance makes its own square of it.
    public func setServerIcon(serverID: String, picture: Data, mediaType: String) async throws {
        hold(try await client.setServerIcon(serverID: serverID, picture: picture, mediaType: mediaType))
    }

    /// Takes the server's picture away.
    public func removeServerIcon(serverID: String) async throws {
        hold(try await client.removeServerIcon(serverID: serverID))
    }

    /// Whether the server's deleted messages keep their place as
    /// "(deleted)"; the snapshot it answers with is held, and the server's
    /// conversations held here are read again under it.
    public func setShowDeleted(serverID: String, show: Bool) async throws {
        take(try await client.setShowDeleted(serverID: serverID, show: show))
    }

    /// How long the server keeps its attachments (0 for ever), what a full
    /// server does, or both (specs/media.md 4.5); nil leaves one as it is.
    /// The snapshot it answers with is held, as a name's is.
    public func setServerFiles(serverID: String, attachmentRetentionDays: UInt32?, deleteOldestWhenFull: Bool?) async throws {
        hold(try await client.setServerFiles(serverID: serverID, attachmentRetentionDays: attachmentRetentionDays, deleteOldestWhenFull: deleteOldestWhenFull))
    }

    /// What the server's files take, for its settings: asked only where
    /// they are offered, since the instance answers `MANAGE_SERVER` alone.
    public func serverStorage(serverID: String) async throws -> StorageSummary {
        try await client.serverStorage(serverID: serverID)
    }

    /// Where a picture this instance names is (`pictureURL(_:base:)`); nil for none.
    public func pictureURL(_ path: String) -> URL? {
        LumiaKit.pictureURL(path, base: client.baseURL)
    }

    public func signInIdentity() async throws {
        try await client.signInIdentity()
        signedIn = true
    }

    /// Signs out because the person chose to. First, while the session can
    /// still say so, this phone's push subscription is removed: the instance
    /// keeps subscriptions by device, not by session, so a signed-out phone
    /// would otherwise go on getting notices for the account. A failure
    /// there does not stop the rest: the session is revoked, and the tokens
    /// and what was held of the account are forgotten. The identity and the
    /// device keys stay on the phone, and the sign-in screen has no line to
    /// say, since nobody was sent back to it. A second call, or one while
    /// the session is already on its way out, does nothing.
    public func signOut() async {
        guard signedIn, !leaving else { return }
        leaving = true
        defer { leaving = false }
        // A token on its way must land before it can be taken back.
        await pushSending?.value
        try? await ask(.pushUnsubscribe)
        await end()
        ended = nil
    }

    /// The gateway stops, the session is revoked where the instance still
    /// knows it, and nothing of the account is held: whoever signs in next
    /// starts from what the instance says.
    private func end() async {
        stop()
        try? await ask(.signOut)
        signedIn = false
        draftKeeper.forget()
        snapshot = nil
        messages = [:]
        dmChannels = []
        dmLogs = [:]
        roster = nil
        privateMessages = .unknown
        phraseUnconfirmed = false
        linking = nil
        linkingError = nil
        linkPrompts = []
        linkEnding = nil
        people = [:]
        serverMembers = [:]
        serverPresence = [:]
        serverCustomStatus = [:]
        watchingServerIDs = []
        previewed = [:]
        profiles = [:]
        lastError = nil
        reads = ReadCounts()
        mentionsPages = [:]
        newSince = [:]
        channelMemberIDs = [:]
        commandError = nil
        editsSent = EditsSent()
        editRefusals = [:]
        railError = nil
        pendingReveal = nil
        messageLimits = [:]
        readMarks = ReadMarks()
        typing.clear()
        scheduleTypingExpiry()
        pushLedger.forget()
        emojiPictures.forget()
    }

    /// What the sign-in screen says to someone the instance sent back to it.
    public static let signedOutNotice = SessionEnd.signedOut.notice
    private var ended: SessionEnd?

    /// Why the session last ended, taken: the next sign-in screen opens without it.
    public func takeEnded() -> SessionEnd? {
        defer { ended = nil }
        return ended
    }

    /// The line for the sign-in screen, taken: the next one opens without it.
    public func takeEndedNotice() -> String? { takeEnded()?.notice }

    /// The instance no longer takes this session (it was revoked, or ran
    /// out): the gateway stops and what was held of the account goes, as on
    /// signing out, and the sign-in screen says what happened. Not while the
    /// person is signing out themselves: that is theirs, and says nothing.
    private func sessionEnded() async {
        guard signedIn, !leaving else { return }
        leaving = true
        defer { leaving = false }
        ended = .signedOut
        await end()
    }

    /// Another device revoked this one (identity spec 4.4): the core has
    /// already erased what it kept (the instance's store, the identity, the
    /// keys and tokens) and made it a new device. What the app held of the
    /// account goes too, in memory and on this phone (the parsed bodies,
    /// the pictures' cache, the phrase reminder, the rail's open folders,
    /// the emoji used), and the sign-in screen says why, even when the
    /// session had already ended another way.
    func deviceRemoved() async {
        ended = .removed
        hasIdentity = false
        BodyBlocks.forget()
        URLCache.shared.removeAllCachedResponses()
        var reminder = PhraseReminder()
        reminder.clear()
        for key in [OpenFolders.key, FrequentEmoji.key] { UserDefaults.standard.removeObject(forKey: key) }
        guard signedIn, !leaving else { return }
        leaving = true
        defer { leaving = false }
        await end()
    }

    // MARK: Identity

    public func identityLinkRequest(label: String) async throws -> String { try await client.identityLinkRequest(label: label) }

    /// On this phone, which holds the identity: approves another device's
    /// request (identity spec 4.2 steps 3 to 6) and answers the code it
    /// claims with. The roster held here gains the device at once, so
    /// Devices lists it before it has claimed.
    public func identityApprove(request: String) async throws -> String {
        let code = try await client.identityApprove(request: request)
        roster = try? await client.identityShow()
        return code
    }

    /// Claims the code an approving device gave, which makes this phone a
    /// device of the identity and signs it in by identity proof: the core's
    /// claim does both, so nothing signs in a second time here (that made a
    /// session only to revoke it).
    public func identityClaim(code: String) async throws {
        roster = try await client.identityClaim(code: code)
        hasIdentity = true
        signedIn = true
        privateMessages = .ready
    }

    // MARK: Linking by a prompt (identity spec 4.2.1)

    /// As the new device: opens a link session, so every device that holds
    /// the identity prompts. This phone must be signed in to the account.
    /// One given up while it opened answers nothing: the core declined it.
    public func linkStart() async throws {
        linkingError = nil
        do {
            linking = LinkByPrompt.Stage(try await client.linkStart(label: Self.deviceLabel))
        } catch let LumiaError.Other(detail) where detail == LinkByPrompt.givenUp {
            return
        }
    }

    /// Where linking stands, which takes its next step (reveals, then
    /// claims and signs in by identity proof as `identityClaim` does): when
    /// the screen opens, every two seconds while it shows, and whenever the
    /// session moves. One at a time; nothing once it has ended.
    public func linkStep() async {
        guard let stage = linking, stage.polls, !linkStepping else { return }
        linkStepping = true
        defer { linkStepping = false }
        do {
            let next = LinkByPrompt.Stage(try await client.linkProgress())
            if next == .linked {
                hasIdentity = true
                signedIn = true
                privateMessages = .ready
                roster = try? await client.identityShow()
            }
            // Given up meanwhile: the answer is nobody's.
            guard linking != nil else { return }
            linking = next
            linkingError = nil
        } catch {
            if error.lumiaSignedOut { await sessionEnded(); return }
            guard linking != nil else { return }
            // The core no longer has it (given up elsewhere, or claimed by a step that failed after).
            if case let .Other(detail)? = error as? LumiaError, detail == "no link in progress" {
                linking = .expired
            } else {
                linkingError = error.lumiaMessage
            }
        }
    }

    /// Gives linking up, which declines it so the other devices' prompts close.
    public func linkCancel() async {
        guard linking != nil else { return }
        linking = nil
        linkingError = nil
        await client.linkCancel()
    }

    /// Linking is over and the screen has gone on.
    public func linkDone() {
        linking = nil
        linkingError = nil
    }

    /// As a device that holds the identity: the sessions that prompt here,
    /// asked at launch, on coming to the foreground, whenever a session
    /// moves and every two seconds while one is open. A failure keeps what
    /// is held.
    public func refreshLinkPrompts() async {
        guard signedIn, hasIdentity, !promptsAsking else { return }
        promptsAsking = true
        defer { promptsAsking = false }
        do {
            linkPrompts = try await client.linkPrompts()
        } catch {
            if error.lumiaSignedOut { await sessionEnded() }
        }
    }

    /// Continue: answers the prompt, so both devices show their digits.
    public func linkPromptAnswer(_ prompt: LinkPrompt) async throws {
        try await client.linkPromptAnswer(sessionID: prompt.sessionId)
        await refreshLinkPrompts()
    }

    /// They match, after the phone's own unlock (the screen's): approves
    /// and hands over the code; "… is linked." for a moment, and the roster
    /// held here gains the device.
    public func linkPromptConfirm(_ prompt: LinkPrompt) async throws {
        try await client.linkPromptConfirm(sessionID: prompt.sessionId)
        end(with: .linked(prompt.request))
        roster = try? await client.identityShow()
        await refreshLinkPrompts()
    }

    /// Not me, or (`mismatch`) They don't match, which says "Linking was stopped." for a moment.
    public func linkPromptDecline(_ prompt: LinkPrompt, mismatch: Bool = false) async throws {
        try await client.linkPromptDecline(sessionID: prompt.sessionId)
        if mismatch { end(with: .stopped) }
        await refreshLinkPrompts()
    }

    /// Says how a prompt ended, then lets it go.
    private func end(with ending: LinkByPrompt.Ending) {
        linkEnding = ending
        Task { [weak self] in
            try? await Task.sleep(for: LinkByPrompt.endingFor)
            if self?.linkEnding == ending { self?.linkEnding = nil }
        }
    }

    /// What this phone calls itself among the identity's devices.
    public static var deviceLabel: String {
        #if os(iOS)
        UIDevice.current.model
        #else
        "Mac"
        #endif
    }

    /// Finds out where this phone stands with private messages and sets
    /// them up silently where the account allows (architecture 5.4): an
    /// account with no identity is bound to this phone's, made here unless
    /// held, under the reauth the sign-up or sign-in carried. Without one in
    /// hand the person is asked to confirm it's them (`confirm`); an account
    /// whose identity is elsewhere needs this phone brought in (`bringIn`).
    /// A failure to reach the instance leaves it to the next load.
    public func settlePrivateMessages() async {
        guard signedIn, !settling else { return }
        settling = true
        defer { settling = false }
        do {
            let account = try await client.accountIdentity()
            hasIdentity = await client.hasIdentity
            if hasIdentity { roster = try await client.identityShow() }
            var state = PrivateMessages.step(account: account, device: hasIdentity ? roster?.identityId : nil)
            if state == .setUp {
                do {
                    _ = try await client.identitySetup(label: Self.deviceLabel)
                    state = .ready
                } catch {
                    state = PrivateMessages.after(error)
                }
                // Made here, bound or not: a setup asked to confirm it's
                // you first has made the identity already.
                await refreshRoster()
            }
            privateMessages = state
        } catch {
            if error.lumiaSignedOut { await sessionEnded(); return }
        }
        await refreshPhrase()
    }

    /// After the person confirmed it's them: sets up as `settlePrivateMessages` would have.
    public func setUpPrivateMessages() async throws {
        do {
            _ = try await client.identitySetup(label: Self.deviceLabel)
        } catch {
            privateMessages = PrivateMessages.after(error)
            await refreshRoster()
            throw error
        }
        privateMessages = .ready
        await afterIdentity()
    }

    /// The devices of the phrase's identity that restoring would stop, read
    /// from the instance before anything changes; none when it knows no
    /// roster for it. A phrase that is not one is refused here.
    public func restoreDevices(phrase: String) async throws -> [DeviceSummary] {
        try await client.identityRecoverDevices(phrase: phrase)
    }

    /// Restores the identity from its 24 words (identity spec 4.3): this
    /// phone joins it and signs in by it, so this works from the sign-in
    /// screen as well as from inside the app. The devices in `keeping`
    /// stay; every other one is revoked.
    public func restore(phrase: String, keeping: [String] = []) async throws {
        roster = try await client.identityRecoverKeeping(phrase: phrase, label: Self.deviceLabel, keep: keeping)
        hasIdentity = true
        signedIn = true
        privateMessages = .ready
        await afterIdentity()
    }

    /// Starts over with a new identity made here (identity spec 4.7), under
    /// the reauth the person just gave; its phrase follows as at sign-up.
    public func startOver() async throws {
        _ = try await client.identityStartOver(label: Self.deviceLabel)
        privateMessages = .ready
        await afterIdentity()
    }

    /// Once this phone holds the account's identity: the phrase's state,
    /// and, with the app already on screen, the personal conversations it
    /// can now read.
    private func afterIdentity() async {
        await refreshRoster()
        await refreshPhrase()
        guard snapshot != nil else { return }
        await refreshSnapshot()
        await refreshDmChannels()
    }

    /// Confirming it's the person (identity spec 10), for the next setup or start over.
    public func reauthPassword(_ password: String) async throws { try await client.reauthPassword(password) }
    public func reauthCode(_ code: String, mailed: Bool) async throws { try await client.reauthCode(code, mailed: mailed) }
    public func reauthMailCode() async throws { try await client.reauthMailCode() }

    /// Confirms it's the person by a fresh login at the account's OIDC
    /// provider in the system browser, which comes back with the token.
    public func reauthThroughBrowser() async throws {
        let url = try await client.reauthOIDCURL(returnTo: BrowserSheet.reauthReturn)
        guard let parsed = URL(string: url) else { throw BrowserSheetError.badURL }
        let back = try await BrowserSheet().run(parsed, ephemeral: false)
        let token = URLComponents(url: back, resolvingAgainstBaseURL: false)?.queryItems?.first { $0.name == "reauth" }?.value ?? ""
        guard !token.isEmpty else { throw BrowserSheetError.badURL }
        await client.reauthTake(token)
    }

    /// The phrase this phone keeps, its 24 words, or nil once confirmed.
    /// Shown only after the phone's own unlock, which is the screen's.
    public func phrase() async throws -> [String]? {
        try await client.identityPhrase().map(PhraseWords.words)
    }

    /// The three positions to ask for.
    public func phraseQuiz() async throws -> [UInt32] { try await client.identityPhraseQuiz() }

    /// The words for those positions: true ends the reminders; false means
    /// the phrase is shown again, and the next quiz asks other positions.
    public func confirmPhrase(_ words: [String]) async throws -> Bool {
        let right = try await client.identityPhraseConfirm(words: words)
        if right { phraseUnconfirmed = false }
        return right
    }

    /// Whether the phrase still waits, read again.
    public func refreshPhrase() async {
        guard hasIdentity else { phraseUnconfirmed = false; return }
        phraseUnconfirmed = (try? await client.identityPhraseUnconfirmed()) ?? false
    }

    public func identityRevoke(deviceID: String, password: String) async throws {
        roster = try await client.identityRevoke(deviceID: deviceID, password: password)
    }

    // MARK: Passkeys, and sign-in through the browser (architecture 6.3)

    public func passkeys() async throws -> [PasskeySummary] { try await client.passkeys() }
    public func removePasskey(id: String) async throws { try await client.removePasskey(id: id) }

    /// Signs in through the system browser: `oidc` to the provider, else
    /// the instance's web sign-in page (a passkey there, or any method it
    /// offers); the sheet comes back with a one-time token this session
    /// completes as a magic-link token.
    public func signInThroughBrowser(oidc: Bool) async throws {
        let url = oidc
            ? await client.oidcStartURL(returnTo: BrowserSheet.signInReturn)
            : await client.webSignInURL(returnTo: BrowserSheet.signInReturn)
        guard let parsed = URL(string: url) else { throw BrowserSheetError.badURL }
        let back = try await BrowserSheet().run(parsed, ephemeral: !oidc)
        try await client.signInMagicLink(token: back.absoluteString)
        signedIn = true
    }

    /// Opens the instance's web page signed in as this account by a
    /// one-time token (a passkey is made on its Devices page); returns
    /// when the person closes the sheet.
    public func openWebSignedIn(next: String) async throws {
        let token = try await client.handoffToken()
        let url = await client.webHandoffURL(token: token, next: next)
        guard let parsed = URL(string: url) else { throw BrowserSheetError.badURL }
        do {
            _ = try await BrowserSheet().run(parsed, ephemeral: true)
        } catch BrowserSheetError.cancelled {
            // Closed by the person once done: the only way out of the page.
        }
    }

    // MARK: Profile (architecture 6.4)

    /// The signed profile this identity carries: this device's, else the
    /// one the instance holds from another device of the identity.
    public func profile() async throws -> ProfileSummary? { try await client.identityProfile() }

    /// A new version, signed by this device and sent to the instance; the
    /// snapshot follows, so the name on screen does.
    public func setProfile(displayName: String, pronouns: String, bio: String) async throws -> ProfileSummary {
        let current = try? await client.identityProfile()
        let p = try await client.identitySetProfile(displayName: displayName, pronouns: pronouns, bio: bio, avatarURL: current?.avatarUrl ?? "")
        await refreshSnapshot()
        return p
    }

    public func refreshRoster() async {
        hasIdentity = await client.hasIdentity
        guard hasIdentity else { roster = nil; return }
        roster = try? await client.identityShow()
    }

    // MARK: Loading

    /// Loads the snapshot and starts the gateway on every server channel.
    /// A load that fails short of the loop, with the session still held (the
    /// instance slow to answer or out of reach as the app opens), is asked
    /// again after a pause: nothing else would ask before the app next comes
    /// to the foreground, and until then the app would hold no snapshot, no
    /// loop, and open nothing a tapped notification names.
    public func load() async {
        reload?.cancel()
        reload = nil
        do {
            let snap = try await client.snapshot()
            hold(snap)
            reads = ReadCounts(snap.readStates)
            hasIdentity = await client.hasIdentity
            let ids = snap.servers.map(\.serverId)
            await refreshPeople(serverIDs: ids, fetching: ids)
            await refreshDmChannels()
            if events == nil, !starting {
                starting = true
                defer { starting = false }
                // With any channel opened to read before the loop ran.
                let held = snap.channels.filter { !personalKinds.contains($0.kind) }.map(\.channelId)
                try await start(channelIDs: held + previewed.keys.filter { !held.contains($0) })
            } else {
                subscribeGained()
            }
            loadRetry.reset()
        } catch {
            await loadFailed(error)
            if signedIn, !leaving { loadAgain(after: error) }
        }
        if !privateMessages.ready { await settlePrivateMessages() }
        await refreshLinkPrompts()
    }

    /// The next load after one that failed, once its pause is over.
    private func loadAgain(after error: Error) {
        let pause = loadRetry.pause()
        Self.log.info("the load did not come through (\(error.lumiaMessage, privacy: .public)); again in \(pause.components.seconds) s")
        reload = Task { [weak self] in
            try? await Task.sleep(for: pause)
            guard let self, !Task.isCancelled, self.signedIn, !self.leaving else { return }
            self.reload = nil
            await self.load()
        }
    }

    /// Reads the snapshot again. Member lists are read only for servers
    /// newly in it, or for every server with `everyone`: a reconnect may
    /// have missed what changed. A name or picture changed since arrives by
    /// itself (`profileUpdated`), so another account event (a read on
    /// another device, say) costs no list.
    public func refreshSnapshot(everyone: Bool = false) async {
        do {
            let snap = try await client.snapshot()
            take(snap)
            reads = ReadCounts(snap.readStates)
            let ids = snap.servers.map(\.serverId)
            await refreshPeople(serverIDs: ids, fetching: everyone ? ids : ids.filter { serverMembers[$0] == nil })
        } catch {
            await loadFailed(error)
        }
    }

    /// The snapshot did not come (or, in `load`, the gateway loop did not
    /// start under it). A session the instance no longer takes leaves
    /// nothing here that works: back to sign-in, which says why. With nothing
    /// held yet the failure is what there is to say; a refresh of a snapshot
    /// that is held (every welcome and account event asks for one) fails
    /// without a word, and what is held stays on screen.
    private func loadFailed(_ error: Error) async {
        if error.lumiaSignedOut {
            await sessionEnded()
        } else if snapshot == nil {
            lastError = error.lumiaMessage
        }
    }

    public func refreshMessages(channelID: String) async {
        let limit = messageLimits[channelID] ?? Replies.pageSize
        if let list = try? await client.messages(channelID: channelID, limit: limit) { messages[channelID] = list }
    }

    public func refreshDmChannels() async {
        guard await client.dmAvailable else { return }
        if let list = try? await client.dmChannels() { dmChannels = list }
    }

    public func refreshDmLog(channelID: String) async {
        guard await client.dmAvailable else { return }
        if let log = try? await client.dmMessages(channelID: channelID) { dmLogs[channelID] = log }
    }

    /// The member lists of `serverIDs`, those of `fetching` read again and
    /// the rest kept; a server left goes.
    private func refreshPeople(serverIDs: [String], fetching: [String]) async {
        var read: [String: [MemberSummary]] = [:]
        for id in fetching {
            if let list = try? await client.members(serverID: id) { read[id] = list }
        }
        let byServer = MemberLists.after(held: serverMembers, serverIDs: serverIDs, read: read)
        var found: [String: MemberSummary] = [:]
        for id in serverIDs {
            for m in byServer[id] ?? [] { found[m.accountId] = m }
        }
        people = found
        serverMembers = byServer
    }

    // MARK: Actions

    public func sendMessage(channelID: String, body: String) async throws {
        _ = try await client.sendMessage(channelID: channelID, body: body)
        await refreshMessages(channelID: channelID)
    }

    public func dmSend(channelID: String, body: String) async throws {
        try await client.dmSend(channelID: channelID, body: body)
        await refreshDmLog(channelID: channelID)
    }

    public func dmRejoin(channelID: String) async throws {
        try await client.dmRejoin(channelID: channelID)
        await refreshDmChannels()
    }

    /// Opens a conversation with `accountID`, or asks when the instance wants
    /// a request first: the channel id when opened, nil when a request went out.
    public func dmOpenOrRequest(accountID: String) async throws -> String? {
        do {
            let opened = try await client.dmOpen(accountIDs: [accountID])
            await refreshSnapshot()
            await refreshDmChannels()
            return opened.channelId
        } catch {
            guard error.lumiaCode == "FAILED_PRECONDITION" else { throw error }
            _ = try await client.dmRequest(accountID: accountID)
            return nil
        }
    }

    /// Answers a request; the channel id when accepting opened one.
    public func dmRespond(requestID: String, response: String) async throws -> String? {
        let r = try await client.dmRespond(requestID: requestID, response: response)
        await refreshSnapshot()
        await refreshDmChannels()
        if case let .opened(opened) = r { return opened.channelId }
        return nil
    }

    // MARK: Names

    public var me: String { snapshot?.accountId ?? "" }

    public var selfName: String {
        let s = snapshot
        return (s?.displayName).flatMap { $0.isEmpty ? nil : $0 } ?? (s?.handle).flatMap { $0.isEmpty ? nil : $0 } ?? "You"
    }

    /// A person's name: from the shared servers, else a looked-up profile, else the id's start.
    public func name(of accountID: String) -> String {
        if !accountID.isEmpty, accountID == me { return selfName }
        let p = people[accountID] ?? profile(of: accountID)
        if let p {
            for candidate in [p.displayName, p.nickname, p.handle] where !candidate.isEmpty { return candidate }
        }
        return accountID.isEmpty ? "Someone" : String(accountID.prefix(8))
    }

    public func profile(of accountID: String) -> MemberSummary? {
        guard !accountID.isEmpty else { return nil }
        if let known = profiles[accountID] { return known }
        if profilesInFlight.insert(accountID).inserted {
            Task {
                do {
                    let p = try await client.account(accountID)
                    profiles[accountID] = .some(p)
                } catch {
                    // Not visible stays nil; anything else is retried on the next read.
                    if error.lumiaCode == "NOT_FOUND" { profiles[accountID] = .some(nil) }
                }
                profilesInFlight.remove(accountID)
            }
        }
        return nil
    }

    /// The picture an account names (`avatarPath`): this account's own, else
    /// what the shared servers say, else a looked-up profile's; empty for none.
    public func avatarURL(of accountID: String) -> String {
        avatarPath(of: accountID, me: me, own: snapshot?.avatarUrl ?? "", listed: { people[$0] }, lookedUp: profile(of:))
    }

    /// Where an account's picture is, for its avatar: only on this instance
    /// (`pictureURL(_:base:)`); nil for none or one named elsewhere.
    public func picture(of accountID: String) -> URL? {
        pictureURL(avatarURL(of: accountID))
    }

    /// A person's pronouns, from the verified profile the instance showed; empty when none.
    public func pronouns(of accountID: String) -> String {
        if accountID.isEmpty { return "" }
        return (people[accountID] ?? profile(of: accountID))?.pronouns ?? ""
    }

    /// Someone changed their name or picture (issue #15): written over
    /// what is held of them (the servers' lists, the people, a profile
    /// looked up, and for this account the snapshot), so everything that
    /// draws them redraws as they now are, with no member list read again.
    /// Someone not held is read as they are when first asked for.
    func apply(_ update: ProfileUpdate) {
        let id = update.accountID
        if let p = people[id] { people[id] = update.applied(to: p) }
        for (serverID, members) in serverMembers where members.contains(where: { $0.accountId == id }) {
            serverMembers[serverID] = update.applied(to: members)
        }
        if case let .some(.some(p)) = profiles[id] { profiles[id] = .some(update.applied(to: p)) }
        if let snap = snapshot, snap.accountId == id { snapshot = update.applied(to: snap) }
    }

    public func conversationName(_ channel: ChannelSummary) -> String {
        let others = channel.participantAccountIds.filter { $0 != me }
        if others.isEmpty { return "Just you" }
        return others.map { name(of: $0) }.joined(separator: ", ")
    }

    // MARK: Gateway

    private func start(channelIDs: [String]) async throws {
        let stream = try await client.start(channelIDs: channelIDs)
        subscriptions.started(channelIDs)
        connection = .connecting
        let started = ContinuousClock.now
        events = Task { [weak self] in
            for await event in stream {
                guard let self else { return }
                await self.handle(event)
            }
            // `stop()` cancels this task, and by now another loop may have started.
            guard let self, !Task.isCancelled else { return }
            self.events = nil
            // The loop has ended and will not come back by itself. Why is in
            // words only, so the snapshot is asked, once: a session the
            // instance has revoked answers so there and the app goes to
            // sign-in. Anything else starts the loop again with the snapshot,
            // unless this one ended as soon as it began: what ended it would
            // end the next as soon, so that is left to the next time the app
            // comes to the foreground.
            if ContinuousClock.now - started < .seconds(10) {
                await self.refreshSnapshot()
            } else {
                await self.load()
            }
        }
    }

    public func stop() {
        reload?.cancel()
        reload = nil
        events?.cancel()
        events = nil
        subscriptions.clear()
        Task { await client.stop() }
        connection = .idle
    }

    /// Going to the background: the single-writer lock is released so the
    /// notification extension can fetch and decrypt (architecture 15.5).
    public func background() {
        Task { await client.background() }
    }

    /// Back in the foreground: the lock is taken again; when the extension
    /// wrote meanwhile, the gateway loop restarts from the store's cursors;
    /// a loop that had ended is started again; otherwise it is asked for
    /// every head.
    public func foreground() {
        Task {
            let wrote = (try? await client.resume()) ?? false
            if wrote {
                stop()
                await load()
            } else if events == nil {
                await load()
            } else {
                try? await client.foreground()
            }
            // A device may have asked while this phone was away.
            await refreshLinkPrompts()
        }
    }

    /// Hands this phone's APNs token to the instance, which pushes through
    /// the relay. Once per token (`PushLedger`): asked again with one the
    /// instance holds, nothing is sent; one it did not take is sent the
    /// next time, and signing out starts over.
    public func pushSubscribe(token: String) async {
        guard signedIn, !leaving, pushLedger.claim(token) else { return }
        let sending = Task {
            do {
                try await ask(.pushSubscribe(token: token))
            } catch {
                pushLedger.failed(token)
            }
        }
        pushSending = sending
        await sending.value
    }

    func handle(_ event: LumiaEvent) async {
        switch event {
        case .welcomed:
            serverPresence = [:]
            serverCustomStatus = [:]
            watchPresence(serverIDs: watchingServerIDs)
            connection = .connected
            await refreshSnapshot(everyone: true)
        case let .applied(channelId, events):
            // Whoever's message arrived has finished typing it.
            for message in events.compactMap(\.message) {
                typing.stopped(channelID: channelId, accountID: message.authorAccountId)
            }
            scheduleTypingExpiry()
            // A notice changes who is in the channel.
            if events.contains(where: { !($0.message?.notice.isEmpty ?? true) }), channelMemberIDs[channelId] != nil {
                await loadChannelMembers(channelID: channelId)
            }
            await refreshMessages(channelID: channelId)
            await refreshReads()
        case let .snapshotTaken(channelId, _):
            await refreshMessages(channelID: channelId)
        case .linkSessionUpdated:
            // A link session moved, on either side.
            await linkStep()
            await refreshLinkPrompts()
        case .accountApplied:
            // Another device may have read something.
            await refreshReads()
            await refreshSnapshot()
            await refreshDmChannels()
        case let .tagsChanged(serverId):
            // After the `accountApplied` it came in, which took the snapshot
            // again: what the core keeps has the change, so the server's
            // members are read from it, with their tags, and nothing is asked.
            await rereadMembers(serverID: serverId)
        case .emojiChanged:
            // After the `accountApplied` it came in, which took the snapshot
            // again with the server's emoji as they now are: a picture no
            // server lists any more goes (specs/emoji.md 7). The core has
            // named the kept reactions with each anew, so the messages held
            // are read again from it: one left after a removal reads as the
            // emoji was last called.
            emojiChanged()
            for channelID in messages.keys {
                await refreshMessages(channelID: channelID)
            }
        case let .profileUpdated(accountId, handle, displayName, avatarUrl, pronouns, bio):
            apply(ProfileUpdate(accountID: accountId, handle: handle, displayName: displayName, avatarURL: avatarUrl, pronouns: pronouns, bio: bio))
        case let .error(_, message, commandId):
            if let edited = editsSent.refused(command: commandId) {
                editRefusals[edited] = MessageEdits.notSaved(message)
            } else {
                commandError = message
            }
        case let .reconnecting(attempt, _, _):
            connection = .reconnecting(attempt: attempt)
        case let .disconnected(reason, _):
            connection = .disconnected(reason: reason)
        case let .closed(reason, removed):
            connection = .closed(reason: reason)
            events = nil
            if SessionEnd.closed(removed: removed) == .removed { await deviceRemoved() }
        case let .dmSubscribed(channelId), let .dmReady(channelId):
            await refreshDmChannels()
            await refreshDmLog(channelID: channelId)
        case let .dmOperation(channelId, _, _, _):
            await refreshDmLog(channelID: channelId)
            await refreshReads()
        case .dmDelivered:
            for id in dmLogs.keys { await refreshDmLog(channelID: id) }
        case let .dmError(message):
            lastError = message
        case let .presenceDiff(serverId, joined, updated, leftAccountIds):
            var server = serverPresence[serverId] ?? [:]
            var custom = serverCustomStatus[serverId] ?? [:]
            for p in joined + updated {
                server[p.accountId] = p.status
                custom[p.accountId] = p.customStatus
            }
            for id in leftAccountIds {
                server.removeValue(forKey: id)
                custom.removeValue(forKey: id)
            }
            serverPresence[serverId] = server
            serverCustomStatus[serverId] = custom
        case let .typing(channelId, accountId, untilMicros):
            typing.began(channelID: channelId, accountID: accountId, untilMicros: untilMicros, me: me)
            scheduleTypingExpiry()
        case .acked, .subscriptionFailed, .dmNote:
            break
        }
    }

    // MARK: Typing

    /// This person changed what the composer holds and left something in
    /// it: says so to a server's channel (protocol 6.7). The core sends it
    /// at most once in five seconds, so every edit may call this; a
    /// personal conversation is held back here as it is in the core.
    public func typed(channelID: String) {
        guard connection == .connected,
              let channel = snapshot?.channels.first(where: { $0.channelId == channelID }),
              !channel.serverId.isEmpty, !personalKinds.contains(channel.kind) else { return }
        Task { try? await client.typing(channelID: channelID) }
    }

    /// The names of those typing in a channel, whoever began first at the front.
    public func typingNames(in channelID: String) -> [String] {
        typing.accountIDs(in: channelID).map { name(of: $0) }
    }

    /// Nothing arrives when someone stops typing, so the session wakes
    /// itself when the first of those typing runs out, drops whoever has,
    /// and waits for the next; it waits for nothing while nobody is.
    private func scheduleTypingExpiry() {
        let ends = typing.nextEnd
        guard ends != typingWake else { return }
        typingWake = ends
        typingExpiry?.cancel()
        typingExpiry = nil
        guard let ends else { return }
        typingExpiry = Task { [weak self] in
            guard let wait = self.map({ max(0, ends - $0.nowMicros()) }) else { return }
            try? await Task.sleep(for: .microseconds(wait))
            guard !Task.isCancelled, let self else { return }
            self.typingWake = nil
            // Whoever ends at `ends` has ended, whatever the clock says of an early wake.
            self.typing.expire(nowMicros: max(ends, self.nowMicros()))
            self.scheduleTypingExpiry()
        }
    }

    // MARK: Presence

    /// The servers on screen, whose presence this client wants (protocol
    /// 6.7); none for Home. Re-sent on every welcome.
    public func watchPresence(serverIDs: [String]) {
        watchingServerIDs = serverIDs
        guard connection == .connected else { return }
        Task { try? await client.watchPresence(serverIDs: serverIDs) }
    }

    /// The status this person chose and their custom status text.
    public func status() async throws -> StatusSummary { try await client.status() }

    /// Chooses the status (`ONLINE`, `AWAY`, `DO_NOT_DISTURB`) and custom
    /// text; the core keeps it and sends it with the servers watched.
    public func setStatus(status: String, customStatus: String) async throws {
        try await client.setStatus(status: status, customStatus: customStatus)
    }

    /// The custom status an account set, as its presence in a server carries it; empty when none or away.
    public func customStatus(of accountID: String, in serverID: String) -> String {
        serverCustomStatus[serverID]?[accountID] ?? ""
    }

    /// An account's presence in a server, or nil when it is not there.
    public func presence(of accountID: String, in serverID: String) -> String? {
        serverPresence[serverID]?[accountID]
    }
}

public extension String {
    /// nil for the empty string, for optional subtitles built from parts.
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
