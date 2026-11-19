// Roles and titles (architecture 7.5): what a server's settings, a role's
// editor, a member's row and the profile card share, as plain values
// `src/test` checks on the JVM. The core says what this account may do
// (each role's and member's `canManage`, the server's `grantable` and its
// `canManage…` switches) and the instance decides every write; this reads
// those into what a screen offers, and holds the words every screen says
// the same way. The web's sheet (`apps/web/src/client/roles.ts`), carried
// over as it stands: the same answers and the same words.
package chat.lumia.kit

import uniffi.lumia_ffi.MemberSummary
import uniffi.lumia_ffi.RoleSummary
import uniffi.lumia_ffi.ServerSummary
import uniffi.lumia_ffi.TagSummary

/** The most characters a role's name or a title keeps. */
const val LABEL_MAX = 32

/**
 * What a name's or a title's field holds of what was typed or pasted into
 * it: one line, and no more than [LABEL_MAX] characters as a person counts
 * them (the web's `maxLength`), the rest let go.
 */
fun labelClipped(text: String): String {
    val line = text.replace('\n', ' ').replace('\r', ' ')
    if (characters(line) <= LABEL_MAX) return line
    val breaks = java.text.BreakIterator.getCharacterInstance()
    breaks.setText(line)
    var end = 0
    repeat(LABEL_MAX) { end = breaks.next() }
    return line.substring(0, end)
}

/** A role's or a tag's colour as a tone, one of the nine the design system names; none for none, and for anything that is no tone. */
fun toneOf(color: String): AvatarTone? = AvatarTone.entries.firstOrNull { it.stored == color }

/** A section of a server's settings. */
enum class SettingsSection(val label: String) {
    OVERVIEW("Overview"),
    ROLES("Roles"),
    MEMBERS("Members"),
}

/**
 * The sections of a server's settings this account may use, in order:
 * Overview with Manage server, Roles with Manage roles, and Members with
 * Manage members or Manage roles, since a member's roles are changed there
 * too. None, and the settings do not open.
 */
fun settingsSections(canManage: Boolean, canManageRoles: Boolean, canManageMembers: Boolean): List<SettingsSection> = buildList {
    if (canManage) add(SettingsSection.OVERVIEW)
    if (canManageRoles) add(SettingsSection.ROLES)
    if (canManageMembers || canManageRoles) add(SettingsSection.MEMBERS)
}

fun settingsSections(server: ServerSummary): List<SettingsSection> = settingsSections(server.canManage, server.canManageRoles, server.canManageMembers)

/** "5 roles": a server's roles counted without Everyone. */
fun rolesCount(roles: List<RoleSummary>): String {
    val n = roles.count { !it.isEveryone }
    return if (n == 1) "1 role" else "$n roles"
}

/** "1 member", "1,204 members". */
fun membersCount(n: Int): String = if (n == 1) "1 member" else "${grouped(n.toLong())} members"

/** How many of [members] hold a role; Everyone is held by all of them. None while the members are not known, which is not nobody. */
fun holders(role: RoleSummary, members: List<MemberSummary>?): Int? {
    if (members == null) return null
    return if (role.isEveryone) members.size else members.count { role.roleId in it.roleIds }
}

/** A role at or above this account's own highest: listed with a lock, and it does not open. Everyone always opens. */
fun lockedRole(role: RoleSummary): Boolean = !role.isEveryone && !role.canManage

/**
 * Where a role may land when it is moved: before [beforeId], a role this
 * account manages, or before Everyone, which is last. A role is never put
 * before one that is locked, since those are at or above this account's
 * own highest, and landing where it already is moves nothing.
 */
fun landsBefore(roles: List<RoleSummary>, roleId: String, beforeId: String): Boolean {
    val from = roles.indexOfFirst { it.roleId == roleId }
    val to = roles.indexOfFirst { it.roleId == beforeId }
    val moved = roles.getOrNull(from) ?: return false
    val before = roles.getOrNull(to) ?: return false
    if (lockedRole(moved) || moved.isEveryone) return false
    if (to == from || to == from + 1) return false
    return before.isEveryone || !lockedRole(before)
}

