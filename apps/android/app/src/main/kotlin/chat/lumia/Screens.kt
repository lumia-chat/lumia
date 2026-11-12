// The screens pushed over the rail and sidebar, each a floating panel: a
// conversation, a channel, requests, a new message, the person's profile,
// preferences, devices. The same
// states, wording and error handling as the web's and the Apple apps'.
package chat.lumia

import android.os.Build
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import uniffi.lumia_ffi.ProfileSummary
import uniffi.lumia_ffi.PasskeySummary
import androidx.compose.runtime.DisposableEffect
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.compose.ui.platform.LocalContext
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.AlternateEmail
import androidx.compose.material.icons.filled.ChatBubbleOutline
import androidx.compose.material.icons.filled.Computer
import androidx.compose.material.icons.filled.Language
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.PhoneAndroid
import androidx.compose.material.icons.filled.Shield
import androidx.compose.material.icons.filled.VpnKey
import androidx.compose.material3.ModalBottomSheet
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import chat.lumia.kit.AppStore
import chat.lumia.kit.Appearance
import chat.lumia.kit.AvatarTone
import chat.lumia.kit.Density
import chat.lumia.kit.LumiaAvatar
import chat.lumia.kit.LumiaComposer
import chat.lumia.kit.LumiaDayDivider
import chat.lumia.kit.LumiaDeviceRow
import chat.lumia.kit.LumiaEmptyState
import chat.lumia.kit.LumiaErrorText
import chat.lumia.kit.LumiaField
import chat.lumia.kit.LumiaLinkButton
import chat.lumia.kit.LumiaMessageRow
import chat.lumia.kit.LumiaNotice
import chat.lumia.kit.LumiaPrimaryButton
import chat.lumia.kit.LumiaRequestCard
import chat.lumia.kit.LumiaSecondaryButton
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.LumiaTypingLine
import chat.lumia.kit.Measures
import chat.lumia.kit.NoticeTone
import chat.lumia.kit.Route
import chat.lumia.kit.SectionTitle
import chat.lumia.kit.Stamp
import chat.lumia.kit.blockPlaces
import chat.lumia.kit.dayOfMicros
import chat.lumia.kit.initialsOf
import chat.lumia.kit.lumiaCode
import chat.lumia.kit.lumiaMessage
import chat.lumia.kit.timeOfMicros
import kotlinx.coroutines.launch
import uniffi.lumia_ffi.DeviceSummary
import uniffi.lumia_ffi.DmMessageSummary
import uniffi.lumia_ffi.DmRequestSummary
import androidx.compose.foundation.background
import androidx.compose.foundation.shape.RoundedCornerShape
import uniffi.lumia_ffi.InvitePreviewSummary
import uniffi.lumia_ffi.InviteSummary
import uniffi.lumia_ffi.MemberSummary
import uniffi.lumia_ffi.ServerSummary
import chat.lumia.kit.LumiaCard
import chat.lumia.kit.LumiaChoice

// Conversation

