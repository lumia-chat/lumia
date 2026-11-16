// What the Preferences screen chooses, as plain values `src/test` checks on
// the JVM: light, dark or as the phone is, and how a conversation's
// messages are drawn (`Display`): their layout, the room around them, their
// avatars, the size of their text, whether pictures and link previews are
// drawn or left as links, and whether their reactions are drawn. Comfortable, Compact and Minimal are presets
// that write those values and are not kept themselves. Each setting is
// stored under the name every client stores it by. They are this phone's
// and not the account's: `AppStore` keeps them beside the saved instance,
// and nothing of them is sent to it. And the page itself, as values: the
// rows it offers (`DisplaySetting`), its switches and what the line under
// each says (`DisplaySwitch`), and the sample messages its preview draws
// (`PreviewMessage`).
package chat.lumia.kit

import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import java.time.ZoneOffset
import uniffi.lumia_ffi.ReactionSummary

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
 * left as a link: Images at Link draws every attachment as a file's row
 * that opens it, Embeds at Link draws no cards (specs/media.md 5,
 * `mediaShape`, `cardsDrawn`).
 */
enum class Shown(override val stored: String) : Stored {
    SHOWN("shown"),
    LINK("link"),
}

/**
 * Whether a message's reactions are drawn under it. Hidden takes the chips
 * away and nothing else: Add reaction stays in a message's actions, and a
 * reaction added while hidden is there, only not drawn.
 */
enum class Reactions(override val stored: String) : Stored {
    SHOWN("shown"),
    HIDDEN("hidden"),
}

/** The message text's sizes Preferences offers, smallest first. */
val FONT_SIZES = listOf(13, 14, 15, 16, 18, 20)

/** The design's message text, which every text in a message row is drawn at the size of over this. */
const val DEFAULT_FONT_SIZE = 15

/** The font size stored as this, if it is one Preferences offers. */
fun fontSizeNamed(stored: String?): Int? = stored?.toIntOrNull()?.takeIf { it in FONT_SIZES }

/**
 * How this phone draws a conversation's messages: seven settings, each read
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
    val reactions: Reactions = Reactions.SHOWN,
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
        reactions = preset.reactions,
    )

    /** The reactions a message's row draws: all of them shown, none hidden, so no row and no room for one. */
    fun <R> drawn(reactions: List<R>): List<R> = if (this.reactions == Reactions.SHOWN) reactions else emptyList()

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
        REACTIONS to reactions.stored,
    )

    companion object {
        const val LAYOUT = "layout"
        const val SPACING = "spacing"
        const val AVATARS = "avatars"
        const val FONT_SIZE = "font_size"
        const val IMAGES = "images"
        const val EMBEDS = "embeds"
        const val REACTIONS = "reactions"

        /** The keys the settings are kept under. */
        val KEYS = listOf(LAYOUT, SPACING, AVATARS, FONT_SIZE, IMAGES, EMBEDS, REACTIONS)

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
                reactions = storedAs<Reactions>(stored(REACTIONS)) ?: d.reactions,
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
    val reactions: Reactions,
) : Stored {
    COMFORTABLE("comfortable", "Comfortable", Layout.GROUPED, Spacing.ROOMY, Avatars.LARGE, Shown.SHOWN, Shown.SHOWN, Reactions.SHOWN),
    COMPACT("compact", "Compact", Layout.GROUPED, Spacing.COZY, Avatars.SMALL, Shown.SHOWN, Shown.SHOWN, Reactions.SHOWN),
    MINIMAL("minimal", "Minimal", Layout.ONE_LINE, Spacing.TIGHT, Avatars.HIDDEN, Shown.LINK, Shown.LINK, Reactions.HIDDEN);

    /** What its card says under its name. */
    val caption: String
        get() = when (this) {
            COMFORTABLE -> "Large avatars, room to breathe"
            COMPACT -> "Small avatars, more on screen"
            MINIMAL -> "A line each, media as links"
        }

    /** Whether [display] is this preset: every value it writes, but the avatars in one line, which draws none. */
    fun matches(display: Display): Boolean =
        display.layout == layout &&
            display.spacing == spacing &&
            (layout == Layout.ONE_LINE || display.avatars == avatars) &&
            display.images == images &&
            display.embeds == embeds &&
            display.reactions == reactions

    companion object {
        /** The preset named this, for a debug launch. */
        fun named(stored: String?): Preset? = storedAs<Preset>(stored)
    }
}

/**
 * The settings Preferences offers a row each under the presets: the
 * setting's name over a line that says what it changes, and its choices
 * as segments beside or under them, in the words every client uses.
 */
enum class DisplaySetting(val label: String, val caption: String) {
    LAYOUT("Layout", "One person’s messages as a block, or a line each."),
    SPACING("Spacing", "Room between messages."),
    AVATARS("Avatars", "Beside each block of messages."),
    FONT_SIZE("Font size", "Messages and names, in points.");

    /** Its choices in order: what each is stored as, and its words. */
    val options: List<Pair<String, String>>
        get() = when (this) {
            LAYOUT -> listOf(Layout.GROUPED.stored to "Grouped", Layout.ONE_LINE.stored to "One line")
            SPACING -> listOf(Spacing.ROOMY.stored to "Roomy", Spacing.COZY.stored to "Cozy", Spacing.TIGHT.stored to "Tight")
            AVATARS -> listOf(Avatars.LARGE.stored to "Large", Avatars.SMALL.stored to "Small", Avatars.HIDDEN.stored to "Hidden")
            FONT_SIZE -> FONT_SIZES.map { "$it" to "$it" }
        }

