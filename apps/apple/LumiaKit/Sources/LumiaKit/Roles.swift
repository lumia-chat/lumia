// Roles and titles (architecture 7.5), as values the tests cover: what a
// server's settings, a role's editor, a member's row and the profile card
// share before anything is drawn. The core says what this account may do
// (each role's and member's `canManage`, the server's `grantable` and its
// `canManage…` switches) and the instance decides every write; this reads
// those into what a screen offers, and holds the words every screen says
// the same way. The web's `client/roles.ts`, rule for rule and word for word.
import Foundation
import LumiaFFI

/// A section of a server's settings.
public enum SettingsSection: String, CaseIterable, Sendable {
    case overview, roles, members

    public var label: String {
        switch self {
        case .overview: "Overview"
        case .roles: "Roles"
        case .members: "Members"
        }
    }
}

/// What a role's editor holds while it is open.
public struct RoleDraft: Equatable, Sendable {
    public var name: String
    /// A tone, or empty for none.
    public var color: String
    public var hideTag: Bool
    /// The permissions switched on, by their wire names.
    public var permissions: [String]

    public init(name: String = "", color: String = "", hideTag: Bool = false, permissions: [String] = []) {
        self.name = name; self.color = color; self.hideTag = hideTag; self.permissions = permissions
    }
}

/// A role a member holds, as a card and a settings row list it: its name, on its tone or the plain chip.
public struct HeldRole: Equatable, Identifiable, Sendable {
    public let roleID: String
    public let label: String
    public let tone: AvatarTone?
    public var id: String { roleID }

    public init(roleID: String, label: String, tone: AvatarTone?) { self.roleID = roleID; self.label = label; self.tone = tone }
}

/// One of a member's roles to tick.
public struct RoleChoice: Equatable, Identifiable, Sendable {
    public let id: String
    public let name: String
    public let tone: AvatarTone?
    public let checked: Bool
    /// Not this account's to give or take: shown, not changed.
    public let locked: Bool
}

/// What this account may change of a member, on their row in Members and on their card.
public struct MemberOffers: Equatable, Sendable {
    /// Their roles to tick: with Manage roles, for anyone but a guest.
    public let roles: Bool
    /// In the roles' place, for a guest.
    public let rolesNote: String?
    /// Their title: this account's own, or that of a member below it with Manage members.
    public let title: Bool
    /// Kick: with Manage members, a member below this account, never itself.
    public let kick: Bool

    public init(roles: Bool, rolesNote: String?, title: Bool, kick: Bool) {
        self.roles = roles; self.rolesNote = rolesNote; self.title = title; self.kick = kick
    }

    /// Whether a member's row in Members opens at all: there is something of theirs to change.
    public var any: Bool { roles || rolesNote != nil || title || kick }
}

/// A member's tag as a message and a member row draw it: its words, on its tone or, for nil, the plain chip.
public struct TagLook: Equatable, Sendable {
    public let label: String
    public let tone: AvatarTone?

    public init(label: String, tone: AvatarTone?) { self.label = label; self.tone = tone }
}

public enum Roles {
    /// The nine tones a role may have, as the design system names them.
    public static let tones = AvatarTone.allCases

    /// The most characters a role's name or a title keeps.
    public static let labelMax = 32

    /// The permission whose switch asks before it is turned on.
    public static let administrator = "ADMINISTRATOR"

    /// A role's or a tag's colour as a tone; nil for none, and for anything that is no tone.
    public static func tone(of color: String) -> AvatarTone? { AvatarTone(rawValue: color) }

    /// The sections of a server's settings this account may use, in order:
    /// Overview with Manage server, Roles with Manage roles, and Members with
    /// Manage members or Manage roles, since a member's roles are changed there
    /// too. None, and the settings do not open.
    public static func sections(canManage: Bool, canManageRoles: Bool, canManageMembers: Bool) -> [SettingsSection] {
        var sections: [SettingsSection] = []
        if canManage { sections.append(.overview) }
        if canManageRoles { sections.append(.roles) }
        if canManageMembers || canManageRoles { sections.append(.members) }
        return sections
    }

    public static func sections(_ server: ServerSummary) -> [SettingsSection] {
        sections(canManage: server.canManage, canManageRoles: server.canManageRoles, canManageMembers: server.canManageMembers)
    }

    /// "5 roles": a server's roles counted without Everyone.
    public static func rolesCount(_ roles: [RoleSummary]) -> String {
        let n = roles.filter { !$0.isEveryone }.count
        return n == 1 ? "1 role" : "\(n) roles"
    }

    /// "1 member", "1,204 members".
    public static func membersCount(_ n: Int) -> String {
        n == 1 ? "1 member" : "\(grouped(n)) members"
    }

