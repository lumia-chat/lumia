// What Server settings decides, as values: the name the instance would
// keep, the retention choices and when one deletes messages, the words a
// refusal reads as, and which pictures the phone fetches.
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
}
