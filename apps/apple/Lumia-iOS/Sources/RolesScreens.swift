// Roles and titles on a phone (architecture 7.5, plan step 11), after the
// "Roles on phones" boards: a server's settings as a screen of its
// sections, and each section a panel pushed over it. Roles lists the
// server's roles in order, each moved from its menu and never dragged; a
// role's editor takes the list's place with one Save; Members lists the
// server's members, a row opening to its roles, title and Kick. The core
// holds every rule: what is listed, what may be opened, moved, given and
// taken are the snapshot's and the member list's own answers, read through
// the core's `roles…` functions, whose words (`Roles.words`) are every client's.
import LumiaFFI
import LumiaKit
import SwiftUI

/// What the boards measure that the tokens do not name.
enum RoleMetrics {
    /// A section's row on its own screen: 52 high.
    static let sectionRow = LumiaTokens.space40 + LumiaTokens.space12
    /// A role's row: at least 56 high.
    static let roleRow = LumiaTokens.space40 + LumiaTokens.space16
    /// A row's More, and the least a search field stands: 36 and 40.
    static let more = LumiaTokens.space18 * 2
    static let search = LumiaTokens.space40
    /// A colour's swatch, 24 across, in a cell of 30 so two stand 6 apart and a finger finds one.
    static let swatch = LumiaTokens.space24
    static let swatchCell = LumiaTokens.space24 + LumiaTokens.space6
    /// A tick's box: 18, its corners 5.
    static let box = LumiaTokens.space18
    /// A row's glyph: 15, and the lock and the eye a little smaller, 13.
    static let glyph = LumiaTokens.textBody.size
    static let smallGlyph = LumiaTokens.textLinkSm.size
    /// A section's name, 15 at 600; a part's heading, 17 at 700 in the display face; a note, 13.
    static let sectionName = LumiaType(size: LumiaTokens.textBody.size, weight: 600, display: false)
    static let heading = LumiaType(size: LumiaTokens.titleServer.size - LumiaTokens.space2, weight: 700, display: true)
    static let note = LumiaType(size: LumiaTokens.textLinkSm.size, weight: 400, display: false)
    /// The line over a list saying how it works, 13.5; and "None" after the swatches, 12 at 600.
    static let lead = LumiaType(size: LumiaTokens.textLabel.size, weight: 400, display: false)
    static let none = LumiaType(size: LumiaTokens.textTag.size + LumiaTokens.space2 / 4, weight: 600, display: false)
    /// A sheet's title: 22 at 700 in the display face.
    static let sheetTitle = LumiaType(size: LumiaTokens.titleChannel.size + 1, weight: 700, display: true)
}

extension Screen {
    /// Where "Server settings" leads for this account: the screen of its
    /// sections, or straight to the one section it has.
    static func settings(of server: ServerSummary) -> Screen {
        let sections = rolesSettingsSections(server: server)
        return sections.count == 1 ? section(sections[0], of: server.serverId) : .serverSettings(serverID: server.serverId)
    }

    static func section(_ section: SettingsSection, of serverID: String) -> Screen {
        switch section {
        case .overview: .serverOverview(serverID: serverID)
        case .roles: .roles(serverID: serverID)
        case .members: .serverMembers(serverID: serverID)
        }
    }
}

/// What a section's back button goes back to, as its label says it: the
/// sections' screen, or the server where this account has one section only.
@MainActor func settingsBack(_ server: ServerSummary?) -> String {
    guard let server else { return "Server" }
    return rolesSettingsSections(server: server).count == 1 ? server.name : "Server settings"
}

// MARK: The sections

/// Server settings: the sections this account may use, each with what it
/// holds, as a screen of their own.
struct ServerSettingsSectionsScreen: View {
    let session: LumiaSession
    let serverID: String
    @Binding var path: [Screen]

    private var server: ServerSummary? { session.snapshot?.servers.first { $0.serverId == serverID } }

    var body: some View {
        PanelScreen(back: server?.name ?? "Server", title: "Server settings", subtitle: server?.name) {
            ScrollView {
                if let server {
                    let sections = rolesSettingsSections(server: server)
                    VStack(alignment: .leading, spacing: 0) {
                        if sections.isEmpty {
                            // What let this account in was taken away while the screen was open.
                            Text(ServerSettingsRefusal.notAllowed).font(LumiaTokens.textCompact.font).foregroundStyle(LumiaTokens.inkMuted.color)
                        } else {
                            VStack(spacing: 0) {
                                ForEach(Array(sections.enumerated()), id: \.element) { index, section in
                                    if index > 0 { LumiaTokens.line.color.frame(height: 1) }
                                    row(section, server)
                                }
                            }
                            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).strokeBorder(LumiaTokens.lineStrong.color))
                            .accessibilityElement(children: .contain)
                            .accessibilityLabel("Server settings")
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(LumiaTokens.space16)
                }
            }
        }
        // Who is in the server now, so the counts start right.
        .task { await session.refreshMembers(serverID: serverID) }
    }

    private func note(_ section: SettingsSection, _ server: ServerSummary) -> String? {
        switch section {
        case .overview: nil
        case .roles: rolesCount(roles: server.roles)
        case .members: session.serverMembers[serverID].map { Roles.grouped($0.count) }
        }
    }

    private func icon(_ section: SettingsSection) -> String {
        switch section {
        case .overview: "gearshape"
        case .roles: "checkmark.shield"
        case .members: "person.2"
        }
    }

    private func row(_ section: SettingsSection, _ server: ServerSummary) -> some View {
        let note = note(section, server)
        return Button { path.append(Screen.section(section, of: serverID)) } label: {
            HStack(spacing: LumiaTokens.space10) {
                Image(systemName: icon(section)).font(.system(size: RoleMetrics.glyph)).foregroundStyle(LumiaTokens.inkQuiet.color)
                    .frame(width: LumiaTokens.space18)
                Text(section.label).font(RoleMetrics.sectionName.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1)
                Spacer(minLength: LumiaTokens.space8)
                if let note { Text(note).font(RoleMetrics.note.font).foregroundStyle(LumiaTokens.inkMuted.color).lineLimit(1) }
                Image(systemName: "chevron.right").font(.system(size: RoleMetrics.smallGlyph, weight: .semibold)).foregroundStyle(LumiaTokens.inkFaint.color)
            }
            .padding(.horizontal, LumiaTokens.space14)
            .frame(height: RoleMetrics.sectionRow)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(section.label)
        .accessibilityValue(note ?? "")
        .accessibilityIdentifier("section-\(section.key)")
    }
}

/// The drawing in a "Tag beside names" tile (the boards' `TagSketch`): two
/// names, each with its tag, a role's on its tone or a title's on the plain chip.
struct TagSketch: View {
    let titles: Bool