/** The whole order a move sends: every role but Everyone, with [roleId] now before [beforeId]. */
fun movedBefore(roles: List<RoleSummary>, roleId: String, beforeId: String): List<String> = buildList {
    for (r in roles) {
        if (r.roleId == roleId) continue
        if (r.roleId == beforeId) add(roleId)
        if (!r.isEveryone) add(r.roleId)
    }
}

/** The role a step up (-1) or down (1) puts [roleId] before; none where there is no such step (the highest it may go, or last above Everyone). */
fun stepBefore(roles: List<RoleSummary>, roleId: String, step: Int): String? {
    val at = roles.indexOfFirst { it.roleId == roleId }
    if (at < 0) return null
    val before = roles.getOrNull(if (step == -1) at - 1 else at + 2) ?: return null
    return before.roleId.takeIf { landsBefore(roles, roleId, it) }
}

/**
 * The roles as a move left them, drawn before the instance answers: those
 * of [order] in its order, then whatever it does not name (Everyone, and a
 * role made since) as the server has them. No order, and they are the
 * server's.
 */
fun rolesInOrder(roles: List<RoleSummary>, order: List<String>?): List<RoleSummary> {
    if (order == null) return roles
    val byId = roles.associateBy { it.roleId }
    return order.mapNotNull { byId[it] } + roles.filter { it.roleId !in order }
}

/** What a move says to a screen reader: "Design moved to position 3", its place in the order sent. */
fun roleMovedWords(name: String, order: List<String>, roleId: String): String = "$name moved to position ${order.indexOf(roleId) + 1}"

/** What a role's editor holds while it is open: its name, a tone or empty for none, the tag switch, and the permissions switched on, by their wire names. */
data class RoleDraft(val name: String = "", val color: String = "", val hideTag: Boolean = false, val permissions: List<String> = emptyList())

/** The editor's fields for a role, or for a new one: no name, no colour, shown as a tag, nothing granted. */
fun draftOf(role: RoleSummary? = null): RoleDraft =
    if (role == null) RoleDraft() else RoleDraft(role.name, role.color, role.hideTag, role.permissions.toList())

private fun sameSet(a: List<String>, b: List<String>): Boolean = a.size == b.size && a.all { it in b }

/**
 * Whether Save is offered: a new role once it has a name, a role once
 * something of it differs and it still has one. Of Everyone only the
 * permissions are compared, since nothing else of it changes.
 */
fun savable(draft: RoleDraft, role: RoleSummary? = null): Boolean {
    if (role == null) return draft.name.trim().isNotEmpty()
    if (role.isEveryone) return !sameSet(draft.permissions, role.permissions)
    if (draft.name.trim().isEmpty()) return false
    return draft.name.trim() != role.name || draft.color != role.color || draft.hideTag != role.hideTag || !sameSet(draft.permissions, role.permissions)
}

/** A permission this account does not hold, and so cannot give or take: its switch is shown and not turned. */
fun withheld(grantable: List<String>, permission: String): Boolean = permission !in grantable

/** The permission whose switch asks before it is turned on. */
const val ADMINISTRATOR = "ADMINISTRATOR"

/** A draft's permissions with [permission] switched on or off, each once. */
fun turned(permissions: List<String>, permission: String, on: Boolean): List<String> =
    if (on) permissions.filter { it != permission } + permission else permissions.filter { it != permission }

/** A role a member holds, as a card and a settings row list it: its name, on its tone or the plain chip. */
data class HeldRole(val roleId: String, val label: String, val tone: AvatarTone?)

/** Every role a member holds, in the server's order, without Everyone. */
fun rolesHeld(roles: List<RoleSummary>, roleIds: List<String>): List<HeldRole> =
    roles.filter { !it.isEveryone && it.roleId in roleIds }.map { HeldRole(it.roleId, it.name, toneOf(it.color)) }

/** One of a member's roles to tick. */
data class RoleChoice(val id: String, val name: String, val tone: AvatarTone?, val checked: Boolean, val locked: Boolean)

/** A member's roles to tick: the server's in order without Everyone, each not this account's to give or take marked. */
fun roleChoices(roles: List<RoleSummary>, held: List<String>): List<RoleChoice> =
    roles.filter { !it.isEveryone }.map { RoleChoice(it.roleId, it.name, toneOf(it.color), it.roleId in held, !it.canManage) }

