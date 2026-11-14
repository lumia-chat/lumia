// The client as the apps use it: the UniFFI `Client` (blocking calls,
// a listener called from a runtime thread) behind an actor that runs
// every call off the main thread and turns the listener into an
// `AsyncStream` of events. One per instance. The screens hold a
// `LumiaSession` (an observable over this) rather than this directly.
import Foundation
import LumiaFFI

public typealias LumiaEvent = ClientEvent

/// One instance's client, off the main thread.
public actor LumiaClient {
    private let inner: Client
    public let baseURL: URL

    /// Opens the client for `baseURL` with state under `dataDirectory`
    /// (`<dir>/<host>/` for the instance, `<dir>/identity/` shared).
    public init(baseURL: URL, dataDirectory: URL) throws {
        self.baseURL = baseURL
        self.inner = try Client(baseUrl: baseURL.absoluteString, dataDir: dataDirectory.path)
    }

    public var host: String { inner.host() }
    public var deviceID: String { inner.deviceId() }
    public var signedIn: Bool { inner.signedIn() }
    public var hasIdentity: Bool { inner.hasIdentity() }
    public var dmAvailable: Bool { inner.dmAvailable() }

    public func instanceInfo() throws -> InstanceSummary { try inner.instanceInfo() }
    /// With an invite link or code, the account joins its server too.
    public func createAccount(handle: String, password: String, invite: String? = nil) throws {
        try inner.createAccount(handle: handle, password: password, invite: invite)
    }
    /// What an invite link or code shows before joining. Public.
    public func invitePreview(_ linkOrCode: String) throws -> InvitePreviewSummary { try inner.invitePreview(linkOrCode: linkOrCode) }
    /// An invite link for a server (0 uses is unlimited).
    public func emailInvite(serverID: String, email: String) throws -> InviteSummary { try inner.emailInvite(serverId: serverID, email: email) }
    public func createInvite(serverID: String, maxUses: UInt32 = 0, expiresInHours: UInt32? = nil) throws -> InviteSummary {
        try inner.createInvite(serverId: serverID, maxUses: maxUses, expiresInHours: expiresInHours)
    }
    /// Joins the invite's server; returns its id.
    public func acceptInvite(_ linkOrCode: String) throws -> String { try inner.acceptInvite(linkOrCode: linkOrCode) }
    /// This client's status and the servers it has on screen (protocol 6.7).
    public func watchPresence(serverIDs: [String]) throws { try inner.watchPresence(watchServerIds: serverIDs) }
    /// Says this person is typing in a server's channel (protocol 6.7); the core spaces the sends out.
    public func typing(channelID: String) throws { try inner.typing(channelId: channelID) }
    public func status() throws -> StatusSummary { try inner.status() }
    public func setStatus(status: String, customStatus: String) throws { try inner.setStatus(status: status, customStatus: customStatus) }
    /// The server's invites, newest first.
    public func invites(serverID: String) throws -> [InviteSummary] { try inner.invites(serverId: serverID) }
    /// Revokes an invite.
    public func revokeInvite(_ linkOrCode: String) throws { try inner.revokeInvite(linkOrCode: linkOrCode) }
    /// A server this account owns.
    public func createServer(name: String) throws -> ServerSummary { try inner.createServer(name: name) }
    /// A server's settings (architecture 6.7): its name, how long it keeps
    /// messages in days (0 for ever), or both, nil leaving one as it is; its
    /// picture, a file's bytes as chosen, or none. Each returns the snapshot as it then is.
    public func updateServer(serverID: String, name: String?, retentionDays: UInt32?) throws -> SnapshotSummary {
        try inner.updateServer(serverId: serverID, name: name, retentionDays: retentionDays)
    }
    public func setServerIcon(serverID: String, picture: Data, mediaType: String) throws -> SnapshotSummary {
        try inner.setServerIcon(serverId: serverID, picture: picture, mediaType: mediaType)
    }
    public func removeServerIcon(serverID: String) throws -> SnapshotSummary { try inner.removeServerIcon(serverId: serverID) }
    /// Whether the server's deleted messages keep their place as "(deleted)"; returns the snapshot as it then is.
    public func setShowDeleted(serverID: String, show: Bool) throws -> SnapshotSummary {
        try inner.setShowDeleted(serverId: serverID, show: show)
    }
    /// A text channel in a server this account may manage channels in, private when asked.
    public func createChannel(serverID: String, name: String, isPrivate: Bool) throws -> ChannelSummary {
        try inner.createChannel(serverId: serverID, name: name, private: isPrivate)
    }
    /// Channel membership (architecture 6.8): the public channels of a
    /// server this account is not in; joining, leaving; who is in a
    /// channel, adding and removing them; an invitation turned down; this
    /// account's own switches in a server; and the server's default
    /// channels. Those that change the snapshot return it as it then is.
    public func browseChannels(serverID: String) throws -> [ChannelSummary] { try inner.browseChannels(serverId: serverID) }
    public func joinChannel(channelID: String) throws -> SnapshotSummary { try inner.joinChannel(channelId: channelID) }
    public func leaveChannel(channelID: String) throws -> SnapshotSummary { try inner.leaveChannel(channelId: channelID) }
    public func channelMembers(channelID: String) throws -> [String] { try inner.channelMembers(channelId: channelID) }
    public func addChannelMembers(channelID: String, accountIDs: [String]) throws -> ChannelAddSummary {
        try inner.addChannelMembers(channelId: channelID, accountIds: accountIDs)
    }
    public func removeChannelMember(channelID: String, accountID: String) throws { try inner.removeChannelMember(channelId: channelID, accountId: accountID) }
    public func dismissChannelInvitation(channelID: String) throws -> SnapshotSummary { try inner.dismissChannelInvitation(channelId: channelID) }
    public func updateMembership(serverID: String, channelAddsByInvitation: Bool?, serverDmsOff: Bool?) throws -> SnapshotSummary {
        try inner.updateMembership(serverId: serverID, channelAddsByInvitation: channelAddsByInvitation, serverDmsOff: serverDmsOff)
    }
    public func setDefaultChannels(serverID: String, channelIDs: [String]) throws -> SnapshotSummary {
        try inner.setDefaultChannels(serverId: serverID, channelIds: channelIDs)
    }
    /// The server DM with a member of the server, started if there is none (architecture 6.4).
    public func openServerDM(serverID: String, accountID: String) throws -> ChannelSummary {
        try inner.openServerDm(serverId: serverID, accountId: accountID)
    }
    /// Asks the instance to mail a sign-in link to `email`.
    public func requestMagicLink(email: String) throws { try inner.requestMagicLink(email: email) }
    /// Signs in with the token from a magic-link mail (the code, or the whole link).
    public func signInMagicLink(token: String) throws { try inner.signInMagicLink(token: token) }
    public func signIn(handle: String, password: String, totpCode: String? = nil) throws {
        try inner.signIn(handle: handle, password: password, totpCode: totpCode)
    }
    public func signOut() throws { try inner.signOut() }
    public func snapshot() throws -> SnapshotSummary { try inner.snapshot() }
    public func account(_ accountID: String) throws -> MemberSummary { try inner.account(accountId: accountID) }
    public func members(serverID: String, query: String = "") throws -> [MemberSummary] { try inner.members(serverId: serverID, query: query) }
    public func messages(channelID: String, limit: UInt32 = 200) throws -> [MessageSummary] { try inner.messages(channelId: channelID, limit: limit) }

    /// Starts the gateway loop; the stream carries every event in order
    /// until the loop closes or `stop()`.
    public func start(channelIDs: [String]) throws -> AsyncStream<LumiaEvent> {
        let (stream, continuation) = AsyncStream<LumiaEvent>.makeStream()
        try inner.start(channelIds: channelIDs, listener: StreamListener(continuation: continuation))
        return stream
    }
    public func stop() { inner.stop() }
    public func subscribe(channelID: String) throws { try inner.subscribe(channelId: channelID) }
    public func sendMessage(channelID: String, body: String) throws -> String { try inner.sendMessage(channelId: channelID, body: body) }
    /// Quote replies, reactions, mentions and read state (architecture 6.6):
    /// a message with its mentions and what it quotes (only the quote's id
    /// and text are read), the mentions a body makes as the core finds
    /// them, a reaction added or taken away, the read counts and the badge
    /// as this client counts them, a conversation read through `seq`, and a
    /// page of the messages that notify this account.
    public func sendMessageWith(channelID: String, body: String, mentions: [MentionSummary], replyTo: QuoteSummary?) throws -> String {
        try inner.sendMessageWith(channelId: channelID, body: body, mentions: mentions, replyTo: replyTo)
    }
    public func findMentions(body: String, members: [MemberSummary], everyone: Bool) -> [MentionSummary] {
        inner.findMentions(body: body, members: members, everyone: everyone)
    }
    public func react(channelID: String, messageID: String, emoji: String, remove: Bool) throws {
        try inner.react(channelId: channelID, messageId: messageID, emoji: emoji, remove: remove)
    }
    /// Editing and deleting a server's message (architecture 6.6): the
    /// command goes through the gateway, and the answer is its id, which a
    /// refusal's error names.
    public func editMessage(channelID: String, messageID: String, body: String, mentions: [MentionSummary]) throws -> String {
        try inner.editMessage(channelId: channelID, messageId: messageID, body: body, mentions: mentions)
    }
    public func deleteMessage(channelID: String, messageID: String) throws -> String { try inner.deleteMessage(channelId: channelID, messageId: messageID) }
    public func readStates() -> [ReadStateSummary] { inner.readStates() }
    public func badge() -> UInt32 { inner.badge() }
    public func markRead(channelID: String, seq: UInt64) throws -> ReadStateSummary { try inner.markRead(channelId: channelID, seq: seq) }
    public func mentions(serverID: String?, before: String?) throws -> MentionsPage { try inner.mentions(serverId: serverID, before: before) }
    public func foreground() throws { try inner.foreground() }
    /// Going to the background: releases the single-writer lock (architecture 15.5).
    public func background() { inner.background() }
    /// Back in the foreground: takes the lock again; true when the
    /// notification extension wrote meanwhile and the loop must restart.
    public func resume() throws -> Bool { try inner.resume() }
    /// For the notification extension: what the push should say, after
    /// syncing and decrypting under the exclusive lock; nil keeps the generic notice.
    public func notificationContent(timeoutMs: UInt64 = 3000) throws -> NotificationContent? { try inner.notificationContent(timeoutMs: timeoutMs) }
    /// The same, blocking, for the notification extension's one call.
    public nonisolated func notificationContentSync(timeoutMs: UInt64 = 3000) throws -> NotificationContent? { try inner.notificationContent(timeoutMs: timeoutMs) }

    /// What a push that has reached this phone becomes, blocking (the
    /// notification extension's one call, architecture 15.5). Nothing at all
    /// when this client is not signed in or the instance no longer takes its
    /// session: the push is for an account the phone does not hold, and a
    /// stale subscription must not tell whoever has the phone that it has
    /// mail. The message when one could be read; otherwise the generic
    /// notice it came as.
    public nonisolated func notice(timeoutMs: UInt64 = 3000) -> PushNotice {
        guard inner.signedIn() else { return .nothing }
        do {
            return try inner.notificationContent(timeoutMs: timeoutMs).map(PushNotice.message) ?? .generic
        } catch {
            return error.lumiaSignedOut ? .nothing : .generic
        }
    }

    public func identityCreate(label: String) throws -> IdentityCreated { try inner.identityCreate(label: label) }
    public func identityLinkAccount(password: String) throws { try inner.identityLinkAccount(password: password) }
    public func identityLinkRequest(label: String) throws -> String { try inner.identityLinkRequest(label: label) }
    public func identityApprove(request: String) throws -> String { try inner.identityApprove(request: request) }
    public func identityClaim(code: String) throws -> RosterSummary { try inner.identityClaim(code: code) }
    public func signInIdentity() throws { try inner.signInIdentity() }
    public func identityShow() throws -> RosterSummary { try inner.identityShow() }
    public func identityRevoke(deviceID: String, password: String) throws -> RosterSummary {
        try inner.identityRevoke(deviceId: deviceID, password: password)
    }

    /// The account's passkeys (architecture 6.3), and one removed.
    public func passkeys() throws -> [PasskeySummary] { try inner.passkeys() }
    public func removePasskey(id: String) throws { try inner.removePasskey(passkeyId: id) }
    /// A one-time sign-in token for this account, for the instance's web page to sign in as it.
    public func handoffToken() throws -> String { try inner.handoffToken() }
    /// Where the system browser signs in with the instance's OIDC provider; back to `returnTo` with the token.
    public func oidcStartURL(returnTo: String) -> String { inner.oidcStartUrl(returnTo: returnTo) }
    /// Where the system browser signs in through the instance's web page (a passkey); back to `returnTo` with the token.
    public func webSignInURL(returnTo: String) -> String { inner.webSignInUrl(returnTo: returnTo) }
    /// Where the system browser lands signed in as this account by `token`, on `next`.
    public func webHandoffURL(token: String, next: String) -> String { inner.webHandoffUrl(token: token, next: next) }
    /// The signed profile this identity carries (architecture 6.4), and a new version of it.
    public func identityProfile() throws -> ProfileSummary? { try inner.identityProfile() }
    public func identitySetProfile(displayName: String, pronouns: String, bio: String, avatarURL: String) throws -> ProfileSummary {
        try inner.identitySetProfile(displayName: displayName, pronouns: pronouns, bio: bio, avatarUrl: avatarURL)
    }

    public func pushSubscribe(platform: String, token: String) throws { try inner.pushSubscribe(platform: platform, token: token, p256dh: "", auth: "") }
    public func pushUnsubscribe(platform: String) throws { try inner.pushUnsubscribe(platform: platform) }

    public func dmChannels() throws -> [DmChannelSummary] { try inner.dmChannels() }
    public func dmOpen(accountIDs: [String]) throws -> DmOpened { try inner.dmOpen(accountIds: accountIDs) }
    public func dmRequest(accountID: String) throws -> DmRequestSummary { try inner.dmRequest(accountId: accountID) }
    public func dmRespond(requestID: String, response: String) throws -> DmResponse { try inner.dmRespond(requestId: requestID, response: response) }
    public func dmSend(channelID: String, body: String) throws { try inner.dmSend(channelId: channelID, body: body) }
    public func dmSendWith(channelID: String, body: String, quote: QuoteSummary?) throws { try inner.dmSendWith(channelId: channelID, body: body, quote: quote) }
    public func dmReact(channelID: String, messageID: String, emoji: String, remove: Bool) throws {
        try inner.dmReact(channelId: channelID, messageId: messageID, emoji: emoji, remove: remove)
    }
    public func dmEdit(channelID: String, messageID: String, body: String) throws { try inner.dmEdit(channelId: channelID, messageId: messageID, body: body) }
    public func dmDelete(channelID: String, messageID: String) throws { try inner.dmDelete(channelId: channelID, messageId: messageID) }
    public func dmRejoin(channelID: String) throws { try inner.dmRejoin(channelId: channelID) }
    public func dmMessages(channelID: String) throws -> DmLogSummary { try inner.dmMessages(channelId: channelID) }
}

