// Attachments and link cards on the phone (specs/media.md 3, 5 and 6):
// the composer's attach button (the system's Photo Picker for pictures
// and videos, its document picker for any file), the strip of files over
// the field with each one's state and alt text, the link previews over
// it; under a message, its pictures at their shape or in a grid, its
// videos as a poster and its audio as a row, each playing in its place
// on a press (`Playback.kt`), its files as rows with Save and Share,
// those its server let go as rows that open nothing, its cards, and a
// card whose link has an embed loading it in its place on a press; the
// full-screen viewer with pinch zoom; and what editing a message may take
// away. The rules are `Media.kt`'s and `Embeds.kt`'s in the kit; the
// platform's side of a file is `Attachments.kt`'s.
package chat.lumia

import android.content.Context
import android.content.Intent
import android.graphics.drawable.AnimatedImageDrawable
import android.graphics.drawable.Drawable
import android.net.Uri
import android.widget.ImageView
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.InsertDriveFile
import androidx.compose.material.icons.filled.AttachFile
import androidx.compose.material.icons.filled.AudioFile
import androidx.compose.material.icons.filled.BrokenImage
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.Description
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.FolderZip
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Share
import androidx.compose.material.icons.filled.VideoFile
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onLongClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.core.content.FileProvider
import chat.lumia.kit.CARD_PAUSE_MS
import chat.lumia.kit.Display
import chat.lumia.kit.Fetched
import chat.lumia.kit.FileKind
import chat.lumia.kit.LumiaErrorText
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.MediaShape
import chat.lumia.kit.NO_LONGER_AVAILABLE
import chat.lumia.kit.PendingAttachment
import chat.lumia.kit.animationsOff
import chat.lumia.kit.attachmentInput
import chat.lumia.kit.attachmentLabel
import chat.lumia.kit.attachmentPlayKey
import chat.lumia.kit.embedPlayKey
import chat.lumia.kit.embedPressLabel
import chat.lumia.kit.embedSize
import chat.lumia.kit.playingOf
import chat.lumia.kit.cardsDrawn
import chat.lumia.kit.decodeAnimated
import chat.lumia.kit.decodePicture
import chat.lumia.kit.describe
import chat.lumia.kit.fileKindOf
import chat.lumia.kit.fitted
import chat.lumia.kit.keptLinks
import chat.lumia.kit.lengthWords
import chat.lumia.kit.linksToAsk
import chat.lumia.kit.mediaRows
import chat.lumia.kit.mediaShape
import chat.lumia.kit.placeholderPicture
import chat.lumia.kit.rememberBlobBytes
import chat.lumia.kit.rememberBlobPicture
import chat.lumia.kit.rememberCardPicture
import chat.lumia.kit.roomForAttachments
import chat.lumia.kit.shownPreviews
import chat.lumia.kit.sizeWords
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.lumia_ffi.AttachmentSummary
import uniffi.lumia_ffi.BlobSummary
import uniffi.lumia_ffi.CardSummary
import uniffi.lumia_ffi.EmbedSummary
import uniffi.lumia_ffi.cardLinks
import uniffi.lumia_ffi.embedFor

// The composer

/**
 * The link previews a composer shows (spec 6.4): once typing pauses for
 * [CARD_PAUSE_MS], the draft's links (`cardLinks`, at most three) are each
 * asked about once, the instance's card in a server channel and this
 * phone's own in a personal one, and drawn over the field with a × that
 * puts one away; the links kept go with the send ([cardUrls]).
 */
class ComposerCards {
    var links by mutableStateOf<List<String>>(emptyList())
    val previews = mutableStateMapOf<String, CardSummary?>()
    val dismissed = mutableStateMapOf<String, Boolean>()
    private val asked = mutableSetOf<String>()

    /** The links of [body] whose cards a send keeps: those not put away, whether or not a preview came (a server's instance makes a card it has not yet after the send). */
    fun cardUrls(body: String): List<String> = keptLinks(cardLinks(body), dismissed.keys)

    fun shown(): List<Pair<String, CardSummary>> = shownPreviews(links, dismissed.keys, previews)

    /** After a send: nothing previewed, nothing put away. */
    fun reset() {
        links = emptyList()
        previews.clear()
        dismissed.clear()
        asked.clear()
    }

    suspend fun follow(session: LumiaSession, channelId: String, draft: String, launch: (suspend () -> Unit) -> Unit) {
        delay(CARD_PAUSE_MS)
        val now = session.previewLinks(channelId, draft)
        links = now
        for (url in linksToAsk(now, dismissed.keys, asked)) {
            asked += url
            launch { previews[url] = session.cardPreview(channelId, url) }
        }
    }
}

@Composable
fun rememberComposerCards(session: LumiaSession, channelId: String, draft: String): ComposerCards {
    val cards = remember(channelId) { ComposerCards() }
    val scope = rememberCoroutineScope()
    LaunchedEffect(channelId, draft) { cards.follow(session, channelId, draft) { work -> scope.launch { work() } } }
    return cards
}

/**
 * The composer's attach button, a paperclip before the field: a menu of
 * "Photo or video" (the system's Photo Picker, which asks for nothing of
 * the phone's pictures but those chosen) and "File" (its document picker,
 * any file; a picture chosen there goes as a file, its pixels kept and
 * its metadata stripped). Each file goes to the core at once ([attach]).
 */
