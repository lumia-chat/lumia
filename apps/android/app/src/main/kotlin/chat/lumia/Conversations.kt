// The conversations: a personal one, a server's channel or server DM, and
// the Mentions pages (architecture 6.6). Each conversation draws its
// messages with what 6.6 adds (a reply's reference line, reactions,
// mentions by name, "(edited)", the wash of one for this account, the
// unread divider), opens a message's actions on a long press, puts the
// reply bar in the composer's frame, edits a message in place and asks
// before deleting one, and is marked read while its newest message is on
// screen with the app in front. The same states and words as the
// desktop's and the web's.
package chat.lumia

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
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
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AlternateEmail
import androidx.compose.material.icons.filled.ChatBubbleOutline
import androidx.compose.material.icons.filled.Group
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Shield
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.currentStateAsState
import chat.lumia.kit.AppStore
import chat.lumia.kit.AvatarTone
import chat.lumia.kit.Display
import chat.lumia.kit.FLASH_MS
import chat.lumia.kit.LumiaAvatar
import chat.lumia.kit.LumiaBody
import chat.lumia.kit.LumiaComposer
import chat.lumia.kit.LumiaDayDivider
import chat.lumia.kit.LumiaEmptyState
import chat.lumia.kit.LumiaErrorText
import chat.lumia.kit.LumiaMessageRow
import chat.lumia.kit.LumiaNewMessagesLine
import chat.lumia.kit.LumiaNotice
import chat.lumia.kit.LumiaNoticeRow
import chat.lumia.kit.LumiaPrimaryButton
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.LumiaTypingLine
import chat.lumia.kit.MARK_EVERY_MS
import chat.lumia.kit.NoticeTone
import chat.lumia.kit.SAID_FOR_MS
import chat.lumia.kit.SERVER_DM
import chat.lumia.kit.Stamp
import chat.lumia.kit.TEXT
import chat.lumia.kit.addRefused
import chat.lumia.kit.blockPlaces
import chat.lumia.kit.dayOfMicros
import chat.lumia.kit.firstUnread
import chat.lumia.kit.frequentEmoji
import chat.lumia.kit.initialsOf
import chat.lumia.kit.invitedWords
import chat.lumia.kit.isOver
import chat.lumia.kit.lumiaCode
import chat.lumia.kit.lumiaMessage
import chat.lumia.kit.mentionAt
import chat.lumia.kit.mentionChoices
import chat.lumia.kit.mentionPlace
import chat.lumia.kit.drawnBody
import chat.lumia.kit.EditOutcome
import chat.lumia.kit.editOutcome
import chat.lumia.kit.firstLine
import chat.lumia.kit.lastEditable
import chat.lumia.kit.mayDelete
import chat.lumia.kit.mayEdit
import chat.lumia.kit.offersMention
import chat.lumia.kit.sentQuote
import chat.lumia.kit.noticeText
import chat.lumia.kit.notInChannel
import chat.lumia.kit.otherIn
import chat.lumia.kit.outsideWords
import chat.lumia.kit.pickMention
import chat.lumia.kit.quoteOf
import chat.lumia.kit.serverDmNote
import chat.lumia.kit.timeOfMicros
import chat.lumia.kit.rememberClock
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.lumia_ffi.DmMessageSummary
import uniffi.lumia_ffi.MessageSummary
import uniffi.lumia_ffi.QuoteSummary

/** Those a message mentioned who are not in its channel: the bar after a send, while adding them, and once an add made invitations, what it says for a few seconds. */
private data class Outside(val accountIds: List<String>, val adding: Boolean = false, val said: String? = null, val error: String = "")

private fun copy(context: Context, text: String) {
    val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
    clipboard.setPrimaryClip(ClipData.newPlainText("Message", text))
}

/** Whether the conversation's app is in front: a conversation is read only then. */
@Composable
private fun inFront(): Boolean {
    val state by LocalLifecycleOwner.current.lifecycle.currentStateAsState()
    return state.isAtLeast(Lifecycle.State.RESUMED)
}

/**
 * How a conversation's list follows its messages: it opens at the "New
 * messages" line when there is one ([newLine], its message's place, -1
 * for none), at the bottom otherwise, and follows new messages down while
 * the reader is at the newest one. Read further up (a jump, the line), it
 * holds still until the reader comes back to the newest. Returns whether
 * the newest message is on screen.
 */