@Composable
fun ConversationScreen(session: LumiaSession, channelId: String, density: Density, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    val measures = Measures.of(density)
    var draft by rememberSaveable { mutableStateOf("") }
    var error by rememberSaveable { mutableStateOf("") }
    val channel = session.snapshot?.channels?.firstOrNull { it.channelId == channelId }
    val state = session.dmChannels.firstOrNull { it.channelId == channelId }
    val log = session.dmLogs[channelId]
    val others = channel?.participantAccountIds?.filter { it != session.me } ?: emptyList()
    val title = channel?.let { session.conversationName(it) } ?: "Conversation"
    val listState = rememberLazyListState()
    // Whose a message is: this account's from any of its devices, else the sender's.
    val me = session.me
    val senderOf = { m: DmMessageSummary -> if (m.mine) me else m.senderAccountId.ifEmpty { m.senderIdentityId } }
    val sent = log?.messages ?: emptyList()
    val places = remember(sent, me) { blockPlaces(sent.map { Stamp(senderOf(it), it.sentAtMicros) }) }

    LaunchedEffect(channelId) {
        session.refreshDmChannels()
        session.refreshDmLog(channelId)
    }
    LaunchedEffect(log?.messages?.size) {
        val n = log?.messages?.size ?: 0
        if (n > 0) listState.animateScrollToItem(n + 4)
    }

    PanelScreen(
        back = "Home", onBack = back, title = title,
        // The other side's pronouns, from their verified profile, then the host.
        subtitle = listOf(others.map { session.pronounsOf(it) }.filter { it.isNotEmpty() }.joinToString(", "), channel?.host?.ifEmpty { null }?.let { "on $it" } ?: "").filter { it.isNotEmpty() }.joinToString(" · ").ifEmpty { null },
        lead = { LumiaAvatar(initialsOf(title), AvatarTone.of(others.firstOrNull() ?: channelId), LumiaTokens.avatarMd) },
    ) {
        Column(Modifier.fillMaxSize().imePadding()) {
            LazyColumn(state = listState, modifier = Modifier.weight(1f).fillMaxWidth(), contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = LumiaTokens.space8)) {
                item {
                    Column(Modifier.padding(horizontal = LumiaTokens.space16), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                        LumiaNotice(Icons.Filled.Shield, text = "Messages here are end-to-end encrypted. Only your devices and those of ${others.joinToString(", ") { session.nameOf(it) }} can read them.")
                        if (state != null && !state.member) {
                            LumiaNotice(Icons.Filled.Lock, NoticeTone.ACCENT, action = "Rejoin" to {
                                scope.launch { error = ""; try { session.dmRejoin(channelId) } catch (e: Exception) { error = e.lumiaMessage } }
                            }, text = "This device is not in the conversation's group yet. The device that created it sends a welcome; if that never comes, rejoin from the instance's group info.")
                        }
                        if (log != null && log.beforeJoin > 0u) {
                            LumiaNotice(Icons.Filled.Lock, text = "${log.beforeJoin} earlier ${if (log.beforeJoin == 1u) "message was" else "messages were"} sent before this device joined and cannot be read here.")
                        }
                        if (log != null && log.undecryptable > 0u) LumiaNotice(Icons.Filled.Lock, text = "${log.undecryptable} could not be decrypted.")
                    }
                }
                if (log != null && log.messages.isNotEmpty()) item { LumiaDayDivider("Messages", measures) }
                itemsIndexed(sent, key = { _, m -> m.messageId }) { i, m ->
                    val who = if (m.mine) session.selfName else if (m.senderAccountId.isNotEmpty() || others.size != 1) session.nameOf(m.senderAccountId) else session.nameOf(others[0])
                    LumiaMessageRow(who, AvatarTone.of(senderOf(m)), timeOfMicros(m.sentAtMicros), m.body, m.deleted, places[i], measures)
                }
                if (error.isNotEmpty()) item { LumiaErrorText(error) }
            }
            val member = state?.member == true
            LumiaComposer(draft, { draft = it }, if (member) "Message $title" else "Setting up the encrypted group…", enabled = member) {
                val body = draft.trim()
                if (body.isEmpty()) return@LumiaComposer
                draft = ""
                error = ""
                scope.launch { try { session.dmSend(channelId, body) } catch (e: Exception) { error = e.lumiaMessage } }
            }
            // Nobody is shown typing in a personal conversation, and nothing is sent when this
            // person does (protocol 6.7); the line keeps its room, so the composer sits where a
            // channel's does and the boards draw it.
            ComposerFoot(emptyList())
        }
    }
}

// Channel

@Composable
fun ChannelScreen(session: LumiaSession, channelId: String, density: Density, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    val measures = Measures.of(density)
    var draft by rememberSaveable { mutableStateOf("") }
    var error by rememberSaveable { mutableStateOf("") }
    val channel = session.snapshot?.channels?.firstOrNull { it.channelId == channelId }
    val server = session.snapshot?.servers?.firstOrNull { it.serverId == channel?.serverId }
    val messages = session.messages[channelId] ?: emptyList()
    val name = channel?.name ?: "channel"
    val listState = rememberLazyListState()
    val places = remember(messages) { blockPlaces(messages.map { Stamp(it.authorAccountId, it.createdAtMicros) }) }

    LaunchedEffect(channelId) { session.refreshMessages(channelId) }
    LaunchedEffect(messages.size) { if (messages.isNotEmpty()) listState.animateScrollToItem(messages.size + 1) }

    PanelScreen(back = server?.name ?: "the server", onBack = back, title = "# $name", subtitle = server?.name) {
        Column(Modifier.fillMaxSize().imePadding()) {
            LazyColumn(state = listState, modifier = Modifier.weight(1f).fillMaxWidth(), contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = LumiaTokens.space8)) {
                if (messages.isEmpty()) item { LumiaEmptyState(Icons.Filled.ChatBubbleOutline, "Nothing here yet", "Say something.") }
                else item { LumiaDayDivider("Today", measures) }
                itemsIndexed(messages, key = { _, m -> m.messageId }) { i, m ->
                    LumiaMessageRow(session.nameOf(m.authorAccountId), AvatarTone.of(m.authorAccountId), timeOfMicros(m.createdAtMicros), m.body, m.deleted, places[i], measures)
                }
                if (error.isNotEmpty()) item { LumiaErrorText(error) }
            }
            // Every edit that leaves something in the composer says this person is typing;
            // emptied is not typing (a send). The core spaces what is sent.
            LumiaComposer(draft, { draft = it; if (it.isNotBlank()) session.typed(channelId) }, "Message #$name") {
                val body = draft.trim()
                if (body.isEmpty()) return@LumiaComposer
                draft = ""
                error = ""
                scope.launch { try { session.sendMessage(channelId, body) } catch (e: Exception) { error = e.lumiaMessage } }
            }
            ComposerFoot(session.typingIn(channelId))
        }
    }
}

