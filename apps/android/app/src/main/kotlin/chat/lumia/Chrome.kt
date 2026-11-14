// The phone's chrome, after the phone boards: the rail beside a sidebar as
// one screen, and the floating panel a pushed screen draws itself in (a
// header with the back button, a title and subtitle, and actions).
package chat.lumia

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Logout
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Shield
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import chat.lumia.kit.AvatarTone
import chat.lumia.kit.LumiaAvatar
import chat.lumia.kit.LumiaIconButton
import chat.lumia.kit.LumiaPicture
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.initialsOf
import chat.lumia.kit.personalKinds
import chat.lumia.kit.serverDms
import chat.lumia.kit.sidebarChannels
import chat.lumia.kit.unreadIn
import chat.lumia.kit.withUnread
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.widthIn

/** Where the shell is: Home, or a server. */
sealed class Place {
    data object Home : Place()
    data class Server(val id: String) : Place()
}

/** A screen pushed over the rail and sidebar. */
sealed class Screen {
    data class Conversation(val id: String) : Screen()
    /** A channel, opened at a message ([at], from a Mentions page) or at its newest. */
    data class Channel(val id: String, val at: String? = null) : Screen()
    /** The messages that notify this account, in a server or ([serverId] null) every server (architecture 6.6). */
    data class Mentions(val serverId: String?) : Screen()
    data object Requests : Screen()
    data object Devices : Screen()
    data object Profile : Screen()
    data object Preferences : Screen()
    data object NewMessage : Screen()
    data object AddServer : Screen()
    data class NewChannel(val serverId: String) : Screen()
    data class Invite(val serverId: String) : Screen()
    data class ServerSettings(val serverId: String) : Screen()
    data class Browse(val serverId: String) : Screen()
    data class Members(val channelId: String) : Screen()
    data class AddPeople(val channelId: String) : Screen()
    data class NewServerDm(val serverId: String) : Screen()
    data class YourSettings(val serverId: String) : Screen()
}

// Where the shell is, as Android can keep it while the activity is made
// again (the phone turned) or the process is, in the background: a word
// for the kind and the id it carries, if it carries one.

private fun Place.kept(): String = when (this) {
    Place.Home -> "home"
    is Place.Server -> "server $id"
}

private fun placeOf(kept: String): Place = when {
    kept.startsWith("server ") -> Place.Server(kept.removePrefix("server "))
    else -> Place.Home
}

private fun Screen.kept(): String = when (this) {
    is Screen.Conversation -> "conversation $id"
    is Screen.Channel -> "channel $id"
    Screen.Requests -> "requests"
    Screen.Devices -> "devices"
    Screen.Profile -> "profile"
    Screen.Preferences -> "preferences"
    Screen.NewMessage -> "new-message"
    Screen.AddServer -> "add-server"
    is Screen.NewChannel -> "new-channel $serverId"
    is Screen.Invite -> "invite $serverId"
    is Screen.ServerSettings -> "server-settings $serverId"
    is Screen.Browse -> "browse $serverId"
    is Screen.Members -> "members $channelId"
    is Screen.AddPeople -> "add-people $channelId"
    is Screen.NewServerDm -> "new-server-dm $serverId"
    is Screen.YourSettings -> "your-settings $serverId"
    is Screen.Mentions -> if (serverId == null) "mentions" else "mentions $serverId"
}

/** The screen a kept word names; none for a word this build does not know, and the stack is that much shorter. */
private fun screenOf(kept: String): Screen? {
    val id = kept.substringAfter(' ', "")
    return when (kept.substringBefore(' ')) {
        "conversation" -> Screen.Conversation(id)
        "channel" -> Screen.Channel(id)
        "requests" -> Screen.Requests
        "devices" -> Screen.Devices
        "profile" -> Screen.Profile
        "preferences" -> Screen.Preferences
        "new-message" -> Screen.NewMessage
        "add-server" -> Screen.AddServer
        "new-channel" -> Screen.NewChannel(id)
        "invite" -> Screen.Invite(id)
        "server-settings" -> Screen.ServerSettings(id)
        "browse" -> Screen.Browse(id)
        "members" -> Screen.Members(id)
        "add-people" -> Screen.AddPeople(id)
        "new-server-dm" -> Screen.NewServerDm(id)
        "your-settings" -> Screen.YourSettings(id)
        "mentions" -> Screen.Mentions(id.ifEmpty { null })
        else -> null
    }
}

val PlaceSaver: Saver<Place, String> = Saver(save = { it.kept() }, restore = { placeOf(it) })
val StackSaver: Saver<List<Screen>, Any> = listSaver(save = { stack -> stack.map { it.kept() } }, restore = { kept -> kept.mapNotNull(::screenOf) })