@Composable
private fun follow(listState: LazyListState, newestKey: String?, count: Int, firstIndex: Int, newLine: Int, holding: Boolean, onHolding: (Boolean) -> Unit, onLanded: suspend () -> Boolean): Boolean {
    var landed by remember { mutableStateOf(false) }
    val newestVisible by remember(newestKey) { derivedStateOf { newestKey != null && listState.layoutInfo.visibleItemsInfo.any { it.key == newestKey } } }
    LaunchedEffect(newestVisible) { if (newestVisible) onHolding(false) }
    LaunchedEffect(count) {
        if (count == 0) return@LaunchedEffect
        if (!landed) {
            landed = true
            if (onLanded()) return@LaunchedEffect
            if (newLine >= 0) {
                listState.scrollToItem(firstIndex + newLine)
                onHolding(true)
            } else {
                listState.scrollToItem(firstIndex + count - 1)
            }
            return@LaunchedEffect
        }
        if (!holding) listState.animateScrollToItem(firstIndex + count - 1)
    }
    return newestVisible
}

/**
 * The message being edited ([item], its place in the list) brought to the
 * top of what shows, or as far as the list goes, as editing starts and
 * with each step of the keyboard rising (the list shortens as it does), so
 * its field and Cancel and Save are not left under the composer; the list
 * then holds there ([onHold]) rather than following new messages down.
 */
@Composable
private fun BringEditing(listState: LazyListState, item: Int?, onHold: () -> Unit) {
    val keyboard = WindowInsets.ime.getBottom(LocalDensity.current)
    LaunchedEffect(item, keyboard) {
        if (item == null) return@LaunchedEffect
        onHold()
        listState.scrollToItem(item)
    }
}

// A personal conversation

