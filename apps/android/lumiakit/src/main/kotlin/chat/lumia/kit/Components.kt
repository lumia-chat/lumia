// The small components the layout builds from, after the design system's
// (apps/web/src/design/components.tsx) and the Swift ones: avatar, notice,
// empty state, message row, request card, device row, composer and the
// line under it that says who is typing, the auth card and its fields,
// buttons. Tokens only; no literal colours or sizes.
package chat.lumia.kit

import android.provider.Settings
import androidx.compose.animation.core.EaseInOut
import androidx.compose.animation.core.StartOffset
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.keyframes
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.AlignmentLine
import androidx.compose.ui.layout.FirstBaseline
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.Placeable
import androidx.compose.ui.layout.layoutId
import androidx.compose.ui.semantics.onLongClick
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import uniffi.lumia_ffi.DeviceSummary

@Composable
fun LumiaAvatar(initials: String, tone: AvatarTone, size: Dp = LumiaTokens.avatarMd) {
    Box(
        modifier = Modifier.size(size).background(tone.fill.color, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Text(initials, style = TextStyle(fontSize = (size.value * 0.4f).sp, fontWeight = androidx.compose.ui.text.font.FontWeight.SemiBold), color = tone.ink.color)
    }
}

private val Float.sp get() = androidx.compose.ui.unit.TextUnit(this, androidx.compose.ui.unit.TextUnitType.Sp)

enum class NoticeTone { NEUTRAL, ACCENT }

@Composable
fun LumiaNotice(
    icon: ImageVector,
    tone: NoticeTone = NoticeTone.NEUTRAL,
    action: Pair<String, () -> Unit>? = null,
    text: String,
) {
    val accent = tone == NoticeTone.ACCENT
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(if (accent) LumiaTokens.accentWash.color else LumiaTokens.inset.color, RoundedCornerShape(LumiaTokens.radius10))
            .border(1.dp, if (accent) LumiaTokens.accentLine.color else LumiaTokens.line.color, RoundedCornerShape(LumiaTokens.radius10))
            .padding(LumiaTokens.space12),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space10),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, contentDescription = null, tint = if (accent) LumiaTokens.accentInk.color else LumiaTokens.inkSecondary.color, modifier = Modifier.size(18.dp))
        Text(text, style = LumiaTokens.textBody.style, color = LumiaTokens.inkBody.color, modifier = Modifier.weight(1f))
        if (action != null) LumiaSecondaryButton(action.first, onClick = action.second)
    }
}

@Composable
fun LumiaEmptyState(icon: ImageVector, title: String, text: String? = null, action: Pair<String, () -> Unit>? = null) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(vertical = LumiaTokens.space40),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
    ) {
        Icon(icon, contentDescription = null, tint = LumiaTokens.inkQuiet.color, modifier = Modifier.size(28.dp))
        Text(title, style = LumiaTokens.titleChannel.style, color = LumiaTokens.ink.color)
        if (text != null) Text(text, style = LumiaTokens.textBody.style, color = LumiaTokens.inkSecondary.color, textAlign = TextAlign.Center)
        if (action != null) LumiaPrimaryButton(action.first, onClick = action.second)
    }
}

// What the phone boards draw and `tokens.json` has no name for, made from
// the nearest tokens (the desktop shell's `tokens.rs` does the same). A
// message row's own are `Measures` (Preferences.kt), which the layout,
// spacing and avatars chosen in Preferences decide.

/** Under the line that says who is typing, to the panel's foot (20). */
private val TypingFoot = LumiaTokens.space16 + LumiaTokens.space4
/** Between two dots of the typing line (3). */
private val TypingDotGap = (LumiaTokens.space2 + LumiaTokens.space4) / 2

/**
 * A message's row. Grouped, messages one person sends close together are
 * one block (`joins` says what that is, [place] where this one sits in its
 * block): the first is an avatar beside the name, the time and the
 * message; the ones after it are the message alone, in the same column, a
 * paragraph's distance under the one before. A phone has no pointer to
 * show such a row's time under, so it is not drawn; a screen reader still
 * hears whose the message is and when, before it. With the avatars hidden
 * no room is kept for one: the name starts where it was, and opens the
 * profile card as it did. In one line ([Measures.oneLine]) every message
 * is the time, the name and its words, which wrap under their own start
 * (`OneLineRow`). Each row is one stop for a screen reader. [measures] are
 * Preferences': the avatar and the room around the row; its text is drawn
 * at the size Preferences chose (`LocalTextScale`).
 *
 * The row is the boards' padded box, rounded, inside the list's own
 * margin: washed in the accent when it is for this account ([washed]: it
 * mentions it or everyone, or replies to it) and in the soft accent for
 * the moment a jump reaches it ([flashed]).
 */
