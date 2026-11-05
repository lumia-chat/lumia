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
    /// Public profiles looked up one at a time; nil once the instance refused one.
    private var profiles: [String: MemberSummary?] = [:]
    private var profilesInFlight: Set<String> = []
    public private(set) var lastError: String?

    private var events: Task<Void, Never>?

    public init(client: LumiaClient) async {
        self.client = client
        self.host = await client.host
        self.signedIn = await client.signedIn
        self.hasIdentity = await client.hasIdentity
    }

    // MARK: Sign in

    public func instanceInfo() async throws -> InstanceSummary { try await client.instanceInfo() }

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

    /// An invite link for a server this account can invite to.
    public func createInvite(serverID: String) async throws -> InviteSummary {
        try await client.createInvite(serverID: serverID)
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

    /// A text channel in the server, listed once the snapshot is refreshed; returns its id.
    public func createChannel(serverID: String, name: String) async throws -> String {
        let channel = try await client.createChannel(serverID: serverID, name: name)
        await refreshSnapshot()
        return channel.channelId
    }

    public func signInIdentity() async throws {
        try await client.signInIdentity()
        signedIn = true
    }

    public func signOut() async {
        stop()
        try? await client.signOut()
        signedIn = false
        snapshot = nil
        messages = [:]
        dmChannels = []
        dmLogs = [:]
    }

    // MARK: Identity

    public func identityLinkRequest(label: String) async throws -> String { try await client.identityLinkRequest(label: label) }

    public func identityClaim(code: String) async throws {
        _ = try await client.identityClaim(code: code)
        hasIdentity = true
        try await client.signInIdentity()
        signedIn = true
    }

    public func identityRevoke(deviceID: String, password: String) async throws {
        roster = try await client.identityRevoke(deviceID: deviceID, password: password)
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
            hasIdentity = await client.hasIdentity
            await refreshPeople(serverIDs: snap.servers.map(\.serverId))
            await refreshDmChannels()
            if events == nil {
                try await start(channelIDs: snap.channels.filter { !personalKinds.contains($0.kind) }.map(\.channelId))
            }
        } catch {
            lastError = error.lumiaMessage
            if case LumiaError.NotSignedIn = error { signedIn = false }
        }
    }

    public func refreshSnapshot() async {
        if let snap = try? await client.snapshot() {
            snapshot = snap
            await refreshPeople(serverIDs: snap.servers.map(\.serverId))
        }
    }

    public func refreshMessages(channelID: String) async {
        if let list = try? await client.messages(channelID: channelID) { messages[channelID] = list }
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

    public func conversationName(_ channel: ChannelSummary) -> String {
        let others = channel.participantAccountIds.filter { $0 != me }
        if others.isEmpty { return "Just you" }
        return others.map { name(of: $0) }.joined(separator: ", ")
    }

    // MARK: Gateway

    private func start(channelIDs: [String]) async throws {
        let stream = try await client.start(channelIDs: channelIDs)
        connection = .connecting
        events = Task { [weak self] in
            for await event in stream {
                guard let self else { return }
                await self.handle(event)
            }
            self?.events = nil
        }
    }

    public func stop() {
        events?.cancel()
        events = nil
        Task { await client.stop() }
        connection = .idle
    }

    /// Going to the background: the single-writer lock is released so the
    /// notification extension can fetch and decrypt (architecture 15.5).
    public func background() {
        Task { await client.background() }
    }

    /// Back in the foreground: the lock is taken again; when the extension
    /// wrote meanwhile, the gateway loop restarts from the store's cursors,
    /// otherwise it is asked for every head.
    public func foreground() {
        Task {
            let wrote = (try? await client.resume()) ?? false
            if wrote {
                stop()
                await load()
            } else {
                try? await client.foreground()
            }
        }
    }

    /// Registers this device for pushes through the relay (`apns`).
    public func pushSubscribe(platform: String, token: String) async {
        try? await client.pushSubscribe(platform: platform, token: token)
    }

    private func handle(_ event: LumiaEvent) async {
        switch event {
        case .welcomed:
            connection = .connected
            await refreshSnapshot()
        case let .applied(channelId, _), let .snapshotTaken(channelId, _):
            await refreshMessages(channelID: channelId)
        case .accountApplied:
            await refreshSnapshot()
            await refreshDmChannels()
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
        case .dmDelivered:
            for id in dmLogs.keys { await refreshDmLog(channelID: id) }
        case let .dmError(message):
            lastError = message
        case .acked, .error, .subscriptionFailed, .dmNote:
            break
        }
    }
}
