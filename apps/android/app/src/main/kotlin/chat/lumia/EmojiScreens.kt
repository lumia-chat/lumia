// Server settings' Emoji section on the phone (specs/emoji.md 4, 5 and 10;
// the "Emoji on phones" board, carried into Android's back, sheets and
// document picker): the server's emoji against its limit ("Acme · 12 of
// 50"), Upload emoji, which opens the system's picker and then a screen of
// its own with the picture chosen and its name made from the file's, and
// the list, a row's actions (Rename, in place, and Remove, which asks
// first) in a sheet. Shown to whoever holds Manage emoji (the core's
// `rolesSettingsSections`). Each change answers with the snapshot, so the
// list and the count are the instance's at once; another member's phone
// hears of it as `EmojiChanged`.
package chat.lumia

import android.graphics.ImageDecoder
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import chat.lumia.kit.Chosen
import chat.lumia.kit.DrawnEmoji
import chat.lumia.kit.EMOJI_NAME_RULE
import chat.lumia.kit.EMOJI_TOO_LARGE
import chat.lumia.kit.EMOJI_PICTURE_MAX_BYTES
import chat.lumia.kit.EmojiAt
import chat.lumia.kit.LumiaEmoji
import chat.lumia.kit.LumiaErrorText
import chat.lumia.kit.LumiaInput
import chat.lumia.kit.LumiaLinkButton
import chat.lumia.kit.LumiaPrimaryButton
import chat.lumia.kit.LumiaSecondaryButton
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.REMOVE_EMOJI_LINE
import chat.lumia.kit.describe
import chat.lumia.kit.drawnEmoji
import chat.lumia.kit.emojiCount
import chat.lumia.kit.emojiFull
import chat.lumia.kit.emojiFullWords
import chat.lumia.kit.emojiNameHint
import chat.lumia.kit.emojiRefusal
import chat.lumia.kit.lumiaCode
import chat.lumia.kit.lumiaDetails
import chat.lumia.kit.lumiaMessage
import chat.lumia.kit.noEmojiWords
import chat.lumia.kit.readAtMost
import chat.lumia.kit.removeEmojiQuestion
import chat.lumia.kit.sizeWords
import chat.lumia.kit.uploadedBy
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.lumia_ffi.EmojiSummary
import uniffi.lumia_ffi.ServerSummary
import uniffi.lumia_ffi.SettingsSection
import uniffi.lumia_ffi.emojiNameFromFile
import uniffi.lumia_ffi.isEmojiName
import uniffi.lumia_ffi.rolesSectionLabel

// The boards' sizes that `tokens.json` has no name for, made from the nearest.

/** A row of the list: at least 60 high, its picture fitted in 32. */
private val EmojiRow = LumiaTokens.controlHeight + LumiaTokens.space16
private val RowEmoji = LumiaTokens.space28 + LumiaTokens.space4
/** A row's sheet: its picture fitted in 48. */
private val SheetEmoji = LumiaTokens.space40 + LumiaTokens.space8
/** The upload form's picture: 64, in a frame of 88. */
private val UploadEmoji = LumiaTokens.space40 + LumiaTokens.space24
private val UploadFrame = UploadEmoji + LumiaTokens.space24

/** What Upload emoji offers: the kinds of picture the instance makes an emoji of (specs/emoji.md 3). */
private val EMOJI_TYPES = arrayOf("image/png", "image/jpeg", "image/webp", "image/gif")

/** "Acme · 12 of 50": the server and its count against its limit, under a screen's title. */
private fun countLine(server: ServerSummary): String = "${server.name} · ${emojiCount(server.emoji.size, server.emojiLimit)}"

/** A name as the instance keeps it: trimmed and lowercased, which it does itself too. */
private fun kept(name: String): String = name.trim().lowercase()

/**
 * The Emoji section, pushed over Server settings' sections: Upload emoji
 * (off at the limit, which the line under it says), then the server's
 * emoji, oldest first, each with its picture, `:name:`, who uploaded it
 * and More. Upload opens the system's document picker, so nothing of the
 * phone's pictures is asked for but the one chosen; the picture chosen
 * opens the upload screen over this one ([UploadEmojiScreen]).
 */