@Composable
fun AttachButton(session: LumiaSession, channelId: String) {
    val context = LocalContext.current
    val limits = session.uploadLimits ?: return
    var menu by remember { mutableStateOf(false) }
    val pending = session.attaching[channelId].orEmpty()
    fun add(uris: List<Uri>, asFiles: Boolean) {
        for (uri in uris.take(roomForAttachments(session.attaching[channelId].orEmpty()))) {
            val chosen = describe(context, uri)
            val picture = chosen.mediaType.startsWith("image/")
            val asFile = asFiles && picture
            // Past the file cap nothing but a picture (made smaller by the core) can go: said without reading it.
            if (chosen.sizeBytes > limits.fileMaxBytes.toLong() && !(picture && !asFile)) {
                session.attachFailed(channelId, chosen.name, "it is larger than ${sizeWords(limits.fileMaxBytes.toLong())}, the most this instance takes")
                continue
            }
            session.attach(channelId, chosen.name, chosen.sizeBytes.coerceAtLeast(0), chosen.mediaType) { attachmentInput(context, chosen, asFile) }
        }
    }
    val photos = rememberLauncherForActivityResult(ActivityResultContracts.PickMultipleVisualMedia(chat.lumia.kit.MAX_ATTACHMENTS)) { uris -> add(uris, false) }
    val files = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris -> add(uris, true) }
    Box {
        androidx.compose.material3.IconButton(
            onClick = { menu = true },
            enabled = roomForAttachments(pending) > 0,
            modifier = Modifier.size(LumiaTokens.controlHeight),
        ) {
            Icon(Icons.Filled.AttachFile, contentDescription = "Attach", tint = LumiaTokens.iconStrong.color)
        }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(text = { Text("Photo or video", style = LumiaTokens.textBody.style) }, leadingIcon = { Icon(Icons.Filled.Image, null) }, onClick = {
                menu = false
                photos.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageAndVideo))
            })
            DropdownMenuItem(text = { Text("File", style = LumiaTokens.textBody.style) }, leadingIcon = { Icon(Icons.AutoMirrored.Filled.InsertDriveFile, null) }, onClick = {
                menu = false
                files.launch(arrayOf("*/*"))
            })
        }
    }
}

/**
 * What the composer's frame holds over its field besides the reply bar:
 * the files waiting (each its placeholder, name, size or state, a ×, and
 * for a picture its alt text), the link previews, and while a send waits
 * for uploads ([waiting]), a line saying so.
 */
@Composable
fun ComposerStrip(session: LumiaSession, channelId: String, cards: ComposerCards, waiting: Boolean) {
    val pending = session.attaching[channelId].orEmpty()
    val previews = cards.shown()
    if (pending.isEmpty() && previews.isEmpty()) return
    Column(
        Modifier.fillMaxWidth().background(LumiaTokens.inset.color).padding(LumiaTokens.space10),
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
    ) {
        if (pending.isNotEmpty()) {
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                for (p in pending) PendingTile(p, onRemove = { session.removeAttachment(channelId, p.localId) }, onAlt = { session.setAlt(channelId, p.localId, it) })
            }
        }
        for ((url, card) in previews) PreviewRow(session, card) { cards.dismissed[url] = true }
        if (waiting) Text("Sends once the files are uploaded.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
    }
}

/** One waiting file: its placeholder or kind, name, and size, "Uploading…" or why it was refused; a × and, for a picture, its alt text. */
@Composable
private fun PendingTile(p: PendingAttachment, onRemove: () -> Unit, onAlt: (String) -> Unit) {
    val shape = RoundedCornerShape(LumiaTokens.radius12)
    Column(
        Modifier.width(220.dp).clip(shape).background(LumiaTokens.surface.color, shape).border(1.dp, LumiaTokens.line.color, shape).padding(LumiaTokens.space8),
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(LumiaTokens.avatarMd).clip(RoundedCornerShape(LumiaTokens.radius8)).background(LumiaTokens.inset.color), contentAlignment = Alignment.Center) {
                val placeholder = p.draft?.placeholder?.let { placeholderPicture(it) }
                if (placeholder != null) Image(placeholder, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop, filterQuality = FilterQuality.Low)
                else Icon(kindIcon(fileKindOf(p.draft?.mediaType ?: p.mediaType, p.name)), null, tint = LumiaTokens.iconStrong.color, modifier = Modifier.size(20.dp))
                if (p.uploading) CircularProgressIndicator(Modifier.size(20.dp), color = LumiaTokens.accent.color, strokeWidth = 2.dp)
            }
            Column(Modifier.weight(1f)) {
                Text(p.name, style = LumiaTokens.textAuthor.style, color = LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(p.status, style = LumiaTokens.textMeta.style, color = if (p.error != null) LumiaTokens.dnd.color else LumiaTokens.inkMuted.color, maxLines = 3)
            }
            androidx.compose.material3.IconButton(onClick = onRemove, modifier = Modifier.size(32.dp)) {
                Icon(Icons.Filled.Close, contentDescription = "Remove ${p.name}", tint = LumiaTokens.iconStrong.color, modifier = Modifier.size(18.dp))
            }
        }
        if (p.takesAlt && p.error == null) {
            Box(Modifier.fillMaxWidth().background(LumiaTokens.field.color, RoundedCornerShape(LumiaTokens.radius8)).border(1.dp, LumiaTokens.borderForm.color, RoundedCornerShape(LumiaTokens.radius8)).padding(horizontal = LumiaTokens.space8, vertical = LumiaTokens.space6)) {
                chat.lumia.kit.LumiaInput(p.alt, onAlt, "Alt text", modifier = Modifier.fillMaxWidth().semantics { contentDescription = "Alt text for ${p.name}" })
            }
        }
    }
}

