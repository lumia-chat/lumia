// Channel membership and server DMs (architecture 6.8 and 6.4): Browse,
// a channel's members and adding people to it, the picker a server DM is
// started from, a person's profile card, this account's own settings in a
// server, and the Default channels card of Server settings. The words are
// the ones decided for every client; what is offered follows the
// snapshot's permissions, and the instance decides.
package chat.lumia

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Tag
import androidx.compose.material.icons.filled.Group
import androidx.compose.material.icons.outlined.PersonRemove
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import chat.lumia.kit.AvatarTone
import chat.lumia.kit.LumiaAvatar
import chat.lumia.kit.LumiaCard
import chat.lumia.kit.LumiaEmptyState
import chat.lumia.kit.LumiaErrorText
import chat.lumia.kit.LumiaField
import chat.lumia.kit.LumiaLinkButton
import chat.lumia.kit.LumiaNotice
import chat.lumia.kit.LumiaPrimaryButton
import chat.lumia.kit.LumiaSecondaryButton
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTag
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.TAKES_NO_SERVER_DMS
import chat.lumia.kit.addCandidates
import chat.lumia.kit.addRefused
import chat.lumia.kit.addTag
import chat.lumia.kit.defaultChoices
import chat.lumia.kit.defaultsOf
import chat.lumia.kit.dmCandidates
import chat.lumia.kit.initialsOf
import chat.lumia.kit.lumiaCode
import chat.lumia.kit.lumiaDetails
import chat.lumia.kit.lumiaMessage
import chat.lumia.kit.memberCount
import chat.lumia.kit.nameOf
import chat.lumia.kit.removeRefused
import chat.lumia.kit.serverDmRefused
import chat.lumia.kit.serverSettingsRefused
import chat.lumia.kit.takesServerDms
import kotlinx.coroutines.launch
import uniffi.lumia_ffi.ChannelSummary
import uniffi.lumia_ffi.MemberSummary
import uniffi.lumia_ffi.ServerSummary

// Browse channels

/**
 * The public channels of a server this account is not in, each with its
 * topic and how many are in it, and Join. A row opens the channel to read
 * before joining; Join joins and opens it.
 */
@Composable
fun BrowseScreen(session: LumiaSession, server: ServerSummary?, back: () -> Unit, open: (String) -> Unit) {
    val scope = rememberCoroutineScope()
    var channels by remember { mutableStateOf<List<ChannelSummary>?>(null) }
    var error by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf<String?>(null) }
    val serverId = server?.serverId ?: ""

    LaunchedEffect(serverId) {
        if (serverId.isEmpty()) return@LaunchedEffect
        try { channels = session.browseChannels(serverId) } catch (e: Exception) { error = e.lumiaMessage }
    }

    fun join(c: ChannelSummary) {
        scope.launch {
            busy = c.channelId
            error = ""
            try {
                session.joinChannel(c.channelId)
                open(c.channelId)
            } catch (e: Exception) {
                error = e.lumiaMessage
            } finally {
                busy = null
            }
        }
    }

    PanelScreen(back = server?.name ?: "Server", onBack = back, title = "Browse channels", subtitle = server?.name) {
        LazyColumn(Modifier.fillMaxSize().navigationBarsPadding(), contentPadding = PaddingValues(LumiaTokens.space12), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
            if (error.isNotEmpty()) item { LumiaErrorText(error) }
            val list = channels
            if (list != null && list.isEmpty()) {
                item { LumiaEmptyState(Icons.Filled.Tag, "Nothing to browse", "You’re in every public channel here.") }
            }
            items(list ?: emptyList(), key = { it.channelId }) { c ->
                Row(
                    Modifier.fillMaxWidth().clip(RoundedCornerShape(LumiaTokens.radius12)).clickable(onClickLabel = "read #${c.name}") {
                        session.preview(c)
                        open(c.channelId)
                    }.padding(LumiaTokens.space10),
                    horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
                        Text("#${c.name}", style = LumiaTokens.textAuthor.style, color = LumiaTokens.inkEmphasis.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        if (c.topic.isNotEmpty()) Text(c.topic, style = LumiaTokens.textMember.style, color = LumiaTokens.inkSecondary.color, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        Text(memberCount(c.memberCount.toInt()), style = LumiaTokens.textMeta.style, color = LumiaTokens.inkQuiet.color)
                    }
                    LumiaSecondaryButton("Join", enabled = busy == null) { join(c) }
                }
            }
        }
    }
}

