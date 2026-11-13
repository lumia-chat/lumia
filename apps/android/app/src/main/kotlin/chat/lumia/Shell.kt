// The app shell on a phone: the rail beside a sidebar (Home's
// conversations and tiles, or a server's channels) as the root, and every
// other screen a floating panel pushed over it; a stack of screens the
// back button pops. Mirrors the iOS ShellView. The rail's own avatar opens
// the account's menu (profile, preferences, devices, and signing out), as
// the web's does.
package chat.lumia

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AlternateEmail
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Shield
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import chat.lumia.kit.Unread
import chat.lumia.kit.unreadIn
import chat.lumia.kit.withUnread
import androidx.compose.foundation.layout.RowScope
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import kotlinx.coroutines.launch
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.foundation.layout.size
import chat.lumia.kit.LumiaIconButton
import chat.lumia.kit.LumiaPrimaryButton
import chat.lumia.kit.invitationLine
import chat.lumia.kit.invitationsIn
import chat.lumia.kit.lumiaMessage
import chat.lumia.kit.LumiaErrorText
import chat.lumia.kit.otherIn
import chat.lumia.kit.serverDms
import chat.lumia.kit.sidebarChannels
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import chat.lumia.kit.AppStore
import chat.lumia.kit.AvatarTone
import chat.lumia.kit.LumiaAvatar
import chat.lumia.kit.LumiaEmptyState
import chat.lumia.kit.LumiaLinkButton
import chat.lumia.kit.LumiaNotice
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.NoticeTone
import chat.lumia.kit.PushRegistrar
import chat.lumia.kit.Route
import chat.lumia.kit.SectionTitle
import chat.lumia.kit.initialsOf
import chat.lumia.kit.personalKinds
import chat.lumia.kit.rowTimeOfMicros
import chat.lumia.push.PushSetup
import uniffi.lumia_ffi.ChannelSummary
import uniffi.lumia_ffi.ServerSummary