/** A link's card over the composer: its picture, host and title, and a × that keeps it off the message. */
@Composable
private fun PreviewRow(session: LumiaSession, card: CardSummary, onDismiss: () -> Unit) {
    val shape = RoundedCornerShape(LumiaTokens.radius12)
    Row(
        Modifier.fillMaxWidth().clip(shape).background(LumiaTokens.surface.color, shape).border(1.dp, LumiaTokens.line.color, shape).padding(LumiaTokens.space8),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (card.imageUrl.isNotEmpty() || card.image != null) {
            val picture = rememberCardPicture(session, card)
            Box(Modifier.size(LumiaTokens.avatarMd).clip(RoundedCornerShape(LumiaTokens.radius8)).background(LumiaTokens.inset.color)) {
                (picture as? Fetched.Here)?.let { Image(it.value, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop) }
            }
        }
        Column(Modifier.weight(1f).semantics(mergeDescendants = true) { contentDescription = "Link preview, ${card.host}, ${card.title}" }) {
            Text(card.host, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(card.title, style = LumiaTokens.textAuthor.style, color = LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        androidx.compose.material3.IconButton(onClick = onDismiss, modifier = Modifier.size(32.dp)) {
            Icon(Icons.Filled.Close, contentDescription = "Remove the preview of ${card.host}", tint = LumiaTokens.iconStrong.color, modifier = Modifier.size(18.dp))
        }
    }
}

// A message's attachments and cards

/** What TalkBack calls a long press on a message, here as on its row. */
private const val MESSAGE_ACTIONS = "Message actions"

/**
 * What a message carries, under its words: its pictures, then its rows,
 * then its cards (none while Preferences' Embeds is Link). Each piece
 * takes its own taps, so it takes the message's long press too
 * ([onLongPress], the row's: its actions), as a long press on the words
 * does; a tap is still the piece's own, and TalkBack has the long press as
 * each one's "Message actions".
 */
@Composable
fun MessageMedia(session: LumiaSession, playback: Playback, messageId: String, attachments: List<AttachmentSummary>, cards: List<CardSummary>, display: Display, onLongPress: (() -> Unit)? = null) {
    val pictures = attachments.filter { mediaShape(it.kind, display.images, it.expired) == MediaShape.PICTURE }
    // Rows keep the message's order, those its server let go among them.
    val rows = attachments.filter { mediaShape(it.kind, display.images, it.expired) != MediaShape.PICTURE }
    val drawnCards = if (cardsDrawn(display.embeds)) cards else emptyList()
    var viewing by remember { mutableStateOf<AttachmentSummary?>(null) }
    // A video or a recording plays here, in its row (spec 5); anything else opens the viewer.
    val open = { a: AttachmentSummary -> if (playingOf(a) != null) playback.play(attachmentPlayKey(messageId, a.attachmentId), a) else viewing = a }
    // The row leaving the screen: what the core says stops then does (a video pauses, a site's player goes back to its card).
    DisposableEffect(messageId, playback) { onDispose { playback.rowGone(messageId) } }
    Column(Modifier.fillMaxWidth().padding(top = LumiaTokens.space4), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
        if (pictures.isNotEmpty()) PictureGrid(session, playback, messageId, pictures, open, onLongPress)
        for (a in rows) {
            val key = attachmentPlayKey(messageId, a.attachmentId)
            when {
                a.expired -> ExpiredRow(a, onLongPress)
                a.kind == "audio" -> AudioRow(session, playback, key, a, onLongPress)
                // Images as links: a video's row plays it in the row's place.
                a.kind == "video" && playback.attachment == key && !playback.opening -> BoxWithConstraints(Modifier.fillMaxWidth()) {
                    val (w, h) = fitted(a.width.toInt(), a.height.toInt(), maxWidth.value)
                    InlineVideo(playback, a, Modifier.width(w.dp).height(h.dp))
                }
                else -> FileRow(session, a, onOpen = if (a.kind == "file") null else ({ open(a) }), onLongPress, busy = playback.attachment == key && playback.opening)
            }
        }
        for (a in attachments) playback.errors[attachmentPlayKey(messageId, a.attachmentId)]?.let { LumiaErrorText(it) }
        for (card in drawnCards) {
            val embed = remember(card.url) { embedFor(card.url) }
            if (embed == null) CardView(session, card, onLongPress) else EmbedCard(session, playback, messageId, card, embed, onLongPress)
        }
    }
    viewing?.let { a -> ImageViewer(session, a) { viewing = null } }
}

/** One picture at its shape fitted to the column, or several in rows of two (`mediaRows`), each cut square-ish to its tile. */
@Composable
private fun PictureGrid(session: LumiaSession, playback: Playback, messageId: String, pictures: List<AttachmentSummary>, open: (AttachmentSummary) -> Unit, onLongPress: (() -> Unit)?) {
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val column = maxWidth.value
        if (pictures.size == 1) {
            val a = pictures[0]
            val (w, h) = fitted(a.width.toInt(), a.height.toInt(), column)
            PictureTile(session, playback, messageId, a, Modifier.width(w.dp).height(h.dp), open, onLongPress)
            return@BoxWithConstraints
        }
        val gap = LumiaTokens.space4
        Column(verticalArrangement = Arrangement.spacedBy(gap)) {
            var at = 0
            for (n in mediaRows(pictures.size)) {
                val row = pictures.subList(at, at + n)
                at += n
                Row(Modifier.fillMaxWidth().height((column / 2).dp), horizontalArrangement = Arrangement.spacedBy(gap)) {
                    for (a in row) PictureTile(session, playback, messageId, a, Modifier.weight(1f).fillMaxSize(), open, onLongPress)
                }
            }
        }
    }
}

/**
 * A picture, a GIF or a video's poster in its tile: the placeholder until
 * the thumbnail is here, then the thumbnail. A GIF animates in place from
 * its whole file unless the system asks for no motion, then it is its
 * first frame with a GIF badge and plays on a tap. A video has the play
 * symbol and its length, and pressed plays in its place ([InlineVideo]),
 * a spinner over its poster while its file is had. A tap opens the viewer
 * or plays, a long press the message's actions ([onLongPress]). A picture
 * that cannot be drawn says so with its name, as a file would.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun PictureTile(session: LumiaSession, playback: Playback, messageId: String, a: AttachmentSummary, modifier: Modifier, open: (AttachmentSummary) -> Unit, onLongPress: (() -> Unit)?) {
    val key = attachmentPlayKey(messageId, a.attachmentId)
    if (a.kind == "video" && playback.attachment == key && !playback.opening) {
        InlineVideo(playback, a, modifier)
        return
    }
    val busy = a.kind == "video" && playback.attachment == key
    val context = LocalContext.current
    val shape = RoundedCornerShape(LumiaTokens.radius12)
    val still = remember { animationsOff(context) }
    var played by remember(a.attachmentId) { mutableStateOf(false) }
    val gif = a.kind == "gif"
    val animate = gif && (!still || played)
    val thumbnail = rememberBlobPicture(session, a.thumbnail)
    val placeholder = remember(a.attachmentId) { placeholderPicture(a.placeholder) }
    Box(
        modifier
            .clip(shape)
            .background(LumiaTokens.inset.color)
            .semantics(mergeDescendants = true) {
                contentDescription = attachmentLabel(a)
                role = Role.Button
            }
            .combinedClickable(
                onClickLabel = if ((gif && still && !played) || a.kind == "video") "Play" else "Open",
                onLongClickLabel = onLongPress?.let { MESSAGE_ACTIONS },
                onLongClick = onLongPress,
            ) { if (gif && still && !played) played = true else open(a) },
        contentAlignment = Alignment.Center,
    ) {
        if (placeholder != null) Image(placeholder, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop, filterQuality = FilterQuality.Low)
        when (thumbnail) {
            is Fetched.Here -> Image(thumbnail.value, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
            Fetched.Failed -> if (a.kind != "video") Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.padding(LumiaTokens.space8)) {
                Icon(Icons.Filled.BrokenImage, null, tint = LumiaTokens.iconStrong.color)
                Text(a.name, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
            Fetched.Waiting -> Unit
        }
        if (animate) AnimatedGif(session, a.main, Modifier.fillMaxSize())
        if (gif && !animate) Badge("GIF", Modifier.align(Alignment.BottomStart).padding(LumiaTokens.space6))
        if (a.kind == "video") {
            PlaySymbol(busy)
            if (a.durationMs > 0u) Badge(lengthWords(a.durationMs.toLong()), Modifier.align(Alignment.BottomStart).padding(LumiaTokens.space6))
        }
    }
}

/** A small label over a picture: "GIF", a video's length. */
@Composable
private fun Badge(text: String, modifier: Modifier) {
    Text(
        text,
        style = LumiaTokens.textCount.style,
        color = Color.White,
        modifier = modifier.background(LumiaTokens.scrim.color, RoundedCornerShape(LumiaTokens.radius6)).padding(horizontal = LumiaTokens.space6, vertical = LumiaTokens.space2),
    )
}

/** A GIF's whole file, animating, once it is here; nothing before (the thumbnail under it shows). */
@Composable
private fun AnimatedGif(session: LumiaSession, blob: BlobSummary, modifier: Modifier, contentScale: ImageView.ScaleType = ImageView.ScaleType.CENTER_CROP) {
    val bytes = rememberBlobBytes(session, blob)
    var drawable by remember(blob.blobId) { mutableStateOf<Drawable?>(null) }
    LaunchedEffect(bytes) {
        val here = (bytes as? Fetched.Here)?.value ?: return@LaunchedEffect
        drawable = withContext(Dispatchers.Default) { decodeAnimated(here) }
    }
    val d = drawable ?: return
    AndroidView(
        factory = { c -> ImageView(c).apply { scaleType = contentScale; importantForAccessibility = ImageView.IMPORTANT_FOR_ACCESSIBILITY_NO } },
        update = { view -> if (view.drawable !== d) { view.setImageDrawable(d); (d as? AnimatedImageDrawable)?.start() } },
        modifier = modifier,
    )
}

/** The icon a file row and a waiting file show for their kind. */
private fun kindIcon(kind: FileKind): ImageVector = when (kind) {
    FileKind.IMAGE -> Icons.Filled.Image
    FileKind.VIDEO -> Icons.Filled.VideoFile
    FileKind.AUDIO -> Icons.Filled.AudioFile
    FileKind.DOCUMENT -> Icons.Filled.Description
    FileKind.ARCHIVE -> Icons.Filled.FolderZip
    FileKind.CODE -> Icons.Filled.Code
    FileKind.OTHER -> Icons.AutoMirrored.Filled.InsertDriveFile
}

/** A file row's tile, the boards' 34, and the glyph on it and on Save and Share, 18. */
private val FileTile = LumiaTokens.avatarMd
private val FileGlyph = LumiaTokens.space18

/** A file's size and, for one with a length, that: "2.4 MB · 0:12". */
private fun fileMeta(a: AttachmentSummary): String =
    listOfNotNull(sizeWords(a.sizeBytes.toLong()), a.durationMs.takeIf { it > 0u }?.let { lengthWords(it.toLong()) }).joinToString(" · ")

/**
 * A file as the design's attachment row (`lu-attach`): its kind's icon on
 * a tile, its name and size, Save (the system's "save to" picker) and
 * Share, and under it why one of them failed. A picture, video or audio
 * drawn as a row opens its viewer or player on a tap ([onOpen]); a file
 * is saved or shared. A long press anywhere on it, Save and Share too, is
 * the message's ([onLongPress]). A recording's row plays in itself
 * ([AudioRow]): [playing], what its press does ([press]), where it is in
 * place of its size ([meta]) and its position bar under its words ([more]).
 */
@Composable
private fun FileRow(
    session: LumiaSession,
    a: AttachmentSummary,
    onOpen: (() -> Unit)?,
    onLongPress: (() -> Unit)?,
    busy: Boolean = false,
    playing: Boolean = false,
    press: String = "Open",
    meta: String = fileMeta(a),
    more: @Composable ColumnScope.() -> Unit = {},
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument(a.mediaType.ifEmpty { "application/octet-stream" })) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch { error = saveTo(context, session, a.main, uri) }
    }
    Column {
        FileRowView(
            kind = fileKindOf(a.mediaType, a.name),
            plays = a.kind == "audio" || a.kind == "video",
            name = a.name,
            meta = meta,
            label = attachmentLabel(a.kind.takeIf { onOpen != null } ?: "file", a.name, a.alt, a.sizeBytes.toLong(), a.durationMs.toLong()) + if (playing) ", playing" else "",
            onOpen = onOpen,
            onLongPress = onLongPress,
            onSave = { save.launch(a.name) },
            onShare = { scope.launch { error = share(context, session, a) } },
            busy = busy,
            playing = playing,
            press = press,
            more = more,
        )
        error?.let { LumiaErrorText(it) }
    }
}

