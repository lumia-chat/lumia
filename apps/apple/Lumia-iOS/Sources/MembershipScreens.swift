// Channel membership and server DMs on a phone (architecture 6.4 and
// 6.8): Browse, a channel's members and adding people to it, the picker
// for a server DM, this account's own settings in a server, and the
// profile card of someone in a server. The words, states and refusals
// are the web's, the desktop shell's and the Android app's
// (`Membership` in LumiaKit).
import LumiaFFI
import LumiaKit
import SwiftUI

/// Someone in a server, whose profile card is open.
struct PersonInServer: Identifiable, Hashable {
    let accountID: String
    let serverID: String
    var id: String { serverID + "/" + accountID }
}

/// Opens someone's profile card over whatever is on screen.
struct OpenPerson {
    let run: (PersonInServer) -> Void
    func callAsFunction(_ person: PersonInServer) { run(person) }
}

extension EnvironmentValues {
    @Entry var openPerson = OpenPerson { _ in }
}

/// A member of a server as a row says it: the name, and the handle under it.
@MainActor private func memberLine(_ session: LumiaSession, _ accountID: String) -> (name: String, handle: String?) {
    let p = session.people[accountID] ?? session.profile(of: accountID)
    return (session.name(of: accountID), p.map { "@\($0.handle)" })
}

// MARK: Browse

/// The public channels of a server this account is not in, each with its
/// topic, how many are in it and Join; a row opens the channel to read.
struct BrowseScreen: View {
    let session: LumiaSession
    let serverID: String
    @Binding var path: [Screen]
    @State private var channels: [ChannelSummary]?
    @State private var busy: String?
    @State private var error = ""

    private var server: ServerSummary? { session.snapshot?.servers.first { $0.serverId == serverID } }

    var body: some View {
        PanelScreen(back: server?.name ?? "Server", title: "Browse channels", subtitle: server?.name) {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space8) {
                    if !error.isEmpty { LumiaErrorText(error) }
                    if let channels {
                        if channels.isEmpty {
                            LumiaEmptyState(icon: "number", title: "Nothing to browse") { Text("You’re in every public channel here.") }
                        }
                        ForEach(channels, id: \.channelId) { c in
                            HStack(alignment: .center, spacing: LumiaTokens.space12) {
                                Button { open(c) } label: {
                                    VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                                        Text("#\(c.name)").font(LumiaTokens.textAuthor.font).foregroundStyle(LumiaTokens.inkEmphasis.color)
                                        if !c.topic.isEmpty {
                                            Text(c.topic).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color).lineLimit(2)
                                        }
                                        Text(Membership.memberCount(c.memberCount)).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                                    }
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .contentShape(Rectangle())
                                }
                                .buttonStyle(.plain)
                                .accessibilityHint("Opens it to read")
                                Button("Join") { Task { await join(c) } }
                                    .buttonStyle(.lumiaPrimary).disabled(busy != nil)
                                    .accessibilityLabel("Join #\(c.name)")
                            }
                            .padding(LumiaTokens.space14)
                            .background(LumiaTokens.tile.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
                            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).stroke(LumiaTokens.line.color))
                        }
                    } else if error.isEmpty {
                        ProgressView().frame(maxWidth: .infinity).padding(LumiaTokens.space24)
                    }
                }
                .padding(LumiaTokens.space16)
            }
        }
        .task { await load() }
    }

    private func load() async {
        do { channels = try await session.browseChannels(serverID: serverID) } catch { self.error = error.lumiaMessage }
    }

    private func open(_ c: ChannelSummary) {
        session.preview(c)
        path.append(.channel(id: c.channelId))
    }

    /// In, and into it: the channel opens with its composer, and is in the sidebar.
    private func join(_ c: ChannelSummary) async {
        busy = c.channelId
        error = ""
        defer { busy = nil }
        do {
            try await session.joinChannel(channelID: c.channelId)
            channels?.removeAll { $0.channelId == c.channelId }
            path.append(.channel(id: c.channelId))
        } catch {
            self.error = error.lumiaMessage
        }
    }
}

// MARK: A channel's members

/// Who is in a channel, with Add people at the top, removing someone for
/// whoever may (`MANAGE_CHANNELS`) by the row's swipe, and Leave channel
/// at the end.
struct ChannelMembersScreen: View {
    @Environment(\.openPerson) private var openPerson
    let session: LumiaSession
    let channelID: String
    @Binding var path: [Screen]
    @State private var ids: [String]?
    @State private var busy = false
    @State private var error = ""

