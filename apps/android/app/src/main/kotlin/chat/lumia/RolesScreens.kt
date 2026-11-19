// Roles and titles on a phone (architecture 7.5; the "Roles on phones"
// boards, drawn as iOS and carried over as the rest are: the same states,
// words and layout, Android's own back, sheets and menus): the Roles
// section, a server's roles in order with their counts, each moved from its
// More menu; a role's editor, pushed over the list, with one Save; "Add
// members" to a role; and the Members section, each member's row opening
// to their roles, their title and Kick. The core says what this account
// may do and the instance decides; `lumiakit`'s `Roles.kt` holds the rules
// and the words, the web's sheet.
package chat.lumia

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ArrowDownward
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.MoreHoriz
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.VisibilityOff
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import chat.lumia.kit.AvatarTone
import chat.lumia.kit.LOCKED_CHOICE
import chat.lumia.kit.LOCKED_ROLE
import chat.lumia.kit.LumiaAvatar
import chat.lumia.kit.LumiaDangerButton
import chat.lumia.kit.LumiaErrorText
import chat.lumia.kit.LumiaField
import chat.lumia.kit.LumiaInput
import chat.lumia.kit.LumiaNameLine
import chat.lumia.kit.LumiaPrimaryButton
import chat.lumia.kit.LumiaRoleDot
import chat.lumia.kit.LumiaSecondaryButton
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTag
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.NOT_SHOWN_AS_TAG
import chat.lumia.kit.addHoldersButton
import chat.lumia.kit.avatarTone
import chat.lumia.kit.editingOf
import chat.lumia.kit.followed
import chat.lumia.kit.grouped
import chat.lumia.kit.holderCandidates
import chat.lumia.kit.initialsOf
import chat.lumia.kit.labelClipped
import chat.lumia.kit.lumiaCode
import chat.lumia.kit.lumiaDetails
import chat.lumia.kit.memberRefusal
import chat.lumia.kit.nameOf
import chat.lumia.kit.refusalIsOfName
import chat.lumia.kit.rememberClock
import chat.lumia.kit.roleRefusal
import chat.lumia.kit.roleSentences
import chat.lumia.kit.rolesInOrder
import chat.lumia.kit.searched
import chat.lumia.kit.ticked
import chat.lumia.kit.timeOfMicros
import chat.lumia.kit.titled
import chat.lumia.kit.toneOf
import chat.lumia.kit.turned
import kotlinx.coroutines.launch
import uniffi.lumia_ffi.MemberForm
import uniffi.lumia_ffi.MemberForms
import uniffi.lumia_ffi.MemberSummary
import uniffi.lumia_ffi.PermissionChoice
import uniffi.lumia_ffi.RoleChoice
import uniffi.lumia_ffi.RoleDraft
import uniffi.lumia_ffi.RoleDrafts
import uniffi.lumia_ffi.RoleSummary
import uniffi.lumia_ffi.ServerSummary
import uniffi.lumia_ffi.SettingsSection
import uniffi.lumia_ffi.rolesAdministratorQuestion
import uniffi.lumia_ffi.rolesChangesAny
import uniffi.lumia_ffi.rolesChoices
import uniffi.lumia_ffi.rolesCount
import uniffi.lumia_ffi.rolesDeleteQuestion
import uniffi.lumia_ffi.rolesDeleteTitle
import uniffi.lumia_ffi.rolesHeld
import uniffi.lumia_ffi.rolesHolders
import uniffi.lumia_ffi.rolesLandsBefore
import uniffi.lumia_ffi.rolesLocked
import uniffi.lumia_ffi.rolesMemberChanges
import uniffi.lumia_ffi.rolesMemberMark
import uniffi.lumia_ffi.rolesMemberOffers
import uniffi.lumia_ffi.rolesMembersCount
import uniffi.lumia_ffi.rolesMovedBefore
import uniffi.lumia_ffi.rolesMovedWords
import uniffi.lumia_ffi.rolesNameHint
import uniffi.lumia_ffi.rolesOffersAny
import uniffi.lumia_ffi.rolesSavable
import uniffi.lumia_ffi.rolesSectionLabel
import uniffi.lumia_ffi.rolesStepBefore
import uniffi.lumia_ffi.rolesTagSwitchCaption
import uniffi.lumia_ffi.rolesToggled
import uniffi.lumia_ffi.rolesWithheld
import uniffi.lumia_ffi.rolesWithheldCaption

// What the boards draw and `tokens.json` has no name for, made from the nearest tokens.

/** A role's row, at least 56 high (`.lu-role`). */
private val RoleRowHeight = LumiaTokens.tileSize + LumiaTokens.space12

/** A small button and a role's More, 36 (`lu-btn--sm`, the More button's size). */
private val SmallControl = LumiaTokens.rowHeight + LumiaTokens.space4

/** A part's heading is 17 where a server's title is 19 (`.lu-roleedit__heading`). */
private const val HEADING = 17f / 19f

/** A hint under a field and a list's lead: 13.5 at the text's own weight (`.lu-settings__subtitle`). */
private val LeadStyle get() = LumiaTokens.textLabel.style.copy(fontWeight = FontWeight(LumiaTokens.textMeta.weight))

private fun refused(e: Exception, serverName: String): String = roleRefusal(e, serverName)

private fun refused(e: Exception): String = memberRefusal(e)

/**
 * A server's members asked for as one of these screens opens without them
 * ([members] none: their read with the snapshot not answered yet, or
 * failed). Until they are here a screen counts nobody: a role's row and
 * an editor's header say no number, and the delete question nothing of
 * who holds the role (`deleteQuestion`).
 */
@Composable
private fun AskMembers(session: LumiaSession, serverId: String, members: List<MemberSummary>?) {
    val unread = members == null
    LaunchedEffect(serverId, unread) { if (unread && serverId.isNotEmpty()) session.askMembers(serverId) }
}

// Roles

/**
 * A server's roles, highest first (the "RolesPhoneList" and
 * "RolesPhoneMenu" boards): a row each with its colour, its name, how many
 * hold it and "Not shown as tag", opening its editor; Everyone last, with
 * no More; a role at or above this account's own highest with a lock, and
 * it neither opens nor moves. A role is moved from its More menu, Move up
 * and Move down, never dragged: the whole order is sent as one
 * `reorderRoles`, drawn moved at once and put back with the refusal said
 * if the instance refuses. Delete role, from the menu too, asks first in a
 * sheet.
 */
