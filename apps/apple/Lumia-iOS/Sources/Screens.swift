// The screens pushed over the rail and sidebar, each a floating panel:
// requests, a new message, devices and the rest (a conversation and a
// channel are in Conversations.swift, and the bar a channel this account
// is not in has in place of its composer is here). The same
// states, wording and error handling as the web's and the desktop shell's.
import LumiaFFI
import LumiaKit
import PhotosUI
import SwiftUI

/// In place of the composer of a channel this account is not in.
struct JoinBar: View {
    let line: String
    let busy: Bool
    let join: () -> Void

    var body: some View {
        HStack(spacing: LumiaTokens.space12) {
            Text(line).font(LumiaTokens.textBody.font).foregroundStyle(LumiaTokens.inkBody.color)
            Spacer(minLength: 0)
            Button("Join", action: join).buttonStyle(.lumiaPrimary).disabled(busy)
        }
        .padding(.horizontal, LumiaTokens.space16)
        .padding(.vertical, LumiaTokens.space12)
        .background(LumiaTokens.inset.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius16))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius16).stroke(LumiaTokens.line.color))
        .padding(.horizontal, LumiaTokens.space24)
        .padding(.top, LumiaTokens.space4)
        .padding(.bottom, LumiaTokens.space14)
    }
}

// MARK: Requests

struct RequestsScreen: View {
    @Environment(\.setUpPrivateMessages) private var setUp
    let session: LumiaSession
    @Binding var path: [Screen]
    @State private var busy: String?
    @State private var error = ""

    private var requests: [DmRequestSummary] { session.snapshot?.pendingRequests ?? [] }

    var body: some View {
        PanelScreen(back: "Home", title: "Requests", subtitle: requests.isEmpty ? "No one waiting" : "\(requests.count) waiting for you") {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                    LumiaNotice(icon: "shield") {
                        Text("A request shows who is asking and what you share, nothing else. Accepting opens an encrypted conversation; the sender cannot see that you looked.")
                    }
                    if !error.isEmpty { LumiaErrorText(error) }
                    if requests.isEmpty {
                        LumiaEmptyState(icon: "at", title: "Nothing waiting") { Text("Declined requests disappear; blocked people cannot ask again.") }
                    } else {
                        SectionTitle("Waiting for you").padding(.horizontal, -LumiaTokens.space10)
                        ForEach(requests, id: \.requestId) { r in
                            RequestRow(
                                name: session.name(of: r.fromAccountId),
                                handle: (session.people[r.fromAccountId] ?? session.profile(of: r.fromAccountId))?.handle,
                                tone: .of(r.fromAccountId),
                                picture: session.picture(of: r.fromAccountId),
                                meta: meta(r),
                                busy: busy == r.requestId,
                                accept: { Task { await respond(r.requestId, "accept") } },
                                decline: { Task { await respond(r.requestId, "decline") } },
                                block: { Task { await respond(r.requestId, "block") } }
                            )
                        }
                    }
                }
                .padding(LumiaTokens.space16)
            }
        }
    }

    private func meta(_ r: DmRequestSummary) -> String {
        var parts: [String] = []
        if !r.sharedServerIds.isEmpty { parts.append("Shares \(r.sharedServerIds.count == 1 ? "a server" : "\(r.sharedServerIds.count) servers") with you") }
        if r.mutualFriendCount > 0 { parts.append("\(r.mutualFriendCount) mutual \(r.mutualFriendCount == 1 ? "friend" : "friends")") }
        return parts.isEmpty ? "Nothing in common yet" : parts.joined(separator: " · ")
    }

    private func respond(_ id: String, _ response: String) async {
        // Accepting opens an encrypted conversation, which needs this phone set up.
        if response == "accept", session.privateMessages.needsSomething { setUp(); return }
        busy = id
        error = ""
        defer { busy = nil }
        do {
            if let channelID = try await session.dmRespond(requestID: id, response: response), response == "accept" {
                path = [.conversation(id: channelID)]
            }
        } catch {
            self.error = error.lumiaMessage
        }
    }
}

/// A request on a phone: who and what you share, the answers on their own line.
struct RequestRow: View {
    let name: String
    let handle: String?
    let tone: AvatarTone
    let picture: URL?
    let meta: String
    let busy: Bool
    let accept: () -> Void
    let decline: () -> Void
    let block: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space10) {
            HStack(spacing: LumiaTokens.space12) {
                LumiaAvatar(initials: initials(of: name), tone: tone, size: LumiaTokens.avatarLg, picture: picture)
                VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                    HStack(alignment: .firstTextBaseline, spacing: LumiaTokens.space6) {
                        Text(name).font(LumiaTokens.textAuthor.font).foregroundStyle(LumiaTokens.inkEmphasis.color)
                        if let handle, !handle.isEmpty { Text("@\(handle)").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color) }
                    }
                    Text(meta).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                }
            }
            HStack(spacing: LumiaTokens.space8) {
                Button("Accept", action: accept).buttonStyle(.lumiaPrimary)
                Button("Decline", action: decline).buttonStyle(.lumiaSecondary)
                Spacer()
                Button("Block", action: block).buttonStyle(.lumiaLink)
            }
        }
        .disabled(busy)
        .padding(LumiaTokens.space14)
        .background(LumiaTokens.tile.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).stroke(LumiaTokens.line.color))
    }
}

// MARK: New message

// MARK: Add a server

/// A server of your own, or joining one from an invite link or code:
/// what the link leads to first, on which instance, then the join
/// (architecture 6.1).
struct AddServerScreen: View {
    let session: LumiaSession
    let opened: (String) -> Void
    @State private var name = ""
    @State private var creating = false
    @State private var createError = ""
    @State private var text = ""
    @State private var preview: InvitePreviewSummary?
    @State private var error = ""
    @State private var busy = false