@Composable
fun ConversationScreen(store: AppStore, session: LumiaSession, channelId: String, display: Display, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val measures = display.measures
    val clock = rememberClock()
    var draft by rememberSaveable(stateSaver = TextFieldValue.Saver) { mutableStateOf(TextFieldValue("")) }
    var error by rememberSaveable { mutableStateOf("") }
    var replying by remember { mutableStateOf<QuoteSummary?>(null) }
    var actionsFor by remember { mutableStateOf<String?>(null) }
    var pickerFor by remember { mutableStateOf<String?>(null) }
    var flash by remember { mutableStateOf<String?>(null) }
    var holding by remember { mutableStateOf(false) }
    // The message being edited in place, and what its field holds; the one asked about deleting.
    var editing by rememberSaveable { mutableStateOf<String?>(null) }
    var editDraft by rememberSaveable(stateSaver = TextFieldValue.Saver) { mutableStateOf(TextFieldValue("")) }
    var deleting by remember { mutableStateOf<String?>(null) }
    val focus = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    val channel = session.snapshot?.channels?.firstOrNull { it.channelId == channelId }
    val state = session.dmChannels.firstOrNull { it.channelId == channelId }
    val log = session.dmLogs[channelId]
    val others = channel?.participantAccountIds?.filter { it != session.me } ?: emptyList()
    val title = channel?.let { session.conversationName(it) } ?: "Conversation"
    val listState = rememberLazyListState()
    // Whose a message is: this account's from any of its devices, else the sender's.
    val me = session.me
    val senderOf = { m: DmMessageSummary -> if (m.mine) me else m.senderAccountId.ifEmpty { m.senderIdentityId } }
    val whoOf = { m: DmMessageSummary -> if (m.mine) session.selfName else if (m.senderAccountId.isNotEmpty() || others.size != 1) session.nameOf(m.senderAccountId) else session.nameOf(others[0]) }
    // Their picture, by the account `whoOf` names; none for a sender known only by an identity among several.
    val pictureOf = { m: DmMessageSummary -> session.avatarOf(if (m.mine) me else m.senderAccountId.ifEmpty { others.singleOrNull() ?: "" }) }
    val sent = log?.messages ?: emptyList()
    // The long messages opened while the conversation is.
    val expanded = remember { mutableStateMapOf<String, Boolean>() }
    // The room the draft and the field being edited have under the instance's limit.
    val room = remember(draft.text) { session.roomOf(draft.text) }
    val editRoom = remember(editing, editDraft.text) { if (editing == null) null else session.roomOf(editDraft.text) }
    val places = remember(sent, me) { blockPlaces(sent.map { Stamp(senderOf(it), it.sentAtMicros, quotes = it.reply != null) }) }
    // Where this account had read to as the conversation opened, while some of it was unread: the line stays until it is left.
    val readTo = remember { session.readStates[channelId]?.takeIf { it.unread > 0u }?.readSeq }
    val newLine = readTo?.let { r -> firstUnread(sent, r, { it.seq }) { !it.mine } } ?: -1
    val member = state?.member == true
    // The list: the notes, the day's divider, then the messages.
    val firstIndex = 2

    LaunchedEffect(channelId) {
        session.refreshDmChannels()
        session.refreshDmLog(channelId)
    }

    fun jump(id: String) {
        scope.launch {
            val at = (session.dmLogs[channelId]?.messages ?: emptyList()).indexOfFirst { it.messageId == id }
            if (at < 0) return@launch
            holding = true
            listState.animateScrollToItem(firstIndex + at)
            flash = id
            delay(FLASH_MS)
            if (flash == id) flash = null
        }
    }

    fun react(id: String, emoji: String) {
        session.dmReact(channelId, id, emoji)
        store.used(emoji)
    }

    fun edit(m: DmMessageSummary) {
        editing = m.messageId
        editDraft = TextFieldValue(m.body, TextRange(m.body.length))
    }

    fun saveEdit(m: DmMessageSummary) {
        if (isOver(editRoom)) return
        when (val outcome = editOutcome(m.body, editDraft.text)) {
            EditOutcome.Unchanged -> editing = null
            EditOutcome.Delete -> deleting = m.messageId
            is EditOutcome.Save -> { editing = null; session.dmEdit(channelId, m.messageId, outcome.body) }
        }
    }

    val newestVisible = follow(listState, sent.lastOrNull()?.messageId, sent.size, firstIndex, newLine, holding, { holding = it }) { false }
    BringEditing(listState, editing?.let { id -> sent.indexOfFirst { it.messageId == id } }?.takeIf { it >= 0 }?.plus(firstIndex)) { holding = true }
    val front = inFront()
    LaunchedEffect(newestVisible, front) {
        while (newestVisible && front) {
            session.dmLogs[channelId]?.messages?.lastOrNull()?.let { session.markRead(channelId, it.seq) }
            delay(MARK_EVERY_MS)
        }
    }
    LaunchedEffect(replying) { if (replying != null) { focus.requestFocus(); keyboard?.show() } }
    BackHandler(enabled = replying != null && editing == null) { replying = null }
    BackHandler(enabled = editing != null) { editing = null }

    PanelScreen(
        back = "Home", onBack = back, title = title,
        // The other side's pronouns, from their verified profile, then the host.
        subtitle = listOf(others.map { session.pronounsOf(it) }.filter { it.isNotEmpty() }.joinToString(", "), channel?.host?.ifEmpty { null }?.let { "on $it" } ?: "").filter { it.isNotEmpty() }.joinToString(" · ").ifEmpty { null },
        lead = { LumiaAvatar(initialsOf(title), AvatarTone.of(others.firstOrNull() ?: channelId), LumiaTokens.avatarMd, others.singleOrNull()?.let { session.avatarOf(it) }) },
    ) {
        Column(Modifier.fillMaxSize().imePadding()) {
            LazyColumn(state = listState, modifier = Modifier.weight(1f).fillMaxWidth(), contentPadding = PaddingValues(vertical = LumiaTokens.space8)) {
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
                item { if (sent.isNotEmpty()) LumiaDayDivider("Messages", measures) }
                itemsIndexed(sent, key = { _, m -> m.messageId }) { i, m ->
                    Column {
                        if (i == newLine) LumiaNewMessagesLine(measures)
                        LumiaMessageRow(
                            whoOf(m), AvatarTone.of(senderOf(m)), timeOfMicros(m.sentAtMicros, clock), m.body, m.deleted, places[i], measures,
                            picture = pictureOf(m),
                            reference = m.reply?.let { q -> @Composable { ReferenceLine(session, q, measures) { jump(q.messageId) } } },
                            edited = m.edited && !m.deleted,
                            expanded = expanded[m.messageId] == true,
                            onExpanded = { expanded[m.messageId] = it },
                            reactions = if (m.reactions.isNotEmpty() && !m.deleted) ({ ReactionChips(session, m.reactions, { react(m.messageId, it) }, { pickerFor = m.messageId }) }) else null,
                            editor = if (editing == m.messageId && !m.deleted) ({ EditField(editDraft, { editDraft = it }, editRoom, { saveEdit(m) }, { editing = null }) }) else null,
                            under = session.editRefusals[m.messageId]?.let { words -> @Composable { LumiaErrorText(words) } },
                            // A reply to this account, from someone else.
                            washed = !m.mine && m.reply?.authorAccountId == me && me.isNotEmpty(),
                            flashed = flash == m.messageId,
                            onLongPress = if (!m.deleted && member) ({ actionsFor = m.messageId }) else null,
                        )
                    }
                }
                val shown = error.ifEmpty { session.lastError ?: "" }
                if (shown.isNotEmpty()) item { Column(Modifier.padding(horizontal = LumiaTokens.space16)) { LumiaErrorText(shown) } }
            }
            // Every message here reaches everyone in it, so the reply bar offers no "@ On".
            val quoting = replying
            LumiaComposer(
                draft, { draft = it }, if (member) "Message $title" else "Setting up the encrypted group…", enabled = member, focus = focus,
                header = quoting?.let { q -> @Composable { ReplyBar(q.authorAccountId.ifEmpty { null }?.let { session.nameOf(it) } ?: title, null, {}) { replying = null } } },
                active = quoting != null,
                // Up in an empty composer edits this account's newest message.
                onKey = { e -> upEdits(e, draft.text) { lastEditable(sent) { it.mine && !it.deleted }?.let { edit(it); true } ?: false } },
                room = room,
            ) {
                val body = draft.text.trim()
                // Over the limit nothing is sent, and the draft stays.
                if (body.isEmpty() || isOver(room)) return@LumiaComposer
                val quote = replying
                draft = TextFieldValue("")
                replying = null
                error = ""
                holding = false
                session.clearError()
                scope.launch { try { session.dmSendWith(channelId, body, quote) } catch (e: Exception) { error = e.lumiaMessage } }
            }
            // Nobody is shown typing in a personal conversation, and nothing is sent when this
            // person does (protocol 6.7); the line keeps its room, so the composer sits where a
            // channel's does and the boards draw it.
            ComposerFoot(emptyList())
        }
    }

    val acting = actionsFor?.let { id -> sent.firstOrNull { it.messageId == id } }
    if (acting != null) {
        // Only this account's own, in a personal conversation: there is no moderator inside the encryption.
        val own = acting.mine && !acting.deleted
        MessageActionsSheet(
            onAddReaction = { actionsFor = null; pickerFor = acting.messageId },
            onReply = { replying = quoteOf(acting.messageId, if (acting.mine) me else acting.senderAccountId, acting.body); actionsFor = null },
            onEdit = if (own) ({ actionsFor = null; edit(acting) }) else null,
            onCopy = { copy(context, acting.body); actionsFor = null },
            onDelete = if (own) ({ actionsFor = null; deleting = acting.messageId }) else null,
            onDismiss = { actionsFor = null },
        )
    }
    pickerFor?.let { id -> EmojiPickerSheet(frequentEmoji(store.recentEmoji), onPick = { react(id, it); pickerFor = null }, onDismiss = { pickerFor = null }) }
    deleting?.let { id ->
        val m = sent.firstOrNull { it.messageId == id }
        DeleteDialog(
            firstLine(m?.body ?: ""),
            onDelete = { deleting = null; if (editing == id) editing = null; session.dmDelete(channelId, id) },
            onDismiss = { deleting = null },
        )
    }
}

