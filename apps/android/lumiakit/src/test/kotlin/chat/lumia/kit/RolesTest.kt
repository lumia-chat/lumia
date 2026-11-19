// Roles and titles as values (architecture 7.5): the sections a server's
// settings show, the list's counts and locks, where a role may be moved,
// when an editor's Save is offered, what a member's row offers, the tag,
// the search and the words. The cases of the web's `roles.test.ts`, which
// every shell's copy of the sheet is held to.
package chat.lumia.kit

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.lumia_ffi.MemberSummary
import uniffi.lumia_ffi.RoleSummary
import uniffi.lumia_ffi.TagSummary

private fun role(
    id: String,
    name: String = id,
    color: String = "",
    isEveryone: Boolean = false,
    hideTag: Boolean = false,
    permissions: List<String> = emptyList(),
    canManage: Boolean = true,
) = RoleSummary(id, name, color, 0u, isEveryone, hideTag, permissions, canManage)

private fun member(
    id: String,
    display: String = "",
    handle: String = id,
    nickname: String = "",
    roleIds: List<String> = emptyList(),
    title: String = "",
    guest: Boolean = false,
    owner: Boolean = false,
    canManage: Boolean = false,
) = MemberSummary(id, handle, display, nickname, "", "", "", false, false, roleIds, title, guest, owner, null, canManage)

// Acme as Devon sees it: Admin and Moderator are at or above Devon's highest.
private val ACME = listOf(
    role("admin", color = "indigo", canManage = false),
    role("mod", color = "green", canManage = false),
    role("design", color = "peach"),
    role("eng", color = "sky"),
    role("invite", hideTag = true),
    role("everyone", name = "Everyone", isEveryone = true),
)

class RolesTest {
    // Sections

    @Test
    fun eachPermissionOpensItsOwnSectionAndManageRolesOpensMembersToo() {
        assertEquals(listOf(SettingsSection.OVERVIEW, SettingsSection.ROLES, SettingsSection.MEMBERS), settingsSections(canManage = true, canManageRoles = true, canManageMembers = true))
        assertEquals(listOf(SettingsSection.OVERVIEW), settingsSections(canManage = true, canManageRoles = false, canManageMembers = false))
        assertEquals(listOf(SettingsSection.ROLES, SettingsSection.MEMBERS), settingsSections(canManage = false, canManageRoles = true, canManageMembers = false))
        assertEquals(listOf(SettingsSection.MEMBERS), settingsSections(canManage = false, canManageRoles = false, canManageMembers = true))
        assertEquals(emptyList<SettingsSection>(), settingsSections(canManage = false, canManageRoles = false, canManageMembers = false))
    }

    // The list

    @Test
    fun rolesAreCountedWithoutEveryone() {
        assertEquals("5 roles", rolesCount(ACME))
        assertEquals("1 role", rolesCount(listOf(role("a"), role("everyone", isEveryone = true))))
        assertEquals("1 member", membersCount(1))
        assertEquals("1,204 members", membersCount(1204))
    }

    @Test
    fun whoHoldsARoleIsCountedAndEveryoneHoldsEveryone() {
        val members = listOf(member("a", roleIds = listOf("eng")), member("b", roleIds = listOf("eng", "design")), member("c"))
        assertEquals(2, holders(ACME[3], members))
        assertEquals(3, holders(ACME[5], members))
        assertNull(holders(ACME[3], null))
    }

    @Test
    fun aRoleNotManagedIsLockedAndEveryoneNeverIs() {
        assertTrue(lockedRole(ACME[0]))
        assertFalse(lockedRole(ACME[2]))
        assertFalse(lockedRole(role("everyone", isEveryone = true, canManage = false)))
    }

    @Test
    fun aToneIsOneOfTheNine() {
        assertEquals(AvatarTone.SKY, toneOf("sky"))
        assertNull(toneOf(""))
        assertNull(toneOf("#ff0000"))
        // The nine, by the names the instance keeps them by.
        assertEquals(listOf("peach", "blue", "purple", "mint", "amber", "sky", "stone", "green", "indigo"), AvatarTone.entries.map { it.stored })
    }

    // Moving a role

    @Test
    fun aRoleLandsBeforeOneItManagesOrBeforeEveryone() {
        assertTrue(landsBefore(ACME, "eng", "design"))
        assertTrue(landsBefore(ACME, "design", "everyone"))
        assertTrue(landsBefore(ACME, "design", "invite"))
    }

    @Test
    fun neverBeforeALockedRoleNeverWhereItIsAndALockedRoleAndEveryoneDoNotMove() {
        assertFalse(landsBefore(ACME, "eng", "mod"))
        assertFalse(landsBefore(ACME, "eng", "admin"))
        assertFalse(landsBefore(ACME, "eng", "eng"))
        assertFalse(landsBefore(ACME, "eng", "invite"))
        assertFalse(landsBefore(ACME, "mod", "everyone"))
        assertFalse(landsBefore(ACME, "everyone", "design"))
        assertFalse(landsBefore(ACME, "gone", "design"))
    }

