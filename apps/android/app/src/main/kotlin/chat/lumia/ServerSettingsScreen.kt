// A server's settings (architecture 6.7): its picture and name, how long
// it keeps its messages and whether its deleted ones keep their place,
// and its files: what they take, how long it keeps them and what it does
// when full (specs/media.md 4.5).
// Offered from the server's channel list to whoever may change them; the
// web's and the desktop's screen, in the vocabulary of Profile: a card for
// each, a Save for each (the switch saved as it is turned, as Your
// settings' are). Each change
// answers with the snapshot as it then is, so the rail and the header show
// it at once; other members hear of it through their account logs.
package chat.lumia

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.outlined.Group
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.VerifiedUser
import androidx.compose.material3.Icon
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import chat.lumia.kit.AvatarTone
import chat.lumia.kit.LumiaTag
import chat.lumia.kit.SettingsSection
import chat.lumia.kit.TAG_CARD_SUBTITLE
import chat.lumia.kit.TAG_CARD_TITLE
import chat.lumia.kit.TAG_NOTE
import chat.lumia.kit.TAG_ROLES_CAPTION
import chat.lumia.kit.TAG_ROLES_LABEL
import chat.lumia.kit.TAG_TITLES_CAPTION
import chat.lumia.kit.TAG_TITLES_LABEL
import chat.lumia.kit.grouped
import chat.lumia.kit.rolesCount
import chat.lumia.kit.settingsSections
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import chat.lumia.kit.FilesChange
import chat.lumia.kit.LumiaCard
import chat.lumia.kit.LumiaChoice
import chat.lumia.kit.LumiaErrorText
import chat.lumia.kit.LumiaField
import chat.lumia.kit.LumiaLinkButton
import chat.lumia.kit.LumiaPicture
import chat.lumia.kit.LumiaPrimaryButton
import chat.lumia.kit.LumiaSecondaryButton
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.PICTURE_MAX_BYTES
import chat.lumia.kit.PICTURE_TOO_LARGE
import chat.lumia.kit.Retention
import chat.lumia.kit.SERVER_NAME_REFUSED
import chat.lumia.kit.WhenFull
import chat.lumia.kit.initialsOf
import chat.lumia.kit.lumiaCode
import chat.lumia.kit.lumiaDetails
import chat.lumia.kit.lumiaMessage
import chat.lumia.kit.pictureRefused
import chat.lumia.kit.readAtMost
import chat.lumia.kit.serverNameChanged
import chat.lumia.kit.serverNameFits
import chat.lumia.kit.serverSettingsRefused
import chat.lumia.kit.storageWords
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.lumia_ffi.ServerSummary
import uniffi.lumia_ffi.StorageSummary

/** The picture in Overview: a rail tile's shape, larger (`tokens.json` has no name for it). */
private val PictureSize = LumiaTokens.tileSize + LumiaTokens.space12

/**
 * A server's settings as a phone has them (architecture 7.5, the
 * "RolesPhoneSettings" board): its sections as a screen of their own, each
 * pushed over it. Only the sections this account may use are listed
 * (`settingsSections`): Overview with Manage server, Roles with Manage
 * roles, Members with Manage members or Manage roles. Roles says how many
 * the server has at its row's end, Members how many members.
 */
