// The app shell on the Quiet Split, as the design canvas draws it: the
// server rail, a sidebar on the ground (a server's channels, or Home's
// conversations and tiles), the main pane as a floating panel, and an
// aside panel (a server's members, or a conversation's details). Mirrors
// the web's app.tsx; the Home screens are in HomeViews.swift.
import LumiaFFI
import LumiaKit
import SwiftUI

/// Where the shell is: a server (with a channel) or Home (with a view).
enum Place: Hashable {
    case server(id: String)
    case home
}

enum HomeView: Hashable {
    case dm, requests, devices, profile, new, add
}

struct ShellView: View {
    @Environment(AppStore.self) private var store
    let session: LumiaSession
    @State private var place: Place = .home
    @State private var channelID: String?
    @State private var homeView: HomeView = .dm
    @State private var dmID: String?
    // Invite links for the server: a new one with its limits, and the
    // ones that exist, each revocable.
    @State private var inviteShown = false
    @State private var invite: InviteSummary?
    @State private var invites: [InviteSummary] = []
    @State private var inviteExpiresInHours: UInt32 = 0
    @State private var inviteMaxUses: UInt32 = 0
    @State private var inviteEmail = ""
    @State private var inviteEmailed = ""
    @State private var inviteError = ""
    @State private var inviteBusy = false
    @State private var inviteCopied = false
    // A new channel in the server, named in a popover and opened once made.
    @State private var channelShown = false
    @State private var channelName = ""
    @State private var channelError = ""
    @State private var channelBusy = false

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
            sidebar
                .frame(width: 240)
            main
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .lumiaPanel()
                .padding(.vertical, LumiaTokens.space8)
            if let aside {
                aside
                    .frame(width: 280)
                    .frame(maxHeight: .infinity)
                    .lumiaPanel()
                    .padding(LumiaTokens.space8)
            } else {
                Spacer().frame(width: LumiaTokens.space8)
            }
        }
        .background(LumiaTokens.bg.color)
        .task { await session.load() }
        // The server on screen is the one whose presence this client wants (protocol 6.7).
        .onChange(of: place, initial: true) { _, place in
            if case let .server(id) = place { session.watchPresence(serverIDs: [id]) } else { session.watchPresence(serverIDs: []) }
        }
        .onChange(of: session.connection) { _, state in
            if state == .connected, case let .server(id) = place { session.watchPresence(serverIDs: [id]) }
        }
        .onChange(of: channels.map(\.channelId)) { _, ids in
            if channelID == nil || !ids.contains(channelID ?? "") { channelID = ids.first }
        }
        .onChange(of: servers.map(\.serverId)) { _, ids in
            if case .home = place, channelID == nil, let first = ids.first, snapshot?.channels.contains(where: { personalKinds.contains($0.kind) }) == false {
                place = .server(id: first)
            }
            #if DEBUG
            if ProcessInfo.processInfo.environment["LUMIA_DEV_PLACE"] == "home" { place = .home }
            #endif
        }
    }

    // MARK: Rail

    private var rail: some View {
        VStack(spacing: LumiaTokens.space12) {
            railButton(current: place == .home, badge: snapshot?.pendingRequests.count ?? 0, label: "Home", rounded: true) {
                place = .home
            } label: {
                Image("Logo").resizable().scaledToFit().frame(width: 36, height: 36)
            }
            Rectangle().fill(LumiaTokens.railDivider.color).frame(width: 24, height: 1)
            ForEach(servers, id: \.serverId) { s in
                railButton(current: server?.serverId == s.serverId, badge: 0, label: s.name, rounded: false) {
                    place = .server(id: s.serverId)
                } label: {
                    Text(initials(of: s.name))
                        .font(server?.serverId == s.serverId ? LumiaTokens.titleMonogram.font : LumiaTokens.textAuthor.font)
                        .foregroundStyle(server?.serverId == s.serverId ? LumiaTokens.inkSelected.color : LumiaTokens.ink.color)
                }
            }
            railButton(current: false, badge: 0, label: "Add a server", rounded: false) {
                place = .home
                homeView = .add
            } label: {
                Image(systemName: "plus").font(.system(size: 16, weight: .medium)).foregroundStyle(LumiaTokens.inkSecondary.color)
            }
            Spacer()
            Button {
                place = .home
                homeView = .profile
            } label: {
                LumiaAvatar(initials: initials(of: session.selfName), tone: .of(session.me), size: LumiaTokens.avatarSelf)
                    .overlay(alignment: .bottomTrailing) {
                        Circle().fill(session.connection == .connected ? LumiaTokens.online.color : LumiaTokens.inkMuted.color)
                            .frame(width: 12, height: 12)
                            .overlay(Circle().stroke(LumiaTokens.rail.color, lineWidth: 2))
                            .offset(x: 2, y: 2)
                    }
            }
            .buttonStyle(.plain)
            .help(session.selfName + " · " + session.connection.label + " — open profile")
            .accessibilityLabel(session.selfName + ", open profile")
            .padding(.bottom, LumiaTokens.space12)
        }
        .padding(.top, LumiaTokens.space16)
        .frame(width: LumiaTokens.railWidth)
        .frame(maxHeight: .infinity)
        .background(LumiaTokens.rail.color)
    }

    private func railButton<L: View>(current: Bool, badge: Int, label: String, rounded: Bool, action: @escaping () -> Void, @ViewBuilder label content: () -> L) -> some View {
        let radius = rounded ? LumiaTokens.radius14 : LumiaTokens.tileSize / 2
        return Button(action: action) {
            content()
                .frame(width: LumiaTokens.tileSize, height: LumiaTokens.tileSize)
                .background(current && !rounded ? LumiaTokens.raised.color : LumiaTokens.tile.color, in: RoundedRectangle(cornerRadius: radius))
                .overlay {
                    if current {
                        RoundedRectangle(cornerRadius: radius)
                            .stroke(LumiaTokens.accent.color, lineWidth: 2)
                            .padding(-4)
                    }
                }
                .overlay(alignment: .topTrailing) {
                    if badge > 0 {
                        Text("\(badge)").font(LumiaTokens.textCount.font).foregroundStyle(LumiaTokens.onAccent.color)
                            .padding(.horizontal, 5).frame(minWidth: 20, minHeight: 20)
                            .background(LumiaTokens.accent.color, in: Capsule())
                            .overlay(Capsule().stroke(LumiaTokens.rail.color, lineWidth: 2))
                            .offset(x: 5, y: -5)
                    }
                }
        }
        .buttonStyle(.plain)
        .help(label)
    }

    // MARK: Sidebar

    @ViewBuilder
    private var sidebar: some View {
        if place == .home {
            HomeSidebar(session: session, view: $homeView, dmID: $dmID)
        } else {
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: LumiaTokens.space6) {
                    Text(server?.name ?? "Lumia")
                        .font(LumiaTokens.titleServer.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1)
                    Image(systemName: "chevron.down").font(.system(size: 11, weight: .bold)).foregroundStyle(LumiaTokens.inkSecondary.color)
                    Spacer(minLength: 0)
                    Button { place = .home; homeView = .new } label: {
                        Image(systemName: "square.and.pencil").font(.system(size: 14, weight: .medium))
                            .foregroundStyle(LumiaTokens.iconStrong.color).frame(width: 34, height: 34)
                    }
                    .buttonStyle(.plain).help("New message")
                }
                .padding(.leading, LumiaTokens.space8)
                .frame(height: 38)
                .padding(.bottom, LumiaTokens.space10)
                ScrollView {
                    VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                        Text("Channels").font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.inkMuted.color)
                            .padding(.horizontal, LumiaTokens.space10).padding(.top, LumiaTokens.space4).padding(.bottom, LumiaTokens.space6)
                        ForEach(channels, id: \.channelId) { c in
                            ChannelRow(name: c.name, active: channelID == c.channelId) { channelID = c.channelId }
                        }
                        if channels.isEmpty {
                            Text("No channels yet. Add one with New channel below.")
                                .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                                .padding(LumiaTokens.space10)
                        }
                        Button("New channel") { channelShown = true }
                            .buttonStyle(.lumiaLink)
                            .padding(.horizontal, LumiaTokens.space10).padding(.top, LumiaTokens.space8)
                            .popover(isPresented: $channelShown, arrowEdge: .bottom) { channelPopover }
                        Button("Invite people") { inviteShown = true }
                            .buttonStyle(.lumiaLink)
                            .padding(.horizontal, LumiaTokens.space10).padding(.top, LumiaTokens.space4)
                            .popover(isPresented: $inviteShown, arrowEdge: .bottom) { invitePopover }
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(.top, LumiaTokens.space14)
            .padding(.leading, LumiaTokens.space12)
            .padding(.trailing, LumiaTokens.space10)
        }
    }

    // MARK: New channel

    /// A text channel in the server; it opens once made.
    private var channelPopover: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space12) {
            Text("New channel").font(LumiaTokens.titleAuth.font).foregroundStyle(LumiaTokens.ink.color)
            Text("A text channel everyone in \(server?.name ?? "the server") can see.").font(LumiaTokens.textSubtitle.font).foregroundStyle(LumiaTokens.inkSecondary.color)
            LumiaField("Name", text: $channelName, placeholder: "general")
            if !channelError.isEmpty { LumiaErrorText(channelError) }
            Button("Create") { Task { await createChannel() } }
                .buttonStyle(.lumiaPrimary).disabled(channelBusy || channelName.trimmingCharacters(in: .whitespaces).isEmpty)
        }
        .padding(LumiaTokens.space16)
        .frame(width: 320)
    }

    private func createChannel() async {
        guard case let .server(id) = place else { return }
        channelBusy = true
        channelError = ""
        defer { channelBusy = false }
        do {
            channelID = try await session.createChannel(serverID: id, name: channelName)
            channelName = ""
            channelShown = false
        } catch {
            channelError = error.lumiaCode == "PERMISSION_DENIED" ? "You cannot add channels to this server." : error.lumiaMessage
        }
    }

    // MARK: Invite people

    /// Invite links for the server: a new one with an expiry and a use
    /// limit, to copy, and the ones that exist, each revocable.
    private var invitePopover: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space12) {
            Text("Invite people").font(LumiaTokens.titleAuth.font).foregroundStyle(LumiaTokens.ink.color)
            Text("A link to join \(server?.name ?? "the server").").font(LumiaTokens.textSubtitle.font).foregroundStyle(LumiaTokens.inkSecondary.color)
            Picker("Expires", selection: $inviteExpiresInHours) {
                Text("Never").tag(UInt32(0))
                Text("In a day").tag(UInt32(24))
                Text("In a week").tag(UInt32(168))
            }
            .pickerStyle(.segmented)
            Picker("Uses", selection: $inviteMaxUses) {
                Text("Unlimited").tag(UInt32(0))
                Text("Once").tag(UInt32(1))
                Text("Ten").tag(UInt32(10))
            }
            .pickerStyle(.segmented)
            if !inviteError.isEmpty { LumiaErrorText(inviteError) }
            Button("Make a link") { Task { await makeInvite() } }
                .buttonStyle(.lumiaPrimary).disabled(inviteBusy)
            // The invite as a mail (architecture 6.2): one use, a week, to the address.
            LumiaField("Email", text: $inviteEmail, placeholder: "pat@example.com")
            Button("Send by email") { Task { await emailInvite() } }
                .buttonStyle(.lumiaSecondary).disabled(inviteBusy || inviteEmail.trimmingCharacters(in: .whitespaces).isEmpty)
            if !inviteEmailed.isEmpty {
                Text("Sent to \(inviteEmailed); it works once, within a week.").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
            }
            if let invite {
                Text(invite.url)
                    .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.ink.color)
                    .textSelection(.enabled)
                    .padding(LumiaTokens.space10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(LumiaTokens.inset.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius10))
                Button(inviteCopied ? "Copied" : "Copy link") {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(invite.url, forType: .string)
                    inviteCopied = true
                }
                .buttonStyle(.lumiaSecondary)
            }
            if !invites.isEmpty {
                Text("Invites").font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.inkMuted.color)
                ForEach(invites, id: \.code) { i in
                    HStack(spacing: LumiaTokens.space10) {
                        VStack(alignment: .leading, spacing: 0) {
                            Text(i.code).font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.ink.color)
                            Text(describeInvite(i)).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                        }
                        Spacer()
                        Button("Revoke") { Task { await revokeInvite(i) } }
                            .buttonStyle(.lumiaLink).disabled(inviteBusy)
                    }
                }
            }
        }
        .padding(LumiaTokens.space16)
        .frame(width: 380)
        .task(id: inviteShown) {
            guard inviteShown, case let .server(id) = place else { return }
            do {
                invites = try await session.invites(serverID: id)
            } catch {
                inviteError = error.lumiaCode == "PERMISSION_DENIED" ? "You cannot invite people to this server." : error.lumiaMessage
            }
        }
        .onChange(of: place) { _, _ in
            invite = nil
            invites = []
            inviteError = ""
            inviteCopied = false
        }
    }

    private func describeInvite(_ i: InviteSummary) -> String {
        let uses = (i.email.isEmpty ? "" : "to \(i.email) · ") + (i.maxUses == 0 ? "\(i.uses) uses" : "\(i.uses)/\(i.maxUses) uses")
        guard let micros = i.expiresAtMicros else { return "\(uses) · no expiry" }
        let at = Date(timeIntervalSince1970: TimeInterval(micros) / 1_000_000)
        return "\(uses) · expires \(at.formatted(date: .abbreviated, time: .shortened))"
    }

    private func makeInvite() async {
        guard case let .server(id) = place else { return }
        inviteBusy = true
        inviteError = ""
        inviteCopied = false
        defer { inviteBusy = false }
        do {
            let made = try await session.createInvite(serverID: id, maxUses: inviteMaxUses, expiresInHours: inviteExpiresInHours == 0 ? nil : inviteExpiresInHours)
            invite = made
            invites.insert(made, at: 0)
        } catch {
            inviteError = error.lumiaCode == "PERMISSION_DENIED" ? "You cannot invite people to this server." : error.lumiaCode == "RATE_LIMITED" ? "Too many links in the last hour; try later." : error.lumiaMessage
        }
    }

    private func emailInvite() async {
        guard case let .server(id) = place else { return }
        inviteBusy = true
        inviteError = ""
        inviteEmailed = ""
        defer { inviteBusy = false }
        do {
            let made = try await session.emailInvite(serverID: id, email: inviteEmail)
            invites.insert(made, at: 0)
            inviteEmailed = made.email
            inviteEmail = ""
        } catch {
            inviteError = error.lumiaCode == "FAILED_PRECONDITION" ? "This instance sends no mail; make a link instead." : error.lumiaCode == "INVALID_ARGUMENT" ? "That is not an email address." : error.lumiaCode == "RATE_LIMITED" ? "Too many invites in the last hour; try later." : error.lumiaMessage
        }
    }

    private func revokeInvite(_ i: InviteSummary) async {
        inviteBusy = true
        inviteError = ""
        defer { inviteBusy = false }
        do {
            try await session.revokeInvite(i.code)
            invites.removeAll { $0.code == i.code }
            if invite?.code == i.code { invite = nil }
        } catch {
            inviteError = error.lumiaMessage
        }
    }

    // MARK: Main

    @ViewBuilder
    private var main: some View {
        if place == .home {
            HomeMain(session: session, view: homeView, dmID: $dmID, go: { view, id in
                homeView = view
                if let id { dmID = id }
            }, openServer: { id in
                homeView = .dm
                place = .server(id: id)
            })
        } else if let channel = channels.first(where: { $0.channelId == channelID }) {
            ChannelView(session: session, channel: channel)
        } else {
            VStack(spacing: 0) {
                LumiaPaneHeader(title: server?.name ?? "Lumia")
                LumiaEmptyState(icon: "number", title: "No channel yet") {
                    Text("Add a channel with New channel in the sidebar, or a server with the rail's plus.")
                }
                Spacer()
            }
        }
    }

    // MARK: Aside

    private var aside: AnyView? {
        if place == .home {
            if homeView == .dm, let dmID { return AnyView(ConversationAside(session: session, channelID: dmID)) }
            return nil
        }
        guard let server else { return nil }
        return AnyView(MembersAside(session: session, server: server))
    }
}

