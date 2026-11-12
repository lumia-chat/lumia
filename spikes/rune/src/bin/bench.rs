//! Windowless frame costs of the shell over the seeded store, at 120 Hz:
//! `cargo run --profile perf --features profile --bin bench`.
//!
//! Each workload drives a `Runtime` the way a window would and reads this thread's CPU clock
//! around every frame: input dispatch, the view, commit, layout, text and draw-list building.
//! GPU submission is not in it (the headless `--script` runs print that), and text is set in
//! Rune's built-in Inter, since a bare `Runtime` cannot be given the app's fonts.

use lumia_rune_spike::ui::{self, State};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const SIZE: (f32, f32) = (1180.0, 760.0);
const FPS: f64 = 120.0;
/// Over the message list, clear of the sidebar and the composer.
const POINTER: (f32, f32) = (700.0, 380.0);
const PHASE_NAMES: [&str; 6] = ["view", "commit", "layout", "text", "paint", "apply"];

struct Samples {
    cpu: Vec<f64>,
    phases: [f64; 6],
    commits: u64,
}

fn runtime() -> Runtime<State> {
    let mut rt = Runtime::new(ui::init(), ui::view, SIZE, Env::new(true, 2.0));
    rt.frame(0.0);
    rt.pointer_move(POINTER.0, POINTER.1);
    for _ in 0..120 {
        rt.frame(1.0 / FPS);
    }
    rt
}

/// Runs `frames` frames, `step` before each, and times step and frame together.
fn measure(rt: &mut Runtime<State>, frames: usize, mut step: impl FnMut(&mut Runtime<State>, usize)) -> Samples {
    let mut out = Samples { cpu: Vec::with_capacity(frames), phases: [0.0; 6], commits: 0 };
    let commits = rt.stats.commits;
    for i in 0..frames {
        rune::take_frame_phases();
        let start = rune::thread_cpu_ms();
        step(rt, i);
        rt.frame(1.0 / FPS);
        out.cpu.push(rune::thread_cpu_ms() - start);
        let phases = rune::take_frame_phases();
        for (total, cpu) in out.phases.iter_mut().zip(&phases[rune::PHASES..]) {
            *total += cpu;
        }
    }
    out.commits = rt.stats.commits - commits;
    out
}

fn report(name: &str, note: &str, mut s: Samples) {
    let frames = s.cpu.len();
    s.cpu.sort_by(f64::total_cmp);
    let at = |q: f64| s.cpu[((frames - 1) as f64 * q).round() as usize];
    let over = s.cpu.iter().filter(|ms| **ms > 1000.0 / FPS).count();
    let phases: Vec<String> =
        PHASE_NAMES.iter().zip(s.phases).map(|(name, total)| format!("{name} {:.3}", total / frames as f64)).collect();
    println!(
        "{name:<8} p50 {:.3} ms  p95 {:.3}  p99 {:.3}  max {:.3}  over 8.33 ms: {over}/{frames}  commits {}  [{}]  {note}",
        at(0.5),
        at(0.95),
        at(0.99),
        at(1.0),
        s.commits,
        phases.join(", "),
    );
}

