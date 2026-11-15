// Setting up private messages (identity spec 4.1, 4.3, 4.7; architecture
// 5.4; issue #20), as plain values `src/test` checks on the JVM: what a
// signed-in phone needs before it can send one, when Home reminds it to
// save the recovery phrase, the three-word check's fields, the phrase as a
// restore field counts it and as a printed page or a file lays it out, and
// the words the screens say. The phrase itself, the quiz's positions and
// whether a word is right are the core's.
package chat.lumia.kit

/** What a signed-in phone needs before it can send or accept a private message. */
enum class PrivateMessages {
    /** This phone holds the identity the account is bound to. */
    READY,

    /**
     * The account has no identity: set it up here, as sign-up does, with
     * no question unless a reauth must be asked for first.
     */
    SET_UP,

    /** The account's identity is on another device: link this phone, restore it with the phrase, or start over. */
    ELSEWHERE,
}

/**
 * What a phone holding the identity [held] (hex, or none) needs for an
 * account bound to [bound] (hex, or none: the core's `accountIdentity`).
 * An account with none is set up here, with the identity this phone
 * holds when it holds one; one bound elsewhere never gets a second.
 */
fun privateMessagesFor(held: String?, bound: String?): PrivateMessages = when {
    bound.isNullOrEmpty() -> PrivateMessages.SET_UP
    held != null && held.equals(bound, ignoreCase = true) -> PrivateMessages.READY
    else -> PrivateMessages.ELSEWHERE
}

/**
 * What Home says where a phone cannot send private messages yet, by what
 * it needs; none when it can. Not asked yet (the instance not reached), a
 * phone holding an identity is taken to be ready, and one holding none
 * is told what is true either way.
 */
fun setupNotice(hasIdentity: Boolean, need: PrivateMessages?): String? = when (need) {
    PrivateMessages.READY -> null
    PrivateMessages.SET_UP -> "Private messages aren't set up on this phone yet."
    PrivateMessages.ELSEWHERE -> "Your account has private messages on another device. Bring this phone in to read and send them here."
    null -> if (hasIdentity) null else "Private messages need this phone."
}

// The reminder

/** How long "In a week" puts Home's reminder off. */
const val PHRASE_SNOOZE_MS = 7L * 24 * 60 * 60 * 1000

/** Until when Home's reminder is put off, "In a week" being tapped at [now] (milliseconds since the epoch). */
fun phraseSnoozedUntil(now: Long): Long = now + PHRASE_SNOOZE_MS

/** The time a reminder was put off until, as this phone keeps it; none for nothing kept or a value it cannot read. */
fun phraseSnoozeOf(stored: String?): Long? = stored?.trim()?.toLongOrNull()

/**
 * Whether Home shows the reminder: while the phrase waits to be confirmed,
 * unless it was put off until after [now]. Devices' card is not put off.
 */
fun phraseReminderShows(unconfirmed: Boolean, snoozedUntil: Long?, now: Long): Boolean =
    unconfirmed && (snoozedUntil == null || now >= snoozedUntil)

// The phrase

/** How many words a recovery phrase has. */
const val PHRASE_WORDS = 24

/** What the phrase screen, the printed page and the file say over the words. */
const val PHRASE_WARNING = "Whoever has these 24 words has your identity and your private messages. Lumia can't recover them for you."

/** What the phrase screen says under them. */
const val PHRASE_KEEP = "Write them down in order, or print them, and keep them somewhere safe and offline. There's no copy button: clipboards sync to other devices, and other apps can read them."

/** The question before a phrase is saved as a file, and the line under it. */
const val PHRASE_SAVE_TITLE = "Save your phrase as a file?"
const val PHRASE_SAVE_WARNING = "This file is your identity: anyone who opens it can read your private messages."

/** The warning in the save sheet's note: a synced folder is a cloud's copy. */
const val PHRASE_SAVE_CLOUD = "A folder that syncs to iCloud, Google Drive or OneDrive puts a copy in that cloud. Paper, or a drive you keep offline, is safer."

