// A server's own emoji on the phone (specs/emoji.md), as values: the
// `:word` at the caret and what choosing puts in, "Frequently used" in a
// server and out of one, the Emoji section's words and its refusals', and
// a paragraph with an emoji held drawn as its picture's place, one not held
// as its words. The rules that are the core's are asked of the core itself
// (the library `scripts/core.sh` builds for this machine), as `ViewerTest`
// does, so the phone's side is checked against what the core gives it.
package chat.lumia.kit

import androidx.compose.ui.text.SpanStyle
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.lumia_ffi.BodyBlock
import uniffi.lumia_ffi.BodyStyle
import uniffi.lumia_ffi.EmojiRefSummary
import uniffi.lumia_ffi.EmojiSummary
import uniffi.lumia_ffi.emojiNameFromFile
import uniffi.lumia_ffi.isEmojiName
import uniffi.lumia_ffi.parseBodyMarked

private val plain = SpanStyle()
private val look = InlineLook(plain, plain, plain, plain, plain, plain, plain)

private fun emoji(id: String, name: String, animated: Boolean = false, width: UInt = 64u, height: UInt = 64u) =
    EmojiSummary(id, name, "/api/v1/avatars/" + "a".repeat(64), animated, width, height, "maya")

private val parrot = emoji("e8", "partyparrot", animated = true)
private val lgtm = emoji("e2", "lgtm", width = 128u, height = 64u)
private val acme = listOf(lgtm, parrot)

private fun pictureOf(path: String) = pictureAddress("https://chat.acme.dev", path)

/** The UTF-8 refs the core would have found for each `:name:` of [names] in [body], as a message carries them. */
private fun refs(body: String, vararg names: Pair<String, String>): List<EmojiRefSummary> = names.map { (name, id) ->
    val at = body.indexOf(":$name:")
    val start = body.substring(0, at).toByteArray().size
    EmojiRefSummary(id, start.toUInt(), (start + name.length + 2).toUInt())
}

class ServerEmojiTest {
    @Test
    fun theColonWordAtTheCaretOpensTheList() {
        val text = "Shipped the importer :par"
        assertEquals(EmojiQuery(21, 25, "par"), emojiAt(text, 25, 25))
        // At the start, and with capitals, which the core lowercases.
        assertEquals(EmojiQuery(0, 4, "Par"), emojiAt(":Par", 4, 4))
        // One letter is a query; whether it lists anything is the core's (nothing under two).
        assertEquals(EmojiQuery(0, 2, "p"), emojiAt(":p", 2, 2))
        // Not a time, not a word's colon, not a colon alone, not past a space.
        assertNull(emojiAt("at 12:30", 8, 8))
        assertNull(emojiAt("note:par", 8, 8))
        assertNull(emojiAt("so :", 4, 4))
        assertNull(emojiAt(":par ty", 7, 7))
        // Not while something is selected, nor with the caret before it.
        assertNull(emojiAt(text, 22, 25))
        assertNull(emojiAt(text, 10, 10))
    }

    @Test
    fun choosingCompletesTheNameWithASpace() {
        val text = "Shipped :par today"
        val q = emojiAt(text, 12, 12)!!
        assertEquals(Picked("Shipped :partyparrot:  today", 22), pickEmoji(text, q, "partyparrot"))
    }

    @Test
    fun theEmojiButtonPutsInAtTheCaretOverWhatIsSelected() {
        // 🎉 is two UTF-16 units, as the field counts.
        assertEquals(Picked("Hi 🎉there", 5), insertAt("Hi there", 3, 3, "🎉"))
        assertEquals(Picked(":lgtm: ", 7), insertAt("", 0, 0, EmojiPick.Custom(lgtm).inserted))
        // A selection is replaced, whichever way it was made.
        assertEquals(Picked("Ship :lgtm: now", 12), insertAt("Ship it now", 8, 5, ":lgtm: "))
        assertEquals("👍", EmojiPick.Unicode("👍").inserted)
    }

