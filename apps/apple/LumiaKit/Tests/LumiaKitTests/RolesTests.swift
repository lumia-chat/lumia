// Roles and titles (architecture 7.5): what the phone keeps of its own over
// the core's rules and words. The rules themselves are the core's, held to
// `proto/vectors/roles/cases.json` by its own tests; these hold the phone's
// adapters (its tones, its counts as `Int`, an error read into the core's
// refusal), what it draws on its own, and, against a running instance, the
// session's side.
import Foundation
@testable import LumiaKit
import LumiaFFI
import XCTest

final class RolesTests: XCTestCase {
    private func role(_ id: String, name: String? = nil, color: String = "", isEveryone: Bool = false, hideTag: Bool = false, permissions: [String] = [], canManage: Bool = true) -> RoleSummary {
        RoleSummary(roleId: id, name: name ?? id, color: color, position: 0, isEveryone: isEveryone, hideTag: hideTag, permissions: permissions, canManage: canManage)
    }

    private func member(_ handle: String = "m", name: String = "", title: String = "", roleIDs: [String] = []) -> MemberSummary {
        MemberSummary(accountId: handle, handle: handle, displayName: name, nickname: "", pronouns: "", bio: "", avatarUrl: "", channelAddsByInvitation: false, serverDmsOff: false, roleIds: roleIDs, title: title, guest: false, owner: false, tag: nil, canManage: false)
    }

    /// Acme as Devon sees it: Admin and Moderator are at or above Devon's highest.
    private var acme: [RoleSummary] {
        [
            role("admin", color: "indigo", canManage: false),
            role("mod", color: "green", canManage: false),
            role("design", color: "peach"),
            role("eng", color: "sky"),
            role("invite", hideTag: true),
            role("everyone", name: "Everyone", isEveryone: true),
        ]
    }

    // MARK: The phone's adapters

    // Every tone the core names is one the phone draws, and nothing else is.
    func testTheCoresTonesAreThePhonesColours() {
        for tone in AvatarTone.allCases {
            XCTAssertEqual(rolesToneOf(color: tone.rawValue), tone.rawValue)
            XCTAssertEqual(Roles.tone(of: tone.rawValue), tone)
        }
        XCTAssertEqual(Roles.tones.count, 9)
        XCTAssertNil(Roles.tone(of: ""))
        XCTAssertNil(Roles.tone(of: "red"))
        XCTAssertNil(Roles.tone(nil))
        XCTAssertEqual(rolesTagLook(tag: TagSummary(label: "Moderator", color: "green"))?.avatarTone, .green)
        XCTAssertNil(rolesTagLook(tag: TagSummary(label: "Staff Engineer", color: ""))?.avatarTone)
        XCTAssertEqual(rolesChoices(roles: acme, held: []).map(\.avatarTone), [.indigo, .green, .peach, .sky, nil])
        XCTAssertEqual(rolesHeld(roles: acme, roleIds: ["design", "invite"]).map(\.look), [TagLook(label: "design", tone: "peach"), TagLook(label: "invite", tone: nil)])
    }

    func testCountsAsThePhoneHoldsThem() {
        XCTAssertEqual(Roles.membersCount(1), "1 member")
        XCTAssertEqual(Roles.membersCount(1204), "1,204 members")
        XCTAssertEqual(Roles.grouped(1_234_567), "1,234,567")
        XCTAssertEqual(Roles.grouped(-3), "0")
        XCTAssertEqual(Roles.holders(acme[1], members: [member("a", roleIDs: ["mod"]), member("b")]), 1)
        XCTAssertNil(Roles.holders(acme[1], members: nil))
        XCTAssertEqual(Roles.labelMax, 32)
        XCTAssertEqual(Roles.words.administrator, "ADMINISTRATOR")
    }

    // Every role's holders in one call, read back by role id; none while the members are unknown.
    func testHolderCountsAreOneCallReadBackByRole() {
        let people = [member("a", roleIDs: ["mod", "eng"]), member("b", roleIDs: ["eng"]), member("c")]
        let counts = Roles.holderCounts(acme, members: people)
        XCTAssertEqual(counts?["mod"], 1)
        XCTAssertEqual(counts?["eng"], 2)
        XCTAssertEqual(counts?["admin"], 0)
        XCTAssertEqual(counts?["everyone"], 3)
        XCTAssertEqual(counts?.count, acme.count)
        XCTAssertNil(Roles.holderCounts(acme, members: nil))
    }

