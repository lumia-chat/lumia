//! Windowless frame costs of the shell over the seeded store, and snapshots of it:
//! `cargo run --profile perf --features headless --bin bench [scroll|fling|type|append|switch|top]`
//! `cargo run --profile perf --features headless --bin bench snap out/dark.png [dark|light] [bodies]`
//! `cargo run --profile perf --features headless --bin bench tree out/tree.json`
//!
//! The same workloads as `spikes/rune`'s bench. Each drives a headless GPUI window (the test
//! platform with the real text system) the way a display would and reads this thread's CPU
//! clock around every frame: input dispatch, `render`, layout, prepaint and paint into the
//! scene. GPU submission is not in it. Background tasks the frame spawned run on this thread
//! too and are counted, where a window would run them on other cores.

use std::sync::Arc;

use gpui_kit::assets::Assets;
use gpui_kit::component::Root;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Entity, HeadlessAppContext, Keystroke, ListOffset, ScrollDelta, ScrollEvent,
    TouchPhase, Window, point, px, size,
};
use lumia_gpui_spike::shell::Shell;

const FPS: f64 = 120.0;
/// Over the message list, clear of the sidebar and the composer.
const POINTER: (f32, f32) = (700.0, 380.0);

/// The `bodies` scene's messages: every block of the body subset and a line per script.
const BODIES: &[&str] = &[
    "Release notes:\n\n- **Gateway** resume is `O(1)` in channels\n- Invites by [email](https://lumia.chat)\n\n1. Tag\n2. Ship\n\n> Two containers, nothing else.",
    "The cursor fix:\n\n```rust\nfn resume(cursor: u64) -> u64 {\n    cursor + 1 // contiguous or refused\n}\n```",
    "了解しました。明日の朝に確認します 🙏\n\n好的，我明天早上看一下。\n\nتمام، سأراجعها غدًا صباحًا ثم أرد هنا.\n\nСпасибо, посмотрю после обеда 👩🏽‍💻",
];

struct Harness {
    cx: HeadlessAppContext,
    window: AnyWindowHandle,
    shell: Entity<Shell>,
}

fn cpu_ms() -> f64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `ts` is a valid timespec for the call to fill.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    ts.tv_sec as f64 * 1000.0 + ts.tv_nsec as f64 / 1e6
}

