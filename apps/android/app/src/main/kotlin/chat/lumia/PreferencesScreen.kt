// Preferences: how Lumia looks on this phone, the "Preferences · Light" and
// "Preferences · Dark" boards (2026-11-16) in one column. Appearance as
// three pictures of the app; Messages with a preview of four messages at
// its head, the three presets as pictures of their layout, the settings as
// rows of compact segments in an outlined group and the media's as
// switches in another. Every choice takes effect as it is made and is the
// phone's (`AppStore`); the page's rules and words are `Preferences.kt`'s.
package chat.lumia

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.Icon
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import chat.lumia.kit.AppStore
import chat.lumia.kit.Appearance
import chat.lumia.kit.AvatarTone
import chat.lumia.kit.Display
import chat.lumia.kit.DisplaySetting
import chat.lumia.kit.DisplaySwitch
import chat.lumia.kit.LocalLumiaDark
import chat.lumia.kit.LocalPressable
import chat.lumia.kit.LocalTextScale
import chat.lumia.kit.LumiaColor
import chat.lumia.kit.LumiaMessageRow
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.LumiaType
import chat.lumia.kit.MediaShape
import chat.lumia.kit.Preset
import chat.lumia.kit.PreviewMessage
import chat.lumia.kit.attachmentLabel
import chat.lumia.kit.cardsDrawn
import chat.lumia.kit.fileKindOf
import chat.lumia.kit.fitted
import chat.lumia.kit.mediaShape
import chat.lumia.kit.segmentLines
import chat.lumia.kit.sizeWords

/** A choice's picture, 72 tall: the boards' 92, made shorter for a third of a phone's width. */
private val PictureHeight = LumiaTokens.tileSize + LumiaTokens.space28

/** A font size's segment, the boards' 36 wide. */
private val FontSegment = LumiaTokens.rowHeight + LumiaTokens.space4

/** The ring around the picture chosen, the boards' 2 wide and 2 out from it. */
private val RingWidth = LumiaTokens.space2
private val RingGap = LumiaTokens.space2

/** "PREVIEW"'s letters, the boards' 0.06em apart: the tokens' type has no tracking. */
private val PreviewTracking = 0.06.em

/**
 * How Lumia looks on this phone: light or dark (or as the phone is), and
 * how a conversation's messages are drawn. The desktop's page without its
 * scroll bar row (a phone's scroll indicators are the system's).
 *
 * Messages starts with the preview, so it is in view while choosing, then
 * the presets: the card lit is the one the settings all match
 * (`Display.preset`), none once one is changed, and the Custom line says
 * so. The avatars are not offered in one line, which draws none.
 */