    // An identifier's key is the phone's, whatever the row says.
    func testASectionIsIdentified() {
        XCTAssertEqual([SettingsSection.overview, .roles, .members].map(\.key), ["overview", "roles", "members"])
    }

    func testWhatIsOfferedOpensARowOnlyWhereThereIsSomething() {
        XCTAssertFalse(MemberOffers(roles: false, rolesNote: nil, title: false, kick: false).any)
        XCTAssertTrue(MemberOffers(roles: false, rolesNote: "Guests aren’t given roles.", title: false, kick: false).any)
        XCTAssertTrue(MemberOffers(roles: false, rolesNote: nil, title: false, kick: true).any)
    }

    // The instance's code and `details.field` reach the core's words; an
    // error that is no API error is said in its own.
    func testARefusalIsReadFromTheError() {
        func api(_ code: String, field: String? = nil, message: String = "") -> any Error {
            LumiaError.Api(code: code, detail: message, details: field.map { ["field": $0] } ?? [:])
        }
        XCTAssertEqual(Roles.roleRefusal(api("ALREADY_EXISTS"), serverName: "Acme"), "Another role in Acme has that name.")
        XCTAssertEqual(Roles.roleRefusal(api("INVALID_ARGUMENT", field: "color"), serverName: "Acme"), "A role’s colour is one of the nine, or none.")
        XCTAssertEqual(Roles.roleRefusal(api("INVALID_ARGUMENT", field: "other", message: "bad"), serverName: "Acme"), "bad")
        XCTAssertEqual(Roles.roleRefusal(LumiaError.NotSignedIn, serverName: "Acme"), "Not signed in.")
        XCTAssertEqual(Roles.memberRefusal(api("INVALID_ARGUMENT", field: "title")), "A title is up to 32 characters on one line, with no hidden characters.")
        XCTAssertEqual(Roles.memberRefusal(api("RATE_LIMITED", message: "too many requests")), "You’ve changed your title too many times this hour. Try again later.")
        XCTAssertEqual(Roles.memberRefusal(LumiaError.NotSignedIn), "Not signed in.")
    }

    // MARK: What the phone draws on its own

    func testAMoveIsDrawnAtOnceWithEveryoneStillLast() {
        XCTAssertEqual(Roles.drawnMoved(acme, roleID: "eng", beforeID: "design").map(\.roleId), ["admin", "mod", "eng", "design", "invite", "everyone"])
        XCTAssertEqual(Roles.drawnMoved(acme, roleID: "design", beforeID: "everyone").map(\.roleId), ["admin", "mod", "eng", "invite", "design", "everyone"])
        XCTAssertEqual(Roles.drawnMoved(acme, roleID: "gone", beforeID: "design").map(\.roleId), acme.map(\.roleId))
        // What is drawn is what the core sends.
        let order = rolesMovedBefore(roles: acme, roleId: "eng", beforeId: "design")
        XCTAssertEqual(Roles.drawnMoved(acme, roleID: "eng", beforeID: "design").filter { !$0.isEveryone }.map(\.roleId), order)
    }

    // A member list that could not be read is not an empty one: who holds
    // a role stays unknown, or as it was last known, and "Delete
    // Moderator?" does not say nobody holds a role forty members hold.
    func testAMemberListThatCouldNotBeReadIsNotAnEmptyOne() {
        let forty = (0..<40).map { member("m\($0)", roleIDs: ["mod"]) }
        func asked(_ list: [MemberSummary]?) -> [String] { rolesDeleteQuestion(held: rolesHolders(role: acme[1], members: list)) }
        // Never read, and the read failed: no list, so no count.
        var lists = MemberLists.after(held: [:], serverIDs: ["acme"], read: [:])
        XCTAssertNil(lists["acme"])
        XCTAssertNil(Roles.holders(acme[1], members: lists["acme"]))
        XCTAssertEqual(asked(lists["acme"]), ["This can’t be undone."])
        // Read: counted.
        lists = MemberLists.after(held: lists, serverIDs: ["acme"], read: ["acme": forty])
        XCTAssertEqual(Roles.holders(acme[1], members: lists["acme"]), 40)
        // Asked again and not read: what was held stays.
        lists = MemberLists.after(held: lists, serverIDs: ["acme", "trail"], read: [:])
        XCTAssertEqual(lists["acme"]?.count, 40)
        XCTAssertNil(lists["trail"])
        XCTAssertEqual(asked(lists["acme"])[0], "40 members hold this role. They lose what it allowed.")
        // A server left goes, whatever was read of it.
        XCTAssertEqual(MemberLists.after(held: lists, serverIDs: ["trail"], read: ["acme": forty, "trail": []]), ["trail": []])
    }

