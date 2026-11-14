// The rail's rules as values (architecture 6.9): a folder's tone, title,
// count and label, where a dragged tile lands, what a move says, and the
// open folders a device keeps.
package chat.lumia.kit

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.lumia_ffi.RailItem

private fun server(id: String) = RailItem.Server(id)
private fun folder(id: String, vararg servers: String, name: String = "", tone: String = "mint") = RailItem.Folder(id, name, tone, servers.toList())

class RailTest {
    // Acme, Work (Northwind, Guild, Design), Trail.
    private val rail = listOf(server("acme"), folder("work", "nw", "guild", "design"), server("trail"))

    @Test
    fun aRefusalIsSaidInTheWebsWords() {
        val folders = uniffi.lumia_ffi.LumiaException.Api("INVALID_ARGUMENT", "invalid argument", mapOf("field" to "folders"))
        assertEquals("The rail was not saved: There can be at most 100 folders.", railRefusal(folders))
        val name = uniffi.lumia_ffi.LumiaException.Api("INVALID_ARGUMENT", "invalid argument", mapOf("field" to "name"))
        assertEquals("The rail was not saved: A folder's name is up to 50 characters.", railRefusal(name))
        assertEquals("The rail was not saved: offline", railRefusal(uniffi.lumia_ffi.LumiaException.Other("offline")))
    }

    @Test
    fun aToneIsKeptByItsNameAndAnUnknownOneIsTheIdsHash() {
        assertEquals(AvatarTone.MINT, toneNamed("mint", "x"))
        assertEquals(AvatarTone.INDIGO, toneNamed("indigo", "x"))
        assertEquals(AvatarTone.of("work"), toneNamed("teal", "work"))
        assertEquals("peach", AvatarTone.PEACH.stored)
        assertEquals("Sky", AvatarTone.SKY.label)
        // The nine in the core's order, which Folder settings offers them in.
        assertEquals(listOf("peach", "blue", "purple", "mint", "amber", "sky", "stone", "green", "indigo"), AvatarTone.entries.map { it.stored })
    }

    @Test
    fun aFolderReadsAsItsNameOrItsServers() {
        assertEquals("Work", folderTitle("Work", "Northwind, Guild and Design"))
        assertEquals("Work", folderTitle("  Work ", "Northwind"))
        assertEquals("Northwind, Guild and Design", folderTitle("", "Northwind, Guild and Design"))
        assertEquals("Northwind", folderTitle("   ", "Northwind"))
        assertEquals(listOf("a", "b", "c", "d"), folderFaces(listOf("a", "b", "c", "d", "e")))
        assertEquals(listOf("a"), folderFaces(listOf("a")))
    }

    @Test
    fun aClosedFolderAddsUpItsServersMentionsElseShowsTheDot() {
        val sum = folderUnread(listOf(Unread(4, 2), Unread(1, 0), Unread(3, 1)))
        assertEquals(Unread(8, 3), sum)
        assertFalse(showsDot(sum, current = false))
        val quiet = folderUnread(listOf(Unread(2, 0), Unread.None))
        assertTrue(showsDot(quiet, current = false))
        assertFalse(showsDot(quiet, current = true))
        assertFalse(showsDot(folderUnread(emptyList()), current = false))
    }

    @Test
    fun aFolderSaysItIsOneAndWhatIsInIt() {
        assertEquals("Folder, Work", folderLabel("Work", Unread.None))
        assertEquals("Folder, Work, 3 mentions", folderLabel("Work", Unread(5, 3)))
        assertEquals("Folder, Acme and Lumia, 1 mention", folderLabel("Acme and Lumia", Unread(1, 1)))
        assertEquals("Folder, Work, unread", folderLabel("Work", Unread(2, 0)))
    }

    @Test
    fun aTilesQuartersAreBetweenAndItsMiddleOnto() {
        assertEquals(DropZone.BEFORE, dropZone(10f, 44f, between = false))
        assertEquals(DropZone.ONTO, dropZone(12f, 44f, between = false))
        assertEquals(DropZone.ONTO, dropZone(32f, 44f, between = false))
        assertEquals(DropZone.AFTER, dropZone(34f, 44f, between = false))
        // A folder only goes between: halves.
        assertEquals(DropZone.BEFORE, dropZone(21f, 44f, between = true))
        assertEquals(DropZone.AFTER, dropZone(23f, 44f, between = true))
    }

    @Test
    fun aServerDroppedBetweenRailItemsGoesThere() {
        assertEquals(RailDrop.Between(null, "acme"), railDrop(rail, "trail", RailTile("acme"), DropZone.BEFORE))
        assertEquals(RailDrop.Between(null, "work"), railDrop(rail, "trail", RailTile("work", folder = true), DropZone.BEFORE))
        // After the last: the rail's end.
        assertEquals(RailDrop.Between(null, null), railDrop(rail, "acme", RailTile("trail"), DropZone.AFTER))
        // Out of a folder, between rail items.
        assertEquals(RailDrop.Between(null, "trail"), railDrop(rail, "guild", RailTile("work", folder = true), DropZone.AFTER))
    }