fn main() {
    if cfg!(debug_assertions) {
        panic!("build with --profile perf");
    }
    let only = std::env::args().nth(1);
    let wants = |name: &str| only.as_deref().is_none_or(|o| o == name);

    if wants("scroll") {
        let mut rt = runtime();
        let newest = rt.state.messages[rt.state.messages.len() - 1].key.clone();
        let s = measure(&mut rt, 960, |rt, i| {
            if i == 480 {
                assert!(rt.find(&newest).is_none(), "4 s of scrolling left the newest message behind");
            }
            rt.wheel(POINTER.0, POINTER.1, if i < 480 { 12.0 } else { -12.0 });
        });
        assert!(rt.find(&newest).is_some(), "and 4 s back returned to it");
        report("scroll", "12 pt a frame up through history for 4 s, then back", s);
    }
    if wants("fling") {
        let mut rt = runtime();
        let oldest = rt.state.messages[0].key.clone();
        let s = measure(&mut rt, 480, |rt, _| rt.wheel(POINTER.0, POINTER.1, 400.0));
        let reached = if rt.find(&oldest).is_some() { "reached the oldest message" } else { "did not reach the oldest message" };
        report("fling", &format!("400 pt a frame into rows never measured (63 screens a second); {reached}"), s);
    }
    if wants("type") {
        let mut rt = runtime();
        let (x, y) = rt.find("composer").expect("the composer is mounted");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / FPS);
        let s = measure(&mut rt, 240, |rt, i| {
            let c = if i % 6 == 5 { ' ' } else { (b'a' + (i % 26) as u8) as char };
            rt.key(KeyEvent { key: Key::Text(c.to_string()), mods: Mods::default() });
        });
        assert_eq!(rt.state.draft.text.len(), 240, "every key reached the composer");
        report("type", "a character a frame in the composer, the 10k-row list beside it", s);
    }
    if wants("append") {
        let mut rt = runtime();
        let before = rt.state.messages.len();
        let s = measure(&mut rt, 480, |rt, i| {
            if i % 12 == 0 {
                rt.update(|s, tx| {
                    s.messages.push(ui::Msg {
                        key: format!("bench-{i}"),
                        author: i % 5,
                        time: "now".into(),
                        body: "A message arrives while the list is pinned to its end.".into(),
                    });
                    tx.scroll_to_bottom("messages");
                });
            }
        });
        assert_eq!(rt.state.messages.len(), before + 40);
        report("append", "a message every 12 frames, pinned to the end", s);
    }
    if wants("switch") {
        let mut rt = runtime();
        let s = measure(&mut rt, 240, |rt, i| {
            if i % 24 == 0 {
                rt.update(|s, _| s.open_channel((s.current + 1) % s.manifest.channels.len()));
            }
        });
        report("switch", "another channel every 24 frames: 10k rows read from SQLite, the list remounted", s);
    }
    if wants("top") {
        let mut rt = runtime();
        let first = rt.state.messages[0].key.clone();
        let s = measure(&mut rt, 240, |rt, i| {
            if i == 0 {
                let first = first.clone();
                rt.update(move |_, tx| tx.scroll_into_view(first));
            }
        });
        assert!(rt.find(&first).is_some(), "the oldest message is mounted");
        report("top", "reveal the oldest of 10k messages from the newest", s);
    }
    // Only with the list as one selection group (`LUMIA_SPIKE_PLAIN`, or `LUMIA_SPIKE_JOINED`
    // under the `kit-patch` feature): Cmd+A in a message body takes the whole channel, and
    // reading the selection builds every row.
    if only.as_deref() == Some("selectall") {
        let mut rt = runtime();
        assert!(rt.state.joined || rt.state.plain, "set LUMIA_SPIKE_PLAIN, or LUMIA_SPIKE_JOINED with --features kit-patch");
        let newest = rt.state.messages[rt.state.messages.len() - 1].key.clone();
        let body = if rt.state.plain { format!("{newest}/body") } else { format!("{newest}/body/b0") };
        let (x, y) = rt.find(&body).expect("the newest body is mounted");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / FPS);
        let start = rune::thread_cpu_ms();
        rt.key(KeyEvent { key: Key::Command('a'), mods: Mods { cmd: true, ..Mods::default() } });
        rt.frame(1.0 / FPS);
        let selected = rune::thread_cpu_ms() - start;
        let start = rune::thread_cpu_ms();
        let text = rt.selected_text().unwrap_or_default();
        let copied = rune::thread_cpu_ms() - start;
        let s = measure(&mut rt, 240, |rt, _| rt.wheel(POINTER.0, POINTER.1, 12.0));
        println!(
            "selectall  Cmd+A {selected:.1} ms, reading the selection {copied:.1} ms for {} messages and {} bytes",
            rt.state.messages.len(),
            text.len(),
        );
        report("scroll", "12 pt a frame with the whole channel selected", s);
    }
}