    private var channel: ChannelSummary? { session.channel(channelID) }
    private var server: ServerSummary? { session.snapshot?.servers.first { $0.serverId == channel?.serverId } }
    private var name: String { channel?.name ?? "channel" }
    private var member: Bool { session.isIn(channelID) }
    private var canManage: Bool { server?.canManageChannels ?? false }

    var body: some View {
        PanelScreen(back: "#\(name)", title: "Members", subtitle: ids.map { "#\(name) · \(Membership.memberCount(UInt32($0.count)))" } ?? "#\(name)") {
            List {
                if member || canManage {
                    Button { path.append(.addPeople(channelID: channelID)) } label: {
                        Label("Add people", systemImage: "person.badge.plus")
                            .font(LumiaTokens.textNav.font).foregroundStyle(LumiaTokens.link.color)
                            .padding(.vertical, LumiaTokens.space6)
                            .padding(.horizontal, LumiaTokens.space10)
                    }
                    .buttonStyle(.plain)
                    .plainRow()
                }
                if !error.isEmpty { LumiaErrorText(error).plainRow() }
                ForEach(ids ?? [], id: \.self) { id in
                    let line = memberLine(session, id)
                    // Their tag in the server, a role on its tone or a title on the plain chip
                    // (architecture 7.5); this account with none is "You", as on the web.
                    let role = channel.flatMap { session.tag(of: id, in: $0.serverId) }
                    HStack(spacing: 2) {
                        Button { if let s = channel?.serverId { openPerson(PersonInServer(accountID: id, serverID: s)) } } label: {
                            LumiaMemberRow(name: line.name, subtitle: line.handle, tone: .of(id), picture: session.picture(of: id), tag: role == nil && id == session.me ? "You" : nil, role: role, status: channel.flatMap { session.presence(of: id, in: $0.serverId) })
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        // The dark Quiet Split's Remove at the row's end; the swipe below does the same.
                        if canManage, id != session.me {
                            Button { Task { await remove(id) } } label: {
                                Image(systemName: "person.badge.minus")
                            }
                            .buttonStyle(MemberRemoveStyle())
                            .disabled(busy)
                            .accessibilityLabel("Remove \(line.name) from #\(name)")
                            .padding(.trailing, LumiaTokens.space4)
                        }
                    }
                    .plainRow()
                    .swipeActions(edge: .trailing) {
                        if canManage, id != session.me {
                            Button(role: .destructive) { Task { await remove(id) } } label: { Text("Remove from #\(name)") }
                        }
                    }
                }
                if member {
                    Button(role: .destructive) { Task { await leave() } } label: {
                        Text("Leave channel").font(LumiaTokens.textNav.font).foregroundStyle(LumiaTokens.dnd.color)
                            .padding(.vertical, LumiaTokens.space8)
                            .padding(.horizontal, LumiaTokens.space10)
                    }
                    .buttonStyle(.plain)
                    .disabled(busy)
                    .plainRow()
                }
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
        }
        .onAppear { Task { await load() } }
    }

    private func load() async {
        do { ids = try await session.channelMembers(channelID: channelID) } catch { self.error = error.lumiaMessage }
    }

    private func remove(_ id: String) async {
        error = ""
        do {
            try await session.removeChannelMember(channelID: channelID, accountID: id)
            ids?.removeAll { $0 == id }
        } catch {
            self.error = MembershipRefusal.remove(code: error.lumiaCode, message: error.lumiaMessage)
        }
    }

    /// Out, and back to the server: the channel is gone from the sidebar.
    private func leave() async {
        busy = true
        error = ""
        defer { busy = false }
        do {
            try await session.leaveChannel(channelID: channelID)
            path.removeAll()
        } catch {
            self.error = error.lumiaMessage
        }
    }
}

private extension View {
    /// A row of a list drawn on the panel, as the rest of the app's rows are.
    func plainRow() -> some View {
        listRowBackground(Color.clear)
            .listRowSeparator(.hidden)
            .listRowInsets(EdgeInsets(top: 0, leading: LumiaTokens.space16, bottom: 0, trailing: LumiaTokens.space16))
    }
}

// MARK: Add people

/// Members of the server not in the channel, several chosen at once;
/// someone who takes adds as invitations is marked so.
struct AddPeopleScreen: View {
    @Environment(\.dismiss) private var dismiss
    let session: LumiaSession
    let channelID: String
    @State private var query = ""
    @State private var chosen: Set<String> = []
    @State private var inChannel: Set<String>?
    @State private var busy = false
    @State private var error = ""

    private var channel: ChannelSummary? { session.channel(channelID) }
    private var name: String { channel?.name ?? "channel" }
    private var candidates: [MemberSummary] {
        guard let inChannel, let serverID = channel?.serverId else { return [] }
        return Membership.pick(session.serverMembers[serverID] ?? [], me: session.me, excluding: inChannel, query: query) { session.name(of: $0.accountId) }
    }

    var body: some View {
        PanelScreen(back: "Members", title: "Add people", subtitle: "#\(name)") {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space8) {
                    LumiaField("Find a person", text: $query, placeholder: "Name or handle")
                        .textInputAutocapitalization(.never).autocorrectionDisabled()
                    if !error.isEmpty { LumiaErrorText(error) }
                    if inChannel != nil, candidates.isEmpty {
                        LumiaEmptyState(icon: "person.2", title: "No one to add") { Text("Everyone here is in #\(name).") }
                    }
                    ForEach(candidates, id: \.accountId) { m in
                        let line = memberLine(session, m.accountId)
                        LumiaCheckRow(checked: chosen.contains(m.accountId), toggle: { toggle(m.accountId) }) {
                            LumiaAvatar(initials: initials(of: line.name), tone: .of(m.accountId), size: LumiaTokens.avatarSm, picture: session.picture(of: m.accountId))
                            VStack(alignment: .leading, spacing: 0) {
                                HStack(spacing: LumiaTokens.space6) {
                                    Text(line.name).font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.ink.color)
                                    if m.channelAddsByInvitation { LumiaTag("Will be invited") }
                                }
                                if let handle = line.handle { Text(handle).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color) }
                            }
                        }
                    }
                }
                .padding(LumiaTokens.space16)
            }
            .scrollDismissesKeyboard(.interactively)
            Button { Task { await add() } } label: { Text("Add").frame(maxWidth: .infinity) }
                .buttonStyle(.lumiaPrimary).disabled(busy || chosen.isEmpty)
                .padding(LumiaTokens.space16)
        }
        .task {
            do { inChannel = Set(try await session.channelMembers(channelID: channelID)) } catch { self.error = error.lumiaMessage }
        }
    }

    private func toggle(_ id: String) {
        if chosen.remove(id) == nil { chosen.insert(id) }
    }

    private func add() async {
        busy = true
        error = ""
        defer { busy = false }
        do {
            _ = try await session.addChannelMembers(channelID: channelID, accountIDs: Array(chosen))
            dismiss()
        } catch {
            self.error = MembershipRefusal.add(code: error.lumiaCode, message: error.lumiaMessage)
        }
    }
}