@Composable
fun Shell(store: AppStore, session: LumiaSession, devScreen: String?, devChannel: String? = null) {
    // Where the person is, kept when the activity is made again (the phone
    // turned): the place and the screens over it come back as they were,
    // over the session the store kept. A shell opened anew, after signing
    // in, starts at Home.
    var place by rememberSaveable(stateSaver = PlaceSaver) { mutableStateOf<Place>(Place.Home) }
    var stack by rememberSaveable(stateSaver = StackSaver) { mutableStateOf<List<Screen>>(emptyList()) }
    // Whether this shell has landed: on a first server, or on the screen a debug launch names. Once, not again after a turn.
    var landed by rememberSaveable { mutableStateOf(false) }
    // A debug launch's profile card, open over the members it names (`profilecard`).
    var devCard by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val servers = session.snapshot?.servers ?: emptyList()
    val push: (Screen) -> Unit = { stack = stack + it }
    val pop: () -> Unit = { stack = stack.dropLast(1) }
    val replace: (List<Screen>) -> Unit = { stack = it }

    val channels = session.snapshot?.channels ?: emptyList()
    val activity = LocalContext.current as? MainActivity
    val target = PushRegistrar.target

    LaunchedEffect(Unit) {
        session.load()
        // Push registration once signed in, and once per sign-in however often
        // this shell is made (`PushSetup`). The smoke keeps the prompt away
        // unless it gave an endpoint to stand in for a distributor's, and so
        // does a load that found the session over: the sign-in screen is next.
        if ((devScreen == null || PushSetup.standIn != null) && session.signedIn) activity?.pushSetup?.register()
    }
    LaunchedEffect(target, channels.map { it.channelId }) {
        // The conversation a tapped notification names, once the snapshot
        // has it (a cold start loads first): a DM from Home, a mention in
        // its server. The generic notice names none and the app stays put.
        val t = target ?: return@LaunchedEffect
        val channel = channels.firstOrNull { it.channelId == t.channelId } ?: return@LaunchedEffect
        PushRegistrar.target = null
        if (channel.kind in personalKinds) {
            place = Place.Home
            stack = listOf(Screen.Conversation(channel.channelId))
        } else {
            place = Place.Server(channel.serverId)
            stack = listOf(Screen.Channel(channel.channelId))
        }
    }
    LaunchedEffect(servers.map { it.serverId }) {
        val first = servers.firstOrNull()?.serverId ?: return@LaunchedEffect
        if (landed) return@LaunchedEffect
        landed = true
        val snapshot = session.snapshot
        // With no conversation yet, a first server is the better place to land.
        if (place == Place.Home && snapshot?.channels?.none { it.kind in personalKinds } == true) place = Place.Server(first)
        if (BuildConfig.DEBUG && devScreen != null) {
            val ours = sidebarChannels(snapshot?.channels ?: emptyList(), first)
            val channel = ours.firstOrNull { it.name == devChannel } ?: ours.firstOrNull()
            val dm = serverDms(snapshot?.channels ?: emptyList(), first).firstOrNull()
            when (devScreen) {
                "home" -> place = Place.Home
                "server" -> place = Place.Server(first)
                "channel" -> { place = Place.Server(first); if (channel != null) stack = listOf(Screen.Channel(channel.channelId)) }
                "requests" -> { place = Place.Home; stack = listOf(Screen.Requests) }
                "devices" -> { place = Place.Home; stack = listOf(Screen.Devices) }
                "profile" -> { place = Place.Home; stack = listOf(Screen.Profile) }
                "preferences" -> { place = Place.Home; stack = listOf(Screen.Preferences) }
                "new" -> { place = Place.Home; stack = listOf(Screen.NewMessage) }
                "add" -> { place = Place.Home; stack = listOf(Screen.AddServer) }
                "newchannel" -> { place = Place.Server(first); stack = listOf(Screen.NewChannel(first)) }
                "invite" -> { place = Place.Server(first); stack = listOf(Screen.Invite(first)) }
                "serversettings" -> { place = Place.Server(first); stack = listOf(Screen.ServerSettings(first)) }
                "browse" -> { place = Place.Server(first); stack = listOf(Screen.Browse(first)) }
                "preview" -> {
                    place = Place.Server(first)
                    scope.launch {
                        val c = runCatching { session.browseChannels(first) }.getOrNull()?.let { list -> list.firstOrNull { it.name == devChannel } ?: list.firstOrNull() }
                        if (c != null) { session.preview(c); stack = listOf(Screen.Channel(c.channelId)) }
                    }
                }
                "members" -> { place = Place.Server(first); if (channel != null) stack = listOf(Screen.Channel(channel.channelId), Screen.Members(channel.channelId)) }
                "addpeople" -> { place = Place.Server(first); if (channel != null) stack = listOf(Screen.Members(channel.channelId), Screen.AddPeople(channel.channelId)) }
                "profilecard" -> {
                    place = Place.Server(first)
                    if (channel != null) {
                        scope.launch {
                            devCard = runCatching { session.channelMembers(channel.channelId) }.getOrNull()?.firstOrNull { it != session.me }
                            stack = listOf(Screen.Members(channel.channelId))
                        }
                    }
                }
                "serverdm" -> { place = Place.Server(first); if (dm != null) stack = listOf(Screen.Channel(dm.channelId)) }
                "newdm" -> { place = Place.Server(first); stack = listOf(Screen.NewServerDm(first)) }
                "yoursettings" -> { place = Place.Server(first); stack = listOf(Screen.YourSettings(first)) }
                "mentions" -> { place = Place.Server(first); stack = listOf(Screen.Mentions(first)) }
                "homementions" -> { place = Place.Home; stack = listOf(Screen.Mentions(null)) }
            }
        }
    }
    // A screen kept from before (the saved state outlasts the process) whose
    // channel or server the account does not have, having left it since or
    // being another account, is dropped once the snapshot says so.
    LaunchedEffect(channels.map { it.channelId }, servers.map { it.serverId }) {
        if (session.snapshot == null) return@LaunchedEffect
        val kept = stack.filter { screen ->
            when (screen) {
                is Screen.Conversation -> channels.any { it.channelId == screen.id }
                // A channel read without being in it is among the previews.
                is Screen.Channel -> channels.any { it.channelId == screen.id } || session.previews.containsKey(screen.id)
                is Screen.Members -> channels.any { it.channelId == screen.channelId } || session.previews.containsKey(screen.channelId)
                is Screen.AddPeople -> channels.any { it.channelId == screen.channelId } || session.previews.containsKey(screen.channelId)
                is Screen.Browse -> servers.any { it.serverId == screen.serverId }
                is Screen.NewServerDm -> servers.any { it.serverId == screen.serverId }
                is Screen.YourSettings -> servers.any { it.serverId == screen.serverId }
                is Screen.NewChannel -> servers.any { it.serverId == screen.serverId }
                is Screen.Invite -> servers.any { it.serverId == screen.serverId }
                // And Server settings once this account may no longer change them.
                is Screen.ServerSettings -> servers.any { it.serverId == screen.serverId && it.canManage }
                else -> true
            }
        }
        if (kept.size != stack.size) stack = kept
    }
    // The server on screen is the one whose presence this client wants (protocol 6.7).
    LaunchedEffect(place, session.connection) {
        session.watchPresence((place as? Place.Server)?.let { listOf(it.id) } ?: emptyList())
    }
    BackHandler(enabled = stack.isNotEmpty()) { pop() }

    when (val top = stack.lastOrNull()) {
        null -> Row(Modifier.fillMaxSize().background(LumiaTokens.bg.color)) {
            PhoneRail(session, place, onAdd = { push(Screen.AddServer) }, onAccount = push, signingOut = store.signingOut, onSignOut = store::signOut) { place = it }
            when (val p = place) {
                Place.Home -> HomeScreen(store, session, push)
                is Place.Server -> {
                    val server = servers.firstOrNull { it.serverId == p.id }
                    if (server != null) ServerScreen(session, server, push) else HomeScreen(store, session, push)
                }
            }
        }
        // A conversation's messages take the room Preferences chose for them. Each
        // conversation is a screen of its own: what one holds (the draft, a quote
        // being replied with) is not another's.
        is Screen.Conversation -> key(top.id) { ConversationScreen(store, session, top.id, store.density, pop) }
        is Screen.Channel -> key(top.id, top.at) { ChannelScreen(store, session, top.id, store.density, pop, push, top.at) }
        // A mention opens its channel at it, in the Mentions page's place.
        is Screen.Mentions -> MentionsScreen(session, top.serverId, pop) { channelId, messageId ->
            val channel = session.channelOf(channelId)
            if (channel != null && top.serverId == null) place = Place.Server(channel.serverId)
            stack = stack.dropLast(1) + Screen.Channel(channelId, messageId)
        }
        Screen.Requests -> RequestsScreen(session, pop, replace)
        Screen.Devices -> DevicesScreen(store, session, pop)
        Screen.Profile -> ProfileScreen(store, session, pop)
        Screen.Preferences -> PreferencesScreen(store, pop)
        Screen.NewMessage -> NewMessageScreen(session, pop, replace)
        Screen.AddServer -> AddServerScreen(session, pop) { id -> replace(emptyList()); place = Place.Server(id) }
        is Screen.NewChannel -> NewChannelScreen(session, servers.firstOrNull { it.serverId == top.serverId }, pop) { id -> place = Place.Server(top.serverId); replace(listOf(Screen.Channel(id))) }
        is Screen.Invite -> InviteScreen(session, servers.firstOrNull { it.serverId == top.serverId }, pop)
        is Screen.ServerSettings -> ServerSettingsScreen(session, servers.firstOrNull { it.serverId == top.serverId }, pop)
        // Join opens the channel in the Browse screen's place, so back is the server.
        is Screen.Browse -> BrowseScreen(session, servers.firstOrNull { it.serverId == top.serverId }, pop) { id -> stack = stack.dropLast(1) + Screen.Channel(id) }
        is Screen.Members -> ChannelMembersScreen(session, top.channelId, devCard, pop, push, replace)
        is Screen.AddPeople -> AddPeopleScreen(session, top.channelId, pop)
        is Screen.NewServerDm -> NewServerDmScreen(session, servers.firstOrNull { it.serverId == top.serverId }, pop) { id -> stack = stack.dropLast(1) + Screen.Channel(id) }
        is Screen.YourSettings -> YourSettingsScreen(session, servers.firstOrNull { it.serverId == top.serverId }, pop)
    }
}

