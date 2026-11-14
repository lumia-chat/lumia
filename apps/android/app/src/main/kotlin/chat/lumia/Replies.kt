// What quote replies, reactions, mentions, editing and deleting add to a
// conversation on the phone (architecture 6.6), as the boards draw them
// (the "Inline replies" board and the phone boards) in Android's idiom: a
// reply's reference line over its row, its reactions as chips under its
// words, a long press opening its actions as a bottom sheet (Add reaction,
// Reply, Edit message, Copy text, Delete message), the emoji picker as a
// sheet, the field a message is edited in where its words were, the
// question before one is deleted, and in the composer's frame the reply
// bar with its "@ On" switch; over the composer, the @ list and the bar
// naming those a message mentioned who are not in its channel. The rules
// these draw from are `Replies.kt` in lumiakit.
//
// A message's text is not selectable on the phone (it never was: a plain
// Text, and the long press is the message's actions), so there is no
// "Quote reply" of a selection here; Reply quotes the whole message.
package chat.lumia

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Reply
import androidx.compose.material.icons.filled.AlternateEmail
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.outlined.AddReaction
import androidx.compose.material.icons.outlined.ContentCopy
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Edit
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.PlainTooltip
import androidx.compose.material3.Text
import androidx.compose.material3.TooltipBox
import androidx.compose.material3.TooltipDefaults
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.material3.rememberTooltipState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import chat.lumia.kit.AvatarTone
import chat.lumia.kit.DELETE_QUESTION
import chat.lumia.kit.EmojiSet
import chat.lumia.kit.LumiaAvatar
import chat.lumia.kit.LumiaCounter
import chat.lumia.kit.LumiaErrorText
import chat.lumia.kit.LumiaInput
import chat.lumia.kit.LumiaLinkButton
import chat.lumia.kit.LumiaPrimaryButton
import chat.lumia.kit.LumiaSecondaryButton
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.Measures
import chat.lumia.kit.MentionChoice
import chat.lumia.kit.ORIGINAL_DELETED
import chat.lumia.kit.initialsOf
import chat.lumia.kit.isOver
import chat.lumia.kit.mentionNote
import chat.lumia.kit.mentionSwitchLabel
import chat.lumia.kit.mentionSwitchWord
import chat.lumia.kit.nameOf
import chat.lumia.kit.reactedWords
import chat.lumia.kit.referenceName
import chat.lumia.kit.referenceWords
import chat.lumia.kit.replyingTo
import uniffi.lumia_ffi.MessageRoom
import uniffi.lumia_ffi.QuoteSummary
import uniffi.lumia_ffi.ReactionSummary

// What the boards draw and `tokens.json` has no name for, made from the
// nearest tokens, as `Components.kt` does.

/** The reference line's quoted avatar and its deleted mark (18). */
private val RefAvatar = LumiaTokens.space18
/** The connector from the reply's avatar: 26 by 12, nudged 8 down. */
private val ConnectorWidth = LumiaTokens.space24 + LumiaTokens.space2
private val ConnectorHeight = LumiaTokens.space12
/** The deleted mark's minus (10 of the 18). */
private val MinusGlyph = LumiaTokens.space10
/** The reply bar: its padding 7 8 7 14, its glyph 15 and the switch's 13, Cancel reply's 14. */
private val BarPadY = (LumiaTokens.space6 + LumiaTokens.space8) / 2
private val ReplyGlyph = (LumiaTokens.space14 + LumiaTokens.space16) / 2
private val SwitchGlyph = (LumiaTokens.space12 + LumiaTokens.space14) / 2
/** A reaction chip: 32 high, 12 at the sides, the emoji at 14. */
private val ChipHeight = LumiaTokens.space28 + LumiaTokens.space4

/** The reference line's 13: the small link's size, at the weight given, scaled as the row's text is. */
@Composable
private fun small(weight: Int) = LumiaTokens.textLinkSm.scaled.copy(fontWeight = FontWeight(weight))

// A reply's reference line