// MARK: A server DM

/// Someone in the server to message there (architecture 6.4): whoever
/// turned server DMs off is listed as unavailable and cannot be picked.
struct NewServerDMScreen: View {
    let session: LumiaSession
    let serverID: String
    @Binding var path: [Screen]
    @State private var query = ""
    @State private var busy: String?
    @State private var error = ""

    private var server: ServerSummary? { session.snapshot?.servers.first { $0.serverId == serverID } }
    private var list: [MemberSummary] {
        Membership.pick(session.serverMembers[serverID] ?? [], me: session.me, query: query) { session.name(of: $0.accountId) }
    }

    var body: some View {
        PanelScreen(back: server?.name ?? "Server", title: "Message someone", subtitle: server.map { "In \($0.name)" }) {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space8) {
                    LumiaField("Find a person", text: $query, placeholder: "Name or handle")
                        .textInputAutocapitalization(.never).autocorrectionDisabled()
                    if !error.isEmpty { LumiaErrorText(error) }
                    ForEach(list, id: \.accountId) { m in
                        let line = memberLine(session, m.accountId)
                        Button { Task { await open(m) } } label: {
                            HStack(spacing: LumiaTokens.space10) {
                                LumiaAvatar(initials: initials(of: line.name), tone: .of(m.accountId), size: LumiaTokens.avatarSm, picture: session.picture(of: m.accountId))
                                VStack(alignment: .leading, spacing: 0) {
                                    Text(line.name).font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.ink.color)
                                    if let handle = line.handle { Text(handle).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color) }
                                }
                                Spacer(minLength: 0)
                                if m.serverDmsOff { LumiaTag("Unavailable") }
                            }
                            .padding(.vertical, LumiaTokens.space4)
                            .opacity(m.serverDmsOff ? 0.5 : 1)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .disabled(m.serverDmsOff || busy != nil)
                        .accessibilityHint(m.serverDmsOff ? Membership.takesNoMessages : "")
                    }
                }
                .padding(LumiaTokens.space16)
            }
            .scrollDismissesKeyboard(.interactively)
        }
    }

    private func open(_ m: MemberSummary) async {
        busy = m.accountId
        error = ""
        defer { busy = nil }
        do {
            let id = try await session.openServerDM(serverID: serverID, accountID: m.accountId)
            path.removeLast()
            path.append(.channel(id: id))
        } catch {
            self.error = MembershipRefusal.serverDM(code: error.lumiaCode, name: session.name(of: m.accountId), message: error.lumiaMessage)
        }
    }
}