    /// The tile's picture: 92 high, a face 20 across, a bar 5 thick.
    static let height = LumiaTokens.space40 * 2 + LumiaTokens.space12
    private static let face = LumiaTokens.space18 + LumiaTokens.space2
    private static let bar = LumiaTokens.space10 / 2

    /// The names' bars are the boards' 34 and 26 halved, 16 and 12: a tile
    /// is half a phone's card, and the tags' words are drawn whole.
    private var rows: [(face: AvatarTone, name: CGFloat, tag: String, tone: AvatarTone?)] {
        titles
            ? [(.peach, LumiaTokens.space16, "Designer", nil), (.mint, LumiaTokens.space12, "Engineer", nil)]
            : [(.peach, LumiaTokens.space16, "Admin", .indigo), (.mint, LumiaTokens.space12, "Design", .peach)]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space12) {
            ForEach(rows, id: \.tag) { row in
                HStack(spacing: LumiaTokens.space8) {
                    Circle().fill(row.face.fill.color).frame(width: Self.face, height: Self.face)
                    Capsule().fill(LumiaTokens.borderActive.color).frame(width: row.name, height: Self.bar)
                    LumiaTag(TagLook(label: row.tag, tone: row.tone?.rawValue), size: .sm)
                }
            }
        }
        .padding(LumiaTokens.space14)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        .background(LumiaTokens.surface.color)
        .accessibilityHidden(true)
    }
}

// MARK: Small pieces

/// A line under a field or a heading saying what it takes or does.
struct RoleHint: View {
    let text: String
    init(_ text: String) { self.text = text }
    var body: some View {
        Text(text).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A part's small heading: "Colour", "Roles".
struct RoleLegend: View {
    let text: String
    init(_ text: String) { self.text = text }
    var body: some View {
        Text(text).font(LumiaTokens.textLabel.font).foregroundStyle(LumiaTokens.inkBody.color)
    }
}

/// A person in a settings list: their picture, their name with a mark
/// beside it, and a line under it.
struct PersonLine: View {
    let session: LumiaSession
    let member: MemberSummary
    var mark: String?
    var line: String?

    var body: some View {
        let name = session.name(of: member.accountId)
        HStack(spacing: LumiaTokens.space12) {
            LumiaAvatar(initials: initials(of: name), tone: .of(member.accountId), size: LumiaTokens.avatarMd, picture: session.picture(of: member.accountId))
            VStack(alignment: .leading, spacing: 0) {
                NameWithTag {
                    Text(name).font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1)
                    if let mark { LumiaTag(mark, tone: .accent, size: .sm) }
                }
                Text(line ?? "@\(member.handle)").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color).lineLimit(1)
            }
            Spacer(minLength: 0)
        }
    }
}

/// A member's roles to tick (`lu-rolepick`): a box, the role's dot and its
/// name; one not this account's to give or take is shown with a lock and
/// is not changed.
struct RoleTicks: View {
    let choices: [RoleChoice]
    let toggle: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(choices) { choice in
                Button { toggle(choice.id) } label: {
                    HStack(spacing: LumiaTokens.space10) {
                        box(choice)
                        LumiaRoleDot(tone: choice.avatarTone)
                        Text(choice.name).font(LumiaTokens.textCompact.font)
                            .foregroundStyle(choice.locked ? LumiaTokens.inkMuted.color : LumiaTokens.ink.color).lineLimit(1)
                        Spacer(minLength: 0)
                        if choice.locked {
                            Image(systemName: "lock").font(.system(size: RoleMetrics.smallGlyph)).foregroundStyle(LumiaTokens.inkFaint.color)
                        }
                    }
                    .padding(.vertical, LumiaTokens.space8)
                    .padding(.horizontal, LumiaTokens.space8)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(choice.locked)
                .accessibilityLabel(choice.name)
                .accessibilityValue(choice.checked ? "ticked" : "not ticked")
                .accessibilityHint(choice.locked ? "At or above your own highest role, so not yours to change" : "")
                .accessibilityIdentifier("tick-\(choice.name)")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Roles")
    }

    private func box(_ choice: RoleChoice) -> some View {
        let fill: Color = switch (choice.checked, choice.locked) {
        case (true, false): LumiaTokens.accent.color
        case (true, true): LumiaTokens.inkFaint.color
        case (false, true): LumiaTokens.inset.color
        case (false, false): LumiaTokens.field.color
        }
        let edge: Color = switch (choice.checked, choice.locked) {
        case (true, false): LumiaTokens.accent.color
        case (true, true): LumiaTokens.inkFaint.color
        case (false, true): LumiaTokens.lineStrong.color
        case (false, false): LumiaTokens.borderForm.color
        }
        return RoundedRectangle(cornerRadius: LumiaTokens.radius5).fill(fill)
            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius5).strokeBorder(edge))
            .overlay {
                if choice.checked {
                    Image(systemName: "checkmark").font(.system(size: LumiaTokens.textCount.size, weight: .heavy)).foregroundStyle(LumiaTokens.onAccent.color)
                }
            }
            .frame(width: RoleMetrics.box, height: RoleMetrics.box)
    }
}

/// Kick, among a member's buttons: the secondary button in the danger's ink and line.
struct KickButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var enabled
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(LumiaTokens.textButtonSecondary.font)
            .foregroundStyle(LumiaTokens.danger.color)
            .padding(.horizontal, LumiaTokens.space14)
            .frame(minHeight: LumiaTokens.controlHeight)
            .background(configuration.isPressed ? LumiaTokens.dangerSoft.color : LumiaTokens.raised.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius8))
            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius8).stroke(configuration.isPressed ? LumiaTokens.danger.color : LumiaTokens.dangerLine.color))
            .opacity(enabled ? 1 : 0.5)
    }
}

