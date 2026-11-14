// The rail: Home, the servers and folders in the account's own order, +,
// and the account's own avatar at its foot (architecture 6.9; the "Server
// folders" boards and the phone boards). Servers, folders, Home and + are
// rounded squares; only the avatar stays round. A closed folder is one
// tile in its tone with its first four servers small; open, its glyph and
// then its servers in a column of its tone; a tap opens or closes it, and
// which are open is this phone's (`AppStore.openFolders`).
//
// A long press lifts a server or a folder: moving drags it (between tiles,
// the accent's line; onto a server's middle, a folder of the two; onto a
// folder, into it; out of an open folder between rail items, out of it),
// and letting go without moving opens its menu as a sheet. TalkBack has
// the menu's items as the tile's own actions, and every move is said.
// The edits are the core's (`Client`'s `rail…` calls through
// `LumiaSession.arrange`); what is the phone's to decide is `Rail.kt` in
// lumiakit.
package chat.lumia

import android.widget.Toast
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ArrowDownward
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.outlined.CreateNewFolder
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material.icons.outlined.FolderOff
import androidx.compose.material.icons.outlined.Tune
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.CollectionInfo
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.collectionInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.onLongClick
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.PlatformTextStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import chat.lumia.kit.AppStore
import chat.lumia.kit.AvatarTone
import chat.lumia.kit.DropZone
import chat.lumia.kit.LocalLumiaDark
import chat.lumia.kit.LumiaField
import chat.lumia.kit.LumiaPicture
import chat.lumia.kit.LumiaPrimaryButton
import chat.lumia.kit.LumiaSecondaryButton
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.RailDrop
import chat.lumia.kit.RailTile
import chat.lumia.kit.Unread
import chat.lumia.kit.dropZone
import chat.lumia.kit.folderFaces
import chat.lumia.kit.folderIn
import chat.lumia.kit.folderLabel
import chat.lumia.kit.folderTitle
import chat.lumia.kit.folderUnread
import chat.lumia.kit.id
import chat.lumia.kit.initialsOf
import chat.lumia.kit.label
import chat.lumia.kit.movedWords
import chat.lumia.kit.personalKinds
import chat.lumia.kit.railDrop
import chat.lumia.kit.serverDms
import chat.lumia.kit.showsDot
import chat.lumia.kit.sidebarChannels
import chat.lumia.kit.siblingsOf
import chat.lumia.kit.stored
import chat.lumia.kit.toneNamed
import chat.lumia.kit.unreadIn
import chat.lumia.kit.withUnread
import androidx.compose.foundation.isSystemInDarkTheme
import uniffi.lumia_ffi.RailItem
import uniffi.lumia_ffi.ServerSummary

/** How much a lifted tile grows (the phone board's 1.1). */
private const val LIFTED = 1.1f

/** A tile being dragged lets what is under it show a little (the board's 0.92). */
private const val MOVING = 0.92f

/** The rail's unread dot (11 across, the phone boards'). */
private val RailDot = (LumiaTokens.space10 + LumiaTokens.space12) / 2

/** A closed folder's servers drawn small: 14 across, 4 apart, rounded 4. */
private val Mini = LumiaTokens.space14

/** The accent's line a drop between tiles shows: 36 by 4. */
private val LineWidth = LumiaTokens.tileSize - LumiaTokens.space8

/** An open folder's glyph: as wide as a tile, 32 high. */
private val GlyphHeight = LumiaTokens.tileSize - LumiaTokens.space12

/**
 * Where a drag is, over the rail: the tile lifted, where the finger is in
 * the rail's space and where it took the tile, what letting go would do,
 * and where every tile and open folder is drawn, which is how a drag finds
 * what it is over.
 */
private class RailDrag {
    var lifted by mutableStateOf<RailTile?>(null)
    var moved by mutableStateOf(false)
    var at by mutableStateOf(Offset.Zero)
    var grab by mutableStateOf(Offset.Zero)
    var drop by mutableStateOf<RailDrop?>(null)
    var space: LayoutCoordinates? = null
    val tiles = mutableMapOf<String, Pair<RailTile, LayoutCoordinates>>()
    val groups = mutableMapOf<String, LayoutCoordinates>()