@Composable
fun RolesScreen(session: LumiaSession, server: ServerSummary?, back: () -> Unit, push: (Screen) -> Unit) {
    val scope = rememberCoroutineScope()
    val view = LocalView.current
    val serverId = server?.serverId ?: ""
    val members = session.serverMembers[serverId]
    // The order as a move left it, drawn before the instance answers.
    var moved by remember { mutableStateOf<List<String>?>(null) }
    var error by rememberSaveable { mutableStateOf("") }
    var menu by remember { mutableStateOf<String?>(null) }
    var asking by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val roles = rolesInOrder(server?.roles ?: emptyList(), moved)
    // How many hold each role, asked of the core once for the list as it is, not for every row as it is drawn.
    val held = remember(roles, members) { roles.associate { it.roleId to rolesHolders(it, members) } }
    AskMembers(session, serverId, members)

    fun move(roleId: String, beforeId: String?) {
        if (server == null || beforeId == null || moved != null || !rolesLandsBefore(roles, roleId, beforeId)) return
        val order = rolesMovedBefore(roles, roleId, beforeId)
        val name = roles.firstOrNull { it.roleId == roleId }?.name ?: ""
        error = ""
        moved = order
        scope.launch {
            try {
                session.reorderRoles(server.serverId, order)
                view.announceForAccessibility(rolesMovedWords(name, (order.indexOf(roleId) + 1).toULong()))
            } catch (e: Exception) {
                error = refused(e, server.name)
            } finally {
                moved = null
            }
        }
    }

    fun delete(roleId: String) {
        if (server == null || busy) return
        scope.launch {
            busy = true
            error = ""
            try {
                session.deleteRole(server.serverId, roleId)
            } catch (e: Exception) {
                error = refused(e, server.name)
            } finally {
                busy = false
                asking = null
            }
        }
    }

    PanelScreen(back = "Server settings", onBack = back, title = rolesSectionLabel(SettingsSection.ROLES), subtitle = server?.let { "${it.name} · ${rolesCount(it.roles)}" }) {
        LazyColumn(
            Modifier.fillMaxSize().navigationBarsPadding(),
            contentPadding = PaddingValues(LumiaTokens.space16),
            verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6),
        ) {
            item { Text(roleSentences.rolesHintMenu, style = LeadStyle, color = LumiaTokens.inkMuted.color, modifier = Modifier.padding(bottom = LumiaTokens.space6)) }
            if (error.isNotEmpty()) item { LumiaErrorText(error) }
            items(roles, key = { it.roleId }) { role ->
                val moves = !role.isEveryone && !rolesLocked(role)
                RoleRow(role, held[role.roleId], onOpen = { push(Screen.RoleEditor(serverId, role.roleId)) }) {
                    if (moves) {
                        Box(Modifier.padding(end = LumiaTokens.space6)) {
                            IconButton(onClick = { menu = role.roleId }, modifier = Modifier.size(SmallControl)) {
                                Icon(Icons.Filled.MoreHoriz, contentDescription = "More for ${role.name}", tint = LumiaTokens.iconStrong.color, modifier = Modifier.size(LumiaTokens.space18))
                            }
                            RoleMenu(
                                role.name,
                                open = menu == role.roleId,
                                up = rolesStepBefore(roles, role.roleId, up = true),
                                down = rolesStepBefore(roles, role.roleId, up = false),
                                onDismiss = { menu = null },
                                onEdit = { push(Screen.RoleEditor(serverId, role.roleId)) },
                                onMove = { before -> move(role.roleId, before) },
                                onDelete = { asking = role.roleId },
                            )
                        }
                    }
                }
            }
            item {
                SmallButton("New role", Icons.Filled.Add, Modifier.fillMaxWidth().padding(top = LumiaTokens.space6)) { push(Screen.RoleEditor(serverId, null)) }
            }
        }
    }

    val asked = roles.firstOrNull { it.roleId == asking }
    if (asked != null) {
        AskSheet(rolesDeleteTitle(asked.name), rolesDeleteQuestion(held[asked.roleId]), confirm = "Delete role", danger = true, keep = "Keep it", busy = busy, onConfirm = { delete(asked.roleId) }) { asking = null }
    }
}

/**
 * A role's row (`.lu-role`): its colour as a dot, its name over how many
 * hold it, and where it leads; [end] is its More. One this account may not
 * manage is on the inset ground with a lock, and has nothing to press.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun RoleRow(role: RoleSummary, held: ULong?, onOpen: () -> Unit, end: @Composable () -> Unit) {
    val locked = rolesLocked(role)
    val shape = RoundedCornerShape(LumiaTokens.radius14)
    Row(
        Modifier.fillMaxWidth().heightIn(min = RoleRowHeight).clip(shape)
            .background(if (locked) LumiaTokens.inset.color else LumiaTokens.surface.color)
            .border(1.dp, LumiaTokens.lineStrong.color, shape),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            Modifier.weight(1f).heightIn(min = RoleRowHeight)
                .then(if (locked) Modifier else Modifier.clickable(onClickLabel = "edit", role = Role.Button, onClick = onOpen))
                .semantics(mergeDescendants = true) {}
                .padding(horizontal = LumiaTokens.space14, vertical = LumiaTokens.space8),
            horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            LumiaRoleDot(toneOf(role.color))
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2 / 2)) {
                Text(role.name, style = LumiaTokens.textButtonSecondary.style, color = if (locked) LumiaTokens.inkBody.color else LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
                    if (held != null) Text(rolesMembersCount(held), style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
                    if (role.hideTag && !role.isEveryone) {
                        Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space4), verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Outlined.VisibilityOff, contentDescription = null, tint = LumiaTokens.inkMuted.color, modifier = Modifier.size(LumiaTokens.space14))
                            Text(NOT_SHOWN_AS_TAG, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
                        }
                    }
                }
            }
            if (locked) Icon(Icons.Outlined.Lock, contentDescription = LOCKED_ROLE, tint = LumiaTokens.inkQuiet.color, modifier = Modifier.size(LumiaTokens.space16))
            else Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null, tint = LumiaTokens.inkFaint.color, modifier = Modifier.size(LumiaTokens.space18))
        }
        end()
    }
}

/**
 * A role's menu, from its More button (the design's menu, as the account
 * menu draws it): its name, Edit role, Move up and Move down, each faint
 * where there is no such step ([up] and [down] are the roles a step would
 * put it before), and Delete role in the danger ink.
 */
