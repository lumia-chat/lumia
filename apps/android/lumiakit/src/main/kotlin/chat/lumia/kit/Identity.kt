// Setting up private messages (identity spec 4.1, 4.3, 4.7; architecture
// 5.4; issue #20), as plain values `src/test` checks on the JVM: what a
// signed-in phone needs before it can send one, when Home reminds it to
// save the recovery phrase, the three-word check's fields, the phrase as a
// restore field counts it and as a printed page or a file lays it out,
// restoring while keeping devices (4.3 step 3): which devices are offered,
// how one reads and what the button says by how many are ticked,
// approving another device (4.2): what a pasted request reads as and the
// code it answers, linking by prompt (4.2.1): the new device's stages and
// the prompt another device shows, which one and in what words, and the
// words the screens say. The phrase itself, the quiz's positions, whether
// a word is right, what a request holds and the six digits are the core's.
package chat.lumia.kit

import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale
import uniffi.lumia_ffi.DeviceSummary

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

// Restoring, keeping devices (identity spec 4.3 step 3)

/** Restore's first question, before the phrase: a device still at hand links this phone instead. */
const val RESTORE_ASK = "Do you still have a device with your private messages?"
const val RESTORE_ASK_LINE = "If you do, link this phone from it instead. Restoring is for when your devices are gone: it stops the ones you don't keep."

/** What "Keep any devices?" says over the devices, and the warning under them. */
const val RESTORE_KEEP_LINE = "Tick the devices you still have. The others stop reading your private messages, so a lost one can't keep reading them."
const val RESTORE_KEEP_WARNING = "Keep only devices you have with you now. A kept device you've lost would keep reading your private messages."

/**
 * A restore between the phrase and "Keep any devices?": the phrase, the
 * devices restoring would stop (the core's `identityRecoverDevices`) and
 * the ids ticked to keep. Held in memory by the session for that step
 * alone and dropped when the restore ends or is left; never saved, and
 * never written out (`toString` leaves the phrase out).
 */
class Restoring(val phrase: String, val devices: List<DeviceSummary>, val kept: Set<String> = emptySet()) {
    /** The same, with [deviceId] ticked if it was not and not if it was. */
    fun toggled(deviceId: String): Restoring =
        Restoring(phrase, devices, if (deviceId in kept) kept - deviceId else kept + deviceId)

    /** What restoring keeps: the ticked ids, in the list's order. */
    val keep: List<String> get() = devices.map { it.deviceId }.filter { it in kept }

    override fun toString(): String = "Restoring(${devices.size} devices, ${kept.size} kept)"
}

/**
 * The devices "Keep any devices?" lists, from what the core answered for
 * the phrase: none revoked. Empty, the phrase restores straight away, as
 * it did before there was a choice.
 */
fun restoreChoices(devices: List<DeviceSummary>): List<DeviceSummary> =
    devices.filter { !it.revoked }

/** The restore button, by how many devices are ticked. */
fun restoreKeepButton(kept: Int): String = when (kept) {
    0 -> "Restore and stop them all"
    1 -> "Restore and keep 1 device"
    else -> "Restore and keep $kept devices"
}

/** A device's name in the list: its label, else what it is. [kind] is the core's: `native`, `web` or `bot`. */
fun restoreDeviceName(kind: String, label: String): String = label.trim().ifEmpty {
    when (kind) {
        "web" -> "Browser"
        "bot" -> "Bot"
        else -> "Device"
    }
}

/**
 * The line under a device's name: "Added Nov 12", or for a browser the
 * site it is on first, "On chat.acme.dev · added Nov 1" ([webOrigin]
 * without its scheme). Nothing of when for a device with no date.
 */
fun restoreDeviceMeta(webOrigin: String, addedAtMicros: Long, zone: ZoneId = ZoneId.systemDefault()): String {
    val day = if (addedAtMicros > 0) {
        Instant.ofEpochMilli(addedAtMicros / 1000).atZone(zone).format(DateTimeFormatter.ofPattern("MMM d", Locale.ENGLISH))
    } else null
    val site = webOrigin.substringAfter("://").trimEnd('/')
    return when {
        site.isNotEmpty() && day != null -> "On $site · added $day"
        site.isNotEmpty() -> "On $site"
        day != null -> "Added $day"
        else -> ""
    }
}

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

// Approving a device (identity spec 4.2)

/**
 * What the new device's Link screen says over its request: where to take
 * it, on a device that has the private messages, and what comes back.
 */
const val LINK_REQUEST_HOW = "On a device that has your private messages, open Devices, choose Approve a device and paste this request. Then type the code it shows here."

/** What the approve field sends the core: the request without the spaces and line breaks a paste or a share may have put in it. */
fun linkRequestOf(text: String): String = text.filterNot { it.isWhitespace() }

/**
 * Whether what was pasted is worth asking the core about: something is
 * there, and nothing a request (base64) cannot hold. Whether it is one is
 * the core's to say (`linkRequestSummary`).
 */