    /// How many of `members` hold a role; Everyone is held by all of them. Nil until the members are known.
    public static func holders(_ role: RoleSummary, members: [MemberSummary]?) -> Int? {
        guard let members else { return nil }
        return role.isEveryone ? members.count : members.filter { $0.roleIds.contains(role.roleId) }.count
    }

    /// A role at or above this account's own highest: listed with a lock, and it does not open. Everyone always opens.
    public static func locked(_ role: RoleSummary) -> Bool { !role.isEveryone && !role.canManage }

    /// Where a role may land when it is moved: before `beforeID`, a role this
    /// account manages, or before Everyone, which is last. A role is never put
    /// before one that is locked, since those are at or above this account's
    /// own highest, and landing where it already is moves nothing.
    public static func landsBefore(_ roles: [RoleSummary], roleID: String, beforeID: String) -> Bool {
        guard let from = roles.firstIndex(where: { $0.roleId == roleID }),
              let to = roles.firstIndex(where: { $0.roleId == beforeID }) else { return false }
        let moved = roles[from], before = roles[to]
        if locked(moved) || moved.isEveryone { return false }
        if to == from || to == from + 1 { return false }
        return before.isEveryone || !locked(before)
    }

    /// The whole order a move sends: every role but Everyone, with `roleID` now before `beforeID`.
    public static func movedBefore(_ roles: [RoleSummary], roleID: String, beforeID: String) -> [String] {
        var order: [String] = []
        for r in roles where r.roleId != roleID {
            if r.roleId == beforeID { order.append(roleID) }
            if !r.isEveryone { order.append(r.roleId) }
        }
        return order
    }

    /// The role a step up (-1) or down (1) puts `roleID` before; nil where there is no such step (the highest it may go, or last above Everyone).
    public static func stepBefore(_ roles: [RoleSummary], roleID: String, step: Int) -> String? {
        guard let at = roles.firstIndex(where: { $0.roleId == roleID }) else { return nil }
        let i = step == -1 ? at - 1 : at + 2
        guard roles.indices.contains(i), landsBefore(roles, roleID: roleID, beforeID: roles[i].roleId) else { return nil }
        return roles[i].roleId
    }

    /// `roles` as a move draws them at once, before the instance answers: `roleID` before `beforeID`.
    public static func drawnMoved(_ roles: [RoleSummary], roleID: String, beforeID: String) -> [RoleSummary] {
        guard let moved = roles.first(where: { $0.roleId == roleID }) else { return roles }
        var out: [RoleSummary] = []
        for r in roles where r.roleId != roleID {
            if r.roleId == beforeID { out.append(moved) }
            out.append(r)
        }
        return out
    }

    /// The editor's fields for a role, or for a new one: no name, no colour, shown as a tag, nothing granted.
    public static func draft(of role: RoleSummary? = nil) -> RoleDraft {
        role.map { RoleDraft(name: $0.name, color: $0.color, hideTag: $0.hideTag, permissions: $0.permissions) } ?? RoleDraft()
    }

    private static func sameSet(_ a: [String], _ b: [String]) -> Bool {
        a.count == b.count && a.allSatisfy(b.contains)
    }

    private static func trimmed(_ s: String) -> String { s.trimmingCharacters(in: .whitespacesAndNewlines) }

    /// Whether Save is offered: a new role once it has a name, a role once
    /// something of it differs and it still has one. Of Everyone only the
    /// permissions are compared, since nothing else of it changes.
    public static func savable(_ draft: RoleDraft, role: RoleSummary? = nil) -> Bool {
        guard let role else { return !trimmed(draft.name).isEmpty }
        if role.isEveryone { return !sameSet(draft.permissions, role.permissions) }
        if trimmed(draft.name).isEmpty { return false }
        return trimmed(draft.name) != role.name || draft.color != role.color || draft.hideTag != role.hideTag || !sameSet(draft.permissions, role.permissions)
    }

    /// A permission this account does not hold, and so cannot give or take: its switch is shown and not turned.
    public static func withheld(grantable: [String], permission: String) -> Bool { !grantable.contains(permission) }

    /// Every role a member holds, in the server's order, without Everyone.
    public static func held(_ roles: [RoleSummary], roleIDs: [String]) -> [HeldRole] {
        roles.filter { !$0.isEveryone && roleIDs.contains($0.roleId) }.map { HeldRole(roleID: $0.roleId, label: $0.name, tone: tone(of: $0.color)) }
    }

    /// A member's roles to tick: the server's in order without Everyone, each not this account's to give or take marked.
    public static func choices(_ roles: [RoleSummary], held: [String]) -> [RoleChoice] {
        roles.filter { !$0.isEveryone }.map { RoleChoice(id: $0.roleId, name: $0.name, tone: tone(of: $0.color), checked: held.contains($0.roleId), locked: !$0.canManage) }
    }