    // The core's places of those found, read back into the members, in their order.
    func testASearchIsReadBackIntoTheMembers() {
        let people = [member("sam", name: "Sam Okafor"), member("devon", name: "Devon Park"), member("maya", name: "Maya Chen")]
        XCTAssertEqual(Roles.searched(people, query: "").map(\.handle), ["sam", "devon", "maya"])
        XCTAssertEqual(Roles.searched(people, query: "a").map(\.handle), ["sam", "devon", "maya"])
        XCTAssertEqual(Roles.searched(people, query: "chen").map(\.handle), ["maya"])
        XCTAssertEqual(Roles.searched([], query: "chen"), [])
    }

    // "Add members": those the core lets be offered and its search finds, by name.
    func testAddMembersIsInTheOrderOfNames() {
        let people = [member("sam", name: "Sam Okafor"), member("devon", name: "Devon Park", roleIDs: ["mod"]), member("maya", name: "Maya Chen")]
        XCTAssertEqual(Roles.candidates(people, roleID: "eng", query: "", name: \.displayName).map(\.handle), ["devon", "maya", "sam"])
        XCTAssertEqual(Roles.candidates(people, roleID: "mod", query: "", name: \.displayName).map(\.handle), ["maya", "sam"])
        XCTAssertEqual(Roles.candidates(people, roleID: "eng", query: "o", name: \.displayName).map(\.handle), ["devon", "sam"])
    }

    func testThePhonesOwnWords() {
        XCTAssertEqual(PhoneRoleWords.membersIn(128, serverName: "Acme"), "128 in Acme")
        XCTAssertEqual(PhoneRoleWords.membersIn(1204, serverName: "Acme"), "1,204 in Acme")
        XCTAssertEqual(PhoneRoleWords.rowValue(held: 41, hideTag: true, locked: false), "41 members, not shown as tag")
        XCTAssertEqual(PhoneRoleWords.rowValue(held: 2, hideTag: false, locked: true), "2 members, not yours to change")
        XCTAssertEqual(PhoneRoleWords.rowValue(held: nil, hideTag: false, locked: false), "")
    }

    // MARK: A tag beside a name

    func testATagGivesWayBeforeTheNameDoes() {
        // Room for both: each as wide as it is.
        XCTAssertEqual(NameWithTag.fitted([80, 60], tag: 1, width: 200, gap: 6), [80, 60])
        // A long tag beside a short name: the name whole, the tag what is left.
        XCTAssertEqual(NameWithTag.fitted([60, 200], tag: 1, width: 206, gap: 6), [60, 140])
        // Both long: the tag down to a few letters (52 with the gap before it), and only then the name cut.
        XCTAssertEqual(NameWithTag.fitted([200, 200], tag: 1, width: 206, gap: 6), [154, 46])
        XCTAssertEqual(NameWithTag.fitted([150, 200], tag: 1, width: 206, gap: 6), [150, 50])
        // A tag shorter than that is drawn whole, and the name has the rest.
        XCTAssertEqual(NameWithTag.fitted([300, 30], tag: 1, width: 206, gap: 6), [170, 30])
        // A mark between them ("You") is never cut.
        XCTAssertEqual(NameWithTag.fitted([200, 30, 200], tag: 2, width: 212, gap: 6), [124, 30, 46])
        // No tag: the name is cut at the line's end.
        XCTAssertEqual(NameWithTag.fitted([300], tag: nil, width: 200, gap: 6), [200])
        XCTAssertEqual(NameWithTag.fitted([300, 30], tag: nil, width: 200, gap: 6), [164, 30])
    }

    // MARK: Against a running instance

