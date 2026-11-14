// A message's words as the phone draws them (issue #18): the core's blocks
// (`parseBody`; Body.kt makes a paragraph's styled text) as paragraphs,
// code blocks, quotes and lists, as the desktop draws them
// (apps/desktop/src/ui/body/mod.rs), a long body collapsed to its first
// lines with "Show more" under it, and the composer's counter. Tokens
// only; what the design system has no name for is made from the nearest.
package chat.lumia.kit

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.collapse
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.expand
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt
import kotlinx.coroutines.delay
import uniffi.lumia_ffi.BodyBlock
import uniffi.lumia_ffi.BodyInline
import uniffi.lumia_ffi.MentionSummary
import uniffi.lumia_ffi.MessageRoom
import uniffi.lumia_ffi.parseBody

/** Inline code beside the message text around it, as the desktop draws it. */
private const val CODE_SCALE = 0.9f

/** How much of a collapsed body fades into the row under it, in lines. */
private const val FADE_LINES = 1.5f

/** How long Copy reads "Copied". */
private const val COPIED_MS = 1500L

/**
 * A message's words: [body] read by the core into its blocks, its
 * [mentions] (the message's) drawn as the design's `lu-mention` pill, `@`
 * and the person's name as [nameOf] gives it, in the accent's ink on its
 * soft wash; inline code a chip on the inset ground; links in the link
 * ink, opened in the browser. An [edited] message's words are followed by
 * "(edited)", smaller and quieter, on its last line, or on a line of its
 * own when the body does not end in a paragraph.
 *
 * With [onExpanded], a body drawn taller than 14 lines is drawn at the
 * height of its first 10, its last fading into the row under it, with
 * "Show more" under it ([expanded]: all of it, and "Show less").
 */
@Composable
fun LumiaBody(
    body: String,
    modifier: Modifier = Modifier,
    mentions: List<MentionSummary> = emptyList(),
    nameOf: (String) -> String? = { null },
    edited: Boolean = false,
    expanded: Boolean = false,
    onExpanded: ((Boolean) -> Unit)? = null,
) {
    val blocks = remember(body, mentions) { parseBody(body, mentions) }
    val scale = LocalTextScale.current
    val look = InlineLook(
        emphasis = SpanStyle(fontStyle = FontStyle.Italic),
        strong = SpanStyle(fontWeight = FontWeight(LumiaTokens.textAuthor.weight)),
        strike = SpanStyle(textDecoration = TextDecoration.LineThrough),
        code = SpanStyle(fontFamily = FontFamily.Monospace, fontSize = LumiaTokens.textMessage.size.scaled(scale * CODE_SCALE)),
        mention = SpanStyle(color = LumiaTokens.accentInk.color, fontWeight = FontWeight(LumiaTokens.textSection.weight)),
        link = SpanStyle(color = LumiaTokens.link.color, textDecoration = TextDecoration.Underline),
        edited = SpanStyle(color = LumiaTokens.inkMuted.color, fontSize = LumiaTokens.textMeta.size.scaled(scale)),
    )
    // The names drawn, so a name that changes draws again.
    val names = mentions.map { if (it.everyone) null else nameOf(it.accountId) }
    val drawing = Drawing(look, mentions, names, nameOf)
    val inline = edited && editedInline(blocks)
    val words = @Composable {
        Column {
            Blocks(blocks, drawing, inItem = false, editedLast = inline)
            if (edited && !inline) {
                Text(
                    EDITED, style = LumiaTokens.textMeta.scaled, color = LumiaTokens.inkMuted.color,
                    modifier = Modifier.padding(top = LumiaTokens.space4).semantics { contentDescription = EDITED_LABEL },
                )
            }
        }
    }
    if (onExpanded == null) Box(modifier) { words() } else Collapsing(expanded, onExpanded, modifier, words)
}

/** What every paragraph of one body is drawn with. */
private class Drawing(val look: InlineLook, val mentions: List<MentionSummary>, val names: List<String?>, val nameOf: (String) -> String?)

/**
 * Blocks one under another, as the desktop sets them: paragraphs 4 apart,
 * what is not a paragraph 8 from what is around it, a list in an item 4
 * under the item's words.
 */