// Home

@Composable
fun HomeScreen(store: AppStore, session: LumiaSession, push: (Screen) -> Unit) {
    val byHost = remember(session.snapshot) {
        val order = mutableListOf<String>()
        val groups = mutableMapOf<String, MutableList<ChannelSummary>>()
        for (c in session.snapshot?.channels ?: emptyList()) {
            if (c.kind !in personalKinds) continue
            val host = c.host.ifEmpty { session.host }
            if (groups[host] == null) order.add(host)
            groups.getOrPut(host) { mutableListOf() }.add(c)
        }
        order.map { it to (groups[it] ?: emptyList()) }
    }
    LaunchedEffect(byHost.flatMap { it.second.map { c -> c.channelId } }) {
        // The rows show each conversation's last line.
        for (c in byHost.flatMap { it.second }) if (session.dmLogs[c.channelId] == null) session.refreshDmLog(c.channelId)
    }
    Column(Modifier.fillMaxSize().statusBarsPadding()) {
        SideHeader("Home", action = Triple(Icons.Filled.Edit, "New message") { push(Screen.NewMessage) })
        LazyColumn(
            modifier = Modifier.weight(1f).fillMaxWidth(),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(start = LumiaTokens.space12, end = LumiaTokens.space12, bottom = LumiaTokens.space24),
            verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2),
        ) {
            item {
                Row(Modifier.fillMaxWidth().padding(bottom = LumiaTokens.space10), horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
                    Tile("Requests", Icons.Filled.AlternateEmail, session.snapshot?.pendingRequests?.size ?: 0, Modifier.weight(1f)) { push(Screen.Requests) }
                    Tile("Devices", Icons.Filled.Shield, 0, Modifier.weight(1f)) { push(Screen.Devices) }
                    Tile("New", Icons.Filled.Edit, 0, Modifier.weight(1f)) { push(Screen.NewMessage) }
                }
            }
            // Every server's messages that notify this account (architecture 6.6): the badge less the personal conversations'.
            item {
                val personal = unreadIn(session.readStates, byHost.flatMap { it.second }.map { it.channelId }).mentions
                Column(Modifier.padding(bottom = LumiaTokens.space8)) { MentionsRow((session.badge - personal).coerceAtLeast(0)) { push(Screen.Mentions(null)) } }
            }
            if (!session.hasIdentity) {
                item {
                    LumiaNotice(Icons.Filled.Shield, NoticeTone.ACCENT, action = "Link" to { store.route = Route.LINK }, text = "Personal messages are end-to-end encrypted. This phone has no device key yet; a device you already use approves it.")
                }
            } else if (byHost.isEmpty()) {
                item {
                    LumiaEmptyState(Icons.Filled.Lock, "Your conversations", "Personal messages are end-to-end encrypted. Start one with someone you share a server with.", action = "New message" to { push(Screen.NewMessage) })
                }
            }
            for ((host, channels) in byHost) {
                item { SectionTitle(host) }
                items(channels, key = { it.channelId }) { c -> ConversationRow(session, c) { push(Screen.Conversation(c.channelId)) } }
            }
            item { LumiaLinkButton("Add a server", Modifier.padding(horizontal = LumiaTokens.space10, vertical = LumiaTokens.space8)) { push(Screen.AddServer) } }
        }
        Text(session.connection.label, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkQuiet.color, modifier = Modifier.navigationBarsPadding().padding(horizontal = LumiaTokens.space16, vertical = LumiaTokens.space6))
    }
}