/**
 * What is under a conversation's composer, to the foot of the panel: the
 * line that says who is typing. The navigation bar lies over the panel's
 * foot, and the line's own room there takes it in; the keyboard, when it
 * is up, covers the bar and is already under the screen (`imePadding`).
 * A composable of its own, so the keyboard's travel redraws this line and
 * not the messages.
 */
@Composable
private fun ComposerFoot(typing: List<String>) {
    val bar = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    val keyboard = WindowInsets.ime.asPaddingValues().calculateBottomPadding()
    LumiaTypingLine(typing, systemFoot = (bar - keyboard).coerceAtLeast(0.dp))
}

// Requests

@Composable
fun RequestsScreen(session: LumiaSession, back: () -> Unit, replace: (List<Screen>) -> Unit) {
    val scope = rememberCoroutineScope()
    var busy by remember { mutableStateOf<String?>(null) }
    var error by rememberSaveable { mutableStateOf("") }
    val requests = session.snapshot?.pendingRequests ?: emptyList()

    fun meta(r: DmRequestSummary): String {
        val parts = mutableListOf<String>()
        if (r.sharedServerIds.isNotEmpty()) parts.add("Shares ${if (r.sharedServerIds.size == 1) "a server" else "${r.sharedServerIds.size} servers"} with you")
        if (r.mutualFriendCount > 0u) parts.add("${r.mutualFriendCount} mutual ${if (r.mutualFriendCount == 1u) "friend" else "friends"}")
        return if (parts.isEmpty()) "Nothing in common yet" else parts.joinToString(" · ")
    }

    fun respond(id: String, response: String) {
        scope.launch {
            busy = id
            error = ""
            try {
                val channelId = session.dmRespond(id, response)
                if (channelId != null && response == "accept") replace(listOf(Screen.Conversation(channelId)))
            } catch (e: Exception) {
                error = e.lumiaMessage
            } finally {
                busy = null
            }
        }
    }

    PanelScreen(back = "Home", onBack = back, title = "Requests", subtitle = if (requests.isEmpty()) "No one waiting" else "${requests.size} waiting for you") {
        LazyColumn(Modifier.fillMaxSize().navigationBarsPadding(), contentPadding = androidx.compose.foundation.layout.PaddingValues(LumiaTokens.space16), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
            item { LumiaNotice(Icons.Filled.Shield, text = "A request shows who is asking and what you share, nothing else. Accepting opens an encrypted conversation; the sender cannot see that you looked.") }
            if (error.isNotEmpty()) item { LumiaErrorText(error) }
            if (requests.isEmpty()) {
                item { LumiaEmptyState(Icons.Filled.AlternateEmail, "Nothing waiting", "Declined requests disappear; blocked people cannot ask again.") }
            } else {
                item { SectionTitle("Waiting for you") }
                items(requests, key = { it.requestId }) { r ->
                    LumiaRequestCard(
                        name = session.nameOf(r.fromAccountId),
                        handle = (session.people[r.fromAccountId] ?: session.profileOf(r.fromAccountId))?.handle,
                        tone = AvatarTone.of(r.fromAccountId),
                        meta = meta(r),
                        busy = busy == r.requestId,
                        accept = { respond(r.requestId, "accept") },
                        decline = { respond(r.requestId, "decline") },
                        block = { respond(r.requestId, "block") },
                    )
                }
            }
        }
    }
}

// New message