/// A channel in the sidebar: the hash glyph, the name, the active row raised.
struct ChannelRow: View {
    let name: String
    let active: Bool
    let select: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: select) {
            HStack(spacing: LumiaTokens.space8) {
                Text("#").font(LumiaTokens.textNav.font)
                    .foregroundStyle(active ? LumiaTokens.accentInk.color : LumiaTokens.inkMuted.color)
                    .frame(width: 10)
                Text(name).font(LumiaTokens.textNav.font)
                    .foregroundStyle(active ? LumiaTokens.inkSelected.color : LumiaTokens.inkQuiet.color)
                    .fontWeight(active ? .bold : .regular)
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, LumiaTokens.space10)
            .frame(height: LumiaTokens.rowHeight)
            .background(active ? LumiaTokens.surface.color : (hovering ? LumiaTokens.hoverRow.color : .clear), in: RoundedRectangle(cornerRadius: LumiaTokens.radius8))
            .shadow(color: active ? .black.opacity(0.08) : .clear, radius: 2, y: 1)
            .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius8))
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
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
            LumiaPaneHeader(title: channel.name, glyph: {
                Image(systemName: "number").font(.system(size: 18, weight: .bold)).foregroundStyle(LumiaTokens.accentInk.color)
            })
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: LumiaTokens.space2) {
                        if messages.isEmpty {
                            LumiaEmptyState(icon: "bubble.left", title: "Nothing here yet") { Text("Say something.") }
                        } else {
                            LumiaDayDivider(label: "Today")
                        }
                        ForEach(messages, id: \.messageId) { m in
                            LumiaMessageRow(author: session.name(of: m.authorAccountId), tone: .of(m.authorAccountId), time: time(ofMicros: m.createdAtMicros), text: m.body, deleted: m.deleted)
                                .id(m.messageId)
                        }
                        if !error.isEmpty { LumiaErrorText(error).padding(.horizontal, LumiaTokens.space12) }
                    }
                    .padding(.horizontal, LumiaTokens.space14)
                    .padding(.top, LumiaTokens.space14)
                    .padding(.bottom, LumiaTokens.space8)
                }
                .defaultScrollAnchor(.bottom)
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

