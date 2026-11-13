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
                    Button { if let s = channel?.serverId { openPerson(PersonInServer(accountID: id, serverID: s)) } } label: {
                        LumiaMemberRow(name: line.name, subtitle: line.handle, tone: .of(id), tag: id == session.me ? "You" : nil, status: channel.flatMap { session.presence(of: id, in: $0.serverId) })
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
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
                            LumiaAvatar(initials: initials(of: line.name), tone: .of(m.accountId), size: LumiaTokens.avatarSm)
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
                                LumiaAvatar(initials: initials(of: line.name), tone: .of(m.accountId), size: LumiaTokens.avatarSm)
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

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space16) {
            HStack(spacing: LumiaTokens.space14) {
                LumiaAvatar(initials: initials(of: name), tone: .of(person.accountID), size: LumiaTokens.avatarLg)
                VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                    HStack(spacing: LumiaTokens.space6) {
                        Text(name).font(LumiaTokens.titleAuth.font).foregroundStyle(LumiaTokens.ink.color)
                        if me { LumiaTag("You") }
                    }
                    if let handle = member?.handle, !handle.isEmpty {
                        Text("@\(handle)").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    }
                }
            }
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
        .padding(LumiaTokens.space24)
        .presentationDetents([.medium])
        .presentationDragIndicator(.visible)
        .presentationBackground(LumiaTokens.surface.color)
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