@Composable
fun PhoneRail(session: LumiaSession, place: Place, onAdd: () -> Unit, onAccount: (Screen) -> Unit, signingOut: Boolean, onSignOut: () -> Unit, onPlace: (Place) -> Unit) {
    val servers = session.snapshot?.servers ?: emptyList()
    val channels = session.snapshot?.channels ?: emptyList()
    // Home's badge: the requests that wait and the personal conversations' unread messages (every one counts, in a DM).
    val requests = session.snapshot?.pendingRequests?.size ?: 0
    val personal = unreadIn(session.readStates, channels.filter { it.kind in personalKinds }.map { it.channelId }).mentions
    val homeLabel = when {
        requests == 0 && personal == 0 -> "Home"
        requests == 0 -> "Home, $personal unread"
        personal == 0 -> "Home, $requests requests waiting"
        else -> "Home, $requests requests waiting, $personal unread"
    }
    Column(
        modifier = Modifier.width(LumiaTokens.railWidth).fillMaxHeight().background(LumiaTokens.rail.color).statusBarsPadding().padding(top = LumiaTokens.space8, bottom = LumiaTokens.space12),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
    ) {
        RailTile(current = place == Place.Home, badge = requests + personal, label = homeLabel, home = true, onClick = { onPlace(Place.Home) }) {
            Image(painterResource(R.drawable.logo), contentDescription = null, modifier = Modifier.size(32.dp))
        }
        Box(Modifier.width(24.dp).height(1.dp).background(LumiaTokens.railDivider.color))
        for (s in servers) {
            // What is unread in it, its channels and server DMs: a mark at the rail's edge, and its count for this account over the tile's corner.
            val unread = unreadIn(session.readStates, (sidebarChannels(channels, s.serverId) + serverDms(channels, s.serverId)).map { it.channelId })
            val current = place == Place.Server(s.serverId)
            RailTile(current = current, badge = unread.mentions, label = withUnread(s.name, unread), dot = unread.unread > 0 && unread.mentions == 0 && !current, onClick = { onPlace(Place.Server(s.serverId)) }) {
                // The server's picture, when it has one, over its initials, as a person's is drawn.
                Text(initialsOf(s.name), style = LumiaTokens.titleMonogram.style, color = LumiaTokens.ink.color)
                LumiaPicture(session.pictureOf(s.iconUrl), Modifier.matchParentSize())
            }
        }
        RailTile(current = false, badge = 0, label = "Add a server", onClick = onAdd) {
            Icon(Icons.Filled.Add, contentDescription = null, tint = LumiaTokens.inkSecondary.color, modifier = Modifier.size(20.dp))
        }
        Spacer(Modifier.weight(1f))
        AccountMenu(session, onAccount, signingOut, onSignOut)
    }
}

/** The design's compact menu is this wide (`.lu-menu--compact`); `tokens.json` has no name for it. */
private val MenuWidth = 220.dp

/**
 * The rail's own avatar and the menu it opens: the account's own things, as
 * the web's and the desktop's account menu has them and in their order,
 * and under a rule the way out, "Sign out". No board draws a phone's; it
 * is the design's menu (a surface with a strong line, rows of a glyph and
 * a label) beside the avatar as the web places it, its foot level with the
 * avatar's or as near that as the system's bar lets a popup come. A tap
 * elsewhere or the back button closes it.
 *
 * Signing out asks nothing first, as on the web and the desktop: nothing
 * goes that signing in does not bring back. The tap closes the menu, and
 * the avatar opens none while the sign-out is under way: there is nothing
 * to do with a session that is going.
 */
