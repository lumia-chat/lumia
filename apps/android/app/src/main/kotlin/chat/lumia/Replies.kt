// What quote replies, reactions and mentions add to a conversation on the
// phone (architecture 6.6), the desktop's and the web's states and words in
// Android's idiom: a long press on a message opens its actions as a bottom
// sheet (the six quick reactions in a row, Reply, Add reaction, Copy), Add
// reaction opens the emoji picker as a sheet, a quote is drawn above a
// message's words and its reactions as chips under them, and over the
// composer go the quote being replied with, the @ list and the bar naming
// those a message mentioned who are not in its channel. The rules these
// draw from are `Replies.kt` in lumiakit.
//
// A message's text is not selectable on the phone (it never was: a plain
// Text, and the long press is the message's actions), so there is no
// "Quote reply" of a selection here; Reply quotes the whole message.
package chat.lumia

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Reply
import androidx.compose.material.icons.filled.AlternateEmail
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.outlined.AddReaction
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import chat.lumia.kit.AvatarTone
import chat.lumia.kit.EmojiSet
import chat.lumia.kit.LumiaAvatar
import chat.lumia.kit.LumiaErrorText
import chat.lumia.kit.LumiaInput
import chat.lumia.kit.LumiaLinkButton
import chat.lumia.kit.LumiaSecondaryButton
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.MentionChoice
import chat.lumia.kit.QUICK_REACTIONS
import chat.lumia.kit.initialsOf
import chat.lumia.kit.mentionNote
import chat.lumia.kit.nameOf
import chat.lumia.kit.reactedWords
import chat.lumia.kit.replyingTo
import uniffi.lumia_ffi.QuoteSummary
import uniffi.lumia_ffi.ReactionSummary

/** A message's quick reaction or chip as words are an emoji's own; this names it to a screen reader. */
private fun reactWith(emoji: String) = "React with $emoji"

// A message's quote and reactions

/**
 * A quote above a message's words: the accent's rule on its left, the
 * quoted author's name (none in a personal conversation where it is not
 * known) and the text cut to two lines. A tap finds the original; a quote
 * of a deleted message says so and does nothing.
 */
@Composable
fun QuoteBlock(session: LumiaSession, quote: QuoteSummary, onJump: () -> Unit) {
    val rule = Modifier.width(LumiaTokens.space2).fillMaxHeight()
    if (quote.originalDeleted) {
        Row(Modifier.fillMaxWidth().height(androidx.compose.foundation.layout.IntrinsicSize.Min), horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
            Box(rule.background(LumiaTokens.lineStrong.color, RoundedCornerShape(LumiaTokens.space2)))
            Text("Original message was deleted", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
        }
        return
    }
    val name = quote.authorAccountId.takeIf { it.isNotEmpty() }?.let { session.nameOf(it) }
    Row(
        Modifier
            .fillMaxWidth()
            .height(androidx.compose.foundation.layout.IntrinsicSize.Min)
            .clip(RoundedCornerShape(LumiaTokens.radius8))
            .clickable(role = Role.Button, onClickLabel = "go to the original", onClick = onJump)
            .semantics(mergeDescendants = true) { contentDescription = if (name != null) "Quote of $name: ${quote.text}" else "Quote: ${quote.text}" }
            .padding(vertical = LumiaTokens.space2),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
    ) {
        Box(rule.background(LumiaTokens.accent.color, RoundedCornerShape(LumiaTokens.space2)))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
            if (name != null) Text(name, style = LumiaTokens.textSection.style, color = LumiaTokens.inkEmphasis.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(quote.text, style = LumiaTokens.textCompact.style, color = LumiaTokens.inkSecondary.color, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
    }
}

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
                        .height(LumiaTokens.space28)
                        .clip(shape)
                        .background(if (r.me) LumiaTokens.accentSoft.color else LumiaTokens.reaction.color, shape)
                        .border(1.dp, if (r.me) LumiaTokens.reactionOnBorder.color else LumiaTokens.reaction.color, shape)
                        .clickable(role = Role.Button) { onToggle(r.reaction) }
                        .semantics(mergeDescendants = true) {
                            contentDescription = "$words, ${r.count}"
                            selected = r.me
                        }
                        .padding(horizontal = LumiaTokens.space10),
                    horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space6),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(r.reaction, style = LumiaTokens.textCompact.style)
                    Text("${r.count}", style = LumiaTokens.textLabel.style, color = if (r.me) LumiaTokens.accentInk.color else LumiaTokens.inkBody.color)
                }
            }
        }
        Box(
            Modifier.size(LumiaTokens.space28).clip(CircleShape).clickable(role = Role.Button, onClick = onAdd).semantics { contentDescription = "Add reaction" },
            contentAlignment = Alignment.Center,
        ) {
            Icon(Icons.Outlined.AddReaction, contentDescription = null, tint = LumiaTokens.inkMuted.color, modifier = Modifier.size(LumiaTokens.space16))
        }
    }
}