    /// The roles a tick leaves a member with; a locked role is not changed by one.
    public static func toggled(_ roles: [RoleSummary], held: [String], roleID: String) -> [String] {
        guard let role = roles.first(where: { $0.roleId == roleID }), !role.isEveryone, role.canManage else { return held }
        return held.contains(roleID) ? held.filter { $0 != roleID } : held + [roleID]
    }

    /// What this account may change of a member; `me` when the member is this account itself.
    public static func offers(canManageRoles: Bool, guest: Bool, canManage: Bool, me: Bool) -> MemberOffers {
        MemberOffers(
            roles: canManageRoles && !guest,
            rolesNote: canManageRoles && guest ? RoleWords.guestRolesNote : nil,
            title: me || canManage,
            kick: !me && canManage
        )
    }

    public static func offers(_ server: ServerSummary, member: MemberSummary, me: Bool) -> MemberOffers {
        offers(canManageRoles: server.canManageRoles, guest: member.guest, canManage: member.canManage, me: me)
    }

    /// The word beside a name on a card and in Members: the server's owner, or a guest.
    public static func mark(owner: Bool, guest: Bool) -> String? {
        owner ? "Owner" : guest ? "Guest" : nil
    }

    /// The words beside the name on a profile card, each a tag in the
    /// accent: "You" on this account's own, then who they are to the
    /// server. Both are shown, on every client: "You" does not stand in
    /// for "Owner" or "Guest".
    public static func cardMarks(me: Bool, owner: Bool, guest: Bool) -> [String] {
        (me ? ["You"] : []) + (mark(owner: owner, guest: guest).map { [$0] } ?? [])
    }

    /// A member's tag as a message and a member row draw it: its words, on its tone or the plain chip.
    public static func tag(of tag: TagSummary?) -> TagLook? {
        guard let tag, !tag.label.isEmpty else { return nil }
        return TagLook(label: tag.label, tone: tone(of: tag.color))
    }

    /// Members whose name, handle or title has `query` in it, whatever its capitals; all of them for nothing typed.
    public static func searched(_ members: [MemberSummary], query: String) -> [MemberSummary] {
        var q = trimmed(query).lowercased()
        if q.hasPrefix("@") { q.removeFirst() }
        guard !q.isEmpty else { return members }
        return members.filter { m in [m.displayName, m.nickname, m.handle, m.title].contains { $0.lowercased().contains(q) } }
    }

    /// A count with a comma between each three digits, as every client writes it.
    public static func grouped(_ n: Int) -> String { MessageCounter.grouped(UInt64(max(0, n))) }

    /// What a save of a member's roles and title sends: each only where it changed, the roles first.
    public static func memberChanges(_ member: MemberSummary, roleIDs: [String]?, title: String?) -> (roleIDs: [String]?, title: String?) {
        let roles = roleIDs.flatMap { sameSet($0, member.roleIds) ? nil : $0 }
        let named = title.map(trimmed).flatMap { $0 == member.title ? nil : $0 }
        return (roles, named)
    }

    /// Who may be given a role from its editor: members who do not hold it, a guest never, by `query`, by name.
    public static func candidates(_ members: [MemberSummary], roleID: String, query: String, name: (MemberSummary) -> String) -> [MemberSummary] {
        searched(members.filter { !$0.guest && !$0.roleIds.contains(roleID) }, query: query)
            .sorted { name($0).localizedCaseInsensitiveCompare(name($1)) == .orderedAscending }
    }
}

// MARK: Words

public enum RoleWords {
    public static let rolesHint = "In order, highest first. Drag a role to move it. A new role starts last, just above Everyone."
    /// The same where a role is moved from its menu alone (a phone, a narrow window).
    public static let rolesHintMenu = "In order, highest first. A role is moved from its menu. A new role starts last, just above Everyone."
    public static let everyoneSubtitle = "What every member may do before any role adds to it. Guests are not among them."
    public static let titleHint = "Up to 32 characters. A title grants nothing."
    public static let guestRolesNote = "Guests aren’t given roles."
    public static let deleteRoleHint = "Its members lose what it allowed. This can’t be undone."
    public static let tagNote = "A profile shows a member’s title and roles either way."
    /// Overview's card of the two: its title and the line under it.
    public static let tagTitle = "Tag beside names"
    public static let tagSubtitle = "What shows beside a member’s name in messages and in the member list."
    public static let tagRoles = (label: "Roles", caption: "Each member’s highest role that is shown as a tag, in the role’s colour.")
    public static let tagTitles = (label: "Titles", caption: "Each member’s title, in no colour. A member without a title has no tag.")

