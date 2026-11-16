// What Server settings decides, as values: the name the instance would
// keep, the retention choices and when one deletes messages or
// attachments, what a full server does and what its files take, the words
// a refusal reads as, and which pictures the phone fetches.
package chat.lumia.kit

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ServerSettingsTest {
    @Test
    fun aNameIsOneToAHundredCharactersOnceTrimmed() {
        assertTrue(serverNameFits("Lumia"))
        assertTrue(serverNameFits("  Lumia  "))
        assertTrue(serverNameFits("x".repeat(100)))
        assertFalse(serverNameFits("x".repeat(101)))
        assertFalse(serverNameFits(""))
        assertFalse(serverNameFits("   "))
        // Characters as a person sees them, as the instance counts: an accent
        // written as two code points is one, and so is a flag.
        assertEquals(1, characters("é"))
        assertEquals(1, characters("🇨🇦"))
        assertTrue(serverNameFits("é".repeat(100)))
    }

    @Test
    fun saveIsOfferedForANameThatIsThereAndNew() {
        assertFalse(serverNameChanged("Lumia", "Lumia"))
        assertFalse(serverNameChanged("  Lumia ", "Lumia"))
        assertFalse(serverNameChanged("", "Lumia"))
        assertFalse(serverNameChanged("   ", "Lumia"))
        assertTrue(serverNameChanged("Lumia team", "Lumia"))
    }

    @Test
    fun theChoicesAreForeverAYearNinetyAndThirtyDays() {
        assertEquals(listOf(0, 365, 90, 30), Retention.choices(0))
        assertEquals(listOf(0, 365, 90, 30), Retention.choices(90))
        assertEquals(listOf("Forever", "1 year", "90 days", "30 days"), Retention.standard.map(Retention::label))
    }

    @Test
    fun aValueSetElsewhereIsAChoiceOfItsOwnAfterTheRest() {
        assertEquals(listOf(0, 365, 90, 30, 7), Retention.choices(7))
        assertEquals("7 days", Retention.label(7))
        assertEquals("1 day", Retention.label(1))
    }

    @Test
    fun shorterDeletesAndForeverIsTheLongest() {
        assertTrue(Retention.shortens(0, 365))
        assertTrue(Retention.shortens(0, 30))
        assertTrue(Retention.shortens(365, 90))
        assertTrue(Retention.shortens(90, 30))
        assertFalse(Retention.shortens(30, 90))
        assertFalse(Retention.shortens(30, 0))
        assertFalse(Retention.shortens(0, 0))
        assertFalse(Retention.shortens(90, 90))
    }

    @Test
    fun theWarningSaysWhatGoesAndOnlyWhenSomethingDoes() {
        assertEquals("Messages older than 1 year will be deleted for everyone. This cannot be undone.", Retention.warning(0, 365))
        assertEquals("Messages older than 30 days will be deleted for everyone. This cannot be undone.", Retention.warning(90, 30))
        assertNull(Retention.warning(30, 90))
        assertNull(Retention.warning(365, 0))
        assertNull(Retention.warning(90, 90))
    }

    @Test
    fun aRefusalReadsAsTheWebSaysIt() {
        assertEquals("You cannot change this server's settings.", serverSettingsRefused("PERMISSION_DENIED", null))
        assertEquals("A server name is 1 to 100 characters.", serverSettingsRefused("INVALID_ARGUMENT", "name"))
        assertNull(serverSettingsRefused("INVALID_ARGUMENT", "retention_days"))
        assertNull(serverSettingsRefused(null, null))
        assertEquals("This instance does not keep pictures.", pictureRefused("FAILED_PRECONDITION", null))
        assertEquals("That picture is too large. Choose one under 8 MB.", pictureRefused("INVALID_ARGUMENT", "too_large"))
        assertEquals("That file is not a picture Lumia can read. Choose a PNG, JPEG, WebP or GIF.", pictureRefused("INVALID_ARGUMENT", "unsupported"))
        assertEquals("That picture could not be read.", pictureRefused("INVALID_ARGUMENT", "invalid"))
        assertEquals("You cannot change this server's settings.", pictureRefused("PERMISSION_DENIED", null))
        assertNull(pictureRefused("NOT_FOUND", null))
    }

    @Test
    fun onlyTheInstancesOwnPicturesAreFetched() {
        val name = "0123456789abcdef".repeat(4)
        assertEquals("http://10.0.2.2:4031/api/v1/avatars/$name", pictureAddress("http://10.0.2.2:4031/", "/api/v1/avatars/$name"))
        assertEquals("https://chat.example.com/api/v1/avatars/$name", pictureAddress("https://chat.example.com", "/api/v1/avatars/$name"))
        assertNull(pictureAddress("https://chat.example.com", ""))
        assertNull(pictureAddress("https://chat.example.com", "https://elsewhere.example/api/v1/avatars/$name"))
        assertNull(pictureAddress("https://chat.example.com", "/api/v1/avatars/${name.uppercase()}"))
        assertNull(pictureAddress("https://chat.example.com", "/api/v1/avatars/${name.dropLast(1)}"))
        assertNull(pictureAddress("https://chat.example.com", "/api/v1/avatars/../../$name"))
        assertNull(pictureAddress("https://chat.example.com", "/api/v1/blobs/$name"))
    }

    @Test
    fun aServerWhoseShowDeletedChangedIsReadAgain() {
        val before = mapOf("a" to false, "b" to true, "c" to false)
        assertEquals(setOf("a", "b"), showDeletedChanged(before, mapOf("a" to true, "b" to false, "c" to false)))
        assertEquals(emptySet<String>(), showDeletedChanged(before, before))
        // A server gained or left holds nothing to read again.
        assertEquals(emptySet<String>(), showDeletedChanged(before, mapOf("d" to true)))
        assertEquals(emptySet<String>(), showDeletedChanged(emptyMap(), mapOf("a" to true)))
    }

    @Test
    fun shorterAttachmentRetentionWarnsOfTheNextDailyRun() {
        assertEquals(
            "Attachments older than 90 days will be deleted for everyone. Their messages stay. This cannot be undone.",
            Retention.attachmentsWarning(0, 90),
        )
        assertEquals(
            "Attachments older than 30 days will be deleted for everyone. Their messages stay. This cannot be undone.",
            Retention.attachmentsWarning(365, 30),
        )
        assertNull(Retention.attachmentsWarning(30, 90))
        assertNull(Retention.attachmentsWarning(90, 0))
        assertNull(Retention.attachmentsWarning(0, 0))
    }

    @Test
    fun aFullServerRefusesFirstAndSaysBeforeDeletingTheOldest() {
        assertEquals(listOf("Refuse new uploads", "Delete the oldest attachments"), WhenFull.choices.map { it.second })
        assertEquals(listOf(false, true), WhenFull.choices.map { it.first })
        assertEquals(
            "When an upload would not fit, the oldest messages’ attachments are deleted for everyone to make room.",
            WhenFull.warning(false, true),
        )
        assertNull(WhenFull.warning(true, true))
        assertNull(WhenFull.warning(true, false))
        assertNull(WhenFull.warning(false, false))
    }

    @Test
    fun savingFilesSendsOnlyWhatDiffers() {
        assertEquals(FilesChange(null, null), FilesChange.of(0, false, 0, false))
        assertFalse(FilesChange.of(0, false, 0, false).changed)
        assertEquals(FilesChange(30, null), FilesChange.of(0, false, 30, false))
        assertEquals(FilesChange(null, true), FilesChange.of(90, false, 90, true))
        assertEquals(FilesChange(0, false), FilesChange.of(90, true, 0, false))
        assertTrue(FilesChange.of(90, true, 0, false).changed)
    }

    @Test
    fun usageSaysTheLimitOnlyWhenThereIsOne() {
        val gb = 1024L * 1024 * 1024
        assertEquals("1.2 GB of 5 GB used", storageWords(gb * 12 / 10, 5 * gb))
        assertEquals("1.2 GB used", storageWords(gb * 12 / 10, 0))
        assertEquals("0 bytes of 5 GB used", storageWords(0, 5 * gb))
        assertEquals("340 KB used", storageWords(340 * 1024, 0))
    }
}