@Composable
private fun AccountMenu(session: LumiaSession, onOpen: (Screen) -> Unit, signingOut: Boolean, onSignOut: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(LumiaTokens.radius14)
    // Past the rail's edge: the avatar is in the middle of it.
    val beside = with(LocalDensity.current) { (LumiaTokens.avatarSelf + (LumiaTokens.railWidth - LumiaTokens.avatarSelf) / 2 + LumiaTokens.space8).roundToPx() }
    Box {
        Box(
            Modifier.clip(CircleShape).clickable(enabled = !signingOut, onClickLabel = "open the menu", role = Role.Button) { open = true }
                .semantics { contentDescription = session.selfName + ", your account" },
        ) {
            LumiaAvatar(initialsOf(session.selfName), AvatarTone.of(session.me), LumiaTokens.avatarSelf, session.avatarOf(session.me))
        }
        if (open && !signingOut) {
            Popup(alignment = Alignment.BottomStart, offset = IntOffset(beside, 0), onDismissRequest = { open = false }, properties = PopupProperties(focusable = true)) {
                Column(
                    Modifier
                        .width(MenuWidth)
                        // A popup draws a shadow only so far past its own edge: a deeper one is cut off square.
                        .shadow(LumiaTokens.space4, shape)
                        .background(LumiaTokens.surface.color, shape)
                        .border(1.dp, LumiaTokens.lineStrong.color, shape)
                        .padding(LumiaTokens.space6)
                        .semantics { paneTitle = "Your account" },
                    verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2),
                ) {
                    for ((icon, label, screen) in listOf(
                        Triple(Icons.Filled.Person, "View profile", Screen.Profile),
                        Triple(Icons.Filled.Tune, "Preferences", Screen.Preferences),
                        Triple(Icons.Filled.Shield, "Devices", Screen.Devices),
                    )) {
                        MenuItem(icon, label) { open = false; onOpen(screen) }
                    }
                    // The design's rule between a menu's groups (`.lu-menu__rule`).
                    Box(Modifier.fillMaxWidth().padding(horizontal = LumiaTokens.space6, vertical = LumiaTokens.space4).height(1.dp).background(LumiaTokens.line.color))
                    MenuItem(Icons.AutoMirrored.Filled.Logout, "Sign out") { open = false; onSignOut() }
                }
            }
        }
    }
}

/** A row of the menu: a glyph and what it does, a button to TalkBack by its label. */
@Composable
private fun MenuItem(icon: ImageVector, label: String, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(LumiaTokens.controlHeight)
            .clip(RoundedCornerShape(LumiaTokens.radius8))
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = LumiaTokens.space10),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space10),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, contentDescription = null, tint = LumiaTokens.inkBody.color, modifier = Modifier.size(18.dp))
        Text(label, style = LumiaTokens.textCompact.style, color = LumiaTokens.inkBody.color)
    }
}

/** The rail's unread dot (11 across, the phone boards'). */
private val RailDot = (LumiaTokens.space10 + LumiaTokens.space12) / 2

/**
 * A tile of the rail. A server with something unread and nothing for this
 * account has the unread dot at the tile's top right; with mentions, their
 * count over the corner, a little further out ([home]'s further still, as
 * the boards place it); each ringed in the rail's colour.
 */
@Composable
private fun RailTile(current: Boolean, badge: Int, label: String, dot: Boolean = false, home: Boolean = false, onClick: () -> Unit, content: @Composable BoxScope.() -> Unit) {
    val shape = if (current) RoundedCornerShape(LumiaTokens.radius14) else CircleShape
    Box(Modifier.width(LumiaTokens.railWidth), contentAlignment = Alignment.Center) {
        Box {
            Box(
                modifier = Modifier
                    .size(LumiaTokens.tileSize)
                    .clip(shape)
                    .background(if (current) LumiaTokens.tabActive.color else LumiaTokens.tile.color)
                    .clickable(onClick = onClick)
                    .semantics { contentDescription = label },
                contentAlignment = Alignment.Center,
            ) {
                content()
                // The current one's ring, over what the tile holds, a picture too.
                if (current) Box(Modifier.matchParentSize().border(2.dp, LumiaTokens.accent.color, shape))
            }
            if (dot && badge == 0) {
                Box(
                    Modifier.align(Alignment.TopEnd).size(RailDot)
                        .background(LumiaTokens.unread.color, CircleShape)
                        .border(LumiaTokens.space2, LumiaTokens.rail.color, CircleShape),
                )
            }
            // Over the tile's corner: 4 out, Home's 5.
            if (badge > 0) {
                val out = if (home) (LumiaTokens.space4 + LumiaTokens.space6) / 2 else LumiaTokens.space4
                Box(Modifier.align(Alignment.TopEnd).offset(x = out, y = -out)) { CountBadge(badge, BadgeSize.RAIL) }
            }
        }
    }
}

/** Where a count badge is, which decides its size (the phone boards'). */
enum class BadgeSize {
    /** A sidebar row's: 20 high and at least as wide, 6 at its sides. */
    ROW,

    /** A quick tile's: 18 high, 5 at its sides, its count a little smaller. */
    TILE,

    /** The rail's: 20 high, 5 at its sides, ringed 2 in the rail's colour inside that. */
    RAIL,
}