// A server's channel or server DM

/**
 * A server's channel or server DM. A channel this account is not in (one
 * opened from Browse) reads as any does, with a bar in place of the
 * composer to join it (architecture 6.8); a server DM is titled by the other
 * person and says once, at its top, whose it is (architecture 6.4). Notices
 * are one quiet line each; an author's avatar or name opens their profile
 * card. Opened at a message ([at], from a Mentions page), it finds it.
 */
@Composable
fun ChannelScreen(store: AppStore, session: LumiaSession, channelId: String, display: Display, back: () -> Unit, push: (Screen) -> Unit, at: String? = null) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val measures = display.measures
    val clock = rememberClock()
    var draft by rememberSaveable(stateSaver = TextFieldValue.Saver) { mutableStateOf(TextFieldValue("")) }
    var error by rememberSaveable { mutableStateOf("") }
    var joining by remember { mutableStateOf(false) }
    var card by remember { mutableStateOf<String?>(null) }
    var replying by remember { mutableStateOf<QuoteSummary?>(null) }
    var actionsFor by remember { mutableStateOf<String?>(null) }
    var pickerFor by remember { mutableStateOf<String?>(null) }
    var outside by remember { mutableStateOf<Outside?>(null) }
    var flash by remember { mutableStateOf<String?>(null) }
    var holding by remember { mutableStateOf(false) }
    // Whether the reply notifies the person it quotes: on for each reply, until turned off.
    var mentionOn by remember { mutableStateOf(true) }
    var editing by rememberSaveable { mutableStateOf<String?>(null) }
    var editDraft by rememberSaveable(stateSaver = TextFieldValue.Saver) { mutableStateOf(TextFieldValue("")) }
    var deleting by remember { mutableStateOf<String?>(null) }
    val focus = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    val channel = session.channelOf(channelId)
    val server = session.snapshot?.servers?.firstOrNull { it.serverId == channel?.serverId }
    val messages = session.messages[channelId] ?: emptyList()
    // The long messages opened while the channel is.
    val expanded = remember { mutableStateMapOf<String, Boolean>() }
    // The room the draft and the field being edited have under the instance's limit.
    val room = remember(draft.text) { session.roomOf(draft.text) }
    val editRoom = remember(editing, editDraft.text) { if (editing == null) null else session.roomOf(editDraft.text) }
    val name = channel?.name ?: "channel"
    val dm = channel?.kind == SERVER_DM
    val other = channel?.let { otherIn(it, session.me) }
    val title = if (dm) channel?.let { session.conversationName(it) } ?: "Conversation" else "# $name"
    val member = session.isIn(channelId)
    val listState = rememberLazyListState()
    // A notice is never in a block, and the message after one opens its own; nor is a quote, which opens one under its name.
    val places = remember(messages) { blockPlaces(messages.map { Stamp(if (it.notice.isEmpty()) it.authorAccountId else "notice:${it.messageId}", it.createdAtMicros, quotes = it.reply != null) }) }
    val readTo = remember { session.readStates[channelId]?.takeIf { it.unread > 0u }?.readSeq }
    val newLine = readTo?.let { r -> firstUnread(messages, r, { it.seq }) { it.notice.isEmpty() && it.authorAccountId != session.me } } ?: -1
    // The list: a server DM's note, the day's divider (or that nothing is here), then the messages.
    val firstIndex = if (dm) 2 else 1
    // The @ list, in a server's text channel this account is in: the `@word` at the caret.
    val query = if (channel?.kind == TEXT && member) mentionAt(draft.text, draft.selection.start, draft.selection.end) else null
    val choices = query?.let { q -> mentionChoices(q.query, session.serverMembers[channel?.serverId] ?: emptyList(), session.me, session.channelMemberIds[channelId], server?.canMentionEveryone == true) } ?: emptyList()

    LaunchedEffect(channelId) {
        session.clearError()
        session.refreshMessages(channelId)
        if (channel?.kind == TEXT) session.loadChannelMembers(channelId)
    }

    fun join() {
        scope.launch {
            joining = true
            error = ""
            try { session.joinChannel(channelId) } catch (e: Exception) { error = e.lumiaMessage } finally { joining = false }
        }
    }

    /** Finds a message (a quote's original, the one a Mentions page opened), reading further back for it, and marks it a moment; nothing when it cannot be found. */
    suspend fun reveal(id: String): Boolean {
        holding = true
        if (!session.reveal(channelId, id)) return false
        val list = session.messages[channelId] ?: return false
        val index = list.indexOfFirst { it.messageId == id }
        if (index < 0) return false
        listState.animateScrollToItem((firstIndex + index - 1).coerceAtLeast(0))
        flash = id
        scope.launch {
            delay(FLASH_MS)
            if (flash == id) flash = null
        }
        return true
    }

    fun react(id: String, emoji: String) {
        session.react(channelId, id, emoji)
        store.used(emoji)
    }

    fun addOutside() {
        val o = outside ?: return
        outside = o.copy(adding = true, error = "")
        scope.launch {
            try {
                val added = session.addChannelMembers(channelId, o.accountIds)
                session.loadChannelMembers(channelId)
                if (added.invitedAccountIds.isEmpty()) {
                    outside = null
                } else {
                    val words = invitedWords(added.invitedAccountIds.map { session.nameOf(it) }, name)
                    outside = o.copy(adding = false, said = words)
                    delay(SAID_FOR_MS)
                    if (outside?.said == words) outside = null
                }
            } catch (e: Exception) {
                outside = o.copy(adding = false, error = addRefused(e.lumiaCode) ?: e.lumiaMessage)
            }
        }
    }

    fun edit(m: MessageSummary) {
        editing = m.messageId
        editDraft = TextFieldValue(m.body, TextRange(m.body.length))
    }

    fun saveEdit(m: MessageSummary) {
        if (isOver(editRoom)) return
        when (val outcome = editOutcome(m.body, editDraft.text)) {
            EditOutcome.Unchanged -> editing = null
            EditOutcome.Delete -> deleting = m.messageId
            is EditOutcome.Save -> { editing = null; session.editMessage(channelId, m.messageId, outcome.body) }
        }
    }

    fun send() {
        val body = draft.text.trim()
        // Over the limit nothing is sent, and the draft stays.
        if (body.isEmpty() || isOver(room)) return
        val quote = replying?.let { sentQuote(it, mentionOn, session.me) }
        draft = TextFieldValue("")
        replying = null
        error = ""
        holding = false
        outside = null
        session.clearError()
        scope.launch {
            try {
                val missed = session.sendMessageWith(channelId, body, quote)
                if (missed.isNotEmpty()) outside = Outside(missed)
            } catch (e: Exception) {
                error = e.lumiaMessage
            }
        }
    }

    BringEditing(listState, editing?.let { id -> messages.indexOfFirst { it.messageId == id } }?.takeIf { it >= 0 }?.plus(firstIndex)) { holding = true }
    val newestVisible = follow(listState, messages.lastOrNull()?.messageId, messages.size, firstIndex, newLine, holding, { holding = it }) {
        at != null && run { reveal(at); true }
    }
    val front = inFront()
    LaunchedEffect(newestVisible, front, member) {
        while (newestVisible && front && member) {
            session.messages[channelId]?.lastOrNull()?.let { session.markRead(channelId, it.seq) }
            delay(MARK_EVERY_MS)
        }
    }
    LaunchedEffect(replying) { if (replying != null) { mentionOn = true; focus.requestFocus(); keyboard?.show() } }
    BackHandler(enabled = replying != null && editing == null) { replying = null }
    BackHandler(enabled = editing != null) { editing = null }

    PanelScreen(
        back = server?.name ?: "the server", onBack = back, title = title, subtitle = server?.name,
        lead = if (dm) ({ LumiaAvatar(initialsOf(title), AvatarTone.of(other ?: channelId), LumiaTokens.avatarMd, other?.let { session.avatarOf(it) }) }) else null,
        actions = if (!dm && channel != null) listOf(Triple(Icons.Filled.Group, "Members") { push(Screen.Members(channelId)) }) else emptyList(),
    ) {
        Column(Modifier.fillMaxSize().imePadding()) {
            LazyColumn(state = listState, modifier = Modifier.weight(1f).fillMaxWidth(), contentPadding = PaddingValues(vertical = LumiaTokens.space8)) {
                if (dm) item { Column(Modifier.padding(horizontal = LumiaTokens.space16, vertical = LumiaTokens.space8)) { LumiaNotice(Icons.Filled.Shield, text = serverDmNote(server?.name ?: "this server")) } }
                if (messages.isEmpty()) item { LumiaEmptyState(Icons.Filled.ChatBubbleOutline, "Nothing here yet", if (member) "Say something." else null) }
                else item { LumiaDayDivider("Today", measures) }
                itemsIndexed(messages, key = { _, m -> m.messageId }) { i, m ->
                    Column {
                        if (i == newLine) LumiaNewMessagesLine(measures)
                        if (m.notice.isNotEmpty()) {
                            noticeText(m.notice, session.nameOf(m.authorAccountId), session.nameOf(m.noticeAccountId))?.let { LumiaNoticeRow(it, timeOfMicros(m.createdAtMicros, clock), measures) }
                        } else {
                            LumiaMessageRow(
                                session.nameOf(m.authorAccountId), AvatarTone.of(m.authorAccountId), timeOfMicros(m.createdAtMicros, clock), m.body, m.deleted, places[i], measures,
                                picture = session.avatarOf(m.authorAccountId),
                                onAuthor = if (server != null) ({ card = m.authorAccountId }) else null,
                                reference = m.reply?.let { q -> @Composable { ReferenceLine(session, q, measures) { scope.launch { reveal(q.messageId) } } } },
                                // Its mentions as `@` and the person's name, as the member list shows them.
                                mentions = m.mentions,
                                nameOf = session::knownNameOf,
                                edited = m.edited && !m.deleted,
                                expanded = expanded[m.messageId] == true,
                                onExpanded = { expanded[m.messageId] = it },
                                reactions = if (m.reactions.isNotEmpty() && !m.deleted) ({ ReactionChips(session, m.reactions, { if (member) react(m.messageId, it) }, { if (member) pickerFor = m.messageId }) }) else null,
                                editor = if (editing == m.messageId && !m.deleted) ({ EditField(editDraft, { editDraft = it }, editRoom, { saveEdit(m) }, { editing = null }) }) else null,
                                under = session.editRefusals[m.messageId]?.let { words -> @Composable { LumiaErrorText(words) } },
                                washed = m.mentionsMe,
                                flashed = flash == m.messageId,
                                onLongPress = if (!m.deleted && member) ({ actionsFor = m.messageId }) else null,
                            )
                        }
                    }
                }
                val shown = error.ifEmpty { session.lastError ?: "" }
                if (shown.isNotEmpty()) item { Column(Modifier.padding(horizontal = LumiaTokens.space16)) { LumiaErrorText(shown) } }
            }
            if (member || channel == null) {
                val missed = outside
                if (choices.isNotEmpty() && query != null) {
                    MentionList(choices, name, session::avatarOf) { choice ->
                        val picked = pickMention(draft.text, query, choice)
                        draft = TextFieldValue(picked.text, TextRange(picked.caret))
                    }
                } else if (missed != null) {
                    OutsideBar(
                        words = missed.said ?: outsideWords(missed.accountIds.map { session.nameOf(it) }, name),
                        channel = name, said = missed.said != null, adding = missed.adding, error = missed.error,
                        onAdd = ::addOutside, onDismiss = { outside = null },
                    )
                }
                // The reply bar, in the composer's frame: "@ On" in a server's text channel, where
                // the quote is a mention, and not for a reply to this account's own message.
                val quoting = replying
                val header: (@Composable () -> Unit)? = quoting?.let { q ->
                    {
                        val offered = channel?.kind == TEXT && offersMention(q, session.me)
                        ReplyBar(session.nameOf(q.authorAccountId), if (offered) mentionOn else null, { mentionOn = !mentionOn }) { replying = null }
                    }
                }
                // Every edit that leaves something in the composer says this person is typing;
                // emptied is not typing (a send), nor is the caret moving. The core spaces what is sent.
                LumiaComposer(
                    draft, { next -> val typed = next.text != draft.text; draft = next; if (typed && next.text.isNotBlank()) session.typed(channelId) }, if (dm) "Message $title" else "Message #$name", focus = focus,
                    header = header,
                    active = quoting != null,
                    // Up in an empty composer edits this account's newest message.
                    onKey = { e -> upEdits(e, draft.text) { lastEditable(messages) { mayEdit(it.authorAccountId, session.me, it.notice.isNotEmpty(), it.deleted) }?.let { edit(it); true } ?: false } },
                    room = room,
                ) { send() }
            } else {
                // Not in it: read it, and join from here (architecture 6.8).
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = LumiaTokens.space16)
                        .background(LumiaTokens.inset.color, RoundedCornerShape(LumiaTokens.radius12))
                        .padding(start = LumiaTokens.space14, end = LumiaTokens.space8, top = LumiaTokens.space8, bottom = LumiaTokens.space8),
                    horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(notInChannel(name), style = LumiaTokens.textBody.style, color = LumiaTokens.inkBody.color, modifier = Modifier.weight(1f))
                    LumiaPrimaryButton("Join", enabled = !joining) { join() }
                }
            }
            ComposerFoot(if (member) session.typingIn(channelId) else emptyList())
        }
    }

    val shown = card
    if (shown != null && server != null) {
        ProfileCard(session, server, shown, onDismiss = { card = null }, openChannel = { push(Screen.Channel(it)) }, openConversation = { push(Screen.Conversation(it)) })
    }
    val acting = actionsFor?.let { id -> messages.firstOrNull { it.messageId == id } }
    if (acting != null) {
        val notice = acting.notice.isNotEmpty()
        MessageActionsSheet(
            onAddReaction = { actionsFor = null; pickerFor = acting.messageId },
            onReply = { replying = quoteOf(acting.messageId, acting.authorAccountId, acting.body); actionsFor = null },
            onEdit = if (mayEdit(acting.authorAccountId, session.me, notice, acting.deleted)) ({ actionsFor = null; edit(acting) }) else null,
            // What is drawn: its mentions by name.
            onCopy = { copy(context, drawnBody(acting.body, acting.mentions, session::knownNameOf).text); actionsFor = null },
            // Its own, or anyone's for a moderator here; the instance decides either way.
            onDelete = if (mayDelete(acting.authorAccountId, session.me, notice, acting.deleted, server?.canManageMessages == true)) ({ actionsFor = null; deleting = acting.messageId }) else null,
            onDismiss = { actionsFor = null },
        )
    }
    pickerFor?.let { id -> EmojiPickerSheet(frequentEmoji(store.recentEmoji), onPick = { react(id, it); pickerFor = null }, onDismiss = { pickerFor = null }) }
    deleting?.let { id ->
        val m = messages.firstOrNull { it.messageId == id }
        DeleteDialog(
            firstLine(m?.let { drawnBody(it.body, it.mentions, session::knownNameOf).text } ?: ""),
            onDelete = { deleting = null; if (editing == id) editing = null; session.deleteMessage(channelId, id) },
            onDismiss = { deleting = null },
        )
    }
}