    // Against a running instance with open registration
    // (`LUMIA_TEST_INSTANCE`, as in `LumiaKitTests`; skipped without it):
    // what the instance refuses of a title and of a role's name comes
    // through the core with its code and its field, and is said in the
    // sheet's words. A title or a name with a character nobody can see in
    // it, a title on two lines, and this account's own title changed an
    // eleventh time in the hour.
    func testWhatTheInstanceRefusesOfATitleAndANameIsSaidInTheSheetsWords() async throws {
        guard let instance = ProcessInfo.processInfo.environment["LUMIA_TEST_INSTANCE"], let base = URL(string: instance) else {
            throw XCTSkip("LUMIA_TEST_INSTANCE names no instance")
        }
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let client = try LumiaClient(baseURL: base, dataDirectory: dir)
        try await client.createAccount(handle: "titled\(UInt64(Date().timeIntervalSince1970 * 1000))", password: "a password for the kit's test")
        let server = try await client.createServer(name: "Acme")
        let me = try await client.snapshot().accountId
        func said(_ error: any Error) -> String {
            Roles.memberRefusal(error)
        }
        func refused(_ title: String) async -> String? {
            do {
                _ = try await client.changeMember(serverID: server.serverId, accountID: me, title: title)
                return nil
            } catch { return said(error) }
        }

        // Hidden characters and a second line: refused, and nothing counted.
        let hidden = "A title is up to 32 characters on one line, with no hidden characters."
        for title in ["Foun\u{200B}der", "Founder\u{200E}!", "Foun\u{2028}der", "Foun\nder", "\u{202E}Founder"] {
            let words = await refused(title)
            XCTAssertEqual(words, hidden, "for \(title.unicodeScalars.map { String($0.value, radix: 16) })")
        }
        do {
            _ = try await client.createRole(serverID: server.serverId, name: "Mod\u{200B}erator", color: "", hideTag: false, permissions: [])
            XCTFail("a role's name with a hidden character was taken")
        } catch {
            XCTAssertEqual(
                Roles.roleRefusal(error, serverName: "Acme"),
                "A role’s name is 1 to 32 characters on one line, with no hidden characters, and can’t be “everyone”."
            )
        }

        // Ten changes of one's own title in the hour are taken, the eleventh is not.
        for n in 1...10 {
            let words = await refused("Founder \(n)")
            XCTAssertNil(words, "change \(n) was refused")
        }
        let eleventh = await refused("Founder 11")
        XCTAssertEqual(eleventh, "You’ve changed your title too many times this hour. Try again later.")
        let kept = try await client.members(serverID: server.serverId).first { $0.accountId == me }?.title
        XCTAssertEqual(kept, "Founder 10")
    }

    /// A request of the test's own to the instance, as another session of
    /// an account makes it: the status, and the answer read as JSON.
    private func elsewhere(_ instance: String, _ method: String, _ path: String, token: String? = nil, body: [String: Any]? = nil) async throws -> (status: Int, json: [String: Any]) {
        var request = URLRequest(url: URL(string: "\(instance)/api/v1\(path)")!)
        request.httpMethod = method
        request.setValue("application/json", forHTTPHeaderField: "accept")
        if let token { request.setValue("Bearer \(token)", forHTTPHeaderField: "authorization") }
        if let body {
            request.setValue("application/json", forHTTPHeaderField: "content-type")
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
        }
        let (data, response) = try await URLSession.shared.data(for: request)
        return ((response as? HTTPURLResponse)?.statusCode ?? 0, (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:])
    }

    /// A session of the test's own on an account, beside the phone's.
    private func signedInBeside(_ instance: String, handle: String, password: String) async throws -> String {
        let pair = try await elsewhere(instance, "POST", "/auth/sign-in/password", body: ["handleOrEmail": handle, "password": password]).json
        return try XCTUnwrap(pair["accessToken"] as? String)
    }