/** A conversation in Home's list: who, when, and the last line. */
@Composable
fun ConversationRow(session: LumiaSession, channel: ChannelSummary, open: () -> Unit) {
    val others = channel.participantAccountIds.filter { it != session.me }
    val name = session.conversationName(channel)
    val last = session.dmLogs[channel.channelId]?.messages?.lastOrNull()
    val preview = when {
        last == null -> if (channel.host.isEmpty()) "End-to-end encrypted" else "on ${channel.host}"
        last.deleted -> "Message deleted"
        last.mine -> "You: ${last.body}"
        others.size > 1 -> "${session.nameOf(last.senderAccountId.ifEmpty { last.senderIdentityId })}: ${last.body}"
        else -> last.body
    }
    // Unread: the last line in the strong ink, and the count (every message, in a DM) at the end.
    val unread = session.unread(channel.channelId)
    Row(
        modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(LumiaTokens.radius12)).clickable(onClick = open)
            .semantics(mergeDescendants = true) { contentDescription = withUnread("Conversation with $name", unread) }
            .padding(LumiaTokens.space10),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        LumiaAvatar(initialsOf(name), AvatarTone.of(others.firstOrNull() ?: channel.channelId), LumiaTokens.avatarLg)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
            Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8), verticalAlignment = Alignment.Bottom) {
                Text(name, style = LumiaTokens.textAuthor.style, color = LumiaTokens.inkEmphasis.color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                if (last != null) Text(rowTimeOfMicros(last.sentAtMicros), style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    preview,
                    style = if (unread.unread > 0) LumiaTokens.textMember.style.copy(fontWeight = FontWeight(LumiaTokens.textSection.weight)) else LumiaTokens.textMember.style,
                    color = if (unread.unread > 0) LumiaTokens.ink.color else LumiaTokens.inkSecondary.color,
                    maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
                )
                if (unread.mentions > 0) CountBadge(unread.mentions)
            }
        }
    }
}