/** The roles a tick leaves a member with; a locked role is not changed by one. */
fun toggledRole(roles: List<RoleSummary>, held: List<String>, roleId: String): List<String> {
    val role = roles.firstOrNull { it.roleId == roleId }
    if (role == null || role.isEveryone || !role.canManage) return held
    return if (roleId in held) held.filter { it != roleId } else held + roleId
}

/** Whether two sets of roles are one: what a save of a member's roles compares. */
fun sameRoles(a: List<String>, b: List<String>): Boolean = sameSet(a, b)

/**
 * What this account may change of a member, on their row in Members and
 * on their card: their [roles] to tick, with Manage roles, for anyone but
 * a guest; [rolesNote] in the roles' place, for a guest; their [title],
 * this account's own, or that of a member below it with Manage members;
 * and [kick], with Manage members, a member below this account, never
 * itself.
 */
data class MemberOffers(val roles: Boolean, val rolesNote: String?, val title: Boolean, val kick: Boolean)

fun memberOffers(canManageRoles: Boolean, guest: Boolean, canManage: Boolean, me: Boolean): MemberOffers = MemberOffers(
    roles = canManageRoles && !guest,
    rolesNote = if (canManageRoles && guest) GUEST_ROLES_NOTE else null,
    title = me || canManage,
    kick = !me && canManage,
)

fun memberOffers(server: ServerSummary, member: MemberSummary, me: Boolean): MemberOffers = memberOffers(server.canManageRoles, member.guest, member.canManage, me)

/** Whether a member's row in Members opens at all: there is something of theirs to change. */
fun offersAny(offers: MemberOffers): Boolean = offers.roles || offers.rolesNote != null || offers.title || offers.kick

/** The word beside a name on a card and in Members: the server's owner, or a guest. */
fun memberMark(owner: Boolean, guest: Boolean): String? = if (owner) "Owner" else if (guest) "Guest" else null

fun memberMark(member: MemberSummary): String? = memberMark(member.owner, member.guest)

/** A tag as it is drawn: its words, on its [tone] or, with none, the plain chip. */
data class TagLook(val label: String, val tone: AvatarTone?)

/** A member's tag as a message and a member row draw it; none for none, and for one with no words. */
fun tagOf(tag: TagSummary?): TagLook? = if (tag == null || tag.label.isEmpty()) null else TagLook(tag.label, toneOf(tag.color))

/** Members whose name, handle or title has [query] in it, whatever its capitals; all of them for nothing typed. */
fun searched(members: List<MemberSummary>, query: String): List<MemberSummary> {
    val q = query.trim().lowercase().removePrefix("@")
    if (q.isEmpty()) return members
    return members.filter { m -> listOf(m.displayName, m.nickname, m.handle, m.title).any { it.lowercase().contains(q) } }
}

/** Who "Add members" offers a role: the server's members who do not hold it, found by [query], by name. A guest is offered no roles, so is not listed. */
fun holderCandidates(members: List<MemberSummary>, roleId: String, query: String): List<MemberSummary> =
    searched(members.filter { !it.guest && roleId !in it.roleIds }, query).sortedBy { nameOf(it).lowercase() }

/** What the Add button of "Add members" says for [chosen] ticked. */
fun addHoldersButton(chosen: Int): String = if (chosen > 1) "Add $chosen members" else "Add"

// The words

const val ROLES_HINT = "In order, highest first. Drag a role to move it. A new role starts last, just above Everyone."

/** The same where a role is moved from its menu alone (a phone, a narrow window). */
const val ROLES_HINT_MENU = "In order, highest first. A role is moved from its menu. A new role starts last, just above Everyone."
const val EVERYONE_SUBTITLE = "What every member may do before any role adds to it. Guests are not among them."
const val TITLE_HINT = "Up to 32 characters. A title grants nothing."
const val GUEST_ROLES_NOTE = "Guests aren’t given roles."
const val DELETE_ROLE_HINT = "Its members lose what it allowed. This can’t be undone."
const val TAG_NOTE = "A profile shows a member’s title and roles either way."
const val TAG_ROLES_LABEL = "Roles"
const val TAG_ROLES_CAPTION = "Each member’s highest role that is shown as a tag, in the role’s colour."
const val TAG_TITLES_LABEL = "Titles"
const val TAG_TITLES_CAPTION = "Each member’s title, in no colour. A member without a title has no tag."
const val TAG_CARD_TITLE = "Tag beside names"
const val TAG_CARD_SUBTITLE = "What shows beside a member’s name in messages and in the member list."
const val NO_HOLDERS = "Nobody holds this role yet."
const val NO_MEMBER_MATCHES = "No member matches that."
const val NO_ONE_TO_ADD = "Everyone who can hold this role already does."
const val READING_MEMBERS = "Reading the members…"
const val NOT_SHOWN_AS_TAG = "Not shown as tag"
const val LOCKED_ROLE = "At or above your own highest role, so not yours to change"
const val LOCKED_CHOICE = "Not yours to give or take"