@Composable
fun ServerEmojiScreen(session: LumiaSession, server: ServerSummary?, back: () -> Unit) {
    val scope = rememberCoroutineScope()
    val serverId = server?.serverId ?: ""
    val list = server?.emoji ?: emptyList()
    val drawn = remember(list) { list.mapNotNull { drawnEmoji(it, session::pictureOf) }.associateBy { it.id } }
    val full = server != null && emojiFull(list.size, server.emojiLimit)

    // The picture chosen to upload, by its address on the phone, kept through a turn of the phone.
    var chosen by rememberSaveable { mutableStateOf<String?>(null) }
    // The document picker, not the Photo Picker: the Photo Picker names every picture by an id of its
    // own ("1000000039.png"), and the name is to be made from the file's (specs/emoji.md 10). Only
    // the four kinds the instance takes are offered, Downloads and the phone's photos among the places.
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri -> if (uri != null) chosen = uri.toString() }
    fun choose() = picker.launch(EMOJI_TYPES)

    var acting by remember { mutableStateOf<String?>(null) }
    var renaming by rememberSaveable { mutableStateOf<String?>(null) }
    var renameDraft by rememberSaveable { mutableStateOf("") }
    var renameError by rememberSaveable { mutableStateOf("") }
    var renameBusy by remember { mutableStateOf(false) }
    var removing by remember { mutableStateOf<String?>(null) }
    var removeBusy by remember { mutableStateOf(false) }
    var error by rememberSaveable { mutableStateOf("") }

    val open = chosen
    if (open != null && server != null) {
        BackHandler { chosen = null }
        UploadEmojiScreen(session, server, Uri.parse(open), choose = ::choose, done = { chosen = null })
        return
    }

    fun rename(e: EmojiSummary) {
        val name = kept(renameDraft)
        renameError = ""
        if (name == e.name) { renaming = null; return }
        // The core's rule, said before anything is sent.
        if (!isEmojiName(name)) { renameError = EMOJI_NAME_RULE; return }
        scope.launch {
            renameBusy = true
            try {
                session.renameEmoji(serverId, e.emojiId, name)
                renaming = null
            } catch (ex: Exception) {
                renameError = emojiRefusal(ex.lumiaCode, ex.lumiaDetails, server?.name ?: "", name, server?.emojiLimit ?: 0u, ex.lumiaMessage).words
            } finally {
                renameBusy = false
            }
        }
    }

    fun remove(e: EmojiSummary) {
        scope.launch {
            removeBusy = true
            error = ""
            try {
                session.removeEmoji(serverId, e.emojiId)
            } catch (ex: Exception) {
                error = emojiRefusal(ex.lumiaCode, ex.lumiaDetails, server?.name ?: "", e.name, server?.emojiLimit ?: 0u, ex.lumiaMessage).words
            } finally {
                removeBusy = false
                removing = null
            }
        }
    }

    BackHandler(enabled = renaming != null) { renaming = null }
    PanelScreen(back = "Server settings", onBack = back, title = rolesSectionLabel(SettingsSection.EMOJI), subtitle = server?.let { countLine(it) }) {
        Column(
            Modifier.fillMaxSize().imePadding().verticalScroll(rememberScrollState()).navigationBarsPadding().padding(LumiaTokens.space16),
            verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
        ) {
            SmallButton("Upload emoji", Icons.Filled.Add, Modifier.fillMaxWidth(), enabled = server != null && !full) { choose() }
            if (full) Text(emojiFullWords(server!!.name, server.emojiLimit), style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
            if (error.isNotEmpty()) LumiaErrorText(error)
            if (list.isEmpty()) {
                if (server != null) Text(noEmojiWords(server.name), style = LumiaTokens.textBody.style, color = LumiaTokens.inkMuted.color)
            } else {
                val shape = RoundedCornerShape(LumiaTokens.radius14)
                Column(Modifier.fillMaxWidth().clip(shape).border(1.dp, LumiaTokens.lineStrong.color, shape)) {
                    list.forEachIndexed { i, e ->
                        if (i > 0) Box(Modifier.fillMaxWidth().height(1.dp).background(LumiaTokens.line.color))
                        if (renaming == e.emojiId) {
                            RenameRow(e, drawn[e.emojiId], renameDraft, { renameDraft = it; renameError = "" }, renameError, renameBusy, { renaming = null }) { rename(e) }
                        } else {
                            EmojiRowOf(e, drawn[e.emojiId], session.nameOf(e.createdByAccountId)) { acting = e.emojiId }
                        }
                    }
                }
            }
        }
    }

    acting?.let { id -> list.firstOrNull { it.emojiId == id } }?.let { e ->
        EmojiActionsSheet(
            e, drawn[e.emojiId], session.nameOf(e.createdByAccountId),
            onRename = { acting = null; renaming = e.emojiId; renameDraft = e.name; renameError = "" },
            onRemove = { acting = null; removing = e.emojiId },
            onDismiss = { acting = null },
        )
    }
    removing?.let { id -> list.firstOrNull { it.emojiId == id } }?.let { e ->
        AskSheet(removeEmojiQuestion(e.name), listOf(REMOVE_EMOJI_LINE), "Remove", danger = true, keep = "Keep it", busy = removeBusy, onConfirm = { remove(e) }, onDismiss = { removing = null })
    }
}