    fun bounds(c: LayoutCoordinates?): Rect? {
        val s = space ?: return null
        if (c == null || !s.isAttached || !c.isAttached) return null
        return s.localBoundingBoxOf(c, clipBounds = false)
    }

    /** Where the tile of this id is drawn: an open folder's whole column, else the tile. */
    fun boundsOf(id: String): Rect? = bounds(groups[id] ?: tiles[id]?.second)

    /** What letting go here would do: over the tile nearest the finger, by where on it. */
    fun find(rail: List<RailItem>, dragged: String): RailDrop? {
        val y = at.y
        val placed = tiles.values.mapNotNull { (tile, c) -> bounds(c)?.let { tile to it } }.sortedBy { it.second.top }
        if (placed.isEmpty() || rail.isEmpty()) return null
        val folder = rail.any { it is RailItem.Folder && it.folderId == dragged }
        val last = rail.last()
        if (y > placed.last().second.bottom) return railDrop(rail, dragged, RailTile(last.id, folder = last is RailItem.Folder), DropZone.AFTER)
        val (tile, b) = placed.minBy { (_, b) -> if (y < b.top) b.top - y else if (y > b.bottom) y - b.bottom else 0f }
        return railDrop(rail, dragged, tile, dropZone((y - b.top).coerceIn(0f, b.height), b.height, between = folder))
    }
}

/** What a tile's menu and its TalkBack actions offer: a label, a glyph, and what it does. */
private data class RailAction(val label: String, val icon: ImageVector, val danger: Boolean = false, val ruled: Boolean = false, val run: () -> Unit)