@Composable
fun ServerSettingsScreen(session: LumiaSession, server: ServerSummary?, back: () -> Unit, push: (Screen) -> Unit) {
    val name = server?.name ?: ""
    val sections = server?.let { settingsSections(it) } ?: emptyList()
    val members = server?.let { session.serverMembers[it.serverId] }
    val shape = RoundedCornerShape(LumiaTokens.radius14)
    PanelScreen(back = name.ifEmpty { "Server" }, onBack = back, title = "Server settings", subtitle = name) {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).navigationBarsPadding().padding(LumiaTokens.space16)) {
            Column(Modifier.fillMaxWidth().clip(shape).border(1.dp, LumiaTokens.lineStrong.color, shape)) {
                sections.forEachIndexed { i, section ->
                    if (i > 0) Box(Modifier.fillMaxWidth().height(1.dp).background(LumiaTokens.line.color))
                    val (icon, note, screen) = when (section) {
                        SettingsSection.OVERVIEW -> Triple(Icons.Outlined.Settings, null, Screen.ServerOverview(server!!.serverId))
                        SettingsSection.ROLES -> Triple(Icons.Outlined.VerifiedUser, rolesCount(server!!.roles), Screen.Roles(server.serverId))
                        SettingsSection.MEMBERS -> Triple(Icons.Outlined.Group, members?.takeIf { it.isNotEmpty() }?.let { grouped(it.size.toLong()) }, Screen.ServerMembers(server!!.serverId))
                    }
                    Row(
                        Modifier.fillMaxWidth().height(SectionRow)
                            .clickable(role = Role.Button) { push(screen) }
                            .semantics(mergeDescendants = true) {}
                            .padding(horizontal = LumiaTokens.space14),
                        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space10),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(icon, contentDescription = null, tint = LumiaTokens.inkQuiet.color, modifier = Modifier.size(LumiaTokens.space18))
                        Text(section.label, style = LumiaTokens.textButton.style.copy(fontWeight = FontWeight(LumiaTokens.textMember.weight)), color = LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                        if (note != null) Text(note, style = LumiaTokens.textLinkSm.style.copy(fontWeight = FontWeight(LumiaTokens.textMeta.weight)), color = LumiaTokens.inkMuted.color)
                        Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null, tint = LumiaTokens.inkFaint.color, modifier = Modifier.size(LumiaTokens.space18))
                    }
                }
            }
        }
    }
}

/** A section's row on the sections' screen, the board's 52 high (`tokens.json` has no name for it). */
private val SectionRow = LumiaTokens.controlHeight + LumiaTokens.space6

/**
 * "Tag beside names" (architecture 7.5): what shows beside a member's name
 * in messages and in the member list, their role or their title, as two
 * pictured choices. Saved as it is chosen, as the switches are; refused or
 * not reached, the choice goes back to what the server has and the line
 * under it says why.
 */
@Composable
private fun TagCard(session: LumiaSession, server: ServerSummary) {
    val scope = rememberCoroutineScope()
    // What is chosen while a change is on its way, and after one is refused, what the server has.
    var titles by remember(server.tagTitles) { mutableStateOf(server.tagTitles) }
    var busy by remember { mutableStateOf(false) }
    var error by rememberSaveable { mutableStateOf("") }
    var saved by rememberSaveable { mutableStateOf(false) }

    fun choose(next: Boolean) {
        if (busy || next == titles) return
        titles = next
        scope.launch {
            busy = true
            error = ""
            saved = false
            try {
                session.setTagTitles(server.serverId, next)
                saved = true
            } catch (e: Exception) {
                titles = server.tagTitles
                error = serverSettingsRefused(e.lumiaCode, e.lumiaDetails["field"]) ?: e.lumiaMessage
            } finally {
                busy = false
            }
        }
    }

    LumiaCard(TAG_CARD_TITLE, TAG_CARD_SUBTITLE) {
        Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
            Row(Modifier.selectableGroup(), horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
                PictureChoice(TAG_ROLES_LABEL, TAG_ROLES_CAPTION, !titles, { choose(false) }) { modifier -> TagSketch(titles = false, modifier) }
                PictureChoice(TAG_TITLES_LABEL, TAG_TITLES_CAPTION, titles, { choose(true) }) { modifier -> TagSketch(titles = true, modifier) }
            }
            Text(TAG_NOTE, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
            if (error.isNotEmpty()) LumiaErrorText(error)
            if (saved) Text("Saved.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
        }
    }
}

/**
 * A small drawing of what the tag beside a name shows (the design's
 * `TagSketch`): two names, each a face and a bar, with a role's tag in its
 * tone or a title's on the plain chip.
 */
@Composable
private fun TagSketch(titles: Boolean, modifier: Modifier) {
    val rows = if (titles) {
        listOf(Triple(AvatarTone.PEACH, "Designer", null), Triple(AvatarTone.MINT, "Engineer", null))
    } else {
        listOf(Triple(AvatarTone.PEACH, "Admin", AvatarTone.INDIGO), Triple(AvatarTone.MINT, "Design", AvatarTone.PEACH))
    }
    Column(
        modifier.background(LumiaTokens.surface.color).padding(horizontal = LumiaTokens.space10).clearAndSetSemantics {},
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space10, Alignment.CenterVertically),
    ) {
        rows.forEachIndexed { i, (face, words, tone) ->
            Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space6), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(LumiaTokens.space14).clip(CircleShape).background(face.fill.color))
                Box(Modifier.width(if (i == 0) LumiaTokens.space28 else LumiaTokens.space24).height(LumiaTokens.space4).clip(RoundedCornerShape(LumiaTokens.radiusPill)).background(LumiaTokens.borderActive.color))
                LumiaTag(words, Modifier.weight(1f, fill = false), tone = tone, small = true)
            }
        }
    }
}