// A channel's members

/** A person in a list: avatar with their presence, name, a line under it, a tag. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun MemberRow(
    name: String,
    line: String?,
    tone: AvatarTone,
    status: String?,
    tag: String? = null,
    dim: Boolean = false,
    onClick: (() -> Unit)? = null,
    onLongClick: (() -> Unit)? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    val dot = when (status) {
        "ONLINE" -> LumiaTokens.online.color
        "AWAY" -> LumiaTokens.away.color
        "DO_NOT_DISTURB" -> LumiaTokens.unread.color
        else -> null
    }
    val tap = if (onClick != null || onLongClick != null) {
        Modifier.combinedClickable(enabled = onClick != null || onLongClick != null, onClick = { onClick?.invoke() }, onLongClick = onLongClick, onLongClickLabel = if (onLongClick != null) "more" else null)
    } else Modifier
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(LumiaTokens.radius10)).then(tap).padding(horizontal = LumiaTokens.space10, vertical = LumiaTokens.space6),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box {
            LumiaAvatar(initialsOf(name), tone, LumiaTokens.avatarMd)
            if (dot != null) {
                Box(Modifier.align(Alignment.BottomEnd).size(LumiaTokens.space10).background(LumiaTokens.surface.color, CircleShape).padding(LumiaTokens.space2).background(dot, CircleShape))
            }
        }
        Column(Modifier.weight(1f)) {
            Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space6), verticalAlignment = Alignment.CenterVertically) {
                Text(name, style = LumiaTokens.textMember.style, color = if (dim) LumiaTokens.inkQuiet.color else LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
                if (tag != null) LumiaTag(tag)
            }
            if (!line.isNullOrEmpty()) Text(line, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        trailing?.invoke()
    }
}

/**
 * Who is in a channel (architecture 6.8): Add people at the top, the
 * members, each opening their profile card, a long press offering to take
 * someone out to whoever may (`MANAGE_CHANNELS`), and Leave channel last.
 */
@Composable
fun ChannelMembersScreen(
    session: LumiaSession,
    channelId: String,
    devCard: String?,
    back: () -> Unit,
    push: (Screen) -> Unit,
    replace: (List<Screen>) -> Unit,
) {
    val scope = rememberCoroutineScope()
    val channel = session.channelOf(channelId)
    val server = session.snapshot?.servers?.firstOrNull { it.serverId == channel?.serverId }
    val name = channel?.name ?: "channel"
    val inIt = session.isIn(channelId)
    val members = session.serverMembers[channel?.serverId] ?: emptyList()
    var ids by remember { mutableStateOf<List<String>?>(null) }
    var error by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var menu by remember { mutableStateOf<String?>(null) }
    var card by remember { mutableStateOf(devCard) }

    suspend fun load() {
        try { ids = session.channelMembers(channelId) } catch (e: Exception) { error = e.lumiaMessage }
    }
    LaunchedEffect(channelId) { load() }

    fun remove(accountId: String) {
        scope.launch {
            busy = true
            error = ""
            try {
                session.removeChannelMember(channelId, accountId)
                load()
            } catch (e: Exception) {
                error = removeRefused(e.lumiaCode) ?: e.lumiaMessage
            } finally {
                busy = false
            }
        }
    }

    fun leave() {
        scope.launch {
            busy = true
            error = ""
            try {
                session.leaveChannel(channelId)
                replace(emptyList())
            } catch (e: Exception) {
                error = e.lumiaMessage
            } finally {
                busy = false
            }
        }
    }

    PanelScreen(back = "#$name", onBack = back, title = "Members", subtitle = "#$name") {
        LazyColumn(Modifier.fillMaxSize().navigationBarsPadding(), contentPadding = PaddingValues(LumiaTokens.space12), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
            if (inIt || server?.canManageChannels == true) {
                item { LumiaPrimaryButton("Add people", Modifier.fillMaxWidth().padding(bottom = LumiaTokens.space8)) { push(Screen.AddPeople(channelId)) } }
            }
            if (error.isNotEmpty()) item { LumiaErrorText(error) }
            items(ids ?: emptyList(), key = { it }) { id ->
                val m = members.firstOrNull { it.accountId == id }
                val me = id == session.me
                val canRemove = !me && server?.canManageChannels == true
                Box {
                    MemberRow(
                        name = session.nameOf(id),
                        line = m?.handle?.let { "@$it" },
                        tone = AvatarTone.of(id),
                        status = server?.let { session.presenceOf(id, it.serverId) },
                        tag = if (me) "You" else null,
                        onClick = { card = id },
                        onLongClick = if (canRemove) ({ menu = id }) else null,
                        trailing = if (canRemove) ({ MemberRemoveButton("Remove ${session.nameOf(id)} from #$name", enabled = !busy) { remove(id) } }) else null,
                    )
                    DropdownMenu(expanded = menu == id, onDismissRequest = { menu = null }, containerColor = LumiaTokens.surface.color) {
                        DropdownMenuItem(
                            text = { Text("Remove from #$name", style = LumiaTokens.textCompact.style, color = LumiaTokens.dnd.color) },
                            enabled = !busy,
                            onClick = { menu = null; remove(id) },
                        )
                    }
                }
            }
            if (inIt) {
                item { LumiaLinkButtonDanger("Leave channel", enabled = !busy, modifier = Modifier.padding(top = LumiaTokens.space12)) { leave() } }
            }
        }
    }

    val shown = card
    if (shown != null && server != null) {
        ProfileCard(session, server, shown, onDismiss = { card = null }, openChannel = { push(Screen.Channel(it)) }, openConversation = { push(Screen.Conversation(it)) })
    }
}

