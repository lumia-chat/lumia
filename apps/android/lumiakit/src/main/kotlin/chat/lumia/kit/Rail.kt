// The rail's order and folders (architecture 6.9), the rules the phone
// draws them by, as plain values with no Compose in them: a folder's tone,
// title, count and what it is to a screen reader; where a dragged tile
// lands; what a move says. The arrangement's own rules (moving, merging,
// tidying, a folder's words) are the core's, through `Client`'s `rail…`
// calls; these are what is left to a shell.
package chat.lumia.kit

import uniffi.lumia_ffi.RailItem

/** The tone a folder keeps by name ("mint"); a name this build does not know is the one [id] hashes to, as a server's is. */
fun toneNamed(name: String, id: String): AvatarTone =
    AvatarTone.entries.firstOrNull { it.stored == name } ?: AvatarTone.of(id)

/** The name a tone is kept by, the core's and every client's: "mint". */
val AvatarTone.stored: String get() = name.lowercase()

/** The name a tone is offered by in Folder settings: "Mint". */
val AvatarTone.label: String get() = name.lowercase().replaceFirstChar { it.uppercase() }

/** A rail item's id: a server's, or a folder's. */
val RailItem.id: String
    get() = when (this) {
        is RailItem.Server -> serverId
        is RailItem.Folder -> folderId
    }

/** The folder of this id on the rail, if there is one. */
fun folderIn(rail: List<RailItem>, id: String?): RailItem.Folder? =
    rail.filterIsInstance<RailItem.Folder>().firstOrNull { it.folderId == id }

/** How a folder reads: its name, or, with none, its servers' words (the core's `railFolderWords`, "Acme, Lumia and 2 more"). */
fun folderTitle(name: String, words: String): String = name.trim().ifEmpty { words }

/** What a closed folder shows small: its first four servers. */
fun folderFaces(serverIds: List<String>): List<String> = serverIds.take(4)

/**
 * A folder's count: its servers' mentions added up, its badge; with none,
 * whether anything in them is unread, its dot.
 */
fun folderUnread(servers: List<Unread>): Unread =
    servers.fold(Unread.None) { a, b -> Unread(a.unread + b.unread, a.mentions + b.mentions) }

/** A rail tile's dot: something unread and nothing of it for this account, on a tile not on screen. */
fun showsDot(unread: Unread, current: Boolean): Boolean = unread.unread > 0 && unread.mentions == 0 && !current

/** A folder to a screen reader, its expanded or collapsed said apart: "Folder, Work, 3 mentions". */
fun folderLabel(title: String, unread: Unread): String = withUnread("Folder, $title", unread)

/**
 * How many stand where [id] does: the rail's items, or its folder's
 * servers. Move up is offered past the first and Move down before the
 * last, as the core's `railStep` moves only there.
 */
fun siblingsOf(rail: List<RailItem>, id: String): Int =
    if (rail.any { it.id == id }) rail.size else rail.filterIsInstance<RailItem.Folder>().firstOrNull { id in it.serverIds }?.serverIds?.size ?: 0

/** Where over a tile a drag is: its top or bottom quarter is between tiles, its middle half onto it. */
enum class DropZone { BEFORE, ONTO, AFTER }

/**
 * Where over a tile [height] high a drag [y] down from its top is. A
 * folder drags whole and only between items ([between]): then the tile's
 * top half is before it and its bottom half after.
 */
fun dropZone(y: Float, height: Float, between: Boolean): DropZone = when {
    between -> if (y < height / 2) DropZone.BEFORE else DropZone.AFTER
    y < height / 4 -> DropZone.BEFORE
    y > height * 3 / 4 -> DropZone.AFTER
    else -> DropZone.ONTO
}

/**
 * A tile as the rail draws it: a server on the rail or in an open folder
 * ([inFolder]), or a folder ([folder]), closed or its open glyph.
 */
data class RailTile(val id: String, val folder: Boolean = false, val inFolder: String? = null)

/** What letting a dragged tile go does. */
sealed class RailDrop {
    /** Between tiles: onto the rail ([folder] null) or into a folder, before [before] or at the end. */
    data class Between(val folder: String?, val before: String?) : RailDrop()

    /** Onto a server: a folder of the two, or into its folder when it is in one (the core's `railMerge`). */
    data class OntoServer(val server: String) : RailDrop()

