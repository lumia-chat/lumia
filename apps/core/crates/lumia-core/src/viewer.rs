//! The picture viewer's zoom (docs/specs/media.md 5): how far a picture opened from a message
//! zooms, by what steps, about which point, and how far it may be moved. Rules only, with no
//! gestures: each client turns its own wheel, pinch, press, keys and buttons into these calls,
//! so a picture zooms the same way everywhere.
//!
//! A picture opens fitted: shrunk into its room, never grown past its own size, which is a
//! picture pixel to a point (the unit a client lays out in, as a conversation sizes it). A
//! [`View`]'s `scale` is in times the fitted size, so 1 is fitted, and its `x` and `y` are how
//! far the picture's centre is from the room's, in points; a point a zoom is about is given
//! the same way. `proto/vectors/viewer/cases.json` holds the cases every client agrees on.

/// One step of the zoom buttons and of the plus and minus keys.
pub const STEP: f32 = 1.5;

/// The furthest zoom, in times the picture's own size.
pub const OWN_SIZES: f32 = 4.0;

/// How far from fitted a picture's own size must be for a press to go there: under it the
/// press would hardly show, and goes to twice the fitted size.
const DETAIL_FROM: f32 = 1.5;

/// How fast a wheel zooms, a point of scrolling: a mouse's notch of 100 is about 1.28 times.
const WHEEL_RATE: f32 = 0.0025;

/// The same for a trackpad's pinch where it arrives as a wheel with Control held, whose
/// deltas are a few points each.
const PINCH_RATE: f32 = 0.01;

/// The most one wheel event zooms, as an exponent: about 1.82 times.
const WHEEL_MOST: f32 = 0.6;

/// How a picture is zoomed and moved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// In times the fitted size: 1 is fitted.
    pub scale: f32,
    /// The picture's centre from the room's, in points.
    pub x: f32,
    pub y: f32,
}

/// Fitted and centred, as a picture opens.
pub const FIT: View = View {
    scale: 1.0,
    x: 0.0,
    y: 0.0,
};

/// A picture fitted into its room, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// The fitted picture.
    pub width: f32,
    pub height: f32,
    /// The room it zooms in, which cuts off what is past it.
    pub room_width: f32,
    pub room_height: f32,
    /// The scale of the picture's own size: 1 for a picture that fits as it is.
    pub own: f32,
}

impl Frame {
    /// The furthest scale: [`OWN_SIZES`] times the picture's own size.
    #[must_use]
    pub fn max_scale(&self) -> f32 {
        self.own * OWN_SIZES
    }

    /// Where a press on a fitted picture zooms to: its own size, or twice the fitted size for
    /// a picture whose own size is hardly larger.
    #[must_use]
    pub fn detail_scale(&self) -> f32 {
        if self.own >= DETAIL_FROM {
            self.own
        } else {
            2.0
        }
    }
}

/// A picture of `own_width` by `own_height` pixels fitted into a room, `margin` points kept
/// clear on each side of it while it is fitted: shrunk to fit, never grown.
#[must_use]
pub fn frame(
    own_width: f32,
    own_height: f32,
    room_width: f32,
    room_height: f32,
    margin: f32,
) -> Frame {
    let (own_width, own_height) = (own_width.max(1.0), own_height.max(1.0));
    let (room_width, room_height) = (room_width.max(1.0), room_height.max(1.0));
    let within = |room: f32| (room - 2.0 * margin).max(1.0);
    let k = (within(room_width) / own_width)
        .min(within(room_height) / own_height)
        .min(1.0);
    Frame {
        width: own_width * k,
        height: own_height * k,
        room_width,
        room_height,
        own: 1.0 / k,
    }
}

/// Zoomed to `scale` about the point `at`, which stays where it is on screen unless the
/// picture's edges say otherwise ([`held`]). Between fitted and [`Frame::max_scale`].
#[must_use]
pub fn zoom_at(view: View, scale: f32, at: [f32; 2], frame: &Frame) -> View {
    let next = scale.clamp(1.0, frame.max_scale().max(1.0));
    let k = next / view.scale.max(f32::EPSILON);
    held(
        View {
            scale: next,
            x: at[0] - (at[0] - view.x) * k,
            y: at[1] - (at[1] - view.y) * k,
        },
        frame,
    )
}

/// Moved by `dx` and `dy` points, as a drag moves it.
#[must_use]
pub fn pan_by(view: View, dx: f32, dy: f32, frame: &Frame) -> View {
    held(
        View {
            scale: view.scale,
            x: view.x + dx,
            y: view.y + dy,
        },
        frame,
    )
}

/// `view` kept inside the frame, as after a move or when the room changes size: along a side
/// the picture is longer than the room it covers the room, and along one it fits it is
/// centred.
#[must_use]
pub fn held(view: View, frame: &Frame) -> View {
    let scale = view.scale.clamp(1.0, frame.max_scale().max(1.0));
    let axis = |offset: f32, size: f32, room: f32| {
        let most = ((size * scale - room) / 2.0).max(0.0);
        // Adding zero makes a negative zero a zero.
        offset.clamp(-most, most) + 0.0
    };
    View {
        scale,
        x: axis(view.x, frame.width, frame.room_width),
        y: axis(view.y, frame.height, frame.room_height),
    }
}

/// `steps` of the zoom buttons or keys, in (above 0) or out, about the room's centre.
#[must_use]
pub fn step(view: View, steps: i32, frame: &Frame) -> View {
    zoom_at(view, view.scale * STEP.powi(steps), [0.0, 0.0], frame)
}