/// How tall a sheet's words stand, so the sheet is no taller than they are.
private struct SheetHeight: PreferenceKey {
    static var defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

/// A question asked first, in a sheet (the "Deleting a role asks first"
/// board): what is asked, what it costs in a sentence or two, the button
/// that goes on in its own words and the one that does not. A refusal is
/// said in the sheet, where it was tried.
struct AskSheet: View {
    let title: String
    let lines: [String]
    let confirm: String
    var danger = false
    let cancel: String
    var busy = false
    var error = ""
    let go: () -> Void
    let stop: () -> Void
    @State private var height: CGFloat = 0

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space16) {
            VStack(alignment: .leading, spacing: LumiaTokens.space8) {
                Text(title).font(RoleMetrics.sheetTitle.font).foregroundStyle(LumiaTokens.ink.color)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityAddTraits(.isHeader)
                Text(lines.joined(separator: " ")).font(LumiaTokens.textSubtitle.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if !error.isEmpty { LumiaErrorText(error) }
            HStack(spacing: LumiaTokens.space12) {
                if danger {
                    Button(confirm, action: go).buttonStyle(.lumiaDanger).accessibilityIdentifier("ask-confirm")
                } else {
                    Button(confirm, action: go).buttonStyle(.lumiaPrimary).accessibilityIdentifier("ask-confirm")
                }
                Button(cancel, action: stop).buttonStyle(.lumiaSecondary).accessibilityIdentifier("ask-cancel")
            }
            .disabled(busy)
        }
        .padding(.horizontal, LumiaTokens.space24)
        .padding(.top, LumiaTokens.space28)
        .padding(.bottom, LumiaTokens.space16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(GeometryReader { g in Color.clear.preference(key: SheetHeight.self, value: g.size.height) })
        .onPreferenceChange(SheetHeight.self) { height = $0 }
        .presentationDetents(height > 0 ? [.height(height)] : [.medium])
        .presentationDragIndicator(.visible)
        .presentationBackground(LumiaTokens.surface.color)
    }
}

/// "Delete Moderator?", from a role's menu and from its editor: how many
/// hold it, then Delete role or Keep it.
struct DeleteRoleSheet: View {
    let session: LumiaSession
    let server: ServerSummary
    let role: RoleSummary
    /// The role is gone: the sheet closes, and its editor with it.
    let done: () -> Void
    let keep: () -> Void
    @State private var busy = false
    @State private var error = ""

    var body: some View {
        AskSheet(
            title: rolesDeleteTitle(roleName: role.name),
            lines: rolesDeleteQuestion(held: rolesHolders(role: role, members: session.serverMembers[server.serverId])),
            confirm: "Delete role", danger: true, cancel: "Keep it", busy: busy, error: error,
            go: { Task { await delete() } }, stop: keep
        )
    }

    private func delete() async {
        busy = true
        error = ""
        defer { busy = false }
        do {
            try await session.deleteRole(serverID: server.serverId, roleID: role.roleId)
            done()
        } catch {
            self.error = Roles.roleRefusal(error, serverName: server.name)
        }
    }
}

// MARK: Roles

/// A server's roles in order, highest first, Everyone last: each with how
/// many hold it, opening its editor; a role moved up or down, edited or
/// deleted from its More; one at or above this account's own highest shown
/// with a lock, not opened and not moved; and New role.
struct RolesScreen: View {
    let session: LumiaSession
    let serverID: String
    @Binding var path: [Screen]
    /// A move drawn at once, until the instance answers it.
    @State private var moved: [RoleSummary]?
    @State private var busy = false
    @State private var error = ""
    @State private var deleting: RoleSummary?

    private var server: ServerSummary? { session.snapshot?.servers.first { $0.serverId == serverID } }
    private var roles: [RoleSummary] { moved ?? server?.roles ?? [] }

    var body: some View {
        PanelScreen(back: settingsBack(server), title: "Roles", subtitle: server.map { "\($0.name) · \(rolesCount(roles: $0.roles))" }) {
            ScrollView {
                if let server {
                    VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                        Text(Roles.words.rolesHintMenu).font(RoleMetrics.lead.font)
                            .foregroundStyle(LumiaTokens.inkMuted.color)
                            .fixedSize(horizontal: false, vertical: true)
                        if !error.isEmpty { LumiaErrorText(error).accessibilityIdentifier("roles-error") }
                        let roles = roles
                        VStack(spacing: LumiaTokens.space6) {
                            ForEach(roles, id: \.roleId) { role in row(role, among: roles) }
                        }
                        .accessibilityElement(children: .contain)
                        .accessibilityLabel("Roles")
                        if server.canManageRoles {
                            Button { path.append(.role(serverID: serverID, roleID: nil)) } label: {
                                Label("New role", systemImage: "plus").frame(maxWidth: .infinity)
                            }
                            .buttonStyle(.lumiaSecondary)
                        }
                    }
                    .padding(LumiaTokens.space16)
                }
            }
        }
        .sheet(item: $deleting) { role in
            if let server {
                DeleteRoleSheet(session: session, server: server, role: role, done: { deleting = nil }, keep: { deleting = nil })
            }
        }
        .task { if session.serverMembers[serverID] == nil { await session.refreshMembers(serverID: serverID) } }
    }

    private func row(_ role: RoleSummary, among roles: [RoleSummary]) -> some View {
        let locked = rolesLocked(role: role)
        let moves = !role.isEveryone && !locked
        let held = Roles.holders(role, members: session.serverMembers[serverID])
        return HStack(spacing: 0) {
            if locked {
                words(role, held: held, end: "lock", moves: false)
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(role.name)
                    .accessibilityValue(PhoneRoleWords.rowValue(held: held, hideTag: role.hideTag, locked: true))
            } else {
                Button { path.append(.role(serverID: serverID, roleID: role.roleId)) } label: {
                    words(role, held: held, end: "chevron.right", moves: moves).contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(role.name)
                .accessibilityValue(PhoneRoleWords.rowValue(held: held, hideTag: role.hideTag, locked: false))
                .accessibilityIdentifier("role-\(role.name)")
            }
            if moves { menu(role, among: roles) }
        }
        .frame(minHeight: RoleMetrics.roleRow)
        .background(locked ? LumiaTokens.inset.color : LumiaTokens.surface.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).strokeBorder(LumiaTokens.lineStrong.color))
    }

    private func words(_ role: RoleSummary, held: Int?, end: String, moves: Bool) -> some View {
        let locked = rolesLocked(role: role)
        return HStack(spacing: LumiaTokens.space12) {
            LumiaRoleDot(tone: Roles.tone(of: role.color))
            VStack(alignment: .leading, spacing: 1) {
                Text(role.name).font(LumiaTokens.textButtonSecondary.font)
                    .foregroundStyle(locked ? LumiaTokens.inkBody.color : LumiaTokens.ink.color).lineLimit(1)
                // Side by side where they fit, else one under the other.
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: LumiaTokens.space12) { meta(role, held: held) }
                    VStack(alignment: .leading, spacing: 0) { meta(role, held: held) }
                }
                .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color)
            }
            Spacer(minLength: 0)
            Image(systemName: end).font(.system(size: locked ? RoleMetrics.glyph : RoleMetrics.smallGlyph, weight: locked ? .regular : .semibold))
                .foregroundStyle(locked ? LumiaTokens.inkQuiet.color : LumiaTokens.inkFaint.color)
        }
        .padding(.vertical, LumiaTokens.space8)
        .padding(.leading, LumiaTokens.space14)
        .padding(.trailing, moves ? LumiaTokens.space6 : LumiaTokens.space14)
        .frame(maxWidth: .infinity, minHeight: RoleMetrics.roleRow, alignment: .leading)
    }

    @ViewBuilder private func meta(_ role: RoleSummary, held: Int?) -> some View {
        if let held { Text(Roles.membersCount(held)).lineLimit(1) }
        if role.hideTag {
            HStack(spacing: LumiaTokens.space4) {
                Image(systemName: "eye.slash").font(.system(size: LumiaTokens.textCount.size))
                Text("Not shown as tag").lineLimit(1)
            }
        }
    }

    /// A role's More: Edit role, Move up and Move down in place of a drag
    /// (each only where there is such a step), and Delete role.
    private func menu(_ role: RoleSummary, among roles: [RoleSummary]) -> some View {
        Menu {
            Section(role.name) {
                Button { path.append(.role(serverID: serverID, roleID: role.roleId)) } label: { Label("Edit role", systemImage: "gearshape") }
            }
            Section {
                Button { step(role, up: true) } label: { Label("Move up", systemImage: "arrow.up") }
                    .disabled(busy || rolesStepBefore(roles: roles, roleId: role.roleId, up: true) == nil)
                Button { step(role, up: false) } label: { Label("Move down", systemImage: "arrow.down") }
                    .disabled(busy || rolesStepBefore(roles: roles, roleId: role.roleId, up: false) == nil)
            }
            Section {
                Button(role: .destructive) { deleting = role } label: { Label("Delete role", systemImage: "trash") }
            }
        } label: {
            Image(systemName: "ellipsis").font(.system(size: 17, weight: .semibold)).foregroundStyle(LumiaTokens.iconStrong.color)
                .frame(width: RoleMetrics.more, height: RoleMetrics.more)
                .contentShape(Rectangle())
        }
        .accessibilityLabel("More for \(role.name)")
        .accessibilityIdentifier("more-\(role.name)")
        .padding(.trailing, LumiaTokens.space6)
    }

    /// A step up or down: the list drawn moved at once, the whole order
    /// sent, and on a refusal the list as it was with the refusal said.
    private func step(_ role: RoleSummary, up: Bool) {
        let now = roles
        guard !busy, let before = rolesStepBefore(roles: now, roleId: role.roleId, up: up) else { return }
        let drawn = Roles.drawnMoved(now, roleID: role.roleId, beforeID: before)
        let order = rolesMovedBefore(roles: now, roleId: role.roleId, beforeId: before)
        moved = drawn
        error = ""
        busy = true
        if let position = drawn.firstIndex(where: { $0.roleId == role.roleId }) {
            AccessibilityNotification.Announcement(rolesMovedWords(roleName: role.name, position: UInt64(position + 1))).post()
        }
        Task {
            defer { busy = false; moved = nil }
            do {
                try await session.reorderRoles(serverID: serverID, roleIDs: order)
            } catch {
                self.error = Roles.roleRefusal(error, serverName: server?.name ?? "this server")
            }
        }
    }
}