@Composable
private fun RoleMenu(name: String, open: Boolean, up: String?, down: String?, onDismiss: () -> Unit, onEdit: () -> Unit, onMove: (String) -> Unit, onDelete: () -> Unit) {
    DropdownMenu(
        expanded = open,
        onDismissRequest = onDismiss,
        containerColor = LumiaTokens.surface.color,
        shape = RoundedCornerShape(LumiaTokens.radius14),
        border = BorderStroke(1.dp, LumiaTokens.lineStrong.color),
    ) {
        Column(Modifier.width(MenuWidth).padding(horizontal = LumiaTokens.space6).semantics { paneTitle = name }, verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
            Text(name, style = LumiaTokens.textSection.style, color = LumiaTokens.inkMuted.color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(horizontal = LumiaTokens.space10, vertical = LumiaTokens.space6))
            MenuItem(Icons.Outlined.Settings, "Edit role") { onDismiss(); onEdit() }
            MenuRule()
            MenuItem(Icons.Filled.ArrowUpward, "Move up", enabled = up != null) { onDismiss(); up?.let(onMove) }
            MenuItem(Icons.Filled.ArrowDownward, "Move down", enabled = down != null) { onDismiss(); down?.let(onMove) }
            MenuRule()
            MenuItem(Icons.Outlined.Delete, "Delete role", danger = true) { onDismiss(); onDelete() }
        }
    }
}

/**
 * A question asked first, in a sheet (the "RolesPhoneDelete" board): what
 * is asked, what it costs in a sentence or two, the button that goes on in
 * its own words (in the danger colour for what cannot be taken back), and
 * the one that leaves things as they are.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun AskSheet(title: String, lines: List<String>, confirm: String, danger: Boolean, keep: String, busy: Boolean = false, onConfirm: () -> Unit, onDismiss: () -> Unit) {
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = LumiaTokens.surface.color, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(
            Modifier.fillMaxWidth().padding(start = LumiaTokens.space24, end = LumiaTokens.space24, bottom = LumiaTokens.space24).navigationBarsPadding().semantics { paneTitle = title },
            verticalArrangement = Arrangement.spacedBy(LumiaTokens.space16),
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                Text(title, style = LumiaTokens.titleChannel.style, color = LumiaTokens.ink.color, modifier = Modifier.semantics { heading() })
                Text(lines.joinToString(" "), style = LumiaTokens.textSubtitle.style, color = LumiaTokens.inkSecondary.color)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                if (danger) LumiaDangerButton(confirm, enabled = !busy, onClick = onConfirm) else LumiaPrimaryButton(confirm, enabled = !busy, onClick = onConfirm)
                LumiaSecondaryButton(keep, onClick = onDismiss)
            }
        }
    }
}

/** A small outlined button (`lu-btn--sm`): a glyph and its words, 36 high. */
@Composable
internal fun SmallButton(label: String, icon: ImageVector?, modifier: Modifier = Modifier, enabled: Boolean = true, onClick: () -> Unit) {
    val shape = RoundedCornerShape(LumiaTokens.radius10)
    Row(
        modifier.height(SmallControl).clip(shape)
            .background(LumiaTokens.raised.color)
            .border(1.dp, LumiaTokens.border.color, shape)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = LumiaTokens.space14),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) Icon(icon, contentDescription = null, tint = LumiaTokens.ink.color, modifier = Modifier.size(LumiaTokens.space16))
        Text(label, style = LumiaTokens.textLinkSm.style, color = LumiaTokens.ink.color, maxLines = 1)
    }
}

// A role's editor

private fun stored(draft: RoleDraft): List<Any> = listOf(draft.name, draft.color, draft.hideTag, ArrayList(draft.permissions))

private fun draftStored(stored: List<*>): RoleDraft = RoleDraft(stored[0] as String, stored[1] as String, stored[2] as Boolean, (stored[3] as List<*>).filterIsInstance<String>())

/** An editor through a turn of the phone: the role as it opened on it, and the draft. */
private val EditingSaver = listSaver<RoleDrafts, Any>(
    save = { stored(it.opened) + stored(it.draft) },
    restore = { RoleDrafts(draftStored(it.subList(0, 4)), draftStored(it.subList(4, 8))) },
)

/**
 * A role's editor (the "RolesPhoneEditor" boards): its name, its colour
 * (the nine tones and None), Show as tag with its preview, the permissions
 * under the core's headings, who holds it, and Delete role, with one Save
 * in the header for what is typed and switched. Administrator is set apart
 * and asks before it is turned on; a permission this account does not hold
 * is dimmed, says so, and is not turned. Who holds the role is changed at
 * once, a member at a time (Remove at a row's end, "Add members"). Delete
 * role asks first, in a sheet, and says how many hold it. With no [roleId]
 * it is a new role's: nothing in it yet, no holders and nothing to delete;
 * saved, it is that role's editor ([opened]). Everyone's is its
 * permissions only. A role deleted elsewhere takes its editor with it (the
 * shell drops the screen).
 *
 * The editor holds the role as it opened on it beside the draft
 * (`RoleDrafts`). Save is offered for a draft that differs from that,
 * never for what someone else changed of the role, and sends the
 * difference and nothing else (`LumiaSession.changeRole`): a rename here
 * does not switch back on a permission another admin switched off
 * meanwhile. Until something is changed here the editor follows the role
 * (`followed`); after a save it is the role as the answer has it.
 */
