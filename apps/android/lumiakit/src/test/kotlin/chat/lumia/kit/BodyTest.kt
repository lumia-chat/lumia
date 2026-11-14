// A message body's rules as values (issue #18): when a long body
// collapses, what the composer's counter says, and a paragraph of the
// core's blocks as styled text, its ranges in UTF-16 moved with the names
// and thin spaces put in.
package chat.lumia.kit

import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.lumia_ffi.BodyBlock
import uniffi.lumia_ffi.BodyInline
import uniffi.lumia_ffi.BodyLink
import uniffi.lumia_ffi.BodyRun
import uniffi.lumia_ffi.BodyStyle
import uniffi.lumia_ffi.MentionSummary
import uniffi.lumia_ffi.MessageRoom

private val look = InlineLook(
    emphasis = SpanStyle(fontStyle = FontStyle.Italic),
    strong = SpanStyle(fontWeight = FontWeight.Bold),
    strike = SpanStyle(textDecoration = TextDecoration.LineThrough),
    code = SpanStyle(fontWeight = FontWeight.Light),
    mention = SpanStyle(fontWeight = FontWeight.SemiBold),
    link = SpanStyle(textDecoration = TextDecoration.Underline),
    edited = SpanStyle(fontWeight = FontWeight.Thin),
)

private fun run(start: Int, end: Int, style: BodyStyle) = BodyRun(start.toUInt(), end.toUInt(), style)
private fun inline(text: String, vararg runs: BodyRun, links: List<BodyLink> = emptyList()) = BodyInline(text, runs.toList(), links)
private fun mention(account: String, everyone: Boolean = false) = MentionSummary(account, everyone, 0u, 0u)
private val names = mapOf("ada" to "Ada Lovelace")
private fun styled(i: BodyInline, mentions: List<MentionSummary> = emptyList(), edited: Boolean = false) =
    styledInline(i, look, mentions, { m, typed -> mentionWords(m, typed) { names[it] } }, edited)

/** The text each span of [style] covers. */
private fun StyledInline.covered(style: SpanStyle): List<String> =
    text.spanStyles.filter { it.item == style }.map { text.text.substring(it.start, it.end) }

private const val T = THIN_SPACE

class BodyTest {
    @Test
    fun aBodyCollapsesOverFourteenLinesToTen() {
        assertFalse(collapses(1))
        assertFalse(collapses(14))
        assertTrue(collapses(15))
        assertTrue(collapses(400))
        assertEquals(10, COLLAPSED_LINES)
        assertEquals("Show more", showWords(expanded = false))
        assertEquals("Show less", showWords(expanded = true))
    }

    @Test
    fun theCounterSaysWhatIsLeftOrHowMuchTooLongWithItsThousandsGrouped() {
        assertEquals("1,204 characters left", roomWords(1204))
        assertEquals("1 character left", roomWords(1))
        assertEquals("0 characters left", roomWords(0))
        assertEquals("1 character too long", roomWords(-1))
        assertEquals("12 characters too long", roomWords(-12))
        assertEquals("16,384 characters too long", roomWords(-16384))
        assertEquals("999", grouped(999))
        assertEquals("1,000", grouped(1000))
        assertEquals("1,234,567", grouped(1234567))
        assertFalse(isOver(null))
        assertFalse(isOver(MessageRoom(true, 0)))
        assertTrue(isOver(MessageRoom(true, -1)))
    }

    @Test
    fun onlyADraftThatCouldBeNearTheLimitIsAskedAbout() {
        // 16,384 bytes: the counter shows from 14,746; a unit is at most three bytes.
        assertFalse(mayNearLimit(0, 16384u))
        assertFalse(mayNearLimit(4915, 16384u))
        assertTrue(mayNearLimit(4916, 16384u))
        assertTrue(mayNearLimit(20000, 16384u))
    }

    @Test
    fun aStyledRunAfterAnEmojiIsWhereTheCoreSaysInUtf16() {
        // "😀 **bold**": the emoji is two units, so the bold is 3 to 7.
        val s = styled(inline("😀 bold", run(3, 7, BodyStyle.Strong)))
        assertEquals("😀 bold", s.text.text)
        assertEquals(listOf("bold"), s.covered(look.strong))
        assertEquals("😀 bold", s.words)
    }

    @Test
    fun aMentionIsTheNameInAPillAndWhatFollowsMovesWithIt() {
        // "😀 hi @ada *there*" as the core gives it.
        val i = inline("😀 hi @ada there", run(6, 10, BodyStyle.Mention(0u)), run(11, 16, BodyStyle.Emphasis))
        val s = styled(i, listOf(mention("ada")))
        assertEquals("😀 hi $T@Ada Lovelace$T there", s.text.text)
        assertEquals(listOf("$T@Ada Lovelace$T"), s.pills.map { s.text.text.substring(it) })
        assertEquals(listOf("$T@Ada Lovelace$T"), s.covered(look.mention))
        assertEquals(listOf("there"), s.covered(look.emphasis))
        // A screen reader has the words without the thin spaces.
        assertEquals("😀 hi @Ada Lovelace there", s.words)
    }

    @Test
    fun aMentionWithNoNameIsAsTypedAndEveryoneIsEveryone() {
        val i = inline("@bo and @everyone", run(0, 3, BodyStyle.Mention(0u)), run(8, 17, BodyStyle.Mention(1u)))
        val s = styled(i, listOf(mention("bo"), mention("", everyone = true)))
        assertEquals("${T}@bo$T and $T@everyone$T", s.text.text)
        // A run whose index is past the message's mentions is its text.
        val past = styled(inline("@x", run(0, 2, BodyStyle.Mention(4u))))
        assertEquals("$T@x$T", past.text.text)
    }

    @Test
    fun inlineCodeIsAChipPaddedByThinSpacesAndALinkAroundItStretches() {
        // "see [`x()` docs](https://lumia.chat) **now**"
        val i = inline(
            "see x() docs now",
            run(4, 7, BodyStyle.Code), run(13, 16, BodyStyle.Strong),
            links = listOf(BodyLink(4u, 12u, "https://lumia.chat")),
        )
        val s = styled(i)
        assertEquals("see ${T}x()$T docs now", s.text.text)
        assertEquals(listOf("${T}x()$T"), s.chips.map { s.text.text.substring(it) })
        assertEquals(listOf("now"), s.covered(look.strong))
        val links = s.text.getLinkAnnotations(0, s.text.length)
        assertEquals(1, links.size)
        assertEquals("https://lumia.chat", (links[0].item as LinkAnnotation.Url).url)
        assertEquals("${T}x()$T docs", s.text.text.substring(links[0].start, links[0].end))
        assertEquals("see x() docs now", s.words)
    }

    @Test
    fun editedFollowsTheLastParagraphsWordsOnly() {
        val s = styled(inline("done"), edited = true)
        assertEquals("done $EDITED", s.text.text)
        assertEquals(listOf(EDITED), s.covered(look.edited))
        assertEquals("done, $EDITED_LABEL", s.words)
        val para = BodyBlock.Para(inline("done"))
        assertTrue(editedInline(listOf(para)))
        assertFalse(editedInline(listOf(para, BodyBlock.Code(null, "x"))))
        assertFalse(editedInline(listOf(BodyBlock.Quote(listOf(para)))))
        assertFalse(editedInline(emptyList()))
    }

    @Test
    fun aListsMarkerIsABulletOrItsNumberFromItsStart() {
        assertEquals("•", listMarker(null, 3))
        assertEquals("1.", listMarker(1uL, 0))
        assertEquals("5.", listMarker(3uL, 2))
    }
}