extension RoleSummary: @retroactive Identifiable {
    public var id: String { roleId }
}

// MARK: A role's editor

/// A role's editor, in the list's place: its name, colour and whether it is
/// shown as a tag, with a preview; the permissions under their headings,
/// Administrator apart and asked first, one this account does not hold
/// shown and not turned; who holds it, with Remove and Add members; and
/// Delete role, asked first. One Save, in the header, keeps the role; the
/// holders are given and taken as they are pressed, each a change of that
/// one role and never the whole set of a member's. Everyone's editor is
/// its permissions only, and a new role's has no holders and nothing to delete.
///
/// The editor holds the role as it opened on it (`opened`) beside the one
/// being edited (`draft`). Save is offered when the two differ and sends
/// what differs, so what someone else changed of the role meanwhile is
/// neither offered to save nor undone by it; an editor nobody has touched
/// follows the role as it changes (the core's `rolesFollowed`).
struct RoleEditorScreen: View {
    @Environment(\.dismiss) private var dismiss
    let session: LumiaSession
    let serverID: String
    /// Nil for a new role.
    let roleID: String?
    @Binding var path: [Screen]
    @State private var draft = rolesDraftOf(role: nil)
    /// The role as the editor opened on it, or as its last save left it; nil for a new role.
    @State private var opened: RoleDraft?
    @State private var loaded = false
    @State private var busy = false
    @State private var error = ""
    @State private var saved = false
    @State private var askingAdministrator = false
    @State private var askingDelete = false
    /// The role was deleted from here: nothing more is drawn of it while the screen goes.
    @State private var leaving = false
    @State private var holdersError = ""
    @State private var removing: String?
    /// When the preview's message was "sent": as the screen opened.
    @State private var openedAt = Date()

    private var server: ServerSummary? { session.snapshot?.servers.first { $0.serverId == serverID } }
    private var role: RoleSummary? { roleID.flatMap { id in server?.roles.first { $0.roleId == id } } }
    private var everyone: Bool { role?.isEveryone ?? false }
    private var members: [MemberSummary]? { session.serverMembers[serverID] }
    private var held: Int? { role.flatMap { Roles.holders($0, members: members) } }
    private var holding: [MemberSummary] {
        guard let role, !everyone else { return [] }
        return (members ?? []).filter { $0.roleIds.contains(role.roleId) }
    }
    private var shownName: String {
        guard let role else { return "New role" }
        if everyone { return role.name }
        let typed = draft.name.trimmingCharacters(in: .whitespacesAndNewlines)
        return typed.isEmpty ? role.name : typed
    }
    private var subtitle: String? {
        guard let server else { return nil }
        guard role != nil, let held else { return server.name }
        return "\(server.name) · \(Roles.membersCount(held))"
    }
    private var savable: Bool { loaded && !busy && rolesSavable(draft: draft, opened: opened) && (roleID == nil || role != nil) }