    var body: some View {
        PanelScreen(back: "Home", title: "Add a server", subtitle: "Your own, or one you were invited to") {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space16) {
                    LumiaCard(title: "Create a server", subtitle: "You own it; invite people once it exists.", width: nil) {
                        LumiaField("Name", text: $name, placeholder: "Our place")
                        if !createError.isEmpty { LumiaErrorText(createError) }
                        Button("Create") { Task { await create() } }
                            .buttonStyle(.lumiaPrimary).disabled(creating || name.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                    LumiaCard(title: "Join with an invite", subtitle: "Paste the link someone sent you; what it leads to shows first.", width: nil) {
                        LumiaField("Invite link or code", text: $text, placeholder: "https://chat.example.com/invite/…")
                            .textInputAutocapitalization(.never).autocorrectionDisabled()
                        if !error.isEmpty { LumiaErrorText(error) }
                        Button("Look it up") { Task { await look() } }
                            .buttonStyle(.lumiaPrimary).disabled(busy || text.trimmingCharacters(in: .whitespaces).isEmpty)
                        if let p = preview {
                            Text("\(p.serverName) · \(p.memberCount) \(p.memberCount == 1 ? "member" : "members") on \(p.host), run by \(p.operatorName.isEmpty ? "an unnamed operator" : p.operatorName)\(p.asGuest ? " · you would join as a guest" : ""). Your account on this instance joins it; nothing else is shared.")
                                .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                            Button("Join \(p.serverName)") { Task { await join(p) } }
                                .buttonStyle(.lumiaPrimary).disabled(busy)
                        }
                    }
                }
                .padding(LumiaTokens.space16)
            }
        }
    }

    private func create() async {
        creating = true
        createError = ""
        defer { creating = false }
        do {
            opened(try await session.createServer(name: name))
        } catch {
            createError = error.lumiaMessage
        }
    }

    private func look() async {
        busy = true
        error = ""
        preview = nil
        defer { busy = false }
        do {
            preview = try await session.invitePreview(text)
        } catch {
            self.error = error.lumiaCode == "NOT_FOUND" ? "That invite is unknown, expired or used up." : error.lumiaMessage
        }
    }

    private func join(_ p: InvitePreviewSummary) async {
        busy = true
        error = ""
        defer { busy = false }
        do {
            opened(try await session.acceptInvite(p.code))
        } catch {
            self.error = error.lumiaMessage
        }
    }
}

// MARK: New channel

/// A text channel in the server, public or private (architecture 6.8);
/// it opens once made, with this account in it.
struct NewChannelScreen: View {
    let session: LumiaSession
    let server: ServerSummary?
    let opened: (String) -> Void
    @State private var name = ""
    @State private var isPrivate = false
    @State private var error = ""
    @State private var busy = false

    var body: some View {
        PanelScreen(back: server?.name ?? "Server", title: "Create a channel", subtitle: "A text channel in \(server?.name ?? "the server")") {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                    LumiaField("Name", text: $name, placeholder: "general")
                        .textInputAutocapitalization(.never).autocorrectionDisabled()
                    LumiaSwitch("Private", caption: "Only people added can see it.", isOn: $isPrivate)
                    if !error.isEmpty { LumiaErrorText(error) }
                    Button("Create") { Task { await create() } }
                        .buttonStyle(.lumiaPrimary).disabled(busy || name.trimmingCharacters(in: .whitespaces).isEmpty)
                }
                .padding(LumiaTokens.space16)
            }
        }
    }

    private func create() async {
        guard let server else { return }
        busy = true
        error = ""
        defer { busy = false }
        do {
            opened(try await session.createChannel(serverID: server.serverId, name: name, isPrivate: isPrivate))
        } catch {
            self.error = MembershipRefusal.createChannel(code: error.lumiaCode, message: error.lumiaMessage)
        }
    }
}

// MARK: Invite people

/// Invite links for the server: a new one with an expiry and a use limit,
/// ready to copy or share, and the ones that exist, each revocable.
struct InviteScreen: View {
    let session: LumiaSession
    let server: ServerSummary?
    @State private var expiresInHours: UInt32 = 0
    @State private var maxUses: UInt32 = 0
    @State private var invite: InviteSummary?
    @State private var invites: [InviteSummary] = []
    @State private var error = ""
    @State private var busy = false
    @State private var copied = false
    @State private var email = ""
    @State private var emailed = ""

    var body: some View {
        PanelScreen(back: server?.name ?? "Server", title: "Invite people", subtitle: "A link to join \(server?.name ?? "the server")") {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                    Text("Expires").font(LumiaTokens.textLabel.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    Picker("Expires", selection: $expiresInHours) {
                        Text("Never").tag(UInt32(0))
                        Text("In a day").tag(UInt32(24))
                        Text("In a week").tag(UInt32(168))
                    }
                    .pickerStyle(.segmented)
                    Text("Uses").font(LumiaTokens.textLabel.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    Picker("Uses", selection: $maxUses) {
                        Text("Unlimited").tag(UInt32(0))
                        Text("Once").tag(UInt32(1))
                        Text("Ten").tag(UInt32(10))
                    }
                    .pickerStyle(.segmented)
                    if !error.isEmpty { LumiaErrorText(error) }
                    Button("Make a link") { Task { await make() } }
                        .buttonStyle(.lumiaPrimary).disabled(busy)
                    // The invite as a mail (architecture 6.2): one use, a week, to the address.
                    LumiaField("Email", text: $email, placeholder: "pat@example.com")
                        .textContentType(.emailAddress).keyboardType(.emailAddress).textInputAutocapitalization(.never).autocorrectionDisabled()
                    Button("Send by email") { Task { await sendByEmail() } }
                        .buttonStyle(.lumiaSecondary).disabled(busy || email.trimmingCharacters(in: .whitespaces).isEmpty)
                    if !emailed.isEmpty {
                        Text("Sent to \(emailed); it works once, within a week.").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    }
                    if let invite {
                        Text(invite.url)
                            .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.ink.color)
                            .textSelection(.enabled)
                            .padding(LumiaTokens.space12)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(LumiaTokens.surface.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius10))
                        HStack(spacing: LumiaTokens.space8) {
                            Button(copied ? "Copied" : "Copy link") {
                                UIPasteboard.general.string = invite.url
                                copied = true
                            }
                            .buttonStyle(.lumiaPrimary)
                            if let url = URL(string: invite.url) {
                                ShareLink(item: url) { Text("Share") }.buttonStyle(.lumiaSecondary)
                            }
                        }
                    }
                    if !invites.isEmpty {
                        SectionTitle("Invites")
                        ForEach(invites, id: \.code) { i in
                            HStack(spacing: LumiaTokens.space10) {
                                VStack(alignment: .leading, spacing: 0) {
                                    Text(i.code).font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.ink.color)
                                    Text(describe(i)).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                                }
                                Spacer()
                                Button("Revoke") { Task { await revoke(i) } }
                                    .buttonStyle(.lumiaLink).disabled(busy)
                            }
                            .padding(.vertical, LumiaTokens.space4)
                        }
                    }
                }
                .padding(LumiaTokens.space16)
            }
        }
        .task { await load() }
    }

    private func describe(_ i: InviteSummary) -> String {
        let uses = (i.email.isEmpty ? "" : "to \(i.email) · ") + (i.maxUses == 0 ? "\(i.uses) uses" : "\(i.uses)/\(i.maxUses) uses")
        guard let micros = i.expiresAtMicros else { return "\(uses) · no expiry" }
        let at = Date(timeIntervalSince1970: TimeInterval(micros) / 1_000_000)
        return "\(uses) · expires \(at.formatted(date: .abbreviated, time: .shortened))"
    }

    private func load() async {
        guard let server else { return }
        do {
            invites = try await session.invites(serverID: server.serverId)
        } catch {
            self.error = error.lumiaCode == "PERMISSION_DENIED" ? "You cannot invite people to this server." : error.lumiaMessage
        }
    }

    private func make() async {
        guard let server else { return }
        busy = true
        error = ""
        copied = false
        defer { busy = false }
        do {
            let made = try await session.createInvite(serverID: server.serverId, maxUses: maxUses, expiresInHours: expiresInHours == 0 ? nil : expiresInHours)
            invite = made
            invites.insert(made, at: 0)
        } catch {
            self.error = error.lumiaCode == "PERMISSION_DENIED" ? "You cannot invite people to this server." : error.lumiaCode == "RATE_LIMITED" ? "Too many links in the last hour; try later." : error.lumiaMessage
        }
    }

    private func sendByEmail() async {
        guard let server else { return }
        busy = true
        error = ""
        emailed = ""
        defer { busy = false }
        do {
            let made = try await session.emailInvite(serverID: server.serverId, email: email)
            invites.insert(made, at: 0)
            emailed = made.email
            email = ""
        } catch {
            self.error = error.lumiaCode == "FAILED_PRECONDITION" ? "This instance sends no mail; make a link instead." : error.lumiaCode == "INVALID_ARGUMENT" ? "That is not an email address." : error.lumiaCode == "RATE_LIMITED" ? "Too many invites in the last hour; try later." : error.lumiaMessage
        }
    }

    private func revoke(_ i: InviteSummary) async {
        busy = true
        error = ""
        defer { busy = false }
        do {
            try await session.revokeInvite(i.code)
            invites.removeAll { $0.code == i.code }
            if invite?.code == i.code { invite = nil }
        } catch {
            self.error = error.lumiaMessage
        }
    }
}