@Composable
fun RoleEditorScreen(session: LumiaSession, server: ServerSummary?, roleId: String?, back: () -> Unit, opened: (String) -> Unit, push: (Screen) -> Unit) {
    val scope = rememberCoroutineScope()
    val focus = LocalFocusManager.current
    val serverId = server?.serverId ?: ""
    val serverName = server?.name ?: ""
    val role = roleId?.let { id -> server?.roles?.firstOrNull { it.roleId == id } }
    val everyone = role?.isEveryone == true
    val members = session.serverMembers[serverId]
    // What the editor opened on and its draft, as last changed here; what is drawn and saved is that as the role has
    // changed underneath an editor nobody touched (`followed`). The role is not known until the snapshot is (a launch
    // that comes back to this screen): an editor opened on nothing follows to it once it is.
    var kept by rememberSaveable(roleId, stateSaver = EditingSaver) { mutableStateOf(editingOf(role)) }
    val editing = followed(kept, role)
    val draft = editing.draft
    var busy by remember { mutableStateOf(false) }
    var error by rememberSaveable { mutableStateOf("") }
    var nameError by rememberSaveable { mutableStateOf("") }
    var saved by rememberSaveable { mutableStateOf(false) }
    var holdersError by rememberSaveable { mutableStateOf("") }
    var holdersBusy by remember { mutableStateOf(false) }
    var askingAdmin by remember { mutableStateOf(false) }
    var askingDelete by remember { mutableStateOf(false) }
    AskMembers(session, serverId, members)

    val held = remember(role, members) { role?.let { rolesHolders(it, members) } }
    val holding = if (role != null && !everyone) (members ?: emptyList()).filter { role.roleId in it.roleIds } else emptyList()
    val shownName = when {
        role == null -> "New role"
        everyone -> role.name
        else -> draft.name.trim().ifEmpty { role.name }
    }
    // Against what the editor opened on, never the role as it now is: someone else's change is nothing to save here.
    val canSave = !busy && server != null && (roleId == null || role != null) && rolesSavable(draft, editing.opened.takeIf { role != null })

    // Nothing is changed while a save is on its way: its answer is what the editor then holds.
    fun change(next: RoleDraft) {
        if (busy) return
        saved = false
        kept = editing.copy(draft = next)
    }

    fun save() {
        if (!canSave) return
        val sent = editing
        scope.launch {
            busy = true
            error = ""
            nameError = ""
            saved = false
            try {
                if (role != null) {
                    // What was changed here and nothing else; the role as the answer has it is what the editor now opened on.
                    val answered = session.changeRole(server.serverId, role.roleId, was = sent.opened, now = sent.draft)
                    if (answered != null) kept = editingOf(answered)
                    saved = true
                    // The keyboard goes with the edit, and the rest of the screen is there again.
                    focus.clearFocus()
                } else {
                    val made = session.createRole(server.serverId, sent.draft)
                    if (made != null) opened(made) else back()
                }
            } catch (e: Exception) {
                val words = refused(e, server.name)
                if (refusalIsOfName(e.lumiaCode, e.lumiaDetails["field"])) nameError = words else error = words
            } finally {
                busy = false
            }
        }
    }

    // Administrator asks first; every other switch is the draft's at once.
    fun turn(name: String, on: Boolean) {
        if (on && name == roleSentences.administrator) askingAdmin = true
        else change(draft.copy(permissions = turned(draft.permissions, name, on)))
    }

    fun take(m: MemberSummary) {
        if (role == null || holdersBusy) return
        scope.launch {
            holdersBusy = true
            holdersError = ""
            try {
                // This role taken, and nothing else of theirs said: their other roles are as the instance has them.
                session.changeMember(serverId, m.accountId, take = listOf(role.roleId))
            } catch (e: Exception) {
                holdersError = refused(e)
            } finally {
                holdersBusy = false
            }
        }
    }

    fun delete() {
        if (role == null || busy) return
        scope.launch {
            busy = true
            error = ""
            try {
                // The role gone from the snapshot, the shell drops this screen.
                session.deleteRole(serverId, role.roleId)
            } catch (e: Exception) {
                error = refused(e, serverName)
            } finally {
                busy = false
                askingDelete = false
            }
        }
    }

    PanelScreen(
        back = "Roles",
        onBack = back,
        title = shownName,
        subtitle = if (held != null) "$serverName · ${rolesMembersCount(held)}" else serverName,
        trailing = { HeaderAction(Icons.Filled.Check, "Save", enabled = canSave) { save() } },
    ) {
        LazyColumn(Modifier.fillMaxSize().imePadding().navigationBarsPadding(), contentPadding = PaddingValues(LumiaTokens.space16)) {
            if (error.isNotEmpty()) item { Box(Modifier.padding(bottom = LumiaTokens.space12)) { LumiaErrorText(error) } }
            if (saved) item { Text("Saved.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color, modifier = Modifier.padding(bottom = LumiaTokens.space12)) }
            if (everyone) {
                item { Text(roleSentences.everyoneSubtitle, style = LeadStyle, color = LumiaTokens.inkMuted.color, modifier = Modifier.padding(bottom = LumiaTokens.space16)) }
            } else {
                item {
                    Column(Modifier.padding(bottom = LumiaTokens.space24), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space10)) {
                        Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
                            LumiaField("Name", draft.name, { nameError = ""; change(draft.copy(name = labelClipped(it))) }, placeholder = "Moderator")
                            if (nameError.isNotEmpty()) LumiaErrorText(nameError)
                            Text(rolesNameHint(serverName), style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
                        }
                        Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
                            Text("Colour", style = LumiaTokens.textLabel.style, color = LumiaTokens.inkBody.color)
                            ToneChoices(toneOf(draft.color)) { tone -> change(draft.copy(color = tone?.name?.lowercase() ?: "")) }
                        }
                        OutlinedGroup {
                            ToggleRow("Show as tag", rolesTagSwitchCaption(!draft.hideTag), !draft.hideTag) { shown -> change(draft.copy(hideTag = !shown)) }
                            GroupRule()
                            TagPreview(session, draft)
                        }
                    }
                }
            }
            item { Heading("Permissions", Modifier.padding(bottom = LumiaTokens.space10)) }
            for (group in session.rolePermissions) {
                item(key = "group ${group.title}") {
                    val row = @Composable { p: PermissionChoice, apart: Boolean ->
                        val kept = server == null || rolesWithheld(server.grantable, p.name)
                        ToggleRow(p.label, if (kept) rolesWithheldCaption(p.detail) else p.detail, p.name in draft.permissions, enabled = !kept, apart = apart) { on -> turn(p.name, on) }
                    }
                    Column(Modifier.padding(bottom = LumiaTokens.space10), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space10)) {
                        Text(group.title, style = LumiaTokens.textSection.style, color = LumiaTokens.inkMuted.color, modifier = Modifier.semantics { heading() })
                        // Administrator, set apart: the one that grants every other.
                        val (apart, rest) = group.permissions.partition { it.name == roleSentences.administrator }
                        if (apart.isNotEmpty()) OutlinedGroup(apart = true) { for (p in apart) row(p, true) }
                        OutlinedGroup {
                            rest.forEachIndexed { i, p ->
                                if (i > 0) GroupRule()
                                row(p, false)
                            }
                        }
                    }
                }
            }
            if (role != null && !everyone && members != null) {
                item {
                    Column(Modifier.padding(top = LumiaTokens.space14, bottom = LumiaTokens.space8), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space10)) {
                        Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8), verticalAlignment = Alignment.CenterVertically, modifier = Modifier.semantics(mergeDescendants = true) { heading() }) {
                            Heading("Members with this role")
                            Text(grouped(holding.size.toLong()), style = LumiaTokens.textLinkSm.style, color = LumiaTokens.inkMuted.color)
                        }
                        SmallButton("Add members", Icons.Filled.Add) { push(Screen.RoleHolders(serverId, role.roleId)) }
                        if (holdersError.isNotEmpty()) LumiaErrorText(holdersError)
                        if (holding.isEmpty()) Text(roleSentences.nobodyHolds, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
                    }
                }
                itemsIndexed(holding, key = { _, m -> "holder ${m.accountId}" }) { i, m ->
                    val name = nameOf(m)
                    val first = i == 0
                    val last = i == holding.lastIndex
                    Box(
                        Modifier.listEdge(first, last, rules = false)
                            .padding(start = LumiaTokens.space4, end = LumiaTokens.space4, top = if (first) LumiaTokens.space4 else 0.dp, bottom = if (last) LumiaTokens.space4 else 0.dp),
                    ) {
                        MemberRow(
                            name = name,
                            line = "@${m.handle}",
                            tone = AvatarTone.of(m.accountId),
                            status = null,
                            trailing = { MemberRemoveButton("Remove $name from ${role.name}", enabled = !holdersBusy) { take(m) } },
                            picture = session.avatarOf(m.accountId),
                        )
                    }
                }
            }
            if (role != null && !everyone) {
                item {
                    Column(Modifier.padding(top = LumiaTokens.space24), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space10)) {
                        Heading("Delete role")
                        Text(roleSentences.deleteRoleHint, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
                        LumiaDangerButton("Delete role", enabled = !busy) { askingDelete = true }
                    }
                }
            }
        }
    }

    if (askingAdmin) {
        AskSheet(
            roleSentences.administratorTitle,
            rolesAdministratorQuestion(serverName, shownName, held),
            confirm = "Turn on",
            danger = false,
            keep = "Cancel",
            onConfirm = {
                askingAdmin = false
                change(draft.copy(permissions = turned(draft.permissions, roleSentences.administrator, true)))
            },
        ) { askingAdmin = false }
    }
    if (askingDelete && role != null) {
        AskSheet(rolesDeleteTitle(role.name), rolesDeleteQuestion(held), confirm = "Delete role", danger = true, keep = "Keep it", busy = busy, onConfirm = { delete() }) { askingDelete = false }
    }
}