@Composable
fun PhoneRail(store: AppStore, session: LumiaSession, place: Place, onAdd: () -> Unit, onAccount: (Screen) -> Unit, signingOut: Boolean, onSignOut: () -> Unit, onPlace: (Place) -> Unit) {
    val servers = session.snapshot?.servers ?: emptyList()
    val byId = servers.associateBy { it.serverId }
    val channels = session.snapshot?.channels ?: emptyList()
    val rail = session.rail.filter { item -> item !is RailItem.Server || item.serverId in byId }
    val context = LocalContext.current
    val view = LocalView.current
    val haptics = LocalHapticFeedback.current
    val density = LocalDensity.current
    val drag = remember { RailDrag() }
    // The tile whose menu is open, and its page: its items, or the folders to move it to.
    var menuFor by rememberSaveable { mutableStateOf<String?>(null) }
    var foldersPage by rememberSaveable { mutableStateOf(false) }
    var settingsFor by rememberSaveable { mutableStateOf<String?>(null) }

    // Home's badge: the requests that wait and the personal conversations' unread messages (every one counts, in a DM).
    val requests = session.snapshot?.pendingRequests?.size ?: 0
    val personal = unreadIn(session.readStates, channels.filter { it.kind in personalKinds }.map { it.channelId }).mentions
    val homeLabel = when {
        requests == 0 && personal == 0 -> "Home"
        requests == 0 -> "Home, $personal unread"
        personal == 0 -> "Home, $requests requests waiting"
        else -> "Home, $requests requests waiting, $personal unread"
    }
    fun unreadOf(serverId: String): Unread = unreadIn(session.readStates, (sidebarChannels(channels, serverId) + serverDms(channels, serverId)).map { it.channelId })
    fun nameOf(serverId: String): String = byId[serverId]?.name ?: ""
    fun titleOf(folder: RailItem.Folder): String =
        folderTitle(folder.name, session.railNow { railFolderWords(folder.serverIds.map(::nameOf)) })
    fun nameOfTile(id: String): String = folderIn(rail, id)?.let(::titleOf) ?: nameOf(id)

    // A refused or unreached arrangement: the snapshot's rail is drawn again, and this says why.
    LaunchedEffect(session.railError) {
        session.takeRailError()?.let { Toast.makeText(context, it, Toast.LENGTH_LONG).show() }
    }

    /** An edit made and kept, and said to a screen reader. */
    fun edit(id: String, change: uniffi.lumia_ffi.Client.(List<RailItem>) -> List<RailItem>) {
        val before = session.rail
        val name = nameOfTile(id)
        val next = session.arrange(change) ?: return
        val wasIn = session.railNow { railFolderOf(before, id) }
        val nowIn = session.railNow { railFolderOf(next, id) }
        val nowFolder = folderIn(next, nowIn)
        val words = movedWords(
            name = name,
            wasIn = wasIn,
            nowIn = nowIn,
            nowInTitle = nowFolder?.let { f -> folderTitle(f.name, session.railNow { railFolderWords(f.serverIds.map(::nameOf)) }) },
            newFolder = nowIn != null && folderIn(before, nowIn) == null,
            position = session.railNow { railPositionOf(next, id) }?.toInt(),
        )
        view.announceForAccessibility(words)
    }

    /** "Move to folder"'s list: every folder the server is not in, by how it reads. */
    fun moveTo(id: String): List<RailAction> {
        val inFolder = session.railNow { railFolderOf(rail, id) }
        return rail.filterIsInstance<RailItem.Folder>().filter { it.folderId != inFolder }.map { f ->
            RailAction("Move to ${titleOf(f)}", Icons.Outlined.Folder) { edit(id) { railIntoFolder(it, id, f.folderId) } }
        }
    }

    fun actionsOf(id: String, withFolders: Boolean): List<RailAction> {
        val position = session.railNow { railPositionOf(rail, id) }?.toInt() ?: return emptyList()
        val count = siblingsOf(rail, id)
        val steps = buildList {
            if (position > 1) add(RailAction("Move up", Icons.Filled.ArrowUpward) { edit(id) { railStep(it, id, -1) } })
            if (position < count) add(RailAction("Move down", Icons.Filled.ArrowDownward) { edit(id) { railStep(it, id, 1) } })
        }
        val folder = folderIn(rail, id)
        if (folder != null) {
            return steps + listOf(
                RailAction("Folder settings…", Icons.Outlined.Tune, ruled = true) { settingsFor = id },
                RailAction("Delete folder", Icons.Outlined.Delete, danger = true, ruled = true) {
                    val title = titleOf(folder)
                    if (session.arrange { railDeleteFolder(it, id) } != null) view.announceForAccessibility("$title deleted")
                },
            )
        }
        val inFolder = session.railNow { railFolderOf(rail, id) }
        val others = rail.filterIsInstance<RailItem.Folder>().filter { it.folderId != inFolder }
        return steps + buildList {
            add(
                RailAction("New folder", Icons.Outlined.CreateNewFolder, ruled = true) {
                    val made = session.railNow { railNewFolderId() }
                    edit(id) { railNewFolder(it, id, made) }
                    // Made from the menu, it asks for its name and tone (not when made by a drag).
                    if (folderIn(session.rail, made) != null) settingsFor = made
                },
            )
            if (others.isNotEmpty()) {
                if (withFolders) addAll(moveTo(id)) else add(RailAction("Move to folder", Icons.Outlined.Folder) { foldersPage = true })
            }
            if (inFolder != null) add(RailAction("Remove from folder", Icons.Outlined.FolderOff) { edit(id) { railOutOfFolder(it, id) } })
        }
    }

    fun dropped(dragged: String, drop: RailDrop) {
        when (drop) {
            is RailDrop.Between -> edit(dragged) { railMoveTo(it, dragged, drop.folder, drop.before) }
            is RailDrop.OntoServer -> {
                val made = session.railNow { railNewFolderId() }
                edit(dragged) { railMerge(it, dragged, drop.server, made) }
            }
            is RailDrop.IntoFolder -> edit(dragged) { railIntoFolder(it, dragged, drop.folder) }
        }
    }

    /** The long press, the drag and the tap of one tile. */
    @Composable
    fun Modifier.gestures(tile: RailTile, onTap: () -> Unit): Modifier {
        var coords by remember { mutableStateOf<LayoutCoordinates?>(null) }
        val tap by rememberUpdatedState(onTap)
        val currentRail by rememberUpdatedState(rail)
        DisposableEffect(tile.id) { onDispose { drag.tiles.remove(tile.id) } }
        return this
            .onGloballyPositioned { coords = it; drag.tiles[tile.id] = tile to it }
            .pointerInput(tile) {
                val slop = viewConfiguration.touchSlop
                val hold = viewConfiguration.longPressTimeoutMillis
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    var tapped = false
                    // Held still for the long press's time: null, timed out.
                    val held = withTimeoutOrNull(hold) {
                        while (true) {
                            val change = awaitPointerEvent().changes.firstOrNull { it.id == down.id } ?: break
                            if (!change.pressed) { tapped = !change.isConsumed; break }
                            if (change.isConsumed || (change.position - down.position).getDistance() > slop) break
                        }
                        false
                    } == null
                    if (!held) {
                        if (tapped) tap()
                        return@awaitEachGesture
                    }
                    // Lifted where it is: the finger keeps its place on the tile.
                    val space = drag.space
                    val own = coords
                    if (space == null || own == null || !own.isAttached) return@awaitEachGesture
                    haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                    drag.grab = down.position
                    drag.at = space.localPositionOf(own, down.position)
                    drag.moved = false
                    drag.drop = null
                    drag.lifted = tile
                    try {
                        while (true) {
                            val change = awaitPointerEvent().changes.firstOrNull { it.id == down.id } ?: break
                            if (!change.pressed) { change.consume(); break }
                            if (!drag.moved && (change.position - down.position).getDistance() > slop) drag.moved = true
                            if (drag.moved && own.isAttached) {
                                drag.at = space.localPositionOf(own, change.position)
                                drag.drop = drag.find(currentRail, tile.id)
                            }
                            change.consume()
                        }
                        val drop = drag.drop
                        if (!drag.moved) { menuFor = tile.id; foldersPage = false } else if (drop != null) dropped(tile.id, drop)
                    } finally {
                        drag.lifted = null
                        drag.drop = null
                        drag.moved = false
                    }
                }
            }
    }

    /** What a tile is to TalkBack beyond its words: a button, its tap, its menu, and the menu's items as its actions. */
    fun Modifier.spoken(label: String, id: String, current: Boolean, expanded: Boolean?, onTap: () -> Unit): Modifier = semantics {
        role = Role.Button
        contentDescription = label
        if (expanded != null) stateDescription = if (expanded) "Expanded" else "Collapsed"
        onClick { onTap(); true }
        onLongClick("Open the menu") { menuFor = id; foldersPage = false; true }
        customActions = actionsOf(id, withFolders = true).map { a -> CustomAccessibilityAction(a.label) { a.run(); true } }
    }

    @Composable
    fun ServerTile(server: ServerSummary, inFolder: String?) {
        val unread = unreadOf(server.serverId)
        val current = place == Place.Server(server.serverId)
        val open = { onPlace(Place.Server(server.serverId)) }
        val tile = RailTile(server.serverId, inFolder = inFolder)
        RailSquare(
            ring = current || (drag.drop as? RailDrop.OntoServer)?.server == server.serverId,
            grown = (drag.drop as? RailDrop.OntoServer)?.server == server.serverId,
            badge = unread.mentions,
            dot = showsDot(unread, current),
            hidden = drag.lifted?.id == server.serverId,
            modifier = Modifier
                .gestures(tile, open)
                .spoken(withUnread(server.name, unread), server.serverId, current, null, open),
        ) { ServerFace(session, server) }
    }

    @Composable
    fun FolderTile(folder: RailItem.Folder) {
        val open = folder.folderId in store.openFolders
        val title = titleOf(folder)
        val tone = toneNamed(folder.tone, folder.folderId)
        val toggle = { store.toggleFolder(folder.folderId, session.rail) }
        val lifted = drag.lifted?.id == folder.folderId
        val into = (drag.drop as? RailDrop.IntoFolder)?.folder == folder.folderId
        if (!open) {
            val unread = folderUnread(folder.serverIds.map(::unreadOf))
            val holdsCurrent = folder.serverIds.any { place == Place.Server(it) }
            RailSquare(
                ring = into || holdsCurrent,
                grown = into,
                badge = unread.mentions,
                dot = showsDot(unread, holdsCurrent),
                fill = tone.fill.color,
                edge = tone.ink.color.copy(alpha = EDGE_ALPHA),
                hidden = lifted,
                modifier = Modifier
                    .gestures(RailTile(folder.folderId, folder = true), toggle)
                    .spoken(folderLabel(title, unread), folder.folderId, holdsCurrent, false, toggle),
            ) { FolderFaces(session, folder.serverIds.mapNotNull { byId[it] }) }
            return
        }
        val shape = RoundedCornerShape(LumiaTokens.radius18)
        Column(
            Modifier
                .alpha(if (lifted) 0f else 1f)
                .onGloballyPositioned { drag.groups[folder.folderId] = it }
                .width(LumiaTokens.tileSize + LumiaTokens.space8)
                .background(tone.fill.color, shape)
                .then(if (into) Modifier.border(2.dp, LumiaTokens.accent.color, shape) else Modifier)
                .padding(top = LumiaTokens.space4, bottom = LumiaTokens.space8)
                .semantics { collectionInfo = CollectionInfo(folder.serverIds.size, 1) },
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(LumiaTokens.space10),
        ) {
            DisposableEffect(folder.folderId) { onDispose { drag.groups.remove(folder.folderId) } }
            Box(
                Modifier
                    .width(LumiaTokens.tileSize).height(GlyphHeight)
                    .clip(RoundedCornerShape(LumiaTokens.radius12))
                    .gestures(RailTile(folder.folderId, folder = true), toggle)
                    .spoken(folderLabel(title, Unread.None), folder.folderId, false, true, toggle),
                contentAlignment = Alignment.Center,
            ) {
                Icon(Icons.Outlined.Folder, contentDescription = null, tint = tone.ink.color, modifier = Modifier.size(LumiaTokens.space18))
            }
            for (id in folder.serverIds) byId[id]?.let { ServerTile(it, folder.folderId) }
        }
    }

    Box(
        Modifier.width(LumiaTokens.railWidth).fillMaxHeight().background(LumiaTokens.rail.color)
            .onGloballyPositioned { drag.space = it },
    ) {
        Column(
            modifier = Modifier.fillMaxHeight().width(LumiaTokens.railWidth).statusBarsPadding().padding(top = LumiaTokens.space8, bottom = LumiaTokens.space12),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            RailSquare(home = true, badge = requests + personal, ring = place == Place.Home, modifier = Modifier.clickable(onClickLabel = null, role = Role.Button) { onPlace(Place.Home) }.semantics { contentDescription = homeLabel }) {
                Image(painterResource(R.drawable.logo), contentDescription = null, modifier = Modifier.size(LumiaTokens.tileSize - LumiaTokens.space8))
            }
            Spacer(Modifier.height(LumiaTokens.space12))
            Box(Modifier.width(LumiaTokens.space24).height(1.dp).background(LumiaTokens.railDivider.color))
            // 12 to the first tile, 4 of it inside the list, room for its ring and badge.
            Spacer(Modifier.height(LumiaTokens.space8))
            Column(
                Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState(), enabled = drag.lifted == null)
                    .padding(vertical = LumiaTokens.space4)
                    .semantics { collectionInfo = CollectionInfo(rail.size, 1) },
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
            ) {
                for (item in rail) {
                    when (item) {
                        is RailItem.Server -> byId[item.serverId]?.let { ServerTile(it, null) }
                        is RailItem.Folder -> FolderTile(item)
                    }
                }
                AddTile(onAdd)
            }
            Spacer(Modifier.height(LumiaTokens.space12))
            AccountMenu(session, onAccount, signingOut, onSignOut)
        }
        // The lifted tile over everything, at the finger, and the line where a drop between tiles would go.
        val lifted = drag.lifted
        if (lifted != null) {
            val drop = drag.drop
            val shape = RoundedCornerShape(LumiaTokens.radius14)
            Box(
                Modifier
                    .offset { (drag.at - drag.grab).let { IntOffset(it.x.toInt(), it.y.toInt()) } }
                    .graphicsLayer { scaleX = LIFTED; scaleY = LIFTED; alpha = if (drag.moved) MOVING else 1f; shadowElevation = LumiaTokens.space12.toPx(); this.shape = shape; clip = true },
            ) {
                val folder = folderIn(rail, lifted.id)
                if (folder != null) {
                    val tone = toneNamed(folder.tone, folder.folderId)
                    Box(Modifier.size(LumiaTokens.tileSize).background(tone.fill.color, shape), contentAlignment = Alignment.Center) {
                        FolderFaces(session, folder.serverIds.mapNotNull { byId[it] })
                    }
                } else {
                    byId[lifted.id]?.let { s -> Box(Modifier.size(LumiaTokens.tileSize).background(LumiaTokens.tile.color, shape), contentAlignment = Alignment.Center) { ServerFace(session, s) } }
                }
            }
            // Over the lifted tile, which is under the finger where the line would be.
            if (drop is RailDrop.Between) {
                val y = lineAt(drag, rail, drop, density)
                if (y != null) {
                    val half = with(density) { Offset(LineWidth.toPx() / 2, LumiaTokens.space2.toPx()) }
                    val middle = with(density) { LumiaTokens.railWidth.toPx() / 2 }
                    Box(
                        Modifier.offset { IntOffset((middle - half.x).toInt(), (y - half.y).toInt()) }
                            .size(LineWidth, LumiaTokens.space4)
                            .background(LumiaTokens.accent.color, RoundedCornerShape(LumiaTokens.space2)),
                    )
                }
            }
        }
    }

    val menuId = menuFor
    if (menuId != null && rail.any { it.id == menuId || (it is RailItem.Folder && menuId in it.serverIds) }) {
        RailMenu(
            title = nameOfTile(menuId),
            actions = if (foldersPage) moveTo(menuId) else actionsOf(menuId, withFolders = false),
            onDismiss = { menuFor = null; foldersPage = false },
        )
    } else if (menuId != null) {
        menuFor = null
    }
    val settingsId = settingsFor
    val settingsFolder = settingsId?.let { folderIn(rail, it) }
    if (settingsFolder != null) {
        FolderSettings(
            folder = settingsFolder,
            words = session.railNow { railFolderWords(settingsFolder.serverIds.map(::nameOf)) },
            onDismiss = { settingsFor = null },
        ) { name, tone ->
            settingsFor = null
            session.arrange { railSetFolder(it, settingsFolder.folderId, name, tone.stored) }
        }
    }
}

