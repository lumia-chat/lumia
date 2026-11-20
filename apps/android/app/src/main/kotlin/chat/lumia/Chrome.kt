// The phone's chrome, after the phone boards: where the shell is and the
// screens over it, the account menu the rail's own avatar opens (the rail
// is `Rail.kt`), the count badge, the sidebar's header and tiles, and the
// floating panel a pushed screen draws itself in (a header with the back
// button, a title and subtitle, and actions).
package chat.lumia

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.initialsOf
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
    /** A server's settings: its sections, a screen of their own on a phone (architecture 7.5). */
    data class ServerSettings(val serverId: String) : Screen()
    /** The Overview section: the server's picture, name, tag, history, files and default channels. */
    data class ServerOverview(val serverId: String) : Screen()
    /** The Roles section: the server's roles in order. */
    data class Roles(val serverId: String) : Screen()
    /** A role's editor, or with no [roleId] a new role's. */
    data class RoleEditor(val serverId: String, val roleId: String? = null) : Screen()
    /** "Add members" to a role, from its editor. */
    data class RoleHolders(val serverId: String, val roleId: String) : Screen()
    /** The Members section: the server's members, their roles and titles. */
    data class ServerMembers(val serverId: String) : Screen()
    /** The Emoji section: the server's own emoji, uploaded, renamed and removed (specs/emoji.md 10). */
    data class ServerEmoji(val serverId: String) : Screen()
    data class Browse(val serverId: String) : Screen()
    data class Members(val channelId: String) : Screen()
    data class AddPeople(val channelId: String) : Screen()
    data class NewServerDm(val serverId: String) : Screen()
    data class YourSettings(val serverId: String) : Screen()
    /** The recovery phrase (identity spec 4.1); [again] after a check with a word wrong. */
    data class Phrase(val again: Boolean = false) : Screen()
    /** The phrase's three-word check. */
    data object PhraseCheck : Screen()
    /** Restore with the phrase, on a phone signed in to an account whose identity is elsewhere (identity spec 4.3). */
    data object Restore : Screen()
    /** Start over with a new identity (identity spec 4.7). */
    data object StartOver : Screen()
    /** Approve another device from this one, which holds the identity (identity spec 4.2). */
    data object Approve : Screen()
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
    is Screen.ServerOverview -> "server-overview $serverId"
    is Screen.Roles -> "roles $serverId"
    is Screen.RoleEditor -> if (roleId == null) "role $serverId" else "role $serverId $roleId"
    is Screen.RoleHolders -> "role-holders $serverId $roleId"
    is Screen.ServerMembers -> "server-members $serverId"
    is Screen.ServerEmoji -> "server-emoji $serverId"
    is Screen.Browse -> "browse $serverId"
    is Screen.Members -> "members $channelId"
    is Screen.AddPeople -> "add-people $channelId"
    is Screen.NewServerDm -> "new-server-dm $serverId"
    is Screen.YourSettings -> "your-settings $serverId"
    is Screen.Mentions -> if (serverId == null) "mentions" else "mentions $serverId"
    is Screen.Phrase -> if (again) "phrase again" else "phrase"
    Screen.PhraseCheck -> "phrase-check"
    Screen.Restore -> "restore"
    Screen.StartOver -> "start-over"
    Screen.Approve -> "approve"
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
        "server-overview" -> Screen.ServerOverview(id)
        "roles" -> Screen.Roles(id)
        // A server's id and then the role's; a new role's editor has the server's alone.
        "role" -> Screen.RoleEditor(id.substringBefore(' '), id.substringAfter(' ', "").ifEmpty { null })
        "role-holders" -> id.substringAfter(' ', "").takeIf { it.isNotEmpty() }?.let { Screen.RoleHolders(id.substringBefore(' '), it) }
        "server-members" -> Screen.ServerMembers(id)
        "server-emoji" -> Screen.ServerEmoji(id)
        "browse" -> Screen.Browse(id)
        "members" -> Screen.Members(id)
        "add-people" -> Screen.AddPeople(id)
        "new-server-dm" -> Screen.NewServerDm(id)
        "your-settings" -> Screen.YourSettings(id)
        "mentions" -> Screen.Mentions(id.ifEmpty { null })
        "phrase" -> Screen.Phrase(again = id == "again")
        "phrase-check" -> Screen.PhraseCheck
        "restore" -> Screen.Restore
        "start-over" -> Screen.StartOver
        "approve" -> Screen.Approve
        else -> null
    }
}

val PlaceSaver: Saver<Place, String> = Saver(save = { it.kept() }, restore = { placeOf(it) })
val StackSaver: Saver<List<Screen>, Any> = listSaver(save = { stack -> stack.map { it.kept() } }, restore = { kept -> kept.mapNotNull(::screenOf) })

/** The design's compact menu is this wide (`.lu-menu--compact`); `tokens.json` has no name for it. */
internal val MenuWidth = 220.dp

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
internal fun AccountMenu(session: LumiaSession, onOpen: (Screen) -> Unit, signingOut: Boolean, onSignOut: () -> Unit) {
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

/**
 * A row of a menu: a glyph and what it does, a button to TalkBack by its
 * label. One that takes something away is in the danger ink ([danger]);
 * one that would do nothing here is drawn faint and takes no tap.
 */
@Composable
internal fun MenuItem(icon: ImageVector, label: String, danger: Boolean = false, enabled: Boolean = true, onClick: () -> Unit) {
    val ink = when {
        !enabled -> LumiaTokens.inkFaint.color
        danger -> LumiaTokens.danger.color
        else -> LumiaTokens.inkBody.color
    }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(LumiaTokens.controlHeight)
            .clip(RoundedCornerShape(LumiaTokens.radius8))
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = LumiaTokens.space10),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space10),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, contentDescription = null, tint = ink, modifier = Modifier.size(18.dp))
        Text(label, style = LumiaTokens.textCompact.style, color = ink)
    }
}

/** The design's rule between a menu's groups (`.lu-menu__rule`). */
@Composable
internal fun MenuRule() {
    Box(Modifier.fillMaxWidth().padding(horizontal = LumiaTokens.space6, vertical = LumiaTokens.space4).height(1.dp).background(LumiaTokens.line.color))
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
    // An action that is not always there to take (a role's Save), drawn after the rest.
    trailing: (@Composable () -> Unit)? = null,
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
                trailing?.invoke()
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(LumiaTokens.line.color))
            content()
        }
    }
}
