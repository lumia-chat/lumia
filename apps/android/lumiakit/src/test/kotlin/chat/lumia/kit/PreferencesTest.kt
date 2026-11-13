// What Preferences chooses, as values: what an appearance resolves to, how
// what is stored reads, the presets and which one a phone's settings are,
// and the measures each setting gives a message. The desktop shell's tests
// of the same are in apps/desktop/tests/preferences.rs.
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
    fun aPhoneThatHasChosenNothingFollowsTheSystemAndIsComfortableAt15() {
        assertEquals(Appearance.SYSTEM, Appearance.of(null))
        val d = Display.read { null }
        assertEquals(Display(Layout.GROUPED, Spacing.ROOMY, Avatars.LARGE, 15, Shown.SHOWN, Shown.SHOWN), d)
        assertEquals(Preset.COMFORTABLE, d.preset)
        assertEquals(1f, d.textScale)
    }

    @Test
    fun theSettingsAreStoredUnderTheNamesEveryClientStoresThemBy() {
        assertEquals(listOf("light", "dark", "system"), Appearance.entries.map { it.stored })
        assertEquals(listOf("grouped", "one_line"), Layout.entries.map { it.stored })
        assertEquals(listOf("roomy", "cozy", "tight"), Spacing.entries.map { it.stored })
        assertEquals(listOf("large", "small", "hidden"), Avatars.entries.map { it.stored })
        assertEquals(listOf("shown", "link"), Shown.entries.map { it.stored })
        assertEquals(listOf(13, 14, 15, 16, 18, 20), FONT_SIZES)
        assertEquals(listOf("layout", "spacing", "avatars", "font_size", "images", "embeds"), Display.KEYS)
        val minimal = Display().with(Preset.MINIMAL).copy(fontSize = 18)
        assertEquals(
            mapOf("layout" to "one_line", "spacing" to "tight", "avatars" to "hidden", "font_size" to "18", "images" to "link", "embeds" to "link"),
            minimal.written(),
        )
        // Nothing is stored for a preset, and nothing under the old density.
        assertFalse("preset" in minimal.written())
        assertFalse("density" in minimal.written())
    }

    @Test
    fun whatIsStoredReadsBackAsItWas() {
        for (layout in Layout.entries) for (spacing in Spacing.entries) for (avatars in Avatars.entries)
            for (size in FONT_SIZES) for (images in Shown.entries) for (embeds in Shown.entries) {
                val d = Display(layout, spacing, avatars, size, images, embeds)
                assertEquals(d, Display.read(d.written()::get))
            }
        for (choice in Appearance.entries) assertEquals(choice, Appearance.of(choice.stored))
    }

    @Test
    fun aStoredValueThatIsNotUnderstoodReadsAsItsDefaultAndTheRestAsStored() {
        // Another build's choice, another case, nothing at all: none is an error.
        for (stored in listOf("", "sepia", "Dark", "DARK", " dark", "compact")) {
            assertEquals("appearance from \"$stored\"", Appearance.SYSTEM, Appearance.of(stored))
        }
        val read = Display.read(
            mapOf(
                "layout" to "One_line",
                "spacing" to "tight",
                "avatars" to "medium",
                "font_size" to "17",
                "images" to "link",
                "embeds" to " link",
            )::get,
        )
        assertEquals(Display(Layout.GROUPED, Spacing.TIGHT, Avatars.LARGE, 15, Shown.LINK, Shown.SHOWN), read)
        for (size in listOf("", "12", "21", "15.0", "fifteen", "-15")) {
            assertEquals("font size from \"$size\"", 15, Display.read(mapOf("font_size" to size)::get).fontSize)
        }
        // The old density means nothing now: no migration.
        assertEquals(Display(), Display.read(mapOf("density" to "compact")::get))
        // A debug launch that names a look takes only a name there is.
        assertNull(Appearance.named("sepia"))
        assertNull(Preset.named("cosy"))
        assertNull(Preset.named(null))
        assertNull(Preset.named("Minimal"))
        assertEquals(Preset.MINIMAL, Preset.named("minimal"))
        assertNull(fontSizeNamed("17"))
        assertNull(fontSizeNamed(null))
        assertEquals(20, fontSizeNamed("20"))
    }

    @Test
    fun aPresetWritesItsValuesAndNotTheFontSize() {
        val start = Display(Layout.ONE_LINE, Spacing.TIGHT, Avatars.SMALL, 18, Shown.LINK, Shown.SHOWN)
        assertEquals(Display(Layout.GROUPED, Spacing.ROOMY, Avatars.LARGE, 18, Shown.SHOWN, Shown.SHOWN), start.with(Preset.COMFORTABLE))
        assertEquals(Display(Layout.GROUPED, Spacing.COZY, Avatars.SMALL, 18, Shown.SHOWN, Shown.SHOWN), start.with(Preset.COMPACT))
        assertEquals(Display(Layout.ONE_LINE, Spacing.TIGHT, Avatars.HIDDEN, 18, Shown.LINK, Shown.LINK), start.with(Preset.MINIMAL))
        for (preset in Preset.entries) {
            for (size in FONT_SIZES) assertEquals(preset, Display(fontSize = size).with(preset).preset)
        }
    }

    @Test
    fun aSettingChangedAfterAPresetMakesItCustom() {
        val compact = Display().with(Preset.COMPACT)
        assertEquals(Preset.COMPACT, compact.preset)
        assertNull(compact.copy(spacing = Spacing.TIGHT).preset)
        assertNull(compact.copy(avatars = Avatars.HIDDEN).preset)
        assertNull(compact.copy(layout = Layout.ONE_LINE).preset)
        // What has no control yet still counts: a preset is all of its values.
        assertNull(Display().copy(images = Shown.LINK).preset)
        assertNull(Display().with(Preset.MINIMAL).copy(embeds = Shown.SHOWN).preset)
        // The font size is never a preset's, so changing it leaves the preset as it was.
        assertEquals(Preset.COMPACT, compact.copy(fontSize = 20).preset)
        // Put back, it is the preset again.
        assertEquals(Preset.COMPACT, compact.copy(spacing = Spacing.TIGHT).copy(spacing = Spacing.COZY).preset)
    }

    @Test
    fun oneLineDrawsNoAvatarsSoTheyAreNotComparedThere() {
        val minimal = Display().with(Preset.MINIMAL)
        for (avatars in Avatars.entries) assertEquals(Preset.MINIMAL, minimal.copy(avatars = avatars).preset)
        // In grouped they are: Comfortable with the small avatar is no preset.
        assertNull(Display().copy(avatars = Avatars.SMALL).preset)
        for (avatars in Avatars.entries) assertNull(minimal.copy(avatars = avatars).measures.avatar)
    }

    @Test
    fun roomyIsWhatComfortableDrewBefore() {
        val m = Display().measures
        assertEquals(m, Measures.Comfortable)
        assertEquals(LumiaTokens.avatarLg, m.avatar)
        assertEquals(40.dp, m.avatar)
        assertEquals(8.dp, m.edge)
        assertEquals(8.dp, m.blockGap)
        assertEquals(14.dp, m.beside)
        assertEquals(4.dp, m.within)
        assertEquals(8.dp, m.divider)
        assertFalse(m.oneLine)
        // The 8 the boards have between blocks, over the 8 at each row's edge.
        assertEquals(24.dp, m.betweenBlocks)
        assertEquals(54.dp, m.gutter)
    }

    @Test
    fun cozyIsHalfTheRoomAndTightIsCozyWithTheLeastAtTheEdges() {
        val roomy = Measures.of(Layout.GROUPED, Spacing.ROOMY, Avatars.LARGE)
        val cozy = Measures.of(Layout.GROUPED, Spacing.COZY, Avatars.LARGE)
        val tight = Measures.of(Layout.GROUPED, Spacing.TIGHT, Avatars.LARGE)
        // Between blocks, between a block's messages and under its name, and around a day's divider.
        assertEquals(roomy.betweenBlocks / 2, cozy.betweenBlocks)
        assertEquals(roomy.edge / 2, cozy.edge)
        assertEquals(roomy.blockGap / 2, cozy.blockGap)
        assertEquals(roomy.within / 2, cozy.within)
        assertEquals(roomy.divider / 2, cozy.divider)
        assertEquals(10.dp, cozy.beside)
        assertEquals(cozy.copy(edge = 2.dp, blockGap = 2.dp, line = 0.dp), tight)
        // The room between one line and the next in one line: 4, 2 or none.
        assertEquals(listOf(4.dp, 2.dp, 0.dp), Spacing.entries.map { Measures.of(Layout.ONE_LINE, it, Avatars.LARGE).line })
    }

    @Test
    fun theAvatarsAreTheirOwnSettingAndHiddenKeepsNoRoomForOne() {
        for (spacing in Spacing.entries) {
            assertEquals(LumiaTokens.avatarLg, Measures.of(Layout.GROUPED, spacing, Avatars.LARGE).avatar)
            assertEquals(LumiaTokens.avatarSm, Measures.of(Layout.GROUPED, spacing, Avatars.SMALL).avatar)
            val hidden = Measures.of(Layout.GROUPED, spacing, Avatars.HIDDEN)
            assertNull(hidden.avatar)
            assertEquals(0.dp, hidden.gutter)
            // The room is the spacing's whatever the avatar.
            assertEquals(Measures.of(Layout.GROUPED, spacing, Avatars.LARGE).copy(avatar = null), hidden)
        }
        assertEquals(28.dp + 10.dp, Display().with(Preset.COMPACT).measures.gutter)
    }

    @Test
    fun theFontSizeScalesTheTextByItsShareOf15AndNothingElse() {
        assertEquals(listOf(13f / 15, 14f / 15, 1f, 16f / 15, 18f / 15, 20f / 15), FONT_SIZES.map { Display(fontSize = it).textScale })
        for (size in FONT_SIZES) assertEquals(Display().measures, Display(fontSize = size).measures)
        // In sp still, so the phone's own text scale applies on top: multiplied, not replaced.
        val message = LumiaTokens.textMessage.style.scaled(Display(fontSize = 20).textScale)
        assertEquals(20f, message.fontSize.value, 0.001f)
        assertTrue(message.fontSize.isSp)
        assertEquals(12.5f * 20 / 15, LumiaTokens.textMeta.size.scaled(20f / 15).value, 0.001f)
        assertEquals(LumiaTokens.textMeta.style, LumiaTokens.textMeta.style.scaled(1f))
    }
}
