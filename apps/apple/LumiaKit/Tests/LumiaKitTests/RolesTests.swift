// Roles and titles (architecture 7.5): the rules and words every client
// shares, with the cases the web's `roles.test.ts` has.
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

    // MARK: Sections

    func testEachPermissionOpensItsOwnAndManageRolesOpensMembersToo() {
        XCTAssertEqual(Roles.sections(canManage: true, canManageRoles: true, canManageMembers: true), [.overview, .roles, .members])
        XCTAssertEqual(Roles.sections(canManage: true, canManageRoles: false, canManageMembers: false), [.overview])
        XCTAssertEqual(Roles.sections(canManage: false, canManageRoles: true, canManageMembers: false), [.roles, .members])
        XCTAssertEqual(Roles.sections(canManage: false, canManageRoles: false, canManageMembers: true), [.members])
        XCTAssertEqual(Roles.sections(canManage: false, canManageRoles: false, canManageMembers: false), [])
    }

    // MARK: The list

    func testCountsRolesWithoutEveryone() {
        XCTAssertEqual(Roles.rolesCount(acme), "5 roles")
        XCTAssertEqual(Roles.rolesCount([role("a"), role("everyone", isEveryone: true)]), "1 role")
    }

    func testCountsMembersWithTheirThousandsGrouped() {
        XCTAssertEqual(Roles.membersCount(1), "1 member")
        XCTAssertEqual(Roles.membersCount(0), "0 members")
        XCTAssertEqual(Roles.membersCount(128), "128 members")
        XCTAssertEqual(Roles.membersCount(1204), "1,204 members")
    }

    func testCountsWhoHoldsARoleAndEveryoneHoldsEveryone() {
        let members = [member(roleIDs: ["eng"]), member(roleIDs: ["eng", "design"]), member()]
        XCTAssertEqual(Roles.holders(acme[3], members: members), 2)
        XCTAssertEqual(Roles.holders(acme[5], members: members), 3)
        XCTAssertNil(Roles.holders(acme[3], members: nil))
    }

    func testARoleNotManagedIsLockedAndEveryoneNeverIs() {
        XCTAssertTrue(Roles.locked(acme[0]))
        XCTAssertFalse(Roles.locked(acme[2]))
        XCTAssertFalse(Roles.locked(role("everyone", isEveryone: true, canManage: false)))
    }

    func testAToneIsOneOfTheNine() {
        XCTAssertEqual(Roles.tone(of: "sky"), .sky)
        XCTAssertNil(Roles.tone(of: ""))
        XCTAssertNil(Roles.tone(of: "#ff0000"))
        XCTAssertEqual(Roles.tones.map(\.rawValue), ["peach", "blue", "purple", "mint", "amber", "sky", "stone", "green", "indigo"])
    }

    // MARK: Moving a role

    func testLandsBeforeARoleItManagesOrBeforeEveryone() {
        XCTAssertTrue(Roles.landsBefore(acme, roleID: "eng", beforeID: "design"))
        XCTAssertTrue(Roles.landsBefore(acme, roleID: "design", beforeID: "everyone"))
        XCTAssertTrue(Roles.landsBefore(acme, roleID: "design", beforeID: "invite"))
    }

    func testNeverBeforeALockedRoleNeverWhereItIsAndALockedRoleAndEveryoneDoNotMove() {
        XCTAssertFalse(Roles.landsBefore(acme, roleID: "eng", beforeID: "mod"))
        XCTAssertFalse(Roles.landsBefore(acme, roleID: "eng", beforeID: "admin"))
        XCTAssertFalse(Roles.landsBefore(acme, roleID: "eng", beforeID: "eng"))
        XCTAssertFalse(Roles.landsBefore(acme, roleID: "eng", beforeID: "invite"))
        XCTAssertFalse(Roles.landsBefore(acme, roleID: "mod", beforeID: "everyone"))
        XCTAssertFalse(Roles.landsBefore(acme, roleID: "everyone", beforeID: "design"))
        XCTAssertFalse(Roles.landsBefore(acme, roleID: "gone", beforeID: "design"))
    }

    func testTheOrderSentIsEveryRoleButEveryone() {
        XCTAssertEqual(Roles.movedBefore(acme, roleID: "eng", beforeID: "design"), ["admin", "mod", "eng", "design", "invite"])
        XCTAssertEqual(Roles.movedBefore(acme, roleID: "design", beforeID: "everyone"), ["admin", "mod", "eng", "invite", "design"])
    }

    func testAStepUpOrDownWhereThereIsOne() {
        XCTAssertEqual(Roles.stepBefore(acme, roleID: "eng", step: -1), "design")
        XCTAssertEqual(Roles.stepBefore(acme, roleID: "eng", step: 1), "everyone")
        XCTAssertEqual(Roles.stepBefore(acme, roleID: "design", step: 1), "invite")
        // Design is the highest Devon manages, and Can invite the last above Everyone.
        XCTAssertNil(Roles.stepBefore(acme, roleID: "design", step: -1))
        XCTAssertNil(Roles.stepBefore(acme, roleID: "invite", step: 1))
        XCTAssertNil(Roles.stepBefore(acme, roleID: "admin", step: 1))
        XCTAssertNil(Roles.stepBefore(acme, roleID: "gone", step: 1))
    }

    func testAMoveIsDrawnAtOnceWithEveryoneStillLast() {
        XCTAssertEqual(Roles.drawnMoved(acme, roleID: "eng", beforeID: "design").map(\.roleId), ["admin", "mod", "eng", "design", "invite", "everyone"])
        XCTAssertEqual(Roles.drawnMoved(acme, roleID: "design", beforeID: "everyone").map(\.roleId), ["admin", "mod", "eng", "invite", "design", "everyone"])
        XCTAssertEqual(Roles.drawnMoved(acme, roleID: "gone", beforeID: "design").map(\.roleId), acme.map(\.roleId))
    }

    // MARK: The editor

    private var mod: RoleSummary { role("mod", name: "Moderator", color: "green", permissions: ["MANAGE_ROLES", "SEND_IN_THREADS"]) }

    private func draft(_ role: RoleSummary? = nil, _ change: (inout RoleDraft) -> Void = { _ in }) -> RoleDraft {
        var d = Roles.draft(of: role)
        change(&d)
        return d
    }

    func testANewRoleSavesOnceItHasAName() {
        XCTAssertEqual(Roles.draft(), RoleDraft(name: "", color: "", hideTag: false, permissions: []))
        XCTAssertFalse(Roles.savable(draft()))
        XCTAssertFalse(Roles.savable(draft { $0.name = "  " }))
        XCTAssertTrue(Roles.savable(draft { $0.name = "Design" }))
    }

    func testARoleSavesOnceSomethingDiffers() {
        XCTAssertFalse(Roles.savable(draft(mod), role: mod))
        XCTAssertFalse(Roles.savable(draft(mod) { $0.name = " Moderator " }, role: mod))
        XCTAssertTrue(Roles.savable(draft(mod) { $0.name = "Mods" }, role: mod))
        XCTAssertFalse(Roles.savable(draft(mod) { $0.name = "" }, role: mod))
        XCTAssertTrue(Roles.savable(draft(mod) { $0.color = "" }, role: mod))
        XCTAssertTrue(Roles.savable(draft(mod) { $0.hideTag = true }, role: mod))
        XCTAssertFalse(Roles.savable(draft(mod) { $0.permissions = ["SEND_IN_THREADS", "MANAGE_ROLES"] }, role: mod))
        XCTAssertTrue(Roles.savable(draft(mod) { $0.permissions = ["MANAGE_ROLES"] }, role: mod))
    }

    func testOfEveryoneOnlyThePermissionsCount() {
        let everyone = role("everyone", name: "Everyone", isEveryone: true, permissions: ["VIEW_CHANNEL"])
        XCTAssertFalse(Roles.savable(draft(everyone), role: everyone))
        XCTAssertTrue(Roles.savable(draft(everyone) { $0.permissions = [] }, role: everyone))
    }

    func testAPermissionNotHeldIsWithheld() {
        XCTAssertTrue(Roles.withheld(grantable: ["MANAGE_ROLES"], permission: "ADMINISTRATOR"))
        XCTAssertFalse(Roles.withheld(grantable: ["MANAGE_ROLES"], permission: "MANAGE_ROLES"))
        XCTAssertEqual(Roles.administrator, "ADMINISTRATOR")
    }

    func testTheQuestionsCountWhoHoldsTheRole() {
        XCTAssertEqual(RoleWords.administratorQuestion(serverName: "Acme", roleName: "Moderator", held: 3), ["Administrator allows everything in Acme, in every channel, whatever a channel’s own permissions say.", "3 members hold Moderator."])
        XCTAssertEqual(RoleWords.administratorQuestion(serverName: "Acme", roleName: "Moderator", held: 1)[1], "1 member holds Moderator.")
        XCTAssertEqual(RoleWords.administratorQuestion(serverName: "Acme", roleName: "", held: nil).count, 1)
        XCTAssertEqual(RoleWords.administratorQuestion(serverName: "Acme", roleName: "Moderator", held: 0).count, 1)
        XCTAssertEqual(RoleWords.administratorQuestion(serverName: "Acme", roleName: "Member", held: 1204)[1], "1,204 members hold Member.")
        XCTAssertEqual(RoleWords.deleteQuestion(held: 3), ["3 members hold this role. They lose what it allowed.", "This can’t be undone."])
        XCTAssertEqual(RoleWords.deleteQuestion(held: 1)[0], "1 member holds this role. They lose what it allowed.")
        XCTAssertEqual(RoleWords.deleteQuestion(held: 0)[0], "Nobody holds this role.")
        XCTAssertEqual(RoleWords.deleteQuestion(held: nil)[0], "Nobody holds this role.")
    }

    // MARK: A member

    func testTheirRolesInTheServersOrderWithoutEveryone() {
        XCTAssertEqual(Roles.held(acme, roleIDs: ["invite", "everyone", "design"]), [
            HeldRole(roleID: "design", label: "design", tone: .peach),
            HeldRole(roleID: "invite", label: "invite", tone: nil),
        ])
    }

    func testTheRolesToTickMarkWhatIsNotThisAccountsToGive() {
        let choices = Roles.choices(acme, held: ["mod", "eng"])
        XCTAssertEqual(choices.map(\.id), ["admin", "mod", "design", "eng", "invite"])
        XCTAssertEqual(choices.filter(\.checked).map(\.id), ["mod", "eng"])
        XCTAssertEqual(choices.filter(\.locked).map(\.id), ["admin", "mod"])
    }

    func testATickGivesAndTakesAndLeavesALockedRoleAsItWas() {
        XCTAssertEqual(Roles.toggled(acme, held: ["eng"], roleID: "design"), ["eng", "design"])
        XCTAssertEqual(Roles.toggled(acme, held: ["eng", "design"], roleID: "eng"), ["design"])
        XCTAssertEqual(Roles.toggled(acme, held: ["mod"], roleID: "mod"), ["mod"])
        XCTAssertEqual(Roles.toggled(acme, held: [], roleID: "admin"), [])
        XCTAssertEqual(Roles.toggled(acme, held: [], roleID: "everyone"), [])
    }

    func testWhatIsOfferedOfAMember() {
        XCTAssertEqual(Roles.offers(canManageRoles: true, guest: false, canManage: true, me: false), MemberOffers(roles: true, rolesNote: nil, title: true, kick: true))
        // Above this account: their roles below its own can still be given and taken.
        XCTAssertEqual(Roles.offers(canManageRoles: true, guest: false, canManage: false, me: false), MemberOffers(roles: true, rolesNote: nil, title: false, kick: false))
        XCTAssertEqual(Roles.offers(canManageRoles: true, guest: true, canManage: true, me: false), MemberOffers(roles: false, rolesNote: "Guests aren’t given roles.", title: true, kick: true))
        XCTAssertEqual(Roles.offers(canManageRoles: false, guest: false, canManage: true, me: false), MemberOffers(roles: false, rolesNote: nil, title: true, kick: true))
        // This account itself: its title is its own, and it is never offered Kick.
        XCTAssertEqual(Roles.offers(canManageRoles: false, guest: false, canManage: false, me: true), MemberOffers(roles: false, rolesNote: nil, title: true, kick: false))
        XCTAssertFalse(Roles.offers(canManageRoles: false, guest: false, canManage: false, me: false).any)
        XCTAssertTrue(Roles.offers(canManageRoles: true, guest: true, canManage: false, me: false).any)
    }

    func testTheOwnerAndAGuestAreMarked() {
        XCTAssertEqual(Roles.mark(owner: true, guest: false), "Owner")
        XCTAssertEqual(Roles.mark(owner: false, guest: true), "Guest")
        XCTAssertNil(Roles.mark(owner: false, guest: false))
    }

    func testATagOnItsToneOrThePlainChip() {
        XCTAssertEqual(Roles.tag(of: TagSummary(label: "Moderator", color: "green")), TagLook(label: "Moderator", tone: .green))
        XCTAssertEqual(Roles.tag(of: TagSummary(label: "Staff Engineer", color: "")), TagLook(label: "Staff Engineer", tone: nil))
        XCTAssertNil(Roles.tag(of: nil))
        XCTAssertNil(Roles.tag(of: TagSummary(label: "", color: "green")))
    }

    func testASearchReadsTheNameTheHandleAndTheTitle() {
        let people = [member("devon", name: "Devon Park", title: "Staff Engineer"), member("maya", name: "Maya Chen")]
        XCTAssertEqual(Roles.searched(people, query: "").count, 2)
        XCTAssertEqual(Roles.searched(people, query: "ENGINEER").map(\.handle), ["devon"])
        XCTAssertEqual(Roles.searched(people, query: "@maya").map(\.handle), ["maya"])
        XCTAssertEqual(Roles.searched(people, query: "chen").map(\.handle), ["maya"])
        XCTAssertEqual(Roles.searched(people, query: "nobody"), [])
    }

    func testASaveSendsOnlyWhatChangedOfAMember() {
        let devon = member("devon", name: "Devon Park", title: "Staff Engineer", roleIDs: ["mod", "eng"])
        var changes = Roles.memberChanges(devon, roleIDs: ["eng", "mod"], title: " Staff Engineer ")
        XCTAssertNil(changes.roleIDs)
        XCTAssertNil(changes.title)
        changes = Roles.memberChanges(devon, roleIDs: ["eng"], title: "")
        XCTAssertEqual(changes.roleIDs, ["eng"])
        XCTAssertEqual(changes.title, "")
        // What an editor did not offer is not sent.
        changes = Roles.memberChanges(devon, roleIDs: nil, title: "Principal")
        XCTAssertNil(changes.roleIDs)
        XCTAssertEqual(changes.title, "Principal")
    }

    func testWhoMayBeGivenARoleFromItsEditor() {
        var guest = member("jordan", name: "Jordan Lee")
        guest.guest = true
        let people = [member("sam", name: "Sam Okafor"), member("devon", name: "Devon Park", roleIDs: ["mod"]), guest, member("maya", name: "Maya Chen")]
        XCTAssertEqual(Roles.candidates(people, roleID: "mod", query: "", name: \.displayName).map(\.handle), ["maya", "sam"])
        XCTAssertEqual(Roles.candidates(people, roleID: "mod", query: "sam", name: \.displayName).map(\.handle), ["sam"])
        XCTAssertEqual(Roles.candidates(people, roleID: "eng", query: "", name: \.displayName).map(\.handle), ["devon", "maya", "sam"])
    }

    func testThePhonesOwnWords() {
        XCTAssertEqual(RoleWords.membersIn(128, serverName: "Acme"), "128 in Acme")
        XCTAssertEqual(RoleWords.membersIn(1204, serverName: "Acme"), "1,204 in Acme")
        XCTAssertEqual(RoleWords.moved("Design", position: 3), "Design moved to position 3")
        XCTAssertEqual(RoleWords.deleteTitle("Moderator"), "Delete Moderator?")
        XCTAssertEqual(RoleWords.rowValue(held: 41, hideTag: true, locked: false), "41 members, not shown as tag")
        XCTAssertEqual(RoleWords.rowValue(held: 2, hideTag: false, locked: true), "2 members, not yours to change")
        XCTAssertEqual(RoleWords.rowValue(held: nil, hideTag: false, locked: false), "")
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

    // MARK: Refusals and words

    func testARolesRefusals() {
        func said(_ code: String?, field: String? = nil, message: String = "") -> String {
            RoleWords.roleRefusal(code: code, details: field.map { ["field": $0] } ?? [:], message: message, serverName: "Acme")
        }
        XCTAssertEqual(said("ALREADY_EXISTS"), "Another role in Acme has that name.")
        XCTAssertTrue(said("INVALID_ARGUMENT", field: "name").contains("1 to 32 characters"))
        XCTAssertEqual(said("INVALID_ARGUMENT", field: "name"), "A role’s name is 1 to 32 characters, and can’t be “everyone”.")
        XCTAssertEqual(said("INVALID_ARGUMENT", field: "color"), "A role’s colour is one of the nine, or none.")
        XCTAssertTrue(said("INVALID_ARGUMENT", field: "role_ids").contains("Try again"))
        XCTAssertTrue(said("PERMISSION_DENIED").contains("isn’t yours to change"))
        XCTAssertEqual(said("NOT_FOUND"), "That role is gone. Someone deleted it.")
        XCTAssertEqual(said("FAILED_PRECONDITION"), "This server has as many roles as it can hold.")
        XCTAssertEqual(said(nil, message: "transport: offline"), "transport: offline")
        XCTAssertEqual(said("INVALID_ARGUMENT", field: "other", message: "bad"), "bad")
    }

    func testAMembersRefusals() {
        func said(_ code: String?, field: String? = nil, message: String = "") -> String {
            RoleWords.memberRefusal(code: code, details: field.map { ["field": $0] } ?? [:], message: message)
        }
        XCTAssertEqual(said("NOT_FOUND"), "They are no longer a member.")
        XCTAssertTrue(said("INVALID_ARGUMENT", field: "title").contains("32 characters"))
        XCTAssertEqual(said("PERMISSION_DENIED"), "That isn’t yours to change. They may be at or above your own highest role.")
        XCTAssertEqual(said("FAILED_PRECONDITION"), "A member can’t hold that many roles.")
        XCTAssertEqual(said(nil, message: "nope"), "nope")
    }

    func testTheWordsAreTheSheets() {
        XCTAssertEqual(RoleWords.rolesHintMenu, "In order, highest first. A role is moved from its menu. A new role starts last, just above Everyone.")
        XCTAssertEqual(RoleWords.roleNameHint(serverName: "Acme"), "1 to 32 characters. No other role in Acme can have the same name.")
        XCTAssertEqual(RoleWords.tagSwitchCaption(shown: true), "This role can be the tag beside a member’s name.")
        XCTAssertEqual(RoleWords.tagSwitchCaption(shown: false), "This role only carries permissions. It is nobody’s tag.")
        XCTAssertEqual(RoleWords.withheldCaption("Change the server’s name, picture and settings."), "Change the server’s name, picture and settings. You don’t hold it, so it is not yours to give or take.")
        XCTAssertEqual(RoleWords.tagRoles.label, "Roles")
        XCTAssertEqual(RoleWords.tagTitles.caption, "Each member’s title, in no colour. A member without a title has no tag.")
    }
}