/**
 * A file's row as drawn, what [FileRow] does left to it: [kind]'s icon on
 * its tile (the play glyph for audio or video, which [plays], the pause
 * glyph while it is [playing], a spinner while it is [busy]), [name] and
 * [meta] under it, [more] under those, Save and Share. A tap is [onOpen],
 * named [press] to TalkBack. With no [onOpen], [onSave] and [onShare]
 * nothing in it can be pressed: Preferences' preview draws the row this
 * way, a picture of a file's row.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun FileRowView(
    kind: FileKind,
    plays: Boolean,
    name: String,
    meta: String,
    label: String,
    onOpen: (() -> Unit)?,
    onLongPress: (() -> Unit)?,
    onSave: (() -> Unit)?,
    onShare: (() -> Unit)?,
    busy: Boolean = false,
    playing: Boolean = false,
    press: String = "Open",
    more: @Composable ColumnScope.() -> Unit = {},
) {
    val shape = RoundedCornerShape(LumiaTokens.radius12)
    Row(
        Modifier.fillMaxWidth().clip(shape).background(LumiaTokens.inset.color, shape)
            .then(if (onOpen != null) Modifier.combinedClickable(onClickLabel = press, role = Role.Button, onLongClickLabel = onLongPress?.let { MESSAGE_ACTIONS }, onLongClick = onLongPress, onClick = onOpen) else Modifier)
            .padding(LumiaTokens.space8),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space10),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val (fill, ink) = kindTone(kind)
        Box(Modifier.size(FileTile).background(fill, RoundedCornerShape(LumiaTokens.radius9)), contentAlignment = Alignment.Center) {
            if (busy) CircularProgressIndicator(Modifier.size(FileGlyph), color = ink, strokeWidth = LumiaTokens.space2)
            else Icon(if (playing) Icons.Filled.Pause else if (plays) Icons.Filled.PlayArrow else kindIcon(kind), null, tint = ink, modifier = Modifier.size(FileGlyph))
        }
        Column(Modifier.weight(1f)) {
            Column(
                Modifier.semantics(mergeDescendants = true) {
                    contentDescription = label
                    // A file's row takes no tap of its own, so the row's long press reaches it; TalkBack's is here.
                    if (onLongPress != null) onLongClick(label = MESSAGE_ACTIONS) { onLongPress(); true }
                },
            ) {
                Text(name, style = LumiaTokens.textAuthor.style, color = LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(meta, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
            }
            more()
        }
        RowButton(Icons.Filled.Download, "Save $name", onLongPress, onSave)
        RowButton(Icons.Filled.Share, "Share $name", onLongPress, onShare)
    }
}

/** A file row's tile colour and its icon's, by its kind; the PDF tone for documents. */
@Composable
private fun kindTone(kind: FileKind): Pair<Color, Color> = when (kind) {
    FileKind.DOCUMENT -> LumiaTokens.filePdf.color to LumiaTokens.filePdfInk.color
    FileKind.IMAGE -> LumiaTokens.toneBlue.color to LumiaTokens.toneBlueInk.color
    FileKind.VIDEO -> LumiaTokens.tonePurple.color to LumiaTokens.tonePurpleInk.color
    FileKind.AUDIO -> LumiaTokens.toneGreen.color to LumiaTokens.toneGreenInk.color
    FileKind.ARCHIVE -> LumiaTokens.toneAmber.color to LumiaTokens.toneAmberInk.color
    else -> LumiaTokens.toneStone.color to LumiaTokens.toneStoneInk.color
}

