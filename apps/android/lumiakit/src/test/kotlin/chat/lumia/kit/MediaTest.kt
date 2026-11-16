// Attachments and link cards: the composer's waiting files and when a send
// may go, which links are previewed and kept, how pictures fit and share
// a column, how Preferences' Images and Embeds draw them, those a server
// let go, and their words.
package chat.lumia.kit

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.lumia_ffi.AttachmentDraft

private fun draft(id: String, kind: String = "image", alt: String = "") =
    AttachmentDraft(id, "channel", kind, "$id.jpg", "image/jpeg", 800u, 600u, 0u, ByteArray(0), alt, 1234u)

private fun pending(id: String, ready: Boolean = false, error: String? = null, type: String = "image/jpeg") =
    PendingAttachment(id, "$id.jpg", 5000, type, draft = if (ready) draft("d-$id") else null, error = error)

class MediaTest {
    @Test
    fun aWaitingFileIsUploadingThenReadyOrRefused() {
        val uploading = pending("a")
        assertTrue(uploading.uploading)
        assertEquals("Uploading…", uploading.status)
        val ready = pending("a", ready = true)
        assertTrue(ready.ready)
        assertFalse(ready.uploading)
        // The core's size once processed, not the file's.
        assertEquals("1.2 KB", ready.status)
        val refused = pending("a", error = attachRefused("a picture that cannot fit at 640 px"))
        assertFalse(refused.uploading)
        assertFalse(refused.ready)
        assertEquals("Not attached: a picture that cannot fit at 640 px", refused.status)
    }

    @Test
    fun theKindIsTheCoresOnceProcessedAndOnlyPicturesTakeAltText() {
        assertEquals("gif", pending("a", type = "image/gif").kind)
        assertEquals("video", pending("a", type = "video/mp4").kind)
        assertEquals("audio", pending("a", type = "audio/mpeg").kind)
        assertEquals("file", pending("a", type = "application/pdf").kind)
        // A HEIC the platform could not decode the core sends as a file.
        val heic = PendingAttachment("a", "a.heic", 1, "image/heic", draft = draft("d", kind = "file"))
        assertEquals("file", heic.kind)
        assertFalse(heic.takesAlt)
        assertTrue(pending("a").takesAlt)
        assertTrue(pending("a", type = "image/gif").takesAlt)
        assertFalse(pending("a", type = "video/mp4").takesAlt)
    }

    @Test
    fun aSendGoesWithWordsOrFilesAndWaitsForUploads() {
        assertFalse(maySend("", emptyList(), over = false))
        assertTrue(maySend("hi", emptyList(), over = false))
        assertFalse(maySend("hi", emptyList(), over = true))
        // Files alone send; one still uploading lets Send be asked for, and the send waits.
        assertTrue(maySend("", listOf(pending("a", ready = true)), over = false))
        assertTrue(maySend(" ", listOf(pending("a")), over = false))
        assertTrue(waitsFor(listOf(pending("a", ready = true), pending("b"))))
        assertFalse(waitsFor(listOf(pending("a", ready = true), pending("b", error = "x"))))
        // Only refused files: nothing to send.
        assertFalse(maySend("", listOf(pending("a", error = "x")), over = false))
    }

    @Test
    fun aSendNamesTheReadyDraftsInOrderAndLeavesTheRefused() {
        val strip = listOf(pending("a", ready = true), pending("b", error = "x"), pending("c", ready = true))
        assertEquals(listOf("d-a", "d-c"), draftIds(strip))
        assertEquals(7, roomForAttachments(strip))
        assertEquals(0, roomForAttachments(List(12) { pending("$it") }))
        assertEquals(MAX_ATTACHMENTS, draftIds(List(12) { pending("$it", ready = true) }).size)
    }

    @Test
    fun theLinksKeptAreTheDraftsLessThoseDismissedAtMostThree() {
        val links = listOf("https://a.dev", "https://b.dev", "https://c.dev", "https://d.dev")
        assertEquals(links.take(3), keptLinks(links, emptySet()))
        // Putting one away lets the fourth in, as the core's three were in order.
        assertEquals(listOf("https://a.dev", "https://c.dev", "https://d.dev"), keptLinks(links, setOf("https://b.dev")))
        // Each asked about once.
        assertEquals(listOf("https://c.dev"), linksToAsk(links.take(3), emptySet(), setOf("https://a.dev", "https://b.dev")))
        assertEquals(emptyList<String>(), linksToAsk(links.take(1), setOf("https://a.dev"), emptySet()))
    }

