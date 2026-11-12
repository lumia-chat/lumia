// What Preferences chooses, as values: what an appearance resolves to, how a
// stored choice reads, and the measures each density gives a message. The
// desktop shell's tests of the same are in apps/desktop/tests/preferences.rs
// (`light_and_dark_are_the_choice_made_and_system_is_whichever_the_computer_is_in`,
// `compact_gives_messages_less_room_and_the_same_words`).
package chat.lumia.kit

import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PreferencesTest {
    @Test
    fun lightAndDarkAreTheChoiceMadeAndSystemIsWhicheverThePhoneIsIn() {
        for (systemDark in listOf(false, true)) {
            assertFalse(Appearance.LIGHT.dark(systemDark))
            assertTrue(Appearance.DARK.dark(systemDark))
            assertEquals(systemDark, Appearance.SYSTEM.dark(systemDark))
        }
    }

    @Test
    fun aPhoneThatHasChosenNothingFollowsTheSystemAndIsComfortable() {
        assertEquals(Appearance.SYSTEM, Appearance.of(null))
        assertEquals(Density.COMFORTABLE, Density.of(null))
    }

    @Test
    fun aStoredChoiceReadsAsItWasMadeUnderTheNamesTheDesktopStores() {
        assertEquals(listOf("light", "dark", "system"), Appearance.entries.map { it.stored })
        assertEquals(listOf("comfortable", "compact"), Density.entries.map { it.stored })
        for (choice in Appearance.entries) assertEquals(choice, Appearance.of(choice.stored))
        for (choice in Density.entries) assertEquals(choice, Density.of(choice.stored))
    }

    @Test
    fun aStoredValueThatIsNotUnderstoodReadsAsTheDefault() {
        // Another build's choice, another case, nothing at all: none is an error.
        for (stored in listOf("", "sepia", "Dark", "DARK", " dark", "compact")) {
            assertEquals("appearance from \"$stored\"", Appearance.SYSTEM, Appearance.of(stored))
        }
        for (stored in listOf("", "cosy", "Compact", "COMPACT", "compact ", "dark")) {
            assertEquals("density from \"$stored\"", Density.COMFORTABLE, Density.of(stored))
        }
        // A debug launch that names a look takes only a name there is.
        assertNull(Appearance.named("sepia"))
        assertNull(Appearance.named(null))
        assertNull(Density.named("cosy"))
        assertNull(Density.named(null))
        assertEquals(Appearance.DARK, Appearance.named("dark"))
        assertEquals(Density.COMPACT, Density.named("compact"))
    }

    @Test
    fun comfortableIsWhatTheAppDrewBeforeThereWasAChoice() {
        val m = Measures.of(Density.COMFORTABLE)
        assertEquals(LumiaTokens.avatarLg, m.avatar)
        assertEquals(40.dp, m.avatar)
        assertEquals(8.dp, m.edge)
        assertEquals(8.dp, m.blockGap)
        assertEquals(12.dp, m.beside)
        assertEquals(4.dp, m.within)
        assertEquals(8.dp, m.divider)
        // The 8 the boards have between blocks, over the 8 at each row's edge.
        assertEquals(24.dp, m.betweenBlocks)
    }

    @Test
    fun compactIsTheSmallAvatarAndHalfTheRoom() {
        val comfortable = Measures.of(Density.COMFORTABLE)
        val compact = Measures.of(Density.COMPACT)
        assertEquals(LumiaTokens.avatarSm, compact.avatar)
        assertEquals(28.dp, compact.avatar)
        // Between blocks, between a block's messages and under its name, and around a day's divider.
        assertEquals(comfortable.betweenBlocks / 2, compact.betweenBlocks)
        assertEquals(comfortable.edge / 2, compact.edge)
        assertEquals(comfortable.blockGap / 2, compact.blockGap)
        assertEquals(comfortable.within / 2, compact.within)
        assertEquals(comfortable.divider / 2, compact.divider)
        // The avatar is nearer what is beside it, and still apart from it.
        assertTrue(compact.beside < comfortable.beside && compact.beside > 0.dp)
    }
}