/** A link-styled button in the error colour, for what takes something away. */
@Composable
fun LumiaLinkButtonDanger(label: String, enabled: Boolean = true, modifier: Modifier = Modifier, onClick: () -> Unit) {
    androidx.compose.material3.TextButton(onClick = onClick, enabled = enabled, modifier = modifier) {
        Text(label, style = LumiaTokens.textLinkSm.style, color = LumiaTokens.dnd.color)
    }
}

// Add people

/**
 * Members of the server to add to a channel, several at once: everyone
 * not in it, found by name or handle; someone whose adds become
 * invitations is marked so.
 */
@Composable
fun AddPeopleScreen(session: LumiaSession, channelId: String, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    val channel = session.channelOf(channelId)
    val name = channel?.name ?: "channel"
    val members = session.serverMembers[channel?.serverId] ?: emptyList()
    var inChannel by remember { mutableStateOf<Set<String>?>(null) }
    var query by rememberSaveable { mutableStateOf("") }
    var chosen by rememberSaveable { mutableStateOf(listOf<String>()) }
    var error by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }

    LaunchedEffect(channelId) {
        try { inChannel = session.channelMembers(channelId).toSet() } catch (e: Exception) { error = e.lumiaMessage }
    }

    fun add() {
        scope.launch {
            busy = true
            error = ""
            try {
                session.addChannelMembers(channelId, chosen)
                back()
            } catch (e: Exception) {
                error = addRefused(e.lumiaCode) ?: e.lumiaMessage
            } finally {
                busy = false
            }
        }
    }

    val candidates = inChannel?.let { addCandidates(members, it, session.me, query) }
    PanelScreen(back = "Members", onBack = back, title = "Add people", subtitle = "#$name") {
        Column(Modifier.fillMaxSize().imePadding().navigationBarsPadding()) {
            LazyColumn(Modifier.weight(1f).fillMaxWidth(), contentPadding = PaddingValues(LumiaTokens.space12), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
                item { Box(Modifier.padding(bottom = LumiaTokens.space8)) { LumiaField("Find people", query, { query = it }, placeholder = "Name or handle") } }
                if (candidates != null && candidates.isEmpty()) {
                    item { LumiaEmptyState(Icons.Filled.Group, "No one to add", if (query.isBlank()) "Everyone in the server is in #$name." else "No one in the server answers to that.") }
                }
                items(candidates ?: emptyList(), key = { it.accountId }) { m ->
                    val on = m.accountId in chosen
                    Row(
                        Modifier.fillMaxWidth().clip(RoundedCornerShape(LumiaTokens.radius10))
                            .toggleable(value = on, role = Role.Checkbox) { chosen = if (it) chosen + m.accountId else chosen - m.accountId },
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(Modifier.weight(1f)) {
                            MemberRow(nameOf(m), "@${m.handle}", AvatarTone.of(m.accountId), null, tag = addTag(m))
                        }
                        Checkbox(checked = on, onCheckedChange = null, colors = CheckboxDefaults.colors(checkedColor = LumiaTokens.accent.color, checkmarkColor = LumiaTokens.onAccent.color, uncheckedColor = LumiaTokens.borderForm.color))
                    }
                }
            }
            Column(Modifier.padding(LumiaTokens.space16), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                if (error.isNotEmpty()) LumiaErrorText(error)
                LumiaPrimaryButton("Add", Modifier.fillMaxWidth(), enabled = !busy && chosen.isNotEmpty()) { add() }
            }
        }
    }
}

