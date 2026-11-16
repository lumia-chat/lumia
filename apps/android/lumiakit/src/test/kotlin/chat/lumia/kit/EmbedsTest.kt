// What plays inside a conversation: what an attachment or an embed is
// while it plays, what its row leaving does, an embed's address, origins,
// size, the height a post reports, the page that holds it and its words.
package chat.lumia.kit

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.lumia_ffi.AttachmentSummary
import uniffi.lumia_ffi.BlobSummary
import uniffi.lumia_ffi.EmbedSummary

private val blob = BlobSummary("b", 10u, ByteArray(0), ByteArray(0), ByteArray(0))

private fun attachment(kind: String, expired: Boolean = false) =
    AttachmentSummary("a", kind, "clip.mp4", "video/mp4", 640u, 360u, 12_000u, ByteArray(0), "", 1000u, blob, null, expired)

private fun embed(
    provider: String = "youtube",
    kind: String = "player",
    url: String = "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ?autoplay=1",
    needsParent: Boolean = false,
    aspect: Pair<UInt, UInt> = 16u to 9u,
    fixedHeight: UInt = 0u,
    resizes: Boolean = false,
    maxWidth: UInt = 0u,
    label: String = "Play video from YouTube",
) = EmbedSummary(provider, provider.replaceFirstChar { it.uppercase() }, kind, url, "", "", needsParent, aspect.first, aspect.second, fixedHeight, resizes, maxWidth, label)

class EmbedsTest {
    @Test
    fun onlyAVideoOrARecordingStillHeldPlays() {
        assertEquals(PlayKind.VIDEO, playKindOf(attachment("video")))
        assertEquals(PlayKind.AUDIO, playKindOf(attachment("audio")))
        assertNull(playKindOf(attachment("image")))
        assertNull(playKindOf(attachment("file")))
        // One its server let go has no blobs: never offered.
        assertNull(playKindOf(attachment("video", expired = true)))
        assertNull(playKindOf(attachment("audio", expired = true)))
    }

    @Test
    fun anEmbedPlaysByItsKind() {
        assertEquals(PlayKind.EMBED_PLAYER, playKindOf(embed()))
        assertEquals(PlayKind.EMBED_AUDIO, playKindOf(embed(kind = "audio")))
        assertEquals(PlayKind.EMBED_POST, playKindOf(embed(kind = "post")))
        assertTrue(PlayKind.EMBED_POST.isEmbed)
        assertFalse(PlayKind.VIDEO.isEmbed)
    }

    @Test
    fun aVideoPausesAnEmbedGoesBackAndAudioPlaysOn() {
        assertEquals(OffScreen.PAUSE, offScreen(PlayKind.VIDEO))
        assertEquals(OffScreen.BACK_TO_CARD, offScreen(PlayKind.EMBED_PLAYER))
        assertEquals(OffScreen.BACK_TO_CARD, offScreen(PlayKind.EMBED_POST))
        assertEquals(OffScreen.PLAYS_ON, offScreen(PlayKind.AUDIO))
        assertEquals(OffScreen.PLAYS_ON, offScreen(PlayKind.EMBED_AUDIO))
    }

    @Test
    fun theSameLinkInTwoMessagesIsTwoEmbeds() {
        val url = "https://youtu.be/dQw4w9WgXcQ"
        assertFalse(embedPlayKey("m1", url) == embedPlayKey("m2", url))
        assertFalse(attachmentPlayKey("m1", "a") == embedPlayKey("m1", "a"))
    }

    @Test
    fun twitchNamesTheInstanceAsItsParent() {
        val clip = embed(provider = "twitch", url = "https://clips.twitch.tv/embed?clip=Funny", needsParent = true)
        assertEquals("https://clips.twitch.tv/embed?clip=Funny&parent=chat.acme.dev", embedAddress(clip, "chat.acme.dev"))
        assertEquals(embed().url, embedAddress(embed(), "chat.acme.dev"))
        // A Bluesky post named by handle has no address until the handle resolves.
        assertNull(embedAddress(embed(provider = "bluesky", kind = "post", url = ""), "chat.acme.dev"))
    }