    @Test
    fun theOrderSentIsEveryRoleButEveryone() {
        assertEquals(listOf("admin", "mod", "eng", "design", "invite"), movedBefore(ACME, "eng", "design"))
        assertEquals(listOf("admin", "mod", "eng", "invite", "design"), movedBefore(ACME, "design", "everyone"))
        assertEquals("eng moved to position 3", roleMovedWords("eng", movedBefore(ACME, "eng", "design"), "eng"))
    }

    @Test
    fun aMoveIsDrawnBeforeItIsAnswered() {
        assertEquals(ACME, rolesInOrder(ACME, null))
        assertEquals(listOf("admin", "mod", "eng", "design", "invite", "everyone"), rolesInOrder(ACME, movedBefore(ACME, "eng", "design")).map { it.roleId })
        // A role the order does not name (made since) keeps its place after those it does, and one it names that is gone is left out.
        assertEquals(listOf("eng", "admin", "mod", "design", "invite", "everyone"), rolesInOrder(ACME, listOf("gone", "eng")).map { it.roleId })
    }

    @Test
    fun aLabelIsOneLineOfThirtyTwoCharacters() {
        assertEquals("Moderator", labelClipped("Moderator"))
        assertEquals("a".repeat(32), labelClipped("a".repeat(40)))
        assertEquals("Staff Engineer", labelClipped("Staff\nEngineer"))
        // Characters as a person counts them: a flag is one, though it is four UTF-16 units.
        assertEquals("🇨🇦".repeat(32), labelClipped("🇨🇦".repeat(33)))
    }

    @Test
    fun aStepUpOrDownWhereThereIsOne() {
        assertEquals("design", stepBefore(ACME, "eng", -1))
        assertEquals("everyone", stepBefore(ACME, "eng", 1))
        assertEquals("invite", stepBefore(ACME, "design", 1))
        // Design is the highest Devon manages, and Can invite the last above Everyone.
        assertNull(stepBefore(ACME, "design", -1))
        assertNull(stepBefore(ACME, "invite", 1))
        assertNull(stepBefore(ACME, "admin", 1))
        assertNull(stepBefore(ACME, "gone", 1))
    }

    // The editor

    private val mod = role("mod", name = "Moderator", color = "green", permissions = listOf("MANAGE_ROLES", "SEND_IN_THREADS"))

    @Test
    fun aNewRoleSavesOnceItHasAName() {
        assertFalse(savable(draftOf()))
        assertFalse(savable(draftOf().copy(name = "  ")))
        assertTrue(savable(draftOf().copy(name = "Design")))
    }

    @Test
    fun aRoleSavesOnceSomethingDiffers() {
        assertFalse(savable(draftOf(mod), mod))
        assertFalse(savable(draftOf(mod).copy(name = " Moderator "), mod))
        assertTrue(savable(draftOf(mod).copy(name = "Mods"), mod))
        assertFalse(savable(draftOf(mod).copy(name = ""), mod))
        assertTrue(savable(draftOf(mod).copy(color = ""), mod))
        assertTrue(savable(draftOf(mod).copy(hideTag = true), mod))
        assertFalse(savable(draftOf(mod).copy(permissions = listOf("SEND_IN_THREADS", "MANAGE_ROLES")), mod))
        assertTrue(savable(draftOf(mod).copy(permissions = listOf("MANAGE_ROLES")), mod))
    }

    @Test
    fun ofEveryoneOnlyThePermissionsCount() {
        val everyone = role("everyone", name = "Everyone", isEveryone = true, permissions = listOf("VIEW_CHANNEL"))
        assertFalse(savable(draftOf(everyone), everyone))
        assertTrue(savable(draftOf(everyone).copy(permissions = emptyList()), everyone))
    }

    @Test
    fun aPermissionNotHeldIsWithheld() {
        assertTrue(withheld(listOf("MANAGE_ROLES"), "ADMINISTRATOR"))
        assertFalse(withheld(listOf("MANAGE_ROLES"), "MANAGE_ROLES"))
        assertEquals("Delete anyone’s messages. You don’t hold it, so it is not yours to give or take.", withheldCaption("Delete anyone’s messages."))
    }

    @Test
    fun aSwitchTurnedIsInTheDraftOnce() {
        assertEquals(listOf("A", "B"), turned(listOf("A"), "B", on = true))
        assertEquals(listOf("A", "B"), turned(listOf("B", "A"), "B", on = true))
        assertEquals(listOf("A"), turned(listOf("A", "B"), "B", on = false))
        assertEquals(listOf("A"), turned(listOf("A"), "B", on = false))
    }

