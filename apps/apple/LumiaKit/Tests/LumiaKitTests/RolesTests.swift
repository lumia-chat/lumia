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

    func testARoleSavesOnceSomethingDiffersFromWhatTheEditorOpenedOn() {
        let opened = draft(mod)
        XCTAssertFalse(Roles.savable(draft(mod), opened: opened))
        XCTAssertFalse(Roles.savable(draft(mod) { $0.name = " Moderator " }, opened: opened))
        XCTAssertTrue(Roles.savable(draft(mod) { $0.name = "Mods" }, opened: opened))
        XCTAssertFalse(Roles.savable(draft(mod) { $0.name = "" }, opened: opened))
        XCTAssertTrue(Roles.savable(draft(mod) { $0.color = "" }, opened: opened))
        XCTAssertTrue(Roles.savable(draft(mod) { $0.hideTag = true }, opened: opened))
        XCTAssertFalse(Roles.savable(draft(mod) { $0.permissions = ["SEND_IN_THREADS", "MANAGE_ROLES"] }, opened: opened))
        XCTAssertTrue(Roles.savable(draft(mod) { $0.permissions = ["MANAGE_ROLES"] }, opened: opened))
    }

    func testOfEveryoneOnlyThePermissionsCount() {
        let everyone = role("everyone", name: "Everyone", isEveryone: true, permissions: ["VIEW_CHANNEL"])
        XCTAssertFalse(Roles.savable(draft(everyone), opened: draft(everyone), everyone: true))
        XCTAssertFalse(Roles.savable(draft(everyone) { $0.name = "All" }, opened: draft(everyone), everyone: true))
        XCTAssertTrue(Roles.savable(draft(everyone) { $0.permissions = [] }, opened: draft(everyone), everyone: true))
    }

    // Someone else changes the role while its editor is open. Save is about
    // what this editor changed, so it does not light for their change; an
    // editor nobody has touched follows the role, and one that was touched
    // keeps both what it opened on and what it holds, so that the save
    // sends its own change and nothing else.
    func testAnEditorFollowsItsRoleUntilItIsTouched() {
        let opened = draft(mod)
        var elsewhere = mod
        elsewhere.permissions = ["SEND_IN_THREADS"]
        elsewhere.color = "sky"

        // Untouched: both are the role as it now is, and there is nothing to save.
        var next = Roles.followed(opened: opened, draft: draft(mod), role: elsewhere)
        XCTAssertEqual(next.opened, draft(elsewhere))
        XCTAssertEqual(next.draft, draft(elsewhere))
        XCTAssertFalse(Roles.savable(next.draft, opened: next.opened))
        // A permission switched on and off again is untouched, whatever order it left.
        next = Roles.followed(opened: opened, draft: draft(mod) { $0.permissions = ["SEND_IN_THREADS", "MANAGE_ROLES"] }, role: elsewhere)
        XCTAssertEqual(next.draft, draft(elsewhere))

        // Touched: the name typed stays, and so does what the editor opened on.
        let renamed = draft(mod) { $0.name = "Mods" }
        next = Roles.followed(opened: opened, draft: renamed, role: elsewhere)
        XCTAssertEqual(next.opened, opened)
        XCTAssertEqual(next.draft, renamed)
        XCTAssertTrue(Roles.savable(next.draft, opened: next.opened))
        // Not followed, Save is still not lit by the other's change alone:
        // a space typed and nothing else is no change to save.
        let spaced = draft(mod) { $0.name = "Moderator " }
        next = Roles.followed(opened: opened, draft: spaced, role: elsewhere)
        XCTAssertEqual(next.draft, spaced)
        XCTAssertFalse(Roles.savable(next.draft, opened: next.opened), "someone else's change lit Save")
        XCTAssertTrue(Roles.untouched(opened: opened, draft: draft(mod)))
        XCTAssertFalse(Roles.untouched(opened: opened, draft: spaced))
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
        XCTAssertEqual(RoleWords.deleteQuestion(held: 0), ["Nobody holds this role.", "This can’t be undone."])
        XCTAssertEqual(RoleWords.deleteQuestion(held: 1204)[0], "1,204 members hold this role. They lose what it allowed.")
        // Not known, the member list not read yet or not read at all: nothing
        // is said of who holds it, least of all that nobody does.
        XCTAssertEqual(RoleWords.deleteQuestion(held: nil), ["This can’t be undone."])
        XCTAssertEqual(RoleWords.deleteQuestion(held: Roles.holders(acme[1], members: nil)), ["This can’t be undone."])
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

    // A member list that could not be read is not an empty one: who holds
    // a role stays unknown, or as it was last known, and "Delete
    // Moderator?" does not say nobody holds a role forty members hold.
    func testAMemberListThatCouldNotBeReadIsNotAnEmptyOne() {
        let forty = (0..<40).map { member("m\($0)", roleIDs: ["mod"]) }
        // Never read, and the read failed: no list, so no count.
        var lists = MemberLists.after(held: [:], serverIDs: ["acme"], read: [:])
        XCTAssertNil(lists["acme"])
        XCTAssertNil(Roles.holders(acme[1], members: lists["acme"]))
        XCTAssertEqual(RoleWords.deleteQuestion(held: Roles.holders(acme[1], members: lists["acme"])), ["This can’t be undone."])
        // Read: counted.
        lists = MemberLists.after(held: lists, serverIDs: ["acme"], read: ["acme": forty])
        XCTAssertEqual(Roles.holders(acme[1], members: lists["acme"]), 40)
        // Asked again and not read: what was held stays.
        lists = MemberLists.after(held: lists, serverIDs: ["acme", "trail"], read: [:])
        XCTAssertEqual(lists["acme"]?.count, 40)
        XCTAssertNil(lists["trail"])
        XCTAssertEqual(RoleWords.deleteQuestion(held: Roles.holders(acme[1], members: lists["acme"]))[0], "40 members hold this role. They lose what it allowed.")
        // A server left goes, whatever was read of it.
        XCTAssertEqual(MemberLists.after(held: lists, serverIDs: ["trail"], read: ["acme": forty, "trail": []]), ["trail": []])
    }

    // On a profile card "You" stands beside "Owner" or "Guest", not in its place.
    func testACardsMarksSayYouAndWhoTheyAreToTheServer() {
        XCTAssertEqual(Roles.cardMarks(me: true, owner: true, guest: false), ["You", "Owner"])
        XCTAssertEqual(Roles.cardMarks(me: true, owner: false, guest: true), ["You", "Guest"])
        XCTAssertEqual(Roles.cardMarks(me: true, owner: false, guest: false), ["You"])
        XCTAssertEqual(Roles.cardMarks(me: false, owner: true, guest: false), ["Owner"])
        XCTAssertEqual(Roles.cardMarks(me: false, owner: false, guest: true), ["Guest"])
        XCTAssertEqual(Roles.cardMarks(me: false, owner: false, guest: false), [])
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
        let opened = Roles.form(of: devon)
        XCTAssertEqual(opened, MemberForm(roleIDs: ["mod", "eng"], title: "Staff Engineer"))
        // Nothing changed: nothing to give, to take or to say.
        var changes = Roles.memberChanges(opened: opened, form: MemberForm(roleIDs: ["eng", "mod"], title: " Staff Engineer "))
        XCTAssertEqual(changes.give, [])
        XCTAssertEqual(changes.take, [])
        XCTAssertNil(changes.title)
        // One unticked, one ticked, the title cleared: each named, and never the whole set.
        changes = Roles.memberChanges(opened: opened, form: MemberForm(roleIDs: ["eng", "design"], title: ""))
        XCTAssertEqual(changes.give, ["design"])
        XCTAssertEqual(changes.take, ["mod"])
        XCTAssertEqual(changes.title, "")
        // The title alone, trimmed.
        changes = Roles.memberChanges(opened: opened, form: MemberForm(roleIDs: ["mod", "eng"], title: " Principal "))
        XCTAssertEqual(changes.give, [])
        XCTAssertEqual(changes.take, [])
        XCTAssertEqual(changes.title, "Principal")
    }

    // A member given a role elsewhere while their form is open: the role is
    // in neither list of the save, so it stays theirs; an untouched form
    // follows them, a touched one keeps what it opened on and what it holds.
    func testAMembersFormFollowsThemUntilItIsTouched() {
        let devon = member("devon", name: "Devon Park", title: "Staff Engineer", roleIDs: ["mod"])
        let opened = Roles.form(of: devon)
        var elsewhere = devon
        elsewhere.roleIds = ["mod", "design"]
        elsewhere.title = "Principal"

        var next = Roles.followed(opened: opened, form: opened, member: elsewhere)
        XCTAssertEqual(next.opened, Roles.form(of: elsewhere))
        XCTAssertEqual(next.form, Roles.form(of: elsewhere))

        // Mod unticked before the change arrived: Design is not this form's to take.
        let touched = MemberForm(roleIDs: [], title: "Staff Engineer")
        next = Roles.followed(opened: opened, form: touched, member: elsewhere)
        XCTAssertEqual(next.opened, opened)
        XCTAssertEqual(next.form, touched)
        let changes = Roles.memberChanges(opened: next.opened, form: next.form)
        XCTAssertEqual(changes.give, [])
        XCTAssertEqual(changes.take, ["mod"])
        XCTAssertNil(changes.title, "a title someone else changed was sent back as it was")
        XCTAssertFalse(Roles.untouched(opened: opened, form: MemberForm(roleIDs: ["mod"], title: "Staff Engineer ")))
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
        XCTAssertEqual(said("INVALID_ARGUMENT", field: "name"), "A role’s name is 1 to 32 characters on one line, with no hidden characters, and can’t be “everyone”.")
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
        XCTAssertEqual(said("INVALID_ARGUMENT", field: "title"), "A title is up to 32 characters on one line, with no hidden characters.")
        // This account's own title, changed more than ten times in an hour.
        XCTAssertEqual(said("RATE_LIMITED", message: "too many requests"), "You’ve changed your title too many times this hour. Try again later.")
        XCTAssertEqual(said("PERMISSION_DENIED"), "That isn’t yours to change. They may be at or above your own highest role.")
        XCTAssertEqual(said("FAILED_PRECONDITION"), "A member can’t hold that many roles.")
        XCTAssertEqual(said(nil, message: "nope"), "nope")
    }

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
            RoleWords.memberRefusal(code: error.lumiaCode, details: error.lumiaDetails, message: error.lumiaMessage)
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
                RoleWords.roleRefusal(code: error.lumiaCode, details: error.lumiaDetails, message: error.lumiaMessage, serverName: "Acme"),
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
        var opened = Roles.draft(of: made)
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
        (opened, draft) = Roles.followed(opened: opened, draft: draft, role: changed)
        XCTAssertEqual(opened.permissions.sorted(), ["MANAGE_MESSAGES", "MENTION_EVERYONE"])
        XCTAssertTrue(Roles.savable(draft, opened: opened))
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
        opened = Roles.draft(of: saved)
        draft = opened
        XCTAssertFalse(Roles.savable(draft, opened: opened))
        let recoloured = try await elsewhere(instance, "PATCH", "/servers/\(serverID)/roles/\(made.roleId)", token: token, body: ["color": "mint"])
        XCTAssertEqual(recoloured.status, 200, "\(recoloured.json)")
        await session.refreshSnapshot()
        (opened, draft) = Roles.followed(opened: opened, draft: draft, role: try XCTUnwrap(role()))
        XCTAssertEqual(draft.color, "mint")
        XCTAssertFalse(Roles.savable(draft, opened: opened))
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
        var opened = Roles.form(of: try XCTUnwrap(member()))
        var form = opened
        form.roleIDs = Roles.toggled(roles, held: form.roleIDs, roleID: engineering)
        XCTAssertEqual(form.roleIDs, [])

        // Elsewhere they are given Design, and the phone hears of it.
        let token = try await signedInBeside(instance, handle: handle, password: password)
        let given = try await elsewhere(instance, "PATCH", "/servers/\(serverID)/members/\(them)", token: token, body: ["addRoleIds": [design]])
        XCTAssertEqual(given.status, 200, "\(given.json)")
        await session.refreshMembers(serverID: serverID)
        XCTAssertEqual(Set(member()?.roleIds ?? []), [design, engineering])
        (opened, form) = Roles.followed(opened: opened, form: form, member: try XCTUnwrap(member()))
        XCTAssertEqual(opened.roleIDs, [engineering])

        // Saved: Engineering taken, Design still theirs, here and on the instance.
        try await session.saveMember(serverID: serverID, accountID: them, opened: opened, form: form)
        XCTAssertEqual(member()?.roleIds, [design], "the save wrote the roles the form first saw over the one given elsewhere")
        let listed = try await session.client.members(serverID: serverID).first { $0.accountId == them }
        XCTAssertEqual(listed?.roleIds, [design])

        // One request, whole or not at all: a title refused gives no role with it.
        opened = Roles.form(of: try XCTUnwrap(member()))
        form = MemberForm(roleIDs: opened.roleIDs + [support], title: "Sup\u{200B}port lead")
        do {
            try await session.saveMember(serverID: serverID, accountID: them, opened: opened, form: form)
            XCTFail("a title with a hidden character was taken")
        } catch {
            XCTAssertEqual(RoleWords.memberRefusal(code: error.lumiaCode, details: error.lumiaDetails, message: error.lumiaMessage), "A title is up to 32 characters on one line, with no hidden characters.")
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
