// What the Preferences screen chooses, as plain values `src/test` checks on
// the JVM: light, dark or as the phone is, and how much room a
// conversation's messages take. The desktop shell's `Appearance` and
// `Density` (apps/desktop/src/settings.rs) and its `Measures`
// (src/ui/timeline.rs), each choice stored under the name the desktop
// stores it by. They are this phone's and not the account's: `AppStore`
// keeps them beside the saved instance, and nothing of them is sent to it.
package chat.lumia.kit

import androidx.compose.ui.unit.Dp

/** Light, dark, or whichever the phone is in. */
enum class Appearance(val stored: String) {
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
        fun named(stored: String?): Appearance? = entries.firstOrNull { it.stored == stored }

        /** What a stored choice reads as: nothing stored, or a name this build does not know, is the default. */
        fun of(stored: String?): Appearance = named(stored) ?: Default
    }
}

/** How much room a conversation's messages take. */
enum class Density(val stored: String) {
    /** The boards' measures. */
    COMFORTABLE("comfortable"),

    /** A smaller avatar and less room around each message and between blocks: more of a conversation on the screen. */
    COMPACT("compact");

    companion object {
        val Default = COMFORTABLE

        /** The density stored under this name, if it is one. */
        fun named(stored: String?): Density? = entries.firstOrNull { it.stored == stored }

        /** What a stored choice reads as: nothing stored, or a name this build does not know, is the default. */
        fun of(stored: String?): Density = named(stored) ?: Default
    }
}

/**
 * A message row's measures, which the density decides. Comfortable is what
 * the phone boards draw, with what they have no token for made from the
 * nearest ones; compact is the same row with the small avatar and half the
 * room between blocks, between a block's messages and around a day's
 * divider. The text is the same size in both, and nothing but the list of
 * messages changes: not the sidebar, the composer or the typing line.
 */
data class Measures(
    val avatar: Dp,
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
) {
    /** From the foot of one block's last message to the head of the next block. */
    val betweenBlocks: Dp get() = edge + blockGap + edge

    companion object {
        val Comfortable = Measures(
            avatar = LumiaTokens.avatarLg,
            edge = LumiaTokens.space8,
            blockGap = LumiaTokens.space8,
            beside = LumiaTokens.space12,
            within = LumiaTokens.space4,
            divider = LumiaTokens.space8,
        )

        val Compact = Measures(
            avatar = LumiaTokens.avatarSm,
            edge = LumiaTokens.space4,
            blockGap = LumiaTokens.space4,
            beside = LumiaTokens.space8,
            within = LumiaTokens.space2,
            divider = LumiaTokens.space4,
        )

        fun of(density: Density): Measures = when (density) {
            Density.COMFORTABLE -> Comfortable
            Density.COMPACT -> Compact
        }
    }
}
