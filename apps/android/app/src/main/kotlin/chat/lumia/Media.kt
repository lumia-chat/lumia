// Attachments and link cards on the phone (specs/media.md 3, 5 and 6):
// the composer's attach button (the system's Photo Picker for pictures
// and videos, its document picker for any file), the strip of files over
// the field with each one's state and alt text, the link previews over
// it; under a message, its pictures at their shape or in a grid, its
// videos and audio as a poster or row that plays in the system's player
// view, its files as rows with Save and Share, those its server let go as
// rows that open nothing, its cards; the full-screen
// viewer with pinch zoom; and what editing a message may take away. The
// rules are `Media.kt`'s in the kit; the platform's side of a file is
// `Attachments.kt`'s.
package chat.lumia

import android.content.Context
import android.content.Intent
import android.graphics.drawable.AnimatedImageDrawable
import android.graphics.drawable.Drawable
import android.net.Uri
import android.view.Gravity
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.MediaController
import android.widget.VideoView
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
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Share
import androidx.compose.material.icons.filled.VideoFile
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
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
import uniffi.lumia_ffi.cardLinks

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
fun MessageMedia(session: LumiaSession, attachments: List<AttachmentSummary>, cards: List<CardSummary>, display: Display, onLongPress: (() -> Unit)? = null) {
    val pictures = attachments.filter { mediaShape(it.kind, display.images, it.expired) == MediaShape.PICTURE }
    // Rows keep the message's order, those its server let go among them.
    val rows = attachments.filter { mediaShape(it.kind, display.images, it.expired) != MediaShape.PICTURE }
    val drawnCards = if (cardsDrawn(display.embeds)) cards else emptyList()
    var viewing by remember { mutableStateOf<AttachmentSummary?>(null) }
    var playing by remember { mutableStateOf<AttachmentSummary?>(null) }
    val open = { a: AttachmentSummary -> if (a.kind == "video" || a.kind == "audio") playing = a else viewing = a }
    Column(Modifier.fillMaxWidth().padding(top = LumiaTokens.space4), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
        if (pictures.isNotEmpty()) PictureGrid(session, pictures, open, onLongPress)
        for (a in rows) {
            if (a.expired) ExpiredRow(a, onLongPress) else FileRow(session, a, onOpen = if (a.kind == "file") null else ({ open(a) }), onLongPress)
        }
        for (card in drawnCards) CardView(session, card, onLongPress)
    }
    viewing?.let { a -> ImageViewer(session, a) { viewing = null } }
    playing?.let { a -> PlayerDialog(session, a) { playing = null } }
}