@Composable
fun LumiaMessageRow(
    author: String,
    tone: AvatarTone,
    time: String,
    text: String,
    deleted: Boolean = false,
    place: BlockPlace = BlockPlace.Alone,
    measures: Measures = Measures.Comfortable,
    onAuthor: (() -> Unit)? = null,
    // What architecture 6.6 adds: the reply's reference line over the
    // avatar's row, where the body's mentions are (drawn as pills), the
    // "(edited)" after its words, the reactions under them, the field it
    // is edited in where its words were, a line under it (an edit
    // refused), the wash of a message for this account or that a jump
    // just reached, and the long press that opens its actions.
    reference: (@Composable () -> Unit)? = null,
    mentions: List<IntRange> = emptyList(),
    edited: Boolean = false,
    reactions: (@Composable () -> Unit)? = null,
    editor: (@Composable () -> Unit)? = null,
    under: (@Composable () -> Unit)? = null,
    washed: Boolean = false,
    flashed: Boolean = false,
    onLongPress: (() -> Unit)? = null,
) {
    // Who wrote it, tapped on the avatar or the name: their profile card, where a screen offers one.
    val authorTap = if (onAuthor != null) Modifier.clickable(onClickLabel = "view $author's profile", role = Role.Button, onClick = onAuthor) else Modifier
    val body = @Composable { modifier: Modifier ->
        Column(modifier, verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
            when {
                editor != null -> editor()
                deleted -> Text("(deleted)", style = LumiaTokens.textMessage.scaled, color = LumiaTokens.inkQuiet.color)
                else -> LumiaBody(text, mentions, edited = edited)
            }
            if (reactions != null && editor == null) Box(Modifier.padding(top = LumiaTokens.space2)) { reactions() }
            under?.invoke()
        }
    }
    val editing = editor != null
    val shape = RoundedCornerShape(LumiaTokens.radius12)
    val fill = when {
        flashed -> LumiaTokens.accentSoft.color
        washed -> LumiaTokens.accentWash.color
        else -> null
    }
    // In one line, every message is a line of its own, with the room between lines and none for a block.
    val opens = place.opens && !measures.oneLine
    val frame = Modifier
        .fillMaxWidth()
        // The list's margin, and the room between blocks outside the row's box.
        .padding(horizontal = LumiaTokens.space6)
        .padding(top = if (opens) measures.blockGap else 0.dp)
        .then(if (fill != null) Modifier.background(fill, shape) else Modifier)
        .then(
            if (onLongPress != null && !editing) {
                Modifier.pointerInput(onLongPress) { detectTapGestures(onLongPress = { onLongPress() }) }
            } else Modifier,
        )
        // One stop for a screen reader, but for a row being edited, whose field is a stop of its own.
        .semantics(mergeDescendants = !editing) {
            if (onLongPress != null && !editing) onLongClick(label = "Message actions") { onLongPress(); true }
        }
        .padding(
            start = LumiaTokens.space10,
            end = LumiaTokens.space10,
            top = when {
                measures.oneLine -> measures.line / 2
                place.opens -> measures.edge
                else -> measures.within / 2
            },
            bottom = when {
                measures.oneLine -> measures.line / 2
                place.closes -> measures.edge
                else -> measures.within / 2
            },
        )
    if (measures.oneLine || !place.opens) {
        Box(frame) {
            // Who and when, before the words: the line's time and name are not read out themselves.
            Box(Modifier.matchParentSize().semantics { contentDescription = "$author, $time" })
            if (measures.oneLine) OneLineRow(author, time, authorTap, onAuthor != null, reference) { body(Modifier) }
            else body(Modifier.padding(start = measures.gutter))
        }
        return
    }
    val head = @Composable { modifier: Modifier ->
        Column(verticalArrangement = Arrangement.spacedBy(measures.within), modifier = modifier) {
            Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8), verticalAlignment = Alignment.Bottom) {
                Text(author, style = LumiaTokens.textAuthor.scaled, color = LumiaTokens.inkEmphasis.color, modifier = authorTap)
                Text(time, style = LumiaTokens.textMeta.scaled, color = LumiaTokens.inkQuiet.color)
            }
            body(Modifier)
        }
    }
    // A reply: its reference line over the avatar's row, the boards' 2 apart.
    Column(frame, verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
        reference?.invoke()
        val avatar = measures.avatar
        if (avatar == null) {
            // No avatar and no room kept for one: the name, which opens the profile card, starts at the row's edge.
            head(Modifier)
        } else {
            Row(horizontalArrangement = Arrangement.spacedBy(measures.beside), verticalAlignment = Alignment.Top) {
                // The name beside it says who; the initials would only be read out before it.
                Box(Modifier.clip(CircleShape).then(authorTap).clearAndSetSemantics {}) { LumiaAvatar(initialsOf(author), tone, avatar) }
                head(Modifier.weight(1f))
            }
        }
    }
}