    var body: some View {
        PanelScreen(back: "Roles", title: roleID == nil ? "New role" : shownName, subtitle: subtitle, actions: [(icon: "checkmark", label: "Save", run: { Task { await save() } })], inactive: savable ? [] : ["Save"]) {
            VStack(spacing: 0) {
                // What Save came to, in view wherever the editor is scrolled.
                if !error.isEmpty || saved {
                    Group {
                        if !error.isEmpty { LumiaErrorText(error).accessibilityIdentifier("role-error") } else {
                            Text("Saved.").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .accessibilityIdentifier("role-saved")
                        }
                    }
                    .padding(.horizontal, LumiaTokens.space16)
                    .padding(.vertical, LumiaTokens.space8)
                    LumiaTokens.line.color.frame(height: 1)
                }
                ScrollViewReader { proxy in
                    ScrollView {
                        if let server, loaded, !leaving {
                            if roleID != nil, role == nil {
                                // Deleted elsewhere while it was open.
                                Text(rolesRoleRefusal(code: "NOT_FOUND", field: "", message: "", serverName: server.name))
                                    .font(LumiaTokens.textCompact.font).foregroundStyle(LumiaTokens.inkMuted.color)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .padding(LumiaTokens.space16)
                            } else {
                                VStack(alignment: .leading, spacing: LumiaTokens.space24) {
                                    if everyone { RoleHint(Roles.words.everyoneSubtitle) } else { identity(server) }
                                    permissions(server).id("permissions")
                                    if let role, !everyone {
                                        holders(role)
                                        delete.id("end")
                                    }
                                }
                                .padding(LumiaTokens.space16)
                            }
                        }
                    }
                    .scrollDismissesKeyboard(.interactively)
                    #if DEBUG
                    // The smoke run's pictures further down (`LUMIA_DEV_SCROLL` `permissions` or `end`).
                    .task(id: loaded) {
                        guard loaded, let to = ProcessInfo.processInfo.environment["LUMIA_DEV_SCROLL"] else { return }
                        try? await Task.sleep(for: .milliseconds(300))
                        proxy.scrollTo(to, anchor: to == "end" ? .bottom : .top)
                        if ProcessInfo.processInfo.environment["LUMIA_DEV_ASK"] == "delete" { askingDelete = true }
                        if ProcessInfo.processInfo.environment["LUMIA_DEV_ASK"] == "administrator" { askingAdministrator = true }
                    }
                    #endif
                }
            }
        }
        .onChange(of: server == nil, initial: true) { _, _ in load() }
        // The editor of a new role is the editor of the role made, once it is: opened on that.
        .onChange(of: roleID) { _, _ in
            loaded = false
            load()
        }
        // The role changed underneath: an editor nobody has touched follows it.
        .onChange(of: role) { _, now in follow(now) }
        .onChange(of: draft) { _, now in
            if now.name.count > Roles.labelMax { draft.name = String(now.name.prefix(Roles.labelMax)) }
            // The draft as a save or the role itself left it is not a change: "Saved." stays said.
            if let opened, !Roles.untouched(opened: opened, draft: now) { saved = false }
        }
        .sheet(isPresented: $askingAdministrator) {
            AskSheet(
                title: Roles.words.administratorTitle,
                lines: rolesAdministratorQuestion(serverName: server?.name ?? "this server", roleName: shownName, held: role == nil ? nil : held.map { UInt64($0) }),
                confirm: "Turn on", cancel: "Cancel",
                go: {
                    if !draft.permissions.contains(Roles.words.administrator) { draft.permissions.append(Roles.words.administrator) }
                    askingAdministrator = false
                },
                stop: { askingAdministrator = false }
            )
        }
        .sheet(isPresented: $askingDelete) {
            if let server, let role {
                DeleteRoleSheet(session: session, server: server, role: role, done: {
                    leaving = true
                    askingDelete = false
                    dismiss()
                }, keep: { askingDelete = false })
            }
        }
        .task { if session.serverMembers[serverID] == nil { await session.refreshMembers(serverID: serverID) } }
    }

    /// The fields as the role has them, once, and the role as it was then,
    /// which is what a save is measured against.
    private func load() {
        guard !loaded, server != nil else { return }
        draft = rolesDraftOf(role: role)
        opened = roleID == nil ? nil : draft
        loaded = true
    }

    /// The role as it now is, changed from elsewhere: followed while nothing
    /// here was changed; once something was, what is typed is not written
    /// over, and the save sends that and nothing else.
    private func follow(_ role: RoleSummary?) {
        guard loaded, let role, let was = opened else { return }
        let next = rolesFollowed(opened: was, draft: draft, now: rolesDraftOf(role: role))
        opened = next.opened
        draft = next.draft
    }

    // MARK: Name, colour, tag