    @Test
    fun theWebViewGoesOnlyToTheEmbedsOrigins() {
        val origins = listOf("https://www.youtube-nocookie.com", "https://platform.twitter.com")
        assertTrue(isEmbedOrigin("https://www.youtube-nocookie.com/embed/x?autoplay=1", origins))
        assertTrue(isEmbedOrigin("HTTPS://Platform.Twitter.com/embed/Tweet.html?id=20", origins))
        assertFalse(isEmbedOrigin("https://www.youtube.com/watch?v=x", origins))
        assertFalse(isEmbedOrigin("https://www.youtube-nocookie.com.evil.example/", origins))
        assertFalse(isEmbedOrigin("http://www.youtube-nocookie.com/embed/x", origins))
        assertFalse(isEmbedOrigin("javascript:alert(1)", origins))
        assertFalse(isEmbedOrigin("intent://x#Intent;end", origins))
        assertEquals("http://10.0.2.2:4001", originOf("http://10.0.2.2:4001/"))
        assertEquals("https://chat.acme.dev", originOf("https://Chat.Acme.dev"))
    }

    @Test
    fun aPlayerFitsTheColumnAtItsShapeAndAtMost400Tall() {
        assertEquals(360f to 202.5f, embedSize(embed(), 360f))
        // A Short is upright: 400 tall at most, narrower to keep its shape.
        val (w, h) = embedSize(embed(aspect = 9u to 16u), 360f)
        assertEquals(400f, h)
        assertEquals(225f, w, 0.01f)
    }

    @Test
    fun audioAndPostsTakeTheirSitesHeights() {
        assertEquals(360f to 166f, embedSize(embed(provider = "soundcloud", kind = "audio", aspect = 0u to 0u, fixedHeight = 166u), 360f))
        val instagram = embed(provider = "instagram", kind = "post", aspect = 0u to 0u, fixedHeight = 640u, maxWidth = 550u)
        assertEquals(360f to 640f, embedSize(instagram, 360f))
        assertEquals(550f to 640f, embedSize(instagram, 700f))
        // A post that resizes starts at its height, then takes what it reported; one that does not ignores a report.
        val x = embed(provider = "x", kind = "post", aspect = 0u to 0u, fixedHeight = 250u, resizes = true, maxWidth = 550u)
        assertEquals(360f to 250f, embedSize(x, 360f))
        assertEquals(360f to 612f, embedSize(x, 360f, 612))
        assertEquals(360f to 640f, embedSize(instagram, 360f, 300))
    }

    @Test
    fun aPostsHeightIsReadInItsSitesOwnForm() {
        assertEquals(412, reportedHeight("bluesky", """{"height":412.4,"id":"abc"}"""))
        assertEquals(538, reportedHeight("x", """{"twttr.embed":{"id":"0","method":"twttr.private.resize","params":[{"height":538,"width":550}]}}"""))
        assertEquals(700, reportedHeight("reddit", """{"type":"resize.embed","data":700}"""))
        // Anything else is not a height: another method, another site's form, a site that does not resize, out of bounds.
        assertNull(reportedHeight("x", """{"twttr.embed":{"method":"twttr.private.rendered","params":[{"height":538}]}}"""))
        assertNull(reportedHeight("reddit", """{"type":"other","data":700}"""))
        assertNull(reportedHeight("instagram", """{"height":700}"""))
        assertNull(reportedHeight("bluesky", """{"height":0}"""))
        assertNull(reportedHeight("bluesky", """{"height":99999}"""))
        assertNull(reportedHeight("bluesky", "not json"))
    }

    @Test
    fun thePressAndTheFrameAreNamed() {
        assertEquals("Play video from YouTube, Never Gonna Give You Up", embedPressLabel(embed(), " Never Gonna Give You Up "))
        assertEquals("Show post from X", embedPressLabel(embed(provider = "x", kind = "post", label = "Show post from X"), ""))
        assertEquals("Youtube", embedFrameTitle(embed(), ""))
        assertEquals("A title", embedFrameTitle(embed(), "A title"))
    }

    @Test
    fun thePageHoldsTheIframeAloneAndPassesOnOnlyItsOwnMessages() {
        val page = embedPage("https://embed.bsky.app/embed/did:plc:abc/app.bsky.feed.post/3k?a=1&b=\"2\"", "A <post>", "t0k3n", forwards = true)
        assertTrue(page.contains("""src="https://embed.bsky.app/embed/did:plc:abc/app.bsky.feed.post/3k?a=1&amp;b=&quot;2&quot;""""))
        assertTrue(page.contains("""title="A &lt;post&gt;""""))
        assertTrue(page.contains("event.source !== frame.contentWindow || event.origin !== 'https://embed.bsky.app'"))
        assertTrue(page.contains("$EMBED_BRIDGE.report('t0k3n', data)"))
        assertTrue(page.contains("allow=\"autoplay; encrypted-media; fullscreen; picture-in-picture\""))
        // A player that does not resize has no script at all.
        assertFalse(embedPage("https://www.youtube-nocookie.com/embed/x", "x", "t", forwards = false).contains("<script>"))
    }
}