/**
 * A row of a sidebar: a channel, a server DM, Mentions. One with unread
 * messages is in the strong ink and weight, and their count for this
 * account at its end; [label] names it to a screen reader with that.
 */
@Composable
fun NavRow(label: String, unread: Unread, open: () -> Unit, content: @Composable RowScope.(strong: Boolean) -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth().height(LumiaTokens.controlHeight).clip(RoundedCornerShape(LumiaTokens.radius10)).clickable(onClick = open)
            .semantics(mergeDescendants = true) { contentDescription = label }
            .padding(horizontal = LumiaTokens.space10),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        content(unread.unread > 0)
        if (unread.mentions > 0) CountBadge(unread.mentions)
    }
}

/** A sidebar row's name: in the strong ink and weight while something in it is unread. */
@Composable
fun RowScope.NavName(name: String, strong: Boolean) {
    Text(
        name,
        style = if (strong) LumiaTokens.textNav.style.copy(fontWeight = FontWeight(LumiaTokens.textSection.weight)) else LumiaTokens.textNav.style,
        // The desktop's: the strong ink for unread, the quiet ink for the rest.
        color = if (strong) LumiaTokens.ink.color else LumiaTokens.inkQuiet.color,
        maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
    )
}

/** The Mentions row at the top of a server's list and of Home's, with the count of those unread. */
@Composable
fun MentionsRow(mentions: Int, open: () -> Unit) {
    NavRow(withUnread("Mentions", Unread(mentions, mentions)), Unread(0, mentions), open) {
        Icon(Icons.Filled.AlternateEmail, contentDescription = null, tint = LumiaTokens.inkMuted.color, modifier = Modifier.size(LumiaTokens.space14))
        NavName("Mentions", false)
    }
}

// A server

/**
 * A server's sidebar (architecture 6.8): invitations waiting, the channels
 * this account is in under a heading whose + creates (to whoever may) or
 * browses them, its server DMs under their own heading and +, then what
 * the server offers this account.
 */