    private func identity(_ server: ServerSummary) -> some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space10) {
            VStack(alignment: .leading, spacing: LumiaTokens.space6) {
                LumiaField("Name", text: $draft.name, placeholder: "Moderator", spoken: "Name")
                    .autocorrectionDisabled()
                RoleHint(rolesNameHint(serverName: server.name))
            }
            VStack(alignment: .leading, spacing: LumiaTokens.space6) {
                RoleLegend("Colour")
                swatches
            }
            VStack(alignment: .leading, spacing: 0) {
                let shown = !draft.hideTag
                HStack(spacing: LumiaTokens.space16) {
                    VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                        Text("Show as tag").font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.ink.color)
                        RoleHint(rolesTagSwitchCaption(shown: shown))
                    }
                    .accessibilityHidden(true)
                    Toggle("Show as tag", isOn: Binding(get: { !draft.hideTag }, set: { draft.hideTag = !$0 }))
                        .labelsHidden().tint(LumiaTokens.accent.color)
                        .accessibilityHint(rolesTagSwitchCaption(shown: shown))
                }
                .padding(.vertical, LumiaTokens.space14)
                .padding(.horizontal, LumiaTokens.space16)
                LumiaTokens.line.color.frame(height: 1)
                preview
            }
            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).strokeBorder(LumiaTokens.lineStrong.color))
        }
    }

    /// The nine tones and None: one chosen, ringed in the accent.
    private var swatches: some View {
        LumiaFlow(spacing: 0) {
            ForEach(Roles.tones, id: \.self) { tone in
                let chosen = draft.color == tone.rawValue
                Button { draft.color = tone.rawValue } label: {
                    Circle().fill(tone.fill.color)
                        .overlay { if !chosen { Circle().strokeBorder(LumiaTokens.lineStrong.color) } }
                        .overlay {
                            if chosen { Image(systemName: "checkmark").font(.system(size: LumiaTokens.textCount.size, weight: .heavy)).foregroundStyle(tone.ink.color) }
                        }
                        .frame(width: RoleMetrics.swatch, height: RoleMetrics.swatch)
                        .overlay { if chosen { ring(Circle()) } }
                        .frame(width: RoleMetrics.swatchCell, height: RoleMetrics.swatchCell)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(RailWords.toneName(tone))
                .accessibilityAddTraits(chosen ? .isSelected : [])
            }
            let none = draft.color.isEmpty
            Button { draft.color = "" } label: {
                Text("None").font(RoleMetrics.none.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    .padding(.horizontal, LumiaTokens.space10)
                    .frame(height: RoleMetrics.swatch)
                    .background(LumiaTokens.chip.color, in: Capsule())
                    .overlay { if none { ring(Capsule()) } }
                    .padding(.horizontal, LumiaTokens.space6 / 2)
                    .frame(height: RoleMetrics.swatchCell)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("None")
            .accessibilityAddTraits(none ? .isSelected : [])
        }
        // The cells' own room is the swatches' gap: the row starts where the field does.
        .padding(.leading, -LumiaTokens.space6 / 2)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Colour")
    }

    /// The chosen one's ring: 2 wide, 2 off its edge.
    private func ring<S: InsettableShape>(_ shape: S) -> some View {
        shape.strokeBorder(LumiaTokens.accent.color, lineWidth: LumiaTokens.space2).padding(-LumiaTokens.space4)
    }

    /// How a message of this account's would carry the role's tag.
    private var preview: some View {
        let name = session.name(of: session.me)
        let tag = draft.name.trimmingCharacters(in: .whitespacesAndNewlines)
        let shows = !draft.hideTag && !tag.isEmpty
        // On one line where it fits; a long name's tag and time go under it, as the board's row wraps.
        return LumiaFlow(spacing: LumiaTokens.space8) {
            Text("Preview").font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.inkMuted.color)
                .padding(.trailing, LumiaTokens.space4)
            LumiaAvatar(initials: initials(of: name), tone: .of(session.me), size: LumiaTokens.avatarSm, picture: session.picture(of: session.me))
            Text(name).font(LumiaTokens.textAuthor.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1)
            if shows { LumiaTag(TagLook(label: tag, tone: rolesToneOf(color: draft.color))) }
            Text(time(ofMicros: Int64(openedAt.timeIntervalSince1970 * 1_000_000))).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color)
                .lineLimit(1)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, LumiaTokens.space12)
        .padding(.horizontal, LumiaTokens.space16)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Preview")
        .accessibilityValue(shows ? "\(name), \(tag)" : "\(name), no tag")
    }

    // MARK: Permissions

    private func permissions(_ server: ServerSummary) -> some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space10) {
            Text("Permissions").font(RoleMetrics.heading.font).foregroundStyle(LumiaTokens.ink.color)
                .accessibilityAddTraits(.isHeader)
            ForEach(Roles.permissions, id: \.title) { group in
                Text(group.title).font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.inkMuted.color)
                    .accessibilityAddTraits(.isHeader)
                // Administrator, set apart: the one that grants every other.
                if let apart = group.permissions.first(where: { $0.name == Roles.words.administrator }) {
                    permission(apart, server, apart: true)
                        .background(LumiaTokens.toneAmber.color.opacity(0.6), in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
                        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).strokeBorder(LumiaTokens.toneAmber.color))
                        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).strokeBorder(LumiaTokens.toneAmberInk.color.opacity(0.22)))
                }
                let rest = group.permissions.filter { $0.name != Roles.words.administrator }
                VStack(spacing: 0) {
                    ForEach(Array(rest.enumerated()), id: \.element.name) { index, p in
                        if index > 0 { LumiaTokens.line.color.frame(height: 1) }
                        permission(p, server, apart: false)
                    }
                }
                .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).strokeBorder(LumiaTokens.lineStrong.color))
            }
        }
    }

    private func permission(_ p: PermissionChoice, _ server: ServerSummary, apart: Bool) -> some View {
        let withheld = rolesWithheld(grantable: server.grantable, permission: p.name)
        // Everyone's permissions are changed only by one who may.
        let off = withheld || role?.canManage == false
        let caption = withheld ? rolesWithheldCaption(detail: p.detail) : p.detail
        return HStack(spacing: LumiaTokens.space16) {
            VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                Text(p.label).font(LumiaTokens.textMember.font).foregroundStyle(apart ? LumiaTokens.toneAmberInk.color : LumiaTokens.ink.color)
                Text(caption).font(LumiaTokens.textMeta.font).foregroundStyle(apart ? LumiaTokens.toneAmberInk.color : LumiaTokens.inkMuted.color)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityHidden(true)
            Toggle(p.label, isOn: Binding(get: { draft.permissions.contains(p.name) }, set: { turn(p.name, on: $0) }))
                .labelsHidden().tint(LumiaTokens.accent.color)
                .disabled(off)
                .accessibilityHint(caption)
                .accessibilityIdentifier("permission-\(p.name)")
        }
        .padding(.vertical, LumiaTokens.space14)
        .padding(.horizontal, LumiaTokens.space16)
    }

    /// A switch turned: Administrator is asked about before it goes on.
    private func turn(_ permission: String, on: Bool) {
        if on, permission == Roles.words.administrator, !draft.permissions.contains(permission) {
            askingAdministrator = true
        } else if on {
            if !draft.permissions.contains(permission) { draft.permissions.append(permission) }
        } else {
            draft.permissions.removeAll { $0 == permission }
        }
    }

    // MARK: Who holds it, and Delete

    private func holders(_ role: RoleSummary) -> some View {
        let holding = holding
        return VStack(alignment: .leading, spacing: LumiaTokens.space10) {
            HStack(alignment: .firstTextBaseline, spacing: LumiaTokens.space6) {
                Text("Members with this role").font(RoleMetrics.heading.font).foregroundStyle(LumiaTokens.ink.color)
                    .accessibilityAddTraits(.isHeader)
                Text(Roles.grouped(holding.count)).font(LumiaTokens.textLinkSm.font).foregroundStyle(LumiaTokens.inkMuted.color)
            }
            Button { path.append(.roleMembers(serverID: serverID, roleID: role.roleId)) } label: { Label("Add members", systemImage: "plus") }
                .buttonStyle(.lumiaSecondary)
            if !holdersError.isEmpty { LumiaErrorText(holdersError) }
            if holding.isEmpty {
                RoleHint(Roles.words.nobodyHolds)
            } else {
                VStack(spacing: 0) {
                    ForEach(holding, id: \.accountId) { m in
                        let name = session.name(of: m.accountId)
                        HStack(spacing: LumiaTokens.space2) {
                            PersonLine(session: session, member: m)
                                .padding(.vertical, LumiaTokens.space6).padding(.leading, LumiaTokens.space10)
                            Button { Task { await remove(m, from: role) } } label: { Image(systemName: "person.badge.minus") }
                                .buttonStyle(MemberRemoveStyle())
                                .disabled(removing != nil)
                                .accessibilityLabel("Remove \(name) from \(role.name)")
                        }
                    }
                }
                .padding(LumiaTokens.space4)
                .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).strokeBorder(LumiaTokens.lineStrong.color))
            }
        }
    }

    private var delete: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space10) {
            Text("Delete role").font(RoleMetrics.heading.font).foregroundStyle(LumiaTokens.ink.color)
                .accessibilityAddTraits(.isHeader)
            RoleHint(Roles.words.deleteRoleHint)
            Button("Delete role") { askingDelete = true }.buttonStyle(.lumiaDanger)
                .accessibilityIdentifier("delete-role")
        }
    }

    /// Takes the role from one who holds it, at once.
    private func remove(_ member: MemberSummary, from role: RoleSummary) async {
        removing = member.accountId
        holdersError = ""
        defer { removing = nil }
        do {
            try await session.changeMember(serverID: serverID, accountID: member.accountId, take: [role.roleId])
        } catch {
            holdersError = Roles.memberRefusal(error)
        }
    }

    /// One Save: what the fields changed of the role since the editor opened
    /// on it, after which both are the role as the answer has it; or a new
    /// role, which is then the role this editor is of.
    private func save() async {
        guard savable, let server else { return }
        busy = true
        error = ""
        saved = false
        defer { busy = false }
        var sent = draft
        sent.name = draft.name.trimmingCharacters(in: .whitespacesAndNewlines)
        do {
            if let roleID {
                // Only ever a change of the role the editor opened on, never a second one made.
                guard let was = opened else { return }
                try await session.changeRole(serverID: serverID, roleID: roleID, was: was, now: sent)
                let now = self.role.map { rolesDraftOf(role: $0) } ?? sent
                opened = now
                draft = now
                saved = true
                AccessibilityNotification.Announcement("Saved").post()
            } else {
                try await session.createRole(serverID: serverID, draft: sent)
                // The role made is the one with that name, which is its alone whatever its capitals.
                let made = self.server?.roles.first { !$0.isEveryone && $0.name.lowercased() == sent.name.lowercased() }
                if let made, let at = path.lastIndex(of: .role(serverID: serverID, roleID: nil)) {
                    path[at] = .role(serverID: serverID, roleID: made.roleId)
                } else {
                    dismiss()
                }
            }
        } catch {
            self.error = Roles.roleRefusal(error, serverName: server.name)
        }
    }
}