/**
 * An attachment its server let go (specs/media.md 4.5), whatever its
 * kind: the file row's shape with its kind's icon on the stone tile, its
 * name and "No longer available". It fetches nothing, not even a
 * thumbnail, and has no Save, Share, viewer or player; the message keeps
 * its long press here as anywhere on it ([onLongPress], TalkBack's too).
 */
@Composable
private fun ExpiredRow(a: AttachmentSummary, onLongPress: (() -> Unit)?) {
    val shape = RoundedCornerShape(LumiaTokens.radius12)
    Row(
        Modifier.fillMaxWidth().clip(shape).background(LumiaTokens.inset.color, shape)
            .semantics(mergeDescendants = true) {
                contentDescription = attachmentLabel(a)
                if (onLongPress != null) onLongClick(label = MESSAGE_ACTIONS) { onLongPress(); true }
            }
            .padding(LumiaTokens.space8),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space10),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(FileTile).background(LumiaTokens.toneStone.color, RoundedCornerShape(LumiaTokens.radius9)), contentAlignment = Alignment.Center) {
            Icon(kindIcon(fileKindOf(a.mediaType, a.name)), null, tint = LumiaTokens.toneStoneInk.color, modifier = Modifier.size(FileGlyph))
        }
        Column(Modifier.weight(1f)) {
            Text(a.name, style = LumiaTokens.textAuthor.style, color = LumiaTokens.inkSecondary.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(NO_LONGER_AVAILABLE, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
        }
    }
}

/**
 * Save or Share on a file's row, an icon button's size and touch area, whose long press is the message's.
 * With no [onClick], its glyph in the same room and nothing to press (Preferences' preview).
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun RowButton(icon: ImageVector, description: String, onLongPress: (() -> Unit)?, onClick: (() -> Unit)?) {
    Box(
        Modifier.size(32.dp).minimumInteractiveComponentSize().clip(CircleShape)
            .then(if (onClick != null) Modifier.combinedClickable(role = Role.Button, onLongClickLabel = onLongPress?.let { MESSAGE_ACTIONS }, onLongClick = onLongPress, onClick = onClick) else Modifier),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = description.takeIf { onClick != null }, tint = LumiaTokens.iconStrong.color, modifier = Modifier.size(FileGlyph))
    }
}

/** Writes an attachment's file where the person chose; the words of a failure, or none. */
private suspend fun saveTo(context: Context, session: LumiaSession, blob: BlobSummary, uri: Uri): String? = try {
    val bytes = session.attachmentBytes(blob)
    withContext(Dispatchers.IO) { context.contentResolver.openOutputStream(uri)?.use { it.write(bytes) } }
    null
} catch (e: Exception) {
    "Not saved: ${e.message ?: "the file could not be had"}"
}