// MARK: Your settings in a server

/// This account's own two switches in a server (architecture 6.8), kept
/// on its membership at the instance so every device follows them; each
/// is saved as it is turned.
struct YourSettingsScreen: View {
    let session: LumiaSession
    let serverID: String
    @State private var takesAdds = true
    @State private var takesDMs = true
    @State private var loaded = false
    @State private var busy = false
    @State private var error = ""
    @State private var saved = false

    private var server: ServerSummary? { session.snapshot?.servers.first { $0.serverId == serverID } }

    var body: some View {
        PanelScreen(back: server?.name ?? "Server", title: "Your settings in \(server?.name ?? "this server")") {
            ScrollView {
                if loaded {
                    LumiaCard(title: "Channels and messages", width: nil) {
                        VStack(alignment: .leading, spacing: LumiaTokens.space16) {
                            LumiaSwitch("Let others add me to channels", caption: "When off, being added becomes an invitation you can accept or dismiss.", isOn: Binding(get: { takesAdds }, set: { on in
                                takesAdds = on
                                Task { await save(adds: on, dms: nil) }
                            }))
                            LumiaSwitch("Let members of this server message me", caption: "When off, people here can still message you privately.", isOn: Binding(get: { takesDMs }, set: { on in
                                takesDMs = on
                                Task { await save(adds: nil, dms: on) }
                            }))
                            if !error.isEmpty { LumiaErrorText(error) }
                            if saved { Text("Saved.").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color) }
                        }
                        .disabled(busy)
                    }
                    .padding(LumiaTokens.space16)
                }
            }
        }
        .task(id: server?.serverId) {
            guard let server, !loaded else { return }
            takesAdds = !server.channelAddsByInvitation
            takesDMs = !server.serverDmsOff
            loaded = true
        }
    }

    /// One switch as it now is; on a refusal it goes back to what the server has.
    private func save(adds: Bool?, dms: Bool?) async {
        busy = true
        error = ""
        saved = false
        defer { busy = false }
        do {
            try await session.updateMembership(serverID: serverID, channelAddsByInvitation: adds.map { !$0 }, serverDmsOff: dms.map { !$0 })
            saved = true
        } catch {
            self.error = error.lumiaMessage
            if let server {
                takesAdds = !server.channelAddsByInvitation
                takesDMs = !server.serverDmsOff
            }
        }
    }
}

// MARK: Profile card

/// Someone in a server: who they are, and a way to write to them there
/// (a server DM, unless they take none) or privately (a personal DM).
struct ProfileCard: View {
    @Environment(\.setUpPrivateMessages) private var setUp
    let session: LumiaSession
    let person: PersonInServer
    /// Opens the conversation a button led to.
    let open: (Screen) -> Void
    @State private var busy = false
    @State private var note = ""