    @Test
    fun aServerDroppedOntoAServerOrAFolderGathersThem() {
        assertEquals(RailDrop.OntoServer("acme"), railDrop(rail, "trail", RailTile("acme"), DropZone.ONTO))
        assertEquals(RailDrop.IntoFolder("work"), railDrop(rail, "trail", RailTile("work", folder = true), DropZone.ONTO))
        // Onto a server in an open folder: the core puts it into that folder.
        assertEquals(RailDrop.OntoServer("guild"), railDrop(rail, "acme", RailTile("guild", inFolder = "work"), DropZone.ONTO))
    }

    @Test
    fun aServerBetweenAnOpenFoldersServersGoesIntoItThere() {
        assertEquals(RailDrop.Between("work", "guild"), railDrop(rail, "acme", RailTile("guild", inFolder = "work"), DropZone.BEFORE))
        assertEquals(RailDrop.Between("work", "design"), railDrop(rail, "acme", RailTile("guild", inFolder = "work"), DropZone.AFTER))
        assertEquals(RailDrop.Between("work", null), railDrop(rail, "acme", RailTile("design", inFolder = "work"), DropZone.AFTER))
        // Moved within its folder.
        assertEquals(RailDrop.Between("work", "nw"), railDrop(rail, "design", RailTile("nw", inFolder = "work"), DropZone.BEFORE))
    }

    @Test
    fun aFolderOnlyGoesBetweenRailItems() {
        assertEquals(RailDrop.Between(null, null), railDrop(rail, "work", RailTile("trail"), DropZone.AFTER))
        assertEquals(RailDrop.Between(null, "acme"), railDrop(rail, "work", RailTile("acme"), DropZone.BEFORE))
        assertNull(railDrop(rail, "work", RailTile("acme"), DropZone.ONTO))
        val two = listOf(folder("a", "x"), folder("b", "y"))
        assertNull(railDrop(two, "a", RailTile("b", folder = true), DropZone.ONTO))
        // Over another open folder's servers: before or after that folder.
        val open = listOf(server("acme"), folder("work", "nw", "guild"), folder("play", "game"))
        assertEquals(RailDrop.Between(null, "work"), railDrop(open, "play", RailTile("guild", inFolder = "work"), DropZone.BEFORE))
        assertEquals(RailDrop.Between(null, null), railDrop(open, "work", RailTile("game", inFolder = "play"), DropZone.AFTER))
    }

    @Test
    fun aDropThatChangesNothingIsNone() {
        assertNull(railDrop(rail, "acme", RailTile("acme"), DropZone.ONTO))
        // Just before the tile after it, or just after the tile before it: where it is.
        assertNull(railDrop(rail, "acme", RailTile("work", folder = true), DropZone.BEFORE))
        assertNull(railDrop(rail, "trail", RailTile("work", folder = true), DropZone.AFTER))
        assertNull(railDrop(rail, "guild", RailTile("nw", inFolder = "work"), DropZone.AFTER))
        assertNull(railDrop(rail, "guild", RailTile("design", inFolder = "work"), DropZone.BEFORE))
        // Into the folder it is last in.
        assertNull(railDrop(rail, "design", RailTile("work", folder = true), DropZone.ONTO))
        // A folder over its own servers.
        assertNull(railDrop(rail, "work", RailTile("nw", inFolder = "work"), DropZone.BEFORE))
        assertNull(railDrop(rail, "work", RailTile("design", inFolder = "work"), DropZone.AFTER))
    }

    @Test
    fun aTileStandsAmongTheRailsItemsOrItsFoldersServers() {
        assertEquals(3, siblingsOf(rail, "acme"))
        assertEquals(3, siblingsOf(rail, "work"))
        assertEquals(3, siblingsOf(rail, "guild"))
        assertEquals(1, siblingsOf(listOf(folder("f", "only")), "only"))
        assertEquals(0, siblingsOf(rail, "gone"))
    }

    @Test
    fun aMoveSaysWhereItWent() {
        assertEquals("Acme moved to position 3", movedWords("Acme", null, null, null, newFolder = false, position = 3))
        assertEquals("Acme moved into Work", movedWords("Acme", null, "work", "Work", newFolder = false, position = 4))
        assertEquals("Acme moved to position 1", movedWords("Acme", "work", "work", "Work", newFolder = false, position = 1))
        assertEquals("New folder, Acme and Trail Runners", movedWords("Trail Runners", null, "f", "Acme and Trail Runners", newFolder = true, position = 2))
        assertEquals("Work moved to position 2", movedWords("Work", null, null, null, newFolder = false, position = 2))
    }

    @Test
    fun theOpenFoldersAreKeptOneALineAndOnlyThoseStillThere() {
        assertEquals(emptySet<String>(), openFoldersOf(null))
        assertEquals(setOf("a", "b"), openFoldersOf("a\nb\n"))
        assertEquals("work", openFoldersStored(setOf("work", "gone"), rail))
        assertEquals(rail[1], folderIn(rail, "work"))
        assertNull(folderIn(rail, "acme"))
    }
}