    /** Onto a folder: into it, at its end. */
    data class IntoFolder(val folder: String) : RailDrop()
}

/**
 * What dropping [dragged] in [zone] over [target] does, on [rail]: null
 * where it would leave everything as it is. A server between the servers
 * of an open folder goes into it there, and between rail items out of any
 * folder; a folder goes only between rail items, so over an open folder's
 * servers it is before or after that folder.
 */
fun railDrop(rail: List<RailItem>, dragged: String, target: RailTile, zone: DropZone): RailDrop? {
    if (target.id == dragged) return null
    val draggingFolder = rail.any { it is RailItem.Folder && it.folderId == dragged }
    val top = rail.map { it.id }
    fun after(id: String): String? = top.getOrNull(top.indexOf(id) + 1)
    fun between(folder: String?, before: String?): RailDrop? = if (before == dragged) null else RailDrop.Between(folder, before)
    val inside = target.inFolder
    val drop = when {
        inside != null && draggingFolder -> when (zone) {
            DropZone.BEFORE -> between(null, inside)
            else -> between(null, after(inside))
        }
        inside != null -> {
            val servers = (rail.firstOrNull { it.id == inside } as? RailItem.Folder)?.serverIds ?: return null
            when (zone) {
                DropZone.BEFORE -> between(inside, target.id)
                DropZone.AFTER -> between(inside, servers.getOrNull(servers.indexOf(target.id) + 1))
                DropZone.ONTO -> RailDrop.OntoServer(target.id)
            }
        }
        target.folder -> when (zone) {
            DropZone.BEFORE -> between(null, target.id)
            DropZone.AFTER -> between(null, after(target.id))
            DropZone.ONTO -> if (draggingFolder) null else RailDrop.IntoFolder(target.id)
        }
        else -> when (zone) {
            DropZone.BEFORE -> between(null, target.id)
            DropZone.AFTER -> between(null, after(target.id))
            DropZone.ONTO -> if (draggingFolder) null else RailDrop.OntoServer(target.id)
        }
    }
    // A server already where it would go: its own folder at its own place.
    if (drop is RailDrop.Between) {
        val place = placeOf(rail, dragged)
        if (place != null && place.first == drop.folder && place.second == drop.before) return null
    }
    if (drop is RailDrop.IntoFolder && rail.any { it is RailItem.Folder && it.folderId == drop.folder && dragged in it.serverIds && it.serverIds.last() == dragged }) return null
    return drop
}

/** Where a tile stands as a drop would name it: its folder (null on the rail) and what comes after it there. */
private fun placeOf(rail: List<RailItem>, id: String): Pair<String?, String?>? {
    val top = rail.map { it.id }
    val at = top.indexOf(id)
    if (at >= 0) return null to top.getOrNull(at + 1)
    for (item in rail) {
        if (item is RailItem.Folder) {
            val inner = item.serverIds.indexOf(id)
            if (inner >= 0) return item.folderId to item.serverIds.getOrNull(inner + 1)
        }
    }
    return null
}

/**
 * What a move says to a screen reader (architecture 6.9): "Acme moved into
 * Work" when it went into a folder it was not in, "New folder, Acme and
 * Lumia" when that folder is new, else "Acme moved to position 3", its
 * place on the rail or in its folder.
 */
fun movedWords(name: String, wasIn: String?, nowIn: String?, nowInTitle: String?, newFolder: Boolean, position: Int?): String = when {
    nowIn != null && newFolder && nowInTitle != null -> "New folder, $nowInTitle"
    nowIn != null && nowIn != wasIn && nowInTitle != null -> "$name moved into $nowInTitle"
    position != null -> "$name moved to position $position"
    else -> "$name moved"
}

/** The folders a device keeps open, as the `lumia` preferences keep them: one id a line. */
fun openFoldersOf(stored: String?): Set<String> = stored?.lines()?.filter { it.isNotBlank() }?.toSet() ?: emptySet()

/** The open folders as they are kept, only those still on the rail. */
fun openFoldersStored(open: Set<String>, rail: List<RailItem>): String =
    open.filter { id -> rail.any { it is RailItem.Folder && it.folderId == id } }.sorted().joinToString("\n")