/**
 * Up on a hardware keyboard in an empty composer: [edit] this account's
 * newest message, as the desktop's composer does. True when it did.
 */
private fun upEdits(e: KeyEvent, draft: String, edit: () -> Boolean): Boolean =
    e.type == KeyEventType.KeyDown && e.key == Key.DirectionUp && draft.isEmpty() && edit()

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

// Mentions

/**
 * The messages that notify this account (they mention it or everyone, or
 * quote it), newest first: a server's ([serverId]), or every server's at
 * Home (null). Each says where it was said over the message as a
 * conversation draws it; a tap opens its channel at it. Older ones are
 * read as the list nears its end.
 */
@Composable
fun MentionsScreen(session: LumiaSession, serverId: String?, back: () -> Unit, open: (channelId: String, messageId: String) -> Unit) {
    val list = session.mentionLists[serverId ?: ""]
    val messages = list?.messages ?: emptyList()
    val listState = rememberLazyListState()
    val server = serverId?.let { id -> session.snapshot?.servers?.firstOrNull { it.serverId == id } }
    // The long messages opened while the page is.
    val expanded = remember { mutableStateMapOf<String, Boolean>() }
    LaunchedEffect(serverId) { session.loadMentions(serverId, more = false) }
    // Within five of the end of what is listed, the page after it.
    LaunchedEffect(listState, messages.size) {
        snapshotFlow { listState.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0 }
            .collect { last -> if (messages.isNotEmpty() && last + 5 >= messages.size) session.loadMentions(serverId, more = true) }
    }
    PanelScreen(
        back = server?.name ?: "Home", onBack = back, title = "Mentions",
        subtitle = if (serverId != null) "Messages here that mention you or reply to you" else "Messages in every server that mention you or reply to you",
    ) {
        LazyColumn(state = listState, modifier = Modifier.fillMaxSize().navigationBarsPadding(), contentPadding = PaddingValues(LumiaTokens.space16), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
            list?.error?.let { e -> item { LumiaErrorText(e) } }
            if (messages.isEmpty()) {
                item {
                    if (list == null || !list.loaded) Text("Loading…", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color, modifier = Modifier.padding(LumiaTokens.space16))
                    else LumiaEmptyState(Icons.Filled.AlternateEmail, "No mentions yet", "When someone mentions you or replies to you, it shows up here.")
                }
            }
            itemsIndexed(messages, key = { _, m -> m.messageId }) { _, m ->
                MentionItem(session, m, home = serverId == null, expanded = expanded[m.messageId] == true, onExpanded = { expanded[m.messageId] = it }) { open(m.channelId, m.messageId) }
            }
        }
    }
}

