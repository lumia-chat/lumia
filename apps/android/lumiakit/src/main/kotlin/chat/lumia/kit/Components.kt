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
import androidx.compose.foundation.clickable
import androidx.compose.foundation.border
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
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
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
// the nearest tokens (the desktop shell's `tokens.rs` does the same).

/** The room between one block of messages and the next. The boards drew 2; Ryan asked for more (2026-11-12). */
private val BlockGap = LumiaTokens.space8
/** Between two messages of a block, and under its name: a paragraph's distance, as the boards draw a message of several. */
private val BlockWithin = LumiaTokens.space4
/** Under the line that says who is typing, to the panel's foot (20). */
private val TypingFoot = LumiaTokens.space16 + LumiaTokens.space4
/** Between two dots of the typing line (3). */
private val TypingDotGap = (LumiaTokens.space2 + LumiaTokens.space4) / 2

/**
 * A message's row. Messages one person sends close together are one block
 * (`joins` says what that is, [place] where this one sits in its block):
 * the first is an avatar beside the name, the time and the message; the
 * ones after it are the message alone, in the same column, a paragraph's
 * distance under the one before. A phone has no pointer to show such a
 * row's time under, so it is not drawn; a screen reader still hears whose
 * the message is and when, before it. Each row is one stop for it.
 */
@Composable
fun LumiaMessageRow(author: String, tone: AvatarTone, time: String, text: String, deleted: Boolean = false, place: BlockPlace = BlockPlace.Alone) {
    val body = @Composable { modifier: Modifier ->
        Text(
            if (deleted) "(deleted)" else text,
            style = LumiaTokens.textMessage.style,
            color = if (deleted) LumiaTokens.inkQuiet.color else LumiaTokens.inkBody.color,
            modifier = modifier,
        )
    }
    val frame = Modifier
        .fillMaxWidth()
        .semantics(mergeDescendants = true) {}
        .padding(
            start = LumiaTokens.space16,
            end = LumiaTokens.space16,
            top = if (place.opens) BlockGap + LumiaTokens.space8 else BlockWithin / 2,
            bottom = if (place.closes) LumiaTokens.space8 else BlockWithin / 2,
        )
    if (!place.opens) {
        Box(frame) {
            Box(Modifier.matchParentSize().semantics { contentDescription = "$author, $time" })
            body(Modifier.padding(start = LumiaTokens.avatarLg + LumiaTokens.space12))
        }
        return
    }
    Row(modifier = frame, horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12), verticalAlignment = Alignment.Top) {
        // The name beside it says who; the initials would only be read out before it.
        Box(Modifier.clearAndSetSemantics {}) { LumiaAvatar(initialsOf(author), tone, LumiaTokens.avatarLg) }
        Column(verticalArrangement = Arrangement.spacedBy(BlockWithin), modifier = Modifier.weight(1f)) {
            Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8), verticalAlignment = Alignment.Bottom) {
                Text(author, style = LumiaTokens.textAuthor.style, color = LumiaTokens.inkEmphasis.color)
                Text(time, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkQuiet.color)
            }
            body(Modifier)
        }
    }
}

@Composable
fun LumiaDayDivider(label: String) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = LumiaTokens.space16, vertical = LumiaTokens.space8),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space10),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.weight(1f).height(1.dp).background(LumiaTokens.line.color))
        Text(label, style = LumiaTokens.textSection.style, color = LumiaTokens.inkQuiet.color)
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
            LumiaInput(text, onText, placeholder, secure = secure, keyboard = keyboard, modifier = Modifier.fillMaxWidth())
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

/** A row of pills with one chosen: the design's segmented tabs at a form's width. */
@Composable
fun LumiaChoice(options: List<Pair<String, String>>, value: String, onChange: (String) -> Unit, modifier: Modifier = Modifier) {
    Row(
        modifier
            .fillMaxWidth()
            .background(LumiaTokens.chip.color, RoundedCornerShape(LumiaTokens.radiusPill))
            .padding(LumiaTokens.space2),
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
                    .clickable { onChange(id) }
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

/** Content with the ink colour for text that sets none. */
@Composable
fun LumiaInk(content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalContentColor provides LumiaTokens.ink.color, content = content)
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