/**
 * A message as one line: its time and its author's name, then its words,
 * which wrap under their own start, and its reactions under them. A
 * reply's short reference line ([reference]) is over the words, at their
 * start. The time and the name sit on the words' first line, the three
 * sharing a baseline; a long name is cut short at a quarter of the line,
 * so the words keep at least half of it on a phone. The name opens the profile card where [tappable]
 * ([authorTap]), and is a stop of its own then, as a grouped row's is;
 * else the row's description says it, and neither it nor the time is read
 * out twice.
 */
@Composable
private fun OneLineRow(author: String, time: String, authorTap: Modifier, tappable: Boolean, reference: (@Composable () -> Unit)?, words: @Composable () -> Unit) {
    Layout(
        content = {
            Text(time, style = LumiaTokens.textMeta.scaled, color = LumiaTokens.inkQuiet.color, maxLines = 1, softWrap = false, modifier = Modifier.layoutId(LineTime).clearAndSetSemantics {})
            Text(
                author, style = LumiaTokens.textAuthor.scaled, color = LumiaTokens.inkEmphasis.color, maxLines = 1, overflow = TextOverflow.Ellipsis,
                modifier = Modifier.layoutId(LineName).then(if (tappable) authorTap else Modifier.clearAndSetSemantics {}),
            )
            if (reference != null) Box(Modifier.layoutId(LineReference)) { reference() }
            Box(Modifier.layoutId(LineWords)) { words() }
        },
    ) { measurables, constraints ->
        val gap = LumiaTokens.space8.roundToPx()
        val loose = constraints.copy(minWidth = 0, minHeight = 0)
        val width = constraints.maxWidth
        val timeP = measurables.first { it.layoutId == LineTime }.measure(loose)
        val nameP = measurables.first { it.layoutId == LineName }.measure(loose.copy(maxWidth = width / 4))
        val start = (timeP.width + gap + nameP.width + gap).coerceAtMost(width / 2)
        val rest = loose.copy(maxWidth = (width - start).coerceAtLeast(0))
        val refP = measurables.firstOrNull { it.layoutId == LineReference }?.measure(rest)
        val wordsP = measurables.first { it.layoutId == LineWords }.measure(rest)
        val top = refP?.let { it.height + LumiaTokens.space2.roundToPx() } ?: 0
        fun baseline(p: Placeable, otherwise: Int) = p[FirstBaseline].takeIf { it != AlignmentLine.Unspecified } ?: otherwise
        val nameBase = baseline(nameP, nameP.height)
        val timeBase = baseline(timeP, nameBase)
        // Words with no baseline (the field an edit is made in) start level with the name.
        val wordsBase = baseline(wordsP, nameBase)
        val line = top + maxOf(timeBase, nameBase, wordsBase)
        val height = maxOf(line - timeBase + timeP.height, line - nameBase + nameP.height, line - wordsBase + wordsP.height)
        layout(width, height) {
            refP?.placeRelative(start, 0)
            timeP.placeRelative(0, line - timeBase)
            nameP.placeRelative(timeP.width + gap, line - nameBase)
            wordsP.placeRelative(start, line - wordsBase)
        }
    }
}

private const val LineTime = "time"
private const val LineName = "name"
private const val LineReference = "reference"
private const val LineWords = "words"

/**
 * A message's words, its mentions (by [mentions], where they are in the
 * text) drawn as the design's `lu-mention` pill: the text, `@` and the
 * person's name, in the accent's ink and the strong weight, on the
 * accent's soft wash, reaching 6 past the words and 1 over the line, with
 * the radius 6. The pill's room beside the words is a thin space each side
 * of them, inside the pill, so it does not run over its neighbours; a
 * screen reader has the text without them. An [edited] message's words are
 * followed by "(edited)", smaller and quieter, on its last line.
 */