// MARK: Server settings

/// Overview, the first of a server's settings' sections: its name, picture,
/// what the tag beside a name shows, how long it keeps messages and its files
/// (architecture 6.7 and 7.5, specs/media.md 4.5), for whoever may change them
/// (`canManage`). The words, states and errors
/// are the web's and the desktop shell's; the Profile screen's cards, and
/// the photo's picker and refusals.
struct ServerOverviewScreen: View {
    let session: LumiaSession
    let serverID: String
    // What the tag beside a name shows: saved as it is chosen, the choice drawn from the server.
    @State private var tagBusy = false
    @State private var tagError = ""
    @State private var name = ""
    @State private var nameBusy = false
    @State private var nameError = ""
    @State private var nameSaved = false
    @State private var picked: PhotosPickerItem?
    @State private var pictureBusy = false
    @State private var pictureError = ""
    @State private var pictureSaved = false
    @State private var retention: UInt32 = 0
    @State private var retentionBusy = false
    @State private var retentionError = ""
    @State private var retentionSaved = false
    // Show deleted messages: saved as it is turned, put back on a refusal.
    @State private var showDeleted = false
    @State private var showDeletedBusy = false
    @State private var showDeletedError = ""
    @State private var showDeletedSaved = false
    // A server's files (specs/media.md 4.5): what they take, asked as the
    // screen opens, and how long they are kept and what a full server
    // does, both saved by one Save.
    @State private var storage: StorageSummary?
    @State private var storageError = ""
    @State private var fileRetention: UInt32 = 0
    @State private var deleteOldest = false
    @State private var filesBusy = false
    @State private var filesError = ""
    @State private var filesSaved = false
    // The form waits for the server: fields shown before it would be overwritten by it.
    @State private var loaded = false
    // Default channels (architecture 6.8): the server's public channels, those
    // this account is in and those Browse lists, and which of them are default.
    @State private var publicChannels: [ChannelSummary]?
    @State private var defaults = DefaultChannels([])
    @State private var defaultsBusy = false
    @State private var defaultsError = ""
    @State private var defaultsSaved = false

    private var server: ServerSummary? { session.snapshot?.servers.first { $0.serverId == serverID } }