fun roleNameHint(serverName: String): String = "1 to 32 characters. No other role in $serverName can have the same name."

/** Under "Show as tag", as it stands. */
fun tagSwitchCaption(shown: Boolean): String =
    if (shown) "This role can be the tag beside a member’s name." else "This role only carries permissions. It is nobody’s tag."

/** A permission's line where its switch is not this account's to turn. */
fun withheldCaption(detail: String): String = "$detail You don’t hold it, so it is not yours to give or take."

/** What is asked before Administrator is turned on: the lines under "Turn on Administrator?". A new role has no holders to count. */
fun administratorQuestion(serverName: String, roleName: String, held: Int?): List<String> = buildList {
    add("Administrator allows everything in $serverName, in every channel, whatever a channel’s own permissions say.")
    if (held != null && held > 0) add(if (held == 1) "1 member holds $roleName." else "${grouped(held.toLong())} members hold $roleName.")
}

/**
 * What is asked before a role is deleted: the lines under "Delete
 * Moderator?". How many hold it is said only where it is known: with the
 * members not read, still on their way or not to be had, [held] is none
 * and nothing is said of holders, since "Nobody" would be a guess about a
 * role that forty may hold.
 */
fun deleteQuestion(held: Int?): List<String> = buildList {
    when {
        held == null -> Unit
        held == 0 -> add("Nobody holds this role.")
        held == 1 -> add("1 member holds this role. They lose what it allowed.")
        else -> add("${grouped(held.toLong())} members hold this role. They lose what it allowed.")
    }
    add("This can’t be undone.")
}

/** Why a role was not saved, moved or deleted: the API's [code] and `details.field`, else the failure's own [message]. */
fun roleRefusal(code: String?, field: String?, message: String, serverName: String): String = when {
    code == "PERMISSION_DENIED" -> "That isn’t yours to change. A role at or above your own highest, or a permission you don’t hold, is not yours to give."
    code == "ALREADY_EXISTS" -> "Another role in $serverName has that name."
    code == "NOT_FOUND" -> ROLE_GONE
    code == "INVALID_ARGUMENT" && field == "name" -> "A role’s name is 1 to 32 characters on one line, with no hidden characters, and can’t be “everyone”."
    code == "INVALID_ARGUMENT" && field == "color" -> "A role’s colour is one of the nine, or none."
    code == "INVALID_ARGUMENT" && field == "role_ids" -> "The roles changed while you were moving one. Try again."
    code == "FAILED_PRECONDITION" -> "This server has as many roles as it can hold."
    else -> message
}

const val ROLE_GONE = "That role is gone. Someone deleted it."

/** Whether a role's refusal is about its name, and so is said under that field. */
fun refusalIsOfName(code: String?, field: String?): Boolean = code == "ALREADY_EXISTS" || (code == "INVALID_ARGUMENT" && field == "name")

/**
 * Why a member's roles or title were not saved, or they were not kicked.
 * `RATE_LIMITED` is the instance's limit on a member's own title, ten
 * changes an hour in a server; nothing else here is held to one.
 */
fun memberRefusal(code: String?, field: String?, message: String): String = when {
    code == "PERMISSION_DENIED" -> "That isn’t yours to change. They may be at or above your own highest role."
    code == "NOT_FOUND" -> "They are no longer a member."
    code == "FAILED_PRECONDITION" -> "A member can’t hold that many roles."
    code == "INVALID_ARGUMENT" && field == "title" -> "A title is up to 32 characters on one line, with no hidden characters."
    code == "RATE_LIMITED" -> "You’ve changed your title too many times this hour. Try again later."
    else -> message
}