@Composable
fun PreferencesScreen(store: AppStore, back: () -> Unit) {
    val display = store.display
    val preset = display.preset
    PanelScreen(back = "Home", onBack = back, title = "Preferences", subtitle = "How Lumia looks on this phone") {
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).navigationBarsPadding()
                .padding(horizontal = LumiaTokens.space16, vertical = LumiaTokens.space24),
            verticalArrangement = Arrangement.spacedBy(LumiaTokens.space40),
        ) {
            Section("Appearance", "Light, dark, or whichever this phone is in.") {
                Row(Modifier.selectableGroup(), horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
                    for ((choice, name) in listOf(Appearance.LIGHT to "Light", Appearance.DARK to "Dark", Appearance.SYSTEM to "System")) {
                        PictureChoice(name, null, store.appearance == choice, { store.choose(choice) }) { modifier ->
                            when (choice) {
                                Appearance.SYSTEM -> SystemSketch(modifier)
                                else -> WindowSketch(choice == Appearance.DARK, modifier)
                            }
                        }
                    }
                }
            }
            Section("Messages", "Start from a preset, then change anything below.") {
                MessagesPreview(display)
                Row(Modifier.padding(top = LumiaTokens.space8).selectableGroup(), horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
                    for (p in Preset.entries) {
                        PictureChoice(p.label, p.caption, preset == p, { store.choose(display.with(p)) }) { modifier -> PresetSketch(p, modifier) }
                    }
                }
                if (preset == null) CustomLine()
                Group {
                    DisplaySetting.offered(display).forEachIndexed { i, setting ->
                        if (i > 0) Divider()
                        SettingRow(setting.label, setting.caption) {
                            val segments = @Composable { each: Dp? ->
                                Segments(setting.options, setting.chosen(display), { name -> setting.choose(display, name)?.let(store::choose) }, each)
                            }
                            if (setting == DisplaySetting.FONT_SIZE) {
                                // The two As say smaller and larger to the eye; to TalkBack the segments say it.
                                Flanked(
                                    { Text("A", style = LumiaTokens.textCount.style, color = LumiaTokens.inkMuted.color, modifier = Modifier.clearAndSetSemantics {}) },
                                    { segments(FontSegment) },
                                    { Text("A", style = LumiaTokens.titleMonogram.style, color = LumiaTokens.inkMuted.color, modifier = Modifier.clearAndSetSemantics {}) },
                                )
                            } else {
                                segments(null)
                            }
                        }
                    }
                }
                Group {
                    DisplaySwitch.entries.forEachIndexed { i, switch ->
                        if (i > 0) Divider()
                        SwitchRow(switch.label, switch.caption(display), switch.on(display)) { store.choose(switch.turned(display, it)) }
                    }
                }
            }
        }
    }
}

/** A section: its name, a line under it, and what it holds 16 under that. */
@Composable
private fun Section(title: String, subtitle: String, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space16)) {
        Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
            Text(title, style = LumiaTokens.titleServer.style, color = LumiaTokens.ink.color, modifier = Modifier.semantics { heading() })
            Text(subtitle, style = LumiaTokens.textCompact.style, color = LumiaTokens.inkMuted.color)
        }
        content()
    }
}

// The pictures to choose from

/**
 * One of three choices drawn as a picture: [sketch] over its [name], a
 * tick after the name of the one chosen, and its [caption] under it. The
 * one chosen has the accent's ring 2 outside its picture, 2 from it; the
 * rest the strong line's outline. A radio button to TalkBack, its name and
 * caption read together, and its place among the three.
 */
@Composable
internal fun RowScope.PictureChoice(name: String, caption: String?, chosen: Boolean, onChoose: () -> Unit, sketch: @Composable (Modifier) -> Unit) {
    val accent = LumiaTokens.accent.color
    val outline = LumiaTokens.lineStrong.color
    val shape = RoundedCornerShape(LumiaTokens.radius12)
    Column(
        Modifier.weight(1f).selectable(selected = chosen, role = Role.RadioButton, onClick = onChoose),
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
    ) {
        sketch(
            Modifier
                .fillMaxWidth()
                .height(PictureHeight)
                .drawBehind {
                    if (chosen) {
                        val stroke = RingWidth.toPx()
                        // The stroke is drawn about its middle: the gap, then half of it.
                        val out = RingGap.toPx() + stroke / 2
                        drawRoundRect(
                            accent,
                            topLeft = Offset(-out, -out),
                            size = Size(size.width + out * 2, size.height + out * 2),
                            cornerRadius = CornerRadius(LumiaTokens.radius12.toPx() + out),
                            style = Stroke(stroke),
                        )
                    }
                }
                .clip(shape)
                .then(if (chosen) Modifier else Modifier.border(1.dp, outline, shape)),
        )
        Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
            Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space4), verticalAlignment = Alignment.CenterVertically) {
                Text(name, style = LumiaTokens.textMember.style, color = if (chosen) LumiaTokens.ink.color else LumiaTokens.inkBody.color)
                if (chosen) Icon(Icons.Filled.Check, contentDescription = null, tint = LumiaTokens.accentInk.color, modifier = Modifier.size(LumiaTokens.space16))
            }
            if (caption != null) Text(caption, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
        }
    }
}

