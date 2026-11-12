//! Scenes: named states the window can be drawn in without anyone clicking there, for
//! Rune's headless tooling (`lumia --snap out.png --scene NAME`, `--tree`, `--script`).
//! A scene is a function that puts the state somewhere; [`SCENES`] is the list `main` hands
//! to Rune. Add one for a screen that is otherwise several presses away.

use crate::app::{HomeView, Route, State};
use crate::session::Connection;

/// A scene: its name for `--scene`, and what it does to the state.
pub type Scene = (&'static str, fn(&mut State));

/// Every scene.
pub const SCENES: &[Scene] = &[
    ("live", live),
    ("live-home", |s| {
        live(s);
        s.go_home(HomeView::Conversations);
    }),
];

/// The `live` scene, for a headless picture of the real thing (`--snap out.png --scene live`
/// with the `LUMIA_DEV_*` environment): waits, on the wall clock, until the instance has
/// answered and the open channel has its messages, or ten seconds. A headless run has a
/// virtual clock and one frame, so nothing else would give the network time to answer.
pub fn live(state: &mut State) {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut settled = None;
    while Instant::now() < deadline {
        if state.drain() {
            settled = None;
        }
        let loaded = state.route == Route::Shell
            && state.session.connection == Connection::Connected
            && state.session.snapshot.as_ref().is_some_and(|snap| {
                snap.servers
                    .iter()
                    .all(|x| state.session.members.contains_key(&x.server_id))
            });
        // Loaded, and quiet for a moment: the channel's replay has landed.
        match (loaded, settled) {
            (true, Some(since)) if Instant::now() - since > Duration::from_millis(600) => return,
            (true, None) => settled = Some(Instant::now()),
            (false, _) => settled = None,
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    eprintln!(
        "lumia: the live scene gave up waiting; connection {:?}, error {:?}",
        state.session.connection, state.session.last_error
    );
}
