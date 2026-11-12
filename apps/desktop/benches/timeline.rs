//! Windowless frame costs of the message list over 1,000 varied messages, at 120 Hz:
//! `cargo bench --features profile --bench timeline [-- WORKLOAD]`.
//!
//! Each workload drives a `Runtime` the way a window would and reads this thread's CPU clock
//! around every frame: input dispatch, the view, commit, layout, text and draw-list building.
//! GPU submission is not in it, and text is set in Rune's built-in face, since a bare
//! `Runtime` cannot be given the app's fonts. No instance is behind the state.

#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../tests/common/mod.rs"]
mod common;

use lumia_shell::app::State;
use lumia_shell::ui;
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const SIZE: (f32, f32) = (1180.0, 760.0);
const FPS: f64 = 120.0;
const MESSAGES: usize = 1_000;
/// Over the message list, clear of the sidebar and the composer.
const POINTER: (f32, f32) = (600.0, 380.0);

/// A body for message `i`: every construct of the protocol's subset and a line per script,
/// in the proportions of a channel where most messages are a sentence or two.
fn body(i: usize) -> String {
    match i % 20 {
        0 => format!(
            "Shipped build {i} on **Friday**, with the *usual* caveats and ~~no~~ few surprises."
        ),
        1 => "Reading it now.".into(),
        2 => format!(
            "The `Session::poll` path drains the queue on the window's thread; see \
             [the architecture](https://example.com/architecture#{i}) for why it is not a channel."
        ),
        3 => format!(
            "Three things before the release:\n\n- the smoke script against a fresh instance\n\
             - budgets on the real shell\n  - launch to first frame\n  - idle memory\n- note {i} in the plan"
        ),
        4 => format!(
            "```rust\nfn frame(rt: &mut Runtime<State>, n: usize) -> f64 {{\n    let start = thread_cpu_ms();\n    \
             for _ in 0..n {{\n        rt.frame(1.0 / 120.0);\n    }}\n    thread_cpu_ms() - start // {i}\n}}\n```"
        ),
        5 => format!(
            "> A drag stops at the end of the message it began in.\n\nNot any more, as of message {i}."
        ),
        6 => "日本語のメッセージです。句読点は、どうなりますか。".into(),
        7 => "这是一条中文消息，用来检查换行和标点。".into(),
        8 => "한국어 메시지입니다. 줄바꿈과 문장 부호를 확인합니다.".into(),
        9 => "هذه رسالة باللغة العربية للتحقق من الاتجاه من اليمين إلى اليسار.".into(),
        10 => "Сообщение на русском языке, чтобы проверить кириллицу.".into(),
        11 => "Looks good 👍🏽 the family 👨‍👩‍👧‍👦 agrees, and so does 🏳️‍🌈.".into(),
        12 => format!(
            "A longer message, the kind someone writes when the answer is not one line. It runs on for a while so \
             that it wraps three or four times at the width of the conversation, which is what most of the cost \
             of a row is: shaping the text and breaking it into lines. This is paragraph one of message {i}.\n\n\
             And a second paragraph, shorter, to close the thought."
        ),
        13 => format!("1. Open the instance\n2. Sign in\n3. Send message {i}"),
        14 => "ok".into(),
        15 => format!(
            "Run `cargo test` and then `scripts/smoke.sh`; both must pass before {i} lands."
        ),
        16 => "See https://example.com/releases/0.2.1 for the notes.".into(),
        17 => format!("```\nplain fence {i}\nwith two lines\n```\n\nand a sentence after it."),
        18 => "# not a heading\n\n<b>not bold</b> and ![not an image](https://example.com/x.png)"
            .into(),
        _ => format!("Message {i} is a sentence of ordinary length, as most are."),
    }
}

fn state() -> State {
    let mut state = common::signed_in();
    let now = chrono::Utc::now().timestamp_micros();
    let minute = 60_000_000;
    state.session.entries = (0..MESSAGES)
        .map(|i| {
            let author = ["grace", "me", "stranger"][i % 3];
            let at = now - ((MESSAGES - i) as i64) * 7 * minute;
            common::message(&format!("m{i}"), author, &body(i), at)
        })
        .collect();
    state.session.revision += 1;
    state
}

fn runtime() -> Runtime<State> {
    let mut rt = Runtime::new(state(), ui::view, SIZE, Env::new(false, 2.0));
    rt.frame(0.0);
    rt.pointer_move(POINTER.0, POINTER.1);
    for _ in 0..120 {
        rt.frame(1.0 / FPS);
    }
    rt
}

/// Runs `frames` frames, `step` before each, and times step and frame together.
fn measure(
    rt: &mut Runtime<State>,
    frames: usize,
    mut step: impl FnMut(&mut Runtime<State>, usize),
) -> Vec<f64> {
    let mut cpu = Vec::with_capacity(frames);
    for i in 0..frames {
        let start = rune::thread_cpu_ms();
        step(rt, i);
        rt.frame(1.0 / FPS);
        cpu.push(rune::thread_cpu_ms() - start);
    }
    cpu
}

fn report(name: &str, note: &str, mut cpu: Vec<f64>) {
    let frames = cpu.len();
    cpu.sort_by(f64::total_cmp);
    let at = |q: f64| cpu[((frames - 1) as f64 * q).round() as usize];
    let over = cpu.iter().filter(|ms| **ms > 2.0).count();
    println!(
        "{name:<10} p50 {:.3} ms  p95 {:.3}  p99 {:.3}  max {:.3}  over 2 ms: {over}/{frames}  {note}",
        at(0.5),
        at(0.95),
        at(0.99),
        at(1.0),
    );
}

fn mounted(rt: &Runtime<State>, id: &str) -> bool {
    rt.find(id).is_some()
}