/** The name a saved phrase is offered under, and the printed page's. */
const val PHRASE_FILE_NAME = "Lumia recovery phrase.txt"
const val PHRASE_DOCUMENT_TITLE = "Lumia recovery phrase"

/** A phrase's words, as the core gives them (separated by spaces) or as typed: lower case, any run of space between two. */
fun phraseWords(text: String): List<String> = text.trim().split(Regex("\\s+")).filter { it.isNotEmpty() }.map { it.lowercase() }

/** The restore field's count, "12 of 24". */
fun phraseCount(text: String): String = "${phraseWords(text).size} of $PHRASE_WORDS"

/** Whether Restore is offered: once all 24 words are in, and no more. */
fun restoreReady(text: String): Boolean = phraseWords(text).size == PHRASE_WORDS

/** What Restore sends the core: the words, one space between each. */
fun phraseOf(text: String): String = phraseWords(text).joinToString(" ")

/**
 * The words as the screen and the printed page lay them out: numbered
 * from 1, in two columns read down, the first half in the first.
 */
fun phraseColumns(words: List<String>): List<List<Pair<Int, String>>> {
    val numbered = words.mapIndexed { i, w -> (i + 1) to w }
    val half = (numbered.size + 1) / 2
    return listOf(numbered.take(half), numbered.drop(half))
}

/**
 * The saved file: the title, the warning, then the words numbered one a
 * line, in order, so nothing in its layout can be read in the wrong order.
 */
fun phraseDocument(words: List<String>): String = buildString {
    append(PHRASE_DOCUMENT_TITLE).append("\n\n")
    append(PHRASE_WARNING).append("\n\n")
    words.forEachIndexed { i, w -> append("${i + 1}. $w\n") }
}

// The three-word check

/** A word as a check field holds it: lower case, no spaces. */
fun quizWord(text: String): String = text.filterNot { it.isWhitespace() }.lowercase()

/**
 * The check (identity spec 4.1 step 7): the three positions the core asks
 * for, 1 to 24, and what each field holds. Confirm is offered once every
 * field holds a word; whether they are right is the core's to say.
 */
data class PhraseQuiz(val positions: List<Int>, val words: List<String> = positions.map { "" }) {
    /** The field at [index] typed into: kept as [quizWord] has it. */
    fun typed(index: Int, text: String): PhraseQuiz =
        copy(words = words.mapIndexed { i, w -> if (i == index) quizWord(text) else w })

    /** Whether Confirm is offered. */
    val ready: Boolean get() = words.size == positions.size && positions.isNotEmpty() && words.all { it.isNotEmpty() }

    /**
     * The completions offered under the field at [index], of those the
     * wordlist gave for what it holds: none for an empty field, nor for
     * one already holding the only word they could be.
     */
    fun offered(index: Int, completions: List<String>): List<String> {
        val word = words.getOrElse(index) { "" }
        if (word.isEmpty() || completions == listOf(word)) return emptyList()
        return completions
    }
}

/** What a field's completions are to a screen reader, "Words starting with fa". */
fun completionsLabel(prefix: String): String = "Words starting with $prefix"

// Starting over

/** How many people this account talks to privately: the others in its personal conversations, each once. */
fun privateContacts(participants: List<List<String>>, me: String?): Int =
    participants.flatten().filter { it != me }.toSet().size

/**
 * What Start over says of the people who will see a key change (identity
 * spec 4.7): only someone who talked privately with the old identity
 * pinned it, so with nobody, nobody is warned, and the screen says which.
 */
fun keyChangeWords(people: Int): String = when (people) {
    0 -> "Nobody will see a key change: you have no private conversations yet."
    1 -> "The 1 person you talk to privately will see that your key changed."
    else -> "The $people people you talk to privately will see that your key changed."
}