@Composable
fun NewMessageScreen(session: LumiaSession, back: () -> Unit, replace: (List<Screen>) -> Unit) {
    val scope = rememberCoroutineScope()
    var query by rememberSaveable { mutableStateOf("") }
    var note by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf<String?>(null) }
    val q = query.trim().lowercase()
    val list = session.people.values
        .filter { it.accountId != session.me }
        .filter { q.isEmpty() || it.handle.lowercase().contains(q) || it.displayName.lowercase().contains(q) || it.nickname.lowercase().contains(q) }
        .sortedBy { (it.displayName.ifEmpty { it.handle }).lowercase() }

    fun message(p: MemberSummary) {
        scope.launch {
            busy = p.accountId
            note = ""
            try {
                val channelId = session.dmOpenOrRequest(p.accountId)
                if (channelId != null) replace(listOf(Screen.Conversation(channelId)))
                else note = "Request sent to ${p.displayName.ifEmpty { p.handle }}. The conversation opens when they accept."
            } catch (e: Exception) {
                note = if (e.lumiaCode == "ALREADY_EXISTS") "You already asked; waiting for their answer." else e.lumiaMessage
            } finally {
                busy = null
            }
        }
    }

    PanelScreen(back = "Home", onBack = back, title = "New message", subtitle = "Someone you share a server with") {
        LazyColumn(Modifier.fillMaxSize().imePadding().navigationBarsPadding(), contentPadding = androidx.compose.foundation.layout.PaddingValues(LumiaTokens.space16), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
            item { LumiaField("Find a person", query, { query = it }, placeholder = "Name or handle") }
            if (note.isNotEmpty()) item { LumiaNotice(Icons.Filled.AlternateEmail, text = note) }
            if (list.isEmpty()) {
                item { LumiaEmptyState(Icons.Filled.AlternateEmail, "No one to show", "Join a server to find people to message.") }
            } else {
                items(list, key = { it.accountId }) { p ->
                    val name = p.displayName.ifEmpty { p.handle }
                    Row(Modifier.fillMaxWidth().padding(vertical = LumiaTokens.space4), horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space10), verticalAlignment = Alignment.CenterVertically) {
                        LumiaAvatar(initialsOf(name), AvatarTone.of(p.accountId), LumiaTokens.avatarSm)
                        Column(Modifier.weight(1f)) {
                            Text(name, style = LumiaTokens.textMember.style, color = LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text("@${p.handle}", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkQuiet.color)
                        }
                        LumiaSecondaryButton("Message", enabled = busy != p.accountId) { message(p) }
                    }
                }
            }
        }
    }
}

// Add a server

/**
 * A server of your own, or joining one from an invite link or code: what
 * the link leads to first, on which instance, then the join (architecture 6.1).
 */
@Composable
fun AddServerScreen(session: LumiaSession, back: () -> Unit, opened: (String) -> Unit) {
    val scope = rememberCoroutineScope()
    var name by rememberSaveable { mutableStateOf("") }
    var creating by remember { mutableStateOf(false) }
    var createError by rememberSaveable { mutableStateOf("") }
    var text by rememberSaveable { mutableStateOf("") }
    var preview by remember { mutableStateOf<InvitePreviewSummary?>(null) }
    var error by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }

    fun create() {
        scope.launch {
            creating = true
            createError = ""
            try {
                opened(session.createServer(name))
            } catch (e: Exception) {
                createError = e.lumiaMessage
            } finally {
                creating = false
            }
        }
    }

    fun look() {
        scope.launch {
            busy = true
            error = ""
            preview = null
            try {
                preview = session.invitePreview(text)
            } catch (e: Exception) {
                error = if (e.lumiaCode == "NOT_FOUND") "That invite is unknown, expired or used up." else e.lumiaMessage
            } finally {
                busy = false
            }
        }
    }

    fun join(p: InvitePreviewSummary) {
        scope.launch {
            busy = true
            error = ""
            try {
                opened(session.acceptInvite(p.code))
            } catch (e: Exception) {
                error = e.lumiaMessage
            } finally {
                busy = false
            }
        }
    }

    PanelScreen(back = "Home", onBack = back, title = "Add a server", subtitle = "Your own, or one you were invited to") {
        LazyColumn(Modifier.fillMaxSize().imePadding().navigationBarsPadding(), contentPadding = androidx.compose.foundation.layout.PaddingValues(LumiaTokens.space16), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space16)) {
            item {
                LumiaCard("Create a server", "You own it; invite people once it exists.") {
                    Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
                        LumiaField("Name", name, { name = it }, placeholder = "Our place")
                        if (createError.isNotEmpty()) LumiaErrorText(createError)
                        LumiaPrimaryButton("Create", Modifier.fillMaxWidth(), enabled = !creating && name.isNotBlank()) { create() }
                    }
                }
            }
            item {
                LumiaCard("Join with an invite", "Paste the link someone sent you; what it leads to shows first.") {
                    Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
                        LumiaField("Invite link or code", text, { text = it }, placeholder = "https://chat.example.com/invite/…")
                        if (error.isNotEmpty()) LumiaErrorText(error)
                        LumiaPrimaryButton("Look it up", Modifier.fillMaxWidth(), enabled = !busy && text.isNotBlank()) { look() }
                        preview?.let { p ->
                            Text("${p.serverName} · ${p.memberCount} ${if (p.memberCount == 1u) "member" else "members"} on ${p.host}, run by ${p.operatorName.ifEmpty { "an unnamed operator" }}${if (p.asGuest) " · you would join as a guest" else ""}. Your account on this instance joins it; nothing else is shared.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
                            LumiaPrimaryButton("Join ${p.serverName}", Modifier.fillMaxWidth(), enabled = !busy) { join(p) }
                        }
                    }
                }
            }
        }
    }
}

// New channel