/**
 * Above a replying message's row (the "Inline replies" board): a connector
 * from under the reply's avatar's centre, the quoted person's small avatar
 * and name (`@` and this account's own name, in the accent, when it quoted
 * this account), and the quoted text on one line, cut short; one button
 * that finds the original ([onJump]; none on a Mentions page, whose item is
 * the button). A quote of a deleted message: the connector, a minus in a
 * circle and "Original message was deleted", and nothing to press. In a
 * personal conversation, where the author may not be known, the avatar and
 * the name are left out. [measures] are the row's, whose avatar the
 * connector starts under (with none, there is no connector); none on a Mentions
 * page, where it is in the message's column and starts at its edge. In one
 * line it is the short line over the words, at their start: `↳ Priya:
 * first words…` (`ShortReference`).
 */
@Composable
fun ReferenceLine(session: LumiaSession, quote: QuoteSummary, measures: Measures?, onJump: (() -> Unit)?) {
    // The connector starts under the avatar's centre: half the avatar, less half its stroke.
    val indent = measures?.avatar?.let { it / 2 - LumiaTokens.space2 / 2 } ?: 0.dp
    val known = quote.authorAccountId.isNotEmpty()
    val toMe = known && quote.authorAccountId == session.me
    val name = if (known) session.nameOf(quote.authorAccountId) else null
    val button = if (quote.originalDeleted || onJump == null) {
        Modifier.clearAndSetSemantics { contentDescription = if (quote.originalDeleted) ORIGINAL_DELETED else referenceWords(name, toMe, quote.text, jump = false) }
    } else {
        Modifier.clearAndSetSemantics {
            contentDescription = referenceWords(name, toMe, quote.text)
            role = Role.Button
            onClick { onJump(); true }
        }.clickable(role = Role.Button, onClick = onJump)
    }
    if (measures?.oneLine == true) {
        ShortReference(quote, name, toMe, session, button)
        return
    }
    Row(
        Modifier.fillMaxWidth().padding(start = indent).then(button),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space6),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // With the avatars hidden there is nothing for it to come from.
        if (measures == null || measures.avatar != null) Connector()
        if (quote.originalDeleted) {
            Box(Modifier.size(RefAvatar).background(LumiaTokens.chip.color, CircleShape), contentAlignment = Alignment.Center) { Minus() }
            Text(ORIGINAL_DELETED, style = small(LumiaTokens.textMessage.weight).copy(fontStyle = FontStyle.Italic), color = LumiaTokens.inkMuted.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
            return@Row
        }
        if (name != null) {
            LumiaAvatar(initialsOf(name), AvatarTone.of(quote.authorAccountId), RefAvatar, session.avatarOf(quote.authorAccountId))
            Text(
                referenceName(quote.authorAccountId, session.me, name),
                style = small(LumiaTokens.textSection.weight),
                color = if (toMe) LumiaTokens.accentInk.color else LumiaTokens.inkBody.color,
                maxLines = 1, overflow = TextOverflow.Ellipsis,
                modifier = Modifier.widthIn(max = NameMost),
            )
        }
        Text(quote.text.replace('\n', ' '), style = small(LumiaTokens.textMessage.weight), color = LumiaTokens.inkMuted.color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
    }
}

/**
 * A one-line row's reference: on one line and cut short, the turn arrow,
 * the quoted person's name and a colon (`@` and this account's own name, in
 * the accent, when it was quoted), then the quoted words, quieter; or the
 * arrow and "Original message was deleted", in italics. [button] is what a
 * tap and a screen reader do with it, as the grouped line's.
 */
@Composable
private fun ShortReference(quote: QuoteSummary, name: String?, toMe: Boolean, session: LumiaSession, button: Modifier) {
    val strong = SpanStyle(color = if (toMe) LumiaTokens.accentInk.color else LumiaTokens.inkBody.color, fontWeight = FontWeight(LumiaTokens.textSection.weight))
    val words = buildAnnotatedString {
        append(TURN)
        when {
            quote.originalDeleted -> withStyle(SpanStyle(fontStyle = FontStyle.Italic)) { append(ORIGINAL_DELETED) }
            else -> {
                if (name != null) {
                    withStyle(strong) { append(referenceName(quote.authorAccountId, session.me, name)) }
                    append(": ")
                }
                append(quote.text.replace('\n', ' '))
            }
        }
    }
    Text(words, style = small(LumiaTokens.textMessage.weight), color = LumiaTokens.inkMuted.color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.fillMaxWidth().then(button))
}

/** The arrow a one-line reference starts with, and the room after it. */
private const val TURN = "↳ "

/** The connector: down from the avatar's centre, a quarter turn, and along to the quoted avatar (the board's `M1 12V7a5 5 0 0 1 5-5h19`). */
@Composable
private fun Connector() {
    val color = LumiaTokens.borderHover.color
    Canvas(Modifier.padding(top = LumiaTokens.space8).size(ConnectorWidth, ConnectorHeight)) {
        // The board's path is drawn in points of a 26 by 12 box.
        val u = size.height / 12f
        val path = Path().apply {
            moveTo(1 * u, 12 * u)
            lineTo(1 * u, 7 * u)
            arcTo(Rect(1 * u, 2 * u, 11 * u, 12 * u), 180f, 90f, false)
            lineTo(25 * u, 2 * u)
        }
        drawPath(path, color, style = Stroke(width = 2 * u, cap = StrokeCap.Round))
    }
}

/** The deleted original's mark: a short rule across its circle. */
@Composable
private fun Minus() {
    val color = LumiaTokens.inkMuted.color
    Canvas(Modifier.size(MinusGlyph)) {
        // The board's `M5 12h14` in a box of 24, at a stroke of 2.4.
        val u = size.width / 24f
        drawLine(color, Offset(5 * u, 12 * u), Offset(19 * u, 12 * u), strokeWidth = 2.4f * u, cap = StrokeCap.Round)
    }
}

// A message's reactions

/**
 * A message's reactions as the design's chips: the emoji and how many,
 * the one this account has in the accent; a tap toggles this account's,
 * a long press says who reacted ("You and Ada reacted with 👍"). After
 * them, Add reaction opens the picker.
 */
@OptIn(ExperimentalLayoutApi::class, ExperimentalMaterial3Api::class)
@Composable
fun ReactionChips(session: LumiaSession, reactions: List<ReactionSummary>, onToggle: (String) -> Unit, onAdd: () -> Unit) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space6), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
        for (r in reactions) {
            val words = reactedWords(r, session.me, session::knownNameOf)
            val shape = RoundedCornerShape(LumiaTokens.radiusPill)
            TooltipBox(
                positionProvider = TooltipDefaults.rememberPlainTooltipPositionProvider(),
                tooltip = { PlainTooltip { Text(words) } },
                state = rememberTooltipState(),
            ) {
                Row(
                    Modifier
                        // Taller only where the text has grown past it (a large font size, the phone's text scale).
                        .heightIn(min = ChipHeight)
                        .clip(shape)
                        .background(if (r.me) LumiaTokens.accentSoft.color else LumiaTokens.reaction.color, shape)
                        .border(1.dp, if (r.me) LumiaTokens.reactionOnBorder.color else LumiaTokens.reaction.color, shape)
                        .clickable(role = Role.Button) { onToggle(r.reaction) }
                        .semantics(mergeDescendants = true) {
                            contentDescription = "$words, ${r.count}"
                            selected = r.me
                        }
                        .padding(horizontal = LumiaTokens.space12),
                    horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space6),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(r.reaction, style = LumiaTokens.textLinkSm.scaled.copy(fontWeight = FontWeight(LumiaTokens.textMessage.weight)))
                    Text("${r.count}", style = LumiaTokens.textLinkSm.scaled, color = if (r.me) LumiaTokens.accentInk.color else LumiaTokens.inkSecondary.color)
                }
            }
        }
        Box(
            Modifier.size(ChipHeight).clip(CircleShape).clickable(role = Role.Button, onClick = onAdd).semantics { contentDescription = "Add reaction" },
            contentAlignment = Alignment.Center,
        ) {
            Icon(Icons.Outlined.AddReaction, contentDescription = null, tint = LumiaTokens.inkMuted.color, modifier = Modifier.size(LumiaTokens.space16))
        }
    }
}

