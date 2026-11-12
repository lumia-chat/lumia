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
import androidx.compose.foundation.layout.Spacer
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
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
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
import uniffi.lumia_ffi.ChannelSummary
import uniffi.lumia_ffi.ServerSummary

@Composable
fun Shell(store: AppStore, session: LumiaSession, devScreen: String?) {
    var place by remember { mutableStateOf<Place>(Place.Home) }
    var stack by remember { mutableStateOf<List<Screen>>(emptyList()) }
    val servers = session.snapshot?.servers ?: emptyList()
    val push: (Screen) -> Unit = { stack = stack + it }
    val pop: () -> Unit = { stack = stack.dropLast(1) }
    val replace: (List<Screen>) -> Unit = { stack = it }

    val channels = session.snapshot?.channels ?: emptyList()
    val activity = LocalContext.current as? MainActivity
    val target = PushRegistrar.target

    LaunchedEffect(Unit) {
        session.load()
        // Push registration once signed in; the smoke keeps the prompt away, and
        // so does a load that found the session over: the sign-in screen is next.
        if (devScreen == null && session.signedIn) activity?.pushSetup?.register()
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
        val snapshot = session.snapshot
        // With no conversation yet, a first server is the better place to land.
        if (place == Place.Home && snapshot?.channels?.none { it.kind in personalKinds } == true) place = Place.Server(first)
        if (BuildConfig.DEBUG && devScreen != null) {
            val channel = snapshot?.channels?.firstOrNull { it.serverId == first && it.kind !in personalKinds }
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
            }
        }
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
        // A conversation's messages take the room Preferences chose for them.
        is Screen.Conversation -> ConversationScreen(session, top.id, store.density, pop)
        is Screen.Channel -> ChannelScreen(session, top.id, store.density, pop)
        Screen.Requests -> RequestsScreen(session, pop, replace)
        Screen.Devices -> DevicesScreen(store, session, pop)
        Screen.Profile -> ProfileScreen(store, session, pop)
        Screen.Preferences -> PreferencesScreen(store, pop)
        Screen.NewMessage -> NewMessageScreen(session, pop, replace)
        Screen.AddServer -> AddServerScreen(session, pop) { id -> replace(emptyList()); place = Place.Server(id) }
        is Screen.NewChannel -> NewChannelScreen(session, servers.firstOrNull { it.serverId == top.serverId }, pop) { id -> place = Place.Server(top.serverId); replace(listOf(Screen.Channel(id))) }
        is Screen.Invite -> InviteScreen(session, servers.firstOrNull { it.serverId == top.serverId }, pop)
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
    Row(
        modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(LumiaTokens.radius12)).clickable(onClick = open).padding(LumiaTokens.space10),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        LumiaAvatar(initialsOf(name), AvatarTone.of(others.firstOrNull() ?: channel.channelId), LumiaTokens.avatarLg)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
            Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8), verticalAlignment = Alignment.Bottom) {
                Text(name, style = LumiaTokens.textAuthor.style, color = LumiaTokens.inkEmphasis.color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                if (last != null) Text(rowTimeOfMicros(last.sentAtMicros), style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
            }
            Text(preview, style = LumiaTokens.textMember.style, color = LumiaTokens.inkSecondary.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

// A server

@Composable
fun ServerScreen(session: LumiaSession, server: ServerSummary, push: (Screen) -> Unit) {
    val channels = (session.snapshot?.channels ?: emptyList()).filter { it.serverId == server.serverId && it.kind !in personalKinds }
    Column(Modifier.fillMaxSize().statusBarsPadding()) {
        SideHeader(server.name, meta = session.host, action = Triple(Icons.Filled.Edit, "New message") { push(Screen.NewMessage) })
        LazyColumn(
            modifier = Modifier.weight(1f).fillMaxWidth(),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(start = LumiaTokens.space12, end = LumiaTokens.space12, bottom = LumiaTokens.space24),
            verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2),
        ) {
            item { SectionTitle("Channels") }
            if (channels.isEmpty()) {
                item { Text("No channels yet. Add one with New channel below.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkQuiet.color, modifier = Modifier.padding(LumiaTokens.space10)) }
            }
            items(channels, key = { it.channelId }) { c ->
                Row(
                    modifier = Modifier.fillMaxWidth().height(LumiaTokens.controlHeight).clip(RoundedCornerShape(LumiaTokens.radius10)).clickable { push(Screen.Channel(c.channelId)) }.padding(horizontal = LumiaTokens.space10),
                    horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text("#", style = LumiaTokens.textNav.style, color = LumiaTokens.inkMuted.color, modifier = Modifier.width(12.dp))
                    Text(c.name, style = LumiaTokens.textNav.style, color = LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Spacer(Modifier.weight(1f))
                }
            }
            item { LumiaLinkButton("New channel", Modifier.padding(horizontal = LumiaTokens.space10, vertical = LumiaTokens.space8)) { push(Screen.NewChannel(server.serverId)) } }
            item { LumiaLinkButton("Invite people", Modifier.padding(horizontal = LumiaTokens.space10, vertical = LumiaTokens.space4)) { push(Screen.Invite(server.serverId)) } }
        }
        Text(session.connection.label, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkQuiet.color, modifier = Modifier.navigationBarsPadding().padding(horizontal = LumiaTokens.space16, vertical = LumiaTokens.space6))
    }
}