fun linkRequestWorthAsking(text: String): Boolean {
    val request = linkRequestOf(text)
    return request.isNotEmpty() && request.all { it in 'A'..'Z' || it in 'a'..'z' || it in '0'..'9' || it == '+' || it == '/' || it == '=' }
}

/** What the approve screen says of pasted text that is not a request. */
const val NOT_A_LINK_REQUEST = "That isn't a link request. Copy the whole request the new device shows, then paste it here."

/**
 * Who a request would add, as the approve screen names it (step 2): a
 * browser by the origin it is scoped to, anything else by its label.
 * [kind] is the core's: `native`, `web` or `bot`.
 */
fun linkRequestWho(kind: String, label: String, webOrigin: String): String {
    val name = label.trim()
    return when {
        kind == "web" && webOrigin.isNotEmpty() -> "A browser on $webOrigin"
        kind == "web" -> "A browser"
        kind == "bot" && name.isNotEmpty() -> "A bot named “$name”"
        kind == "bot" -> "A bot"
        name.isNotEmpty() -> "A device named “$name”"
        else -> "A device with no name"
    }
}

/**
 * The line under it: a browser's own name where it gave one, and the
 * device id's first eight, as Devices lists the device once it is added.
 */
fun linkRequestMeta(kind: String, label: String, deviceId: String): String {
    val name = label.trim()
    val id = deviceId.take(8)
    return if (kind == "web" && name.isNotEmpty()) "$name · $id" else id
}

/** What approving lets the new device do, said before Approve. */
const val LINK_APPROVE_WARNING = "It will read your private messages from now on, as this phone does. Approve only a device you have with you."

/** What the code is for, said under it. */
const val LINK_CODE_HOW = "Type this code on the new device. It works once, within ten minutes."

/** The code an approval answers, as it is read out: two groups of four. */
fun linkCodeForReading(code: String): String = code.trim().chunked(4).joinToString(" ")

/** The same to a screen reader, a character at a time. */
fun linkCodeSpoken(code: String): String = code.trim().toList().joinToString(" ")

/**
 * What an approval the instance refused says, by the API error's [code];
 * for the rest, [message] (the failure as `lumiaMessage` has it).
 */
fun approveError(code: String?, message: String): String = when (code) {
    "CONFLICT" -> "Your devices changed on another device while this one was approving. Try again."
    "FAILED_PRECONDITION", "PERMISSION_DENIED" -> "This account isn't linked to the identity on this phone, so it can't add a device to it."
    "IDENTITY_PROOF_INVALID" -> "The instance refused the new list of devices. Start linking again on the new device and paste its new request."
    "INVALID_ARGUMENT", "PAYLOAD_TOO_LARGE" -> "The instance couldn't take this request. Start linking again on the new device and paste its new request."
    "RATE_LIMITED" -> "Too many tries. Wait a minute, then try again."
    else -> "Not approved: $message"
}

/**
 * An approval (steps 3 to 6): under way, answered with the code the new
 * device claims with, or refused with what to say. The session holds it,
 * so the phone turned or the screen left while it runs keeps the code.
 */
data class Approval(val request: String, val code: String? = null, val error: String? = null) {
    val underWay: Boolean get() = code == null && error == null
}

// Linking by prompt (identity spec 4.2.1)

/** How often the link screen, and a prompt while one is open, ask again besides at every account event. */
const val LINK_POLL_MS = 2_000L

/** How long "Pixel 9 is linked." or "Linking was stopped." stays before the prompt goes. */
const val PROMPT_DONE_MS = 2_500L

/**
 * Where linking this phone by prompt stands, as the new device: starting
 * (asked for, not answered), waiting for another device to answer,
 * comparing the digits, linked, or over: declined on the other device,
 * expired, or not started at all ([FAILED], with what went wrong).
 */
enum class LinkStage {
    STARTING, WAITING, COMPARE, LINKED, DECLINED, EXPIRED, FAILED;

    /** Whether the link screen asks again: while another device may still move it. */
    val polls: Boolean get() = this == WAITING || this == COMPARE

    /** Whether it is over and only Try again and Cancel are left. */
    val over: Boolean get() = this == DECLINED || this == EXPIRED || this == FAILED
}

/** The core's `LinkProgress.stage` as a stage; one this build does not know is still waiting. */
fun linkStageOf(stage: String): LinkStage = when (stage) {
    "compare" -> LinkStage.COMPARE
    "linked" -> LinkStage.LINKED
    "declined" -> LinkStage.DECLINED
    "expired" -> LinkStage.EXPIRED
    else -> LinkStage.WAITING
}

/**
 * This phone's link by prompt: its stage, the six digits while comparing
 * (the core's, already `482 913`), and a failure in words: the reason it
 * did not start, or an ask that did not reach the instance, which the next
 * one clears.
 */
data class Linking(val stage: LinkStage, val digits: String = "", val error: String? = null)

/** What the new device says while it waits for another to answer. */
const val LINK_WAITING = "Open Lumia on a device that has your private messages. It will ask about this phone."

/** What it says under the digits. */
const val LINK_COMPARE_HOW = "Check that your other device shows the same number, then approve it there."