/** A row: the picture, `:name:` over who uploaded it ("· Animated" for one that plays), and More, which opens its actions. */
@Composable
private fun EmojiRowOf(e: EmojiSummary, drawn: DrawnEmoji?, by: String, onMore: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = EmojiRow).padding(start = LumiaTokens.space14, end = LumiaTokens.space4, top = LumiaTokens.space8, bottom = LumiaTokens.space8),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        EmojiBox(drawn, RowEmoji)
        Column(Modifier.weight(1f).semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(":${e.name}:", style = LumiaTokens.textMember.style.copy(fontWeight = FontWeight(LumiaTokens.textAuthor.weight)), color = LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(uploadedBy(by, e.animated), style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        androidx.compose.material3.IconButton(onClick = onMore, modifier = Modifier.size(LumiaTokens.controlHeight)) {
            Icon(Icons.Filled.MoreVert, contentDescription = "More for :${e.name}:", tint = LumiaTokens.inkSecondary.color)
        }
    }
}

/** A picture fitted in a box of [side], nothing until it is here; nothing a screen reader is told, its row naming it. */
@Composable
private fun EmojiBox(drawn: DrawnEmoji?, side: androidx.compose.ui.unit.Dp) {
    Box(Modifier.size(side).clearAndSetSemantics {}, contentAlignment = Alignment.Center) {
        if (drawn != null) LumiaEmoji(drawn, Modifier.fillMaxSize())
    }
}

/**
 * A row being renamed, in place (the boards' "Rename"): on the inset
 * ground, the name between its colons in a field that has the keyboard,
 * Cancel and Save, and under them why a name was refused.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun RenameRow(e: EmojiSummary, drawn: DrawnEmoji?, value: String, onValue: (String) -> Unit, error: String, busy: Boolean, onCancel: () -> Unit, onSave: () -> Unit) {
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    // The whole row, its Save and its line too, kept in view over the keyboard as it rises.
    val seen = remember { BringIntoViewRequester() }
    val keyboard = WindowInsets.ime.getBottom(LocalDensity.current)
    LaunchedEffect(keyboard, error) { seen.bringIntoView() }
    Row(
        Modifier.fillMaxWidth().bringIntoViewRequester(seen).background(LumiaTokens.inset.color).padding(start = LumiaTokens.space14, end = LumiaTokens.space10, top = LumiaTokens.space12, bottom = LumiaTokens.space12),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
        verticalAlignment = Alignment.Top,
    ) {
        Box(Modifier.padding(top = LumiaTokens.space24 + LumiaTokens.space4)) { EmojiBox(drawn, RowEmoji) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
            NameField("New name for :${e.name}:", value, onValue, enabled = !busy, focus = focus)
            Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
                LumiaSecondaryButton("Cancel", enabled = !busy, onClick = onCancel)
                LumiaPrimaryButton("Save", enabled = !busy && value.isNotBlank(), onClick = onSave)
            }
            if (error.isNotEmpty()) LumiaErrorText(error)
        }
    }
}

/**
 * The name's field, between quiet colons (the boards' `:` affixes): its
 * label over it, read with it; no capitals offered and no suggestions.
 */
@Composable
private fun NameField(label: String, value: String, onValue: (String) -> Unit, enabled: Boolean, focus: FocusRequester? = null) {
    val shape = RoundedCornerShape(LumiaTokens.radius8)
    Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
        Text(label, style = LumiaTokens.textLabel.style, color = LumiaTokens.inkSecondary.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Row(
            Modifier.fillMaxWidth().height(LumiaTokens.controlHeight).background(LumiaTokens.field.color, shape).border(1.dp, LumiaTokens.borderForm.color, shape).padding(horizontal = LumiaTokens.space10),
            horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space2),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(":", style = LumiaTokens.textBody.style, color = LumiaTokens.inkMuted.color, modifier = Modifier.clearAndSetSemantics {})
            LumiaInput(
                value, { onValue(it.take(NAME_TYPED_MAX)) }, "", enabled = enabled,
                keyboard = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, keyboardType = KeyboardType.Ascii),
                modifier = Modifier.weight(1f).then(if (focus != null) Modifier.focusRequester(focus) else Modifier).semantics { contentDescription = label },
            )
            Text(":", style = LumiaTokens.textBody.style, color = LumiaTokens.inkMuted.color, modifier = Modifier.clearAndSetSemantics {})
        }
    }
}