    @Test
    fun frequentlyUsedHoldsAServersOwnOnlyInThatServer() {
        val recent = listOf(CUSTOM_KEPT + "e8", "🔥", CUSTOM_KEPT + "gone", "👍")
        val inAcme = frequentIn(recent, acme)
        assertEquals(EmojiPick.Custom(parrot), inAcme[0])
        assertEquals(listOf("🔥", "👍", "❤️", "😂", "🎉", "😮", "😢"), inAcme.drop(1).map { (it as EmojiPick.Unicode).emoji })
        // In a personal conversation, and in another server, none of Acme's.
        assertEquals(listOf("🔥", "👍", "❤️", "😂", "🎉", "😮", "😢"), frequentIn(recent, null).map { (it as EmojiPick.Unicode).emoji })
        assertTrue(frequentIn(recent, emptyList()).none { it is EmojiPick.Custom })
        assertEquals(frequentEmoji(recent), frequentIn(recent, null).map { (it as EmojiPick.Unicode).emoji })
        // What is kept of each, and at most sixteen shown.
        assertEquals(CUSTOM_KEPT + "e8", EmojiPick.Custom(parrot).kept)
        assertEquals(RECENT_EMOJI, frequentIn(List(30) { "x$it" }, acme).size)
    }

    @Test
    fun theSectionSaysItsCountAndItsLimit() {
        assertEquals("12 of 50", emojiCount(12, 50u))
        assertEquals("3", emojiCount(3, 0u))
        assertFalse(emojiFull(49, 50u))
        assertTrue(emojiFull(50, 50u))
        // Above a lowered limit is at it too.
        assertTrue(emojiFull(60, 50u))
        assertFalse(emojiFull(60, 0u))
        assertEquals("Acme has as many emoji as it may (50). Remove one to upload another.", emojiFullWords("Acme", 50u))
        assertEquals("Uploaded by Devon Park · Animated", uploadedBy("Devon Park", true))
        assertEquals("Uploaded by Maya Chen", uploadedBy("Maya Chen", false))
        assertEquals("Remove :parrotwave:?", removeEmojiQuestion("parrotwave"))
        assertEquals("2 to 32 letters, digits and _. It is typed as :acme_heart_v2: in a message.", emojiNameHint("acme_heart_v2"))
        assertEquals("2 to 32 letters, digits and _. It is typed as :name: in a message.", emojiNameHint(""))
    }

    @Test
    fun aRefusalIsSaidWhereItBelongsInTheBoardsWords() {
        fun r(code: String, vararg details: Pair<String, String>) = emojiRefusal(code, details.toMap(), "Acme", "partyparrot", 50u, "the instance's words")
        assertEquals(EmojiRefusal(EmojiAt.NAME, "Acme already has :partyparrot:. Choose another name."), r("ALREADY_EXISTS", "existing_id" to "e8"))
        assertEquals(EmojiRefusal(EmojiAt.NAME, EMOJI_NAME_RULE), r("INVALID_ARGUMENT", "field" to "name"))
        assertEquals(EmojiRefusal(EmojiAt.PICTURE, EMOJI_TOO_LARGE), r("INVALID_ARGUMENT", "field" to "picture", "reason" to "too_large"))
        assertEquals(EmojiRefusal(EmojiAt.PICTURE, EMOJI_UNSUPPORTED), r("INVALID_ARGUMENT", "field" to "picture", "reason" to "unsupported"))
        assertEquals(EmojiRefusal(EmojiAt.PICTURE, EMOJI_UNREADABLE), r("INVALID_ARGUMENT", "field" to "picture", "reason" to "invalid"))
        assertEquals(EmojiRefusal(EmojiAt.FORM, "Acme has as many emoji as it may (40). Remove one to upload another."), r("RESOURCE_EXHAUSTED", "state" to "emoji", "limit" to "40"))
        assertEquals(EmojiRefusal(EmojiAt.NAME, EMOJI_NAME_RULE), r("INVALID_ARGUMENT"))
        assertEquals(EmojiRefusal(EmojiAt.FORM, "You can’t manage Acme’s emoji."), r("PERMISSION_DENIED"))
        assertEquals(EmojiRefusal(EmojiAt.FORM, "That emoji has been removed."), r("NOT_FOUND"))
        assertEquals(EmojiRefusal(EmojiAt.FORM, "This instance does not keep pictures."), r("FAILED_PRECONDITION", "state" to "blobs"))
        assertEquals(EmojiRefusal(EmojiAt.FORM, "The instance is busy. Try again in a moment."), r("RATE_LIMITED"))
        assertEquals(EmojiRefusal(EmojiAt.FORM, "the instance's words"), r("UNAVAILABLE"))
    }