/** A colour of the theme named, whatever the app is in: the Light and Dark pictures are each always their own. */
private fun LumiaColor.of(dark: Boolean): Color = if (dark) this.dark else this.light

/** A rounded bar in a sketch, standing for a line of words. */
@Composable
private fun Bar(color: Color, modifier: Modifier = Modifier) {
    Box(modifier.height(LumiaTokens.space4).clip(RoundedCornerShape(LumiaTokens.radiusPill)).background(color))
}

/** The rail's strip of a sketched window. */
@Composable
private fun RailStrip(dark: Boolean) {
    Box(Modifier.fillMaxHeight().width(LumiaTokens.space8).background(LumiaTokens.rail.of(dark)))
}

/** The sidebar of a sketched window: three of its rows. */
@Composable
private fun SidebarSketch(dark: Boolean, modifier: Modifier) {
    val bar = LumiaTokens.railDivider.of(dark)
    Column(modifier.padding(horizontal = LumiaTokens.space6, vertical = LumiaTokens.space10), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
        Bar(bar, Modifier.fillMaxWidth(0.8f))
        Bar(bar, Modifier.fillMaxWidth())
        Bar(bar, Modifier.fillMaxWidth(0.64f))
    }
}

/** The main pane of a sketched window: a panel with a line in the accent over two of words. */
@Composable
private fun PanelSketch(dark: Boolean, modifier: Modifier) {
    val bar = LumiaTokens.border.of(dark)
    Column(
        modifier.clip(RoundedCornerShape(LumiaTokens.radius7)).background(LumiaTokens.surface.of(dark)).padding(LumiaTokens.space8),
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6),
    ) {
        Bar(LumiaTokens.accent.of(dark), Modifier.fillMaxWidth(0.5f))
        Bar(bar, Modifier.fillMaxWidth(0.9f))
        Bar(bar, Modifier.fillMaxWidth(0.72f))
    }
}

/** The app's window in the light or the dark theme: the rail, the sidebar, the panel. */
@Composable
private fun WindowSketch(dark: Boolean, modifier: Modifier) {
    Row(modifier.background(LumiaTokens.bg.of(dark))) {
        RailStrip(dark)
        SidebarSketch(dark, Modifier.weight(0.8f))
        PanelSketch(dark, Modifier.weight(1.2f).fillMaxHeight().padding(top = LumiaTokens.space6, bottom = LumiaTokens.space6, end = LumiaTokens.space6))
    }
}

/** System: the window half light, half dark. */
@Composable
private fun SystemSketch(modifier: Modifier) {
    Row(modifier) {
        Row(Modifier.weight(1f).fillMaxHeight().background(LumiaTokens.bg.light)) {
            RailStrip(false)
            SidebarSketch(false, Modifier.weight(1f))
        }
        Box(Modifier.weight(1f).fillMaxHeight().background(LumiaTokens.bg.dark).padding(LumiaTokens.space6)) {
            PanelSketch(true, Modifier.fillMaxSize())
        }
    }
}

/**
 * A preset's layout in little, on the track's fill: Comfortable two large
 * round avatars with a name and a line each, Compact small ones with the
 * lines closer, Minimal five lines of a time, a name and words.
 */