@Composable
fun LumiaBody(text: String, mentions: List<IntRange>, modifier: Modifier = Modifier, edited: Boolean = false) {
    val ink = LumiaTokens.accentInk.color
    val wash = LumiaTokens.accentSoft.color
    val quiet = LumiaTokens.inkMuted.color
    val scale = LocalTextScale.current
    // The text drawn, thin spaces and all, and where each pill is in it.
    val (styled, pills) = remember(text, mentions, ink, quiet, edited, scale) {
        val pills = mutableListOf<IntRange>()
        val styled = buildAnnotatedString {
            var at = 0
            for (r in mentions.sortedBy { it.first }) {
                if (r.first < at || r.last >= text.length) continue
                append(text, at, r.first)
                val start = length
                append(THIN_SPACE)
                pushStyle(SpanStyle(color = ink, fontWeight = FontWeight(LumiaTokens.textSection.weight)))
                append(text, r.first, r.last + 1)
                pop()
                append(THIN_SPACE)
                pills.add(start until length)
                at = r.last + 1
            }
            append(text, at, text.length)
            if (edited) {
                append(" ")
                pushStyle(SpanStyle(color = quiet, fontSize = LumiaTokens.textMeta.size.scaled(scale)))
                append(EDITED)
                pop()
            }
        }
        styled to pills.toList()
    }
    var layout by remember { mutableStateOf<TextLayoutResult?>(null) }
    Text(
        styled,
        style = LumiaTokens.textMessage.scaled,
        color = LumiaTokens.inkBody.color,
        onTextLayout = { layout = it },
        modifier = modifier
            .then(if (pills.isNotEmpty() || edited) Modifier.semantics { contentDescription = if (edited) "$text, $EDITED_LABEL" else text } else Modifier)
            .then(
                if (pills.isEmpty()) Modifier else Modifier.drawBehind {
                    val l = layout ?: return@drawBehind
                    val corner = CornerRadius(LumiaTokens.radius6.toPx())
                    val over = LumiaTokens.space2.toPx() / 2
                    for (r in pills) {
                        val start = r.first
                        val end = r.last + 1
                        if (end > l.layoutInput.text.length) continue
                        // The thin space is part of the 6 the pill reaches past the words.
                        val thin = l.getHorizontalPosition(start + 1, true) - l.getHorizontalPosition(start, true)
                        val reach = (LumiaTokens.space6.toPx() - thin).coerceAtLeast(0f)
                        val first = l.getLineForOffset(start)
                        val last = l.getLineForOffset(end - 1)
                        for (line in first..last) {
                            val left = if (line == first) l.getHorizontalPosition(start, true) - reach else l.getLineLeft(line)
                            val right = if (line == last) l.getHorizontalPosition(end, true).takeIf { it > left } ?: l.getLineRight(line) else l.getLineRight(line)
                            val end2 = if (line == last) right + reach else right
                            drawRoundRect(wash, Offset(left, l.getLineTop(line) - over), Size(end2 - left, l.getLineBottom(line) - l.getLineTop(line) + 2 * over), corner)
                        }
                    }
                },
            ),
    )
}

/** The room inside a mention's pill, each side of its words. */
private const val THIN_SPACE = " "

/**
 * The unread divider, above the first message after where this account
 * had read to: a rule in the accent's line, then "New" in the accent's
 * ink, and nothing after the word. "New messages" to a screen reader.
 */
@Composable
fun LumiaNewMessagesLine(measures: Measures = Measures.Comfortable) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            // The boards' 6 over and under it, three quarters of a day's divider's, halved in Compact.
            .padding(horizontal = LumiaTokens.space16, vertical = measures.divider * 3 / 4)
            .clearAndSetSemantics { contentDescription = NEW_LABEL },
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.weight(1f).height(1.dp).background(LumiaTokens.accentLine.color))
        Text(NEW_WORD, style = LumiaTokens.textSection.scaled.copy(fontWeight = FontWeight(LumiaTokens.textCount.weight)), color = LumiaTokens.accentInk.color)
    }
}

/**
 * A notice the instance posted in a channel (architecture 6.8): "Ada added
 * Grace", one quiet line in the message text's column, with no avatar and
 * nothing to do with it, and the time after it. In one line, the time and
 * then the words, where a message's time is. One stop for a screen reader,
 * in the order they are drawn.
 */