// The message's actions

/**
 * A long press on a message: Add reaction, Reply, Edit message (this
 * account's own), Copy text and Delete message (its own, or anyone's for a
 * moderator); each that is null is not offered.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MessageActionsSheet(onAddReaction: () -> Unit, onReply: (() -> Unit)?, onEdit: (() -> Unit)?, onCopy: () -> Unit, onDelete: (() -> Unit)?, onDismiss: () -> Unit) {
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = LumiaTokens.surface.color, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.fillMaxWidth().padding(start = LumiaTokens.space16, end = LumiaTokens.space16, bottom = LumiaTokens.space24).semantics { contentDescription = "Message actions" }) {
            SheetItem(Icons.Outlined.AddReaction, "Add reaction", onAddReaction)
            if (onReply != null) SheetItem(Icons.AutoMirrored.Filled.Reply, "Reply", onReply)
            if (onEdit != null) SheetItem(Icons.Outlined.Edit, "Edit message", onEdit)
            SheetItem(Icons.Outlined.ContentCopy, "Copy text", onCopy)
            // The design's menu has no danger ink, so it is drawn as the rest.
            if (onDelete != null) SheetItem(Icons.Outlined.Delete, "Delete message", onDelete)
        }
    }
}

@Composable
private fun SheetItem(icon: ImageVector, label: String, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = LumiaTokens.controlHeight + LumiaTokens.space8).clip(RoundedCornerShape(LumiaTokens.radius10)).clickable(role = Role.Button, onClick = onClick).padding(horizontal = LumiaTokens.space10),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space14),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, contentDescription = null, tint = LumiaTokens.inkBody.color, modifier = Modifier.size(20.dp))
        Text(label, style = LumiaTokens.textNav.style, color = LumiaTokens.ink.color)
    }
}

/**
 * The emoji picker, as a sheet: a search field ("Search emoji"), then
 * "Frequently used" from this phone's own reactions, then every emoji by
 * Unicode's groups (`EmojiSet`: up to Unicode 16, the default skin tone).
 * A search matches an emoji's name and shortcodes. Choosing one reacts
 * with it and closes the sheet.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun EmojiPickerSheet(frequent: List<String>, onPick: (String) -> Unit, onDismiss: () -> Unit) {
    var search by rememberSaveable { mutableStateOf("") }
    val found = remember(search) { EmojiSet.search(search) }
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = LumiaTokens.surface.color, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.fillMaxWidth().padding(horizontal = LumiaTokens.space16)) {
            Box(
                Modifier.fillMaxWidth().height(LumiaTokens.controlHeight)
                    .background(LumiaTokens.field.color, RoundedCornerShape(LumiaTokens.radius8))
                    .border(1.dp, LumiaTokens.borderForm.color, RoundedCornerShape(LumiaTokens.radius8))
                    .padding(horizontal = LumiaTokens.space10),
                contentAlignment = Alignment.CenterStart,
            ) {
                LumiaInput(search, { search = it }, "Search emoji", modifier = Modifier.fillMaxWidth().semantics { contentDescription = "Search emoji" })
            }
            val cell = LumiaTokens.tileSize
            LazyVerticalGrid(
                columns = GridCells.Adaptive(cell),
                modifier = Modifier.fillMaxWidth().height(cell * 7).padding(top = LumiaTokens.space8),
            ) {
                fun heading(words: String) {
                    item(span = { GridItemSpan(maxLineSpan) }, key = "heading $words", contentType = "heading") {
                        Text(words, style = LumiaTokens.textSection.style, color = LumiaTokens.inkMuted.color, modifier = Modifier.padding(top = LumiaTokens.space8, bottom = LumiaTokens.space4).semantics { heading() })
                    }
                }
                if (search.isNotBlank()) {
                    if (found.isEmpty()) heading("No emoji found")
                    items(found, key = { "found ${it.emoji}" }, contentType = { "emoji" }) { EmojiCell(it.emoji, it.name, onPick) }
                } else {
                    heading("Frequently used")
                    items(frequent, key = { "frequent $it" }, contentType = { "emoji" }) { EmojiCell(it, EmojiSet.nameOf(it) ?: it, onPick) }
                    for ((group, emoji) in EmojiSet.byGroup) {
                        heading(group)
                        items(emoji, key = { "${group} ${it.emoji}" }, contentType = { "emoji" }) { EmojiCell(it.emoji, it.name, onPick) }
                    }
                }
            }
            Box(Modifier.navigationBarsPadding().height(LumiaTokens.space8))
        }
    }
}

@Composable
private fun EmojiCell(emoji: String, name: String, onPick: (String) -> Unit) {
    Box(
        Modifier.size(LumiaTokens.tileSize).clip(RoundedCornerShape(LumiaTokens.radius8)).clickable(role = Role.Button) { onPick(emoji) }.semantics { contentDescription = name },
        contentAlignment = Alignment.Center,
    ) {
        Text(emoji, style = LumiaTokens.titleChannel.style)
    }
}

// Editing and deleting

/**
 * The field a message is edited in, where its words were: the design's
 * text field (the field's fill, the active border, the radius 12), growing
 * with its text to 12 lines and then scrolling inside itself, holding the
 * words with the caret at their end. Under it, Cancel and Save, a phone
 * having no Escape or Return to say: a hardware keyboard's Return saves
 * (Shift-Return is a new line) and Escape cancels. [room] is the core's
 * for what it holds: the composer's counter under it near the limit, and
 * over it, Save and Return refused and the words kept.
 */