    // Against a running instance (`LUMIA_TEST_INSTANCE`): a role's editor is
    // open and its name typed over; another session of the account switches
    // one of its permissions off; the editor's rename is saved. The rename
    // is taken and the permission is still off: the save sent what the
    // editor changed, not the role as the editor first saw it.
    @MainActor
    func testARenameFromAnEditorLeavesWhatSomeoneElseSwitchedOff() async throws {
        guard let instance = ProcessInfo.processInfo.environment["LUMIA_TEST_INSTANCE"], let base = URL(string: instance) else {
            throw XCTSkip("LUMIA_TEST_INSTANCE names no instance")
        }
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let (handle, password) = ("editor\(UInt64(Date().timeIntervalSince1970 * 1000))", "a password for the kit's test")
        let session = await LumiaSession(client: try LumiaClient(baseURL: base, dataDirectory: dir))
        try await session.createAccount(handle: handle, password: password)
        let serverID = try await session.client.createServer(name: "Acme").serverId
        await session.refreshSnapshot()
        try await session.createRole(serverID: serverID, draft: RoleDraft(name: "Support", color: "sky", hideTag: false, permissions: ["MANAGE_MESSAGES", "MENTION_EVERYONE"]))
        func role() -> RoleSummary? { session.snapshot?.servers.first { $0.serverId == serverID }?.roles.first { ["Support", "Helpers"].contains($0.name) } }
        let made = try XCTUnwrap(role())
        XCTAssertEqual(made.permissions.sorted(), ["MANAGE_MESSAGES", "MENTION_EVERYONE"])

        // The editor opens on the role, and its name is typed over.
        var opened = rolesDraftOf(role: made)
        var draft = opened
        draft.name = "Helpers"

        // Elsewhere, Mention everyone is switched off.
        let token = try await signedInBeside(instance, handle: handle, password: password)
        let patched = try await elsewhere(instance, "PATCH", "/servers/\(serverID)/roles/\(made.roleId)", token: token, body: ["revoke": ["MENTION_EVERYONE"]])
        XCTAssertEqual(patched.status, 200, "\(patched.json)")
        await session.refreshSnapshot()
        let changed = try XCTUnwrap(role())
        XCTAssertEqual(changed.permissions, ["MANAGE_MESSAGES"])
        XCTAssertEqual(changed.name, "Support")

        // The editor was touched, so it does not follow, and Save is its rename.
        do { let next = rolesFollowed(opened: opened, draft: draft, now: rolesDraftOf(role: changed)); (opened, draft) = (next.opened, next.draft) }
        XCTAssertEqual(opened.permissions.sorted(), ["MANAGE_MESSAGES", "MENTION_EVERYONE"])
        XCTAssertTrue(rolesSavable(draft: draft, opened: opened))
        try await session.changeRole(serverID: serverID, roleID: made.roleId, was: opened, now: draft)
        let saved = try XCTUnwrap(role())
        XCTAssertEqual(saved.name, "Helpers")
        XCTAssertEqual(saved.permissions, ["MANAGE_MESSAGES"], "the rename switched back on what someone else had switched off")
        // As the instance itself lists it, not only as this client holds it.
        let listed = try await elsewhere(instance, "GET", "/servers/\(serverID)/roles", token: token).json
        let there = try XCTUnwrap((listed["roles"] as? [[String: Any]])?.first { $0["roleId"] as? String == made.roleId })
        XCTAssertEqual(there["name"] as? String, "Helpers")
        XCTAssertEqual(there["permissions"] as? [String], ["MANAGE_MESSAGES"])

        // After its save the editor holds the role as answered, with nothing
        // to save; an untouched one follows the next change from elsewhere.
        opened = rolesDraftOf(role: saved)
        draft = opened
        XCTAssertFalse(rolesSavable(draft: draft, opened: opened))
        let recoloured = try await elsewhere(instance, "PATCH", "/servers/\(serverID)/roles/\(made.roleId)", token: token, body: ["color": "mint"])
        XCTAssertEqual(recoloured.status, 200, "\(recoloured.json)")
        await session.refreshSnapshot()
        do { let next = rolesFollowed(opened: opened, draft: draft, now: rolesDraftOf(role: try XCTUnwrap(role()))); (opened, draft) = (next.opened, next.draft) }
        XCTAssertEqual(draft.color, "mint")
        XCTAssertFalse(rolesSavable(draft: draft, opened: opened))
        // Saved as it opened, it asks nothing and changes nothing.
        try await session.changeRole(serverID: serverID, roleID: made.roleId, was: opened, now: draft)
        XCTAssertEqual(role()?.color, "mint")
    }