    var body: some View {
        PanelScreen(back: settingsBack(server), title: rolesSectionLabel(section: .overview), subtitle: server?.name) {
            ScrollViewReader { proxy in
                ScrollView {
                    if let server, loaded {
                        VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                            overview(server)
                            tagBesideNames(server).id("tag")
                            history(server)
                            files(server)
                            defaultChannels(server).id("defaults")
                        }
                        .padding(LumiaTokens.space16)
                    }
                }
                .scrollDismissesKeyboard(.interactively)
                #if DEBUG
                // The smoke run's picture of the last card (`LUMIA_DEV_SCROLL=defaults`), or of "Tag beside names" (`tag`).
                .onChange(of: publicChannels?.count) { _, _ in
                    if ProcessInfo.processInfo.environment["LUMIA_DEV_SCROLL"] == "defaults" { proxy.scrollTo("defaults", anchor: .bottom) }
                    if ProcessInfo.processInfo.environment["LUMIA_DEV_SCROLL"] == "tag" { proxy.scrollTo("tag", anchor: .top) }
                }
                #endif
            }
        }
        .task(id: server?.serverId) {
            guard let server, !loaded else { return }
            name = server.name
            retention = server.retentionDays
            showDeleted = server.showDeleted
            fileRetention = server.attachmentRetentionDays
            deleteOldest = server.deleteOldestWhenFull
            loaded = true
            async let asked: () = loadStorage(server)
            await loadDefaults(server)
            await asked
        }
        .onChange(of: picked) { _, item in
            guard let item else { return }
            picked = nil
            Task { await choose(item) }
        }
    }

    private func overview(_ server: ServerSummary) -> some View {
        let url = session.pictureURL(server.iconUrl)
        return LumiaCard(title: "Overview", width: nil) {
            VStack(alignment: .leading, spacing: LumiaTokens.space16) {
                HStack(spacing: LumiaTokens.space14) {
                    // As the rail draws the server: its picture over its initials.
                    LumiaPictureOver(url: url) {
                        Text(initials(of: server.name)).font(LumiaTokens.titleMonogram.font).foregroundStyle(LumiaTokens.ink.color)
                    }
                    .frame(width: LumiaTokens.tileSize, height: LumiaTokens.tileSize)
                    .background(LumiaTokens.tile.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
                    .clipShape(RoundedRectangle(cornerRadius: LumiaTokens.radius14))
                    // The card is the tile's colour in light: its edge says where it is.
                    .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).stroke(LumiaTokens.line.color))
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(url == nil ? "No server picture" : "Server picture")
                    .accessibilityAddTraits(.isImage)
                    VStack(alignment: .leading, spacing: LumiaTokens.space6) {
                        // Side by side where they fit on one line each, else one over the other.
                        ViewThatFits(in: .horizontal) {
                            HStack(spacing: LumiaTokens.space8) { pictureButtons(has: url != nil) }
                            VStack(alignment: .leading, spacing: LumiaTokens.space8) { pictureButtons(has: url != nil) }
                        }
                        if !pictureError.isEmpty {
                            LumiaErrorText(pictureError)
                        } else {
                            Text(pictureSaved ? "Saved." : "A PNG, JPEG, WebP or GIF up to 8 MB. People see its middle.")
                                .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                        }
                    }
                }
                LumiaField("Name", text: $name, placeholder: "Our place", spoken: "Name")
                if !nameError.isEmpty { LumiaErrorText(nameError) }
                if nameSaved { Text("Saved.").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color) }
                Button { Task { await saveName(server) } } label: { Text("Save").frame(maxWidth: .infinity) }
                    .buttonStyle(.lumiaPrimary).disabled(nameBusy || !ServerName.canSave(name, current: server.name))
                    .accessibilityHint("Saves the server's name")
                    .accessibilityIdentifier("save-name")
            }
        }
        .onChange(of: name) { _, _ in nameSaved = false; nameError = "" }
    }

    @ViewBuilder private func pictureButtons(has: Bool) -> some View {
        PhotosPicker(selection: $picked, matching: .images, preferredItemEncoding: .compatible) {
            Text(pictureBusy ? "Sending…" : "Choose a picture…").lineLimit(1).fixedSize()
        }
        .buttonStyle(.lumiaSecondary).disabled(pictureBusy)
        if has {
            Button { Task { await removePicture() } } label: { Text("Remove").lineLimit(1).fixedSize() }
                .buttonStyle(.lumiaSecondary).disabled(pictureBusy)
                .accessibilityHint("Takes the server's picture away")
        }
    }

    /// "Tag beside names" (architecture 7.5): the member's role or their
    /// title, as two pictured choices, saved as one is chosen. A refusal is
    /// said in the note's place and the choice stays as the server has it.
    private func tagBesideNames(_ server: ServerSummary) -> some View {
        LumiaCard(title: Roles.words.tagHeading, subtitle: Roles.words.tagSubtitle, width: nil) {
            VStack(alignment: .leading, spacing: LumiaTokens.space16) {
                ChoiceTiles(label: Roles.words.tagHeading) {
                    ChoiceTile(name: Roles.words.tagRolesLabel, caption: Roles.words.tagRolesCaption, chosen: !server.tagTitles, choose: { Task { await chooseTag(titles: false, server) } }, height: TagSketch.height) {
                        TagSketch(titles: false)
                    }
                    ChoiceTile(name: Roles.words.tagTitlesLabel, caption: Roles.words.tagTitlesCaption, chosen: server.tagTitles, choose: { Task { await chooseTag(titles: true, server) } }, height: TagSketch.height) {
                        TagSketch(titles: true)
                    }
                }
                .disabled(tagBusy)
                if !tagError.isEmpty {
                    LumiaErrorText(tagError)
                } else {
                    Text(Roles.words.tagNote).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }

    private func chooseTag(titles: Bool, _ server: ServerSummary) async {
        guard !tagBusy, titles != server.tagTitles else { return }
        tagBusy = true
        tagError = ""
        defer { tagBusy = false }
        do {
            try await session.setTagTitles(serverID: serverID, titles: titles)
        } catch {
            tagError = ServerSettingsRefusal.settings(code: error.lumiaCode, details: error.lumiaDetails, message: error.lumiaMessage)
        }
    }

    private func history(_ server: ServerSummary) -> some View {
        LumiaCard(title: "Message history", subtitle: "How long this server keeps its messages.", width: nil) {
            VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                LumiaSegmentedTabs("Message history", tabs: Retention.choices(current: server.retentionDays).map { .init(id: $0, label: Retention.label($0)) }, selected: $retention)
                // Nothing is deleted until Save; this says what Save will do.
                if let warning = Retention.warning(chosen: retention, current: server.retentionDays) { LumiaErrorText(warning) }
                if !retentionError.isEmpty { LumiaErrorText(retentionError) }
                if retentionSaved { Text("Saved.").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color) }
                Button { Task { await saveRetention(server) } } label: { Text("Save").frame(maxWidth: .infinity) }
                    .buttonStyle(.lumiaPrimary).disabled(retentionBusy || retention == server.retentionDays)
                    .accessibilityHint("Saves how long this server keeps its messages")
                    .accessibilityIdentifier("save-retention")
                LumiaSwitch("Show deleted messages", caption: "Off, a deleted message disappears for everyone. On, it stays in its place as “(deleted)”.", isOn: Binding(get: { showDeleted }, set: { on in
                    showDeleted = on
                    Task { await saveShowDeleted(on) }
                }))
                .disabled(showDeletedBusy)
                .padding(.top, LumiaTokens.space4)
                if !showDeletedError.isEmpty { LumiaErrorText(showDeletedError) }
                if showDeletedSaved {
                    Text("Saved.").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                        .accessibilityIdentifier("show-deleted-saved")
                }
            }
        }
        .onChange(of: retention) { _, _ in retentionSaved = false; retentionError = "" }
    }

    private func files(_ server: ServerSummary) -> some View {
        LumiaCard(title: "Files", subtitle: "What this server’s attachments take, and how long it keeps them.", width: nil) {
            VStack(alignment: .leading, spacing: LumiaTokens.space16) {
                if let storage {
                    Text(ServerFiles.usage(storage)).font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.ink.color)
                        .accessibilityIdentifier("storage-usage")
                } else if !storageError.isEmpty {
                    LumiaErrorText(storageError)
                } else {
                    ProgressView()
                }
                PreferenceRow("Keep attachments for") {
                    LumiaSegmentedTabs("Keep attachments for", tabs: Retention.choices(current: server.attachmentRetentionDays).map { .init(id: $0, label: Retention.label($0)) }, selected: $fileRetention)
                    // Nothing is deleted until Save; this says what Save will do.
                    if let warning = ServerFiles.retentionWarning(chosen: fileRetention, current: server.attachmentRetentionDays) { LumiaErrorText(warning) }
                }
                PreferenceRow("When the server is full") {
                    // Two choices too long for a segment each on a phone: one under the other.
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(ServerFiles.fullChoices, id: \.self) { choice in
                            ChoiceRow(ServerFiles.fullLabel(choice), chosen: deleteOldest == choice) { deleteOldest = choice }
                        }
                    }
                    .accessibilityElement(children: .contain)
                    .accessibilityLabel("When the server is full")
                    // What the chosen one does; a warning while it would start deleting.
                    if ServerFiles.fullWarns(chosen: deleteOldest, current: server.deleteOldestWhenFull) {
                        LumiaErrorText(ServerFiles.fullDescription(deleteOldest))
                    } else {
                        Text(ServerFiles.fullDescription(deleteOldest)).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                    if !filesError.isEmpty { LumiaErrorText(filesError) }
                    if filesSaved { Text("Saved.").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color) }
                    Button { Task { await saveFiles(server) } } label: { Text("Save").frame(maxWidth: .infinity) }
                        .buttonStyle(.lumiaPrimary).disabled(filesBusy || !ServerFiles.canSave(retention: fileRetention, deleteOldest: deleteOldest, server: server))
                        .accessibilityHint("Saves how long this server keeps its attachments and what it does when full")
                        .accessibilityIdentifier("save-files")
                }
            }
        }
        .onChange(of: fileRetention) { _, _ in filesSaved = false; filesError = "" }
        .onChange(of: deleteOldest) { _, _ in filesSaved = false; filesError = "" }
    }

    private func defaultChannels(_ server: ServerSummary) -> some View {
        LumiaCard(title: "Default channels", subtitle: "New members join these channels.", width: nil) {
            VStack(alignment: .leading, spacing: LumiaTokens.space4) {
                if let publicChannels {
                    if publicChannels.isEmpty {
                        Text("This server has no public channels.").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    }
                    ForEach(publicChannels, id: \.channelId) { c in
                        LumiaCheckRow(checked: defaults.isChosen(c.channelId), toggle: { defaults.toggle(c.channelId); defaultsSaved = false; defaultsError = "" }) {
                            Text("#\(c.name)").font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.ink.color)
                        }
                    }
                } else if !defaultsError.isEmpty {
                    EmptyView()
                } else {
                    ProgressView()
                }
                if !defaultsError.isEmpty { LumiaErrorText(defaultsError) }
                if defaultsSaved { Text("Saved.").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color) }
                Button { Task { await saveDefaults(server) } } label: { Text("Save").frame(maxWidth: .infinity) }
                    .buttonStyle(.lumiaPrimary).disabled(defaultsBusy || !defaults.changed)
                    .accessibilityHint("Saves which channels new members join")
                    .accessibilityIdentifier("save-defaults")
                    .padding(.top, LumiaTokens.space8)
            }
        }
    }

    /// What the server's files take; asked only here, where the settings
    /// are offered, since the instance answers whoever may manage it alone.
    private func loadStorage(_ server: ServerSummary) async {
        do {
            storage = try await session.serverStorage(serverID: server.serverId)
        } catch {
            storageError = ServerSettingsRefusal.settings(code: error.lumiaCode, details: error.lumiaDetails, message: error.lumiaMessage)
        }
    }

    /// Only what changed is sent; nil leaves the other as the server has it.
    private func saveFiles(_ server: ServerSummary) async {
        filesError = ""
        filesSaved = false
        filesBusy = true
        defer { filesBusy = false }
        do {
            try await session.setServerFiles(
                serverID: server.serverId,
                attachmentRetentionDays: fileRetention == server.attachmentRetentionDays ? nil : fileRetention,
                deleteOldestWhenFull: deleteOldest == server.deleteOldestWhenFull ? nil : deleteOldest
            )
            filesSaved = true
        } catch {
            filesError = ServerSettingsRefusal.settings(code: error.lumiaCode, details: error.lumiaDetails, message: error.lumiaMessage)
        }
    }

    private func loadDefaults(_ server: ServerSummary) async {
        do {
            let browsed = try await session.browseChannels(serverID: server.serverId)
            let list = Membership.publicChannels(joined: session.snapshot?.channels ?? [], browsed: browsed, serverID: server.serverId)
            publicChannels = list
            defaults = DefaultChannels(list)
        } catch {
            defaultsError = ServerSettingsRefusal.settings(code: error.lumiaCode, details: error.lumiaDetails, message: error.lumiaMessage)
        }
    }

    private func saveDefaults(_ server: ServerSummary) async {
        defaultsError = ""
        defaultsSaved = false
        defaultsBusy = true
        defer { defaultsBusy = false }
        do {
            try await session.setDefaultChannels(serverID: server.serverId, channelIDs: defaults.ids(in: publicChannels ?? []))
            await loadDefaults(server)
            defaultsSaved = true
        } catch {
            defaultsError = ServerSettingsRefusal.settings(code: error.lumiaCode, details: error.lumiaDetails, message: error.lumiaMessage)
        }
    }

    private func saveName(_ server: ServerSummary) async {
        nameError = ""
        nameSaved = false
        // What the instance would refuse is not sent.
        guard ServerName.isValid(name) else { nameError = ServerName.refusal; return }
        // The field shows what is sent; an edit after this clears the line under it.
        name = ServerName.trimmed(name)
        nameBusy = true
        defer { nameBusy = false }
        do {
            try await session.updateServer(serverID: server.serverId, name: name, retentionDays: nil)
            nameSaved = true
        } catch {
            nameError = ServerSettingsRefusal.settings(code: error.lumiaCode, details: error.lumiaDetails, message: error.lumiaMessage)
        }
    }

    private func saveRetention(_ server: ServerSummary) async {
        retentionError = ""
        retentionSaved = false
        retentionBusy = true
        defer { retentionBusy = false }
        do {
            try await session.updateServer(serverID: server.serverId, name: nil, retentionDays: retention)
            retentionSaved = true
        } catch {
            retentionError = ServerSettingsRefusal.settings(code: error.lumiaCode, details: error.lumiaDetails, message: error.lumiaMessage)
        }
    }

    /// The switch as it now is; on a refusal it goes back to what the server has.
    private func saveShowDeleted(_ on: Bool) async {
        showDeletedError = ""
        showDeletedSaved = false
        showDeletedBusy = true
        defer { showDeletedBusy = false }
        do {
            try await session.setShowDeleted(serverID: serverID, show: on)
            showDeletedSaved = true
        } catch {
            showDeletedError = ServerSettingsRefusal.settings(code: error.lumiaCode, details: error.lumiaDetails, message: error.lumiaMessage)
            if let server { showDeleted = server.showDeleted }
        }
    }

    /// The picture chosen, as the photo library gives it in a form the
    /// instance reads (`.compatible` turns a HEIC into a JPEG).
    private func choose(_ item: PhotosPickerItem) async {
        pictureError = ""
        pictureSaved = false
        pictureBusy = true
        defer { pictureBusy = false }
        do {
            guard let data = try await item.loadTransferable(type: Data.self) else {
                pictureError = "That picture could not be read."
                return
            }
            // What the instance would refuse for its size is not sent at all.
            guard data.count <= ServerSettingsRefusal.largestPicture else {
                pictureError = ServerSettingsRefusal.tooLarge
                return
            }
            let mediaType = item.supportedContentTypes.first?.preferredMIMEType ?? "application/octet-stream"
            try await session.setServerIcon(serverID: serverID, picture: data, mediaType: mediaType)
            pictureSaved = true
        } catch {
            pictureError = ServerSettingsRefusal.picture(code: error.lumiaCode, details: error.lumiaDetails, message: error.lumiaMessage)
        }
    }

    private func removePicture() async {
        pictureError = ""
        pictureSaved = false
        pictureBusy = true
        defer { pictureBusy = false }
        do {
            try await session.removeServerIcon(serverID: serverID)
            pictureSaved = true
        } catch {
            pictureError = ServerSettingsRefusal.picture(code: error.lumiaCode, details: error.lumiaDetails, message: error.lumiaMessage)
        }
    }
}