@Composable
private fun PresetSketch(preset: Preset, modifier: Modifier) {
    val strong = LumiaTokens.borderActive.color
    val soft = LumiaTokens.borderForm.color
    val avatar = @Composable { tone: AvatarTone, size: Dp -> Box(Modifier.size(size).clip(CircleShape).background(tone.fill.color)) }
    Box(modifier.background(LumiaTokens.track.color).padding(LumiaTokens.space10)) {
        when (preset) {
            Preset.COMFORTABLE -> Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space10)) {
                for ((tone, widths) in listOf(AvatarTone.MINT to (0.36f to 0.88f), AvatarTone.AMBER to (0.3f to 0.7f))) {
                    Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                        avatar(tone, LumiaTokens.space16)
                        Column(Modifier.weight(1f).padding(top = LumiaTokens.space2), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
                            Bar(strong, Modifier.fillMaxWidth(widths.first))
                            Bar(soft, Modifier.fillMaxWidth(widths.second))
                        }
                    }
                }
            }
            Preset.COMPACT -> Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
                val indent = Modifier.padding(start = LumiaTokens.space10 + LumiaTokens.space6)
                for ((tone, name, lines) in listOf(Triple(AvatarTone.MINT, 0.3f, listOf(0.8f)), Triple(AvatarTone.AMBER, 0.26f, listOf(0.64f, 0.72f)))) {
                    Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space6), verticalAlignment = Alignment.CenterVertically) {
                        avatar(tone, LumiaTokens.space10)
                        Bar(strong, Modifier.fillMaxWidth(name))
                    }
                    for (line in lines) Bar(soft, indent.fillMaxWidth(line))
                }
            }
            Preset.MINIMAL -> Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
                for ((name, words) in listOf(LumiaTokens.space14 to 0.5f, LumiaTokens.space14 to 0.38f, LumiaTokens.space12 to 0.6f, LumiaTokens.space14 to 0.3f, LumiaTokens.space12 to 0.46f)) {
                    Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
                        Bar(soft, Modifier.width(LumiaTokens.space10))
                        Bar(strong, Modifier.width(name))
                        Bar(soft, Modifier.fillMaxWidth(words))
                    }
                }
            }
        }
    }
}

/** Under the presets when none is lit: a dot in the accent and the line, which TalkBack reads as it comes. */
@Composable
private fun CustomLine() {
    Row(
        Modifier.semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(LumiaTokens.space6).clip(CircleShape).background(LumiaTokens.accent.color))
        Text(
            buildAnnotatedString {
                withStyle(SpanStyle(fontWeight = FontWeight(LumiaTokens.textMember.weight), color = LumiaTokens.ink.color)) { append("Custom:") }
                append(" these settings match no preset.")
            },
            style = LumiaTokens.textLinkSm.style.copy(fontWeight = FontWeight(LumiaTokens.textMeta.weight)),
            color = LumiaTokens.inkMuted.color,
        )
    }
}

// The rows

/** An outlined group of rows (radius 14, the strong line), each divided from the next by the line. */
@Composable
private fun Group(content: @Composable () -> Unit) {
    val shape = RoundedCornerShape(LumiaTokens.radius14)
    Column(Modifier.fillMaxWidth().clip(shape).border(1.dp, LumiaTokens.lineStrong.color, shape)) { content() }
}

@Composable
private fun Divider() {
    Box(Modifier.fillMaxWidth().height(1.dp).background(LumiaTokens.line.color))
}

/**
 * A setting's row: its name over its caption, and its [control] beside
 * them where the words keep at least half the row, else under them (a
 * phone's width puts every one of these under; a wide window beside, as
 * the boards draw it).
 */
@Composable
private fun SettingRow(label: String, caption: String, control: @Composable () -> Unit) {
    Layout(
        content = {
            Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
                Text(label, style = LumiaTokens.textMember.style, color = LumiaTokens.ink.color)
                Text(caption, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
            }
            Box { control() }
        },
        modifier = Modifier.fillMaxWidth().padding(horizontal = LumiaTokens.space16, vertical = LumiaTokens.space14),
    ) { measurables, constraints ->
        val (words, choice) = measurables
        val loose = constraints.copy(minWidth = 0, minHeight = 0)
        val width = constraints.maxWidth
        val choiceP = choice.measure(loose)
        val beside = width - choiceP.width - LumiaTokens.space16.roundToPx()
        if (beside >= width / 2) {
            val wordsP = words.measure(loose.copy(maxWidth = beside))
            val height = maxOf(wordsP.height, choiceP.height)
            layout(width, height) {
                wordsP.placeRelative(0, (height - wordsP.height) / 2)
                choiceP.placeRelative(width - choiceP.width, (height - choiceP.height) / 2)
            }
        } else {
            val wordsP = words.measure(loose)
            val gap = LumiaTokens.space10.roundToPx()
            layout(width, wordsP.height + gap + choiceP.height) {
                wordsP.placeRelative(0, 0)
                choiceP.placeRelative(0, wordsP.height + gap)
            }
        }
    }
}

