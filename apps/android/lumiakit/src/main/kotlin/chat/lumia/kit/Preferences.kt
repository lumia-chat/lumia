// What the Preferences screen chooses, as plain values `src/test` checks on
// the JVM: light, dark or as the phone is, and how a conversation's
// messages are drawn (`Display`): their layout, the room around them, their
// avatars, the size of their text, and whether pictures and link previews
// are drawn or left as links. Comfortable, Compact and Minimal are presets
// that write those values and are not kept themselves. Each setting is
// stored under the name every client stores it by. They are this phone's
// and not the account's: `AppStore` keeps them beside the saved instance,
// and nothing of them is sent to it.
package chat.lumia.kit

import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** A choice kept under a name: the one every client stores it by. */
interface Stored {
    val stored: String
}

/** The choice of [E] stored under this name, if it is one: another case, another build's name or nothing is none. */
inline fun <reified E> storedAs(name: String?): E? where E : Enum<E>, E : Stored =
    enumValues<E>().firstOrNull { it.stored == name }

/** Light, dark, or whichever the phone is in. */
enum class Appearance(override val stored: String) : Stored {
    LIGHT("light"),
    DARK("dark"),
    SYSTEM("system");

    /** Whether the app is dark, given whether the phone is. */
    fun dark(systemDark: Boolean): Boolean = when (this) {
        LIGHT -> false
        DARK -> true
        SYSTEM -> systemDark
    }

    companion object {
        /** A phone that has chosen nothing follows the system, as it changes. */
        val Default = SYSTEM

        /** The appearance stored under this name, if it is one. */
        fun named(stored: String?): Appearance? = storedAs<Appearance>(stored)

        /** What a stored choice reads as: nothing stored, or a name this build does not know, is the default. */
        fun of(stored: String?): Appearance = named(stored) ?: Default
    }
}

/** How a conversation's messages are laid out. */
enum class Layout(override val stored: String) : Stored {
    /** What one person sends close together as one block, under their avatar, name and time: the boards' rows. */
    GROUPED("grouped"),

    /** Every message a line of its own, the time, the name and the words, wrapping under the words' start. */
    ONE_LINE("one_line"),
}

/** How much room there is around and between messages. */
enum class Spacing(override val stored: String) : Stored {
    /** What Comfortable drew before there were settings. */
    ROOMY("roomy"),

    /** What Compact drew. */
    COZY("cozy"),

    /** Cozy with the least room at a block's edges and between blocks. */
    TIGHT("tight"),
}

/** How large a message's avatar is, or none: whatever the room around it. */
enum class Avatars(override val stored: String) : Stored {
    LARGE("large"),
    SMALL("small"),
    HIDDEN("hidden"),
}

/**
 * Whether a message's pictures, or its links' previews, are drawn in it or
 * left as a link. In the settings and the presets only: nothing draws
 * either yet, so Preferences offers no control for them.
 */
enum class Shown(override val stored: String) : Stored {
    SHOWN("shown"),
    LINK("link"),
}

/** The message text's sizes Preferences offers, smallest first. */
val FONT_SIZES = listOf(13, 14, 15, 16, 18, 20)

/** The design's message text, which every text in a message row is drawn at the size of over this. */
const val DEFAULT_FONT_SIZE = 15

/** The font size stored as this, if it is one Preferences offers. */
fun fontSizeNamed(stored: String?): Int? = stored?.toIntOrNull()?.takeIf { it in FONT_SIZES }

/**
 * How this phone draws a conversation's messages: six settings, each read
 * on its own and each its default when what is stored under it is missing
 * or not understood. The defaults are Comfortable at the design's 15.
 */
data class Display(
    val layout: Layout = Layout.GROUPED,
    val spacing: Spacing = Spacing.ROOMY,
    val avatars: Avatars = Avatars.LARGE,
    val fontSize: Int = DEFAULT_FONT_SIZE,
    val images: Shown = Shown.SHOWN,
    val embeds: Shown = Shown.SHOWN,
) {
    /** The preset whose values these all are, or null, which the page shows as "Custom". */
    val preset: Preset? get() = Preset.entries.firstOrNull { it.matches(this) }

    /** These with [preset]'s values written over them. The font size is never a preset's. */
    fun with(preset: Preset): Display = copy(
        layout = preset.layout,
        spacing = preset.spacing,
        avatars = preset.avatars,
        images = preset.images,
        embeds = preset.embeds,
    )

    /** How much larger than the design every text in a message row, and the composer's, is drawn. */
    val textScale: Float get() = fontSize / DEFAULT_FONT_SIZE.toFloat()

    /** A message row's avatar and room. */
    val measures: Measures get() = Measures.of(layout, spacing, avatars)

    /** What is kept on the phone, by key: each setting under its own name, the font size as its number. */
    fun written(): Map<String, String> = mapOf(
        LAYOUT to layout.stored,
        SPACING to spacing.stored,
        AVATARS to avatars.stored,
        FONT_SIZE to fontSize.toString(),
        IMAGES to images.stored,
        EMBEDS to embeds.stored,
    )

    companion object {
        const val LAYOUT = "layout"
        const val SPACING = "spacing"
        const val AVATARS = "avatars"
        const val FONT_SIZE = "font_size"
        const val IMAGES = "images"
        const val EMBEDS = "embeds"

        /** The keys the settings are kept under. */
        val KEYS = listOf(LAYOUT, SPACING, AVATARS, FONT_SIZE, IMAGES, EMBEDS)

        /**
         * What is kept reads as ([stored] gives what is under a key, or
         * null): each setting from its own key, and one missing or not
         * understood its default, whatever the others are.
         */
        fun read(stored: (String) -> String?): Display {
            val d = Display()
            return Display(
                layout = storedAs<Layout>(stored(LAYOUT)) ?: d.layout,
                spacing = storedAs<Spacing>(stored(SPACING)) ?: d.spacing,
                avatars = storedAs<Avatars>(stored(AVATARS)) ?: d.avatars,
                fontSize = fontSizeNamed(stored(FONT_SIZE)) ?: d.fontSize,
                images = storedAs<Shown>(stored(IMAGES)) ?: d.images,
                embeds = storedAs<Shown>(stored(EMBEDS)) ?: d.embeds,
            )
        }
    }
}

