// Whose picture a person's avatar draws: this account's own from the
// snapshot, anyone else's from the member lists, then a looked-up profile.
package chat.lumia.kit

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.lumia_ffi.MemberSummary

private fun person(id: String, avatar: String) = MemberSummary(id, id, "", "", "", "", avatar, false, false)

class PicturesTest {
    private val ours = "/api/v1/avatars/" + "0123456789abcdef".repeat(4)
    private val listed = "/api/v1/avatars/" + "1".repeat(64)
    private val lookedUp = "/api/v1/avatars/" + "2".repeat(64)

    @Test
    fun ownPictureIsTheSnapshotsThenTheListsThenALookedUpProfile() {
        val asked = mutableListOf<String>()
        val lookUp = { id: String -> asked.add(id); person(id, lookedUp) }
        // This account: the snapshot's, even where a list still has an older one, and nothing looked up.
        assertEquals(ours, avatarPathOf("me", "me", ours, person("me", listed), lookUp))
        assertEquals("", avatarPathOf("me", "me", "", person("me", listed), lookUp))
        // Someone a server lists: the list's, and no lookup.
        assertEquals(listed, avatarPathOf("ada", "me", ours, person("ada", listed), lookUp))
        assertEquals("", avatarPathOf("ada", "me", ours, person("ada", ""), lookUp))
        assertEquals(emptyList<String>(), asked)
        // Anyone else: the profile looked up, or none while it is not here.
        assertEquals(lookedUp, avatarPathOf("grace", "me", ours, null, lookUp))
        assertEquals(listOf("grace"), asked)
        assertEquals("", avatarPathOf("grace", "me", ours, null) { null })
        // No one: nothing, and nothing asked.
        assertEquals("", avatarPathOf("", "", ours, null, lookUp))
        assertEquals(listOf("grace"), asked)
    }

    @Test
    fun aPersonsPictureIsFetchedOnlyFromTheInstancesOwnPath() {
        val elsewhere = person("eve", "https://elsewhere.example$listed")
        assertNull(pictureAddress("https://chat.example.com", avatarPathOf("eve", "me", ours, elsewhere) { null }))
        assertEquals("https://chat.example.com$listed", pictureAddress("https://chat.example.com", avatarPathOf("ada", "me", ours, person("ada", listed)) { null }))
        assertNull(pictureAddress("https://chat.example.com", avatarPathOf("bob", "me", ours, null) { null }))
    }
}
