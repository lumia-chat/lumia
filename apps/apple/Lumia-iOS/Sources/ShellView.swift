// The app shell on a phone: the rail beside a sidebar (Home's
// conversations and tiles, or a server's channels) as the root of a
// navigation stack, and every other screen a floating panel pushed over
// it. Mirrors the web's app.tsx, folded.
import LumiaFFI
import LumiaKit
import SwiftUI

struct ShellView: View {
    @Environment(\.scenePhase) private var scenePhase
    let session: LumiaSession
    @State private var place: Place = .home
    @State private var path: [Screen] = []
    /// Whose profile card is open, over whatever is on screen.
    @State private var person: PersonInServer?

    private var snapshot: SnapshotSummary? { session.snapshot }
    private var servers: [ServerSummary] { snapshot?.servers ?? [] }
    private var channels: [ChannelSummary] { snapshot?.channels ?? [] }

    var body: some View {
        NavigationStack(path: $path) {
            HStack(spacing: 0) {
                PhoneRail(session: session, place: $place, add: { path = [.addServer] }, open: { path = [$0] })
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
                case let .channel(id): ChannelScreen(session: session, channelID: id, path: $path)
                case .requests: RequestsScreen(session: session, path: $path)
                case .devices: DevicesScreen(session: session)
                case .profile: ProfileScreen(session: session)
                case .preferences: PreferencesScreen()
                case .newMessage: NewMessageScreen(session: session, path: $path)
                case .addServer: AddServerScreen(session: session) { id in path = []; place = .server(id: id) }
                case let .newChannel(serverID): NewChannelScreen(session: session, server: servers.first { $0.serverId == serverID }) { id in path = [.channel(id: id)] }
                case let .invite(serverID): InviteScreen(session: session, server: servers.first { $0.serverId == serverID })
                case let .serverSettings(serverID): ServerSettingsScreen(session: session, serverID: serverID)
                case let .browse(serverID): BrowseScreen(session: session, serverID: serverID, path: $path)
                case let .channelMembers(channelID): ChannelMembersScreen(session: session, channelID: channelID, path: $path)
                case let .addPeople(channelID): AddPeopleScreen(session: session, channelID: channelID)
                case let .newServerDM(serverID): NewServerDMScreen(session: session, serverID: serverID, path: $path)
                case let .yourSettings(serverID): YourSettingsScreen(session: session, serverID: serverID)
                case let .mentions(serverID): MentionsScreen(session: session, serverID: serverID) { openMention($0, from: serverID) }
                }
            }
        }
        .sheet(item: $person) { person in
            ProfileCard(session: session, person: person) { screen in
                self.person = nil
                path.append(screen)
            }
        }
        .environment(\.openPerson, OpenPerson { person = $0 })
        .task {
            PushRegistrar.shared.session = session
            await session.load()
            PushRegistrar.shared.register()
        }
        // The app icon's badge follows the counts, and a conversation read
        // here or elsewhere takes its notifications with it (architecture 6.6).
        .onChange(of: session.reads) { before, after in
            PushRegistrar.shared.readsChanged(before: before, after: after)
        }
        .task(id: PushRegistrar.shared.token) { await PushRegistrar.shared.subscribe(session) }
        // The server on screen is the one whose presence this client wants (protocol 6.7).
        .onChange(of: place, initial: true) { _, place in
            if case let .server(id) = place { session.watchPresence(serverIDs: [id]) } else { session.watchPresence(serverIDs: []) }
        }
        .onChange(of: session.connection) { _, state in
            if state == .connected, case let .server(id) = place { session.watchPresence(serverIDs: [id]) }
            // A token the instance did not take (no network when it was
            // offered) is offered again at each welcome; one it holds is not sent.
            if state == .connected { Task { await PushRegistrar.shared.subscribe(session) } }
        }
        .onChange(of: servers.map(\.serverId)) { _, ids in
            // With no conversation yet, a first server is the better place to land.
            if case .home = place, let first = ids.first, snapshot?.channels.contains(where: { personalKinds.contains($0.kind) }) == false {
                place = .server(id: first)
            }
            #if DEBUG
            // The smoke run (scripts/smoke-ios.sh) opens one screen per launch
            // through LUMIA_DEV_SCREEN: home, server, channel, requests, devices, profile,
            // preferences, new, add, newchannel, invite, serversettings, mentions (the server's Mentions page),
            // homementions (Home's; `channel` opens LUMIA_DEV_CHANNEL's channel when it names one), and for channel membership
            // browse, preview (Browse's first channel, read-only), members, addpeople and profilecard
            // (of the channel LUMIA_DEV_CHANNEL names, else the first; the card is its first member
            // but this account, or LUMIA_DEV_PERSON's handle), serverdm (the first server DM), newdm
            // and yoursettings; the push smoke opens the account's first personal conversation with `conversation`. (LUMIA_DEV_APPEARANCE, LUMIA_DEV_PRESET and LUMIA_DEV_FONT_SIZE
            // choose how that launch looks: `Preferences` in LumiaKit reads them.)
            if let first = ids.first, let screen = ProcessInfo.processInfo.environment["LUMIA_DEV_SCREEN"] {
                let named = ProcessInfo.processInfo.environment["LUMIA_DEV_CHANNEL"].flatMap { name in snapshot?.channels.first { $0.serverId == first && $0.name == name } }
                let channel = named ?? snapshot?.channels.first { $0.serverId == first && !personalKinds.contains($0.kind) }
                let conversation = snapshot?.channels.first { personalKinds.contains($0.kind) }
                switch screen {
                case "home": place = .home
                case "server": place = .server(id: first)
                case "channel": place = .server(id: first); if let channel { path = [.channel(id: channel.channelId)] }
                case "conversation": place = .home; if let conversation { path = [.conversation(id: conversation.channelId)] }
                case "requests": place = .home; path = [.requests]
                case "devices": place = .home; path = [.devices]
                case "profile": place = .home; path = [.profile]
                case "preferences": place = .home; path = [.preferences]
                case "new": place = .home; path = [.newMessage]
                case "add": place = .home; path = [.addServer]
                case "newchannel": place = .server(id: first); path = [.newChannel(serverID: first)]
                case "invite": place = .server(id: first); path = [.invite(serverID: first)]
                case "serversettings": place = .server(id: first); path = [.serverSettings(serverID: first)]
                case "mentions": place = .server(id: first); path = [.mentions(serverID: first)]
                case "homementions": place = .home; path = [.mentions(serverID: nil)]
                default: devMembershipScreen(screen, serverID: first)
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
        .onChange(of: PushRegistrar.shared.target) { _, _ in openTarget() }
        .onChange(of: channels.map(\.channelId)) { _, _ in openTarget() }
    }

    #if DEBUG
    /// The channel membership screens a smoke run opens (`LUMIA_DEV_SCREEN`).
    private func devMembershipScreen(_ screen: String, serverID: String) {
        let env = ProcessInfo.processInfo.environment
        let named = env["LUMIA_DEV_CHANNEL"].flatMap { name in channels.first { $0.serverId == serverID && $0.name == name } }
        guard let channel = named ?? Membership.channels(channels, serverID: serverID).first else { return }
        place = .server(id: serverID)
        switch screen {
        case "browse": path = [.browse(serverID: serverID)]
        case "members": path = [.channel(id: channel.channelId), .channelMembers(channelID: channel.channelId)]
        case "addpeople": path = [.channel(id: channel.channelId), .channelMembers(channelID: channel.channelId), .addPeople(channelID: channel.channelId)]
        case "serverdm": if let dm = Membership.serverDMs(channels, serverID: serverID).first { path = [.channel(id: dm.channelId)] }
        case "newdm": path = [.newServerDM(serverID: serverID)]
        case "yoursettings": path = [.yourSettings(serverID: serverID)]
        case "preview":
            Task {
                guard let first = try? await session.browseChannels(serverID: serverID).first else { return }
                session.preview(first)
                path = [.browse(serverID: serverID), .channel(id: first.channelId)]
            }
        case "profilecard":
            path = [.channel(id: channel.channelId), .channelMembers(channelID: channel.channelId)]
            Task {
                guard let ids = try? await session.channelMembers(channelID: channel.channelId) else { return }
                let handle = env["LUMIA_DEV_PERSON"]
                let members = session.serverMembers[serverID] ?? []
                let chosen = handle.flatMap { h in members.first { $0.handle == h }?.accountId } ?? ids.first { $0 != session.me }
                if let chosen { person = PersonInServer(accountID: chosen, serverID: serverID) }
            }
        default: break
        }
    }
    #endif

    /// A mention tapped on a Mentions page: its channel, at it. From a
    /// server's page the channel is pushed over it; from Home's, its server
    /// is opened with the channel on it.
    private func openMention(_ m: MessageSummary, from serverID: String?) {
        guard let channel = session.channel(m.channelId) else { return }
        session.pendingReveal = Reveal(channelID: m.channelId, messageID: m.messageId)
        if serverID != nil {
            path.append(.channel(id: channel.channelId))
        } else {
            place = .server(id: channel.serverId)
            path = [.channel(id: channel.channelId)]
        }
    }

    /// The conversation a tapped notification names, once the snapshot has
    /// it (a cold start loads first): a DM opens from Home, a mention opens
    /// its channel in its server. The generic notice names none and the app
    /// stays where it was.
    private func openTarget() {
        guard let target = PushRegistrar.shared.target,
              let channel = channels.first(where: { $0.channelId == target.channelID }) else { return }
        PushRegistrar.shared.target = nil
        if personalKinds.contains(channel.kind) {
            place = .home
            path = [.conversation(id: channel.channelId)]
        } else {
            place = .server(id: channel.serverId)
            path = [.channel(id: channel.channelId)]
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
                    // Every server's messages that mention this account or reply to it (architecture 6.6).
                    MentionsRow(count: session.homeMentions) { path.append(.mentions(serverID: nil)) }
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
                    Button("Add a server") { path.append(.addServer) }
                        .buttonStyle(.lumiaLink)
                        .padding(.horizontal, LumiaTokens.space10).padding(.top, LumiaTokens.space8)
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
        // Unread: the name and last line in the strong ink and weight, and how many at the end.
        let unread = session.unread(channel.channelId)
        let bold = unread.unread > 0
        Button(action: open) {
            HStack(spacing: LumiaTokens.space12) {
                LumiaAvatar(initials: initials(of: name), tone: .of(others.first ?? channel.channelId), size: LumiaTokens.avatarLg)
                VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                    // The board's: the name at 700 while unread and 600 after, the
                    // last line in the body's ink while unread and muted after.
                    HStack(alignment: .firstTextBaseline, spacing: LumiaTokens.space8) {
                        Text(name).font(LumiaTokens.textAuthor.font).fontWeight(bold ? .bold : .semibold).foregroundStyle(LumiaTokens.ink.color).lineLimit(1)
                        Spacer(minLength: 0)
                        if let last { Text(rowTime(ofMicros: last.sentAtMicros)).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color) }
                    }
                    HStack(spacing: LumiaTokens.space8) {
                        Text(preview).font(.system(size: LumiaTokens.textMember.size))
                            .foregroundStyle(bold ? LumiaTokens.inkBody.color : LumiaTokens.inkMuted.color).lineLimit(1)
                        Spacer(minLength: 0)
                        if unread.mentions > 0 { LumiaCountBadge(unread.mentions) }
                    }
                }
            }
            .padding(LumiaTokens.space10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius12))
        }
        .buttonStyle(.plain)
        .modifier(UnreadLabel(unread: unread))
    }

    private var preview: String {
        guard let last else { return channel.host.isEmpty ? "End-to-end encrypted" : "on \(channel.host)" }
        if last.deleted { return "Message deleted" }
        if last.mine { return "You: \(last.body)" }
        if others.count > 1 { return "\(session.name(of: last.senderAccountId.isEmpty ? last.senderIdentityId : last.senderAccountId)): \(last.body)" }
        return last.body
    }
}
