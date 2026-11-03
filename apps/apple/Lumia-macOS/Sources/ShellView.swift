// The app shell on the Quiet Split: the server rail, a sidebar (a
// server's channels, or Home's conversations and tiles), the main
// column, and an aside for a conversation's details. Mirrors the web's
// app.tsx; the Home screens are in HomeViews.swift.
import LumiaFFI
import LumiaKit
import SwiftUI

/// Where the shell is: a server (with a channel) or Home (with a view).
enum Place: Hashable {
    case server(id: String)
    case home
}

enum HomeView: Hashable {
    case dm, requests, devices, new
}

struct ShellView: View {
    @Environment(AppStore.self) private var store
    let session: LumiaSession
    @State private var place: Place = .home
    @State private var channelID: String?
    @State private var homeView: HomeView = .dm
    @State private var dmID: String?

    private var snapshot: SnapshotSummary? { session.snapshot }
    private var servers: [ServerSummary] { snapshot?.servers ?? [] }
    private var server: ServerSummary? {
        if case let .server(id) = place { return servers.first { $0.serverId == id } }
        return nil
    }
    private var channels: [ChannelSummary] {
        guard let server else { return [] }
        return (snapshot?.channels ?? []).filter { $0.serverId == server.serverId && !personalKinds.contains($0.kind) }
    }

    var body: some View {
        HStack(spacing: 0) {
            rail
            Divider().overlay(LumiaTokens.railDivider.color)
            sidebar
                .frame(width: 240)
                .background(LumiaTokens.surface.color)
            Divider().overlay(LumiaTokens.panelEdge.color)
            main
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(LumiaTokens.surface.color)
            if place == .home, homeView == .dm, let dmID {
                Divider().overlay(LumiaTokens.panelEdge.color)
                ConversationAside(session: session, channelID: dmID)
                    .frame(width: 240)
                    .background(LumiaTokens.surface.color)
            }
        }
        .task { await session.load() }
        .onChange(of: channels.map(\.channelId)) { _, ids in
            if channelID == nil || !ids.contains(channelID ?? "") { channelID = ids.first }
        }
        .onChange(of: servers.map(\.serverId)) { _, ids in
            if case .home = place, channelID == nil, let first = ids.first, snapshot?.channels.contains(where: { personalKinds.contains($0.kind) }) == false {
                place = .server(id: first)
            }
        }
    }

    // MARK: Rail

    private var rail: some View {
        VStack(spacing: LumiaTokens.space8) {
            railButton(current: place == .home, badge: snapshot?.pendingRequests.count ?? 0) {
                place = .home
            } label: {
                Image("Logo").resizable().scaledToFit().padding(LumiaTokens.space8)
            }
            .help("Home")
            Rectangle().fill(LumiaTokens.railDivider.color).frame(width: 24, height: 1)
            ForEach(servers, id: \.serverId) { s in
                railButton(current: server?.serverId == s.serverId, badge: 0) {
                    place = .server(id: s.serverId)
                } label: {
                    Text(initials(of: s.name)).font(LumiaTokens.titleMonogram.font).foregroundStyle(LumiaTokens.ink.color)
                }
                .help(s.name)
            }
            Spacer()
            LumiaAvatar(initials: initials(of: session.selfName), tone: .of(session.me), size: LumiaTokens.avatarSelf)
                .help(session.selfName)
                .padding(.bottom, LumiaTokens.space12)
        }
        .padding(.top, LumiaTokens.space12)
        .frame(width: LumiaTokens.railWidth)
        .background(LumiaTokens.rail.color)
    }