    private var server: ServerSummary? { session.snapshot?.servers.first { $0.serverId == person.serverID } }
    private var member: MemberSummary? {
        session.serverMembers[person.serverID]?.first { $0.accountId == person.accountID } ?? session.people[person.accountID]
    }
    private var me: Bool { person.accountID == session.me }
    private var name: String { session.name(of: person.accountID) }
    /// Them as this server's list has them: their roles, title and marks are the server's.
    private var listed: MemberSummary? { session.member(person.accountID, in: person.serverID) }
    /// What this account may change of them, from the card as from their row in Members.
    private var offers: MemberOffers? {
        guard let server, let listed else { return nil }
        return rolesMemberOffers(server: server, member: listed, me: me)
    }
    /// Their roles or their title being changed, in the card's place.
    @State private var editing: MemberEditor.Part?

    var body: some View {
        Group {
            if let editing, let server, let listed {
                editor(editing, server, listed)
            } else {
                card
            }
        }
        .padding(LumiaTokens.space24)
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .presentationBackground(LumiaTokens.surface.color)
        #if DEBUG
        // The smoke run's pictures of what each Edit opens (`LUMIA_DEV_EDIT` `roles` or `title`).
        .task {
            switch ProcessInfo.processInfo.environment["LUMIA_DEV_EDIT"] {
            case "roles": editing = .roles
            case "title": editing = .title
            default: break
            }
        }
        #endif
    }

    /// What an Edit opens, in the card's place (the "Profile card" board's
    /// two): the roles to tick, or the title, with Cancel and Save.
    private func editor(_ part: MemberEditor.Part, _ server: ServerSummary, _ listed: MemberSummary) -> some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space16) {
            VStack(alignment: .leading, spacing: LumiaTokens.space4) {
                Text(part == .roles ? "Roles" : "Title").font(LumiaTokens.titleChannel.font).foregroundStyle(LumiaTokens.ink.color)
                    .accessibilityAddTraits(.isHeader)
                Text("\(name) in \(server.name)").font(LumiaTokens.textSubtitle.font).foregroundStyle(LumiaTokens.inkSecondary.color)
            }
            ScrollView {
                MemberEditor(session: session, server: server, member: listed, me: me, parts: [part], kicks: false) { editing = nil }
            }
            .scrollBounceBehavior(.basedOnSize)
            .scrollDismissesKeyboard(.interactively)
        }
    }

    /// Their title under their name, with Edit for whoever may change it; "Add a title" on one's own card with none.
    @ViewBuilder private var titleLine: some View {
        let title = listed?.title ?? ""
        let may = offers?.title ?? false
        if !title.isEmpty {
            HStack(alignment: .firstTextBaseline, spacing: LumiaTokens.space8) {
                Text(title).font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.inkBody.color)
                if may {
                    Button("Edit") { editing = .title }.buttonStyle(CardEditStyle())
                        .accessibilityLabel("Edit title")
                }
            }
        } else if may {
            Button("Add a title") { editing = .title }.buttonStyle(CardEditStyle())
        }
    }

    /// Every role they hold, in the server's order, with Edit for whoever manages roles.
    @ViewBuilder private var rolesPart: some View {
        let held = rolesHeld(roles: server?.roles ?? [], roleIds: listed?.roleIds ?? [])
        let may = offers?.roles ?? false
        if !held.isEmpty || may {
            VStack(alignment: .leading, spacing: LumiaTokens.space6) {
                HStack(alignment: .firstTextBaseline) {
                    Text("Roles").font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.inkMuted.color)
                        .accessibilityAddTraits(.isHeader)
                    Spacer(minLength: LumiaTokens.space8)
                    if may {
                        Button("Edit") { editing = .roles }.buttonStyle(CardEditStyle())
                            .accessibilityLabel("Edit roles")
                    }
                }
                if held.isEmpty {
                    Text(Roles.words.noRoles).font(LumiaTokens.textLabel.font.weight(.regular)).foregroundStyle(LumiaTokens.inkQuiet.color)
                } else {
                    LumiaFlow(spacing: LumiaTokens.space6) {
                        ForEach(held) { r in LumiaTag(r.look) }
                    }
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(held.map(\.label).joined(separator: ", "))
                }
            }
        }
    }

    private var card: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space16) {
            HStack(spacing: LumiaTokens.space14) {
                LumiaAvatar(initials: initials(of: name), tone: .of(person.accountID), size: LumiaTokens.avatarLg, picture: session.picture(of: person.accountID))
                VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                    NameWithTag(gap: LumiaTokens.space8) {
                        Text(name).font(LumiaTokens.titleAuth.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1)
                        // This account, and who they are to the server, its owner
                        // or a guest: both said, "You" never in the other's place.
                        ForEach(rolesCardMarks(me: me, member: listed), id: \.self) { mark in
                            LumiaTag(mark, tone: .accent).accessibilityIdentifier("card-mark")
                        }
                    }
                    titleLine
                    if let handle = member?.handle, !handle.isEmpty {
                        Text("@\(handle)").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    }
                }
            }
            rolesPart
            let custom = session.customStatus(of: person.accountID, in: person.serverID)
            ForEach([("Status", custom), ("Pronouns", member?.pronouns ?? ""), ("About", member?.bio ?? "")].filter { !$0.1.isEmpty }, id: \.0) { label, text in
                VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                    Text(label).font(LumiaTokens.textLabel.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    Text(text).font(LumiaTokens.textBody.font).foregroundStyle(LumiaTokens.inkBody.color)
                }
            }
            if !me {
                VStack(alignment: .leading, spacing: LumiaTokens.space8) {
                    if member?.serverDmsOff == true {
                        Text(Membership.takesNoMessages).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    } else if server?.canStartDms == true {
                        Button { Task { await message() } } label: { Text("Message").frame(maxWidth: .infinity) }
                            .buttonStyle(.lumiaPrimary).disabled(busy)
                    }
                    Button { Task { await messagePrivately() } } label: { Text("Message privately").frame(maxWidth: .infinity) }
                        .buttonStyle(.lumiaSecondary).disabled(busy)
                    if !note.isEmpty {
                        Text(note).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    }
                }
            }
            Spacer(minLength: 0)
        }
    }

    /// A server DM, kept by the server.
    private func message() async {
        busy = true
        note = ""
        defer { busy = false }
        do {
            open(.channel(id: try await session.openServerDM(serverID: person.serverID, accountID: person.accountID)))
        } catch {
            note = MembershipRefusal.serverDM(code: error.lumiaCode, name: name, message: error.lumiaMessage)
        }
    }

    /// A personal DM, end-to-end encrypted: opened, or asked for first.
    private func messagePrivately() async {
        // Closes the card, then what this phone needs (the shell's).
        if session.privateMessages.needsSomething { setUp(); return }
        busy = true
        note = ""
        defer { busy = false }
        do {
            if let channelID = try await session.dmOpenOrRequest(accountID: person.accountID) {
                open(.conversation(id: channelID))
            } else {
                note = "Request sent to \(name). The conversation opens when they accept."
            }
        } catch {
            note = error.lumiaCode == "ALREADY_EXISTS" ? "You already asked; waiting for their answer." : error.lumiaMessage
        }
    }
}

