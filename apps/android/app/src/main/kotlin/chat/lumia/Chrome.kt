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
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import chat.lumia.kit.AvatarTone
import chat.lumia.kit.LumiaAvatar
import chat.lumia.kit.LumiaIconButton
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.initialsOf

/** Where the shell is: Home, or a server. */
sealed class Place {
    data object Home : Place()
    data class Server(val id: String) : Place()
}

/** A screen pushed over the rail and sidebar. */
sealed class Screen {
    data class Conversation(val id: String) : Screen()
    data class Channel(val id: String) : Screen()
    data object Requests : Screen()
    data object Devices : Screen()
    data object Profile : Screen()
    data object NewMessage : Screen()
    data object AddServer : Screen()
    data class NewChannel(val serverId: String) : Screen()
    data class Invite(val serverId: String) : Screen()
}

@Composable
fun PhoneRail(session: LumiaSession, place: Place, onAdd: () -> Unit, onProfile: () -> Unit, onPlace: (Place) -> Unit) {
    val servers = session.snapshot?.servers ?: emptyList()
    Column(
        modifier = Modifier.width(LumiaTokens.railWidth).fillMaxHeight().background(LumiaTokens.rail.color).statusBarsPadding().padding(top = LumiaTokens.space8, bottom = LumiaTokens.space12),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
    ) {
        RailTile(current = place == Place.Home, badge = session.snapshot?.pendingRequests?.size ?: 0, label = "Home", onClick = { onPlace(Place.Home) }) {
            Image(painterResource(R.drawable.logo), contentDescription = null, modifier = Modifier.size(32.dp))
        }
        Box(Modifier.width(24.dp).height(1.dp).background(LumiaTokens.railDivider.color))
        for (s in servers) {
            RailTile(current = place == Place.Server(s.serverId), badge = 0, label = s.name, onClick = { onPlace(Place.Server(s.serverId)) }) {
                Text(initialsOf(s.name), style = LumiaTokens.titleMonogram.style, color = LumiaTokens.ink.color)
            }
        }
        RailTile(current = false, badge = 0, label = "Add a server", onClick = onAdd) {
            Icon(Icons.Filled.Add, contentDescription = null, tint = LumiaTokens.inkSecondary.color, modifier = Modifier.size(20.dp))
        }
        Spacer(Modifier.weight(1f))
        Box(Modifier.clip(CircleShape).clickable(onClick = onProfile).semantics { contentDescription = session.selfName + ", open profile" }) {
            LumiaAvatar(initialsOf(session.selfName), AvatarTone.of(session.me), LumiaTokens.avatarSelf)
        }
    }
}

@Composable
private fun RailTile(current: Boolean, badge: Int, label: String, onClick: () -> Unit, content: @Composable () -> Unit) {
    val shape = if (current) RoundedCornerShape(LumiaTokens.radius14) else CircleShape
    Box(
        modifier = Modifier
            .size(LumiaTokens.tileSize)
            .clip(shape)
            .background(if (current) LumiaTokens.tabActive.color else LumiaTokens.tile.color)
            .border(2.dp, if (current) LumiaTokens.accent.color else LumiaTokens.tile.color, shape)
            .clickable(onClick = onClick)
            .semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        content()
        if (badge > 0) {
            Box(Modifier.align(Alignment.TopEnd)) { CountBadge(badge) }
        }
    }
}

@Composable
fun CountBadge(count: Int) {
    Box(
        modifier = Modifier.heightIn(min = 18.dp).background(LumiaTokens.unread.color, CircleShape).padding(horizontal = LumiaTokens.space4),
        contentAlignment = Alignment.Center,
    ) {
        Text("$count", style = LumiaTokens.textCount.style, color = LumiaTokens.onAccent.color)
    }
}

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

/** The Requests / Devices / New message tiles. */
@Composable
fun Tile(label: String, icon: ImageVector, count: Int = 0, modifier: Modifier = Modifier, onClick: () -> Unit) {
    Box(
        modifier = modifier
            .height(58.dp)
            .clip(RoundedCornerShape(LumiaTokens.radius14))
            .background(LumiaTokens.tile.color)
            .border(1.dp, LumiaTokens.line.color, RoundedCornerShape(LumiaTokens.radius14))
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
            Icon(icon, contentDescription = null, tint = LumiaTokens.inkBody.color, modifier = Modifier.size(18.dp))
            Text(label, style = LumiaTokens.textCompact.style, color = LumiaTokens.inkBody.color)
        }
        if (count > 0) Box(Modifier.align(Alignment.TopEnd).padding(LumiaTokens.space6)) { CountBadge(count) }
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