    // Against a running instance: a member's form is open and one of their
    // roles unticked; elsewhere they are given another role; the form is
    // saved. They keep the role given elsewhere: the save took the one
    // role, it did not write the set the form first saw. And a save whose
    // title is refused gives no role: it is one request, all of it or none.
    @MainActor
    func testARoleGivenElsewhereStaysWhenAnotherIsTakenInAFormOpenedBefore() async throws {
        guard let instance = ProcessInfo.processInfo.environment["LUMIA_TEST_INSTANCE"], let base = URL(string: instance) else {
            throw XCTSkip("LUMIA_TEST_INSTANCE names no instance")
        }
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let stamp = UInt64(Date().timeIntervalSince1970 * 1000)
        let (handle, password) = ("owner\(stamp)", "a password for the kit's test")
        let session = await LumiaSession(client: try LumiaClient(baseURL: base, dataDirectory: dir.appendingPathComponent("owner")))
        try await session.createAccount(handle: handle, password: password)
        let serverID = try await session.client.createServer(name: "Acme").serverId
        await session.refreshSnapshot()
        for name in ["Design", "Engineering", "Support"] {
            try await session.createRole(serverID: serverID, draft: RoleDraft(name: name, color: "", hideTag: false, permissions: []))
        }
        let roles = session.snapshot?.servers.first { $0.serverId == serverID }?.roles ?? []
        let id = { (name: String) in try XCTUnwrap(roles.first { $0.name == name }?.roleId) }
        let (design, engineering, support) = (try id("Design"), try id("Engineering"), try id("Support"))

        // Someone joins, and holds Engineering.
        let invite = try await session.client.createInvite(serverID: serverID)
        let other = try LumiaClient(baseURL: base, dataDirectory: dir.appendingPathComponent("member"))
        try await other.createAccount(handle: "member\(stamp)", password: password, invite: invite.url)
        let them = try await other.snapshot().accountId
        try await session.changeMember(serverID: serverID, accountID: them, give: [engineering])
        await session.refreshMembers(serverID: serverID)
        func member() -> MemberSummary? { session.member(them, in: serverID) }
        XCTAssertEqual(member()?.roleIds, [engineering])

        // Their form opens, and Engineering is unticked.
        var opened = rolesFormOf(member: try XCTUnwrap(member()))
        var form = opened
        form.roleIds = rolesToggled(roles: roles, held: form.roleIds, roleId: engineering)
        XCTAssertEqual(form.roleIds, [])

        // Elsewhere they are given Design, and the phone hears of it.
        let token = try await signedInBeside(instance, handle: handle, password: password)
        let given = try await elsewhere(instance, "PATCH", "/servers/\(serverID)/members/\(them)", token: token, body: ["addRoleIds": [design]])
        XCTAssertEqual(given.status, 200, "\(given.json)")
        await session.refreshMembers(serverID: serverID)
        XCTAssertEqual(Set(member()?.roleIds ?? []), [design, engineering])
        do { let next = rolesMemberFollowed(opened: opened, form: form, now: rolesFormOf(member: try XCTUnwrap(member()))); (opened, form) = (next.opened, next.form) }
        XCTAssertEqual(opened.roleIds, [engineering])

        // Saved: Engineering taken, Design still theirs, here and on the instance.
        try await session.saveMember(serverID: serverID, accountID: them, opened: opened, form: form)
        XCTAssertEqual(member()?.roleIds, [design], "the save wrote the roles the form first saw over the one given elsewhere")
        let listed = try await session.client.members(serverID: serverID).first { $0.accountId == them }
        XCTAssertEqual(listed?.roleIds, [design])

        // One request, whole or not at all: a title refused gives no role with it.
        opened = rolesFormOf(member: try XCTUnwrap(member()))
        form = MemberForm(roleIds: opened.roleIds + [support], title: "Sup\u{200B}port lead")
        do {
            try await session.saveMember(serverID: serverID, accountID: them, opened: opened, form: form)
            XCTFail("a title with a hidden character was taken")
        } catch {
            XCTAssertEqual(Roles.memberRefusal(error), "A title is up to 32 characters on one line, with no hidden characters.")
        }
        let after = try await session.client.members(serverID: serverID).first { $0.accountId == them }
        XCTAssertEqual(after?.roleIds, [design], "a role was given though the title sent with it was refused")
        XCTAssertEqual(after?.title, "")
        // A role's holders, from its editor: given and taken one role at a time.
        try await session.changeMember(serverID: serverID, accountID: them, give: [support])
        try await session.changeMember(serverID: serverID, accountID: them, take: [design])
        XCTAssertEqual(member()?.roleIds, [support])
        // Nothing changed, nothing asked: it would be refused were it sent for someone gone.
        try await session.saveMember(serverID: serverID, accountID: "01890000-0000-7000-8000-00000000dead", opened: opened, form: opened)
    }
}