@Composable
fun LumiaNoticeRow(text: String, time: String, measures: Measures = Measures.Comfortable) {
    val words = @Composable { modifier: Modifier ->
        Text(text, style = LumiaTokens.textMeta.scaled, color = LumiaTokens.inkMuted.color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = modifier)
    }
    val at = @Composable { Text(time, style = LumiaTokens.textMeta.scaled, color = LumiaTokens.inkQuiet.color) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .semantics(mergeDescendants = true) {}
            .then(
                if (measures.oneLine) Modifier.padding(horizontal = LumiaTokens.space16, vertical = measures.line / 2)
                else Modifier.padding(start = LumiaTokens.space16 + measures.gutter, end = LumiaTokens.space16, top = measures.blockGap + measures.within, bottom = measures.within),
            ),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
        verticalAlignment = Alignment.Bottom,
    ) {
        if (measures.oneLine) {
            at()
            words(Modifier.weight(1f, fill = false))
        } else {
            words(Modifier.weight(1f, fill = false))
            at()
        }
    }
}

@Composable
fun LumiaDayDivider(label: String, measures: Measures = Measures.Comfortable) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = LumiaTokens.space16, vertical = measures.divider),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space10),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.weight(1f).height(1.dp).background(LumiaTokens.line.color))
        Text(label, style = LumiaTokens.textSection.scaled, color = LumiaTokens.inkQuiet.color)
        Box(Modifier.weight(1f).height(1.dp).background(LumiaTokens.line.color))
    }
}

/** The composer. It has no room of its own underneath: [LumiaTypingLine] goes under it and has the room to the panel's foot. */
@Composable
fun LumiaComposer(text: String, onText: (String) -> Unit, placeholder: String, enabled: Boolean = true, onSend: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = LumiaTokens.space16)
            .background(LumiaTokens.field.color, RoundedCornerShape(LumiaTokens.radius12))
            .border(1.dp, LumiaTokens.borderComposer.color, RoundedCornerShape(LumiaTokens.radius12))
            .padding(LumiaTokens.space10),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
        verticalAlignment = Alignment.Bottom,
    ) {
        LumiaInput(text, onText, placeholder, enabled = enabled, modifier = Modifier.weight(1f).heightIn(min = 24.dp), maxLines = 8)
        LumiaPrimaryButton("Send", enabled = enabled && text.isNotBlank(), onClick = onSend)
    }
}

/**
 * The composer with where the caret is ([value]'s selection), for what
 * follows it: the @ list completes the `@word` at the caret, and a reply
 * puts the caret here ([focus]). A [header] is drawn inside its frame over
 * the field (the reply bar), and the frame's border is the active one
 * while it is ([active]). [onKey] sees a hardware keyboard's keys first.
 */
@Composable
fun LumiaComposer(
    value: TextFieldValue,
    onValue: (TextFieldValue) -> Unit,
    placeholder: String,
    enabled: Boolean = true,
    focus: FocusRequester? = null,
    header: (@Composable () -> Unit)? = null,
    active: Boolean = false,
    onKey: ((KeyEvent) -> Boolean)? = null,
    onSend: () -> Unit,
) {
    val shape = RoundedCornerShape(LumiaTokens.radius16)
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = LumiaTokens.space16)
            .clip(shape)
            .background(LumiaTokens.field.color, shape)
            .border(1.dp, if (active) LumiaTokens.borderActive.color else LumiaTokens.borderComposer.color, shape),
    ) {
        header?.invoke()
        Row(
            modifier = Modifier.fillMaxWidth().padding(LumiaTokens.space10),
            horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
            verticalAlignment = Alignment.Bottom,
        ) {
            BasicTextField(
                value = value,
                onValueChange = onValue,
                modifier = Modifier.weight(1f).heightIn(min = 24.dp)
                    .then(if (focus != null) Modifier.focusRequester(focus) else Modifier)
                    .then(if (onKey != null) Modifier.onPreviewKeyEvent(onKey) else Modifier),
                enabled = enabled,
                // At the size Preferences chose for messages, as what it sends will be.
                textStyle = LumiaTokens.textBody.scaled.copy(color = LumiaTokens.inkField.color),
                cursorBrush = SolidColor(LumiaTokens.accent.color),
                maxLines = 8,
                decorationBox = { inner ->
                    Box {
                        if (value.text.isEmpty()) Text(placeholder, style = LumiaTokens.textBody.scaled, color = LumiaTokens.inkMuted.color)
                        inner()
                    }
                },
            )
            LumiaPrimaryButton("Send", enabled = enabled && value.text.isNotBlank(), onClick = onSend)
        }
    }
}