    @Test
    fun theCoreNamesAnUploadFromItsFileAndHoldsTheRule() {
        assertEquals("acme_heart_v2", emojiNameFromFile("Acme-Heart v2.png"))
        assertTrue(isEmojiName("partyparrot"))
        assertFalse(isEmojiName("Party Parrot"))
        assertFalse(isEmojiName("p"))
    }

    @Test
    fun anEmojiHeldIsItsPicturesPlaceAndItsNameToAScreenReader() {
        val body = "Shipped 🚀 :partyparrot: and :oldlogo:"
        // The core reads the body with its refs: a ref to one held, and one to an emoji removed since.
        val marked = refs(body, "partyparrot" to "e8", "oldlogo" to "e0")
        val para = parseBodyMarked(body, emptyList(), marked).single() as BodyBlock.Para
        assertEquals(2, para.inline.runs.count { it.style is BodyStyle.Emoji })
        val held = acme.mapNotNull { drawnEmoji(it, ::pictureOf) }.associateBy { it.id }
        val styled = styledInline(para.inline, look, emptyList(), { _, typed -> typed }, emoji = marked, emojiOf = { held[it] })
        // The words are the text's own; the one held is marked for its picture, the one not held is its words as typed.
        assertEquals(body, styled.text.text)
        val marks = styled.text.getStringAnnotations(INLINE_CONTENT_TAG, 0, styled.text.length)
        assertEquals(listOf("emoji e8"), marks.map { it.item })
        assertEquals(":partyparrot:", styled.text.text.substring(marks[0].start, marks[0].end))
        assertEquals(listOf("e8"), styled.emoji.map { it.id })
        assertEquals(body, styled.words)
    }

    @Test
    fun aRenamedEmojiIsDrawnByItsPresentName() {
        val body = "nice :lgtm: work"
        val marked = refs(body, "lgtm" to "e2")
        val para = parseBodyMarked(body, emptyList(), marked).single() as BodyBlock.Para
        val renamed = drawnEmoji(emoji("e2", "looks_good", width = 128u, height = 64u), ::pictureOf)!!
        val styled = styledInline(para.inline, look, emptyList(), { _, typed -> typed }, emoji = marked, emojiOf = { if (it == "e2") renamed else null })
        assertEquals("nice :looks_good: work", styled.text.text)
        assertEquals("nice :looks_good: work", styled.words)
        val mark = styled.text.getStringAnnotations(INLINE_CONTENT_TAG, 0, styled.text.length).single()
        assertEquals(":looks_good:", styled.text.text.substring(mark.start, mark.end))
        // Twice as wide as tall, drawn so.
        assertEquals(2f, renamed.aspect)
    }

    @Test
    fun onlyTheInstancesOwnPictureIsDrawn() {
        assertEquals("https://chat.acme.dev/api/v1/avatars/" + "a".repeat(64), drawnEmoji(parrot, ::pictureOf)?.address)
        assertNull(drawnEmoji(emoji("x", "elsewhere").copy(imageUrl = "https://evil.example/p.gif"), ::pictureOf))
        assertEquals(":partyparrot:", drawnEmoji(parrot, ::pictureOf)?.label)
        assertEquals("par", emojiSearchPrefix(" :par: "))
    }
}