/** The most a name field takes: a name's 32, as the boards' `maxlength`. */
private const val NAME_TYPED_MAX = 32

/**
 * A row's actions, in a sheet (the boards'): its picture at 48 beside
 * `:name:` and who uploaded it, then Rename and Remove, Remove outlined in
 * the danger colour as Kick is. No Cancel: the sheet is let go of.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun EmojiActionsSheet(e: EmojiSummary, drawn: DrawnEmoji?, by: String, onRename: () -> Unit, onRemove: () -> Unit, onDismiss: () -> Unit) {
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = LumiaTokens.surface.color, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(
            Modifier.fillMaxWidth().padding(start = LumiaTokens.space24, end = LumiaTokens.space24, bottom = LumiaTokens.space24).navigationBarsPadding().semantics { paneTitle = ":${e.name}:" },
            verticalArrangement = Arrangement.spacedBy(LumiaTokens.space16),
        ) {
            Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space14), verticalAlignment = Alignment.CenterVertically) {
                EmojiBox(drawn, SheetEmoji)
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
                    Text(":${e.name}:", style = LumiaTokens.titleChannel.style, color = LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.semantics { heading() })
                    Text(uploadedBy(by, e.animated), style = LumiaTokens.textSubtitle.style, color = LumiaTokens.inkSecondary.color)
                }
            }
            Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                LumiaSecondaryButton("Rename", Modifier.fillMaxWidth(), onClick = onRename)
                DangerOutlineButton("Remove", Modifier.fillMaxWidth(), onClick = onRemove)
            }
        }
    }
}

/** An outlined button in the danger colour (the boards' Remove, as Kick is): its line and its ink. */
@Composable
private fun DangerOutlineButton(label: String, modifier: Modifier = Modifier, enabled: Boolean = true, onClick: () -> Unit) {
    val shape = RoundedCornerShape(LumiaTokens.radius8)
    Box(
        modifier.heightIn(min = LumiaTokens.controlHeight).clip(shape).background(LumiaTokens.raised.color).border(1.dp, LumiaTokens.dangerLine.color, shape)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick).padding(horizontal = LumiaTokens.space16),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, style = LumiaTokens.textButton.style, color = LumiaTokens.danger.color)
    }
}

/** The picture chosen, as the phone reads it: what the system says of the file, its bytes (none over 8 MB), and a still of it to show. */
private class Picked(val chosen: Chosen, val bytes: ByteArray?, val still: ImageBitmap?)

/**
 * Upload emoji (the "Upload emoji" phone board), over the Emoji section:
 * the picture chosen in its frame with its file's name and size and
 * "Choose another…", the name the core made from the file's name
 * (`emojiNameFromFile`) to change before Save, the line under it saying
 * how it is typed or why it was refused, and Cancel and Save. A picture
 * over 8 MB is not sent. Saved, it is the list again with the emoji in it.
 */