/**
 * The line under the composer, as the phone boards draw it: who is typing
 * in a server's channel (protocol 6.7), three dots and the words, the
 * names in stronger ink. It keeps its height when nobody is, so the
 * composer and the messages over it stay where they are as people start
 * and stop. [names] are whoever began first at the front; a personal
 * conversation passes none (`LumiaSession.typed` says why).
 *
 * The boards end the line at the panel's foot, where the system's bar
 * lies over the screen: [systemFoot] is how much of the foot the system
 * covers, and the room under the words is that or the line's own,
 * whichever is more, not one on top of the other.
 */
@Composable
fun LumiaTypingLine(names: List<String>, systemFoot: Dp) {
    val words = typingWords(names)
    val strong = SpanStyle(color = LumiaTokens.inkEmphasis.color, fontWeight = FontWeight(LumiaTokens.textSection.weight))
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = LumiaTokens.space16)
            .padding(start = LumiaTokens.space8, top = LumiaTokens.space8, end = LumiaTokens.space8, bottom = maxOf(TypingFoot, systemFoot)),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (names.isNotEmpty()) TypingDots()
        Text(
            buildAnnotatedString {
                append(words.text)
                addStyle(strong, 0, words.named)
            },
            style = LumiaTokens.textMeta.style,
            color = LumiaTokens.inkMuted.color,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
    }
}

/** A typing dot at its faintest; it is at full strength two fifths of the way through its cycle and faint again at four fifths. */
private const val TYPING_DOT_DIM = 0.3f
/** One cycle of a typing dot, and how far each dot is behind the one before. */
private const val TYPING_DOT_PERIOD_MS = 1200
private const val TYPING_DOT_LAG_MS = 150

/**
 * The typing line's three dots, each brightening and fading in turn, the
 * second and third a little behind the one before. Still and at full
 * strength when the system's animations are off. Nothing a screen reader
 * is told: the words beside them say it.
 */
@Composable
private fun TypingDots() {
    val resolver = LocalContext.current.contentResolver
    val still = remember { Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f }
    val color = LumiaTokens.inkMuted.color
    Row(Modifier.clearAndSetSemantics {}, horizontalArrangement = Arrangement.spacedBy(TypingDotGap)) {
        if (still) {
            repeat(3) { Box(Modifier.size(LumiaTokens.space4).background(color, CircleShape)) }
        } else {
            val cycle = rememberInfiniteTransition(label = "typing")
            repeat(3) { n ->
                val alpha by cycle.animateFloat(
                    initialValue = TYPING_DOT_DIM,
                    targetValue = TYPING_DOT_DIM,
                    animationSpec = infiniteRepeatable(
                        keyframes {
                            durationMillis = TYPING_DOT_PERIOD_MS
                            TYPING_DOT_DIM at 0 using EaseInOut
                            1f at TYPING_DOT_PERIOD_MS * 2 / 5 using EaseInOut
                            TYPING_DOT_DIM at TYPING_DOT_PERIOD_MS * 4 / 5
                        },
                        initialStartOffset = StartOffset(n * TYPING_DOT_LAG_MS),
                    ),
                    label = "dot$n",
                )
                Box(Modifier.size(LumiaTokens.space4).graphicsLayer { this.alpha = alpha }.background(color, CircleShape))
            }
        }
    }
}

/** A text input with the design's placeholder, no decoration of its own. */
@Composable
fun LumiaInput(
    text: String,
    onText: (String) -> Unit,
    placeholder: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    secure: Boolean = false,
    maxLines: Int = 1,
    keyboard: KeyboardOptions = KeyboardOptions.Default,
) {
    BasicTextField(
        value = text,
        onValueChange = onText,
        modifier = modifier,
        enabled = enabled,
        textStyle = LumiaTokens.textBody.style.copy(color = LumiaTokens.inkField.color),
        cursorBrush = SolidColor(LumiaTokens.accent.color),
        maxLines = maxLines,
        singleLine = maxLines == 1,
        visualTransformation = if (secure) PasswordVisualTransformation() else VisualTransformation.None,
        keyboardOptions = if (secure) keyboard.copy(keyboardType = KeyboardType.Password) else keyboard,
        decorationBox = { inner ->
            Box {
                if (text.isEmpty()) Text(placeholder, style = LumiaTokens.textBody.style, color = LumiaTokens.inkMuted.color)
                inner()
            }
        },
    )
}

