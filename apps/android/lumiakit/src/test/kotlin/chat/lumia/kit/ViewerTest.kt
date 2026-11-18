// The picture viewer's zoom: the phone's side of the core's rules, asked
// of the core itself (the library `scripts/core.sh` builds for this
// machine). The numbers are cases of `proto/vectors/viewer/cases.json`,
// which every client agrees on, given as a phone has them: the room and
// the fingers in px from the room's top left corner.
package chat.lumia.kit

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.lumia_ffi.ViewerView

/** The cases agree to a thousandth. */
private const val NEAR = 0.001f

private fun assertView(scale: Float, x: Float, y: Float, view: ViewerView) {
    assertEquals(scale, view.scale, NEAR)
    assertEquals(x, view.x, NEAR)
    assertEquals(y, view.y, NEAR)
}

/** A picture twice its room's width, the cases' own: 2000 by 1000 in a room of 1000 by 800 dp, a px a dp unless [density] says otherwise. */
private fun wide(density: Float = 1f) = zoomOf(2000, 1000, 1000f * density, 800f * density, density)

class ViewerTest {
    @Test
    fun aPictureOpensFittedAndZoomsToFourTimesItsOwnSize() {
        // "the largest picture on a phone: the limit is from its own size, not from fitted"
        val z = zoomOf(2560, 1920, 390f, 700f, 1f)
        assertEquals(390f, z.width, NEAR)
        assertEquals(292.5f, z.height, NEAR)
        assertEquals(6.5641f, z.frame.own, NEAR)
        assertEquals(26.2564f, z.frame.maxScale, NEAR)
        assertEquals(6.5641f, z.frame.detailScale, NEAR)
        assertEquals(FITTED, z.view)
        assertEquals(1f, z.scale, 0f)
    }

    @Test
    fun theRoomIsCountedInDpWhateverTheScreen() {
        // The same room on a screen of 2.75 px a dp is the same frame: a picture pixel is a dp, not a px.
        val z = zoomOf(2560, 1920, 390f * 2.75f, 700f * 2.75f, 2.75f)
        assertEquals(390f, z.width, NEAR)
        assertEquals(292.5f, z.height, NEAR)
        assertEquals(390f, z.frame.roomWidth, NEAR)
        assertEquals(700f, z.frame.roomHeight, NEAR)
        assertEquals(26.2564f, z.frame.maxScale, NEAR)
    }

    @Test
    fun aPictureSmallerThanTheRoomIsNotGrown() {
        // "smaller than the room: never grown to fit, and a press goes to twice it"
        val z = zoomOf(400, 300, 1000f, 800f, 1f)
        assertEquals(400f, z.width, NEAR)
        assertEquals(300f, z.height, NEAR)
        assertEquals(1f, z.frame.own, NEAR)
        assertEquals(4f, z.frame.maxScale, NEAR)
        // "a small picture: twice its size, which still fits the room, so centred"
        assertView(2f, 0f, 0f, z.tappedTwice(600f, 450f).view)
    }

    @Test
    fun aPinchZoomsAboutTheFingers() {
        // "the point zoomed about stays where it is": (100, 50) from the centre of a room of 1000 by 800 is (600, 450) from its corner.
        assertView(2f, -100f, -50f, wide().pinched(600f, 450f, 0f, 0f, 2f).view)
        // The same fingers on a screen of two px a dp: the view is in dp still, and drawn in px.
        val dense = wide(2f).pinched(1200f, 900f, 0f, 0f, 2f)
        assertView(2f, -100f, -50f, dense.view)
        assertEquals(2f, dense.scale, NEAR)
        assertEquals(-200f, dense.shiftX, NEAR)
        assertEquals(-100f, dense.shiftY, NEAR)
        // "750 tall in a room of 800: centred along a side that fits, whatever the point"
        assertView(1.5f, 0f, 0f, wide().pinched(500f, 600f, 0f, 0f, 1.5f).view)
    }

    @Test
    fun aPinchStopsAtFittedAndAtTheLimit() {
        // "never past four times its own size"
        assertView(8f, 0f, 0f, wide().pinched(500f, 400f, 0f, 0f, 99f).view)
        // "never smaller than fitted, and fitted is centred"
        val zoomed = wide().pinched(600f, 450f, 0f, 0f, 2f)
        assertView(1f, 0f, 0f, zoomed.pinched(500f, 400f, 0f, 0f, 0.25f).view)
    }

    @Test
    fun aPinchMovesThePictureWithTheFingers() {
        // Zoomed to 2 about (100, 50) and the fingers moved by (30, 20) in the same gesture: the picture went with them.
        assertView(2f, -70f, -30f, wide().pinched(600f, 450f, 30f, 20f, 2f).view)
        // In px on a denser screen, the same.
        assertView(2f, -70f, -30f, wide(2f).pinched(1200f, 900f, 60f, 40f, 2f).view)
    }

    @Test
    fun aDragStopsAtThePicturesEdges() {
        // Two taps in the middle: at 2 and centred.
        val zoomed = wide().tappedTwice(500f, 400f)
        assertView(2f, 0f, 0f, zoomed.view)
        // "a drag stops where the picture's edge meets the room's"
        assertView(2f, 500f, -100f, zoomed.pinched(500f, 400f, 900f, -900f, 1f).view)
        // "a fitted picture does not move"
        assertView(1f, 0f, 0f, wide().pinched(500f, 400f, 50f, 50f, 1f).view)
    }

    @Test
    fun twoTapsZoomInAboutThePointAndBack() {
        // "a press on a fitted picture: its own size, about the point"
        val zoomed = wide().tappedTwice(600f, 450f)
        assertView(2f, -100f, -50f, zoomed.view)
        // "a press on a zoomed picture: fitted again"
        assertEquals(FITTED, zoomed.tappedTwice(800f, 410f).view)
        // The largest picture on a phone goes to its own size, a pixel a dp, not to twice fitted.
        assertEquals(6.5641f, zoomOf(2560, 1920, 390f, 700f, 1f).tappedTwice(195f, 350f).scale, NEAR)
    }

    @Test
    fun aViewIsKeptInsideARoomThatChanged() {
        // "the room grown taller than the zoomed picture: centred along that side"
        val taller = zoomOf(2000, 1000, 1000f, 1200f, 1f)
        assertView(2f, 500f, 0f, taller.holding(ViewerView(2f, 500f, -100f)).view)
        // "the room shrunk: fitted is smaller, and the same scale is still inside its limit of 16"
        assertView(8f, 0f, 0f, zoomOf(2000, 1000, 500f, 400f, 1f).holding(ViewerView(8f, 0f, 0f)).view)
        // A view held where it already is, is that view.
        assertEquals(FITTED, wide().holding(FITTED).view)
    }
}