struct NewMessageScreen: View {
    @Environment(\.setUpPrivateMessages) private var setUp
    let session: LumiaSession
    @Binding var path: [Screen]
    @State private var query = ""
    @State private var note = ""
    @State private var busy: String?

    private var list: [MemberSummary] {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        return session.people.values
            .filter { $0.accountId != session.me }
            .filter { q.isEmpty || $0.handle.lowercased().contains(q) || $0.displayName.lowercased().contains(q) || $0.nickname.lowercased().contains(q) }
            .sorted { ($0.displayName.isEmpty ? $0.handle : $0.displayName).localizedCompare($1.displayName.isEmpty ? $1.handle : $1.displayName) == .orderedAscending }
    }

    var body: some View {
        PanelScreen(back: "Home", title: "New message", subtitle: "Someone you share a server with") {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                    LumiaField("Find a person", text: $query, placeholder: "Name or handle")
                        .textInputAutocapitalization(.never).autocorrectionDisabled()
                    if !note.isEmpty { LumiaNotice(icon: "at") { Text(note) } }
                    if list.isEmpty {
                        LumiaEmptyState(icon: "at", title: "No one to show") { Text("Join a server to find people to message.") }
                    } else {
                        ForEach(list, id: \.accountId) { p in
                            let name = p.displayName.isEmpty ? p.handle : p.displayName
                            HStack(spacing: LumiaTokens.space10) {
                                LumiaAvatar(initials: initials(of: name), tone: .of(p.accountId), size: LumiaTokens.avatarSm, picture: session.picture(of: p.accountId))
                                VStack(alignment: .leading, spacing: 0) {
                                    Text(name).font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.ink.color)
                                    Text("@\(p.handle)").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                                }
                                Spacer()
                                Button("Message") { Task { await message(p) } }
                                    .buttonStyle(.lumiaSecondary).disabled(busy == p.accountId)
                            }
                            .padding(.vertical, LumiaTokens.space4)
                        }
                    }
                }
                .padding(LumiaTokens.space16)
            }
            .scrollDismissesKeyboard(.interactively)
        }
    }

    private func message(_ p: MemberSummary) async {
        if session.privateMessages.needsSomething { setUp(); return }
        busy = p.accountId
        note = ""
        defer { busy = nil }
        do {
            if let channelID = try await session.dmOpenOrRequest(accountID: p.accountId) {
                path = [.conversation(id: channelID)]
            } else {
                note = "Request sent to \(p.displayName.isEmpty ? p.handle : p.displayName). The conversation opens when they accept."
            }
        } catch {
            note = error.lumiaCode == "ALREADY_EXISTS" ? "You already asked; waiting for their answer." : error.lumiaMessage
        }
    }
}

