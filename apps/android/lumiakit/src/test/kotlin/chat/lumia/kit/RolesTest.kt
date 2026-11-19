// Roles and titles (architecture 7.5): what of them the phone keeps, and
// how it hands its own types to the core. The rules and the words are the
// core's, held to `proto/vectors/roles/cases.json` by its own tests; these
// ask the core itself (the library `scripts/core.sh` builds for this
// machine), as `ViewerTest` does, only where an adapter of the phone's
// stands between a screen and it.
package chat.lumia.kit

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.lumia_ffi.LumiaException
import uniffi.lumia_ffi.MemberForm
import uniffi.lumia_ffi.MemberSummary
import uniffi.lumia_ffi.RoleSummary
import uniffi.lumia_ffi.TagSummary
import uniffi.lumia_ffi.rolesChangesAny
import uniffi.lumia_ffi.rolesMemberChanges
import uniffi.lumia_ffi.rolesMovedBefore
import uniffi.lumia_ffi.rolesSavable
import uniffi.lumia_ffi.rolesTagLook
import uniffi.lumia_ffi.rolesToggled

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
    // Tones as the phone draws them

    @Test
    fun aToneTheCoreNamesIsOneOfTheNineAndNoneIsThePlainChip() {
        assertEquals(AvatarTone.INDIGO, toneOf("indigo"))
        assertEquals(AvatarTone.PEACH, avatarTone("peach"))
        assertNull(toneOf(""))
        assertNull(toneOf("magenta"))
        assertNull(avatarTone(null))
        // Every one of the nine is a tone the core knows by the name the phone keeps it by.
        for (tone in AvatarTone.entries) assertEquals(tone, toneOf(tone.stored))
        // A tag's tone, as the core reads it, drawn on the phone's.
        assertEquals(AvatarTone.GREEN, avatarTone(rolesTagLook(TagSummary("Moderator", "green"))?.tone))
        assertNull(avatarTone(rolesTagLook(TagSummary("Staff Engineer", ""))?.tone))
    }

    // The list

    @Test
    fun aMoveIsDrawnBeforeItIsAnswered() {
        assertEquals(ACME, rolesInOrder(ACME, null))
        assertEquals(listOf("admin", "mod", "eng", "design", "invite", "everyone"), rolesInOrder(ACME, rolesMovedBefore(ACME, "eng", "design")).map { it.roleId })
        // A role the order does not name (made since) keeps its place after those it does, and one it names that is gone is left out.
        assertEquals(listOf("eng", "admin", "mod", "design", "invite", "everyone"), rolesInOrder(ACME, listOf("gone", "eng")).map { it.roleId })
    }

    @Test
    fun aLabelIsOneLineOfTheCoresThirtyTwoCharacters() {
        assertEquals(32, roleSentences.labelMax.toInt())
        assertEquals("Moderator", labelClipped("Moderator"))
        assertEquals("a".repeat(32), labelClipped("a".repeat(40)))
        assertEquals("Staff Engineer", labelClipped("Staff\nEngineer"))
        // Characters as a person counts them: a flag is one, though it is four UTF-16 units.
        assertEquals("🇨🇦".repeat(32), labelClipped("🇨🇦".repeat(33)))
    }

    // A role's editor

    private val mod = role("mod", name = "Moderator", color = "green", permissions = listOf("MANAGE_MESSAGES", "MANAGE_ROLES"))

    @Test
    fun aSwitchTurnedIsInTheDraftOnce() {
        assertEquals(listOf("A", "B"), turned(listOf("A"), "B", on = true))
        assertEquals(listOf("A", "B"), turned(listOf("B", "A"), "B", on = true))
        assertEquals(listOf("A"), turned(listOf("A", "B"), "B", on = false))
        assertEquals(listOf("A"), turned(listOf("A"), "B", on = false))
    }

    @Test
    fun anEditorOpensOnTheCoresDraftOfItsRoleTwice() {
        val editing = editingOf(mod)
        assertEquals(editing.opened, editing.draft)
        assertEquals("Moderator", editing.draft.name)
        assertEquals("green", editing.draft.color)
        assertFalse(rolesSavable(editing.draft, editing.opened))
        // A new role's: nothing in it, shown as a tag.
        val fresh = editingOf()
        assertEquals("", fresh.draft.name)
        assertFalse(fresh.draft.hideTag)
        assertTrue(fresh.draft.permissions.isEmpty())
    }

    @Test
    fun anEditorSomeoneChangedStaysAsItIsWhileTheRoleChangesUnderneath() {
        // Renamed here, not yet saved; meanwhile another admin switched Manage roles off.
        val editing = editingOf(mod).let { it.copy(draft = it.draft.copy(name = "Mods")) }
        val now = mod.copy(permissions = listOf("MANAGE_MESSAGES"))
        assertEquals(editing, followed(editing, now))
        assertTrue(rolesSavable(editing.draft, editing.opened))
        // Untouched, it is the role as it now is.
        assertEquals(editingOf(now), followed(editingOf(mod), now))
        // A switch turned and turned back is nothing changed, whatever order it left the permissions in.
        val turnedBack = editingOf(mod).let { it.copy(draft = it.draft.copy(permissions = turned(turned(it.draft.permissions, "MANAGE_MESSAGES", false), "MANAGE_MESSAGES", true))) }
        assertEquals(editingOf(now), followed(turnedBack, now))
    }

    @Test
    fun anEditorWithNoRoleHasNothingToFollowUntilItsRoleIsKnown() {
        // A launch that comes back to the editor: no snapshot yet, so no role.
        val editing = editingOf(null)
        assertEquals(editing, followed(editing, null))
        assertEquals(editingOf(mod), followed(editing, mod))
        // A new role has none to follow, whatever is typed in it.
        val named = editing.copy(draft = editing.draft.copy(name = "Design"))
        assertEquals(named, followed(named, null))
    }

    // A member's form

    @Test
    fun aFormOpensOnTheMemberAndTicksAndTitlesInPlace() {
        val ada = member("ada", roleIds = listOf("eng"), title = "On call")
        val editing = editingOf(ada)
        assertEquals(MemberForm(listOf("eng"), "On call"), editing.opened)
        assertEquals(editing.opened, editing.form)
        val changed = editing.ticked(rolesToggled(ACME, editing.form.roleIds, "design")).titled("Lead")
        assertEquals(MemberForm(listOf("eng", "design"), "Lead"), changed.form)
        assertEquals(editing.opened, changed.opened)
        val asked = rolesMemberChanges(changed.opened, changed.form)
        assertEquals(listOf("design"), asked.give)
        assertEquals(emptyList<String>(), asked.take)
        assertEquals("Lead", asked.title)
    }

    @Test
    fun aFormNobodyTouchedFollowsItsMember() {
        val ada = member("ada", roleIds = listOf("eng"), title = "On call")
        val now = ada.copy(roleIds = listOf("eng", "design"), title = "Lead")
        val followedNow = followed(editingOf(ada), now)
        assertEquals(editingOf(now), followedNow)
        assertFalse(rolesChangesAny(rolesMemberChanges(followedNow.opened, followedNow.form)))
        // A tick and its untick is nothing changed; a space typed after the title is, though nothing to save yet.
        val ticked = editingOf(ada).let { it.ticked(rolesToggled(ACME, rolesToggled(ACME, it.form.roleIds, "design"), "design")) }
        assertEquals(editingOf(now), followed(ticked, now))
        val spaced = editingOf(ada).titled("On call ")
        assertEquals(spaced, followed(spaced, now))
        assertFalse(rolesChangesAny(rolesMemberChanges(spaced.opened, spaced.form)))
    }

    // Finding members

    @Test
    fun aSearchReadsTheNameTheHandleAndTheTitle() {
        val people = listOf(
            member("devon", display = "Devon Park", title = "Staff Engineer"),
            member("maya", display = "Maya Chen"),
        )
        assertEquals(2, searched(people, "").size)
        assertEquals(2, searched(people, "  ").size)
        assertEquals(2, searched(people, "@").size)
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

    /** A failure as the session throws one: no code where the instance gave none (a transport failure). */
    private fun failure(message: String) = RuntimeException(message)

    @Test
    fun aFailureWithNoCodeIsSaidInItsOwnWords() {
        assertEquals("transport: offline", roleRefusal(failure("transport: offline"), "Acme"))
        assertEquals("nope", memberRefusal(failure("nope")))
    }

    @Test
    fun theInstancesCodeAndFieldReachTheCoresWords() {
        val taken = LumiaException.Api("ALREADY_EXISTS", "name taken", emptyMap())
        assertEquals("Another role in Acme has that name.", roleRefusal(taken, "Acme"))
        val colour = LumiaException.Api("INVALID_ARGUMENT", "bad colour", mapOf("field" to "color"))
        assertEquals("A role’s colour is one of the nine, or none.", roleRefusal(colour, "Acme"))
        val title = LumiaException.Api("INVALID_ARGUMENT", "bad title", mapOf("field" to "title"))
        assertEquals("A title is up to 32 characters on one line, with no hidden characters.", memberRefusal(title))
        // A code the core has no words for is said in the instance's.
        assertEquals("slow down", memberRefusal(LumiaException.Api("UNAVAILABLE", "slow down", emptyMap())))
    }

    @Test
    fun aRefusalOfTheNameIsSaidUnderItsField() {
        assertTrue(refusalIsOfName("ALREADY_EXISTS", null))
        assertTrue(refusalIsOfName("INVALID_ARGUMENT", "name"))
        assertFalse(refusalIsOfName("INVALID_ARGUMENT", "color"))
        assertFalse(refusalIsOfName("PERMISSION_DENIED", null))
        assertFalse(refusalIsOfName(null, null))
    }
}