    private func railButton<L: View>(current: Bool, badge: Int, action: @escaping () -> Void, @ViewBuilder label: () -> L) -> some View {
        Button(action: action) {
            label()
                .frame(width: LumiaTokens.tileSize, height: LumiaTokens.tileSize)
                .background(current ? LumiaTokens.tabActive.color : LumiaTokens.tile.color, in: RoundedRectangle(cornerRadius: current ? LumiaTokens.radius14 : LumiaTokens.radiusRound))
                .overlay(alignment: .topTrailing) {
                    if badge > 0 {
                        Text("\(badge)").font(LumiaTokens.textCount.font).foregroundStyle(LumiaTokens.onAccent.color)
                            .padding(.horizontal, LumiaTokens.space4).frame(minWidth: 16, minHeight: 16)
                            .background(LumiaTokens.unread.color, in: Capsule())
                            .offset(x: 4, y: -4)
                    }
                }
        }
        .buttonStyle(.plain)
    }

    // MARK: Sidebar

    @ViewBuilder
    private var sidebar: some View {
        if place == .home {
            HomeSidebar(session: session, view: $homeView, dmID: $dmID)
        } else {
            VStack(alignment: .leading, spacing: 0) {
                Text(server?.name ?? "Lumia")
                    .font(LumiaTokens.titleServer.font).foregroundStyle(LumiaTokens.ink.color)
                    .frame(height: LumiaTokens.headerHeight).padding(.horizontal, LumiaTokens.space16)
                Divider().overlay(LumiaTokens.line.color)
                List(selection: $channelID) {
                    Section("Channels") {
                        ForEach(channels, id: \.channelId) { c in
                            Label(c.name, systemImage: "number").tag(c.channelId)
                        }
                    }
                }
                .scrollContentBackground(.hidden)
                Spacer(minLength: 0)
                Text(session.connection.label)
                    .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                    .padding(LumiaTokens.space12)
            }
        }
    }

    // MARK: Main

    @ViewBuilder
    private var main: some View {
        if place == .home {
            HomeMain(session: session, view: homeView, dmID: $dmID, go: { view, id in
                homeView = view
                if let id { dmID = id }
            })
        } else if let channel = channels.first(where: { $0.channelId == channelID }) {
            ChannelView(session: session, channel: channel)
        } else {
            LumiaEmptyState(icon: "number", title: "No channel yet") {
                Text("Create a server with the CLI and it appears here.")
            }
        }
    }
}

struct ChannelView: View {
    let session: LumiaSession
    let channel: ChannelSummary
    @State private var draft = ""
    @State private var error = ""

    private var messages: [MessageSummary] { session.messages[channel.channelId] ?? [] }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: LumiaTokens.space6) {
                Image(systemName: "number").foregroundStyle(LumiaTokens.inkQuiet.color)
                Text(channel.name).font(LumiaTokens.titleChannel.font).foregroundStyle(LumiaTokens.ink.color)
                Spacer()
            }
            .frame(height: LumiaTokens.headerHeight).padding(.horizontal, LumiaTokens.space16)
            Divider().overlay(LumiaTokens.line.color)
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        if messages.isEmpty {
                            LumiaEmptyState(icon: "bubble.left", title: "Nothing here yet") { Text("Say something.") }
                        } else {
                            LumiaDayDivider(label: "Today")
                        }
                        ForEach(messages, id: \.messageId) { m in
                            LumiaMessageRow(author: session.name(of: m.authorAccountId), tone: .of(m.authorAccountId), time: time(ofMicros: m.createdAtMicros), text: m.body, deleted: m.deleted)
                                .id(m.messageId)
                        }
                        if !error.isEmpty { LumiaErrorText(error).padding(.horizontal, LumiaTokens.space16) }
                    }
                    .padding(.vertical, LumiaTokens.space8)
                }
                .onChange(of: messages.count) { _, _ in
                    if let last = messages.last { proxy.scrollTo(last.messageId, anchor: .bottom) }
                }
            }
            LumiaComposer(text: $draft, placeholder: "Message #\(channel.name)") { Task { await send() } }
        }
        .task(id: channel.channelId) { await session.refreshMessages(channelID: channel.channelId) }
    }

    private func send() async {
        let body = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !body.isEmpty else { return }
        draft = ""
        error = ""
        do { try await session.sendMessage(channelID: channel.channelId, body: body) } catch { self.error = error.lumiaMessage }
    }
}