// MARK: Profile

// The person's own profile (architecture 6.4): the signed profile the
// identity carries, display name, pronouns and bio, signed by this
// phone's device key and sent to the instance as the account's. The same
// screen as the web's and the desktop shell's.
struct ProfileScreen: View {
    @Environment(\.setUpPrivateMessages) private var setUp
    let session: LumiaSession
    @State private var profile: ProfileSummary?
    @State private var displayName = ""
    @State private var pronouns = ""
    @State private var bio = ""
    @State private var busy = false
    @State private var error = ""
    @State private var saved = false
    // The status (protocol 6.7): chosen here, kept by the core, sent with the servers watched.
    @State private var status = "ONLINE"
    @State private var customStatus = ""
    @State private var statusSaved = false
    // The form waits for the profile held: fields shown before it lands
    // would be overwritten by it, typing and all.
    @State private var loaded = false

    var body: some View {
        PanelScreen(back: "Home", title: "Profile", subtitle: "How people see you", actions: [], lead: {
            LumiaAvatar(initials: initials(of: session.selfName), tone: .of(session.me), size: LumiaTokens.avatarMd, picture: session.picture(of: session.me))
        }) {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                    LumiaCard(title: "Status", subtitle: "What people see beside your name in a server's member list.", width: nil) {
                        VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                            LumiaSegmentedTabs("Status", tabs: [.init(id: "ONLINE", label: "Online"), .init(id: "AWAY", label: "Away"), .init(id: "DO_NOT_DISTURB", label: "Do not disturb")], selected: $status)
                            LumiaField("Custom status", text: $customStatus, placeholder: "At lunch")
                            if statusSaved {
                                Text("Status saved.").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                            }
                            Button("Save status") { Task { await saveStatus() } }.buttonStyle(.lumiaPrimary)
                        }
                    }
                    if session.hasIdentity, loaded {
                        LumiaCard(title: handleTitle, subtitle: subtitle, width: nil) {
                            VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                                LumiaField("Display name", text: $displayName, placeholder: "How people see you")
                                    .textContentType(.nickname)
                                LumiaField("Pronouns", text: $pronouns, placeholder: "they/them")
                                    .textInputAutocapitalization(.never).autocorrectionDisabled()
                                LumiaField("Bio", text: $bio, placeholder: "A line about you")
                                if !error.isEmpty { LumiaErrorText(error) }
                                if saved {
                                    Text("Saved. This instance shows it now.").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                                }
                                Button { Task { await save() } } label: { Text("Save").frame(maxWidth: .infinity) }
                                    .buttonStyle(.lumiaPrimary).disabled(busy)
                            }
                        }
                    } else if !session.hasIdentity, session.privateMessages.needsSomething {
                        LumiaNotice(icon: "shield", tone: .accent, action: ("Set up", setUp.run)) {
                            Text("Your profile travels with your identity, signed by its devices. This phone doesn't hold it yet.")
                        }
                    }
                }
                .padding(LumiaTokens.space16)
            }
        }
        .scrollDismissesKeyboard(.interactively)
        .task { await load() }
    }

    private var handleTitle: String {
        (session.snapshot?.handle).flatMap { $0.isEmpty ? nil : "@\($0)" } ?? "You"
    }

    private var subtitle: String {
        profile.map { "Version \($0.version), signed by this phone's device key; every instance you sign in to takes it." } ?? "Nothing set yet: people see your handle."
    }

    private func saveStatus() async {
        statusSaved = false
        try? await session.setStatus(status: status, customStatus: customStatus)
        statusSaved = true
    }

    private func load() async {
        if let s = try? await session.status() {
            status = s.status
            customStatus = s.customStatus
        }
        profile = try? await session.profile()
        displayName = profile?.displayName ?? ""
        pronouns = profile?.pronouns ?? ""
        bio = profile?.bio ?? ""
        loaded = true
    }

    private func save() async {
        busy = true
        error = ""
        saved = false
        defer { busy = false }
        do {
            profile = try await session.setProfile(displayName: displayName, pronouns: pronouns, bio: bio)
            saved = true
        } catch {
            switch error.lumiaCode {
            case "FAILED_PRECONDITION": self.error = "This account is not linked to your identity, so the instance keeps the profile it has."
            case "CONFLICT": self.error = "A newer profile is already held. Reload and try again."
            default: self.error = error.lumiaMessage
            }
        }
    }
}