// The message's actions

/**
 * A long press on a message: the six quick reactions in a row at the top
 * (each a reaction this account adds, or takes away when it has it), then
 * Reply, Add reaction and Copy.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MessageActionsSheet(onReact: (String) -> Unit, onReply: (() -> Unit)?, onAddReaction: () -> Unit, onCopy: () -> Unit, onDismiss: () -> Unit) {
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = LumiaTokens.surface.color, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.fillMaxWidth().padding(start = LumiaTokens.space16, end = LumiaTokens.space16, bottom = LumiaTokens.space24).semantics { contentDescription = "Message actions" }) {
            Row(Modifier.fillMaxWidth().padding(bottom = LumiaTokens.space12), horizontalArrangement = Arrangement.SpaceBetween) {
                for (emoji in QUICK_REACTIONS) {
                    Box(
                        Modifier.size(LumiaTokens.tileSize).clip(CircleShape).background(LumiaTokens.reaction.color).clickable(role = Role.Button) { onReact(emoji) }
                            .semantics { contentDescription = reactWith(emoji) },
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(emoji, style = LumiaTokens.titleChannel.style)
                    }
                }
            }
            if (onReply != null) SheetItem(Icons.AutoMirrored.Filled.Reply, "Reply", onReply)
            SheetItem(Icons.Outlined.AddReaction, "Add reaction", onAddReaction)
            SheetItem(Icons.Filled.ContentCopy, "Copy", onCopy)
        }
    }
}

@Composable
private fun SheetItem(icon: androidx.compose.ui.graphics.vector.ImageVector, label: String, onClick: () -> Unit) {
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

// Over the composer

/** A bar over the composer: what it says, and what can be done at its end; the accent's rule on its left. */
@Composable
private fun ComposerBar(content: @Composable androidx.compose.foundation.layout.RowScope.() -> Unit) {
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

/** While replying: "Replying to Ada" over the quote on one line, and Cancel reply. */
@Composable
fun ReplyBar(name: String, text: String, onCancel: () -> Unit) {
    ComposerBar {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
            val first = replyingTo(name)
            val strong = SpanStyle(color = LumiaTokens.inkEmphasis.color, fontWeight = FontWeight(LumiaTokens.textSection.weight))
            Text(
                buildAnnotatedString { append(first); addStyle(strong, first.length - name.length, first.length) },
                style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color, maxLines = 1, overflow = TextOverflow.Ellipsis,
            )
            Text(text, style = LumiaTokens.textCompact.style, color = LumiaTokens.inkSecondary.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        androidx.compose.material3.IconButton(onClick = onCancel, modifier = Modifier.size(44.dp)) {
            Icon(Icons.Filled.Close, contentDescription = "Cancel reply", tint = LumiaTokens.inkQuiet.color, modifier = Modifier.size(18.dp))
        }
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

/** The most room a name takes in a row of the @ list; the handle and the note have the rest. */
private val NameMost = LumiaTokens.avatarLg * 4

/**
 * The @ list over the composer: @everyone first for one who may, then the
 * server's members that match, each with its avatar, name and handle, and
 * marked when not in the channel. A tap picks.
 */
@Composable
fun MentionList(choices: List<MentionChoice>, channel: String, onPick: (MentionChoice) -> Unit) {
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
                    is MentionChoice.Member -> LumiaAvatar(initialsOf(name), AvatarTone.of(choice.member.accountId), LumiaTokens.avatarXs)
                }
                // The name as it comes (up to half the row), the handle in what is left, the note at the end.
                Text(name, style = LumiaTokens.textCompact.style, color = LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.widthIn(max = NameMost))
                Text(handle, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                if (note != null) Text(note, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}