/** A header's action that is not always there to take: faint, and taking no tap, while it is not. */
@Composable
private fun HeaderAction(icon: ImageVector, label: String, enabled: Boolean, onClick: () -> Unit) {
    IconButton(onClick = onClick, enabled = enabled, modifier = Modifier.size(LumiaTokens.tileSize)) {
        Icon(icon, contentDescription = label, tint = if (enabled) LumiaTokens.iconStrong.color else LumiaTokens.inkFaint.color)
    }
}

/** A part's heading (`.lu-roleedit__heading`). */
@Composable
private fun Heading(text: String, modifier: Modifier = Modifier) {
    Text(text, style = LumiaTokens.titleServer.style.copy(fontSize = LumiaTokens.titleServer.size * HEADING), color = LumiaTokens.ink.color, modifier = modifier.semantics { heading() })
}

/**
 * An outlined group of rows (radius 14, the strong line), as Preferences'
 * are; [apart] is Administrator's, on the caution amber: the tone's fill
 * thinned over the surface, in a line of its ink thinned into it.
 */
@Composable
private fun OutlinedGroup(apart: Boolean = false, content: @Composable () -> Unit) {
    val shape = RoundedCornerShape(LumiaTokens.radius14)
    val fill = if (apart) lerp(LumiaTokens.surface.color, LumiaTokens.toneAmber.color, 0.6f) else Color.Transparent
    val line = if (apart) lerp(LumiaTokens.toneAmber.color, LumiaTokens.toneAmberInk.color, 0.22f) else LumiaTokens.lineStrong.color
    Column(Modifier.fillMaxWidth().clip(shape).background(fill).border(1.dp, line, shape)) { content() }
}

@Composable
private fun GroupRule() {
    Box(Modifier.fillMaxWidth().height(1.dp).background(LumiaTokens.line.color))
}

/**
 * A switch's row in a group: its name over the line that says what it
 * allows, the switch at the end, as Preferences draws one (the accent's
 * track on, the grey one off, the knob white both ways). The whole row
 * takes the tap and is the switch to TalkBack, named by its words. One
 * that is not this account's to turn ([enabled] false) is dimmed and takes
 * no tap; [apart] is Administrator's, its words in the amber's ink.
 */
@Composable
private fun ToggleRow(label: String, caption: String, on: Boolean, enabled: Boolean = true, apart: Boolean = false, onChange: (Boolean) -> Unit) {
    // The boards' knob is white in both themes.
    val knob = LumiaTokens.surface.light
    val track = if (on) LumiaTokens.accent.color else LumiaTokens.borderHover.color
    Row(
        Modifier.fillMaxWidth().toggleable(value = on, enabled = enabled, role = Role.Switch, onValueChange = onChange)
            .padding(horizontal = LumiaTokens.space16, vertical = LumiaTokens.space14),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space16),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
            Text(label, style = LumiaTokens.textMember.style, color = if (apart) LumiaTokens.toneAmberInk.color else LumiaTokens.ink.color)
            Text(caption, style = LumiaTokens.textMeta.style, color = if (apart) LumiaTokens.toneAmberInk.color else LumiaTokens.inkMuted.color)
        }
        Switch(
            checked = on,
            onCheckedChange = null,
            enabled = enabled,
            modifier = Modifier.alpha(if (enabled) 1f else 0.5f),
            // Any thumb content keeps the knob its full size when off.
            thumbContent = {},
            colors = SwitchDefaults.colors(
                checkedThumbColor = knob,
                checkedTrackColor = track,
                checkedBorderColor = Color.Transparent,
                uncheckedThumbColor = knob,
                uncheckedTrackColor = track,
                uncheckedBorderColor = Color.Transparent,
                disabledCheckedThumbColor = knob,
                disabledCheckedTrackColor = track,
                disabledCheckedBorderColor = Color.Transparent,
                disabledUncheckedThumbColor = knob,
                disabledUncheckedTrackColor = track,
                disabledUncheckedBorderColor = Color.Transparent,
            ),
        )
    }
}