// A server DM's picker

/** Someone of the server to message here: anyone but whoever takes no server DMs, who is shown and not offered. */
@Composable
fun NewServerDmScreen(session: LumiaSession, server: ServerSummary?, back: () -> Unit, open: (String) -> Unit) {
    val scope = rememberCoroutineScope()
    var query by rememberSaveable { mutableStateOf("") }
    var error by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf<String?>(null) }
    val members = session.serverMembers[server?.serverId] ?: emptyList()
    val list = dmCandidates(members, session.me, query)

    fun message(m: MemberSummary) {
        val id = server?.serverId ?: return
        scope.launch {
            busy = m.accountId
            error = ""
            try {
                open(session.openServerDm(id, m.accountId))
            } catch (e: Exception) {
                error = serverDmRefused(e.lumiaCode, nameOf(m)) ?: e.lumiaMessage
            } finally {
                busy = null
            }
        }
    }

    PanelScreen(back = server?.name ?: "Server", onBack = back, title = "New message", subtitle = server?.name) {
        LazyColumn(Modifier.fillMaxSize().imePadding().navigationBarsPadding(), contentPadding = PaddingValues(LumiaTokens.space12), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
            item { Box(Modifier.padding(bottom = LumiaTokens.space8)) { LumiaField("Find a person", query, { query = it }, placeholder = "Name or handle") } }
            if (error.isNotEmpty()) item { LumiaErrorText(error) }
            items(list, key = { it.accountId }) { m ->
                val takes = takesServerDms(m)
                MemberRow(
                    name = nameOf(m),
                    line = if (takes) "@${m.handle}" else TAKES_NO_SERVER_DMS,
                    tone = AvatarTone.of(m.accountId),
                    status = server?.let { session.presenceOf(m.accountId, it.serverId) },
                    tag = if (takes) null else "Unavailable",
                    dim = !takes,
                    onClick = if (takes && busy == null) ({ message(m) }) else null,
                )
            }
        }
    }
}

// A person's profile card