/** A text channel in the server; it opens once made. */
@Composable
fun NewChannelScreen(session: LumiaSession, server: ServerSummary?, back: () -> Unit, opened: (String) -> Unit) {
    val scope = rememberCoroutineScope()
    var name by rememberSaveable { mutableStateOf("") }
    var error by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }

    fun create() {
        val id = server?.serverId ?: return
        scope.launch {
            busy = true
            error = ""
            try {
                opened(session.createChannel(id, name))
            } catch (e: Exception) {
                error = if (e.lumiaCode == "PERMISSION_DENIED") "You cannot add channels to this server." else e.lumiaMessage
            } finally {
                busy = false
            }
        }
    }

    PanelScreen(back = server?.name ?: "Server", onBack = back, title = "New channel", subtitle = "A text channel everyone in ${server?.name ?: "the server"} can see") {
        LazyColumn(Modifier.fillMaxSize().imePadding().navigationBarsPadding(), contentPadding = androidx.compose.foundation.layout.PaddingValues(LumiaTokens.space16), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
            item { LumiaField("Name", name, { name = it }, placeholder = "general") }
            if (error.isNotEmpty()) item { LumiaErrorText(error) }
            item { LumiaPrimaryButton("Create", Modifier.fillMaxWidth(), enabled = !busy && name.isNotBlank()) { create() } }
        }
    }
}

// Invite people

/**
 * Invite links for the server: a new one with an expiry and a use limit,
 * ready to copy or share, and the ones that exist, each revocable.
 */
@Composable
fun InviteScreen(session: LumiaSession, server: ServerSummary?, back: () -> Unit) {
    val context = androidx.compose.ui.platform.LocalContext.current
    val scope = rememberCoroutineScope()
    var expiresInHours by rememberSaveable { mutableStateOf(0) }
    var maxUses by rememberSaveable { mutableStateOf(0) }
    var invite by remember { mutableStateOf<InviteSummary?>(null) }
    var invites by remember { mutableStateOf<List<InviteSummary>>(emptyList()) }
    var error by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var copied by remember { mutableStateOf(false) }
    var email by rememberSaveable { mutableStateOf("") }
    var emailed by rememberSaveable { mutableStateOf("") }

    fun describe(i: InviteSummary): String {
        val uses = (if (i.email.isEmpty()) "" else "to ${i.email} · ") + (if (i.maxUses == 0u) "${i.uses} uses" else "${i.uses}/${i.maxUses} uses")
        val at = i.expiresAtMicros ?: return "$uses · no expiry"
        return "$uses · expires ${java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.MEDIUM, java.text.DateFormat.SHORT).format(java.util.Date(at / 1000))}"
    }

    LaunchedEffect(server?.serverId) {
        val id = server?.serverId ?: return@LaunchedEffect
        try {
            invites = session.invites(id)
        } catch (e: Exception) {
            error = if (e.lumiaCode == "PERMISSION_DENIED") "You cannot invite people to this server." else e.lumiaMessage
        }
    }

    fun sendByEmail() {
        val id = server?.serverId ?: return
        scope.launch {
            busy = true
            error = ""
            emailed = ""
            try {
                val made = session.emailInvite(id, email)
                invites = listOf(made) + invites
                emailed = made.email
                email = ""
            } catch (e: Exception) {
                error = when (e.lumiaCode) {
                    "FAILED_PRECONDITION" -> "This instance sends no mail; make a link instead."
                    "INVALID_ARGUMENT" -> "That is not an email address."
                    "RATE_LIMITED" -> "Too many invites in the last hour; try later."
                    else -> e.lumiaMessage
                }
            } finally {
                busy = false
            }
        }
    }

    fun make() {
        val id = server?.serverId ?: return
        scope.launch {
            busy = true
            error = ""
            copied = false
            try {
                val made = session.createInvite(id, maxUses.toUInt(), if (expiresInHours == 0) null else expiresInHours.toUInt())
                invite = made
                invites = listOf(made) + invites
            } catch (e: Exception) {
                error = when (e.lumiaCode) {
                    "PERMISSION_DENIED" -> "You cannot invite people to this server."
                    "RATE_LIMITED" -> "Too many links in the last hour; try later."
                    else -> e.lumiaMessage
                }
            } finally {
                busy = false
            }
        }
    }

    fun revoke(i: InviteSummary) {
        scope.launch {
            busy = true
            error = ""
            try {
                session.revokeInvite(i.code)
                invites = invites.filter { it.code != i.code }
                if (invite?.code == i.code) invite = null
            } catch (e: Exception) {
                error = e.lumiaMessage
            } finally {
                busy = false
            }
        }
    }

    @Composable
    fun Choice(label: String, options: List<Pair<String, Int>>, value: Int, onValue: (Int) -> Unit) {
        Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
            Text(label, style = LumiaTokens.textLabel.style, color = LumiaTokens.inkSecondary.color)
            Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                for ((name, v) in options) {
                    if (v == value) LumiaPrimaryButton(name) { } else LumiaSecondaryButton(name) { onValue(v) }
                }
            }
        }
    }

    PanelScreen(back = server?.name ?: "Server", onBack = back, title = "Invite people", subtitle = "A link to join ${server?.name ?: "the server"}") {
        LazyColumn(Modifier.fillMaxSize().navigationBarsPadding(), contentPadding = androidx.compose.foundation.layout.PaddingValues(LumiaTokens.space16), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
            item { Choice("Expires", listOf("Never" to 0, "In a day" to 24, "In a week" to 168), expiresInHours) { expiresInHours = it } }
            item { Choice("Uses", listOf("Unlimited" to 0, "Once" to 1, "Ten" to 10), maxUses) { maxUses = it } }
            if (error.isNotEmpty()) item { LumiaErrorText(error) }
            item { LumiaPrimaryButton("Make a link", Modifier.fillMaxWidth(), enabled = !busy) { make() } }
            // The invite as a mail (architecture 6.2): one use, a week, to the address.
            item { LumiaField("Email", email, { email = it }, placeholder = "pat@example.com") }
            item { LumiaSecondaryButton("Send by email", Modifier.fillMaxWidth(), enabled = !busy && email.isNotBlank()) { sendByEmail() } }
            if (emailed.isNotEmpty()) item { Text("Sent to $emailed; it works once, within a week.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color) }
            val link = invite
            if (link != null) {
                item {
                    Text(link.url, style = LumiaTokens.textMeta.style, color = LumiaTokens.ink.color, modifier = Modifier.fillMaxWidth().background(LumiaTokens.surface.color, RoundedCornerShape(LumiaTokens.radius10)).padding(LumiaTokens.space12))
                }
                item {
                    Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                        LumiaPrimaryButton(if (copied) "Copied" else "Copy link") {
                            val clipboard = context.getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager
                            clipboard.setPrimaryClip(android.content.ClipData.newPlainText("Lumia invite", link.url))
                            copied = true
                        }
                        LumiaSecondaryButton("Share") {
                            val send = android.content.Intent(android.content.Intent.ACTION_SEND).apply {
                                type = "text/plain"
                                putExtra(android.content.Intent.EXTRA_TEXT, link.url)
                            }
                            context.startActivity(android.content.Intent.createChooser(send, "Invite people"))
                        }
                    }
                }
            }
            if (invites.isNotEmpty()) {
                item { SectionTitle("Invites") }
                items(invites, key = { it.code }) { i ->
                    Row(Modifier.fillMaxWidth().padding(vertical = LumiaTokens.space4), horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space10), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(i.code, style = LumiaTokens.textMember.style, color = LumiaTokens.ink.color)
                            Text(describe(i), style = LumiaTokens.textMeta.style, color = LumiaTokens.inkQuiet.color)
                        }
                        LumiaLinkButton("Revoke", enabled = !busy) { revoke(i) }
                    }
                }
            }
        }
    }
}

