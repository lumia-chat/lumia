// The picture viewer's zoom (specs/media.md 5, issue #24), what a screen
// zooms a picture by, as plain values `src/test` checks on the JVM. The
// rules are the core's (`lumia_core::viewer`, through `viewerFrame`,
// `viewerZoomAt`, `viewerPanBy`, `viewerPressed` and `viewerHeld`) and
// never copied here: how a picture fits its room, how far it zooms, about
// which point, and how far it may be moved. What is here is the phone's
// side of them: the core counts in points from the room's centre, a point
// being a dp, and a screen has its room and its fingers in px from the
// room's top left corner.
package chat.lumia.kit

import uniffi.lumia_ffi.ViewerFrame
import uniffi.lumia_ffi.ViewerView
import uniffi.lumia_ffi.viewerFrame
import uniffi.lumia_ffi.viewerHeld
import uniffi.lumia_ffi.viewerPanBy
import uniffi.lumia_ffi.viewerPressed
import uniffi.lumia_ffi.viewerZoomAt

/** Fitted and centred, as a picture opens. */
val FITTED = ViewerView(scale = 1f, x = 0f, y = 0f)

/**
 * The viewer's picture: how it fits its room ([frame]) and how it is
 * zoomed and moved ([view]), both the core's and in dp, on a screen of
 * [density] px a dp. Made by [zoomOf]; fingers on it answer with the
 * picture as it then is.
 */
data class Zoom(val frame: ViewerFrame, val view: ViewerView, val density: Float) {
    /** The fitted picture's size in dp, which [scale] multiplies: shrunk into the room and never grown, so one smaller than the room is its own size, a pixel a dp. */
    val width: Float get() = frame.width
    val height: Float get() = frame.height

    /** How many times its fitted size it is drawn: 1 is fitted. */
    val scale: Float get() = view.scale

    /** How far its centre is drawn from the room's, in px. */
    val shiftX: Float get() = view.x * density
    val shiftY: Float get() = view.y * density

    /** [view] in this room, kept as far as the room allows: what the screen holds between gestures may be from a room that has since changed (the phone turned, the window resized). */
    fun holding(view: ViewerView): Zoom = copy(view = viewerHeld(view, frame))

    /**
     * Fingers on the picture, as Compose's `detectTransformGestures` has
     * them: their centroid was at ([x], [y]), moved by ([panX], [panY])
     * and they spread [by] times as far apart (one finger dragging
     * spreads by 1). The picture zooms about the centroid, so what is
     * under the fingers stays under them, between fitted and the core's
     * limit, and then moves with them as far as its edges allow.
     */
    fun pinched(x: Float, y: Float, panX: Float, panY: Float, by: Float): Zoom {
        val zoomed = viewerZoomAt(view, view.scale * by, acrossFromCentre(x), downFromCentre(y), frame)
        return copy(view = viewerPanBy(zoomed, panX / density, panY / density, frame))
    }

    /**
     * Two taps at ([x], [y]): a fitted picture zooms in about the point,
     * to its own size (the core's `detailScale`: twice the fitted size
     * for a picture hardly larger than its room), and a zoomed one goes
     * back to fitted.
     */
    fun tappedTwice(x: Float, y: Float): Zoom = copy(view = viewerPressed(view, acrossFromCentre(x), downFromCentre(y), frame))

    /** A place in the room, in px from its top left corner, as the core wants it: in dp from the room's centre. */
    private fun acrossFromCentre(x: Float): Float = x / density - frame.roomWidth / 2f

    private fun downFromCentre(y: Float): Float = y / density - frame.roomHeight / 2f
}

/**
 * A picture of [ownWidth] by [ownHeight] pixels fitted into a room of
 * [roomWidth] by [roomHeight] px, the whole room its own (no margin), on
 * a screen of [density] px a dp: as it opens.
 */
fun zoomOf(ownWidth: Int, ownHeight: Int, roomWidth: Float, roomHeight: Float, density: Float): Zoom =
    Zoom(viewerFrame(ownWidth.toFloat(), ownHeight.toFloat(), roomWidth / density, roomHeight / density, 0f), FITTED, density)