/**
 * Who someone is, in a server (architecture 6.4): name, handle, pronouns,
 * bio; Message (a server DM) first, unless they take none here, and
 * Message privately (a personal DM) beside it. Nothing to do for oneself.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ProfileCard(
    session: LumiaSession,
    server: ServerSummary,
    accountId: String,
    onDismiss: () -> Unit,
    openChannel: (String) -> Unit,
    openConversation: (String) -> Unit,
) {
    val scope = rememberCoroutineScope()
    val member = session.serverMembers[server.serverId]?.firstOrNull { it.accountId == accountId } ?: session.profileOf(accountId)
    val name = session.nameOf(accountId)
    val me = accountId == session.me
    var note by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }

    fun message() {
        scope.launch {
            busy = true
            note = ""
            try {
                val id = session.openServerDm(server.serverId, accountId)
                onDismiss()
                openChannel(id)
            } catch (e: Exception) {
                note = serverDmRefused(e.lumiaCode, name) ?: e.lumiaMessage
            } finally {
                busy = false
            }
        }
    }

    fun messagePrivately() {
        scope.launch {
            busy = true
            note = ""
            try {
                val id = session.dmOpenOrRequest(accountId)
                if (id != null) {
                    onDismiss()
                    openConversation(id)
                } else {
                    note = "Request sent to $name. The conversation opens when they accept."
                }
            } catch (e: Exception) {
                note = if (e.lumiaCode == "ALREADY_EXISTS") "You already asked; waiting for their answer." else e.lumiaMessage
            } finally {
                busy = false
            }
        }
    }

    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = LumiaTokens.surface.color) {
        Column(Modifier.padding(start = LumiaTokens.space24, end = LumiaTokens.space24, bottom = LumiaTokens.space40), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space16)) {
            Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space14), verticalAlignment = Alignment.CenterVertically) {
                LumiaAvatar(initialsOf(name), AvatarTone.of(accountId), LumiaTokens.avatarLg)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
                    Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space6), verticalAlignment = Alignment.CenterVertically) {
                        Text(name, style = LumiaTokens.titleAuth.style, color = LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
                        if (me) LumiaTag("You")
                    }
                    member?.handle?.takeIf { it.isNotEmpty() }?.let { Text("@$it", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color) }
                }
            }
            val pronouns = member?.pronouns ?: ""
            val bio = member?.bio ?: ""
            if (pronouns.isNotEmpty() || bio.isNotEmpty()) {
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
                    if (pronouns.isNotEmpty()) Text(pronouns, style = LumiaTokens.textMember.style, color = LumiaTokens.inkSecondary.color)
                    if (bio.isNotEmpty()) Text(bio, style = LumiaTokens.textBody.style, color = LumiaTokens.inkBody.color)
                }
            }
            if (!me) {
                val dmsOff = member?.serverDmsOff == true
                if (dmsOff) Text(TAKES_NO_SERVER_DMS, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
                Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                    if (!dmsOff && server.canStartDms) LumiaPrimaryButton("Message", Modifier.weight(1f), enabled = !busy) { message() }
                    LumiaSecondaryButton("Message privately", Modifier.weight(1f), enabled = !busy) { messagePrivately() }
                }
                if (note.isNotEmpty()) LumiaNotice(Icons.Filled.Group, text = note)
            }
        }
    }
}

// This account's own settings in a server

/** The two switches a member keeps per server (architecture 6.8), each saved as it is turned. */
@Composable
fun YourSettingsScreen(session: LumiaSession, server: ServerSummary?, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    var error by rememberSaveable { mutableStateOf("") }
    var saved by rememberSaveable { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    // What the switches show while a change is on its way, and after one is refused, what the server has.
    var adds by remember(server?.channelAddsByInvitation) { mutableStateOf(!(server?.channelAddsByInvitation ?: false)) }
    var dms by remember(server?.serverDmsOff) { mutableStateOf(!(server?.serverDmsOff ?: false)) }

    fun save(byInvitation: Boolean?, dmsOff: Boolean?) {
        val id = server?.serverId ?: return
        scope.launch {
            busy = true
            error = ""
            saved = false
            try {
                session.updateMembership(id, byInvitation, dmsOff)
                saved = true
            } catch (e: Exception) {
                adds = !server.channelAddsByInvitation
                dms = !server.serverDmsOff
                error = e.lumiaMessage
            } finally {
                busy = false
            }
        }
    }

    PanelScreen(back = server?.name ?: "Server", onBack = back, title = "Your settings in ${server?.name ?: "the server"}") {
        Column(Modifier.fillMaxSize().navigationBarsPadding().padding(LumiaTokens.space16), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
            LumiaCard("Channels and messages") {
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space16)) {
                    SwitchRow("Let others add me to channels", "When off, being added becomes an invitation you can accept or dismiss.", adds, !busy && server != null) {
                        adds = it
                        save(byInvitation = !it, dmsOff = null)
                    }
                    SwitchRow("Let members of this server message me", "When off, people here can still message you privately.", dms, !busy && server != null) {
                        dms = it
                        save(byInvitation = null, dmsOff = !it)
                    }
                    if (error.isNotEmpty()) LumiaErrorText(error)
                    if (saved) Text("Saved.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
                }
            }
        }
    }
}

/** A switch with its label and the line under it, the whole row the switch to TalkBack. */
@Composable
private fun SwitchRow(label: String, caption: String, on: Boolean, enabled: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth().toggleable(value = on, enabled = enabled, role = Role.Switch, onValueChange = onChange),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
            Text(label, style = LumiaTokens.textMember.style, color = LumiaTokens.ink.color)
            Text(caption, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
        }
        Switch(checked = on, onCheckedChange = null, enabled = enabled, colors = lumiaSwitchColors())
    }
}