// Devices

// The person's own profile (architecture 6.4): the signed profile the
// identity carries, display name, pronouns and bio, signed by this phone's
// device key and sent to the instance as the account's. The same screen
// as the web's and the iOS app's.
@Composable
fun ProfileScreen(store: AppStore, session: LumiaSession, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    var profile by remember { mutableStateOf<ProfileSummary?>(null) }
    var displayName by rememberSaveable { mutableStateOf("") }
    var pronouns by rememberSaveable { mutableStateOf("") }
    var bio by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by rememberSaveable { mutableStateOf("") }
    var saved by rememberSaveable { mutableStateOf(false) }
    // The status (protocol 6.7): chosen here, kept by the core, sent with the servers watched.
    var status by rememberSaveable { mutableStateOf("ONLINE") }
    var customStatus by rememberSaveable { mutableStateOf("") }
    var statusSaved by rememberSaveable { mutableStateOf(false) }
    // The form waits for the profile held: fields shown before it lands
    // would be overwritten by it, typing and all.
    var loaded by remember { mutableStateOf(false) }

    LaunchedEffect(Unit) {
        session.status()?.let { status = it.status; customStatus = it.customStatus }
        profile = runCatching { session.profile() }.getOrNull()
        displayName = profile?.displayName ?: ""
        pronouns = profile?.pronouns ?: ""
        bio = profile?.bio ?: ""
        loaded = true
    }

    val handleTitle = session.snapshot?.handle?.takeIf { it.isNotEmpty() }?.let { "@$it" } ?: "You"
    val subtitle = profile?.let { "Version ${it.version}, signed by this phone's device key; every instance you sign in to takes it." } ?: "Nothing set yet: people see your handle."

    PanelScreen(back = "Home", onBack = back, title = "Profile", subtitle = "How people see you") {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).navigationBarsPadding().imePadding().padding(LumiaTokens.space16), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
            LumiaCard("Status", "What people see beside your name in a server's member list.") {
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
                    LumiaChoice(listOf("ONLINE" to "Online", "AWAY" to "Away", "DO_NOT_DISTURB" to "Do not disturb"), status, { status = it })
                    LumiaField("Custom status", customStatus, { customStatus = it }, placeholder = "At lunch")
                    if (statusSaved) Text("Status saved.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
                    LumiaPrimaryButton("Save status", modifier = Modifier.fillMaxWidth()) {
                        scope.launch {
                            statusSaved = false
                            runCatching { session.setStatus(status, customStatus) }
                            statusSaved = true
                        }
                    }
                }
            }
            if (session.hasIdentity && loaded) {
                LumiaCard(handleTitle, subtitle) {
                    Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
                        LumiaField("Display name", displayName, { displayName = it }, placeholder = "How people see you")
                        LumiaField("Pronouns", pronouns, { pronouns = it }, placeholder = "they/them")
                        LumiaField("Bio", bio, { bio = it }, placeholder = "A line about you")
                        if (error.isNotEmpty()) LumiaErrorText(error)
                        if (saved) Text("Saved. This instance shows it now.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
                        LumiaPrimaryButton("Save", enabled = !busy, modifier = Modifier.fillMaxWidth()) {
                            scope.launch {
                                busy = true
                                error = ""
                                saved = false
                                try {
                                    profile = session.setProfile(displayName, pronouns, bio)
                                    saved = true
                                } catch (e: Exception) {
                                    error = when (e.lumiaCode) {
                                        "FAILED_PRECONDITION" -> "This account is not linked to your identity, so the instance keeps the profile it has."
                                        "CONFLICT" -> "A newer profile is already held. Reload and try again."
                                        else -> e.lumiaMessage
                                    }
                                } finally {
                                    busy = false
                                }
                            }
                        }
                    }
                }
            } else if (!session.hasIdentity) {
                LumiaNotice(Icons.Filled.Shield, NoticeTone.ACCENT, action = "Link this phone" to { store.route = Route.LINK }, text = "Your profile travels with your identity, signed by its devices. This phone holds no device key yet; a device you already use approves it.")
            }
        }
    }
}