    /// What the web's screens say that the sheet does not hold: a role nobody
    /// holds, a search that finds nobody, a card with no roles to list.
    public static let nobodyHolds = "Nobody holds this role yet."
    public static let noMatch = "No member matches that."
    public static let noRoles = "No roles"
    /// The questions' titles and the gone role's line.
    public static let administratorTitle = "Turn on Administrator?"
    public static func deleteTitle(_ roleName: String) -> String { "Delete \(roleName)?" }

    /// Under Members' title: "128 in Acme".
    public static func membersIn(_ n: Int, serverName: String) -> String { "\(Roles.grouped(n)) in \(serverName)" }

    /// A move from a role's menu as assistive technology hears it, as the rail's moves are said: "Design moved to position 3".
    public static func moved(_ roleName: String, position: Int) -> String { "\(roleName) moved to position \(position)" }

    /// A role's row as heard after its name: "3 members", "41 members, not shown as tag", and for one not this account's to change, why.
    public static func rowValue(held: Int?, hideTag: Bool, locked: Bool) -> String {
        var parts: [String] = []
        if let held { parts.append(Roles.membersCount(held)) }
        if hideTag { parts.append("not shown as tag") }
        if locked { parts.append("not yours to change") }
        return parts.joined(separator: ", ")
    }

    public static func roleNameHint(serverName: String) -> String {
        "1 to 32 characters. No other role in \(serverName) can have the same name."
    }

    /// Under "Show as tag", as it stands.
    public static func tagSwitchCaption(shown: Bool) -> String {
        shown ? "This role can be the tag beside a member’s name." : "This role only carries permissions. It is nobody’s tag."
    }

    /// A permission's line where its switch is not this account's to turn.
    public static func withheldCaption(_ detail: String) -> String {
        "\(detail) You don’t hold it, so it is not yours to give or take."
    }

    /// What is asked before Administrator is turned on: the lines under "Turn on Administrator?". A new role has no holders to count.
    public static func administratorQuestion(serverName: String, roleName: String, held: Int?) -> [String] {
        var lines = ["Administrator allows everything in \(serverName), in every channel, whatever a channel’s own permissions say."]
        if let held, held > 0 { lines.append(held == 1 ? "1 member holds \(roleName)." : "\(Roles.grouped(held)) members hold \(roleName).") }
        return lines
    }

    /// What is asked before a role is deleted: the lines under "Delete
    /// Moderator?". How many hold it is said only when it is known: with
    /// the member list not read yet, or not read at all, nothing is said
    /// of holders, since "Nobody holds this role." of a role forty members
    /// hold is the one thing the question must not say.
    public static func deleteQuestion(held: Int?) -> [String] {
        let undone = "This can’t be undone."
        guard let held else { return [undone] }
        let first: String
        switch held {
        case 0: first = "Nobody holds this role."
        case 1: first = "1 member holds this role. They lose what it allowed."
        case let n: first = "\(Roles.grouped(n)) members hold this role. They lose what it allowed."
        }
        return [first, undone]
    }

    /// Why a role was not saved, moved or deleted: `code` and `details` as the error carries them, `message` its own words.
    public static func roleRefusal(code: String?, details: [String: String], message: String, serverName: String) -> String {
        switch code {
        case "PERMISSION_DENIED": "That isn’t yours to change. A role at or above your own highest, or a permission you don’t hold, is not yours to give."
        case "ALREADY_EXISTS": "Another role in \(serverName) has that name."
        case "NOT_FOUND": "That role is gone. Someone deleted it."
        case "INVALID_ARGUMENT" where details["field"] == "name": "A role’s name is 1 to 32 characters on one line, with no hidden characters, and can’t be “everyone”."
        case "INVALID_ARGUMENT" where details["field"] == "color": "A role’s colour is one of the nine, or none."
        case "INVALID_ARGUMENT" where details["field"] == "role_ids": "The roles changed while you were moving one. Try again."
        case "FAILED_PRECONDITION": "This server has as many roles as it can hold."
        default: message
        }
    }

    /// Why a member's roles or title were not saved, or they were not
    /// kicked. `RATE_LIMITED` is this account's own title, which a server
    /// lets it change ten times an hour; nothing else of a member is counted.
    public static func memberRefusal(code: String?, details: [String: String], message: String) -> String {
        switch code {
        case "PERMISSION_DENIED": "That isn’t yours to change. They may be at or above your own highest role."
        case "NOT_FOUND": "They are no longer a member."
        case "FAILED_PRECONDITION": "A member can’t hold that many roles."
        case "RATE_LIMITED": "You’ve changed your title too many times this hour. Try again later."
        case "INVALID_ARGUMENT" where details["field"] == "title": "A title is up to 32 characters on one line, with no hidden characters."
        default: message
        }
    }
}