/// A server's members, in the aside panel.
struct MembersAside: View {
    let session: LumiaSession
    let server: ServerSummary
    @State private var tab = "members"
    @State private var query = ""

    private var members: [MemberSummary] {
        let all = session.serverMembers[server.serverId] ?? []
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        return all
            .filter { q.isEmpty || $0.handle.lowercased().contains(q) || $0.displayName.lowercased().contains(q) || $0.nickname.lowercased().contains(q) }
            .sorted { name(of: $0).localizedCompare(name(of: $1)) == .orderedAscending }
    }

    private func name(of m: MemberSummary) -> String {
        for candidate in [m.displayName, m.nickname, m.handle] where !candidate.isEmpty { return candidate }
        return String(m.accountId.prefix(8))
    }

    /// The handle and, from the verified profile, the pronouns.
    private func subtitle(of m: MemberSummary) -> String? {
        [m.handle.isEmpty ? "" : "@\(m.handle)", m.pronouns, session.customStatus(of: m.accountId, in: server.serverId)].filter { !$0.isEmpty }.joined(separator: " · ").nilIfEmpty
    }

    var body: some View {
        VStack(spacing: 0) {
            LumiaSegmentedTabs(tabs: [.init(id: "members", label: "Members", count: (session.serverMembers[server.serverId] ?? []).count)], selected: $tab)
                .padding(LumiaTokens.space12)
            Rectangle().fill(LumiaTokens.line.color).frame(height: 1)
            HStack(spacing: LumiaTokens.space8) {
                Image(systemName: "magnifyingglass").font(.system(size: 13)).foregroundStyle(LumiaTokens.inkSecondary.color)
                TextField("Find a member", text: $query).textFieldStyle(.plain).font(LumiaTokens.textBody.font)
            }
            .padding(.horizontal, LumiaTokens.space12)
            .frame(height: 38)
            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius10).stroke(LumiaTokens.border.color))
            .padding(.horizontal, LumiaTokens.space14)
            .padding(.top, LumiaTokens.space12)
            .padding(.bottom, LumiaTokens.space6)
            ScrollView {
                VStack(alignment: .leading, spacing: 1) {
                    // Grouped by presence, as the design's member list is: those here, then the rest.
                    let here = members.filter { session.presence(of: $0.accountId, in: server.serverId) != nil }
                    let away = members.filter { session.presence(of: $0.accountId, in: server.serverId) == nil }
                    if !here.isEmpty {
                        Text("Online · \(here.count)").font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.inkMuted.color)
                            .padding(.horizontal, LumiaTokens.space10).padding(.top, LumiaTokens.space8).padding(.bottom, LumiaTokens.space6)
                        ForEach(here, id: \.accountId) { m in
                            LumiaMemberRow(name: name(of: m), subtitle: subtitle(of: m), tone: .of(m.accountId), tag: m.accountId == session.me ? "You" : nil, status: session.presence(of: m.accountId, in: server.serverId))
                        }
                    }
                    if !away.isEmpty {
                        Text("\(here.isEmpty ? "Members" : "Offline") · \(away.count)").font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.inkMuted.color)
                            .padding(.horizontal, LumiaTokens.space10).padding(.top, LumiaTokens.space8).padding(.bottom, LumiaTokens.space6)
                        ForEach(away, id: \.accountId) { m in
                            LumiaMemberRow(name: name(of: m), subtitle: subtitle(of: m), tone: .of(m.accountId), tag: m.accountId == session.me ? "You" : nil)
                        }
                    }
                }
                .padding(.horizontal, LumiaTokens.space8)
                .padding(.bottom, LumiaTokens.space14)
            }
        }
    }
}