@Composable
private fun Blocks(blocks: List<BodyBlock>, drawing: Drawing, inItem: Boolean, editedLast: Boolean = false, modifier: Modifier = Modifier) {
    Column(modifier) {
        blocks.forEachIndexed { i, block ->
            val gap = when {
                i == 0 -> 0.dp
                block is BodyBlock.ListBlock && inItem -> LumiaTokens.space4
                block is BodyBlock.Para && blocks[i - 1] is BodyBlock.Para && !inItem -> LumiaTokens.space4
                else -> LumiaTokens.space8
            }
            Box(Modifier.padding(top = gap)) { Block(block, drawing, inItem, editedLast && i == blocks.lastIndex) }
        }
    }
}

@Composable
private fun Block(block: BodyBlock, drawing: Drawing, inItem: Boolean, edited: Boolean) {
    when (block) {
        is BodyBlock.Para -> Paragraph(block.inline, drawing, edited)
        is BodyBlock.Code -> CodeBlock(block.text, block.lang)
        is BodyBlock.Quote -> {
            // A rule at the leading edge, the quote's blocks beside it.
            val rule = LumiaTokens.lineStrong.color
            Blocks(
                block.blocks, drawing, inItem = false,
                modifier = Modifier
                    .drawBehind {
                        val w = LumiaTokens.space2.toPx()
                        drawRoundRect(rule, Offset.Zero, Size(w, size.height), CornerRadius(w))
                    }
                    .padding(start = LumiaTokens.space2 + LumiaTokens.space12),
            )
        }
        is BodyBlock.ListBlock -> Column {
            block.items.forEachIndexed { k, item ->
                Row(Modifier.padding(top = if (k == 0) 0.dp else LumiaTokens.space4)) {
                    // The marker is not read out: the item's words are.
                    Text(
                        listMarker(block.start, k), style = LumiaTokens.textMessage.scaled, color = LumiaTokens.inkSecondary.color, textAlign = TextAlign.End,
                        modifier = Modifier.widthIn(min = LumiaTokens.space28).padding(end = LumiaTokens.space8).clearAndSetSemantics {},
                    )
                    Blocks(item.blocks, drawing, inItem = true, modifier = Modifier.weight(1f))
                }
            }
        }
    }
}

/**
 * A paragraph: its text with its styles, its line breaks as typed, a word
 * too long for the row broken rather than run past it; its mentions'
 * pills (reaching 6 past the words and 1 over the line, radius 6) and its
 * code's chips drawn under the words. A screen reader has the words
 * without the thin spaces that pad them.
 */
@Composable
private fun Paragraph(inline: BodyInline, drawing: Drawing, edited: Boolean) {
    val look = drawing.look
    val styled = remember(inline, look, drawing.names, edited) { styledInline(inline, look, drawing.mentions, { m, typed -> mentionWords(m, typed, drawing.nameOf) }, edited) }
    val wash = LumiaTokens.accentSoft.color
    val ground = LumiaTokens.inset.color
    var layout by remember { mutableStateOf<TextLayoutResult?>(null) }
    val padded = styled.pills.isNotEmpty() || styled.chips.isNotEmpty()
    Text(
        styled.text,
        style = LumiaTokens.textMessage.scaled,
        color = LumiaTokens.inkBody.color,
        onTextLayout = { layout = it },
        modifier = Modifier
            .then(if (padded || edited) Modifier.semantics { contentDescription = styled.words } else Modifier)
            .then(
                if (!padded) Modifier else Modifier.drawBehind {
                    val l = layout ?: return@drawBehind
                    // A chip's thin spaces are its room; a pill reaches 6 past its words, the thin space part of that.
                    washes(l, styled.chips, ground, LumiaTokens.radius5.toPx(), reach = 0f, over = 0f)
                    washes(l, styled.pills, wash, LumiaTokens.radius6.toPx(), reach = LumiaTokens.space6.toPx(), over = LumiaTokens.space2.toPx() / 2)
                },
            ),
    )
}