@Composable
fun EditField(value: TextFieldValue, onValue: (TextFieldValue) -> Unit, room: MessageRoom?, onSave: () -> Unit, onCancel: () -> Unit) {
    val over = isOver(room)
    val focus = remember { FocusRequester() }
    val shape = RoundedCornerShape(LumiaTokens.radius12)
    LaunchedEffect(Unit) { focus.requestFocus() }
    Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
        BasicTextField(
            value = value,
            onValueChange = onValue,
            modifier = Modifier
                .fillMaxWidth()
                .focusRequester(focus)
                .onPreviewKeyEvent { e ->
                    if (e.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                    when {
                        e.key == Key.Enter && !e.isShiftPressed -> { if (!over) onSave(); true }
                        e.key == Key.Escape -> { onCancel(); true }
                        else -> false
                    }
                }
                .semantics { contentDescription = "Edit message" }
                .background(LumiaTokens.field.color, shape)
                .border(1.dp, LumiaTokens.borderActive.color, shape)
                .padding(horizontal = LumiaTokens.space12, vertical = LumiaTokens.space8),
            textStyle = LumiaTokens.textMessage.scaled.copy(color = LumiaTokens.inkField.color),
            cursorBrush = SolidColor(LumiaTokens.accent.color),
            maxLines = 12,
        )
        LumiaCounter(room, Modifier.padding(horizontal = LumiaTokens.space12))
        Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space4), verticalAlignment = Alignment.CenterVertically) {
            LumiaLinkButton("Cancel", onClick = onCancel)
            LumiaLinkButton("Save", enabled = !over, onClick = onSave)
        }
    }
}

