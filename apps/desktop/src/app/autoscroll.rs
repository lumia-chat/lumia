//! Autoscroll (Ryan, 2026-11-13): the middle button held over a pane that scrolls, and the
//! pointer moved up or down from where it was pressed, scrolls the pane continuously, faster
//! the further the pointer is, until the button comes up, as browsers do on Windows and
//! Linux. A mark stays where the button was pressed. The web does the same, by the same
//! numbers (`apps/web/src/client/autoscroll.ts`).
//!
//! Rune hands the middle button to an element that asks for it (`El::pointer_buttons`), and
//! every pane asks through `ui::scroll`. It tells the app where a pane has scrolled only
//! through a callback that has no state (`El::on_scroll_offset`), so the panes' positions are
//! kept here, per thread: a window's view runs on one. The frame's poll moves the pane held.

use std::cell::RefCell;
use std::collections::HashMap;

use super::State;

/// Within this many points of the press nothing scrolls: a hand never holds quite still.
pub const DEAD_ZONE: f32 = 8.0;
/// Points a second of scrolling for each point past the dead zone.
pub const RATE: f32 = 8.0;
/// The fastest it goes, in points a second.
pub const MAX_SPEED: f32 = 4000.0;

/// How fast a pane scrolls, in points a second, for a pointer `distance` points below the
/// press (above it, negative): down for down.
pub fn speed(distance: f32) -> f32 {
    let past = distance.abs() - DEAD_ZONE;
    if past <= 0.0 {
        return 0.0;
    }
    (past * RATE).min(MAX_SPEED).copysign(distance)
}

thread_local! {
    static POSITIONS: RefCell<HashMap<String, (f32, f32)>> = RefCell::new(HashMap::new());
}

/// Where the pane keyed `key` has scrolled to and how far it can, as Rune reports it.
pub fn report(key: &str, offset: f32, max: f32) {
    POSITIONS.with(|p| {
        p.borrow_mut().insert(key.to_string(), (offset, max));
    });
}

fn position(key: &str) -> Option<(f32, f32)> {
    POSITIONS.with(|p| p.borrow().get(key).copied())
}

/// The pane held, while the middle button is down over one.
#[derive(Debug, Default)]
pub struct Autoscroll {
    held: Option<Held>,
}

#[derive(Debug)]
struct Held {
    key: String,
    /// Where the button came down, in window points.
    origin: [f32; 2],
    /// How far below the press the pointer is now.
    distance: f32,
    /// Where this has scrolled the pane to.
    offset: f32,
    /// The last step's time, in the poll's seconds.
    last: Option<f64>,
}

impl Autoscroll {
    /// Where the mark is drawn: where the button came down.
    pub fn origin(&self) -> Option<[f32; 2]> {
        self.held.as_ref().map(|h| h.origin)
    }

    pub fn active(&self) -> bool {
        self.held.is_some()
    }
}

impl State {
    /// The middle button came down over the pane keyed `key`, at `at` in window points.
    pub fn autoscroll_start(&mut self, key: &str, at: [f32; 2]) {
        let offset = position(key).map_or(0.0, |(offset, _)| offset);
        self.autoscroll.held = Some(Held {
            key: key.to_string(),
            origin: at,
            distance: 0.0,
            offset,
            last: None,
        });
    }

    /// The pointer moved, `distance` points below where the button came down.
    pub fn autoscroll_move(&mut self, distance: f32) {
        if let Some(held) = self.autoscroll.held.as_mut() {
            held.distance = distance;
        }
    }

    /// The button came up, or the gesture was cancelled.
    pub fn autoscroll_stop(&mut self) {
        self.autoscroll.held = None;
    }

    /// One step at `now` (the poll's seconds): the pane to move and where to, when it moves.
    pub fn autoscroll_step(&mut self, now: f64) -> Option<(String, f32)> {
        let held = self.autoscroll.held.as_mut()?;
        // A step after a pause (the window hidden) does not make up for it.
        let dt = held
            .last
            .map_or(0.0, |last| ((now - last) as f32).clamp(0.0, 0.1));
        held.last = Some(now);
        let v = speed(held.distance);
        if v == 0.0 {
            return None;
        }
        let (reported, max) = position(&held.key).unwrap_or((held.offset, f32::MAX));
        // Something else moved the pane meanwhile (older messages loaded above it): go on from
        // where it is, not from where this left it.
        if (reported - held.offset).abs() > MAX_SPEED * 0.1 {
            held.offset = reported;
        }
        held.offset = (held.offset + v * dt).clamp(0.0, max.max(0.0));
        Some((held.key.clone(), held.offset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_goes_faster_the_further_the_pointer_is_and_not_at_all_near_the_press() {
        assert_eq!(speed(0.0), 0.0);
        assert_eq!(speed(DEAD_ZONE), 0.0);
        assert_eq!(speed(-DEAD_ZONE + 1.0), 0.0);
        assert_eq!(speed(DEAD_ZONE + 10.0), 80.0);
        assert_eq!(speed(-(DEAD_ZONE + 10.0)), -80.0);
        assert!(speed(100.0) > speed(50.0));
        assert_eq!(speed(10_000.0), MAX_SPEED);
        assert_eq!(speed(-10_000.0), -MAX_SPEED);
    }
}