/** A rounded wash under each of [ranges] of the text laid out as [l], line by line where one wraps. */
private fun DrawScope.washes(l: TextLayoutResult, ranges: List<IntRange>, color: Color, radius: Float, reach: Float, over: Float) {
    val corner = CornerRadius(radius)
    for (r in ranges) {
        val start = r.first
        val end = r.last + 1
        if (end > l.layoutInput.text.length) continue
        val thin = l.getHorizontalPosition(start + 1, true) - l.getHorizontalPosition(start, true)
        val past = (reach - thin).coerceAtLeast(0f)
        val first = l.getLineForOffset(start)
        val last = l.getLineForOffset(end - 1)
        for (line in first..last) {
            val left = if (line == first) l.getHorizontalPosition(start, true) - past else l.getLineLeft(line)
            val right = if (line == last) l.getHorizontalPosition(end, true).takeIf { it > left } ?: l.getLineRight(line) else l.getLineRight(line)
            val wide = if (line == last) right + past else right
            drawRoundRect(color, Offset(left, l.getLineTop(line) - over), Size(wide - left, l.getLineBottom(line) - l.getLineTop(line) + 2 * over), corner)
        }
    }
}

/**
 * A code block: its lines in the mono face on the inset ground, as the
 * desktop draws one (padding 12 by 10, radius 12, the line's border). A
 * line too long for the row does not wrap: the block scrolls sideways
 * inside itself, and is as tall as its lines. On a phone Copy is always
 * shown, so it sits in a strip of its own over the code, with the
 * language the fence names, and never covers a line.
 */
@Composable
private fun CodeBlock(text: String, lang: String?) {
    val shape = RoundedCornerShape(LumiaTokens.radius12)
    val rule = LumiaTokens.line.color
    Column(
        Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(LumiaTokens.inset.color, shape)
            .border(1.dp, LumiaTokens.line.color, shape),
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = CODE_STRIP)
                .drawBehind {
                    drawLine(rule, Offset(0f, size.height - 0.5f), Offset(size.width, size.height - 0.5f), 1.dp.toPx())
                }
                .padding(start = LumiaTokens.space12, end = LumiaTokens.space4),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                lang.orEmpty(),
                style = LumiaTokens.textTag.style.copy(fontFamily = FontFamily.Monospace, fontWeight = FontWeight(LumiaTokens.textMessage.weight)),
                color = LumiaTokens.inkMuted.color,
                maxLines = 1,
                modifier = Modifier.weight(1f),
            )
            CopyButton(text)
        }
        Text(
            text,
            // The mono face a step under the message text, as the desktop's `TEXT_CODE`.
            style = LumiaTokens.textLinkSm.scaled.copy(fontFamily = FontFamily.Monospace, fontWeight = FontWeight(LumiaTokens.textMessage.weight)),
            color = LumiaTokens.inkBody.color,
            softWrap = false,
            modifier = Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState())
                .padding(horizontal = LumiaTokens.space12, vertical = LumiaTokens.space10),
        )
    }
}

/** The height of a code block's strip over its lines: its language and Copy. */
private val CODE_STRIP = 36.dp

/**
 * "Copy": the block's text to the clipboard, then "Copied" for a moment,
 * which a screen reader is told; "Copy code" to it. A text button in the
 * code block's strip, as tall as the strip and at least 44 wide.
 */
@Composable
private fun CopyButton(text: String) {
    val clipboard = LocalClipboardManager.current
    val view = LocalView.current
    var copied by remember { mutableStateOf(false) }
    LaunchedEffect(copied) {
        if (!copied) return@LaunchedEffect
        delay(COPIED_MS)
        copied = false
    }
    val copy = {
        clipboard.setText(AnnotatedString(text))
        copied = true
        view.announceForAccessibility(COPIED)
    }
    val shape = RoundedCornerShape(LumiaTokens.radius6)
    Text(
        if (copied) COPIED else COPY,
        style = LumiaTokens.textTag.style,
        color = LumiaTokens.inkSecondary.color,
        textAlign = TextAlign.Center,
        modifier = Modifier
            .clip(shape)
            .clearAndSetSemantics {
                contentDescription = COPY_CODE
                role = Role.Button
                onClick { copy(); true }
            }
            .clickable(role = Role.Button, onClick = copy)
            .widthIn(min = LumiaTokens.tileSize)
            .padding(horizontal = LumiaTokens.space10, vertical = LumiaTokens.space8),
    )
}