/**
 * Values Preferences writes at once, all but the font size. A preset is not
 * kept: the page shows the one the settings match, so a setting changed
 * afterwards makes them "Custom".
 */
enum class Preset(
    override val stored: String,
    val label: String,
    val layout: Layout,
    val spacing: Spacing,
    val avatars: Avatars,
    val images: Shown,
    val embeds: Shown,
) : Stored {
    COMFORTABLE("comfortable", "Comfortable", Layout.GROUPED, Spacing.ROOMY, Avatars.LARGE, Shown.SHOWN, Shown.SHOWN),
    COMPACT("compact", "Compact", Layout.GROUPED, Spacing.COZY, Avatars.SMALL, Shown.SHOWN, Shown.SHOWN),
    MINIMAL("minimal", "Minimal", Layout.ONE_LINE, Spacing.TIGHT, Avatars.HIDDEN, Shown.LINK, Shown.LINK);

    /** Whether [display] is this preset: every value it writes, but the avatars in one line, which draws none. */
    fun matches(display: Display): Boolean =
        display.layout == layout &&
            display.spacing == spacing &&
            (layout == Layout.ONE_LINE || display.avatars == avatars) &&
            display.images == images &&
            display.embeds == embeds

    companion object {
        /** The preset named this, for a debug launch. */
        fun named(stored: String?): Preset? = storedAs<Preset>(stored)
    }
}

/**
 * A message row's measures, which the layout, the spacing and the avatars
 * decide; the font size changes the text and none of these. Roomy is what
 * the phone boards draw, with what they have no token for made from the
 * nearest ones; cozy is the same row with half the room between blocks,
 * between a block's messages and around a day's divider, and tight is cozy
 * with the least at a block's edges and between blocks. Nothing but the
 * list of messages changes: not the sidebar, the composer or the typing line.
 */
data class Measures(
    /** The avatar's size; null for none, hidden or in one line, and then no room is kept for it. */
    val avatar: Dp?,
    /** The room over a block's first message and under its last, inside the row. */
    val edge: Dp,
    /** The room between one block and the next, beside their edges. The boards drew 2; Ryan asked for more (2026-11-12). */
    val blockGap: Dp,
    /** Between the avatar and what is beside it. */
    val beside: Dp,
    /** Between two messages of a block, and under its name: a paragraph's distance, as the boards draw a message of several. */
    val within: Dp,
    /** Over and under a day's divider. */
    val divider: Dp,
    /** In one line, the room between one message's line and the next. */
    val line: Dp,
    /** Whether every message is a line of its own (`Layout.ONE_LINE`). */
    val oneLine: Boolean = false,
) {
    /** From the foot of one block's last message to the head of the next block. */
    val betweenBlocks: Dp get() = edge + blockGap + edge

    /** Where a grouped row's words start from its edge: past the avatar and the room beside it, or at the edge with no avatar. */
    val gutter: Dp get() = avatar?.let { it + beside } ?: 0.dp

    companion object {
        fun of(layout: Layout, spacing: Spacing, avatars: Avatars): Measures {
            val avatar = when {
                layout == Layout.ONE_LINE -> null
                avatars == Avatars.LARGE -> LumiaTokens.avatarLg
                avatars == Avatars.SMALL -> LumiaTokens.avatarSm
                else -> null
            }
            val room = when (spacing) {
                Spacing.ROOMY -> Measures(avatar, LumiaTokens.space8, LumiaTokens.space8, LumiaTokens.space14, LumiaTokens.space4, LumiaTokens.space8, LumiaTokens.space4)
                Spacing.COZY -> Measures(avatar, LumiaTokens.space4, LumiaTokens.space4, LumiaTokens.space10, LumiaTokens.space2, LumiaTokens.space4, LumiaTokens.space2)
                Spacing.TIGHT -> Measures(avatar, LumiaTokens.space2, LumiaTokens.space2, LumiaTokens.space10, LumiaTokens.space2, LumiaTokens.space4, 0.dp)
            }
            return room.copy(oneLine = layout == Layout.ONE_LINE)
        }

        /** What a conversation drew before there was a choice, and what a row is drawn with where nothing says otherwise. */
        val Comfortable = of(Layout.GROUPED, Spacing.ROOMY, Avatars.LARGE)
    }
}