/// A press on the picture at `at` (two taps on a phone): a fitted picture zooms to
/// [`Frame::detail_scale`] about the point, and a zoomed one goes back to fitted.
#[must_use]
pub fn pressed(view: View, at: [f32; 2], frame: &Frame) -> View {
    if view.scale > 1.0 {
        FIT
    } else {
        zoom_at(view, frame.detail_scale(), at, frame)
    }
}

/// At the picture's own size, about the room's centre.
#[must_use]
pub fn own_size(view: View, frame: &Frame) -> View {
    zoom_at(view, frame.own, [0.0, 0.0], frame)
}

/// What one wheel event multiplies the scale by. `delta` is the points it would scroll a
/// page, positive down it (a browser's `deltaY`), so up zooms in; `pinch` is a trackpad's
/// pinch arriving as a wheel with Control held.
#[must_use]
pub fn wheel_factor(delta: f32, pinch: bool) -> f32 {
    let rate = if pinch { PINCH_RATE } else { WHEEL_RATE };
    (-delta * rate).clamp(-WHEEL_MOST, WHEEL_MOST).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_is_shrunk_to_fit_and_never_grown() {
        let large = frame(2000.0, 1000.0, 1000.0, 800.0, 0.0);
        assert_eq!((large.width, large.height, large.own), (1000.0, 500.0, 2.0));
        let small = frame(400.0, 300.0, 1000.0, 800.0, 0.0);
        assert_eq!((small.width, small.height, small.own), (400.0, 300.0, 1.0));
        // The margin is kept clear of a fitted picture, and is not part of what a zoomed one
        // must cover.
        let kept = frame(2000.0, 1000.0, 1040.0, 840.0, 20.0);
        assert_eq!((kept.width, kept.room_width), (1000.0, 1040.0));
    }

    #[test]
    fn the_limit_is_in_times_the_own_size_and_a_press_goes_to_it() {
        let large = frame(2000.0, 1000.0, 1000.0, 800.0, 0.0);
        assert_eq!((large.max_scale(), large.detail_scale()), (8.0, 2.0));
        // A picture that fits as it is has nothing larger to show at its own size.
        let small = frame(400.0, 300.0, 1000.0, 800.0, 0.0);
        assert_eq!((small.max_scale(), small.detail_scale()), (4.0, 2.0));
        assert_eq!(zoom_at(FIT, 99.0, [0.0, 0.0], &large).scale, 8.0);
        assert_eq!(own_size(FIT, &small), FIT);
    }

    #[test]
    fn the_point_zoomed_about_stays_put_until_an_edge_holds_the_picture() {
        let f = frame(2000.0, 1000.0, 1000.0, 800.0, 0.0);
        let v = zoom_at(FIT, 2.0, [100.0, 50.0], &f);
        assert_eq!(
            v,
            View {
                scale: 2.0,
                x: -100.0,
                y: -50.0
            }
        );
        // At 1.5 the picture is 750 tall in a room of 800: centred, whatever the point.
        let v = zoom_at(FIT, 1.5, [0.0, 200.0], &f);
        assert_eq!(
            v,
            View {
                scale: 1.5,
                x: 0.0,
                y: 0.0
            }
        );
        assert_eq!(
            zoom_at(
                View {
                    scale: 2.0,
                    x: -100.0,
                    y: -50.0
                },
                0.5,
                [0.0, 0.0],
                &f
            ),
            FIT
        );
    }

    #[test]
    fn a_drag_stops_at_the_pictures_edges() {
        let f = frame(2000.0, 1000.0, 1000.0, 800.0, 0.0);
        let v = pan_by(
            View {
                scale: 2.0,
                x: 0.0,
                y: 0.0,
            },
            900.0,
            -900.0,
            &f,
        );
        assert_eq!(
            v,
            View {
                scale: 2.0,
                x: 500.0,
                y: -100.0
            }
        );
        assert_eq!(pan_by(FIT, 50.0, 50.0, &f), FIT);
        // A smaller room holds the same view further in.
        let smaller = frame(2000.0, 1000.0, 1000.0, 1200.0, 0.0);
        assert_eq!(held(v, &smaller).y, 0.0);
    }

    #[test]
    fn a_press_zooms_in_about_its_point_and_back() {
        let f = frame(2000.0, 1000.0, 1000.0, 800.0, 0.0);
        let v = pressed(FIT, [100.0, 50.0], &f);
        assert_eq!(
            v,
            View {
                scale: 2.0,
                x: -100.0,
                y: -50.0
            }
        );
        assert_eq!(pressed(v, [0.0, 0.0], &f), FIT);
    }

    #[test]
    fn steps_and_the_wheel() {
        let f = frame(2000.0, 1000.0, 1000.0, 800.0, 0.0);
        assert_eq!(step(FIT, 1, &f).scale, 1.5);
        assert_eq!(step(FIT, -1, &f), FIT);
        assert_eq!(
            step(
                View {
                    scale: 2.0,
                    x: 100.0,
                    y: 0.0
                },
                1,
                &f
            ),
            View {
                scale: 3.0,
                x: 150.0,
                y: 0.0
            }
        );
        assert!((wheel_factor(-100.0, false) - 0.25_f32.exp()).abs() < 1e-6);
        assert!((wheel_factor(10.0, true) - (-0.1_f32).exp()).abs() < 1e-6);
        assert!((wheel_factor(-10_000.0, false) - 0.6_f32.exp()).abs() < 1e-6);
    }
}