/** The question before a message is deleted: its first line under it, Cancel and Delete. */
@Composable
fun DeleteDialog(line: String, onDelete: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = LumiaTokens.surface.color,
        title = { Text(DELETE_QUESTION, style = LumiaTokens.titleChannel.style, color = LumiaTokens.ink.color) },
        text = { if (line.isNotEmpty()) Text(line, style = LumiaTokens.textBody.style, color = LumiaTokens.inkMuted.color, maxLines = 3, overflow = TextOverflow.Ellipsis) },
        // The design has no destructive button, so Delete is the primary.
        confirmButton = { LumiaPrimaryButton("Delete", onClick = onDelete) },
        dismissButton = { LumiaSecondaryButton("Cancel", onClick = onDismiss) },
    )
}

// The composer's frame and over it

/**
 * The reply bar, inside the composer's frame over its field (the board's):
 * the inset's fill and a rule under it, the reply glyph, "Replying to
 * **Ada**", the "@ On" switch where it is offered ([mention] null where it
 * is not: a reply to this account's own message, or in a conversation
 * where everyone is told of every message), and Cancel reply.
 */
@Composable
fun ReplyBar(name: String, mention: Boolean?, onMention: () -> Unit, onCancel: () -> Unit) {
    Column {
        Row(
            Modifier.fillMaxWidth().background(LumiaTokens.inset.color)
                .padding(start = LumiaTokens.space14, end = LumiaTokens.space8, top = BarPadY, bottom = BarPadY),
            horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space10),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(Icons.AutoMirrored.Filled.Reply, contentDescription = null, tint = LumiaTokens.inkSecondary.color, modifier = Modifier.size(ReplyGlyph))
            val first = replyingTo(name)
            val strong = SpanStyle(color = LumiaTokens.ink.color, fontWeight = FontWeight(LumiaTokens.textCount.weight))
            Text(
                buildAnnotatedString { append(first); addStyle(strong, first.length - name.length, first.length) },
                style = LumiaTokens.textLabel.style.copy(fontWeight = FontWeight(LumiaTokens.textBody.weight)),
                color = LumiaTokens.inkSecondary.color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
            )
            if (mention != null) MentionSwitch(name, mention, onMention)
            Box(
                Modifier.size(LumiaTokens.space28).clip(RoundedCornerShape(LumiaTokens.radius8)).clickable(role = Role.Button, onClick = onCancel)
                    .semantics { contentDescription = "Cancel reply" },
                contentAlignment = Alignment.Center,
            ) {
                Icon(Icons.Filled.Close, contentDescription = null, tint = LumiaTokens.inkQuiet.color, modifier = Modifier.size(LumiaTokens.space14))
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(LumiaTokens.borderComposer.color))
    }
}

