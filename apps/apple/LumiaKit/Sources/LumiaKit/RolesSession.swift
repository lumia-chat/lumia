// The session's side of roles and titles (architecture 7.5). Every rule is
// the core's: the snapshot carries each server's roles in order with what
// this account may do to each, and a server's members carry their roles,
// title, tag and whether this account manages them. A change made here
// holds the snapshot the core answers with and reads the server's members
// again from what the core keeps (`keptMembers`, no request), so a tag
// beside a name follows it at once; one made elsewhere arrives as
// `tagsChanged`, after the `accountApplied` it came in, and is read the
// same way.
import Foundation
import LumiaFFI

public extension Roles {
    /// The permissions a role's editor offers, under their headings: the core's list, the same on every client.
    static let permissions: [PermissionGroup] = rolePermissions()
}

public extension LumiaSession {
    /// A member of a server as its list has them; nil for someone not in it.
    func member(_ accountID: String, in serverID: String) -> MemberSummary? {
        serverMembers[serverID]?.first { $0.accountId == accountID }
    }

    /// The tag beside someone's name in a server, their role or their
    /// title as the server shows, as the core worked it out; nil for none.
    func tag(of accountID: String, in serverID: String) -> TagLook? {
        Roles.tag(of: member(accountID, in: serverID)?.tag)
    }

    /// A server's members as the core keeps them now, with their tags. A
    /// server the core never listed is left as it is held.
    func rereadMembers(serverID: String) async {
        guard let kept = try? await client.keptMembers(serverID: serverID), !kept.isEmpty else { return }
        serverMembers[serverID] = kept
        for m in kept { people[m.accountId] = m }
    }

    /// A server's members asked of the instance again, as its settings
    /// open: who is in it now, so a role's count and the Members list start
    /// right. A failure leaves what is held.
    func refreshMembers(serverID: String) async {
        guard let list = try? await client.members(serverID: serverID) else { return }
        serverMembers[serverID] = list
        for m in list { people[m.accountId] = m }
    }

    /// A new role, placed last above Everyone.
    func createRole(serverID: String, draft: RoleDraft) async throws {
        take(try await client.createRole(serverID: serverID, name: draft.name, color: draft.color, hideTag: draft.hideTag, permissions: draft.permissions))
        await rereadMembers(serverID: serverID)
    }

    /// A role's name, colour, tag switch and permissions, whole; of Everyone only the permissions change.
    func updateRole(serverID: String, roleID: String, draft: RoleDraft) async throws {
        take(try await client.updateRole(serverID: serverID, roleID: roleID, name: draft.name, color: draft.color, hideTag: draft.hideTag, permissions: draft.permissions))
        await rereadMembers(serverID: serverID)
    }

    /// Deletes a role, from whoever held it too.
    func deleteRole(serverID: String, roleID: String) async throws {
        take(try await client.deleteRole(serverID: serverID, roleID: roleID))
        await rereadMembers(serverID: serverID)
    }

    /// The server's roles in this order: every role but Everyone, once, highest first.
    func reorderRoles(serverID: String, roleIDs: [String]) async throws {
        take(try await client.reorderRoles(serverID: serverID, roleIDs: roleIDs))
        await rereadMembers(serverID: serverID)
    }

    /// Whether the tag beside a name in the server is the member's title; false, their role.
    func setTagTitles(serverID: String, titles: Bool) async throws {
        take(try await client.setTagTitles(serverID: serverID, titles: titles))
        await rereadMembers(serverID: serverID)
    }

    /// The roles a member is to hold, the whole set.
    func setMemberRoles(serverID: String, accountID: String, roleIDs: [String]) async throws {
        _ = try await client.setMemberRoles(serverID: serverID, accountID: accountID, roleIDs: roleIDs)
        await memberChanged(serverID: serverID, accountID: accountID)
    }

    /// A member's title, up to 32 characters; empty clears it. This account's own is its own to set.
    func setMemberTitle(serverID: String, accountID: String, title: String) async throws {
        _ = try await client.setMemberTitle(serverID: serverID, accountID: accountID, title: title)
        await memberChanged(serverID: serverID, accountID: accountID)
    }

    /// A member's roles and title as an editor leaves them: each sent only
    /// where it changed, the roles first; nil for one the editor did not offer.
    func saveMember(serverID: String, member: MemberSummary, roleIDs: [String]?, title: String?) async throws {
        let changes = Roles.memberChanges(member, roleIDs: roleIDs, title: title)
        if let roles = changes.roleIDs { try await setMemberRoles(serverID: serverID, accountID: member.accountId, roleIDs: roles) }
        if let title = changes.title { try await setMemberTitle(serverID: serverID, accountID: member.accountId, title: title) }
    }

    /// Removes a member from the server.
    func removeMember(serverID: String, accountID: String) async throws {
        try await client.removeMember(serverID: serverID, accountID: accountID)
        serverMembers[serverID]?.removeAll { $0.accountId == accountID }
        await rereadMembers(serverID: serverID)
    }

    /// A member's roles or title changed here: the list as the core now
    /// keeps it, and for this account itself the snapshot, which carries
    /// its own roles and title and what they let it do.
    private func memberChanged(serverID: String, accountID: String) async {
        await rereadMembers(serverID: serverID)
        if accountID == me { await refreshSnapshot() }
    }
}