/** A file of the app's cache for another app to read, by the app's provider: the attachment opened, under its own name. */
private suspend fun cached(context: Context, session: LumiaSession, a: AttachmentSummary, folder: String): Uri {
    val bytes = session.attachmentBytes(a.main)
    val file = withContext(Dispatchers.IO) {
        val dir = File(context.cacheDir, "$folder/${a.attachmentId}").apply { mkdirs() }
        val safe = a.name.replace(Regex("[/\\\\:]"), "_").ifEmpty { "attachment" }
        File(dir, safe).apply { writeBytes(bytes) }
    }
    return FileProvider.getUriForFile(context, "${context.packageName}.files", file)
}

/** Hands an attachment to the system's share sheet; the words of a failure, or none. */
private suspend fun share(context: Context, session: LumiaSession, a: AttachmentSummary): String? = try {
    val uri = cached(context, session, a, "shared")
    val send = Intent(Intent.ACTION_SEND).apply {
        type = a.mediaType.ifEmpty { "application/octet-stream" }
        putExtra(Intent.EXTRA_STREAM, uri)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    context.startActivity(Intent.createChooser(send, a.name))
    null
} catch (e: Exception) {
    "Not shared: ${e.message ?: "the file could not be had"}"
}

/**
 * A link's card (spec 6): its site by the link's host (never the page's
 * own name for itself), its title and description, its picture with a
 * play symbol for a video page; a stripe in the page's colour. A tap
 * opens the link in the browser, a long press the message's actions.
 *
 * A card whose link has an embed ([embed], the core's from the link, spec
 * 6.6) is pressed to load it instead ([onPress]): a play button over its
 * picture for a player or audio, "Show post" under it for a post, a
 * spinner while a Bluesky handle resolves ([busy]), and the words of a
 * failure under it ([error]). The press is a button named by the core's
 * label and the card's title.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun CardView(
    session: LumiaSession,
    card: CardSummary,
    onLongPress: (() -> Unit)?,
    embed: EmbedSummary? = null,
    busy: Boolean = false,
    error: String? = null,
    onPress: (() -> Unit)? = null,
) {
    val context = LocalContext.current
    val stripe = parseColour(card.color) ?: LumiaTokens.lineStrong.color
    val plays = embed != null && embed.kind != "post"
    Column {
        CardFrame(
            card.host, card.title, card.description, stripe,
            Modifier
                .combinedClickable(
                    onClickLabel = embed?.label ?: "Open the link",
                    role = Role.Button,
                    onLongClickLabel = onLongPress?.let { MESSAGE_ACTIONS },
                    onLongClick = onLongPress,
                ) { (onPress ?: { openInBrowser(context, card.url) })() }
                .semantics(mergeDescendants = true) {
                    contentDescription = if (embed != null) embedPressLabel(embed, card.title)
                    else listOf("Link preview", card.host, card.title, card.description).filter { it.isNotEmpty() }.joinToString(", ")
                },
        ) {
            var pictured = false
            if (card.imageUrl.isNotEmpty() || card.image != null) {
                val picture = rememberCardPicture(session, card)
                if (picture !is Fetched.Failed) {
                    pictured = true
                    BoxWithConstraints(Modifier.fillMaxWidth().padding(top = LumiaTokens.space6)) {
                        val (w, h) = fitted(card.imageWidth.toInt(), card.imageHeight.toInt(), maxWidth.value, 240f)
                        Box(Modifier.width(w.dp).height(h.dp).clip(RoundedCornerShape(LumiaTokens.radius8)).background(LumiaTokens.surface.color), contentAlignment = Alignment.Center) {
                            (picture as? Fetched.Here)?.let { Image(it.value, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop) }
                            if (card.video || plays) PlaySymbol(busy)
                        }
                    }
                }
            }
            // A post, or a player with no picture to put its button on: the press as words under the card.
            if (embed != null && (!plays || !pictured)) {
                Row(
                    Modifier.padding(top = LumiaTokens.space6).background(LumiaTokens.surface.color, RoundedCornerShape(LumiaTokens.radius8))
                        .border(1.dp, LumiaTokens.line.color, RoundedCornerShape(LumiaTokens.radius8))
                        .padding(horizontal = LumiaTokens.space10, vertical = LumiaTokens.space6),
                    horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space6),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    if (busy) CircularProgressIndicator(Modifier.size(LumiaTokens.space14), color = LumiaTokens.accent.color, strokeWidth = LumiaTokens.space2)
                    else if (plays) Icon(Icons.Filled.PlayArrow, null, tint = LumiaTokens.iconStrong.color, modifier = Modifier.size(LumiaTokens.space16))
                    Text(if (plays) "Play" else "Show post", style = LumiaTokens.textAuthor.style, color = LumiaTokens.ink.color)
                }
            }
        }
        error?.let { LumiaErrorText(it) }
    }
}

/**
 * A card as drawn, what [CardView] does left to it: the [stripe], then
 * [host], [title] in the link ink and [description], and [more] under
 * them (the picture, the embed's press). [modifier] is what it does when
 * pressed; with none it is a picture of a card (Preferences' preview).
 */
@Composable
internal fun CardFrame(host: String, title: String, description: String, stripe: Color, modifier: Modifier = Modifier, more: @Composable ColumnScope.() -> Unit = {}) {
    val shape = RoundedCornerShape(LumiaTokens.radius12)
    Row(Modifier.fillMaxWidth().clip(shape).background(LumiaTokens.inset.color, shape).then(modifier)) {
        Box(Modifier.width(3.dp).heightIn(min = 48.dp).background(stripe))
        Column(Modifier.weight(1f).padding(LumiaTokens.space10), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
            Text(host, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(title, style = LumiaTokens.textAuthor.style, color = LumiaTokens.link.color, maxLines = 2, overflow = TextOverflow.Ellipsis)
            if (description.isNotEmpty()) Text(description, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color, maxLines = 3, overflow = TextOverflow.Ellipsis)
            more()
        }
    }
}

/** The play symbol over a picture or a poster (the boards' 28 in a circle of 40), a spinner in it while what it plays is had. */
@Composable
private fun PlaySymbol(busy: Boolean) {
    Box(Modifier.size(LumiaTokens.avatarLg).background(LumiaTokens.scrim.color, CircleShape), contentAlignment = Alignment.Center) {
        if (busy) CircularProgressIndicator(Modifier.size(LumiaTokens.space28), color = Color.White, strokeWidth = LumiaTokens.space2)
        else Icon(Icons.Filled.PlayArrow, null, tint = Color.White, modifier = Modifier.size(LumiaTokens.space28))
    }
}

/**
 * A card with an embed (spec 6.6): the card until it is pressed, then the
 * site's embed in its place at its size ([embedSize]): a player at the
 * column's width at its shape, at most 400 tall; audio at its site's
 * height; a post at most 550 wide, its site's height or, for X, Bluesky
 * and Reddit, the height it reports. One shown at a time in a
 * conversation ([Playback]); a Bluesky post whose handle Bluesky has no
 * account for says so, and its card then opens the browser; one Bluesky
 * could not be asked about says that, and its next press asks again.
 */
@Composable
private fun EmbedCard(session: LumiaSession, playback: Playback, messageId: String, card: CardSummary, embed: EmbedSummary, onLongPress: (() -> Unit)?) {
    val context = LocalContext.current
    val key = embedPlayKey(messageId, card.url)
    val shown = playback.embed == key && playback.address != null
    if (!shown) {
        val note = playback.notes[key]
        CardView(session, card, onLongPress, embed, busy = playback.embed == key && playback.resolving, error = note?.words) {
            // Word that there is no such post, and the card is its link; anything else, and the press is for the embed (again).
            if (note?.again == false) openInBrowser(context, card.url) else playback.show(key, embed)
        }
        return
    }
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val (w, h) = embedSize(embed, maxWidth.value, playback.reported)
        Box(Modifier.width(w.dp).height(h.dp).background(LumiaTokens.inset.color, RoundedCornerShape(LumiaTokens.radius12))) {
            AndroidView(
                factory = { c -> playback.embedView(c, key, embed, card.title) },
                onRelease = { playback.released(it) },
                modifier = Modifier.fillMaxSize(),
            )
        }
    }
}

/** A page's `#rrggbb`, or none. */
private fun parseColour(hex: String): Color? {
    if (!Regex("#[0-9a-fA-F]{6}").matches(hex)) return null
    return Color(0xFF000000 or hex.substring(1).toLong(16))
}

// The viewer and the player

/**
 * A picture full screen, on black: its display picture (the core fetches
 * and opens it now), pinch to zoom and drag, a double tap back to its
 * size; Save and Share over it, its alt text to a screen reader and under
 * it. A GIF animates here unless the system asks for no motion.
 */
@Composable
private fun ImageViewer(session: LumiaSession, a: AttachmentSummary, onClose: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }
    val bytes = rememberBlobBytes(session, a.main)
    var picture by remember(a.attachmentId) { mutableStateOf<ImageBitmap?>(null) }
    var undrawable by remember(a.attachmentId) { mutableStateOf(false) }
    LaunchedEffect(bytes) {
        val here = (bytes as? Fetched.Here)?.value ?: return@LaunchedEffect
        picture = withContext(Dispatchers.Default) { decodePicture(here) }
        undrawable = picture == null
    }
    val thumbnail = rememberBlobPicture(session, a.thumbnail)
    var scale by remember { mutableStateOf(1f) }
    var offsetX by remember { mutableStateOf(0f) }
    var offsetY by remember { mutableStateOf(0f) }
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument(a.mediaType.ifEmpty { "application/octet-stream" })) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch { error = saveTo(context, session, a.main, uri) }
    }
    Dialog(onDismissRequest = onClose, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Box(Modifier.fillMaxSize().background(Color.Black)) {
            Box(
                Modifier.fillMaxSize()
                    .pointerInputs(onDoubleTap = { scale = 1f; offsetX = 0f; offsetY = 0f }) { pan, zoom ->
                        scale = (scale * zoom).coerceIn(1f, 6f)
                        if (scale == 1f) { offsetX = 0f; offsetY = 0f } else { offsetX += pan.x; offsetY += pan.y }
                    }
                    .graphicsLayer { scaleX = scale; scaleY = scale; translationX = offsetX; translationY = offsetY }
                    .semantics { contentDescription = attachmentLabel(a) },
                contentAlignment = Alignment.Center,
            ) {
                val shown = picture ?: (thumbnail as? Fetched.Here)?.value
                if (a.kind == "gif" && !animationsOff(context)) {
                    if (shown != null) Image(shown, null, Modifier.fillMaxSize(), contentScale = ContentScale.Fit)
                    AnimatedGif(session, a.main, Modifier.fillMaxSize(), ImageView.ScaleType.FIT_CENTER)
                } else if (shown != null) {
                    Image(shown, null, Modifier.fillMaxSize(), contentScale = ContentScale.Fit)
                }
                if (picture == null && !undrawable && bytes !is Fetched.Failed) CircularProgressIndicator(color = Color.White)
            }
            Row(
                Modifier.fillMaxWidth().statusBarsPadding().background(LumiaTokens.scrim.color).padding(horizontal = LumiaTokens.space4),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                androidx.compose.material3.IconButton(onClick = onClose) { Icon(Icons.Filled.Close, contentDescription = "Close", tint = Color.White) }
                Text(a.name, style = LumiaTokens.textAuthor.style, color = Color.White, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                androidx.compose.material3.IconButton(onClick = { save.launch(a.name) }) { Icon(Icons.Filled.Download, contentDescription = "Save", tint = Color.White) }
                androidx.compose.material3.IconButton(onClick = { scope.launch { error = share(context, session, a) } }) { Icon(Icons.Filled.Share, contentDescription = "Share", tint = Color.White) }
            }
            val words = listOfNotNull(
                a.alt.takeIf { it.isNotBlank() },
                error,
                "This picture can't be shown.".takeIf { undrawable || bytes is Fetched.Failed },
            )
            if (words.isNotEmpty()) {
                Column(Modifier.align(Alignment.BottomCenter).fillMaxWidth().background(LumiaTokens.scrim.color).navigationBarsPadding().padding(LumiaTokens.space16)) {
                    for (w in words) Text(w, style = LumiaTokens.textBody.style, color = Color.White, modifier = Modifier.clearAndSetSemantics {})
                }
            }
        }
    }
}