/**
 * A body that may collapse: measured at its full height, in lines of the
 * message text at the width it has; over 14, it is drawn at the height of
 * its first 10, the last line and a half fading out (into whatever the
 * row is drawn on: the wash of a message for this account too), and
 * "Show more" under it; expanded, all of it and "Show less".
 */
@Composable
private fun Collapsing(expanded: Boolean, onExpanded: (Boolean) -> Unit, modifier: Modifier, content: @Composable () -> Unit) {
    val style = LumiaTokens.textMessage.scaled
    val measurer = rememberTextMeasurer()
    val line = remember(style, measurer) { measurer.measure("A", style).size.height }
    // Where the words are cut, 0 for nowhere: set as they are measured and read as they are
    // drawn, so a long body has its fade and its button in the frame it first appears in.
    val cut = remember { mutableFloatStateOf(0f) }
    Layout(
        contents = listOf(
            {
                Column(
                    // Drawn apart, so the fade takes the words away and leaves what is under them.
                    Modifier
                        .graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
                        .drawWithContent {
                            drawContent()
                            val at = cut.floatValue
                            if (at > 0f) {
                                // A mask, not a colour drawn: opaque keeps the words, clear takes
                                // them away, everything under the cut too.
                                drawRect(
                                    Brush.verticalGradient(listOf(Color.Black, Color.Transparent), startY = at - line * FADE_LINES, endY = at),
                                    blendMode = BlendMode.DstIn,
                                )
                            }
                        },
                ) { content() }
            },
            { ShowMore(expanded, onExpanded) },
        ),
        modifier = modifier.clipToBounds(),
    ) { (words, more), constraints ->
        val body = words.first().measure(constraints.copy(minHeight = 0, maxHeight = Constraints.Infinity))
        // Lines to the nearest, as the other clients count them.
        val lines = if (line > 0) (body.height.toFloat() / line).roundToInt() else 0
        val collapses = collapses(lines)
        val shown = if (collapses && !expanded) minOf(body.height, COLLAPSED_LINES * line) else body.height
        cut.floatValue = if (shown < body.height) shown.toFloat() else 0f
        val button = if (collapses) more.first().measure(constraints.copy(minWidth = 0, minHeight = 0)) else null
        val height = shown + (button?.height ?: 0)
        layout(body.width.coerceIn(constraints.minWidth, constraints.maxWidth), height.coerceAtLeast(constraints.minHeight)) {
            body.placeRelative(0, 0)
            button?.placeRelative(0, shown)
        }
    }
}

/** "Show more" or "Show less": a small link-style button in the accent's ink, collapsed or expanded to a screen reader. */
@Composable
private fun ShowMore(expanded: Boolean, onExpanded: (Boolean) -> Unit) {
    Text(
        showWords(expanded),
        style = LumiaTokens.textLinkSm.scaled,
        color = LumiaTokens.accentInk.color,
        modifier = Modifier
            .padding(top = LumiaTokens.space2)
            .clip(RoundedCornerShape(LumiaTokens.radius6))
            .clickable(role = Role.Button) { onExpanded(!expanded) }
            .semantics {
                stateDescription = if (expanded) "Expanded" else "Collapsed"
                if (expanded) collapse { onExpanded(false); true } else expand { onExpanded(true); true }
            }
            .padding(vertical = LumiaTokens.space4),
    )
}

/**
 * The composer's counter, from the core's room for its draft: shown from
 * 90% of the limit (null before), small and muted at its trailing edge,
 * "1,204 characters left", and over it "12 characters too long" in the
 * danger ink. A screen reader is told once as the draft goes over, not at
 * every key.
 */
@Composable
fun LumiaCounter(room: MessageRoom?, modifier: Modifier = Modifier) {
    val view = LocalView.current
    val over = isOver(room)
    val words = room?.let { roomWords(it.left) }
    LaunchedEffect(over) { if (over && words != null) view.announceForAccessibility(words) }
    if (words == null) return
    Text(
        words,
        style = LumiaTokens.textMeta.style,
        color = if (over) LumiaTokens.danger.color else LumiaTokens.inkMuted.color,
        textAlign = TextAlign.End,
        modifier = modifier.fillMaxWidth(),
    )
}