/// "Edit" beside a card's title and its Roles, and "Add a title": the
/// boards' text button, the words in the body's ink over a rule.
struct CardEditStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(LumiaTokens.textLinkSm.font)
            .underline(color: LumiaTokens.underline.color)
            .foregroundStyle(configuration.isPressed ? LumiaTokens.ink.color : LumiaTokens.inkBody.color)
            // A finger's room round a word that small.
            .padding(.vertical, LumiaTokens.space6)
            .contentShape(Rectangle())
            .padding(.vertical, -LumiaTokens.space6)
    }
}

/// A member row's Remove, as the dark Quiet Split draws it: `row-height` square (32) at radius 8,
/// the glyph in the faint ink, and while pressed (a phone has no pointer to hover) the danger ink
/// on its soft fill and border. The target reaches 44 (`tile-size`), the least a finger needs.
struct MemberRemoveStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: LumiaTokens.space16, weight: .medium))
            .foregroundStyle(configuration.isPressed ? LumiaTokens.danger.color : LumiaTokens.inkFaint.color)
            .frame(width: LumiaTokens.rowHeight, height: LumiaTokens.rowHeight)
            .background(
                RoundedRectangle(cornerRadius: LumiaTokens.radius8)
                    .fill(configuration.isPressed ? LumiaTokens.dangerSoft.color : .clear)
            )
            .overlay(
                RoundedRectangle(cornerRadius: LumiaTokens.radius8)
                    .strokeBorder(configuration.isPressed ? LumiaTokens.dangerLine.color : .clear, lineWidth: 1)
            )
            .frame(width: LumiaTokens.tileSize, height: LumiaTokens.tileSize)
            .contentShape(Rectangle())
    }
}