fn main() {
    if cfg!(debug_assertions) {
        panic!("run with `cargo bench`, which optimizes");
    }
    // `cargo bench` passes `--bench`; a workload's name narrows the run to it.
    let only = std::env::args().skip(1).find(|a| !a.starts_with('-'));
    let wants = |name: &str| only.as_deref().is_none_or(|o| o == name);
    let newest = format!("m{}", MESSAGES - 1);

    if wants("scroll") {
        let mut rt = runtime();
        let cpu = measure(&mut rt, 960, |rt, i| {
            if i == 480 {
                assert!(
                    !mounted(rt, &newest),
                    "4 s of scrolling left the newest message behind"
                );
            }
            rt.wheel(POINTER.0, POINTER.1, if i < 480 { 12.0 } else { -12.0 });
        });
        assert!(mounted(&rt, &newest), "and 4 s back returned to it");
        report(
            "scroll",
            "12 pt a frame up through history for 4 s, then back",
            cpu,
        );
    }
    if wants("fling") {
        let mut rt = runtime();
        let cpu = measure(&mut rt, 120, |rt, _| rt.wheel(POINTER.0, POINTER.1, 400.0));
        assert!(!mounted(&rt, "m0"), "the fling ran out of rows to mount");
        report(
            "fling",
            "400 pt a frame for 1 s into rows never measured",
            cpu,
        );
    }
    if wants("append") {
        let mut rt = runtime();
        let cpu = measure(&mut rt, 480, |rt, i| {
            if i.is_multiple_of(12) {
                rt.update(|s, tx| {
                    let at = chrono::Utc::now().timestamp_micros();
                    let n = s.session.entries.len();
                    s.session.entries.push(common::message(
                        &format!("m{n}"),
                        "grace",
                        &body(n),
                        at,
                    ));
                    s.session.revision += 1;
                    tx.scroll_to_bottom("messages");
                });
            }
        });
        assert_eq!(rt.state.session.entries.len(), MESSAGES + 40);
        report(
            "append",
            "a message every 12 frames, pinned to the end: the mounted rows rebuilt",
            cpu,
        );
    }
    if wants("open") {
        let start = rune::thread_cpu_ms();
        let mut rt = Runtime::new(state(), ui::view, SIZE, Env::new(false, 2.0));
        let made = rune::thread_cpu_ms() - start;
        let start = rune::thread_cpu_ms();
        rt.frame(0.0);
        let first = rune::thread_cpu_ms() - start;
        println!(
            "open       building the state {made:.1} ms, the first frame {first:.1} ms (fonts load in it)"
        );
    }
    // What mounting a row adds over the kit's cached build: the source parsed and its
    // elements made, every time. Layout and text shaping are the frame's, not this.
    if wants("build") {
        let bodies: Vec<String> = (0..MESSAGES).map(body).collect();
        let look = ui::components::Look::of(false);
        let _ = ui::body::view(&bodies[4], true, look);
        let time = |pass: &dyn Fn(&str)| {
            let mut each: Vec<f64> = bodies
                .iter()
                .map(|source| {
                    let start = rune::thread_cpu_ms();
                    pass(source);
                    (rune::thread_cpu_ms() - start) * 1000.0
                })
                .collect();
            each.sort_by(f64::total_cmp);
            (
                each[each.len() / 2],
                each[each.len() * 99 / 100],
                each[each.len() - 1],
            )
        };
        let (p50, p99, max) = time(&|source| drop(std::hint::black_box(ui::body::parse(source))));
        println!("build      parsing a body: p50 {p50:.1} µs  p99 {p99:.1}  max {max:.1}");
        let (p50, p99, max) =
            time(&|source| drop(std::hint::black_box(ui::body::view(source, true, look))));
        println!(
            "build      parsing and building its elements: p50 {p50:.1} µs  p99 {p99:.1}  max {max:.1}"
        );
    }
    // Cmd+A in a message body takes whatever selection group the body is in: one message
    // with the kit's Markdown, the whole channel with the shell's own renderer. Reading the
    // selection builds every row it spans.
    if wants("selectall") {
        let mut rt = runtime();
        let (x, y) = rt
            .elements()
            .into_iter()
            .filter(|e| {
                e.path.contains(&format!("/{newest}/"))
                    && e.path.contains("/body")
                    && e.text.is_some()
            })
            .map(|e| (e.rect[0] + 4.0, e.rect[1] + e.rect[3] / 2.0))
            .next()
            .expect("the newest body is mounted");
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / FPS);
        let start = rune::thread_cpu_ms();
        rt.key(KeyEvent {
            key: Key::Command('a'),
            mods: Mods {
                cmd: true,
                ..Mods::default()
            },
        });
        rt.frame(1.0 / FPS);
        let selected = rune::thread_cpu_ms() - start;
        let start = rune::thread_cpu_ms();
        let text = rt.selected_text().unwrap_or_default();
        let copied = rune::thread_cpu_ms() - start;
        println!(
            "selectall  Cmd+A {selected:.2} ms, reading the selection {copied:.2} ms: {} bytes, {} lines, of {MESSAGES} messages",
            text.len(),
            text.lines().count(),
        );
        rt.pointer_move(POINTER.0, POINTER.1);
        let cpu = measure(&mut rt, 480, |rt, _| rt.wheel(POINTER.0, POINTER.1, 12.0));
        report(
            "selected",
            "12 pt a frame up for 4 s with that selection held",
            cpu,
        );
        let cpu = measure(&mut rt, 120, |rt, _| rt.wheel(POINTER.0, POINTER.1, 400.0));
        report(
            "selected",
            "400 pt a frame for 1 s with that selection held",
            cpu,
        );
    }
}