    @Test
    fun previewsShowOnlyWhatCameInTheDraftsOrder() {
        val links = listOf("https://a.dev", "https://b.dev", "https://c.dev")
        // b's page gave no card; c's has not answered.
        val previews = mapOf("https://b.dev" to null, "https://a.dev" to "A")
        assertEquals(listOf("https://a.dev" to "A"), shownPreviews(links, emptySet(), previews))
        assertEquals(emptyList<Pair<String, String>>(), shownPreviews(links, setOf("https://a.dev"), previews))
        // A link no longer in the draft draws nothing, whatever came for it.
        assertEquals(emptyList<Pair<String, String>>(), shownPreviews(listOf("https://c.dev"), emptySet(), previews))
    }

    @Test
    fun onePictureFitsTheColumnAndFourHundredTall() {
        // Small enough: its own size.
        assertEquals(200f to 100f, fitted(200, 100, 320f))
        // Wider than the column: narrowed, its shape kept.
        assertEquals(320f to 240f, fitted(2560, 1920, 320f))
        // Tall: no more than 400, narrowed with it.
        assertEquals(200f to 400f, fitted(1000, 2000, 320f))
        // Unknown: the column at 16:9.
        assertEquals(320f to 180f, fitted(0, 0, 320f))
    }

    @Test
    fun severalPicturesAreRowsOfTwoWithAnOddOneFirst() {
        assertEquals(emptyList<Int>(), mediaRows(0))
        assertEquals(listOf(1), mediaRows(1))
        assertEquals(listOf(2), mediaRows(2))
        assertEquals(listOf(1, 2), mediaRows(3))
        assertEquals(listOf(2, 2), mediaRows(4))
        assertEquals(listOf(1, 2, 2, 2, 2), mediaRows(9))
        assertEquals(10, mediaRows(10).sum())
    }

    @Test
    fun imagesAtLinkDrawEveryAttachmentAsARowAndEmbedsAtLinkNoCards() {
        assertEquals(MediaShape.PICTURE, mediaShape("image", Shown.SHOWN))
        assertEquals(MediaShape.PICTURE, mediaShape("gif", Shown.SHOWN))
        assertEquals(MediaShape.PICTURE, mediaShape("video", Shown.SHOWN))
        assertEquals(MediaShape.FILE_ROW, mediaShape("audio", Shown.SHOWN))
        assertEquals(MediaShape.FILE_ROW, mediaShape("file", Shown.SHOWN))
        for (kind in listOf("image", "gif", "video", "audio", "file")) assertEquals(MediaShape.FILE_ROW, mediaShape(kind, Shown.LINK))
        assertTrue(cardsDrawn(Shown.SHOWN))
        assertFalse(cardsDrawn(Shown.LINK))
        // Minimal leaves both as links.
        assertEquals(Shown.LINK, Display().with(Preset.MINIMAL).images)
        assertFalse(cardsDrawn(Display().with(Preset.MINIMAL).embeds))
    }

    @Test
    fun anAttachmentSaysWhatItIsToAScreenReader() {
        assertEquals("Photo, A cat on a step", attachmentLabel("image", "IMG_1.jpg", " A cat on a step ", 1000, 0))
        assertEquals("Photo", attachmentLabel("image", "IMG_1.jpg", "", 1000, 0))
        assertEquals("GIF, Dancing", attachmentLabel("gif", "a.gif", "Dancing", 1000, 0))
        assertEquals("Video, beach.mp4, 0:12", attachmentLabel("video", "beach.mp4", "", 1000, 12_400))
        assertEquals("Audio, memo.m4a", attachmentLabel("audio", "memo.m4a", "", 1000, 0))
        assertEquals("File, report.pdf, 2.4 MB", attachmentLabel("file", "report.pdf", "", 2_500_000, 0))
    }