/**
 * A setting's choices as compact segments on the track (radius 10, 2
 * inside it), each sized to its words, or [each] wide; the chosen one on
 * the active tab's fill, raised, in the ink, the rest in the muted ink.
 * Too wide for the room they are given (the phone's text made large, a
 * narrow window), they share it evenly, on more lines than one when a
 * line would cut their words (`segmentLines`). Each is a radio button to
 * TalkBack: its words, whether it is chosen, and its place among the rest;
 * the font sizes' say "13 points".
 */
@Composable
private fun Segments(options: List<Pair<String, String>>, chosen: String, onChoose: (String) -> Unit, each: Dp?) {
    val dark = LocalLumiaDark.current ?: isSystemInDarkTheme()
    val shape = RoundedCornerShape(LumiaTokens.radius8)
    Layout(
        content = {
            for ((id, label) in options) {
                val on = id == chosen
                Text(
                    label,
                    style = LumiaTokens.textLinkSm.style,
                    color = if (on) LumiaTokens.ink.color else LumiaTokens.inkMuted.color,
                    textAlign = TextAlign.Center,
                    maxLines = 1,
                    modifier = Modifier
                        .then(
                            if (on) {
                                // The tab's shadow, and in the light theme the edge the boards draw around it.
                                Modifier.shadow(LumiaTokens.space2 / 2, shape)
                                    .background(LumiaTokens.tabActive.color, shape)
                                    .then(if (dark) Modifier else Modifier.border(1.dp, LumiaTokens.border.color, shape))
                            } else Modifier,
                        )
                        .clip(shape)
                        .selectable(selected = on, role = Role.RadioButton) { onChoose(id) }
                        .then(if (each != null) Modifier.semantics { contentDescription = "$label points" } else Modifier)
                        .height(LumiaTokens.rowHeight)
                        .wrapContentHeight(Alignment.CenterVertically)
                        // Words have 12 beside them; a number in its fixed segment keeps 4 when the segments are squeezed.
                        .padding(horizontal = if (each == null) LumiaTokens.space12 else LumiaTokens.space4),
                )
            }
        },
        modifier = Modifier
            .background(LumiaTokens.track.color, RoundedCornerShape(LumiaTokens.radius10))
            .padding(LumiaTokens.space2)
            .selectableGroup(),
    ) { measurables, constraints ->
        val gap = LumiaTokens.space2.roundToPx()
        val fixed = each?.roundToPx()
        val needs = measurables.map { it.maxIntrinsicWidth(Constraints.Infinity) }
        val wanted = if (fixed == null) needs else needs.map { maxOf(it, fixed) }
        val lines = segmentLines(wanted, needs, constraints.maxWidth, gap)
        val rows = measurables.mapIndexed { i, m -> m.measure(Constraints.fixedWidth(lines.widths[i])) }.chunked(lines.perLine.coerceAtLeast(1))
        val heights = rows.map { row -> row.maxOf { it.height } }
        val width = rows.maxOfOrNull { row -> row.sumOf { it.width } + gap * (row.size - 1) } ?: 0
        layout(width, heights.sum() + gap * (rows.size - 1).coerceAtLeast(0)) {
            var y = 0
            rows.forEachIndexed { r, row ->
                var x = 0
                for (p in row) {
                    p.placeRelative(x, y + (heights[r] - p.height) / 2)
                    x += p.width + gap
                }
                y += heights[r] + gap
            }
        }
    }
}