/**
 * The Overview section, pushed over the sections' screen: today's cards,
 * and "Tag beside names" after the first.
 */
@Composable
fun ServerOverviewScreen(session: LumiaSession, server: ServerSummary?, back: () -> Unit) {
    val context = LocalContext.current
    val focus = LocalFocusManager.current
    val scope = rememberCoroutineScope()
    val serverId = server?.serverId ?: ""
    val name = server?.name ?: ""
    val kept = server?.retentionDays?.toInt() ?: Retention.FOREVER

    var draft by rememberSaveable(serverId) { mutableStateOf(name) }
    var nameBusy by remember { mutableStateOf(false) }
    var nameError by rememberSaveable { mutableStateOf("") }
    var nameSaved by rememberSaveable { mutableStateOf(false) }

    var pictureBusy by remember { mutableStateOf(false) }
    var pictureError by rememberSaveable { mutableStateOf("") }

    var retention by rememberSaveable(serverId) { mutableStateOf(kept) }
    var retentionBusy by remember { mutableStateOf(false) }
    var retentionError by rememberSaveable { mutableStateOf("") }
    var retentionSaved by rememberSaveable { mutableStateOf(false) }

    // What the switch shows while a change is on its way, and after one is refused, what the server has.
    var showDeleted by remember(server?.showDeleted) { mutableStateOf(server?.showDeleted ?: false) }
    var showDeletedBusy by remember { mutableStateOf(false) }
    var showDeletedError by rememberSaveable { mutableStateOf("") }
    var showDeletedSaved by rememberSaveable { mutableStateOf(false) }

    fun pictureFailed(e: Exception) {
        pictureError = pictureRefused(e.lumiaCode, e.lumiaDetails["reason"]) ?: e.lumiaMessage
    }

    // The system's photo picker: nothing of the phone's pictures is asked
    // for but the one chosen. The instance makes its own square of it.
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        if (uri == null || serverId.isEmpty()) return@rememberLauncherForActivityResult
        scope.launch {
            pictureBusy = true
            pictureError = ""
            try {
                val (bytes, type) = withContext(Dispatchers.IO) {
                    val resolver = context.contentResolver
                    val bytes = resolver.openInputStream(uri)?.use { readAtMost(it, PICTURE_MAX_BYTES) }
                    bytes to (resolver.getType(uri) ?: "application/octet-stream")
                }
                // What the instance would refuse for its size is not sent at all.
                if (bytes == null) pictureError = PICTURE_TOO_LARGE else session.setServerIcon(serverId, bytes, type)
            } catch (e: Exception) {
                pictureFailed(e)
            } finally {
                pictureBusy = false
            }
        }
    }

    fun removePicture() {
        scope.launch {
            pictureBusy = true
            pictureError = ""
            try {
                session.removeServerIcon(serverId)
            } catch (e: Exception) {
                pictureFailed(e)
            } finally {
                pictureBusy = false
            }
        }
    }

    fun saveName() {
        val next = draft.trim()
        nameSaved = false
        if (!serverNameFits(next)) {
            nameError = SERVER_NAME_REFUSED
            return
        }
        scope.launch {
            nameBusy = true
            nameError = ""
            try {
                session.updateServer(serverId, next, null)
                draft = next
                nameSaved = true
                // The keyboard goes with the edit, and the rest of the screen is there again.
                focus.clearFocus()
            } catch (e: Exception) {
                nameError = serverSettingsRefused(e.lumiaCode, e.lumiaDetails["field"]) ?: e.lumiaMessage
            } finally {
                nameBusy = false
            }
        }
    }

    fun saveRetention() {
        val days = retention
        scope.launch {
            retentionBusy = true
            retentionError = ""
            retentionSaved = false
            try {
                session.updateServer(serverId, null, days)
                retentionSaved = true
            } catch (e: Exception) {
                retentionError = serverSettingsRefused(e.lumiaCode, e.lumiaDetails["field"]) ?: e.lumiaMessage
            } finally {
                retentionBusy = false
            }
        }
    }

    fun saveShowDeleted(show: Boolean) {
        val id = server?.serverId ?: return
        showDeleted = show
        scope.launch {
            showDeletedBusy = true
            showDeletedError = ""
            showDeletedSaved = false
            try {
                session.setShowDeleted(id, show)
                showDeletedSaved = true
            } catch (e: Exception) {
                showDeleted = server.showDeleted
                showDeletedError = serverSettingsRefused(e.lumiaCode, e.lumiaDetails["field"]) ?: e.lumiaMessage
            } finally {
                showDeletedBusy = false
            }
        }
    }

    val picture = server?.let { session.pictureOf(it.iconUrl) }
    PanelScreen(back = "Server settings", onBack = back, title = "Overview", subtitle = name) {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).navigationBarsPadding().imePadding().padding(LumiaTokens.space16), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
            LumiaCard("Overview") {
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
                    Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12), verticalAlignment = Alignment.CenterVertically) {
                        // The picture as the rail draws the server, over its initials.
                        Box(
                            Modifier
                                .size(PictureSize)
                                .clip(RoundedCornerShape(LumiaTokens.radius14))
                                // The rail's tile colour is the card's own; the choice's fill sets it apart.
                                .background(LumiaTokens.chip.color)
                                .border(1.dp, LumiaTokens.line.color, RoundedCornerShape(LumiaTokens.radius14))
                                .clearAndSetSemantics { contentDescription = if (picture != null) "$name's picture" else "$name has no picture" },
                            contentAlignment = Alignment.Center,
                        ) {
                            Text(initialsOf(name), style = LumiaTokens.titleMonogram.style, color = LumiaTokens.ink.color)
                            LumiaPicture(picture, Modifier.matchParentSize())
                        }
                        Column(horizontalAlignment = Alignment.Start) {
                            LumiaSecondaryButton("Choose a picture…", enabled = !pictureBusy && server != null) {
                                picker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
                            }
                            if (picture != null) LumiaLinkButton("Remove", enabled = !pictureBusy) { removePicture() }
                        }
                    }
                    if (pictureError.isNotEmpty()) LumiaErrorText(pictureError)
                    LumiaField("Name", draft, { draft = it; nameSaved = false; nameError = "" }, placeholder = "Our place")
                    if (nameError.isNotEmpty()) LumiaErrorText(nameError)
                    if (nameSaved) Text("Saved.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
                    LumiaPrimaryButton("Save", Modifier.fillMaxWidth(), enabled = !nameBusy && server != null && serverNameChanged(draft, name)) { saveName() }
                }
            }
            // Their role or their title beside a member's name (architecture 7.5), saved as it is chosen.
            if (server != null) TagCard(session, server)
            LumiaCard("Message history", "How long this server keeps its messages.") {
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
                    LumiaChoice(
                        Retention.choices(kept).map { it.toString() to Retention.label(it) },
                        retention.toString(),
                        { retention = it.toInt(); retentionSaved = false; retentionError = "" },
                    )
                    // Nothing is deleted until Save; this says what Save would delete.
                    Retention.warning(kept, retention)?.let { LumiaErrorText(it) }
                    if (retentionError.isNotEmpty()) LumiaErrorText(retentionError)
                    if (retentionSaved) Text("Saved.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
                    LumiaPrimaryButton("Save", Modifier.fillMaxWidth(), enabled = !retentionBusy && server != null && retention != kept) { saveRetention() }
                    // Saved as it is turned; the core's reads leave deleted messages out where it is off.
                    SwitchRow(
                        "Show deleted messages",
                        "Off, a deleted message disappears for everyone. On, it stays in its place as “(deleted)”.",
                        showDeleted,
                        !showDeletedBusy && server != null,
                    ) { saveShowDeleted(it) }
                    if (showDeletedError.isNotEmpty()) LumiaErrorText(showDeletedError)
                    if (showDeletedSaved) Text("Saved.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
                }
            }
            if (server != null) FilesCard(session, server)
            // The channels every newcomer joins (architecture 6.8).
            if (server != null) DefaultChannelsCard(session, server)
        }
    }
}