/**
 * A role's colour to choose: the nine tones as the rail's swatches, then
 * None, a pill ringed in the accent while the role has no colour. They
 * wrap onto a second line where a phone is too narrow for the ten.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ToneChoices(chosen: AvatarTone?, onChoose: (AvatarTone?) -> Unit) {
    val pill = RoundedCornerShape(LumiaTokens.radiusPill)
    FlowRow(
        Modifier.selectableGroup().padding(vertical = LumiaTokens.space4),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space6),
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space10),
    ) {
        for ((i, tone) in AvatarTone.entries.withIndex()) {
            Box(Modifier.align(Alignment.CenterVertically)) { Swatch(tone, chosen = tone == chosen, index = i) { onChoose(tone) } }
        }
        Text(
            "None",
            style = LumiaTokens.textSection.style,
            color = LumiaTokens.inkSecondary.color,
            modifier = Modifier
                .align(Alignment.CenterVertically)
                .then(if (chosen == null) Modifier.border(LumiaTokens.space2, LumiaTokens.accent.color, pill).padding(LumiaTokens.space2 * 3 / 2) else Modifier.padding(LumiaTokens.space2 * 3 / 2))
                .clip(pill)
                .background(LumiaTokens.chip.color)
                .selectable(selected = chosen == null, role = Role.RadioButton) { onChoose(null) }
                .semantics { contentDescription = "No colour" }
                .padding(horizontal = LumiaTokens.space10, vertical = LumiaTokens.space4),
        )
    }
}

/**
 * The tag's preview under "Show as tag": this account's own name as a
 * message draws it, with the role's tag beside it while the role is shown
 * as one and has a name, and the time.
 */