/** "@ On": whether the reply notifies the person quoted, a pill pressed while it does. "Mention Ada" to a screen reader, on or off. */
@Composable
private fun MentionSwitch(name: String, on: Boolean, onToggle: () -> Unit) {
    val ink = if (on) LumiaTokens.accentInk.color else LumiaTokens.inkQuiet.color
    Row(
        Modifier.height(LumiaTokens.space28).clip(RoundedCornerShape(LumiaTokens.radiusPill))
            .background(if (on) LumiaTokens.accentSoft.color else LumiaTokens.chip.color)
            .clearAndSetSemantics {
                contentDescription = mentionSwitchLabel(name)
                stateDescription = mentionSwitchWord(on)
                role = Role.Switch
                selected = on
                onClick { onToggle(); true }
            }
            .clickable(role = Role.Switch, onClick = onToggle)
            .padding(horizontal = LumiaTokens.space10),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space4),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Filled.AlternateEmail, contentDescription = null, tint = ink, modifier = Modifier.size(SwitchGlyph))
        Text(mentionSwitchWord(on), style = LumiaTokens.textSection.style.copy(fontWeight = FontWeight(LumiaTokens.textCount.weight)), color = ink)
    }
}

/** A bar over the composer: what it says, and what can be done at its end; the accent's rule on its left. */
@Composable
private fun ComposerBar(content: @Composable RowScope.() -> Unit) {
    val shape = RoundedCornerShape(LumiaTokens.radius12)
    Row(
        Modifier
            .fillMaxWidth()
            .padding(start = LumiaTokens.space16, end = LumiaTokens.space16, bottom = LumiaTokens.space6)
            .clip(shape)
            .background(LumiaTokens.inset.color, shape)
            .height(androidx.compose.foundation.layout.IntrinsicSize.Min),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width(LumiaTokens.space2).fillMaxHeight().background(LumiaTokens.accent.color))
        Row(
            Modifier.weight(1f).padding(start = LumiaTokens.space10, end = LumiaTokens.space4, top = LumiaTokens.space8, bottom = LumiaTokens.space8),
            horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space10),
            verticalAlignment = Alignment.CenterVertically,
            content = content,
        )
    }
}