/**
 * Files (specs/media.md 4.5): what the server's files take, asked as the
 * screen opens ("1.2 GB of 5 GB used", "1.2 GB used" with no limit); how
 * long it keeps its attachments, Message history's choices; and what it
 * does when full. One Save sends what differs (`FilesChange`), and the
 * line in the error colour above it says first what a choice would
 * delete: nothing goes until Save.
 */
@Composable
private fun FilesCard(session: LumiaSession, server: ServerSummary) {
    val scope = rememberCoroutineScope()
    val serverId = server.serverId
    val keptDays = server.attachmentRetentionDays.toInt()
    val keptDeleteOldest = server.deleteOldestWhenFull

    var storage by remember(serverId) { mutableStateOf<StorageSummary?>(null) }
    var storageError by remember(serverId) { mutableStateOf("") }
    var days by rememberSaveable(serverId) { mutableStateOf(keptDays) }
    var deleteOldest by rememberSaveable(serverId) { mutableStateOf(keptDeleteOldest) }
    var busy by remember { mutableStateOf(false) }
    var error by rememberSaveable { mutableStateOf("") }
    var saved by rememberSaveable { mutableStateOf(false) }

    LaunchedEffect(serverId) {
        try {
            storage = session.serverStorage(serverId)
        } catch (e: Exception) {
            storageError = serverSettingsRefused(e.lumiaCode, e.lumiaDetails["field"]) ?: e.lumiaMessage
        }
    }

    val change = FilesChange.of(keptDays, keptDeleteOldest, days, deleteOldest)

    fun save() {
        scope.launch {
            busy = true
            error = ""
            saved = false
            try {
                session.setServerFiles(serverId, change.attachmentRetentionDays, change.deleteOldestWhenFull)
                saved = true
            } catch (e: Exception) {
                error = serverSettingsRefused(e.lumiaCode, e.lumiaDetails["field"]) ?: e.lumiaMessage
            } finally {
                busy = false
            }
        }
    }

    LumiaCard("Files", "What this server's attachments take, and how long it keeps them.") {
        Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
            storage?.let { Text(storageWords(it.usedBytes.toLong(), it.maxBytes.toLong()), style = LumiaTokens.textBody.style, color = LumiaTokens.ink.color) }
            if (storageError.isNotEmpty()) LumiaErrorText(storageError)
            Setting("Keep attachments for") {
                LumiaChoice(
                    Retention.choices(keptDays).map { it.toString() to Retention.label(it) },
                    days.toString(),
                    { days = it.toInt(); saved = false; error = "" },
                )
            }
            Setting("When the server is full") {
                LumiaChoice(
                    WhenFull.choices.map { (delete, label) -> delete.toString() to label },
                    deleteOldest.toString(),
                    { deleteOldest = it.toBoolean(); saved = false; error = "" },
                )
            }
            // Nothing is deleted until Save; these say what Save would start deleting.
            Retention.attachmentsWarning(keptDays, days)?.let { LumiaErrorText(it) }
            WhenFull.warning(keptDeleteOldest, deleteOldest)?.let { LumiaErrorText(it) }
            if (error.isNotEmpty()) LumiaErrorText(error)
            if (saved) Text("Saved.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
            LumiaPrimaryButton("Save", Modifier.fillMaxWidth(), enabled = !busy && change.changed) { save() }
        }
    }
}