@Composable
private fun TagPreview(session: LumiaSession, draft: RoleDraft) {
    val clock = rememberClock()
    val time = remember(clock) { timeOfMicros(System.currentTimeMillis() * 1000, clock) }
    val name = draft.name.trim()
    Row(
        Modifier.fillMaxWidth().padding(horizontal = LumiaTokens.space16, vertical = LumiaTokens.space12).clearAndSetSemantics {
            contentDescription = if (!draft.hideTag && name.isNotEmpty()) "Preview: ${session.selfName}, $name" else "Preview: ${session.selfName}, no tag"
        },
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("Preview", style = LumiaTokens.textSection.style, color = LumiaTokens.inkMuted.color, modifier = Modifier.padding(end = LumiaTokens.space4))
        LumiaAvatar(initialsOf(session.selfName), AvatarTone.of(session.me), LumiaTokens.avatarSm, session.avatarOf(session.me))
        LumiaNameLine(
            LumiaTokens.space8,
            Modifier.weight(1f),
            tag = if (!draft.hideTag && name.isNotEmpty()) ({ LumiaTag(name, tone = toneOf(draft.color)) }) else null,
            after = { Text(time, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color, maxLines = 1, softWrap = false) },
        ) {
            Text(session.selfName, style = LumiaTokens.textAuthor.style, color = LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

/**
 * The outline of a list drawn a row at a time (a lazy list has no box
 * around its rows to outline): each row its sides in the strong line, the
 * first its rounded top, the last its rounded foot, and, with [rules], the
 * line between it and the row before.
 */
private fun Modifier.listEdge(first: Boolean, last: Boolean, rules: Boolean = true): Modifier = composed {
    val stroke = LumiaTokens.lineStrong.color
    val rule = LumiaTokens.line.color
    drawBehind {
        val w = 1.dp.toPx()
        val h = w / 2
        val r = LumiaTokens.radius14.toPx()
        val path = Path()
        // Up the start side, round the top if it is the first, and down the end side.
        path.moveTo(h, if (last) size.height - r else size.height)
        path.lineTo(h, if (first) r else 0f)
        if (first) {
            path.arcTo(Rect(h, h, 2 * r + h, 2 * r + h), 180f, 90f, false)
            path.lineTo(size.width - r - h, h)
            path.arcTo(Rect(size.width - 2 * r - h, h, size.width - h, 2 * r + h), 270f, 90f, false)
        } else {
            path.moveTo(size.width - h, 0f)
        }
        path.lineTo(size.width - h, if (last) size.height - r else size.height)
        if (last) {
            path.arcTo(Rect(size.width - 2 * r - h, size.height - 2 * r - h, size.width - h, size.height - h), 0f, 90f, false)
            path.lineTo(r + h, size.height - h)
            path.arcTo(Rect(h, size.height - 2 * r - h, 2 * r + h, size.height - h), 90f, 90f, false)
        }
        drawPath(path, stroke, style = Stroke(w))
        if (rules && !first) drawLine(rule, Offset(w, h), Offset(size.width - w, h), w)
    }
}

// Add members to a role

/**
 * "Add members", from a role's editor: the server's members who do not
 * hold the role, found by name, handle or title, ticked and given it
 * together, a member at a time. A guest is offered no roles, so is not
 * listed.
 */
@Composable
fun RoleHoldersScreen(session: LumiaSession, server: ServerSummary?, roleId: String, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    val serverId = server?.serverId ?: ""
    val role = server?.roles?.firstOrNull { it.roleId == roleId }
    val members = session.serverMembers[serverId] ?: emptyList()
    var query by rememberSaveable { mutableStateOf("") }
    var chosen by rememberSaveable { mutableStateOf(listOf<String>()) }
    var error by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    val candidates = holderCandidates(members, roleId, query)

    fun add() {
        if (busy || chosen.isEmpty()) return
        scope.launch {
            busy = true
            error = ""
            try {
                for (accountId in chosen.toList()) {
                    // The role given, a member at a time, and nothing said of their others; one who has it already is as they were.
                    session.changeMember(serverId, accountId, give = listOf(roleId))
                    chosen = chosen - accountId
                }
                back()
            } catch (e: Exception) {
                error = refused(e)
            } finally {
                busy = false
            }
        }
    }

    PanelScreen(back = role?.name ?: "the role", onBack = back, title = "Add members", subtitle = role?.let { "To ${it.name}." }) {
        Column(Modifier.fillMaxSize().imePadding().navigationBarsPadding()) {
            LazyColumn(Modifier.weight(1f).fillMaxWidth(), contentPadding = PaddingValues(LumiaTokens.space12), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
                item { Box(Modifier.padding(start = LumiaTokens.space4, end = LumiaTokens.space4, bottom = LumiaTokens.space8)) { SearchField("Find a member", query) { query = it } } }
                if (candidates.isEmpty()) {
                    item { Text(if (query.isNotBlank()) roleSentences.noMemberMatches else roleSentences.everyoneHolds, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color, modifier = Modifier.padding(LumiaTokens.space10)) }
                }
                items(candidates, key = { it.accountId }) { m ->
                    val on = m.accountId in chosen
                    Row(
                        Modifier.fillMaxWidth().clip(RoundedCornerShape(LumiaTokens.radius10))
                            .toggleable(value = on, enabled = !busy, role = Role.Checkbox) { chosen = if (it) chosen + m.accountId else chosen - m.accountId },
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(Modifier.weight(1f)) { MemberRow(nameOf(m), "@${m.handle}", AvatarTone.of(m.accountId), null, picture = session.avatarOf(m.accountId)) }
                        Checkbox(checked = on, onCheckedChange = null, colors = lumiaCheckboxColors(), modifier = Modifier.padding(end = LumiaTokens.space10))
                    }
                }
            }
            Column(Modifier.padding(LumiaTokens.space16), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                if (error.isNotEmpty()) LumiaErrorText(error)
                LumiaPrimaryButton(addHoldersButton(chosen.size), Modifier.fillMaxWidth(), enabled = !busy && chosen.isNotEmpty()) { add() }
            }
        }
    }
}

@Composable
internal fun lumiaCheckboxColors() = CheckboxDefaults.colors(
    checkedColor = LumiaTokens.accent.color,
    checkmarkColor = LumiaTokens.onAccent.color,
    uncheckedColor = LumiaTokens.borderForm.color,
    disabledCheckedColor = LumiaTokens.inkFaint.color,
    disabledUncheckedColor = LumiaTokens.lineStrong.color,
)

/** A search field (the design's `SearchField`): the glyph, then what is typed; [label] is its placeholder and its name to TalkBack. */
@Composable
internal fun SearchField(label: String, value: String, onValue: (String) -> Unit) {
    val shape = RoundedCornerShape(LumiaTokens.radius12)
    Row(
        Modifier.fillMaxWidth().height(LumiaTokens.tileSize)
            .background(LumiaTokens.field.color, shape)
            .border(1.dp, LumiaTokens.borderForm.color, shape)
            .padding(horizontal = LumiaTokens.space12),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Filled.Search, contentDescription = null, tint = LumiaTokens.inkMuted.color, modifier = Modifier.size(LumiaTokens.space18))
        LumiaInput(value, onValue, label, modifier = Modifier.weight(1f).semantics { contentDescription = label })
    }
}

// Members

/**
 * A server's members in its settings (the "RolesPhoneMembers" and
 * "RolesPhoneMember" boards): found by name, handle or title among those
 * already held (the search asks the instance nothing), each with who they
 * are to the server ("Owner", "Guest"), their handle and title, and their
 * roles as tags under them. A row opens, under itself, to what of theirs
 * is this account's to change (`memberOffers`): their roles to tick, their
 * title, and Kick, which asks nothing first; a row with nothing to change
 * does not open.
 */
@Composable
fun ServerMembersScreen(session: LumiaSession, server: ServerSummary?, back: () -> Unit) {
    val serverId = server?.serverId ?: ""
    val members = session.serverMembers[serverId]
    var query by rememberSaveable { mutableStateOf("") }
    var open by rememberSaveable { mutableStateOf<String?>(null) }
    val found = remember(members, query) { searched(members ?: emptyList(), query) }
    AskMembers(session, serverId, members)

    PanelScreen(
        back = "Server settings",
        onBack = back,
        title = rolesSectionLabel(SettingsSection.MEMBERS),
        subtitle = server?.let { s -> if (members != null) "${grouped(members.size.toLong())} in ${s.name}" else s.name },
    ) {
        LazyColumn(Modifier.fillMaxSize().imePadding().navigationBarsPadding(), contentPadding = PaddingValues(LumiaTokens.space16)) {
            item { Box(Modifier.padding(bottom = LumiaTokens.space12)) { SearchField("Search members", query) { query = it } } }
            when {
                members == null -> item { Text(roleSentences.readingMembers, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color) }
                found.isEmpty() && query.isNotBlank() -> item { Text(roleSentences.noMemberMatches, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color) }
            }
            if (server != null) {
                itemsIndexed(found, key = { _, m -> m.accountId }) { i, m ->
                    val me = m.accountId == session.me
                    val offers = rolesMemberOffers(server, m, me)
                    val opens = rolesOffersAny(offers)
                    val isOpen = opens && open == m.accountId
                    Column(Modifier.fillMaxWidth().listEdge(first = i == 0, last = i == found.lastIndex)) {
                        RosterRow(session, server, m, opens, isOpen) { open = if (isOpen) null else m.accountId }
                        if (isOpen) MemberEditor(session, server, m, me) { open = null }
                    }
                }
            }
        }
    }
}

/** A member's row in the settings' list (`.lu-roster__row`, as a narrow one is laid out): the roles under the name and handle. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun RosterRow(session: LumiaSession, server: ServerSummary, m: MemberSummary, opens: Boolean, open: Boolean, onToggle: () -> Unit) {
    val name = nameOf(m)
    val mark = rolesMemberMark(m)
    val roles = rolesHeld(server.roles, m.roleIds)
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(LumiaTokens.radius14))
            .then(if (opens) Modifier.clickable(role = Role.Button, onClickLabel = if (open) "close" else "edit", onClick = onToggle) else Modifier)
            .semantics(mergeDescendants = true) { if (opens) stateDescription = if (open) "Expanded" else "Collapsed" }
            .padding(horizontal = LumiaTokens.space14, vertical = LumiaTokens.space10),
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12), verticalAlignment = Alignment.CenterVertically) {
            LumiaAvatar(initialsOf(name), AvatarTone.of(m.accountId), LumiaTokens.avatarMd, session.avatarOf(m.accountId))
            Column(Modifier.weight(1f)) {
                LumiaNameLine(LumiaTokens.space6, tag = mark?.let { word -> @Composable { LumiaTag(word, accent = true, small = true) } }) {
                    Text(name, style = LumiaTokens.textMember.style, color = LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Text(if (m.title.isEmpty()) "@${m.handle}" else "@${m.handle} · ${m.title}", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            if (opens) {
                Icon(
                    if (open) Icons.Filled.KeyboardArrowDown else Icons.AutoMirrored.Filled.KeyboardArrowRight,
                    contentDescription = null,
                    tint = if (open) LumiaTokens.inkQuiet.color else LumiaTokens.inkFaint.color,
                    modifier = Modifier.size(LumiaTokens.space18),
                )
            }
        }
        if (roles.isNotEmpty()) {
            FlowRow(
                Modifier.padding(start = LumiaTokens.avatarMd + LumiaTokens.space12),
                horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space6),
                verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4),
            ) {
                for (r in roles) LumiaTag(r.label, tone = avatarTone(r.tone))
            }
        }
    }
}

/** A member's form through a turn of the phone: their roles and title as it opened on them, and as it holds them. */
internal val MemberEditingSaver = listSaver<MemberForms, Any>(
    save = { listOf(ArrayList(it.opened.roleIds), it.opened.title, ArrayList(it.form.roleIds), it.form.title) },
    restore = {
        val ids = { kept: Any -> (kept as List<*>).filterIsInstance<String>() }
        MemberForms(MemberForm(ids(it[0]), it[1] as String), MemberForm(ids(it[2]), it[3] as String))
    },
)

/**
 * What a save of a member's form sends: one request for what the form
 * changed since it opened (the core's `rolesMemberChanges`), the roles to give and to
 * take and the title where it is another, which the instance does whole
 * or not at all; none for a form that changed nothing. Never the roles
 * as the form shows them, whole: one that someone else gave the member
 * meanwhile is not the form's to take.
 */
internal suspend fun saveMember(session: LumiaSession, server: ServerSummary, member: MemberSummary, editing: MemberForms) {
    val changes = rolesMemberChanges(editing.opened, editing.form)
    if (rolesChangesAny(changes)) session.changeMember(server.serverId, member.accountId, changes.give, changes.take, changes.title)
}

/**
 * A member's editing state under their row (the design's `MemberEdit`):
 * the roles to tick, the title, and Kick, each where `memberOffers` says.
 * Save keeps the roles and the title together, in one request, and closes
 * the row; Kick asks nothing first; a refusal is said over the buttons and
 * the row stays open. Until something of it is changed here the form
 * follows the member (`followed`).
 */
@Composable
private fun MemberEditor(session: LumiaSession, server: ServerSummary, member: MemberSummary, me: Boolean, close: () -> Unit) {
    val scope = rememberCoroutineScope()
    val offers = rolesMemberOffers(server, member, me)
    var kept by rememberSaveable(member.accountId, stateSaver = MemberEditingSaver) { mutableStateOf(editingOf(member)) }
    val editing = followed(kept, member)
    val form = editing.form
    var busy by remember { mutableStateOf(false) }
    var error by rememberSaveable(member.accountId) { mutableStateOf("") }

    fun act(what: suspend () -> Unit) {
        if (busy) return
        scope.launch {
            busy = true
            error = ""
            try {
                what()
                close()
            } catch (e: Exception) {
                error = refused(e)
            } finally {
                busy = false
            }
        }
    }

    Column(
        Modifier.fillMaxWidth().padding(start = LumiaTokens.space14, end = LumiaTokens.space14, top = LumiaTokens.space6, bottom = LumiaTokens.space16),
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space16),
    ) {
        if (offers.roles || offers.rolesNote != null) {
            Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
                Text("Roles", style = LumiaTokens.textLabel.style, color = LumiaTokens.inkBody.color)
                if (offers.roles) RoleChoicesList(rolesChoices(server.roles, form.roleIds), enabled = !busy) { id -> kept = editing.ticked(rolesToggled(server.roles, form.roleIds, id)) }
                else Text(offers.rolesNote ?: "", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
            }
        }
        if (offers.title) TitleField(form.title) { if (!busy) kept = editing.titled(it) }
        if (error.isNotEmpty()) LumiaErrorText(error)
        Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8), verticalAlignment = Alignment.CenterVertically) {
            if (offers.kick) KickButton("Kick ${nameOf(member)}", enabled = !busy) { act { session.removeMember(server.serverId, member.accountId) } }
            Spacer(Modifier.weight(1f))
            LumiaSecondaryButton("Cancel", onClick = close)
            LumiaPrimaryButton("Save", enabled = !busy) { act { saveMember(session, server, member, editing) } }
        }
    }
}