@Composable
fun ServerScreen(session: LumiaSession, server: ServerSummary, push: (Screen) -> Unit) {
    val scope = rememberCoroutineScope()
    val all = session.snapshot?.channels ?: emptyList()
    val channels = sidebarChannels(all, server.serverId)
    val dms = serverDms(all, server.serverId)
    val invitations = invitationsIn(session.snapshot?.channelInvitations ?: emptyList(), server.serverId)
    var adding by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf("") }

    fun answer(channelId: String, join: Boolean) {
        scope.launch {
            busy = channelId
            error = ""
            try {
                if (join) {
                    session.joinChannel(channelId)
                    push(Screen.Channel(channelId))
                } else {
                    session.dismissInvitation(channelId)
                }
            } catch (e: Exception) {
                error = e.lumiaMessage
            } finally {
                busy = null
            }
        }
    }

    Column(Modifier.fillMaxSize().statusBarsPadding()) {
        SideHeader(server.name, meta = session.host, action = Triple(Icons.Filled.Edit, "New message") { push(Screen.NewMessage) })
        LazyColumn(
            modifier = Modifier.weight(1f).fillMaxWidth(),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(start = LumiaTokens.space12, end = LumiaTokens.space12, bottom = LumiaTokens.space24),
            verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2),
        ) {
            items(invitations, key = { "invitation ${it.channel.channelId}" }) { i ->
                Column(
                    Modifier.fillMaxWidth().padding(bottom = LumiaTokens.space6)
                        .background(LumiaTokens.accentWash.color, RoundedCornerShape(LumiaTokens.radius10))
                        .padding(LumiaTokens.space10),
                    verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
                ) {
                    Text(invitationLine(i.channel.name, session.nameOf(i.invitedByAccountId)), style = LumiaTokens.textMember.style, color = LumiaTokens.inkBody.color)
                    Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8), verticalAlignment = Alignment.CenterVertically) {
                        LumiaPrimaryButton("Join", enabled = busy == null) { answer(i.channel.channelId, join = true) }
                        LumiaLinkButton("Dismiss", enabled = busy == null) { answer(i.channel.channelId, join = false) }
                    }
                }
            }
            if (error.isNotEmpty()) item { LumiaErrorText(error) }
            // The server's messages that notify this account (architecture 6.6), with how many are unread.
            item {
                val mentions = unreadIn(session.readStates, (channels + dms).map { it.channelId }).mentions
                Column(Modifier.padding(bottom = LumiaTokens.space8)) { MentionsRow(mentions) { push(Screen.Mentions(server.serverId)) } }
            }
            item {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.weight(1f)) { SectionTitle("Channels") }
                    Box {
                        LumiaIconButton(Icons.Filled.Add, "Add channels") { adding = true }
                        DropdownMenu(expanded = adding, onDismissRequest = { adding = false }, containerColor = LumiaTokens.surface.color) {
                            if (server.canManageChannels) {
                                DropdownMenuItem(text = { Text("Create a channel", style = LumiaTokens.textCompact.style, color = LumiaTokens.inkBody.color) }, onClick = { adding = false; push(Screen.NewChannel(server.serverId)) })
                            }
                            DropdownMenuItem(text = { Text("Browse channels", style = LumiaTokens.textCompact.style, color = LumiaTokens.inkBody.color) }, onClick = { adding = false; push(Screen.Browse(server.serverId)) })
                        }
                    }
                }
            }
            if (channels.isEmpty()) {
                item { Text("No channels yet.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkQuiet.color, modifier = Modifier.padding(horizontal = LumiaTokens.space10)) }
                item { LumiaLinkButton("Browse channels", Modifier.padding(horizontal = LumiaTokens.space2)) { push(Screen.Browse(server.serverId)) } }
            }
            items(channels, key = { it.channelId }) { c ->
                NavRow(withUnread(if (c.private) "${c.name}, private channel" else "${c.name} channel", session.unread(c.channelId)), session.unread(c.channelId), { push(Screen.Channel(c.channelId)) }) { strong ->
                    // A private channel wears a lock where a public one has its #.
                    if (c.private) Icon(Icons.Filled.Lock, contentDescription = null, tint = LumiaTokens.inkMuted.color, modifier = Modifier.size(LumiaTokens.space12))
                    else Text("#", style = LumiaTokens.textNav.style, color = LumiaTokens.inkMuted.color, modifier = Modifier.width(LumiaTokens.space12))
                    NavName(c.name, strong)
                }
            }
            item {
                Row(Modifier.fillMaxWidth().padding(top = LumiaTokens.space8), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.weight(1f)) { SectionTitle("Direct messages") }
                    if (server.canStartDms) LumiaIconButton(Icons.Filled.Add, "Message someone here") { push(Screen.NewServerDm(server.serverId)) }
                }
            }
            items(dms, key = { it.channelId }) { c ->
                val other = otherIn(c, session.me)
                val name = session.conversationName(c)
                NavRow(withUnread("Conversation with $name", session.unread(c.channelId)), session.unread(c.channelId), { push(Screen.Channel(c.channelId)) }) { strong ->
                    LumiaAvatar(initialsOf(name), AvatarTone.of(other ?: c.channelId), LumiaTokens.avatarSm)
                    NavName(name, strong)
                }
            }
            item { LumiaLinkButton("Invite people", Modifier.padding(horizontal = LumiaTokens.space10, vertical = LumiaTokens.space4).padding(top = LumiaTokens.space8)) { push(Screen.Invite(server.serverId)) } }
            // Every member's own switches here (architecture 6.8).
            item { LumiaLinkButton("Your settings in ${server.name}", Modifier.padding(horizontal = LumiaTokens.space10, vertical = LumiaTokens.space4)) { push(Screen.YourSettings(server.serverId)) } }
            // Offered to whoever may change them: the owner, or a member whose roles grant MANAGE_SERVER (architecture 6.7).
            if (server.canManage) {
                item { LumiaLinkButton("Server settings", Modifier.padding(horizontal = LumiaTokens.space10, vertical = LumiaTokens.space4)) { push(Screen.ServerSettings(server.serverId)) } }
            }
        }
        Text(session.connection.label, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkQuiet.color, modifier = Modifier.navigationBarsPadding().padding(horizontal = LumiaTokens.space16, vertical = LumiaTokens.space6))
    }
}