/// One of a few choices too long for a segment each, with a mark that
/// says which is chosen, as a segment says it.
private struct ChoiceRow: View {
    let label: String
    let chosen: Bool
    let choose: () -> Void

    init(_ label: String, chosen: Bool, choose: @escaping () -> Void) { self.label = label; self.chosen = chosen; self.choose = choose }

    var body: some View {
        Button(action: choose) {
            HStack(spacing: LumiaTokens.space12) {
                Image(systemName: chosen ? "largecircle.fill.circle" : "circle")
                    .font(.system(size: 20))
                    .foregroundStyle(chosen ? LumiaTokens.accent.color : LumiaTokens.inkQuiet.color)
                Text(label).font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.ink.color)
                Spacer(minLength: 0)
            }
            .padding(.vertical, LumiaTokens.space6)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .accessibilityAddTraits(chosen ? .isSelected : [])
    }
}

/// One of a card's settings (Server settings' Files card): its name over its choice.
private struct PreferenceRow<Content: View>: View {
    let label: String
    @ViewBuilder let content: () -> Content

    init(_ label: String, @ViewBuilder content: @escaping () -> Content) { self.label = label; self.content = content }

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space8) {
            Text(label).font(LumiaTokens.textLabel.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                .accessibilityHidden(true)
            content()
        }
    }
}

// MARK: Devices

struct DevicesScreen: View {
    @Environment(AppStore.self) private var store
    @Environment(\.setUpPrivateMessages) private var setUp
    let session: LumiaSession
    @Binding var path: [Screen]
    @State private var revoking: DeviceSummary?
    @State private var password = ""
    @State private var busy = false
    @State private var error = ""
    // Passkeys (architecture 6.3): the account's sign-in keys, listed and
    // removable here, made on the instance's web page in a browser sheet.
    @State private var passkeys: [PasskeySummary] = []
    @State private var passkeyBusy = false
    @State private var passkeyError = ""

    private var devices: [DeviceSummary] { session.roster?.devices ?? [] }
    private var active: [DeviceSummary] { devices.filter { !$0.revoked } }

    private func label(_ d: DeviceSummary) -> String {
        if d.thisDevice { return "This \(UIDevice.current.model)" }
        if !d.label.isEmpty { return d.label }
        switch d.kind {
        case "web": return "Browser"
        case "native": return "Computer"
        case "mobile": return "Phone"
        default: return d.kind.isEmpty ? "Device" : d.kind
        }
    }

    private func meta(_ d: DeviceSummary) -> String {
        var parts: [String] = []
        if !d.webOrigin.isEmpty { parts.append(d.webOrigin) }
        if d.addedAtMicros > 0 { parts.append("added \(day(ofMicros: d.addedAtMicros))") }
        parts.append(String(d.deviceId.prefix(8)))
        return parts.joined(separator: " · ")
    }