// Preferences

/**
 * How Lumia looks on this phone: light or dark (or as the phone is), and
 * how much room messages take. Each choice takes effect as it is made and
 * is there at the next launch; they are this phone's and not the
 * account's, so nothing here is sent to the instance and signing out
 * leaves them be (`AppStore`). The desktop's page (its scroll bar card is
 * a Mac's alone: a phone's scroll indicators are the system's); no board
 * draws it, so it is the Profile screen's cards with the choice Profile's
 * status has.
 */
@Composable
fun PreferencesScreen(store: AppStore, back: () -> Unit) {
    PanelScreen(back = "Home", onBack = back, title = "Preferences", subtitle = "How Lumia looks on this phone") {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).navigationBarsPadding().padding(LumiaTokens.space16), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
            LumiaCard("Appearance", "Light, dark, or whichever this phone is in.") {
                LumiaChoice(
                    listOf(Appearance.LIGHT.stored to "Light", Appearance.DARK.stored to "Dark", Appearance.SYSTEM.stored to "System"),
                    store.appearance.stored,
                    { store.choose(Appearance.of(it)) },
                )
            }
            LumiaCard("Messages", "Comfortable gives each message room. Compact shows more of a conversation at once.") {
                LumiaChoice(
                    listOf(Density.COMFORTABLE.stored to "Comfortable", Density.COMPACT.stored to "Compact"),
                    store.density.stored,
                    { store.choose(Density.of(it)) },
                )
            }
        }
    }
}

@OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
@Composable
fun DevicesScreen(store: AppStore, session: LumiaSession, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val lifecycle = LocalLifecycleOwner.current
    var revoking by remember { mutableStateOf<DeviceSummary?>(null) }
    var password by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by rememberSaveable { mutableStateOf("") }
    // Passkeys (architecture 6.3): the account's sign-in keys, listed and
    // removable here, made on the instance's web page in a browser tab.
    var passkeys by remember { mutableStateOf<List<PasskeySummary>>(emptyList()) }
    var passkeyError by rememberSaveable { mutableStateOf("") }
    suspend fun loadPasskeys() { passkeys = runCatching { session.passkeys() }.getOrDefault(emptyList()) }
    // Back from the tab (a passkey made there): the list again.
    DisposableEffect(lifecycle) {
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_RESUME) scope.launch { loadPasskeys() } }
        lifecycle.lifecycle.addObserver(observer)
        onDispose { lifecycle.lifecycle.removeObserver(observer) }
    }
    val devices = session.roster?.devices ?: emptyList()
    val active = devices.filter { !it.revoked }

    fun label(d: DeviceSummary): String = when {
        d.thisDevice -> "This ${Build.MODEL.ifEmpty { "phone" }}"
        d.label.isNotEmpty() -> d.label
        d.kind == "web" -> "Browser"
        d.kind == "native" -> "Computer"
        d.kind == "mobile" -> "Phone"
        else -> d.kind.ifEmpty { "Device" }
    }

    fun meta(d: DeviceSummary): String {
        val parts = mutableListOf<String>()
        if (d.webOrigin.isNotEmpty()) parts.add(d.webOrigin)
        if (d.addedAtMicros > 0) parts.add("added ${dayOfMicros(d.addedAtMicros)}")
        parts.add(d.deviceId.take(8))
        return parts.joinToString(" · ")
    }

    fun icon(d: DeviceSummary) = when (d.kind) {
        "web" -> Icons.Filled.Language
        "mobile" -> Icons.Filled.PhoneAndroid
        "native" -> Icons.Filled.Computer
        else -> Icons.Filled.VpnKey
    }

    LaunchedEffect(Unit) { session.refreshRoster(); loadPasskeys() }

    PanelScreen(
        back = "Home", onBack = back, title = "Devices",
        subtitle = if (session.roster == null) "No identity on this phone" else "${active.size} linked to you",
        actions = listOf(Triple(Icons.Filled.Add, "Link a device") { store.route = Route.LINK }),
    ) {
        LazyColumn(Modifier.fillMaxSize().navigationBarsPadding(), contentPadding = androidx.compose.foundation.layout.PaddingValues(LumiaTokens.space16), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
            item { LumiaNotice(Icons.Filled.Shield, text = "Your identity lives on these devices, not on any instance. Revoking one takes effect on every instance that holds your roster.") }
            item {
                LumiaCard("Passkeys", "Sign in to this instance with a key this phone keeps, instead of a password.") {
                    Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                        passkeys.forEach { p ->
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                val added = p.createdAtMicros?.let { dayOfMicros(it) } ?: "—"
                                val used = p.lastUsedAtMicros?.let { dayOfMicros(it) } ?: "never"
                                Text("${p.label.ifEmpty { "Passkey" }} · added $added · last used $used", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color, modifier = Modifier.weight(1f))
                                LumiaLinkButton("Remove") {
                                    scope.launch {
                                        passkeyError = ""
                                        try { session.removePasskey(p.passkeyId); loadPasskeys() } catch (e: Exception) { passkeyError = e.lumiaMessage }
                                    }
                                }
                            }
                        }
                        if (passkeyError.isNotEmpty()) LumiaErrorText(passkeyError)
                        LumiaSecondaryButton("Add a passkey", modifier = Modifier.fillMaxWidth()) {
                            scope.launch {
                                passkeyError = ""
                                try { openInBrowser(context, session.webHandoffUrl("devices")) } catch (e: Exception) { passkeyError = e.lumiaMessage }
                            }
                        }
                        Text("Made on the instance's web page, in a browser tab signed in as you.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkQuiet.color)
                    }
                }
            }
            if (session.roster != null) {
                items(active, key = { it.deviceId }) { d -> LumiaDeviceRow(d, label(d), meta(d), icon(d)) { revoking = d; password = ""; error = "" } }
                val revoked = devices.filter { it.revoked }
                if (revoked.isNotEmpty()) {
                    item { SectionTitle("Revoked") }
                    items(revoked, key = { it.deviceId }) { d -> LumiaDeviceRow(d, d.deviceId.take(8), "", icon(d), null) }
                }
            } else {
                item { LumiaEmptyState(Icons.Filled.Shield, "No identity on this phone", "A device you already use approves this one.", action = "Link this phone" to { store.route = Route.LINK }) }
            }
        }
    }

    val d = revoking
    if (d != null) {
        ModalBottomSheet(onDismissRequest = { revoking = null }, containerColor = LumiaTokens.surface.color) {
            Column(Modifier.padding(start = LumiaTokens.space24, end = LumiaTokens.space24, bottom = LumiaTokens.space40).imePadding(), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space16)) {
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
                    Text("Revoke ${label(d)}?", style = LumiaTokens.titleAuth.style, color = LumiaTokens.ink.color)
                    Text("It stops reading new messages the moment every instance has the change. Confirm with your password.", style = LumiaTokens.textSubtitle.style, color = LumiaTokens.inkSecondary.color)
                }
                LumiaField("Password", password, { password = it }, placeholder = "Your password", secure = true)
                if (error.isNotEmpty()) LumiaErrorText(error)
                Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                    LumiaPrimaryButton("Revoke ${label(d)}", enabled = !busy && password.isNotEmpty()) {
                        scope.launch {
                            busy = true
                            error = ""
                            try {
                                session.identityRevoke(d.deviceId, password)
                                revoking = null
                                password = ""
                            } catch (e: Exception) {
                                error = if (e.lumiaCode == "UNAUTHENTICATED") "That password was not accepted." else e.lumiaMessage
                            } finally {
                                busy = false
                            }
                        }
                    }
                    LumiaSecondaryButton("Keep it") { revoking = null }
                    Spacer(Modifier.weight(1f))
                }
            }
        }
    }
}