/** [middle] between [start] and [end], 8 apart and centred on one line; the middle has what room the two leave. */
@Composable
private fun Flanked(start: @Composable () -> Unit, middle: @Composable () -> Unit, end: @Composable () -> Unit) {
    Layout(content = { start(); middle(); end() }) { measurables, constraints ->
        val (s, m, e) = measurables
        val gap = LumiaTokens.space8.roundToPx()
        val loose = constraints.copy(minWidth = 0, minHeight = 0)
        val sP = s.measure(loose)
        val eP = e.measure(loose)
        val mP = m.measure(loose.copy(maxWidth = (constraints.maxWidth - sP.width - eP.width - gap * 2).coerceAtLeast(0)))
        val height = maxOf(sP.height, mP.height, eP.height)
        layout(sP.width + mP.width + eP.width + gap * 2, height) {
            sP.placeRelative(0, (height - sP.height) / 2)
            mP.placeRelative(sP.width + gap, (height - mP.height) / 2)
            eP.placeRelative(sP.width + gap + mP.width + gap, (height - eP.height) / 2)
        }
    }
}

/**
 * A switch's row: its name over the line that says what its state does,
 * the switch at the end (on, the accent's track and the knob at the end;
 * off, the grey track and the knob at the start; the knob white and the
 * same size both ways). The whole row takes the tap and is the switch to
 * TalkBack, named by its words.
 */
@Composable
private fun SwitchRow(label: String, caption: String, on: Boolean, onChange: (Boolean) -> Unit) {
    // The boards' knob is white in both themes.
    val knob = LumiaTokens.surface.light
    Row(
        Modifier.fillMaxWidth().toggleable(value = on, role = Role.Switch, onValueChange = onChange)
            .padding(horizontal = LumiaTokens.space16, vertical = LumiaTokens.space14),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space16),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
            Text(label, style = LumiaTokens.textMember.style, color = LumiaTokens.ink.color)
            Text(caption, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
        }
        Switch(
            checked = on,
            onCheckedChange = null,
            // Any thumb content keeps the knob its full size when off.
            thumbContent = {},
            colors = SwitchDefaults.colors(
                checkedThumbColor = knob,
                checkedTrackColor = LumiaTokens.accent.color,
                checkedBorderColor = Color.Transparent,
                uncheckedThumbColor = knob,
                uncheckedTrackColor = LumiaTokens.borderHover.color,
                uncheckedBorderColor = Color.Transparent,
            ),
        )
    }
}

// The preview

/**
 * Four messages in #design-review drawn as a conversation draws them with
 * these settings: their layout, room, avatars, font size, pictures, cards
 * and reactions, changing as each is chosen. The rows are the
 * conversation's own (`LumiaMessageRow`, `ReactionChips`, `FileRowView`,
 * `CardFrame`) given sample messages and nothing to do: no tap, long
 * press or link opens anything (`LocalPressable`), nothing in it takes
 * focus, and it is one picture to TalkBack, named [PreviewMessage.LABEL].
 */
@Composable
private fun MessagesPreview(display: Display) {
    val shape = RoundedCornerShape(LumiaTokens.radius16)
    Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
        Text(
            "PREVIEW",
            style = LumiaTokens.textSection.style.copy(fontWeight = FontWeight(LumiaTokens.textCount.weight), letterSpacing = PreviewTracking),
            color = LumiaTokens.inkMuted.color,
            modifier = Modifier.clearAndSetSemantics {},
        )
        Column(
            Modifier
                .fillMaxWidth()
                .clip(shape)
                // The conversation's own ground, so its chips and cards show as they do there.
                .background(LumiaTokens.surface.color)
                .border(1.dp, LumiaTokens.lineStrong.color, shape)
                .clearAndSetSemantics {
                    contentDescription = PreviewMessage.LABEL
                    role = Role.Image
                },
        ) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = LumiaTokens.space16, vertical = LumiaTokens.space12),
                horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("#", style = LumiaTokens.textAuthor.style, color = LumiaTokens.inkMuted.color)
                Text(PreviewMessage.CHANNEL, style = LumiaType(LumiaTokens.textAuthor.size, LumiaTokens.textAuthor.weight, true).style, color = LumiaTokens.ink.color)
            }
            Divider()
            CompositionLocalProvider(LocalTextScale provides display.textScale, LocalPressable provides false) {
                Column(Modifier.fillMaxWidth().padding(vertical = LumiaTokens.space8)) {
                    PreviewMessage.SAMPLES.forEachIndexed { i, m ->
                        LumiaMessageRow(
                            m.author, m.tone, m.time, m.text,
                            place = PreviewMessage.PLACES[i],
                            measures = display.measures,
                            media = if (m.picture || m.card) ({ _ -> PreviewMedia(m, display) }) else null,
                            reactions = display.drawn(m.reactions).takeIf { it.isNotEmpty() }?.let { drawn ->
                                @Composable { ReactionChips(drawn, PreviewMessage.ME, { null }, null) {} }
                            },
                        )
                    }
                }
            }
        }
        Text("Kept on this phone only. Your other devices and your account are not changed.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
    }
}