    @Test
    fun theQuestionsCountWhoHoldsTheRole() {
        assertEquals(
            listOf("Administrator allows everything in Acme, in every channel, whatever a channel’s own permissions say.", "3 members hold Moderator."),
            administratorQuestion("Acme", "Moderator", 3),
        )
        assertEquals("1 member holds Moderator.", administratorQuestion("Acme", "Moderator", 1)[1])
        assertEquals(1, administratorQuestion("Acme", "", null).size)
        assertEquals(1, administratorQuestion("Acme", "Moderator", 0).size)
        assertEquals(listOf("3 members hold this role. They lose what it allowed.", "This can’t be undone."), deleteQuestion(3))
        assertEquals("1 member holds this role. They lose what it allowed.", deleteQuestion(1)[0])
        assertEquals(listOf("Nobody holds this role.", "This can’t be undone."), deleteQuestion(0))
    }

    @Test
    fun theDeleteQuestionSaysNothingOfHoldersItDoesNotKnow() {
        // The members not read, or not to be had: forty may hold the role, so "Nobody" is not said.
        assertEquals(listOf("This can’t be undone."), deleteQuestion(null))
        assertEquals(listOf("This can’t be undone."), deleteQuestion(holders(ACME[3], null)))
        // Read, and nobody has it: that is known, and said.
        assertEquals("Nobody holds this role.", deleteQuestion(holders(ACME[3], listOf(member("a"))))[0])
    }

    // A member

    @Test
    fun theirRolesInTheServersOrderWithoutEveryone() {
        assertEquals(
            listOf(HeldRole("design", "design", AvatarTone.PEACH), HeldRole("invite", "invite", null)),
            rolesHeld(ACME, listOf("invite", "everyone", "design")),
        )
    }

    @Test
    fun theRolesToTickMarkWhatIsNotThisAccountsToGive() {
        val choices = roleChoices(ACME, listOf("mod", "eng"))
        assertEquals(listOf("admin", "mod", "design", "eng", "invite"), choices.map { it.id })
        assertEquals(listOf("mod", "eng"), choices.filter { it.checked }.map { it.id })
        assertEquals(listOf("admin", "mod"), choices.filter { it.locked }.map { it.id })
    }

    @Test
    fun aTickGivesAndTakesAndLeavesALockedRoleAsItWas() {
        assertEquals(listOf("eng", "design"), toggledRole(ACME, listOf("eng"), "design"))
        assertEquals(listOf("design"), toggledRole(ACME, listOf("eng", "design"), "eng"))
        assertEquals(listOf("mod"), toggledRole(ACME, listOf("mod"), "mod"))
        assertEquals(emptyList<String>(), toggledRole(ACME, emptyList(), "admin"))
        assertEquals(emptyList<String>(), toggledRole(ACME, emptyList(), "everyone"))
        assertTrue(sameRoles(listOf("a", "b"), listOf("b", "a")))
        assertFalse(sameRoles(listOf("a"), listOf("a", "b")))
    }

    @Test
    fun whatIsOfferedOfAMember() {
        assertEquals(MemberOffers(roles = true, rolesNote = null, title = true, kick = true), memberOffers(canManageRoles = true, guest = false, canManage = true, me = false))
        // Above this account: their roles below its own can still be given and taken.
        assertEquals(MemberOffers(roles = true, rolesNote = null, title = false, kick = false), memberOffers(canManageRoles = true, guest = false, canManage = false, me = false))
        assertEquals(MemberOffers(roles = false, rolesNote = "Guests aren’t given roles.", title = true, kick = true), memberOffers(canManageRoles = true, guest = true, canManage = true, me = false))
        assertEquals(MemberOffers(roles = false, rolesNote = null, title = true, kick = true), memberOffers(canManageRoles = false, guest = false, canManage = true, me = false))
        // This account itself: its title is its own, and it is never offered Kick.
        assertEquals(MemberOffers(roles = false, rolesNote = null, title = true, kick = false), memberOffers(canManageRoles = false, guest = false, canManage = false, me = true))
        assertFalse(offersAny(memberOffers(canManageRoles = false, guest = false, canManage = false, me = false)))
        assertTrue(offersAny(memberOffers(canManageRoles = true, guest = true, canManage = false, me = false)))
    }

    @Test
    fun theOwnerAndAGuestAreMarked() {
        assertEquals("Owner", memberMark(owner = true, guest = false))
        assertEquals("Guest", memberMark(owner = false, guest = true))
        assertNull(memberMark(owner = false, guest = false))
        assertEquals("Owner", memberMark(member("ryan", owner = true)))
    }