/**
 * After a send that mentioned people not in the channel: who ("Ada isn’t
 * in #design, so they won’t be notified."), Add to #design and Dismiss; after an
 * add that made invitations, who was invited, for a few seconds; a failure's words.
 */
@Composable
fun OutsideBar(words: String, channel: String, said: Boolean, adding: Boolean, error: String, onAdd: () -> Unit, onDismiss: () -> Unit) {
    ComposerBar {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
            Text(words, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkBody.color)
            if (error.isNotEmpty()) LumiaErrorText(error)
            if (!said) {
                Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8), verticalAlignment = Alignment.CenterVertically) {
                    LumiaSecondaryButton("Add to #$channel", enabled = !adding, onClick = onAdd)
                    LumiaLinkButton("Dismiss", onClick = onDismiss)
                }
            }
        }
    }
}

/** The most room a name takes in a row of the @ list, or on a reference line; the rest is the handle's or the quote's. */
private val NameMost: Dp = LumiaTokens.avatarLg * 4

/**
 * The @ list over the composer: @everyone first for one who may, then the
 * server's members that match, each with its avatar, name and handle, and
 * marked when not in the channel. A tap picks.
 */
@Composable
fun MentionList(choices: List<MentionChoice>, channel: String, pictureOf: (String) -> String?, onPick: (MentionChoice) -> Unit) {
    val shape = RoundedCornerShape(LumiaTokens.radius14)
    Column(
        Modifier
            .fillMaxWidth()
            .padding(start = LumiaTokens.space16, end = LumiaTokens.space16, bottom = LumiaTokens.space6)
            .background(LumiaTokens.surface.color, shape)
            .border(1.dp, LumiaTokens.lineStrong.color, shape)
            .padding(LumiaTokens.space6)
            .semantics { contentDescription = "People to mention" },
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2),
    ) {
        for (choice in choices) {
            val note = mentionNote(choice, channel)
            val (name, handle) = when (choice) {
                MentionChoice.Everyone -> "@everyone" to ""
                is MentionChoice.Member -> nameOf(choice.member) to "@${choice.member.handle}"
            }
            Row(
                Modifier.fillMaxWidth().heightIn(min = LumiaTokens.controlHeight + LumiaTokens.space4).clip(RoundedCornerShape(LumiaTokens.radius8))
                    .clickable(role = Role.Button) { onPick(choice) }
                    .semantics(mergeDescendants = true) { contentDescription = listOf("$name $handle".trim(), note).filterNotNull().joinToString(", ") }
                    .padding(horizontal = LumiaTokens.space10),
                horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space10),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                when (choice) {
                    MentionChoice.Everyone -> Box(Modifier.size(LumiaTokens.avatarXs).background(LumiaTokens.accentSoft.color, CircleShape), contentAlignment = Alignment.Center) {
                        Icon(Icons.Filled.AlternateEmail, contentDescription = null, tint = LumiaTokens.accentInk.color, modifier = Modifier.size(LumiaTokens.space14))
                    }
                    is MentionChoice.Member -> LumiaAvatar(initialsOf(name), AvatarTone.of(choice.member.accountId), LumiaTokens.avatarXs, pictureOf(choice.member.accountId))
                }
                // The name as it comes (up to half the row), the handle in what is left, the note at the end.
                Text(name, style = LumiaTokens.textCompact.style, color = LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.widthIn(max = NameMost))
                Text(handle, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                if (note != null) Text(note, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}
