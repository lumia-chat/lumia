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
    public func createInvite(serverID: String, maxUses: UInt32 = 0, expiresInHours: UInt32? = nil) throws -> InviteSummary {
        try inner.createInvite(serverId: serverID, maxUses: maxUses, expiresInHours: expiresInHours)
    }
    /// Joins the invite's server; returns its id.
    public func acceptInvite(_ linkOrCode: String) throws -> String { try inner.acceptInvite(linkOrCode: linkOrCode) }
    /// A server this account owns.
    public func createServer(name: String) throws -> ServerSummary { try inner.createServer(name: name) }
    /// A text channel in a server this account may manage channels in.
    public func createChannel(serverID: String, name: String) throws -> ChannelSummary { try inner.createChannel(serverId: serverID, name: name) }
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

    public func pushSubscribe(platform: String, token: String) throws { try inner.pushSubscribe(platform: platform, token: token) }
    public func pushUnsubscribe(platform: String) throws { try inner.pushUnsubscribe(platform: platform) }

    public func dmChannels() throws -> [DmChannelSummary] { try inner.dmChannels() }
    public func dmOpen(accountIDs: [String]) throws -> DmOpened { try inner.dmOpen(accountIds: accountIDs) }
    public func dmRequest(accountID: String) throws -> DmRequestSummary { try inner.dmRequest(accountId: accountID) }
    public func dmRespond(requestID: String, response: String) throws -> DmResponse { try inner.dmRespond(requestId: requestID, response: response) }
    public func dmSend(channelID: String, body: String) throws { try inner.dmSend(channelId: channelID, body: body) }
    public func dmDelete(channelID: String, messageID: String) throws { try inner.dmDelete(channelId: channelID, messageId: messageID) }
    public func dmRejoin(channelID: String) throws { try inner.dmRejoin(channelId: channelID) }
    public func dmMessages(channelID: String) throws -> DmLogSummary { try inner.dmMessages(channelId: channelID) }
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
        if case let LumiaError.Api(code, _)? = self as? LumiaError { return code }
        return nil
    }

    /// What to show for the failure.
    var lumiaMessage: String {
        switch self as? LumiaError {
        case let .Api(_, message)?: message
        case let .DeviceRevoked(version)?: "This device was revoked (roster v\(version))."
        case .NotSignedIn?: "Not signed in."
        case let .Other(message)?: message
        case nil: localizedDescription
        }
    }
}
