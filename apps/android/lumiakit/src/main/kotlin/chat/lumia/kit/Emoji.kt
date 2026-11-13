// The emoji the picker offers (architecture 6.6): every fully-qualified
// emoji in the default skin tone, by Unicode's groups, with its name and
// shortcodes to search by. The list is `emoji.tsv` beside this file (a
// Java resource, so the JVM tests read it as the app does), written from
// the `emojis` crate the desktop shell uses (0.9.1, Unicode's list) up to
// Unicode 16: a phone's emoji font lags Unicode, and an emoji it does not
// have draws as an empty box. One line each: the emoji, its group, its
// name, its shortcodes joined by commas.
package chat.lumia.kit

/** One emoji to choose. */
data class Emoji(val emoji: String, val group: String, val name: String, val shortcodes: List<String>)

/** How many a search shows at most. */
const val EMOJI_FOUND = 64

object EmojiSet {
    /** Unicode's groups in their order, as the picker heads them. */
    val groupNames = linkedMapOf(
        "SmileysAndEmotion" to "Smileys & emotion",
        "PeopleAndBody" to "People & body",
        "AnimalsAndNature" to "Animals & nature",
        "FoodAndDrink" to "Food & drink",
        "TravelAndPlaces" to "Travel & places",
        "Activities" to "Activities",
        "Objects" to "Objects",
        "Symbols" to "Symbols",
        "Flags" to "Flags",
    )

    /** Every emoji offered, in Unicode's order; read once, when first wanted. */
    val all: List<Emoji> by lazy {
        val text = EmojiSet::class.java.getResourceAsStream("emoji.tsv")?.use { it.readBytes().toString(Charsets.UTF_8) } ?: ""
        parse(text)
    }

    /** The emoji by group, each under its heading, in Unicode's order. */
    val byGroup: List<Pair<String, List<Emoji>>> by lazy {
        val grouped = all.groupBy { it.group }
        groupNames.mapNotNull { (key, heading) -> grouped[key]?.let { heading to it } }
    }

    fun parse(text: String): List<Emoji> = text.lineSequence().mapNotNull { line ->
        val parts = line.split('\t')
        if (parts.size < 3 || parts[0].isEmpty()) null
        else Emoji(parts[0], parts[1], parts[2], parts.getOrNull(3)?.split(',')?.filter { it.isNotEmpty() } ?: emptyList())
    }.toList()

    /** The name of an emoji offered, for a screen reader; none for one not on the list. */
    fun nameOf(emoji: String): String? = all.firstOrNull { it.emoji == emoji }?.name

    /** What a search finds: those whose name or a shortcode holds every word of it, at most [EMOJI_FOUND]. */
    fun search(wanted: String, among: List<Emoji> = all): List<Emoji> {
        val words = wanted.trim().lowercase().split(Regex("\\s+")).filter { it.isNotEmpty() }
        if (words.isEmpty()) return emptyList()
        return among.asSequence()
            .filter { e -> words.all { w -> e.name.lowercase().contains(w) || e.shortcodes.any { it.contains(w) } } }
            .take(EMOJI_FOUND)
            .toList()
    }
}
