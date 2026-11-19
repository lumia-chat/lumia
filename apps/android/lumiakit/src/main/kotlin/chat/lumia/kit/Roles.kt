// Roles and titles (architecture 7.5): what of them is the phone's own.
// Every rule and every word the roles screens follow is the core's
// (`lumia_client::roles`, held to `proto/vectors/roles/cases.json`), asked
// through `lumia-ffi`'s `roles…` functions and `roleWords`; the screens call
// those directly. What is here is what the phone keeps for itself: a tone
// as the colour type it draws with, a field held to one line of the core's
// length, a move drawn before it is answered, a permission switched in a
// draft, the editors' state as a screen keeps it, "Add members"' list in
// order and its button, the words the core has no sentence for,
// and the session's failures handed to the core's refusals. `src/test`
// checks them on the JVM, asking the core itself as `ViewerTest` does.
package chat.lumia.kit

import uniffi.lumia_ffi.MemberForm
import uniffi.lumia_ffi.MemberForms
import uniffi.lumia_ffi.MemberSummary
import uniffi.lumia_ffi.RoleDrafts
import uniffi.lumia_ffi.RoleSummary
import uniffi.lumia_ffi.RoleWords
import uniffi.lumia_ffi.roleWords
import uniffi.lumia_ffi.rolesDraftOf
import uniffi.lumia_ffi.rolesFollowed
import uniffi.lumia_ffi.rolesFormOf
import uniffi.lumia_ffi.rolesHolderCounts
import uniffi.lumia_ffi.rolesMayBeGiven
import uniffi.lumia_ffi.rolesMemberFollowed
import uniffi.lumia_ffi.rolesMemberRefusal
import uniffi.lumia_ffi.rolesRoleRefusal
import uniffi.lumia_ffi.rolesSearched
import uniffi.lumia_ffi.rolesToneOf

/** Every fixed sentence the roles screens say, the core's, asked once (and not before a screen needs one, so the library is loaded by then). */
val roleSentences: RoleWords by lazy { roleWords() }

/** A tone the core named (a held role's, a role to tick, a tag's) as the phone draws it; none for the plain chip. */
fun avatarTone(tone: String?): AvatarTone? = tone?.let { t -> AvatarTone.entries.firstOrNull { it.stored == t } }

/** A role's colour as one of the nine tones, by the core's `rolesToneOf`; none for none, and for anything that is no tone. */
fun toneOf(color: String): AvatarTone? = avatarTone(rolesToneOf(color))

/**
 * What a name's or a title's field holds of what was typed or pasted into
 * it: one line, and no more than the core's `labelMax` characters as a
 * person counts them (the web's `maxLength`), the rest let go.
 */
fun labelClipped(text: String): String {
    val line = text.replace('\n', ' ').replace('\r', ' ')
    val most = roleSentences.labelMax.toInt()
    if (characters(line) <= most) return line
    val breaks = java.text.BreakIterator.getCharacterInstance()
    breaks.setText(line)
    var end = 0
    repeat(most) { end = breaks.next() }
    return line.substring(0, end)
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

/** A draft's permissions with [permission] switched on or off, each once. */
fun turned(permissions: List<String>, permission: String, on: Boolean): List<String> =
    if (on) permissions.filter { it != permission } + permission else permissions.filter { it != permission }

/** An editor as it opens on a role, or on a new one (none): the core's draft of it, twice, nothing of it changed yet. */
fun editingOf(role: RoleSummary? = null): RoleDrafts = rolesDraftOf(role).let { RoleDrafts(it, it) }

/**
 * An editor as the role changes underneath it, by the core's
 * `rolesFollowed`: one nobody has touched is the role as it now is, one
 * somebody has stays as it is. With no role (a new one, or one not known
 * yet) there is nothing to follow.
 */
fun followed(editing: RoleDrafts, role: RoleSummary?): RoleDrafts =
    if (role == null) editing else rolesFollowed(editing.opened, editing.draft, rolesDraftOf(role))

/** A member's form as it opens on them: the core's form of them, twice, nothing of theirs changed yet. */
fun editingOf(member: MemberSummary): MemberForms = rolesFormOf(member).let { MemberForms(it, it) }

/** A member's form as the member changes underneath it, by the core's `rolesMemberFollowed`. */
fun followed(editing: MemberForms, member: MemberSummary): MemberForms =
    rolesMemberFollowed(editing.opened, editing.form, rolesFormOf(member))

/** A form with [roleIds] ticked, its title as it was. */
fun MemberForms.ticked(roleIds: List<String>): MemberForms = copy(form = MemberForm(roleIds, form.title))

/** A form with [title] typed, its roles as they were. */
fun MemberForms.titled(title: String): MemberForms = copy(form = MemberForm(form.roleIds, title))

/**
 * Members Members' search finds by [query]: the core's `rolesSearched`,
 * asked once for the whole list and not once a member, its places read back
 * into the list; all of them for nothing typed, with nothing asked.
 */
fun searched(members: List<MemberSummary>, query: String): List<MemberSummary> =
    if (query.isBlank()) members else rolesSearched(query, members).mapNotNull { members.getOrNull(it.toInt()) }

/**
 * How many of [members] hold each of [roles], by role id: the core's
 * `rolesHolderCounts`, asked once for the whole list and not once a role;
 * none while the members are not known.
 */
fun holderCounts(roles: List<RoleSummary>, members: List<MemberSummary>?): Map<String, ULong>? =
    members?.let { roles.map { r -> r.roleId }.zip(rolesHolderCounts(roles, it)).toMap() }

/** Who "Add members" offers a role: those the core's `rolesMayBeGiven` says may be (no guest, nobody who holds it), found by [query], by name. */
fun holderCandidates(members: List<MemberSummary>, roleId: String, query: String): List<MemberSummary> =
    searched(members, query).filter { rolesMayBeGiven(roleId, it) }.sortedBy { nameOf(it).lowercase() }

/** What the Add button of "Add members" says for [chosen] ticked. */
fun addHoldersButton(chosen: Int): String = if (chosen > 1) "Add $chosen members" else "Add"

// The words the core has no sentence for

const val LOCKED_ROLE = "At or above your own highest role, so not yours to change"
const val LOCKED_CHOICE = "Not yours to give or take"

// Refusals: the session's failure, in the core's words

/** Why a role was not saved, moved or deleted: the API's code and `details.field`, else the failure's own message. */
fun roleRefusal(e: Throwable, serverName: String): String = rolesRoleRefusal(e.lumiaCode ?: "", e.lumiaDetails["field"] ?: "", e.lumiaMessage, serverName)

/** Why a member's roles or title were not saved, or they were not kicked. */
fun memberRefusal(e: Throwable): String = rolesMemberRefusal(e.lumiaCode ?: "", e.lumiaDetails["field"] ?: "", e.lumiaMessage)

/** Whether a role's refusal is about its name, and so is said under that field. */
fun refusalIsOfName(code: String?, field: String?): Boolean = code == "ALREADY_EXISTS" || (code == "INVALID_ARGUMENT" && field == "name")