/// What a push becomes on this phone (`LumiaClient.notice`).
public enum PushNotice: Equatable, Sendable {
    /// The message, read and decrypted: the notice is rewritten to it.
    case message(NotificationContent)
    /// The instance's generic notice, as it came.
    case generic
    /// No notice: the phone holds no session of the account it is for.
    case nothing
}

/// Feeds the listener's events into an `AsyncStream`.
private final class StreamListener: Listener, @unchecked Sendable {
    private let continuation: AsyncStream<LumiaEvent>.Continuation
    init(continuation: AsyncStream<LumiaEvent>.Continuation) { self.continuation = continuation }
    func onEvent(event: ClientEvent) {
        continuation.yield(event)
        if case .closed = event { continuation.finish() }
    }
}

public extension Error {
    /// The API error's code, when the failure was one (`UNAUTHENTICATED`, `NOT_FOUND`, ...).
    var lumiaCode: String? {
        if case let LumiaError.Api(code, _, _)? = self as? LumiaError { return code }
        return nil
    }

    /// The API error's `details`, as the protocol documents them per code (`mfa`, `state`, ...).
    var lumiaDetails: [String: String] {
        if case let LumiaError.Api(_, _, details)? = self as? LumiaError { return details }
        return [:]
    }

    /// Whether the failure of a call made as the signed-in account says the
    /// instance no longer takes this session (protocol 10 and 12): it does
    /// not know the token, the refresh token ran out with it, or the session
    /// or its device was revoked. Nothing but signing in again follows any
    /// of them.
    var lumiaSignedOut: Bool {
        switch self as? LumiaError {
        case let .Api(code, _, _)?: ["UNAUTHENTICATED", "TOKEN_EXPIRED", "SESSION_REVOKED", "DEVICE_REVOKED"].contains(code)
        case .DeviceRevoked?, .NotSignedIn?: true
        case .Other?, nil: false
        }
    }

    /// What to show for the failure.
    var lumiaMessage: String {
        switch self as? LumiaError {
        case let .Api(_, message, _)?: message
        case let .DeviceRevoked(version)?: "This device was revoked (roster v\(version))."
        case .NotSignedIn?: "Not signed in."
        case let .Other(message)?: message
        case nil: localizedDescription
        }
    }
}