/** One mention: where, then who and when, and the message with its quote. */
@Composable
private fun MentionItem(session: LumiaSession, m: MessageSummary, home: Boolean, expanded: Boolean, onExpanded: (Boolean) -> Unit, onOpen: () -> Unit) {
    val channel = session.channelOf(m.channelId)
    val serverName = channel?.let { c -> session.snapshot?.servers?.firstOrNull { it.serverId == c.serverId }?.name }
    val place = mentionPlace(channel?.name, serverName, home)
    val author = session.nameOf(m.authorAccountId)
    val at = "${dayOfMicros(m.createdAtMicros)}, ${timeOfMicros(m.createdAtMicros, rememberClock())}"
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(LumiaTokens.radius12))
            .background(LumiaTokens.inset.color)
            .clickable(role = Role.Button, onClick = onOpen)
            .semantics(mergeDescendants = true) { contentDescription = "$author in $place, $at: ${m.body}" }
            .padding(LumiaTokens.space12),
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
    ) {
        Text(place, style = LumiaTokens.textSection.style, color = LumiaTokens.inkMuted.color)
        Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
            LumiaAvatar(initialsOf(author), AvatarTone.of(m.authorAccountId), LumiaTokens.avatarMd, session.avatarOf(m.authorAccountId))
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
                Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8), verticalAlignment = Alignment.Bottom) {
                    Text(author, style = LumiaTokens.textAuthor.style, color = LumiaTokens.ink.color)
                    Text(at, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
                }
                m.reply?.let { q -> ReferenceLine(session, q, null, null) }
                LumiaBody(m.body, mentions = m.mentions, nameOf = session::knownNameOf, edited = m.edited, expanded = expanded, onExpanded = onExpanded)
            }
        }
    }
}
