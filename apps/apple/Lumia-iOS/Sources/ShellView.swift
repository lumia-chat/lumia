// The app shell on a phone: the rail beside a sidebar (Home's
// conversations and tiles, or a server's channels) as the root of a
// navigation stack, and every other screen a floating panel pushed over
// it. Mirrors the web's app.tsx and the macOS ShellView, folded.
import LumiaFFI
import LumiaKit
import SwiftUI

struct ShellView: View {
    @Environment(\.scenePhase) private var scenePhase
    let session: LumiaSession
    @State private var place: Place = .home
    @State private var path: [Screen] = []

    private var snapshot: SnapshotSummary? { session.snapshot }
    private var servers: [ServerSummary] { snapshot?.servers ?? [] }

    var body: some View {
        NavigationStack(path: $path) {
            HStack(spacing: 0) {
                PhoneRail(session: session, place: $place)
                switch place {
                case .home:
                    HomeScreen(session: session, path: $path)
                case let .server(id):
                    if let server = servers.first(where: { $0.serverId == id }) {
                        ServerScreen(session: session, server: server, path: $path)
                    } else {
                        HomeScreen(session: session, path: $path)
                    }
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(LumiaTokens.bg.color.ignoresSafeArea())
            .toolbar(.hidden, for: .navigationBar)
            .navigationDestination(for: Screen.self) { screen in
                switch screen {
                case let .conversation(id): ConversationScreen(session: session, channelID: id)
                case let .channel(id): ChannelScreen(session: session, channelID: id)
                case .requests: RequestsScreen(session: session, path: $path)
                case .devices: DevicesScreen(session: session)
                case .newMessage: NewMessageScreen(session: session, path: $path)
                }
            }
        }
        .task {
            await session.load()
            PushRegistrar.shared.register()
        }
        .task(id: PushRegistrar.shared.token) { await PushRegistrar.shared.subscribe(session) }
        .onChange(of: servers.map(\.serverId)) { _, ids in
            // With no conversation yet, a first server is the better place to land.
            if case .home = place, let first = ids.first, snapshot?.channels.contains(where: { personalKinds.contains($0.kind) }) == false {
                place = .server(id: first)
            }
            #if DEBUG
            // The smoke run (scripts/smoke-ios.sh) opens one screen per launch
            // through LUMIA_DEV_SCREEN: home, server, channel, requests, devices, new.
            if let first = ids.first, let screen = ProcessInfo.processInfo.environment["LUMIA_DEV_SCREEN"] {
                let channel = snapshot?.channels.first { $0.serverId == first && !personalKinds.contains($0.kind) }
                switch screen {
                case "home": place = .home
                case "server": place = .server(id: first)
                case "channel": place = .server(id: first); if let channel { path = [.channel(id: channel.channelId)] }
                case "requests": place = .home; path = [.requests]
                case "devices": place = .home; path = [.devices]
                case "new": place = .home; path = [.newMessage]
                default: break
                }
            }
            #endif
        }
        .onChange(of: scenePhase) { _, phase in
            // The single-writer lock follows the app's phase (architecture 15.5).
            switch phase {
            case .active: session.foreground()
            case .background: session.background()
            default: break
            }
        }
    }
}

// MARK: Home

struct HomeScreen: View {
    @Environment(AppStore.self) private var store
    let session: LumiaSession
    @Binding var path: [Screen]

    private var byHost: [(host: String, channels: [ChannelSummary])] {
        var order: [String] = []
        var groups: [String: [ChannelSummary]] = [:]
        for c in session.snapshot?.channels ?? [] where personalKinds.contains(c.kind) {
            let host = c.host.isEmpty ? session.host : c.host
            if groups[host] == nil { order.append(host) }
            groups[host, default: []].append(c)
        }
        return order.map { ($0, groups[$0] ?? []) }
    }

    var body: some View {
        VStack(spacing: 0) {
            SideHeader(title: "Home", action: ("square.and.pencil", "New message", { path.append(.newMessage) }))
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                    HStack(spacing: LumiaTokens.space6) {
                        Tile(label: "Requests", icon: "at", count: session.snapshot?.pendingRequests.count ?? 0) { path.append(.requests) }
                        Tile(label: "Devices", icon: "shield") { path.append(.devices) }
                        Tile(label: "New", icon: "square.and.pencil") { path.append(.newMessage) }
                    }
                    if !session.hasIdentity {
                        LumiaNotice(icon: "shield", tone: .accent, action: ("Link", { store.route = .link })) {
                            Text("Personal messages are end-to-end encrypted. This phone has no device key yet; a device you already use approves it.")
                        }
                    } else if byHost.isEmpty {
                        LumiaEmptyState(icon: "lock", title: "Your conversations", action: ("New message", { path.append(.newMessage) })) {
                            Text("Personal messages are end-to-end encrypted. Start one with someone you share a server with.")
                        }
                    }
                    ForEach(byHost, id: \.host) { group in
                        VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                            SectionTitle(group.host)
                            ForEach(group.channels, id: \.channelId) { c in
                                ConversationRow(session: session, channel: c) { path.append(.conversation(id: c.channelId)) }
                            }
                        }
                    }
                }
                .padding(.horizontal, LumiaTokens.space12)
                .padding(.bottom, LumiaTokens.space24)
            }
            Text(session.connection.label)
                .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                .padding(.horizontal, LumiaTokens.space16).padding(.vertical, LumiaTokens.space6)
        }
        .task(id: byHost.flatMap { $0.channels.map(\.channelId) }) {
            // The rows show each conversation's last line.
            for c in byHost.flatMap(\.channels) where session.dmLogs[c.channelId] == nil {
                await session.refreshDmLog(channelID: c.channelId)
            }
        }
    }
}