    var body: some View {
        // Link takes this phone into an identity, so it is offered only to a
        // phone without one; a phone with one approves other devices instead.
        PanelScreen(back: "Home", title: "Devices", subtitle: session.roster == nil ? "No identity on this phone" : "\(active.count) linked to you", actions: session.roster == nil ? [("plus", "Link this phone", { store.route = .link })] : [("plus", "Approve a device", { path.append(.approveDevice) })]) {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                    // Standing while the recovery phrase waits (architecture 5.4); never put off.
                    if session.phraseUnconfirmed {
                        PhraseNotSavedCard { path.append(.phrase) }
                    }
                    LumiaNotice(icon: "shield") {
                        Text("Your identity lives on these devices, not on any instance. Revoking one takes effect on every instance that holds your roster.")
                    }
                    LumiaCard(title: "Passkeys", subtitle: "Sign in to this instance with a key this phone keeps, instead of a password.", width: nil) {
                        VStack(alignment: .leading, spacing: LumiaTokens.space8) {
                            ForEach(passkeys, id: \.passkeyId) { p in
                                HStack {
                                    Text(passkeyLine(p)).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                                    Spacer()
                                    Button("Remove") { Task { await removePasskey(p) } }.buttonStyle(.lumiaLink)
                                }
                            }
                            if !passkeyError.isEmpty { LumiaErrorText(passkeyError) }
                            Button { Task { await addPasskey() } } label: { Label("Add a passkey", systemImage: "key").frame(maxWidth: .infinity) }
                                .buttonStyle(.lumiaSecondary).disabled(passkeyBusy)
                            Text("Made on the instance's web page, in a browser sheet signed in as you.")
                                .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                        }
                    }
                    if session.roster != nil {
                        ForEach(active, id: \.deviceId) { d in
                            LumiaDeviceRow(device: d, label: label(d), meta: meta(d)) { revoking = d; password = ""; error = "" }
                                .padding(.horizontal, LumiaTokens.space14)
                                .background(LumiaTokens.tile.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
                                .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).stroke(d.thisDevice ? LumiaTokens.accent.color : LumiaTokens.line.color))
                        }
                        Button { path.append(.approveDevice) } label: { Label("Approve a device", systemImage: "plus").frame(maxWidth: .infinity) }
                            .buttonStyle(.lumiaSecondary)
                        Text("Start linking on a laptop, a browser or another phone, and this phone asks you about it. Approve a device is for a request it shows when linked another way.")
                            .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                            .fixedSize(horizontal: false, vertical: true)
                        let revoked = devices.filter(\.revoked)
                        if !revoked.isEmpty {
                            SectionTitle("Revoked").padding(.horizontal, -LumiaTokens.space10)
                            ForEach(revoked, id: \.deviceId) { d in
                                LumiaDeviceRow(device: d, label: String(d.deviceId.prefix(8)), meta: "", revoke: nil)
                                    .padding(.horizontal, LumiaTokens.space14)
                            }
                        }
                    } else {
                        noIdentity
                    }
                }
                .padding(LumiaTokens.space16)
            }
        }
        .task {
            await session.refreshRoster()
            await loadPasskeys()
        }
        .sheet(item: Binding(get: { revoking }, set: { if $0 == nil { revoking = nil } })) { d in
            revokeSheet(d)
                .presentationDetents([.medium])
                .presentationDragIndicator(.visible)
                .presentationBackground(LumiaTokens.surface.color)
        }
    }

    /// A phone without the identity, by what the account needs of it.
    @ViewBuilder private var noIdentity: some View {
        switch session.privateMessages {
        case .bringIn:
            VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                Text("Private messages need this phone").font(LumiaTokens.titleChannel.font).foregroundStyle(LumiaTokens.ink.color)
                Text("Your account has private messages on another device. Bring this phone in to read and send them here.")
                    .font(LumiaTokens.textBody.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                NeedsSetupChoices { choice in
                    switch choice {
                    case .link: store.route = .link
                    case .restore: path.append(.restore)
                    case .startOver: path.append(.startOver)
                    }
                }
            }
            .padding(.vertical, LumiaTokens.space12)
        case .confirm, .failed:
            LumiaEmptyState(icon: "shield", title: "No identity on this phone", action: ("Set up private messages", setUp.run)) {
                Text("Private messages aren't set up for your account yet.")
            }
        case .unknown:
            LumiaEmptyState(icon: "shield", title: "No identity on this phone") {
                Text("Private messages are set up here as soon as the instance answers.")
            }
        case .setUp, .ready:
            LumiaEmptyState(icon: "shield", title: "Setting up private messages…")
        }
    }

    private func revokeSheet(_ d: DeviceSummary) -> some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space16) {
            VStack(alignment: .leading, spacing: LumiaTokens.space4) {
                Text("Revoke \(label(d))?").font(LumiaTokens.titleAuth.font).foregroundStyle(LumiaTokens.ink.color)
                Text("It stops reading new messages the moment every instance has the change. Confirm with your password.")
                    .font(LumiaTokens.textSubtitle.font).foregroundStyle(LumiaTokens.inkSecondary.color)
            }
            LumiaField("Password", text: $password, placeholder: "Your password", secure: true)
                .textContentType(.password)
                .onSubmit { Task { await revoke(d) } }
            if !error.isEmpty { LumiaErrorText(error) }
            HStack(spacing: LumiaTokens.space8) {
                Button("Revoke \(label(d))") { Task { await revoke(d) } }
                    .buttonStyle(.lumiaPrimary).disabled(busy || password.isEmpty)
                Button("Keep it") { revoking = nil }.buttonStyle(.lumiaSecondary)
            }
            Spacer()
        }
        .padding(LumiaTokens.space24)
        .padding(.top, LumiaTokens.space8)
    }

    private func passkeyLine(_ p: PasskeySummary) -> String {
        let added = p.createdAtMicros.map { day(ofMicros: $0) } ?? "—"
        let used = p.lastUsedAtMicros.map { day(ofMicros: $0) } ?? "never"
        return "\(p.label.isEmpty ? "Passkey" : p.label) · added \(added) · last used \(used)"
    }

    private func loadPasskeys() async {
        passkeys = (try? await session.passkeys()) ?? []
    }

    private func removePasskey(_ p: PasskeySummary) async {
        passkeyError = ""
        do {
            try await session.removePasskey(id: p.passkeyId)
            await loadPasskeys()
        } catch {
            passkeyError = error.lumiaMessage
        }
    }

    private func addPasskey() async {
        passkeyBusy = true
        passkeyError = ""
        defer { passkeyBusy = false }
        do {
            try await session.openWebSignedIn(next: "devices")
            await loadPasskeys()
        } catch {
            passkeyError = error.lumiaMessage
        }
    }

    private func revoke(_ d: DeviceSummary) async {
        busy = true
        error = ""
        defer { busy = false }
        do {
            try await session.identityRevoke(deviceID: d.deviceId, password: password)
            revoking = nil
            password = ""
        } catch {
            self.error = error.lumiaCode == "UNAUTHENTICATED" ? "That password was not accepted." : error.lumiaMessage
        }
    }
}

extension DeviceSummary: @retroactive Identifiable {
    public var id: String { deviceId }
}