/** What it says when the other device said no, or nobody answered in time. */
const val LINK_DECLINED = "It wasn't approved on your other device."
const val LINK_EXPIRED = "That took too long."

/** What the stage says on the new device; none for one with nothing to add. */
fun linkStageWords(stage: LinkStage): String? = when (stage) {
    LinkStage.WAITING -> LINK_WAITING
    LinkStage.COMPARE -> LINK_COMPARE_HOW
    LinkStage.DECLINED -> LINK_DECLINED
    LinkStage.EXPIRED -> LINK_EXPIRED
    LinkStage.STARTING, LinkStage.LINKED, LinkStage.FAILED -> null
}

/** What a link that did not start says, by the API error's [code]; for the rest, [message]. */
fun linkStartError(code: String?, message: String): String = when (code) {
    "RATE_LIMITED" -> "Too many devices are waiting to be linked. Wait a few minutes, then try again."
    else -> "Linking didn't start: $message"
}

/** The six digits to a screen reader, one at a time: "4 8 2 9 1 3". */
fun linkDigitsSpoken(digits: String): String = digits.filterNot { it.isWhitespace() }.toList().joinToString(" ")

/**
 * Where a prompt stands on this phone, the device that holds the identity:
 * nobody answered yet, answered here and waiting for the new device,
 * comparing the digits, or refused (the new device's secret did not match
 * what it committed to: never approve).
 */
enum class PromptStage { NEW, WAITING, COMPARE, REFUSED }

/** The core's `LinkPrompt.stage` as a stage; one this build does not know offers only Not me, as a refused one does. */
fun promptStageOf(stage: String): PromptStage = when (stage) {
    "new" -> PromptStage.NEW
    "waiting" -> PromptStage.WAITING
    "compare" -> PromptStage.COMPARE
    else -> PromptStage.REFUSED
}

/**
 * Which prompt is shown, of the sessions listed (oldest first, as the
 * instance lists them), one at a time: the one showing while it is still
 * listed, else the oldest; none when none is.
 */
fun promptToShow(listed: List<String>, showing: String?): String? =
    if (showing != null && showing in listed) showing else listed.firstOrNull()

/**
 * Who a prompt is about: a browser by the origin it is scoped to, anything
 * else by its label, or by what it is without one; [start] for the start
 * of a sentence ("The browser on …"), else as it reads inside one ("the
 * browser on …"). [kind] is the core's: `native`, `web` or `bot`.
 */
fun promptName(kind: String, label: String, webOrigin: String, start: Boolean): String {
    val name = label.trim()
    val words = when {
        kind == "web" && webOrigin.isNotEmpty() -> "the browser on $webOrigin"
        kind == "web" -> "the browser"
        name.isNotEmpty() -> return name
        kind == "bot" -> "the bot"
        else -> "the new device"
    }
    return if (start) words.replaceFirstChar { it.uppercase() } else words
}

/** A prompt's title, by its stage. */
fun promptTitle(stage: PromptStage, kind: String, label: String, webOrigin: String): String = when (stage) {
    PromptStage.NEW -> when {
        kind == "web" && webOrigin.isNotEmpty() -> "A browser on $webOrigin"
        kind == "web" -> "A browser"
        label.isNotBlank() -> label.trim()
        kind == "bot" -> "A bot"
        else -> "A new device"
    } + " wants your private messages"
    PromptStage.WAITING -> "Waiting for ${promptName(kind, label, webOrigin, start = false)}…"
    PromptStage.COMPARE -> "Does ${promptName(kind, label, webOrigin, start = false)} show this number?"
    PromptStage.REFUSED -> "This request can't be trusted"
}

/** The line under a prompt's title; none for a stage with nothing to add. */
fun promptLine(stage: PromptStage): String? = when (stage) {
    PromptStage.NEW -> "Only continue if you're linking a device of yours right now."
    PromptStage.REFUSED -> "Don't approve it."
    PromptStage.WAITING, PromptStage.COMPARE -> null
}

/** What the prompt says once it was approved, before it goes. */
fun promptLinkedWords(kind: String, label: String, webOrigin: String): String =
    "${promptName(kind, label, webOrigin, start = true)} is linked."

/** What it says once "They don't match" stopped it. */
const val PROMPT_STOPPED = "Linking was stopped."

/**
 * What a prompt's call that failed says, by the API error's [code]; for
 * the rest, [message] (the failure as `lumiaMessage` has it).
 */
fun promptError(code: String?, message: String): String = when (code) {
    "FAILED_PRECONDITION" -> "Another of your devices answered it first."
    "NOT_FOUND" -> "That request is over. Start linking again on the new device."
    "CONFLICT" -> "Your devices changed on another device while this one was approving. Try again."
    "IDENTITY_PROOF_INVALID", "INVALID_ARGUMENT", "PAYLOAD_TOO_LARGE" -> "The instance refused the new list of devices. Start linking again on the new device."
    "PERMISSION_DENIED" -> "This account isn't linked to the identity on this phone, so it can't add a device to it."
    "RATE_LIMITED" -> "Too many tries. Wait a minute, then try again."
    else -> message.replaceFirstChar { it.uppercase() }
}