/** A title's field and the line under it: up to 32 characters, empty for none. */
@Composable
internal fun TitleField(title: String, onTitle: (String) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
        LumiaField("Title", title, { onTitle(labelClipped(it)) }, placeholder = "No title")
        Text(roleSentences.titleHint, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
    }
}

/**
 * A member's roles to tick, of the server's in order (the design's
 * `RoleChoices`): a box, the role's dot and its name; one not this
 * account's to give or take is shown with a lock and is not changed.
 */
@Composable
internal fun RoleChoicesList(choices: List<RoleChoice>, enabled: Boolean = true, onToggle: (String) -> Unit) {
    Column {
        for (c in choices) {
            Row(
                Modifier.fillMaxWidth().heightIn(min = SmallControl).clip(RoundedCornerShape(LumiaTokens.radius8))
                    .toggleable(value = c.checked, enabled = enabled && !c.locked, role = Role.Checkbox) { onToggle(c.id) }
                    .padding(horizontal = LumiaTokens.space8),
                horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space10),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Checkbox(checked = c.checked, onCheckedChange = null, enabled = !c.locked, colors = lumiaCheckboxColors())
                LumiaRoleDot(avatarTone(c.tone))
                Text(c.name, style = LumiaTokens.textCompact.style, color = if (c.locked) LumiaTokens.inkBody.color else LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                if (c.locked) Icon(Icons.Outlined.Lock, contentDescription = LOCKED_CHOICE, tint = LumiaTokens.inkFaint.color, modifier = Modifier.size(LumiaTokens.space14))
            }
        }
    }
}

/** Kick (`.lu-memberedit__kick`): the secondary button with the danger's line and ink; [label] names whom to TalkBack. */
@Composable
private fun KickButton(label: String, enabled: Boolean, onClick: () -> Unit) {
    OutlinedButton(
        onClick = onClick,
        enabled = enabled,
        modifier = Modifier.heightIn(min = LumiaTokens.controlHeight).semantics { contentDescription = label },
        shape = RoundedCornerShape(LumiaTokens.radius8),
        colors = ButtonDefaults.outlinedButtonColors(containerColor = LumiaTokens.raised.color, contentColor = LumiaTokens.danger.color),
        border = BorderStroke(1.dp, LumiaTokens.dangerLine.color),
    ) { Text("Kick", style = LumiaTokens.textButtonSecondary.style) }
}

/**
 * A control in a line of words, as the boards have none to make the line
 * taller: laid out only as tall as its words, keeping the design's 44 for
 * a finger beyond that.
 */
@Composable
internal fun TightTarget(height: Dp, content: @Composable () -> Unit) {
    Box(Modifier.height(height), contentAlignment = Alignment.Center) {
        Box(Modifier.wrapContentSize(unbounded = true)) { content() }
    }
}