// MARK: Add members

/// The members a role can be given to from its editor: those who do not
/// hold it, a guest never, several chosen at once.
struct RoleMembersScreen: View {
    @Environment(\.dismiss) private var dismiss
    let session: LumiaSession
    let serverID: String
    let roleID: String
    @State private var query = ""
    @State private var chosen: [String] = []
    @State private var busy = false
    @State private var error = ""

    private var server: ServerSummary? { session.snapshot?.servers.first { $0.serverId == serverID } }
    private var role: RoleSummary? { server?.roles.first { $0.roleId == roleID } }
    private var candidates: [MemberSummary] {
        Roles.candidates(session.serverMembers[serverID] ?? [], roleID: roleID, query: query) { session.name(of: $0.accountId) }
    }

    var body: some View {
        PanelScreen(back: role?.name ?? "the role", title: "Add members", subtitle: role?.name) {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space8) {
                    MemberSearchField(text: $query)
                    if !error.isEmpty { LumiaErrorText(error) }
                    let candidates = candidates
                    if candidates.isEmpty {
                        if query.trimmingCharacters(in: .whitespaces).isEmpty {
                            LumiaEmptyState(icon: "person.2", title: "No one to add") { Text(Roles.words.everyoneHolds) }
                        } else {
                            RoleHint(Roles.words.noMemberMatches).padding(.top, LumiaTokens.space4)
                        }
                    }
                    ForEach(candidates, id: \.accountId) { m in
                        LumiaCheckRow(checked: chosen.contains(m.accountId), toggle: { toggle(m.accountId) }) {
                            PersonLine(session: session, member: m)
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
    }

    private func toggle(_ id: String) {
        if let at = chosen.firstIndex(of: id) { chosen.remove(at: at) } else { chosen.append(id) }
    }

    /// The role given to each chosen member, one member at a time, whatever
    /// else each holds; a refusal leaves those not yet given still chosen.
    private func add() async {
        busy = true
        error = ""
        defer { busy = false }
        do {
            for id in chosen {
                try await session.changeMember(serverID: serverID, accountID: id, give: [roleID])
                chosen.removeAll { $0 == id }
            }
            dismiss()
        } catch {
            self.error = Roles.memberRefusal(error)
        }
    }
}

/// The search over a list of members: a glass and the words typed.
struct MemberSearchField: View {
    @Binding var text: String

    var body: some View {
        HStack(spacing: LumiaTokens.space8) {
            Image(systemName: "magnifyingglass").font(.system(size: RoleMetrics.glyph)).foregroundStyle(LumiaTokens.inkQuiet.color)
                .accessibilityHidden(true)
            TextField("Search members", text: $text)
                .textFieldStyle(.plain)
                .font(LumiaTokens.textBody.font)
                .textInputAutocapitalization(.never).autocorrectionDisabled()
                .accessibilityLabel("Search members")
        }
        .padding(.horizontal, LumiaTokens.space12)
        .frame(height: RoleMetrics.search)
        .background(LumiaTokens.field.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius12).stroke(LumiaTokens.borderForm.color))
    }
}

// MARK: Members

/// A server's members in its settings: a search over the list already
/// held, which asks the instance nothing, and each member's row, the owner
/// and a guest marked, the handle and title under the name and the roles
/// held as tags. A row opens to what this account may change of them.
struct ServerMembersScreen: View {
    let session: LumiaSession
    let serverID: String
    @State private var query = ""
    @State private var open: String?

    private var server: ServerSummary? { session.snapshot?.servers.first { $0.serverId == serverID } }
    private var members: [MemberSummary] { session.serverMembers[serverID] ?? [] }

    var body: some View {
        PanelScreen(back: settingsBack(server), title: "Members", subtitle: server.map { PhoneRoleWords.membersIn(members.count, serverName: $0.name) }) {
            ScrollView {
                if let server {
                    VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                        MemberSearchField(text: $query)
                        let list = Roles.searched(members, query: query)
                        if list.isEmpty {
                            if !query.trimmingCharacters(in: .whitespaces).isEmpty { RoleHint(Roles.words.noMemberMatches) }
                        } else {
                            VStack(spacing: 0) {
                                ForEach(Array(list.enumerated()), id: \.element.accountId) { index, m in
                                    if index > 0 { LumiaTokens.line.color.frame(height: 1) }
                                    row(m, server)
                                }
                            }
                            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).strokeBorder(LumiaTokens.lineStrong.color))
                            .accessibilityElement(children: .contain)
                            .accessibilityLabel("Members")
                        }
                    }
                    .padding(LumiaTokens.space16)
                }
            }
            .scrollDismissesKeyboard(.interactively)
        }
        .task {
            await session.refreshMembers(serverID: serverID)
            #if DEBUG
            // The smoke run's picture of a member's editing state (`LUMIA_DEV_PERSON`).
            if let handle = ProcessInfo.processInfo.environment["LUMIA_DEV_PERSON"] { open = members.first { $0.handle == handle }?.accountId }
            #endif
        }
    }

    @ViewBuilder private func row(_ m: MemberSummary, _ server: ServerSummary) -> some View {
        let me = m.accountId == session.me
        let offers = rolesMemberOffers(server: server, member: m, me: me)
        let isOpen = open == m.accountId && offers.any
        let held = rolesHeld(roles: server.roles, roleIds: m.roleIds)
        let name = session.name(of: m.accountId)
        let words = VStack(alignment: .leading, spacing: LumiaTokens.space4) {
            HStack(spacing: LumiaTokens.space12) {
                PersonLine(session: session, member: m, mark: rolesMemberMark(member: m), line: m.title.isEmpty ? "@\(m.handle)" : "@\(m.handle) · \(m.title)")
                if offers.any {
                    Image(systemName: isOpen ? "chevron.down" : "chevron.right").font(.system(size: RoleMetrics.smallGlyph, weight: .semibold))
                        .foregroundStyle(isOpen ? LumiaTokens.inkQuiet.color : LumiaTokens.inkFaint.color)
                }
            }
            if !held.isEmpty {
                LumiaFlow(spacing: LumiaTokens.space6) {
                    ForEach(held) { r in LumiaTag(r.look) }
                }
                .padding(.leading, LumiaTokens.avatarMd + LumiaTokens.space12)
            }
        }
        .padding(.vertical, LumiaTokens.space10)
        .padding(.horizontal, LumiaTokens.space14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        VStack(alignment: .leading, spacing: 0) {
            Group {
                // A row with nothing of theirs to change is its words, not a button that does nothing.
                if offers.any {
                    Button { open = isOpen ? nil : m.accountId } label: { words }.buttonStyle(.plain)
                } else {
                    words
                }
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(name)
            .accessibilityValue(heard(m, held: held, open: offers.any ? isOpen : nil))
            .accessibilityAddTraits(offers.any ? .isButton : [])
            .accessibilityIdentifier("member-\(m.handle)")
            if isOpen {
                MemberEditor(session: session, server: server, member: m, me: me) { open = nil }
                    .padding(.top, LumiaTokens.space6)
                    .padding(.horizontal, LumiaTokens.space14)
                    .padding(.bottom, LumiaTokens.space16)
                    // A new editor for another member: nothing typed for one is shown for the next.
                    .id(m.accountId)
            }
        }
    }

    /// A row as heard after the name: the mark, the handle, the title, the roles, and whether it is open.
    private func heard(_ m: MemberSummary, held: [HeldRole], open: Bool?) -> String {
        var parts: [String] = []
        if let mark = rolesMemberMark(member: m) { parts.append(mark) }
        parts.append("@\(m.handle)")
        if !m.title.isEmpty { parts.append(m.title) }
        parts += held.map(\.label)
        if let open { parts.append(open ? "expanded" : "collapsed") }
        return parts.joined(separator: ", ")
    }
}

/// A member's editing state under their row, and on their card: the roles
/// to tick, the title, and Kick, each where the core's answers offer it.
/// The form holds the member's roles and title as it opened (`opened`)
/// beside what is ticked and typed (`form`), and Save sends what differs in
/// one request: the roles to give, the roles to take, the title if it
/// changed, all of it taken or none. Nothing changed, nothing is sent. A
/// form nobody has touched follows the member as they change underneath
/// (the core's `rolesMemberFollowed`). Kick asks nothing first. A refusal is said over the buttons.
struct MemberEditor: View {
    let session: LumiaSession
    let server: ServerSummary
    let member: MemberSummary
    let me: Bool
    /// Which of the two an editor shows; both, under a member's row.
    var parts: Set<Part> = [.roles, .title]
    var kicks = true
    let close: () -> Void
    @State private var opened = MemberForm.empty
    @State private var form = MemberForm.empty
    @State private var loaded = false
    @State private var busy = false
    @State private var error = ""

    enum Part { case roles, title }

    private var offers: MemberOffers { rolesMemberOffers(server: server, member: member, me: me) }

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space16) {
            if parts.contains(.roles) {
                if offers.roles {
                    VStack(alignment: .leading, spacing: LumiaTokens.space6) {
                        // On a card the editor's own heading says Roles.
                        if parts.count > 1 { RoleLegend("Roles") }
                        RoleTicks(choices: rolesChoices(roles: server.roles, held: form.roleIds)) { form.roleIds = rolesToggled(roles: server.roles, held: form.roleIds, roleId: $0) }
                    }
                } else if let note = offers.rolesNote {
                    VStack(alignment: .leading, spacing: LumiaTokens.space6) {
                        RoleLegend("Roles")
                        RoleHint(note)
                    }
                }
            }
            if parts.contains(.title), offers.title {
                VStack(alignment: .leading, spacing: LumiaTokens.space6) {
                    LumiaField("Title", text: $form.title, placeholder: "Staff Engineer", spoken: "Title")
                    RoleHint(Roles.words.titleHint)
                }
            }
            if !error.isEmpty { LumiaErrorText(error).accessibilityIdentifier("member-error") }
            HStack(spacing: LumiaTokens.space8) {
                if kicks, offers.kick {
                    Button("Kick") { Task { await act { try await session.removeMember(serverID: server.serverId, accountID: member.accountId) } } }
                        .buttonStyle(KickButtonStyle())
                        .accessibilityLabel("Kick \(session.name(of: member.accountId))")
                        .accessibilityIdentifier("member-kick")
                }
                Spacer(minLength: 0)
                Button("Cancel", action: close).buttonStyle(.lumiaSecondary)
                Button("Save") { Task { await act(save) } }.buttonStyle(.lumiaPrimary)
                    .accessibilityIdentifier("member-save")
            }
            .disabled(busy)
        }
        .onAppear {
            guard !loaded else { return }
            opened = rolesFormOf(member: member)
            form = opened
            loaded = true
        }
        // The member changed underneath: a form nobody has touched follows them.
        .onChange(of: member) { _, now in
            guard loaded else { return }
            let next = rolesMemberFollowed(opened: opened, form: form, now: rolesFormOf(member: now))
            opened = next.opened
            form = next.form
        }
        .onChange(of: form.title) { _, now in
            if now.count > Roles.labelMax { form.title = String(now.prefix(Roles.labelMax)) }
        }
    }

    private func save() async throws {
        // What is not offered here was never ticked or typed, so it is as the form opened and is not sent.
        try await session.saveMember(serverID: server.serverId, accountID: member.accountId, opened: opened, form: form)
    }

    /// Does it and closes; a refusal is said here and the editor stays.
    private func act(_ what: () async throws -> Void) async {
        guard !busy else { return }
        busy = true
        error = ""
        defer { busy = false }
        do {
            try await what()
            close()
        } catch {
            self.error = Roles.memberRefusal(error)
        }
    }
}
