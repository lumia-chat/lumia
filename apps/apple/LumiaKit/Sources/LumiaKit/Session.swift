// The session: one instance's client with the state the screens show,
// kept fresh by the gateway's events the way the web's queries are
// (apps/web/src/client/state.ts). Main-actor and observable; every
// client call goes through the actor off the main thread.
import Foundation
import LumiaFFI
import Observation

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
    /// Everyone this account shares a server with, by account id.
    public private(set) var people: [String: MemberSummary] = [:]
    /// A server's members, by server id, for the members aside.
    public private(set) var serverMembers: [String: [MemberSummary]] = [:]
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
    private var profiles: [String: MemberSummary?] = [:]
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
    /// A message to show once its conversation is on screen: a mention
    /// opened from a Mentions page.
    public var pendingReveal: Reveal?
    /// How many of a channel's newest messages are read from the store: a
    /// page, and more as a jump to an older message asks.
    @ObservationIgnored var messageLimits: [String: UInt32] = [:]
    @ObservationIgnored var readMarks = ReadMarks()

    private var events: Task<Void, Never>?
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

    /// With an invite link or code, the account joins its server at creation.
    public func createAccount(handle: String, password: String, invite: String? = nil) async throws {
        try await client.createAccount(handle: handle, password: password, invite: invite)
        signedIn = true
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
    private func take(_ snap: SnapshotSummary) {
        snapshot = snap
        subscribeGained()
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
        snapshot = try await client.updateServer(serverID: serverID, name: name.map(ServerName.trimmed), retentionDays: retentionDays)
    }

    /// Makes a file's bytes, as chosen, the server's picture; the instance makes its own square of it.
    public func setServerIcon(serverID: String, picture: Data, mediaType: String) async throws {
        snapshot = try await client.setServerIcon(serverID: serverID, picture: picture, mediaType: mediaType)
    }

    /// Takes the server's picture away.
    public func removeServerIcon(serverID: String) async throws {
        snapshot = try await client.removeServerIcon(serverID: serverID)
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
        endedNotice = nil
    }

    /// The gateway stops, the session is revoked where the instance still
    /// knows it, and nothing of the account is held: whoever signs in next
    /// starts from what the instance says.
    private func end() async {
        stop()
        try? await ask(.signOut)
        signedIn = false
        snapshot = nil
        messages = [:]
        dmChannels = []
        dmLogs = [:]
        roster = nil
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
        pendingReveal = nil
        messageLimits = [:]
        readMarks = ReadMarks()
        typing.clear()
        scheduleTypingExpiry()
        pushLedger.forget()
    }

    /// What the sign-in screen says to someone the instance sent back to it.
    public static let signedOutNotice = "You were signed out of this phone. Sign in again to carry on."
    private var endedNotice: String?

    /// The line for the sign-in screen, taken: the next one opens without it.
    public func takeEndedNotice() -> String? {
        defer { endedNotice = nil }
        return endedNotice
    }

    /// The instance no longer takes this session (it was revoked, or ran
    /// out): the gateway stops and what was held of the account goes, as on
    /// signing out, and the sign-in screen says what happened. Not while the
    /// person is signing out themselves: that is theirs, and says nothing.
    private func sessionEnded() async {
        guard signedIn, !leaving else { return }
        leaving = true
        defer { leaving = false }
        endedNotice = Self.signedOutNotice
        await end()
    }

    // MARK: Identity

    public func identityLinkRequest(label: String) async throws -> String { try await client.identityLinkRequest(label: label) }

    /// Claims the code an approving device gave, which makes this phone a
    /// device of the identity and signs it in by identity proof: the core's
    /// claim does both, so nothing signs in a second time here (that made a
    /// session only to revoke it).
    public func identityClaim(code: String) async throws {
        _ = try await client.identityClaim(code: code)
        hasIdentity = true
        signedIn = true
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
    public func load() async {
        do {
            let snap = try await client.snapshot()
            snapshot = snap
            reads = ReadCounts(snap.readStates)
            hasIdentity = await client.hasIdentity
            await refreshPeople(serverIDs: snap.servers.map(\.serverId))
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
        } catch {
            await loadFailed(error)
        }
    }

    public func refreshSnapshot() async {
        do {
            let snap = try await client.snapshot()
            take(snap)
            reads = ReadCounts(snap.readStates)
            await refreshPeople(serverIDs: snap.servers.map(\.serverId))
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

    private func refreshPeople(serverIDs: [String]) async {
        var found: [String: MemberSummary] = [:]
        var byServer: [String: [MemberSummary]] = [:]
        for id in serverIDs {
            let members = (try? await client.members(serverID: id)) ?? []
            byServer[id] = members
            for m in members { found[m.accountId] = m }
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

    /// A person's pronouns, from the verified profile the instance showed; empty when none.
    public func pronouns(of accountID: String) -> String {
        if accountID.isEmpty { return "" }
        return (people[accountID] ?? profile(of: accountID))?.pronouns ?? ""
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
            await refreshSnapshot()
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
        case .accountApplied:
            // Another device may have read something.
            await refreshReads()
            await refreshSnapshot()
            await refreshDmChannels()
        case let .error(_, message, _):
            commandError = message
        case let .reconnecting(attempt, _, _):
            connection = .reconnecting(attempt: attempt)
        case let .disconnected(reason, _):
            connection = .disconnected(reason: reason)
        case let .closed(reason):
            connection = .closed(reason: reason)
            events = nil
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