    @Test
    fun aTagOnItsToneOrThePlainChip() {
        assertEquals(TagLook("Moderator", AvatarTone.GREEN), tagOf(TagSummary("Moderator", "green")))
        assertEquals(TagLook("Staff Engineer", null), tagOf(TagSummary("Staff Engineer", "")))
        assertNull(tagOf(null))
        assertNull(tagOf(TagSummary("", "green")))
    }

    @Test
    fun aSearchReadsTheNameTheHandleAndTheTitle() {
        val people = listOf(
            member("devon", display = "Devon Park", title = "Staff Engineer"),
            member("maya", display = "Maya Chen"),
        )
        assertEquals(2, searched(people, "").size)
        assertEquals(listOf("devon"), searched(people, "ENGINEER").map { it.handle })
        assertEquals(listOf("maya"), searched(people, "@maya").map { it.handle })
        assertEquals(listOf("maya"), searched(people, "chen").map { it.handle })
        assertEquals(emptyList<MemberSummary>(), searched(people, "nobody"))
    }

    @Test
    fun addMembersOffersThoseWhoDoNotHoldTheRoleAndNoGuest() {
        val people = listOf(
            member("sam", display = "Sam Okafor", roleIds = listOf("eng")),
            member("maya", display = "Maya Chen"),
            member("jordan", display = "Jordan Lee", guest = true),
            member("devon", display = "Devon Park", roleIds = listOf("mod")),
        )
        assertEquals(listOf("devon", "maya"), holderCandidates(people, "eng", "").map { it.handle })
        assertEquals(listOf("maya"), holderCandidates(people, "eng", "chen").map { it.handle })
        assertEquals("Add", addHoldersButton(0))
        assertEquals("Add", addHoldersButton(1))
        assertEquals("Add 2 members", addHoldersButton(2))
    }

    // Refusals

    @Test
    fun aRolesRefusalInWords() {
        assertEquals("Another role in Acme has that name.", roleRefusal("ALREADY_EXISTS", null, "", "Acme"))
        assertEquals("A role’s name is 1 to 32 characters on one line, with no hidden characters, and can’t be “everyone”.", roleRefusal("INVALID_ARGUMENT", "name", "", "Acme"))
        assertTrue(roleRefusal("INVALID_ARGUMENT", "role_ids", "", "Acme").contains("Try again"))
        assertTrue(roleRefusal("PERMISSION_DENIED", null, "", "Acme").contains("isn’t yours to change"))
        assertEquals("That role is gone. Someone deleted it.", roleRefusal("NOT_FOUND", null, "", "Acme"))
        assertEquals("A role’s colour is one of the nine, or none.", roleRefusal("INVALID_ARGUMENT", "color", "", "Acme"))
        assertEquals("This server has as many roles as it can hold.", roleRefusal("FAILED_PRECONDITION", null, "", "Acme"))
        assertEquals("transport: offline", roleRefusal(null, null, "transport: offline", "Acme"))
        assertTrue(refusalIsOfName("ALREADY_EXISTS", null))
        assertTrue(refusalIsOfName("INVALID_ARGUMENT", "name"))
        assertFalse(refusalIsOfName("INVALID_ARGUMENT", "color"))
        assertFalse(refusalIsOfName("PERMISSION_DENIED", null))
    }

    @Test
    fun aMembersRefusalInWords() {
        assertEquals("They are no longer a member.", memberRefusal("NOT_FOUND", null, ""))
        assertEquals("A title is up to 32 characters on one line, with no hidden characters.", memberRefusal("INVALID_ARGUMENT", "title", ""))
        // A member's own title, changed more than ten times in an hour: the limit said, not the instance's own words.
        assertEquals("You’ve changed your title too many times this hour. Try again later.", memberRefusal("RATE_LIMITED", null, "rate limited"))
        assertTrue(memberRefusal("PERMISSION_DENIED", null, "").contains("isn’t yours to change"))
        assertEquals("A member can’t hold that many roles.", memberRefusal("FAILED_PRECONDITION", null, ""))
        assertEquals("nope", memberRefusal(null, null, "nope"))
    }

    @Test
    fun theWordsAreTheSheets() {
        assertEquals("In order, highest first. A role is moved from its menu. A new role starts last, just above Everyone.", ROLES_HINT_MENU)
        assertEquals("1 to 32 characters. No other role in Acme can have the same name.", roleNameHint("Acme"))
        assertEquals("This role can be the tag beside a member’s name.", tagSwitchCaption(true))
        assertEquals("This role only carries permissions. It is nobody’s tag.", tagSwitchCaption(false))
        assertEquals("Up to 32 characters. A title grants nothing.", TITLE_HINT)
        assertEquals("Its members lose what it allowed. This can’t be undone.", DELETE_ROLE_HINT)
        assertEquals("A profile shows a member’s title and roles either way.", TAG_NOTE)
    }
}
