// Roles and titles (architecture 7.5). Every rule and every word is the
// core's (`lumia_client::roles`, held to `proto/vectors/roles/cases.json`),
// asked through lumia-ffi's `roles…` functions and `roleWords()`, whose
// records (`HeldRole`, `RoleChoice`, `MemberForm`, `MemberOffers`,
// `TagLook`, `SettingsSection`, `RoleWords`) the screens use as they come.
// What is here is the phone's own: the core's tone names as the phone's
// colours, counts as `Int`, the places the core's search answers read back
// into members, "Add members" in the order of names, an error read into the
// refusal's words, a move drawn before the instance answers, and the few
// things a phone says that the other clients do not.
import Foundation
import LumiaFFI

extension HeldRole: Identifiable {
    public var id: String { roleId }
}

extension RoleChoice: Identifiable {}

public extension SettingsSection {
    /// The section as an identifier names it: "overview", "roles", "members",
    /// whatever its row says (the core's `rolesSectionLabel`).
    var key: String {
        switch self {
        case .overview: "overview"
        case .roles: "roles"
        case .members: "members"
        }
    }
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

    /// How many of `members` hold each of `roles`, by role id, asked of the
    /// core once for the whole list (a call a role would cross the binding
    /// with every member each time); nil until the members are known.
    public static func holderCounts(_ roles: [RoleSummary], members: [MemberSummary]?) -> [String: Int]? {
        guard let members else { return nil }
        let counts = rolesHolderCounts(roles: roles, members: members)
        return Dictionary(zip(roles.map(\.roleId), counts.map { Int($0) }), uniquingKeysWith: { first, _ in first })
    }

    /// "1 member", "1,204 members".
    public static func membersCount(_ n: Int) -> String { rolesMembersCount(n: UInt64(max(0, n))) }

    /// A count with a comma between each three digits.
    public static func grouped(_ n: Int) -> String { rolesGrouped(n: UInt64(max(0, n))) }

    /// Members found by what is typed in a search: the core's places of
    /// them, asked once for the whole list, read back into the list; all of
    /// them for nothing typed, with nothing asked.
    public static func searched(_ members: [MemberSummary], query: String) -> [MemberSummary] {
        guard !query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return members }
        return rolesSearched(query: query, members: members).compactMap { at in
            let i = Int(at)
            return members.indices.contains(i) ? members[i] : nil
        }
    }

    /// Who may be given a role from its editor: those the core lets be
    /// offered (no guest, nobody who holds it; `rolesGivenTo`, one call for
    /// the whole list), found by `query`, in the order of their names.
    public static func candidates(_ members: [MemberSummary], roleID: String, query: String, name: (MemberSummary) -> String) -> [MemberSummary] {
        let found = searched(members, query: query)
        return rolesGivenTo(roleId: roleID, members: found)
            .compactMap { i in found.indices.contains(Int(i)) ? found[Int(i)] : nil }
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
        if hideTag { parts.append(Roles.words.notShownAsTag.lowercased()) }
        if locked { parts.append("not yours to change") }
        return parts.joined(separator: ", ")
    }
}