/** Pinch and drag, and a double tap, on the viewer's picture. */
private fun Modifier.pointerInputs(onDoubleTap: () -> Unit, onTransform: (androidx.compose.ui.geometry.Offset, Float) -> Unit): Modifier =
    this
        .then(Modifier.pointerInput(Unit) { detectTransformGestures { _, pan, zoom, _ -> onTransform(pan, zoom) } })
        .then(Modifier.pointerInput(Unit) { detectTapGestures(onDoubleTap = { onDoubleTap() }) })

/**
 * An attached video playing in its row (spec 5): Media3's view over the
 * conversation's one player, with the platform's controls and its
 * full-screen button, which draws the same player full screen
 * ([Playback.fullScreen]) and back.
 */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
@Composable
private fun InlineVideo(playback: Playback, a: AttachmentSummary, modifier: Modifier) {
    val shape = RoundedCornerShape(LumiaTokens.radius12)
    val player = playback.player ?: return
    Box(modifier.clip(shape).background(Color.Black)) {
        AndroidView(
            factory = { c ->
                playerView(c).apply {
                    describePlayer(this, a)
                    setFullscreenButtonClickListener { playback.fullScreen = true }
                }
            },
            update = { view ->
                // Full screen takes the player; back in the row, the button reads "Enter fullscreen" again.
                val target = if (playback.fullScreen) null else player
                if (view.player !== target) {
                    view.player = target
                    // A paused video back on screen draws its frame on the new surface.
                    if (target != null && !target.isPlaying && target.playbackState == androidx.media3.common.Player.STATE_READY) target.seekTo(target.currentPosition)
                }
                view.setFullscreenButtonState(playback.fullScreen)
            },
            onRelease = { it.player = null },
            modifier = Modifier.fillMaxSize(),
        )
    }
}

