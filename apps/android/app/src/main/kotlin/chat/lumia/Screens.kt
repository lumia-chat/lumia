// The screens pushed over the rail and sidebar, each a floating panel: a
// conversation, a channel, requests, a new message, devices. The same
// states, wording and error handling as the web's and the Apple apps'.
package chat.lumia

import android.os.Build
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
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
import chat.lumia.kit.AppStore
import chat.lumia.kit.AvatarTone
import chat.lumia.kit.LumiaAvatar
import chat.lumia.kit.LumiaComposer
import chat.lumia.kit.LumiaDayDivider
import chat.lumia.kit.LumiaDeviceRow
import chat.lumia.kit.LumiaEmptyState
import chat.lumia.kit.LumiaErrorText
import chat.lumia.kit.LumiaField
import chat.lumia.kit.LumiaMessageRow
import chat.lumia.kit.LumiaNotice
import chat.lumia.kit.LumiaPrimaryButton
import chat.lumia.kit.LumiaRequestCard
import chat.lumia.kit.LumiaSecondaryButton
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.NoticeTone
import chat.lumia.kit.Route
import chat.lumia.kit.SectionTitle
import chat.lumia.kit.dayOfMicros
import chat.lumia.kit.initialsOf
import chat.lumia.kit.lumiaCode
import chat.lumia.kit.lumiaMessage
import chat.lumia.kit.timeOfMicros
import kotlinx.coroutines.launch
import uniffi.lumia_ffi.DeviceSummary
import uniffi.lumia_ffi.DmRequestSummary
import uniffi.lumia_ffi.MemberSummary

// Conversation

@Composable
fun ConversationScreen(session: LumiaSession, channelId: String, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    var draft by rememberSaveable { mutableStateOf("") }
    var error by rememberSaveable { mutableStateOf("") }
    val channel = session.snapshot?.channels?.firstOrNull { it.channelId == channelId }
    val state = session.dmChannels.firstOrNull { it.channelId == channelId }
    val log = session.dmLogs[channelId]
    val others = channel?.participantAccountIds?.filter { it != session.me } ?: emptyList()
    val title = channel?.let { session.conversationName(it) } ?: "Conversation"
    val listState = rememberLazyListState()

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
        subtitle = channel?.host?.ifEmpty { null }?.let { "on $it" },
        lead = { LumiaAvatar(initialsOf(title), AvatarTone.of(others.firstOrNull() ?: channelId), LumiaTokens.avatarMd) },
    ) {
        Column(Modifier.fillMaxSize().imePadding().navigationBarsPadding()) {
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
                if (log != null && log.messages.isNotEmpty()) item { LumiaDayDivider("Messages") }
                items(log?.messages ?: emptyList(), key = { it.messageId }) { m ->
                    val who = if (m.mine) session.selfName else if (m.senderAccountId.isNotEmpty() || others.size != 1) session.nameOf(m.senderAccountId) else session.nameOf(others[0])
                    val toneId = if (m.mine) session.me else m.senderAccountId.ifEmpty { m.senderIdentityId }
                    LumiaMessageRow(who, AvatarTone.of(toneId), timeOfMicros(m.sentAtMicros), m.body, m.deleted)
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
        }
    }
}

// Channel

@Composable
fun ChannelScreen(session: LumiaSession, channelId: String, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    var draft by rememberSaveable { mutableStateOf("") }
    var error by rememberSaveable { mutableStateOf("") }
    val channel = session.snapshot?.channels?.firstOrNull { it.channelId == channelId }
    val server = session.snapshot?.servers?.firstOrNull { it.serverId == channel?.serverId }
    val messages = session.messages[channelId] ?: emptyList()
    val name = channel?.name ?: "channel"
    val listState = rememberLazyListState()

    LaunchedEffect(channelId) { session.refreshMessages(channelId) }
    LaunchedEffect(messages.size) { if (messages.isNotEmpty()) listState.animateScrollToItem(messages.size + 1) }

    PanelScreen(back = server?.name ?: "the server", onBack = back, title = "# $name", subtitle = server?.name) {
        Column(Modifier.fillMaxSize().imePadding().navigationBarsPadding()) {
            LazyColumn(state = listState, modifier = Modifier.weight(1f).fillMaxWidth(), contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = LumiaTokens.space8)) {
                if (messages.isEmpty()) item { LumiaEmptyState(Icons.Filled.ChatBubbleOutline, "Nothing here yet", "Say something.") }
                else item { LumiaDayDivider("Today") }
                items(messages, key = { it.messageId }) { m ->
                    LumiaMessageRow(session.nameOf(m.authorAccountId), AvatarTone.of(m.authorAccountId), timeOfMicros(m.createdAtMicros), m.body, m.deleted)
                }
                if (error.isNotEmpty()) item { LumiaErrorText(error) }
            }
            LumiaComposer(draft, { draft = it }, "Message #$name") {
                val body = draft.trim()
                if (body.isEmpty()) return@LumiaComposer
                draft = ""
                error = ""
                scope.launch { try { session.sendMessage(channelId, body) } catch (e: Exception) { error = e.lumiaMessage } }
            }
        }
    }
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

// Devices

@OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
@Composable
fun DevicesScreen(store: AppStore, session: LumiaSession, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    var revoking by remember { mutableStateOf<DeviceSummary?>(null) }
    var password by rememberSaveable { mutableStateOf("") }
    var busy by rememberSaveable { mutableStateOf(false) }
    var error by rememberSaveable { mutableStateOf("") }
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

    LaunchedEffect(Unit) { session.refreshRoster() }

    PanelScreen(
        back = "Home", onBack = back, title = "Devices",
        subtitle = if (session.roster == null) "No identity on this phone" else "${active.size} linked to you",
        actions = listOf(Triple(Icons.Filled.Add, "Link a device") { store.route = Route.LINK }),
    ) {
        LazyColumn(Modifier.fillMaxSize().navigationBarsPadding(), contentPadding = androidx.compose.foundation.layout.PaddingValues(LumiaTokens.space16), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
            item { LumiaNotice(Icons.Filled.Shield, text = "Your identity lives on these devices, not on any instance. Revoking one takes effect on every instance that holds your roster.") }
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