/// A conversation in Home's list: who, when, and the last line.
struct ConversationRow: View {
    let session: LumiaSession
    let channel: ChannelSummary
    let open: () -> Void

    private var others: [String] { channel.participantAccountIds.filter { $0 != session.me } }
    private var name: String { session.conversationName(channel) }
    private var last: DmMessageSummary? { session.dmLogs[channel.channelId]?.messages.last }

    var body: some View {
        Button(action: open) {
            HStack(spacing: LumiaTokens.space12) {
                LumiaAvatar(initials: initials(of: name), tone: .of(others.first ?? channel.channelId), size: LumiaTokens.avatarLg)
                VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                    HStack(alignment: .firstTextBaseline, spacing: LumiaTokens.space8) {
                        Text(name).font(LumiaTokens.textAuthor.font).foregroundStyle(LumiaTokens.inkEmphasis.color).lineLimit(1)
                        Spacer(minLength: 0)
                        if let last { Text(rowTime(ofMicros: last.sentAtMicros)).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color) }
                    }
                    Text(preview).font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.inkSecondary.color).lineLimit(1)
                }
            }
            .padding(LumiaTokens.space10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius12))
        }
        .buttonStyle(.plain)
    }

    private var preview: String {
        guard let last else { return channel.host.isEmpty ? "End-to-end encrypted" : "on \(channel.host)" }
        if last.deleted { return "Message deleted" }
        if last.mine { return "You: \(last.body)" }
        if others.count > 1 { return "\(session.name(of: last.senderAccountId.isEmpty ? last.senderIdentityId : last.senderAccountId)): \(last.body)" }
        return last.body
    }
}

// MARK: A server

struct ServerScreen: View {
    let session: LumiaSession
    let server: ServerSummary
    @Binding var path: [Screen]

    private var channels: [ChannelSummary] {
        (session.snapshot?.channels ?? []).filter { $0.serverId == server.serverId && !personalKinds.contains($0.kind) }
    }

    var body: some View {
        VStack(spacing: 0) {
            SideHeader(title: server.name, meta: session.host, action: ("square.and.pencil", "New message", { path.append(.newMessage) }))
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                    SectionTitle("Channels")
                    if channels.isEmpty {
                        Text("Create a channel with the CLI and it appears here.")
                            .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                            .padding(LumiaTokens.space10)
                    }
                    ForEach(channels, id: \.channelId) { c in
                        Button { path.append(.channel(id: c.channelId)) } label: {
                            HStack(spacing: LumiaTokens.space8) {
                                Text("#").font(LumiaTokens.textNav.font).foregroundStyle(LumiaTokens.inkMuted.color).frame(width: 12)
                                Text(c.name).font(LumiaTokens.textNav.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1)
                                Spacer(minLength: 0)
                            }
                            .padding(.horizontal, LumiaTokens.space10)
                            .frame(height: LumiaTokens.controlHeight)
                            .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius10))
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(.horizontal, LumiaTokens.space12)
                .padding(.bottom, LumiaTokens.space24)
            }
            Text(session.connection.label)
                .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                .padding(.horizontal, LumiaTokens.space16).padding(.vertical, LumiaTokens.space6)
        }
    }
}