/** The inset line a closed folder is drawn with: its tone's ink, faint (the board's 15%). */
private const val EDGE_ALPHA = 0.15f

/** Where the line between tiles goes for [drop], in the rail's space: half a gap above the tile it is before, or below the last. */
private fun lineAt(drag: RailDrag, rail: List<RailItem>, drop: RailDrop.Between, density: androidx.compose.ui.unit.Density): Float? {
    val folder = drop.folder
    val gap = with(density) { (if (folder == null) LumiaTokens.space12 else LumiaTokens.space10).toPx() / 2 }
    val before = drop.before
    if (before != null) return drag.boundsOf(before)?.let { it.top - gap }
    val last = if (folder == null) rail.lastOrNull()?.id else folderIn(rail, folder)?.serverIds?.lastOrNull()
    return last?.let { drag.boundsOf(it) }?.let { it.bottom + gap }
}

/**
 * A rail tile: a rounded square (14), its fill [fill] (the tile's) and,
 * for a folder, its tone's inset line; the ring 2 outside it in the
 * accent for the one on screen or a drop onto it; the unread dot at its
 * top right when nothing in it is for this account, else its mentions
 * over the corner (Home's further out). [home] has the boards' lift: a
 * shadow in the light theme, the border's line in the dark.
 */
@Composable
private fun RailSquare(
    modifier: Modifier = Modifier,
    ring: Boolean = false,
    grown: Boolean = false,
    badge: Int = 0,
    dot: Boolean = false,
    home: Boolean = false,
    hidden: Boolean = false,
    fill: Color = LumiaTokens.tile.color,
    edge: Color? = null,
    content: @Composable BoxScope.() -> Unit,
) {
    val shape = RoundedCornerShape(LumiaTokens.radius14)
    val dark = LocalLumiaDark.current ?: isSystemInDarkTheme()
    val accent = LumiaTokens.accent.color
    val lift = if (home && dark) LumiaTokens.border.color else edge
    // A lifted tile leaves its place empty: the one drawn at the finger is it.
    Box(Modifier.alpha(if (hidden) 0f else 1f).graphicsLayer { if (grown) { scaleX = GROWN; scaleY = GROWN } }) {
        Box(
            modifier = Modifier
                .size(LumiaTokens.tileSize)
                .then(if (ring) Modifier.drawBehind { drawRing(accent) } else Modifier)
                .then(if (home && !dark) Modifier.shadow(LumiaTokens.space2, shape) else Modifier)
                .clip(shape)
                .background(fill)
                .then(if (lift != null) Modifier.border(1.dp, lift, shape) else Modifier)
                .then(modifier),
            contentAlignment = Alignment.Center,
            content = content,
        )
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

/** A tile a drop would go onto grows a little (the board's 1.06). */
private const val GROWN = 1.06f

/** The ring: 2 wide, 2 outside the tile, its corners following the tile's. */
private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawRing(color: Color) {
    val out = LumiaTokens.space2.toPx() * 1.5f
    drawRoundRect(
        color = color,
        topLeft = Offset(-out, -out),
        size = Size(size.width + out * 2, size.height + out * 2),
        cornerRadius = CornerRadius(LumiaTokens.radius14.toPx() + out),
        style = Stroke(width = LumiaTokens.space2.toPx()),
    )
}

/** A server's face: its initials in its tone's ink, its picture over them when it has one. */
@Composable
private fun ServerFace(session: LumiaSession, server: ServerSummary) {
    Box(Modifier.size(LumiaTokens.tileSize), contentAlignment = Alignment.Center) {
        Text(initialsOf(server.name), style = LumiaTokens.textLinkSm.style.copy(fontWeight = FontWeight.Bold), color = AvatarTone.of(server.serverId).monogram.color)
        LumiaPicture(session.pictureOf(server.iconUrl), Modifier.matchParentSize())
    }
}

/** A closed folder's face: its first four servers small, two to a row. */
@Composable
private fun FolderFaces(session: LumiaSession, servers: List<ServerSummary>) {
    val faces = folderFaces(servers.map { it.serverId }).mapNotNull { id -> servers.firstOrNull { it.serverId == id } }
    val style = TextStyle(
        fontSize = LumiaTokens.titleMonogram.size / 3,
        lineHeight = LumiaTokens.titleMonogram.size / 3,
        fontWeight = FontWeight.ExtraBold,
        platformStyle = PlatformTextStyle(includeFontPadding = false),
    )
    Column(Modifier.width(Mini * 2 + LumiaTokens.space4), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
        for (row in faces.chunked(2)) {
            Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
                for (s in row) {
                    Box(Modifier.size(Mini).clip(RoundedCornerShape(LumiaTokens.space4)).background(LumiaTokens.tile.color), contentAlignment = Alignment.Center) {
                        Text(initialsOf(s.name), style = style, color = AvatarTone.of(s.serverId).monogram.color, maxLines = 1)
                        LumiaPicture(session.pictureOf(s.iconUrl), Modifier.matchParentSize())
                    }
                }
            }
        }
    }
}

/** +: create, join or discover servers, a dashed rounded square. */
@Composable
private fun AddTile(onAdd: () -> Unit) {
    val dashed = LumiaTokens.borderDashed.color
    Box(
        Modifier.size(LumiaTokens.tileSize)
            .drawBehind {
                val width = 1.5.dp.toPx()
                val dash = LumiaTokens.space4.toPx()
                drawRoundRect(
                    color = dashed,
                    topLeft = Offset(width / 2, width / 2),
                    size = Size(size.width - width, size.height - width),
                    cornerRadius = CornerRadius(LumiaTokens.radius14.toPx()),
                    style = Stroke(width = width, pathEffect = PathEffect.dashPathEffect(floatArrayOf(dash, dash))),
                )
            }
            .clip(RoundedCornerShape(LumiaTokens.radius14))
            .clickable(role = Role.Button, onClick = onAdd)
            .semantics { contentDescription = "Add a server" },
        contentAlignment = Alignment.Center,
    ) {
        Icon(Icons.Filled.Add, contentDescription = null, tint = LumiaTokens.inkQuiet.color, modifier = Modifier.size(LumiaTokens.space18))
    }
}

/**
 * A tile's menu, as a sheet (the board's menu in a phone's idiom): its
 * name, then its items, Delete folder in the danger ink.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun RailMenu(title: String, actions: List<RailAction>, onDismiss: () -> Unit) {
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = LumiaTokens.surface.color, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.fillMaxWidth().padding(start = LumiaTokens.space16, end = LumiaTokens.space16, bottom = LumiaTokens.space24).navigationBarsPadding().semantics { paneTitle = title }) {
            Text(title, style = LumiaTokens.textSection.style, color = LumiaTokens.inkMuted.color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(horizontal = LumiaTokens.space10, vertical = LumiaTokens.space6))
            for ((i, a) in actions.withIndex()) {
                // The menu's rule between its groups (`.lu-menu__rule`), not over its first item.
                if (a.ruled && i > 0) Box(Modifier.fillMaxWidth().padding(horizontal = LumiaTokens.space6, vertical = LumiaTokens.space4).height(1.dp).background(LumiaTokens.line.color))
                // "Move to folder" opens its list in the sheet; the rest close it.
                val opens = a.label == "Move to folder"
                Row(
                    Modifier.fillMaxWidth().heightIn(min = LumiaTokens.controlHeight + LumiaTokens.space8).clip(RoundedCornerShape(LumiaTokens.radius10))
                        .clickable(role = Role.Button) { if (!opens) onDismiss(); a.run() }
                        .padding(horizontal = LumiaTokens.space10),
                    horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space14),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(a.icon, contentDescription = null, tint = LumiaTokens.inkSecondary.color, modifier = Modifier.size(LumiaTokens.space18 + LumiaTokens.space2))
                    Text(a.label, style = LumiaTokens.textNav.style, color = if (a.danger) LumiaTokens.danger.color else LumiaTokens.ink.color, modifier = Modifier.weight(1f))
                    if (opens) Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null, tint = LumiaTokens.inkFaint.color, modifier = Modifier.size(LumiaTokens.space18))
                }
            }
        }
    }
}

/**
 * Folder settings (the board's dialog): Name, which reads as its servers
 * when left empty, the nine tones as swatches, Cancel and Save.
 */
@Composable
private fun FolderSettings(folder: RailItem.Folder, words: String, onDismiss: () -> Unit, onSave: (String, AvatarTone) -> Unit) {
    var name by rememberSaveable(folder.folderId) { mutableStateOf(folder.name) }
    var tone by rememberSaveable(folder.folderId) { mutableStateOf(toneNamed(folder.tone, folder.folderId).stored) }
    val shape = RoundedCornerShape(LumiaTokens.radius18)
    Dialog(onDismissRequest = onDismiss) {
        Column(
            Modifier.widthIn(max = 320.dp).fillMaxWidth()
                .background(LumiaTokens.surface.color, shape)
                .border(1.dp, LumiaTokens.lineStrong.color, shape)
                .padding(LumiaTokens.space18 + LumiaTokens.space2)
                .semantics { paneTitle = "Folder settings" },
            verticalArrangement = Arrangement.spacedBy(LumiaTokens.space16),
        ) {
            Text("Folder settings", style = LumiaTokens.titleServer.style, color = LumiaTokens.ink.color)
            Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
                LumiaField("Name", name, { name = it }, placeholder = words)
                Text("Without a name it reads as its servers.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
            }
            Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                Text("Colour", style = LumiaTokens.textLabel.style, color = LumiaTokens.inkSecondary.color)
                Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space6), modifier = Modifier.semantics { contentDescription = "Colour" }) {
                    for ((i, t) in AvatarTone.entries.withIndex()) Swatch(t, chosen = t.stored == tone, index = i) { tone = t.stored }
                }
            }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8, Alignment.End)) {
                LumiaSecondaryButton("Cancel", onClick = onDismiss)
                LumiaPrimaryButton("Save") { onSave(name, AvatarTone.entries.first { it.stored == tone }) }
            }
        }
    }
}

