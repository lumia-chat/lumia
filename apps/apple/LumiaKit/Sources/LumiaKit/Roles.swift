// Roles and titles (architecture 7.5). Every rule and every word is the
// core's (`lumia_client::roles`, held to `proto/vectors/roles/cases.json`),
// asked through lumia-ffi's `roles…` functions and `roleWords()`, whose
// records (`HeldRole`, `RoleChoice`, `MemberForm`, `MemberOffers`,
// `TagLook`, `SettingsSection`, `RoleWords`) the screens use as they come.
// What is here is the phone's own: the core's tone names as the phone's
// colours, counts as `Int`, an error read into the refusal's words, a move
// drawn before the instance answers, and the few things a phone says that
// the other clients do not.
import Foundation
import LumiaFFI

extension HeldRole: Identifiable {
    public var id: String { roleId }
}

extension RoleChoice: Identifiable {}

public extension SettingsSection {
    /// The section's name, as its row says it.
    var label: String {
        switch self {
        case .overview: "Overview"
        case .roles: "Roles"
        case .members: "Members"
        }
    }

    /// The section as an identifier names it: "overview", "roles", "members".
    var key: String { label.lowercased() }
}

public extension MemberOffers {
    /// Whether a member's row in Members opens at all: there is something of theirs to change.
    var any: Bool { rolesOffersAny(offers: self) }
}

public extension TagLook {
    /// The tag's tone as the phone draws it; nil for the plain chip.
    var avatarTone: AvatarTone? { Roles.tone(tone) }
}

public extension RoleChoice {
    var avatarTone: AvatarTone? { Roles.tone(tone) }
}

public extension HeldRole {
    /// The role as a tag draws it.
    var look: TagLook { TagLook(label: label, tone: tone) }
}

public extension MemberForm {
    /// A form with nothing in it, before it opens on a member.
    static let empty = MemberForm(roleIds: [], title: "")
}

public enum Roles {
    /// Every fixed sentence the roles screens say, the permission asked
    /// about first and the most characters a name or a title keeps: the core's.
    public static let words = roleWords()

    /// The most characters a role's name or a title keeps.
    public static let labelMax = Int(words.labelMax)

    /// The nine tones a role may have, as the phone draws them.
    public static let tones = AvatarTone.allCases

    /// One of the core's tone names as the phone's colour; nil for none.
    public static func tone(_ name: String?) -> AvatarTone? { name.flatMap(AvatarTone.init(rawValue:)) }

    /// A role's colour as a tone; nil for none, and for anything that is no tone.
    public static func tone(of color: String) -> AvatarTone? { tone(rolesToneOf(color: color)) }

    /// How many of `members` hold a role; nil until the members are known.
    public static func holders(_ role: RoleSummary, members: [MemberSummary]?) -> Int? {
        rolesHolders(role: role, members: members).map { Int($0) }
    }

    /// "1 member", "1,204 members".
    public static func membersCount(_ n: Int) -> String { rolesMembersCount(n: UInt64(max(0, n))) }

    /// A count with a comma between each three digits.
    public static func grouped(_ n: Int) -> String { rolesGrouped(n: UInt64(max(0, n))) }

    /// Members found by what is typed in a search.
    public static func searched(_ members: [MemberSummary], query: String) -> [MemberSummary] {
        members.filter { rolesFound(query: query, member: $0) }
    }

    /// Who may be given a role from its editor: members who do not hold
    /// it, a guest never, found by `query`, in the order of their names.
    public static func candidates(_ members: [MemberSummary], roleID: String, query: String, name: (MemberSummary) -> String) -> [MemberSummary] {
        searched(members.filter { !$0.guest && !$0.roleIds.contains(roleID) }, query: query)
            .sorted { name($0).localizedCaseInsensitiveCompare(name($1)) == .orderedAscending }
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

    /// Whether an editor holds what it opened on, as typed: nothing typed,
    /// chosen or switched that is still there, a permission switched on and
    /// off again as it was. "Saved." stays said while it does.
    public static func untouched(opened: RoleDraft, draft: RoleDraft) -> Bool {
        draft.name == opened.name && draft.color == opened.color && draft.hideTag == opened.hideTag
            && draft.permissions.count == opened.permissions.count && draft.permissions.allSatisfy(opened.permissions.contains)
    }

    /// Why a role was not saved, moved or deleted, in the core's words.
    public static func roleRefusal(_ error: any Error, serverName: String) -> String {
        rolesRoleRefusal(code: error.lumiaCode ?? "", field: error.lumiaDetails["field"] ?? "", message: error.lumiaMessage, serverName: serverName)
    }

    /// Why a member's roles or title were not saved, or they were not kicked, in the core's words.
    public static func memberRefusal(_ error: any Error) -> String {
        rolesMemberRefusal(code: error.lumiaCode ?? "", field: error.lumiaDetails["field"] ?? "", message: error.lumiaMessage)
    }
}

/// What a phone says that the other clients' screens do not.
public enum PhoneRoleWords {
    /// Under Members' title: "128 in Acme".
    public static func membersIn(_ n: Int, serverName: String) -> String { "\(Roles.grouped(n)) in \(serverName)" }

    /// A role's row as heard after its name: "3 members", "41 members, not shown as tag", and for one not this account's to change, why.
    public static func rowValue(held: Int?, hideTag: Bool, locked: Bool) -> String {
        var parts: [String] = []
        if let held { parts.append(Roles.membersCount(held)) }
        if hideTag { parts.append("not shown as tag") }
        if locked { parts.append("not yours to change") }
        return parts.joined(separator: ", ")
    }
}