/** One picture at its shape fitted to the column, or several in rows of two (`mediaRows`), each cut square-ish to its tile. */
@Composable
private fun PictureGrid(session: LumiaSession, pictures: List<AttachmentSummary>, open: (AttachmentSummary) -> Unit, onLongPress: (() -> Unit)?) {
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val column = maxWidth.value
        if (pictures.size == 1) {
            val a = pictures[0]
            val (w, h) = fitted(a.width.toInt(), a.height.toInt(), column)
            PictureTile(session, a, Modifier.width(w.dp).height(h.dp), open, onLongPress)
            return@BoxWithConstraints
        }
        val gap = LumiaTokens.space4
        Column(verticalArrangement = Arrangement.spacedBy(gap)) {
            var at = 0
            for (n in mediaRows(pictures.size)) {
                val row = pictures.subList(at, at + n)
                at += n
                Row(Modifier.fillMaxWidth().height((column / 2).dp), horizontalArrangement = Arrangement.spacedBy(gap)) {
                    for (a in row) PictureTile(session, a, Modifier.weight(1f).fillMaxSize(), open, onLongPress)
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
 * symbol and its length. A tap opens the viewer or the player, a long
 * press the message's actions ([onLongPress]). A picture that cannot be
 * drawn says so with its name, as a file would.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun PictureTile(session: LumiaSession, a: AttachmentSummary, modifier: Modifier, open: (AttachmentSummary) -> Unit, onLongPress: (() -> Unit)?) {
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
                onClickLabel = if (gif && still && !played) "Play" else "Open",
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
            Box(Modifier.size(LumiaTokens.avatarLg).background(LumiaTokens.scrim.color, CircleShape), contentAlignment = Alignment.Center) {
                Icon(Icons.Filled.PlayArrow, null, tint = Color.White, modifier = Modifier.size(28.dp))
            }
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

/**
 * A file as the design's attachment row (`lu-attach`): its kind's icon on
 * a tile, its name and size, Save (the system's "save to" picker) and
 * Share. A picture, video or audio drawn as a row opens its viewer or
 * player on a tap ([onOpen]); a file is saved or shared. A long press
 * anywhere on it, Save and Share too, is the message's ([onLongPress]).
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun FileRow(session: LumiaSession, a: AttachmentSummary, onOpen: (() -> Unit)?, onLongPress: (() -> Unit)?) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument(a.mediaType.ifEmpty { "application/octet-stream" })) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch { error = saveTo(context, session, a.main, uri) }
    }
    val kind = fileKindOf(a.mediaType, a.name)
    val shape = RoundedCornerShape(LumiaTokens.radius12)
    Column {
        Row(
            Modifier.fillMaxWidth().clip(shape).background(LumiaTokens.inset.color, shape)
                .then(if (onOpen != null) Modifier.combinedClickable(onClickLabel = "Open", role = Role.Button, onLongClickLabel = onLongPress?.let { MESSAGE_ACTIONS }, onLongClick = onLongPress, onClick = onOpen) else Modifier)
                .padding(LumiaTokens.space8),
            horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space10),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val (fill, ink) = kindTone(kind)
            Box(Modifier.size(34.dp).background(fill, RoundedCornerShape(LumiaTokens.radius9)), contentAlignment = Alignment.Center) {
                Icon(if (a.kind == "audio" || a.kind == "video") Icons.Filled.PlayArrow else kindIcon(kind), null, tint = ink, modifier = Modifier.size(18.dp))
            }
            Column(
                Modifier.weight(1f).semantics(mergeDescendants = true) {
                    contentDescription = attachmentLabel(a.kind.takeIf { onOpen != null } ?: "file", a.name, a.alt, a.sizeBytes.toLong(), a.durationMs.toLong())
                    // A file's row takes no tap of its own, so the row's long press reaches it; TalkBack's is here.
                    if (onLongPress != null) onLongClick(label = MESSAGE_ACTIONS) { onLongPress(); true }
                },
            ) {
                Text(a.name, style = LumiaTokens.textAuthor.style, color = LumiaTokens.ink.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(
                    listOfNotNull(sizeWords(a.sizeBytes.toLong()), a.durationMs.takeIf { it > 0u }?.let { lengthWords(it.toLong()) }).joinToString(" · "),
                    style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color,
                )
            }
            RowButton(Icons.Filled.Download, "Save ${a.name}", onLongPress) { save.launch(a.name) }
            RowButton(Icons.Filled.Share, "Share ${a.name}", onLongPress) { scope.launch { error = share(context, session, a) } }
        }
        error?.let { LumiaErrorText(it) }
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
        Box(Modifier.size(34.dp).background(LumiaTokens.toneStone.color, RoundedCornerShape(LumiaTokens.radius9)), contentAlignment = Alignment.Center) {
            Icon(kindIcon(fileKindOf(a.mediaType, a.name)), null, tint = LumiaTokens.toneStoneInk.color, modifier = Modifier.size(18.dp))
        }
        Column(Modifier.weight(1f)) {
            Text(a.name, style = LumiaTokens.textAuthor.style, color = LumiaTokens.inkSecondary.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(NO_LONGER_AVAILABLE, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color)
        }
    }
}

/** Save or Share on a file's row, an icon button's size and touch area, whose long press is the message's. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun RowButton(icon: ImageVector, description: String, onLongPress: (() -> Unit)?, onClick: () -> Unit) {
    Box(
        Modifier.size(32.dp).minimumInteractiveComponentSize().clip(CircleShape)
            .combinedClickable(role = Role.Button, onLongClickLabel = onLongPress?.let { MESSAGE_ACTIONS }, onLongClick = onLongPress, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = description, tint = LumiaTokens.iconStrong.color, modifier = Modifier.size(18.dp))
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
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun CardView(session: LumiaSession, card: CardSummary, onLongPress: (() -> Unit)?) {
    val context = LocalContext.current
    val shape = RoundedCornerShape(LumiaTokens.radius12)
    val stripe = parseColour(card.color) ?: LumiaTokens.lineStrong.color
    Row(
        Modifier.fillMaxWidth().clip(shape).background(LumiaTokens.inset.color, shape)
            .combinedClickable(onClickLabel = "Open the link", role = Role.Button, onLongClickLabel = onLongPress?.let { MESSAGE_ACTIONS }, onLongClick = onLongPress) { openInBrowser(context, card.url) }
            .semantics(mergeDescendants = true) { contentDescription = listOf("Link preview", card.host, card.title, card.description).filter { it.isNotEmpty() }.joinToString(", ") },
    ) {
        Box(Modifier.width(3.dp).heightIn(min = 48.dp).background(stripe))
        Column(Modifier.weight(1f).padding(LumiaTokens.space10), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
            Text(card.host, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(card.title, style = LumiaTokens.textAuthor.style, color = LumiaTokens.link.color, maxLines = 2, overflow = TextOverflow.Ellipsis)
            if (card.description.isNotEmpty()) Text(card.description, style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color, maxLines = 3, overflow = TextOverflow.Ellipsis)
            if (card.imageUrl.isNotEmpty() || card.image != null) {
                val picture = rememberCardPicture(session, card)
                if (picture !is Fetched.Failed) {
                    BoxWithConstraints(Modifier.fillMaxWidth().padding(top = LumiaTokens.space6)) {
                        val (w, h) = fitted(card.imageWidth.toInt(), card.imageHeight.toInt(), maxWidth.value, 240f)
                        Box(Modifier.width(w.dp).height(h.dp).clip(RoundedCornerShape(LumiaTokens.radius8)).background(LumiaTokens.surface.color), contentAlignment = Alignment.Center) {
                            (picture as? Fetched.Here)?.let { Image(it.value, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop) }
                            if (card.video) Box(Modifier.size(LumiaTokens.avatarLg).background(LumiaTokens.scrim.color, CircleShape), contentAlignment = Alignment.Center) {
                                Icon(Icons.Filled.PlayArrow, null, tint = Color.White, modifier = Modifier.size(28.dp))
                            }
                        }
                    }
                }
            }
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
 * A video or audio full screen, in the platform's `VideoView` with its
 * media controls: the core opens the file into the app's cache (the
 * instance never holds it in the clear) and it plays from there. Never
 * before a tap.
 */
@Composable
private fun PlayerDialog(session: LumiaSession, a: AttachmentSummary, onClose: () -> Unit) {
    val context = LocalContext.current
    var uri by remember(a.attachmentId) { mutableStateOf<Uri?>(null) }
    var error by remember(a.attachmentId) { mutableStateOf<String?>(null) }
    LaunchedEffect(a.attachmentId) {
        try {
            val bytes = session.attachmentBytes(a.main)
            uri = withContext(Dispatchers.IO) {
                val dir = File(context.cacheDir, "play").apply { mkdirs() }
                val ext = a.name.substringAfterLast('.', "").takeIf { it.length in 1..5 } ?: "bin"
                Uri.fromFile(File(dir, "${a.attachmentId}.$ext").apply { if (!exists() || length() != bytes.size.toLong()) writeBytes(bytes) })
            }
        } catch (e: Exception) {
            error = "This can't be played: ${e.message ?: "the file could not be had"}"
        }
    }
    Dialog(onDismissRequest = onClose, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Box(Modifier.fillMaxSize().background(Color.Black), contentAlignment = Alignment.Center) {
            val here = uri
            when {
                error != null -> Text(error ?: "", style = LumiaTokens.textBody.style, color = Color.White, modifier = Modifier.padding(LumiaTokens.space16))
                here == null -> CircularProgressIndicator(color = Color.White)
                // The view fits itself to the video's shape inside a frame the size of the screen, centred.
                else -> AndroidView(
                    factory = { c ->
                        FrameLayout(c).apply {
                            val video = VideoView(c)
                            addView(video, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT, Gravity.CENTER))
                            val controls = MediaController(c)
                            controls.setAnchorView(video)
                            video.setMediaController(controls)
                            video.contentDescription = attachmentLabel(a)
                            video.setOnPreparedListener { video.start(); controls.show(0) }
                            video.setOnErrorListener { _, _, _ -> error = "This can't be played on this phone."; true }
                            video.setVideoURI(here)
                        }
                    },
                    onRelease = { (it.getChildAt(0) as? VideoView)?.stopPlayback() },
                    modifier = Modifier.fillMaxSize(),
                )
            }
            Row(Modifier.align(Alignment.TopStart).fillMaxWidth().statusBarsPadding().padding(horizontal = LumiaTokens.space4), verticalAlignment = Alignment.CenterVertically) {
                androidx.compose.material3.IconButton(onClick = onClose) { Icon(Icons.Filled.Close, contentDescription = "Close", tint = Color.White) }
                Text(a.name, style = LumiaTokens.textAuthor.style, color = Color.White, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
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