@Composable
fun LumiaField(
    label: String,
    text: String,
    onText: (String) -> Unit,
    placeholder: String = "",
    secure: Boolean = false,
    keyboard: KeyboardOptions = KeyboardOptions.Default,
) {
    Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
        Text(label, style = LumiaTokens.textLabel.style, color = LumiaTokens.inkSecondary.color)
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(LumiaTokens.controlHeight)
                .background(LumiaTokens.field.color, RoundedCornerShape(LumiaTokens.radius8))
                .border(1.dp, LumiaTokens.borderForm.color, RoundedCornerShape(LumiaTokens.radius8))
                .padding(horizontal = LumiaTokens.space10),
            contentAlignment = Alignment.CenterStart,
        ) {
            // The label above names the field to a screen reader too, read before what it holds.
            LumiaInput(text, onText, placeholder, secure = secure, keyboard = keyboard, modifier = Modifier.fillMaxWidth().semantics { contentDescription = label })
        }
    }
}

@Composable
fun LumiaRequestCard(
    name: String,
    handle: String?,
    tone: AvatarTone,
    meta: String,
    busy: Boolean,
    accept: () -> Unit,
    decline: () -> Unit,
    block: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(LumiaTokens.tile.color, RoundedCornerShape(LumiaTokens.radius14))
            .border(1.dp, LumiaTokens.line.color, RoundedCornerShape(LumiaTokens.radius14))
            .padding(LumiaTokens.space14),
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space10),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12), verticalAlignment = Alignment.CenterVertically) {
            LumiaAvatar(initialsOf(name), tone, LumiaTokens.avatarLg)
            Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
                Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space6), verticalAlignment = Alignment.Bottom) {
                    Text(name, style = LumiaTokens.textAuthor.style, color = LumiaTokens.inkEmphasis.color)
                    if (!handle.isNullOrEmpty()) Text("@$handle", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
                }
                Text(meta, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8), verticalAlignment = Alignment.CenterVertically) {
            LumiaPrimaryButton("Accept", enabled = !busy, onClick = accept)
            LumiaSecondaryButton("Decline", enabled = !busy, onClick = decline)
            Spacer(Modifier.weight(1f))
            LumiaLinkButton("Block", enabled = !busy, onClick = block)
        }
    }
}

@Composable
fun LumiaDeviceRow(device: DeviceSummary, label: String, meta: String, icon: ImageVector, revoke: (() -> Unit)?) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(LumiaTokens.tile.color, RoundedCornerShape(LumiaTokens.radius14))
            .border(1.dp, if (device.thisDevice) LumiaTokens.accent.color else LumiaTokens.line.color, RoundedCornerShape(LumiaTokens.radius14))
            .padding(horizontal = LumiaTokens.space14, vertical = LumiaTokens.space12),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, contentDescription = null, tint = LumiaTokens.iconStrong.color, modifier = Modifier.size(24.dp))
        Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2), modifier = Modifier.weight(1f)) {
            Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space6), verticalAlignment = Alignment.CenterVertically) {
                Text(label, style = LumiaTokens.textAuthor.style, color = if (device.revoked) LumiaTokens.inkQuiet.color else LumiaTokens.inkEmphasis.color)
                if (device.thisDevice) LumiaTag("This device")
                if (device.revoked) LumiaTag("Revoked")
            }
            if (meta.isNotEmpty()) Text(meta, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkQuiet.color)
        }
        if (revoke != null && !device.thisDevice && !device.revoked) LumiaSecondaryButton("Revoke", onClick = revoke)
    }
}

@Composable
fun LumiaTag(text: String) {
    Text(
        text,
        style = LumiaTokens.textTag.style,
        color = LumiaTokens.inkSecondary.color,
        modifier = Modifier
            .background(LumiaTokens.chip.color, RoundedCornerShape(LumiaTokens.radiusPill))
            .padding(horizontal = LumiaTokens.space6, vertical = LumiaTokens.space2),
    )
}

/**
 * A row of pills with one chosen: the design's segmented tabs at a form's
 * width. One of a few, as radio buttons are: each pill is a stop that says
 * its label, whether it is the one chosen, and where it is among the rest.
 */