    /** The choice [display] has, as it is stored. */
    fun chosen(display: Display): String = when (this) {
        LAYOUT -> display.layout.stored
        SPACING -> display.spacing.stored
        AVATARS -> display.avatars.stored
        FONT_SIZE -> "${display.fontSize}"
    }

    /** [display] with the choice stored as [name] made; null for a name that is none of its choices. */
    fun choose(display: Display, name: String): Display? = when (this) {
        LAYOUT -> storedAs<Layout>(name)?.let { display.copy(layout = it) }
        SPACING -> storedAs<Spacing>(name)?.let { display.copy(spacing = it) }
        AVATARS -> storedAs<Avatars>(name)?.let { display.copy(avatars = it) }
        FONT_SIZE -> fontSizeNamed(name)?.let { display.copy(fontSize = it) }
    }

    companion object {
        /** The rows offered for [display]: every one but the avatars in one line, which draws none. */
        fun offered(display: Display): List<DisplaySetting> =
            entries.filter { it != AVATARS || display.layout != Layout.ONE_LINE }
    }
}

/**
 * Images, Embeds and Reactions: a switch each in Preferences, on for shown
 * and off for as links (Images, Embeds) or hidden (Reactions), the values
 * stored as ever. The line under each name says what the state it is in
 * does.
 */
enum class DisplaySwitch(val label: String, private val onWords: String, private val offWords: String) {
    IMAGES("Images", "Shown in their messages.", "As links: each one a file row that opens the viewer."),
    EMBEDS("Embeds", "A card under each link.", "As links: no cards."),

    // A phone has no toolbar over a message: Add reaction is in the menu a long press opens.
    REACTIONS("Reactions", "Shown under each message.", "Hidden. You can still add one from a message’s menu.");

    /** Whether it is on in [display]. */
    fun on(display: Display): Boolean = when (this) {
        IMAGES -> display.images == Shown.SHOWN
        EMBEDS -> display.embeds == Shown.SHOWN
        REACTIONS -> display.reactions == Reactions.SHOWN
    }

    /** [display] with it turned on or off. */
    fun turned(display: Display, on: Boolean): Display = when (this) {
        IMAGES -> display.copy(images = if (on) Shown.SHOWN else Shown.LINK)
        EMBEDS -> display.copy(embeds = if (on) Shown.SHOWN else Shown.LINK)
        REACTIONS -> display.copy(reactions = if (on) Reactions.SHOWN else Reactions.HIDDEN)
    }

    /** The line under its name in [display]. */
    fun caption(display: Display): String = if (on(display)) onWords else offWords
}

/**
 * One of the four messages Preferences' preview draws in #design-review,
 * the same on every client: who sent it and when, its words (Markdown, as
 * a message's are), and what it carries: a picture of an app's sign-in
 * screen ([picture]), a link's card ([card]) and reactions, each drawn or
 * not as the settings say, by the rules a conversation draws by.
 */
data class PreviewMessage(
    val author: String,
    val tone: AvatarTone,
    val time: String,
    /** Its minute of the morning, for which messages are one block. */
    val minute: Int,
    val text: String,
    val picture: Boolean = false,
    val card: Boolean = false,
    val reactions: List<ReactionSummary> = emptyList(),
) {
    companion object {
        /** The preview's reader, whose reaction the 🎉 is. */
        const val ME = "me"

        /** The preview to a screen reader: one picture, named this. */
        const val LABEL = "Preview of messages with these settings"

        /** The channel its messages are in. */
        const val CHANNEL = "design-review"

        /** The picture's file, as its row names it with Images as links. */
        const val PICTURE_NAME = "onboarding-first-screen.png"
        const val PICTURE_TYPE = "image/png"
        const val PICTURE_BYTES = 412_000L

        /** The picture's own dimensions, fitted to the column as any message's are. */
        const val PICTURE_WIDTH = 240
        const val PICTURE_HEIGHT = 150

        /** The card under Mei's link. */
        const val CARD_HOST = "lumia.chat"
        const val CARD_TITLE = "What’s new in Lumia 0.13"
        const val CARD_DESCRIPTION = "Video and audio play in the conversation, and links open as cards on every client."

        val SAMPLES = listOf(
            PreviewMessage("Priya Nair", AvatarTone.MINT, "10:42", 642, "Pushed the new onboarding flow to staging. Could you look before standup?"),
            PreviewMessage("Priya Nair", AvatarTone.MINT, "10:42", 642, "The invite step is the one I’m least sure about."),
            PreviewMessage(
                "Tomás Reyes", AvatarTone.AMBER, "10:44", 644, "On it. Here’s the first screen on my phone:",
                picture = true,
                reactions = listOf(ReactionSummary("👀", 2u, false, emptyList()), ReactionSummary("🎉", 3u, true, listOf(ME))),
            ),
            PreviewMessage("Mei Chen", AvatarTone.INDIGO, "10:51", 651, "Release notes are up for review too: [lumia.chat/blog/0-13](https://lumia.chat/blog/0-13)", card = true),
        )

        /** Each sample's place in its block, by the conversation's own rule: Priya's two are one block. */
        val PLACES: List<BlockPlace> = blockPlaces(SAMPLES.map { Stamp(it.author, it.minute * 60_000_000L) }, ZoneOffset.UTC)
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