@Composable
private fun UploadEmojiScreen(session: LumiaSession, server: ServerSummary, uri: Uri, choose: () -> Unit, done: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var picked by remember(uri) { mutableStateOf<Picked?>(null) }
    var name by rememberSaveable(uri.toString()) { mutableStateOf<String?>(null) }
    var nameError by rememberSaveable(uri.toString()) { mutableStateOf("") }
    var pictureError by rememberSaveable(uri.toString()) { mutableStateOf("") }
    var formError by rememberSaveable(uri.toString()) { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }

    LaunchedEffect(uri) {
        val read = withContext(Dispatchers.IO) {
            runCatching {
                val chosen = describe(context, uri)
                val bytes = context.contentResolver.openInputStream(uri)?.use { readAtMost(it, EMOJI_PICTURE_MAX_BYTES) }
                val still = runCatching {
                    ImageDecoder.decodeBitmap(ImageDecoder.createSource(context.contentResolver, uri)) { decoder, info, _ ->
                        // Small: it is drawn at 64.
                        val side = maxOf(info.size.width, info.size.height).coerceAtLeast(1)
                        if (side > 256) decoder.setTargetSize(info.size.width * 256 / side, info.size.height * 256 / side)
                    }.asImageBitmap()
                }.getOrNull()
                Picked(chosen, bytes, still)
            }.getOrNull()
        }
        picked = read
        if (read == null) { pictureError = chat.lumia.kit.EMOJI_UNREADABLE; return@LaunchedEffect }
        if (name == null) name = emojiNameFromFile(read.chosen.name)
        // Over 8 MB is not sent: the instance would refuse it for its size.
        if (read.bytes == null) pictureError = EMOJI_TOO_LARGE
    }

    fun save() {
        val p = picked ?: return
        val bytes = p.bytes ?: return
        val sent = kept(name ?: "")
        nameError = ""
        pictureError = ""
        formError = ""
        if (!isEmojiName(sent)) { nameError = EMOJI_NAME_RULE; return }
        scope.launch {
            busy = true
            try {
                session.uploadEmoji(server.serverId, sent, bytes, p.chosen.mediaType)
                done()
            } catch (e: Exception) {
                val refusal = emojiRefusal(e.lumiaCode, e.lumiaDetails, server.name, sent, server.emojiLimit, e.lumiaMessage)
                when (refusal.at) {
                    EmojiAt.NAME -> nameError = refusal.words
                    EmojiAt.PICTURE -> pictureError = refusal.words
                    EmojiAt.FORM -> formError = refusal.words
                }
            } finally {
                busy = false
            }
        }
    }

    val typed = name ?: ""
    PanelScreen(back = rolesSectionLabel(SettingsSection.EMOJI), onBack = done, title = "Upload emoji", subtitle = countLine(server)) {
        Column(
            Modifier.fillMaxSize().imePadding().verticalScroll(rememberScrollState()).navigationBarsPadding().padding(LumiaTokens.space16),
        ) {
            val shape = RoundedCornerShape(LumiaTokens.radius14)
            Column(
                Modifier.fillMaxWidth().clip(shape).background(LumiaTokens.surface.color).border(1.dp, LumiaTokens.lineStrong.color, shape).padding(LumiaTokens.space16),
                verticalArrangement = Arrangement.spacedBy(LumiaTokens.space14),
            ) {
                Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space14), verticalAlignment = Alignment.CenterVertically) {
                    val frame = RoundedCornerShape(LumiaTokens.radius12)
                    Box(
                        Modifier.size(UploadFrame).clip(frame).background(LumiaTokens.inset.color)
                            .then(if (pictureError.isNotEmpty()) Modifier.border(1.dp, LumiaTokens.dangerLine.color, frame) else Modifier)
                            .clearAndSetSemantics { contentDescription = "The picture chosen" },
                        contentAlignment = Alignment.Center,
                    ) {
                        picked?.still?.let { Image(it, contentDescription = null, contentScale = ContentScale.Fit, modifier = Modifier.size(UploadEmoji)) }
                    }
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
                        Text(picked?.chosen?.name ?: "", style = LumiaTokens.textButton.style.copy(fontWeight = FontWeight(LumiaTokens.textAuthor.weight)), color = LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        picked?.chosen?.sizeBytes?.takeIf { it >= 0 }?.let { Text(sizeWords(it), style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color) }
                        LumiaLinkButton("Choose another…", enabled = !busy, onClick = choose)
                    }
                }
                if (pictureError.isNotEmpty()) LumiaErrorText(pictureError)
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
                    NameField("Name", typed, { name = it; nameError = ""; formError = "" }, enabled = !busy)
                    if (nameError.isNotEmpty()) LumiaErrorText(nameError)
                    else Text(emojiNameHint(kept(typed)), style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
                }
                if (formError.isNotEmpty()) LumiaErrorText(formError)
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8, Alignment.End)) {
                    LumiaSecondaryButton("Cancel", enabled = !busy, onClick = done)
                    LumiaPrimaryButton("Save", enabled = !busy && picked?.bytes != null && typed.isNotBlank(), onClick = ::save)
                }
            }
        }
    }
}