@Composable
fun LumiaChoice(options: List<Pair<String, String>>, value: String, onChange: (String) -> Unit, modifier: Modifier = Modifier) {
    Row(
        modifier
            .fillMaxWidth()
            .background(LumiaTokens.chip.color, RoundedCornerShape(LumiaTokens.radiusPill))
            .padding(LumiaTokens.space2)
            .selectableGroup(),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space2),
    ) {
        for ((id, label) in options) {
            val chosen = id == value
            Text(
                label,
                style = LumiaTokens.textTag.style,
                color = if (chosen) LumiaTokens.ink.color else LumiaTokens.inkSecondary.color,
                textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                modifier = Modifier
                    .weight(1f)
                    .background(if (chosen) LumiaTokens.tabActive.color else androidx.compose.ui.graphics.Color.Transparent, RoundedCornerShape(LumiaTokens.radiusPill))
                    .selectable(selected = chosen, role = Role.RadioButton) { onChange(id) }
                    .padding(vertical = LumiaTokens.space8),
            )
        }
    }
}

/** The auth card: title, subtitle, a form; the width it is given. */
@Composable
fun LumiaCard(title: String, subtitle: String? = null, content: @Composable () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(LumiaTokens.surface.color, RoundedCornerShape(LumiaTokens.radius16))
            .border(1.dp, LumiaTokens.lineAuth.color, RoundedCornerShape(LumiaTokens.radius16))
            .padding(LumiaTokens.space24),
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space16),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
            Text(title, style = LumiaTokens.titleAuth.style, color = LumiaTokens.ink.color)
            if (subtitle != null) Text(subtitle, style = LumiaTokens.textSubtitle.style, color = LumiaTokens.inkSecondary.color)
        }
        content()
    }
}

@Composable
fun LumiaErrorText(text: String) {
    Text(text, style = LumiaTokens.textMeta.style, color = LumiaTokens.dnd.color, modifier = Modifier.fillMaxWidth())
}

// Buttons

@Composable
fun LumiaPrimaryButton(label: String, modifier: Modifier = Modifier, enabled: Boolean = true, onClick: () -> Unit) {
    Button(
        onClick = onClick,
        enabled = enabled,
        modifier = modifier.heightIn(min = LumiaTokens.controlHeight),
        shape = RoundedCornerShape(LumiaTokens.radius8),
        colors = ButtonDefaults.buttonColors(
            containerColor = LumiaTokens.accent.color,
            contentColor = LumiaTokens.onAccent.color,
            disabledContainerColor = LumiaTokens.accent.color.copy(alpha = 0.5f),
            disabledContentColor = LumiaTokens.onAccent.color,
        ),
    ) { Text(label, style = LumiaTokens.textButton.style) }
}

@Composable
fun LumiaSecondaryButton(label: String, modifier: Modifier = Modifier, enabled: Boolean = true, onClick: () -> Unit) {
    OutlinedButton(
        onClick = onClick,
        enabled = enabled,
        modifier = modifier.heightIn(min = LumiaTokens.controlHeight),
        shape = RoundedCornerShape(LumiaTokens.radius8),
        colors = ButtonDefaults.outlinedButtonColors(containerColor = LumiaTokens.raised.color, contentColor = LumiaTokens.ink.color),
        border = androidx.compose.foundation.BorderStroke(1.dp, LumiaTokens.border.color),
    ) { Text(label, style = LumiaTokens.textButtonSecondary.style) }
}

@Composable
fun LumiaLinkButton(label: String, modifier: Modifier = Modifier, enabled: Boolean = true, onClick: () -> Unit) {
    TextButton(onClick = onClick, enabled = enabled, modifier = modifier) {
        Text(label, style = LumiaTokens.textLinkSm.style, color = LumiaTokens.link.color)
    }
}

/** An icon-only button, 44 dp, the design's hit size. */
@Composable
fun LumiaIconButton(icon: ImageVector, label: String, onClick: () -> Unit) {
    androidx.compose.material3.IconButton(onClick = onClick, modifier = Modifier.size(44.dp)) {
        Icon(icon, contentDescription = label, tint = LumiaTokens.iconStrong.color)
    }
}

/** A section title in a list. */
@Composable
fun SectionTitle(text: String) {
    Text(
        text,
        style = LumiaTokens.textSection.style,
        color = LumiaTokens.inkMuted.color,
        modifier = Modifier.padding(start = LumiaTokens.space10, end = LumiaTokens.space10, top = LumiaTokens.space4, bottom = LumiaTokens.space6),
    )
}

/** One line, cut with an ellipsis. */
@Composable
fun OneLine(text: String, style: TextStyle, color: androidx.compose.ui.graphics.Color, modifier: Modifier = Modifier) {
    Text(text, style = style, color = color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = modifier.wrapContentHeight())
}

@Suppress("unused")
private val keep = Modifier.width(0.dp)