/**
 * An attached recording as the design's attachment row, playing in it
 * (spec 5), with Preferences' Images as links too: a file's row
 * ([FileRow]) whose tile is Play and Pause, and while it is the one
 * playing, a bar for its position and its time in place of its size. It
 * plays on as the list scrolls, until another starts or the conversation
 * is left. Save, Share and the long press are a file row's.
 */
@Composable
private fun AudioRow(session: LumiaSession, playback: Playback, key: String, a: AttachmentSummary, onLongPress: (() -> Unit)?) {
    val active = playback.attachment == key
    val had = active && !playback.opening
    val playing = active && playback.playing
    var position by remember(key) { mutableLongStateOf(0L) }
    var length by remember(key) { mutableLongStateOf(a.durationMs.toLong()) }
    LaunchedEffect(active, playing) {
        while (active) {
            playback.player?.let { p ->
                position = p.currentPosition.coerceAtLeast(0)
                if (p.duration > 0) length = p.duration
            }
            if (!playing) break
            delay(250)
        }
    }
    FileRow(
        session, a,
        onOpen = { if (had) playback.toggle() else if (!active) playback.play(key, a) },
        onLongPress = onLongPress,
        busy = active && playback.opening,
        playing = playing,
        press = if (playing) "Pause" else "Play",
        meta = if (had) "${lengthWords(position)} / ${lengthWords(length)}" else fileMeta(a),
    ) {
        if (had && length > 0) {
            Slider(
                value = position.coerceAtMost(length).toFloat(),
                onValueChange = { position = it.toLong(); playback.player?.seekTo(it.toLong()) },
                valueRange = 0f..length.toFloat(),
                colors = SliderDefaults.colors(thumbColor = LumiaTokens.accent.color, activeTrackColor = LumiaTokens.accent.color, inactiveTrackColor = LumiaTokens.line.color),
                modifier = Modifier.fillMaxWidth().height(LumiaTokens.controlHeight).semantics { contentDescription = "Position in ${a.name}" },
            )
        }
    }
}

// Editing

/**
 * Under a message being edited: what it carries, each with a × that takes
 * it off when the edit is saved and Undo that keeps it (spec 4.4: an edit
 * may remove attachments and cards, never add them).
 */
@Composable
fun EditMedia(attachments: List<AttachmentSummary>, cards: List<CardSummary>, removedAttachments: Set<String>, removedCards: Set<String>, onAttachment: (String) -> Unit, onCard: (String) -> Unit) {
    if (attachments.isEmpty() && cards.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
        for (a in attachments) EditMediaRow(a.name, kindIcon(fileKindOf(a.mediaType, a.name)), a.attachmentId in removedAttachments) { onAttachment(a.attachmentId) }
        for (c in cards) EditMediaRow("${c.host}: ${c.title}", Icons.Filled.Description, c.url in removedCards) { onCard(c.url) }
    }
}

@Composable
private fun EditMediaRow(name: String, icon: ImageVector, removed: Boolean, onToggle: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().background(LumiaTokens.inset.color, RoundedCornerShape(LumiaTokens.radius8)).padding(start = LumiaTokens.space8),
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, null, tint = LumiaTokens.iconStrong.color, modifier = Modifier.size(16.dp))
        Text(
            name, style = LumiaTokens.textMeta.style, color = if (removed) LumiaTokens.inkQuiet.color else LumiaTokens.ink.color,
            textDecoration = if (removed) TextDecoration.LineThrough else null, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
        )
        if (removed) chat.lumia.kit.LumiaLinkButton("Undo", onClick = onToggle)
        else androidx.compose.material3.IconButton(onClick = onToggle, modifier = Modifier.size(32.dp)) {
            Icon(Icons.Filled.Close, contentDescription = "Remove $name", tint = LumiaTokens.iconStrong.color, modifier = Modifier.size(16.dp))
        }
    }
}