/**
 * What a sample message carries, in the frame `MessageMedia` puts it in:
 * the picture while Images is on and its file's row (the conversation's
 * own, `FileRowView`) while off, and the card (`CardFrame`) while Embeds
 * is on. The picture is drawn here and not by `MessageMedia`'s tile, which
 * fetches its thumbnail from the instance and opens the viewer: a box in
 * the blue tone at the picture's size fitted to the column, a sign-in
 * screen standing in it, cut to the tile's radius as a picture is.
 */
@Composable
private fun PreviewMedia(m: PreviewMessage, display: Display) {
    Column(Modifier.fillMaxWidth().padding(top = LumiaTokens.space4), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
        if (m.picture) {
            if (mediaShape("image", display.images) == MediaShape.PICTURE) {
                PicturePlaceholder()
            } else {
                FileRowView(
                    kind = fileKindOf(PreviewMessage.PICTURE_TYPE, PreviewMessage.PICTURE_NAME),
                    plays = false,
                    name = PreviewMessage.PICTURE_NAME,
                    meta = sizeWords(PreviewMessage.PICTURE_BYTES),
                    label = attachmentLabel("image", PreviewMessage.PICTURE_NAME, "", PreviewMessage.PICTURE_BYTES, 0),
                    onOpen = null, onLongPress = null, onSave = null, onShare = null,
                )
            }
        }
        if (m.card && cardsDrawn(display.embeds)) {
            CardFrame(PreviewMessage.CARD_HOST, PreviewMessage.CARD_TITLE, PreviewMessage.CARD_DESCRIPTION, LumiaTokens.lineStrong.color)
        }
    }
}

/** The sample picture: an app's sign-in screen, a title over two fields and a button, on the blue tone. */
@Composable
private fun PicturePlaceholder() {
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val (w, h) = fitted(PreviewMessage.PICTURE_WIDTH, PreviewMessage.PICTURE_HEIGHT, maxWidth.value)
        val field = RoundedCornerShape(LumiaTokens.radius6)
        Box(
            Modifier.width(w.dp).height(h.dp).clip(RoundedCornerShape(LumiaTokens.radius12)).background(LumiaTokens.toneBlue.color),
            contentAlignment = Alignment.BottomCenter,
        ) {
            Column(
                Modifier.fillMaxWidth(0.5f).fillMaxHeight(0.85f)
                    .clip(RoundedCornerShape(topStart = LumiaTokens.radius12, topEnd = LumiaTokens.radius12))
                    .background(LumiaTokens.surface.color)
                    .padding(LumiaTokens.space12),
                verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
            ) {
                Bar(LumiaTokens.borderActive.color, Modifier.fillMaxWidth(0.6f))
                repeat(2) { Box(Modifier.fillMaxWidth().height(LumiaTokens.space18).border(1.dp, LumiaTokens.border.color, field)) }
                Box(Modifier.fillMaxWidth().height(LumiaTokens.space18).clip(field).background(LumiaTokens.accent.color))
            }
        }
    }
}