impl Harness {
    fn new(dark: bool) -> Self {
        let text_system = gpui_kit::platform::current_platform(true).text_system();
        let mut cx =
            HeadlessAppContext::with_platform(text_system, Arc::new(Assets), gpui_kit::platform::current_headless_renderer);
        cx.update(gpui_kit::init);
        let mut shell = None;
        let window = cx
            .open_window(size(px(1180.), px(760.)), |window, cx| {
                lumia_gpui_spike::init(dark, cx);
                let view = cx.new(|cx| Shell::new(dark, window, cx));
                shell = Some(view.clone());
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("the headless window opens");
        let mut harness = Harness { cx, window: window.into(), shell: shell.expect("the shell was built") };
        for _ in 0..30 {
            harness.frame(|_, _| {});
        }
        harness
    }

    /// One frame: `step`, then a draw, then whatever the frame left for the executor.
    fn frame(&mut self, step: impl FnOnce(&mut Window, &mut App)) -> f64 {
        let start = cpu_ms();
        self.cx
            .update_window(self.window, |_, window, cx| {
                step(window, cx);
                window.draw(cx).clear(cx);
            })
            .expect("the window is open");
        self.cx.run_until_parked();
        cpu_ms() - start
    }

    fn measure(&mut self, frames: usize, mut step: impl FnMut(&Entity<Shell>, usize, &mut Window, &mut App)) -> Vec<f64> {
        let shell = self.shell.clone();
        (0..frames).map(|i| self.frame(|window, cx| step(&shell, i, window, cx))).collect()
    }

    /// The index of the message at the top of the list's viewport.
    fn top(&mut self) -> usize {
        self.cx.update(|cx| self.shell.read(cx).list.logical_scroll_top().item_ix)
    }

    fn count(&mut self) -> usize {
        self.cx.update(|cx| self.shell.read(cx).messages.len())
    }
}

fn wheel(dy: f32, window: &mut Window, cx: &mut App) {
    let event = ScrollEvent {
        position: point(px(POINTER.0), px(POINTER.1)),
        delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
        touch_phase: TouchPhase::Moved,
        ..Default::default()
    };
    window.dispatch_event(gpui_kit::PlatformInput::Scroll(event), cx);
}

fn report(name: &str, note: &str, mut cpu: Vec<f64>) {
    let frames = cpu.len();
    cpu.sort_by(f64::total_cmp);
    let at = |q: f64| cpu[((frames - 1) as f64 * q).round() as usize];
    let over = cpu.iter().filter(|ms| **ms > 1000.0 / FPS).count();
    println!(
        "{name:<8} p50 {:.3} ms  p95 {:.3}  p99 {:.3}  max {:.3}  over 8.33 ms: {over}/{frames}  {note}",
        at(0.5),
        at(0.95),
        at(0.99),
        at(1.0),
    );
}

fn snap(path: &str, dark: bool, bodies: bool) {
    let mut h = Harness::new(dark);
    if bodies {
        let shell = h.shell.clone();
        h.frame(|_, cx| {
            shell.update(cx, |shell, cx| {
                for (i, body) in BODIES.iter().enumerate() {
                    shell.push(i, *body, cx);
                }
            })
        });
    }
    for _ in 0..10 {
        h.frame(|_, _| {});
    }
    let image = h.cx.capture_screenshot(h.window).expect("the headless renderer captures the window");
    image.save(path).expect("the snapshot is written");
    println!("{path}: {}x{}", image.width(), image.height());
}

fn main() {
    if cfg!(debug_assertions) {
        panic!("build with --profile perf");
    }
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("snap") {
        let path = args.get(2).expect("snap PATH [dark|light] [bodies]");
        return snap(path, args.get(3).is_none_or(|t| t != "light"), args.iter().any(|a| a == "bodies"));
    }
    if args.get(1).map(String::as_str) == Some("tree") {
        let path = args.get(2).expect("tree PATH");
        let mut h = Harness::new(true);
        let json = h
            .cx
            .update_window(h.window, |_, window, _| window.debug_a11y_tree_json())
            .expect("the window is open")
            .expect("the window keeps an accessibility tree");
        std::fs::write(path, json).expect("the tree is written");
        return;
    }
    let wants = |name: &str| args.get(1).is_none_or(|only| only == name);

    if wants("scroll") {
        let mut h = Harness::new(true);
        let newest = h.top();
        let up = h.measure(480, |_, _, window, cx| wheel(12.0, window, cx));
        let turned = h.top();
        assert!(turned + 20 < newest, "4 s of scrolling left the newest messages behind: {newest} to {turned}");
        let mut all = up;
        all.extend(h.measure(480, |_, _, window, cx| wheel(-12.0, window, cx)));
        assert!(h.top() > turned, "and 4 s back returned toward them");
        report("scroll", "12 pt a frame up through history for 4 s, then back", all);
    }
    if wants("fling") {
        let mut h = Harness::new(true);
        let newest = h.top();
        let cpu = h.measure(480, |_, _, window, cx| wheel(400.0, window, cx));
        let reached = h.top();
        report("fling", &format!("400 pt a frame into rows never measured; from message {newest} to {reached}"), cpu);
    }
    if wants("type") {
        let mut h = Harness::new(true);
        let shell = h.shell.clone();
        h.frame(|window, cx| {
            let composer = shell.read(cx).composer.clone();
            composer.update(cx, |composer, cx| composer.focus(window, cx));
        });
        let cpu = h.measure(240, |_, i, window, cx| {
            let key = if i % 6 == 5 { "space" } else { ["a", "b", "c", "d", "e"][i % 5] };
            let keystroke = Keystroke::parse(key).expect("a valid keystroke").with_simulated_ime();
            window.dispatch_keystroke(keystroke, cx);
        });
        let typed = h.cx.update(|cx| h.shell.read(cx).composer.read(cx).value().len());
        assert_eq!(typed, 240, "every key reached the composer");
        report("type", "a character a frame in the composer, the 10k-row list beside it", cpu);
    }
    if wants("append") {
        let mut h = Harness::new(true);
        let before = h.count();
        let cpu = h.measure(480, |shell, i, _, cx| {
            if i % 12 == 0 {
                shell.update(cx, |shell, cx| {
                    shell.push(i % 5, "A message arrives while the list is pinned to its end.", cx)
                });
            }
        });
        assert_eq!(h.count(), before + 40);
        report("append", "a message every 12 frames, pinned to the end", cpu);
    }
    if wants("switch") {
        let mut h = Harness::new(true);
        let cpu = h.measure(240, |shell, i, window, cx| {
            if i % 24 == 0 {
                shell.update(cx, |shell, cx| {
                    let next = (shell.current + 1) % shell.manifest.channels.len();
                    shell.open_channel(next, window, cx);
                });
            }
        });
        report("switch", "another channel every 24 frames: 10k rows read from SQLite, the list reset", cpu);
    }
    if wants("top") {
        let mut h = Harness::new(true);
        let cpu = h.measure(240, |shell, i, _, cx| {
            if i == 0 {
                shell.update(cx, |shell, cx| {
                    shell.list.scroll_to(ListOffset { item_ix: 0, offset_in_item: px(0.) });
                    cx.notify();
                });
            }
        });
        assert_eq!(h.top(), 0, "the oldest message is at the top");
        report("top", "reveal the oldest of 10k messages from the newest", cpu);
    }
}