    @Test
    fun sizesAndLengthsInWords() {
        assertEquals("1 byte", sizeWords(1))
        assertEquals("1023 bytes", sizeWords(1023))
        assertEquals("1 KB", sizeWords(1024))
        assertEquals("340 KB", sizeWords(340 * 1024))
        assertEquals("9.5 MB", sizeWords((9.5 * 1024 * 1024).toLong()))
        assertEquals("10 MB", sizeWords(10L * 1024 * 1024))
        assertEquals("1.2 GB", sizeWords((1.2 * 1024 * 1024 * 1024).toLong()))
        assertEquals("0:07", lengthWords(7_900))
        assertEquals("12:30", lengthWords(750_000))
        assertEquals("1:02:03", lengthWords(3_723_000))
    }

    @Test
    fun aFileRowsIconFollowsItsTypeThenItsName() {
        assertEquals(FileKind.DOCUMENT, fileKindOf("application/pdf", "a.pdf"))
        assertEquals(FileKind.DOCUMENT, fileKindOf("application/octet-stream", "notes.docx"))
        assertEquals(FileKind.ARCHIVE, fileKindOf("application/zip", "a.zip"))
        assertEquals(FileKind.CODE, fileKindOf("application/octet-stream", "main.rs"))
        assertEquals(FileKind.IMAGE, fileKindOf("image/heic", "a.heic"))
        assertEquals(FileKind.OTHER, fileKindOf("application/octet-stream", "blob"))
    }

    @Test
    fun anEditThatTakesSomethingOffIsSavedEvenWithTheWordsUnchanged() {
        assertEquals(EditOutcome.Unchanged, editWithMediaOutcome("hi", "hi ", removing = 0, keepsSomething = true))
        assertEquals(EditOutcome.Save("hi"), editWithMediaOutcome("hi", "hi", removing = 1, keepsSomething = true))
        // Words emptied with a picture left: the picture is the message.
        assertEquals(EditOutcome.Save(""), editWithMediaOutcome("hi", "", removing = 0, keepsSomething = true))
        // Emptied with nothing left to carry: asks to delete.
        assertEquals(EditOutcome.Delete, editWithMediaOutcome("hi", "", removing = 1, keepsSomething = false))
        // A message of a picture alone, opened and closed: nothing.
        assertEquals(EditOutcome.Unchanged, editWithMediaOutcome("", "", removing = 0, keepsSomething = true))
        // Without attachments, as editing always was.
        assertEquals(editOutcome("a", "b"), editWithMediaOutcome("a", "b", removing = 0, keepsSomething = false))
        assertEquals(editOutcome("a", ""), editWithMediaOutcome("a", "", removing = 0, keepsSomething = false))
    }

    @Test
    fun anUploadAFullServerRefusedSaysSoOnTheFile() {
        assertEquals("This server is out of space for files.", uploadRefused("RESOURCE_EXHAUSTED", "storage", "storage quota exceeded"))
        assertEquals("This server is out of space for files.", pending("a", error = uploadRefused("RESOURCE_EXHAUSTED", "storage", "x")).status)
        // Any other refusal is the instance's words, as before.
        assertEquals("Not attached: too many uploads", uploadRefused("RESOURCE_EXHAUSTED", "uploads", "too many uploads"))
        assertEquals("Not attached: file too large", uploadRefused("PAYLOAD_TOO_LARGE", null, "file too large"))
        assertEquals("Not attached: offline", uploadRefused(null, null, "offline"))
    }

    @Test
    fun anAttachmentItsServerLetGoIsARowWhateverItsKindOrImages() {
        for (kind in listOf("image", "gif", "video", "audio", "file")) {
            assertEquals(MediaShape.EXPIRED, mediaShape(kind, Shown.SHOWN, expired = true))
            assertEquals(MediaShape.EXPIRED, mediaShape(kind, Shown.LINK, expired = true))
        }
        assertEquals(MediaShape.PICTURE, mediaShape("image", Shown.SHOWN, expired = false))
        assertEquals("No longer available", NO_LONGER_AVAILABLE)
        assertEquals("Photo, beach.jpg, No longer available", expiredLabel("image", "beach.jpg"))
        assertEquals("GIF, dance.gif, No longer available", expiredLabel("gif", "dance.gif"))
        assertEquals("Video, clip.mp4, No longer available", expiredLabel("video", "clip.mp4"))
        assertEquals("Audio, memo.m4a, No longer available", expiredLabel("audio", "memo.m4a"))
        assertEquals("File, report.pdf, No longer available", expiredLabel("file", "report.pdf"))
        assertEquals("File, No longer available", expiredLabel("file", ""))
    }
}