/** The design's count badge: the accent's pill, the count on it, 99+ past that. (`unread` is the dot's colour, not a badge's.) */
@Composable
fun CountBadge(count: Int, size: BadgeSize = BadgeSize.ROW) {
    val high = if (size == BadgeSize.TILE) LumiaTokens.space18 else LumiaTokens.space18 + LumiaTokens.space2
    val side = if (size == BadgeSize.ROW) LumiaTokens.space6 else (LumiaTokens.space4 + LumiaTokens.space6) / 2
    val pill = RoundedCornerShape(LumiaTokens.radiusPill)
    val style = if (size == BadgeSize.TILE) LumiaTokens.textCount.style.copy(fontSize = LumiaTokens.textCount.size * TILE_COUNT) else LumiaTokens.textCount.style
    Box(
        modifier = Modifier.height(high).widthIn(min = high)
            .background(LumiaTokens.accent.color, pill)
            .then(if (size == BadgeSize.RAIL) Modifier.border(LumiaTokens.space2, LumiaTokens.rail.color, pill) else Modifier)
            .padding(horizontal = side),
        contentAlignment = Alignment.Center,
    ) {
        Text(if (count > 99) "99+" else "$count", style = style, color = LumiaTokens.onAccent.color, maxLines = 1)
    }
}

/** A quick tile's count is 10.5 where a badge's is 11 (`text-count`'s note). */
private const val TILE_COUNT = 10.5f / 11f

/** The sidebar's header: a title, a line under it, and one action. */
@Composable
fun SideHeader(title: String, meta: String? = null, action: Triple<ImageVector, String, () -> Unit>? = null) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(start = LumiaTokens.space16, end = LumiaTokens.space6, top = LumiaTokens.space4, bottom = LumiaTokens.space8),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, style = LumiaTokens.titleServer.style, color = LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (meta != null) Text(meta, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (action != null) LumiaIconButton(action.first, action.second, action.third)
    }
}

/**
 * A quick tile, as the phone boards draw Home's Requests / Devices / New
 * and a server's Mentions: 58 high, three to a row, the surface ringed in
 * the tiles' edge, a glyph over its label; its count as the small badge at
 * its top right, 6 down and 7 in. [label] and [count] name it to a screen
 * reader.
 */
@Composable
fun Tile(label: String, icon: ImageVector, count: Int = 0, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val shape = RoundedCornerShape(LumiaTokens.radius14)
    Box(
        modifier = modifier
            .height(58.dp)
            .clip(shape)
            .background(LumiaTokens.surface.color)
            .border(1.dp, LumiaTokens.edge.color, shape)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics(mergeDescendants = true) { contentDescription = if (count > 0) "$label, $count" else label },
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
            Icon(icon, contentDescription = null, tint = LumiaTokens.inkBody.color, modifier = Modifier.size(18.dp))
            Text(label, style = LumiaTokens.textSection.style, color = LumiaTokens.inkBody.color)
        }
        if (count > 0) Box(Modifier.align(Alignment.TopEnd).padding(top = LumiaTokens.space6, end = (LumiaTokens.space6 + LumiaTokens.space8) / 2)) { CountBadge(count, BadgeSize.TILE) }
    }
}

/**
 * A floating panel: the main pane's card, pushed over the rail and
 * sidebar, with its own header.
 */
@Composable
fun PanelScreen(
    back: String,
    onBack: () -> Unit,
    title: String,
    subtitle: String? = null,
    lead: (@Composable () -> Unit)? = null,
    actions: List<Triple<ImageVector, String, () -> Unit>> = emptyList(),
    content: @Composable () -> Unit,
) {
    Box(Modifier.fillMaxSize().background(LumiaTokens.bg.color).statusBarsPadding().padding(start = LumiaTokens.space8, end = LumiaTokens.space8, bottom = LumiaTokens.space8)) {
        Column(
            Modifier
                .fillMaxSize()
                .clip(RoundedCornerShape(LumiaTokens.radius18))
                .background(LumiaTokens.surface.color)
                .border(1.dp, LumiaTokens.panelEdge.color, RoundedCornerShape(LumiaTokens.radius18)),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth().heightIn(min = LumiaTokens.headerHeight).padding(start = LumiaTokens.space4, end = LumiaTokens.space6),
                horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                LumiaIconButton(Icons.AutoMirrored.Filled.ArrowBack, "Back to $back", onBack)
                lead?.invoke()
                Column(Modifier.weight(1f)) {
                    Text(title, style = LumiaTokens.titleChannel.style, color = LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    if (subtitle != null) Text(subtitle, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                for (a in actions) LumiaIconButton(a.first, a.second, a.third)
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(LumiaTokens.line.color))
            content()
        }
    }
}