/** A tone to choose: its fill in a circle (24), the chosen one ringed in the accent with its ink's check. */
@Composable
private fun Swatch(tone: AvatarTone, chosen: Boolean, index: Int, onClick: () -> Unit) {
    val accent = LumiaTokens.accent.color
    val line = LumiaTokens.lineStrong.color
    Box(
        Modifier.size(LumiaTokens.space24)
            .drawBehind {
                if (chosen) {
                    val out = LumiaTokens.space2.toPx() * 1.5f
                    drawCircle(accent, radius = size.minDimension / 2 + out, style = Stroke(LumiaTokens.space2.toPx()))
                }
            }
            .clip(CircleShape)
            .background(tone.fill.color)
            .then(if (chosen) Modifier else Modifier.border(1.dp, line, CircleShape))
            .clickable(role = Role.RadioButton, onClick = onClick)
            .semantics {
                contentDescription = tone.label
                selected = chosen
                stateDescription = if (chosen) "Selected, ${index + 1} of ${AvatarTone.entries.size}" else "${index + 1} of ${AvatarTone.entries.size}"
            },
        contentAlignment = Alignment.Center,
    ) {
        if (chosen) Icon(Icons.Filled.Check, contentDescription = null, tint = tone.ink.color, modifier = Modifier.size(LumiaTokens.space14))
    }
}