@Composable
fun lumiaSwitchColors() = SwitchDefaults.colors(
    checkedThumbColor = LumiaTokens.onAccent.color,
    checkedTrackColor = LumiaTokens.accent.color,
    uncheckedThumbColor = LumiaTokens.inkQuiet.color,
    uncheckedTrackColor = LumiaTokens.track.color,
    uncheckedBorderColor = LumiaTokens.borderForm.color,
)

// Default channels (Server settings)

/** The channels every newcomer joins: the server's public channels, ticked, and Save. */
@Composable
fun DefaultChannelsCard(session: LumiaSession, server: ServerSummary) {
    val scope = rememberCoroutineScope()
    var browsed by remember { mutableStateOf<List<ChannelSummary>?>(null) }
    var chosen by remember { mutableStateOf<Set<String>?>(null) }
    var error by rememberSaveable { mutableStateOf("") }
    var saved by rememberSaveable { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    val joined = session.snapshot?.channels ?: emptyList()

    LaunchedEffect(server.serverId) {
        try { browsed = session.browseChannels(server.serverId) } catch (e: Exception) { error = e.lumiaMessage }
    }
    val choices = browsed?.let { defaultChoices(joined, it, server.serverId) }
    val current = choices?.let { defaultsOf(it) }
    LaunchedEffect(current) { if (chosen == null && current != null) chosen = current }

    fun save() {
        val ids = choices?.map { it.channelId }?.filter { it in (chosen ?: emptySet()) } ?: return
        scope.launch {
            busy = true
            error = ""
            saved = false
            try {
                session.setDefaultChannels(server.serverId, ids)
                browsed = session.browseChannels(server.serverId)
                saved = true
            } catch (e: Exception) {
                error = serverSettingsRefused(e.lumiaCode, e.lumiaDetails["field"]) ?: e.lumiaMessage
            } finally {
                busy = false
            }
        }
    }

    LumiaCard("Default channels", "New members join these channels.") {
        Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
            for (c in choices ?: emptyList()) {
                val on = chosen?.contains(c.channelId) == true
                Row(
                    Modifier.fillMaxWidth().heightIn(min = LumiaTokens.controlHeight).toggleable(value = on, enabled = !busy, role = Role.Checkbox) {
                        chosen = if (it) (chosen ?: emptySet()) + c.channelId else (chosen ?: emptySet()) - c.channelId
                        saved = false
                    },
                    horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Checkbox(checked = on, onCheckedChange = null, colors = CheckboxDefaults.colors(checkedColor = LumiaTokens.accent.color, checkmarkColor = LumiaTokens.onAccent.color, uncheckedColor = LumiaTokens.borderForm.color))
                    Text("#${c.name}", style = LumiaTokens.textMember.style, color = LumiaTokens.ink.color)
                }
            }
            if (error.isNotEmpty()) LumiaErrorText(error)
            if (saved) Text("Saved.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
            LumiaPrimaryButton("Save", Modifier.fillMaxWidth(), enabled = !busy && chosen != null && chosen != current) { save() }
        }
    }
}

/**
 * A member row's Remove, as the dark Quiet Split draws it: `row-height` square (32) at radius 8,
 * the member-remove glyph in the faint ink, and while pressed (a phone has no pointer to hover)
 * the danger ink on its soft fill and border. The target reaches 44 (`tile-size`) for a finger.
 * The long press's menu does the same.
 */
@Composable
private fun MemberRemoveButton(label: String, enabled: Boolean, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val shape = RoundedCornerShape(LumiaTokens.radius8)
    Box(
        Modifier
            .size(LumiaTokens.tileSize)
            .clickable(interactionSource = interaction, indication = null, enabled = enabled, role = Role.Button, onClickLabel = label, onClick = onClick)
            .semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .size(LumiaTokens.rowHeight)
                .background(if (pressed) LumiaTokens.dangerSoft.color else Color.Transparent, shape)
                .border(1.dp, if (pressed) LumiaTokens.dangerLine.color else Color.Transparent, shape),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                Icons.Outlined.PersonRemove,
                contentDescription = null,
                tint = if (pressed) LumiaTokens.danger.color else LumiaTokens.inkFaint.color,
                modifier = Modifier.size(LumiaTokens.space16),
            )
        }
    }
}
