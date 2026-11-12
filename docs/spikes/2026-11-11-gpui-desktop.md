# Spike: the desktop shell on GPUI, and the two toolkits side by side

Date: 2026-11-11. The second half of the question in [the Rune spike](2026-11-11-rune-desktop.md): the same shell, over the same store, built on Zed's GPUI, so the two Rust-native candidates for one desktop shell on macOS, Windows and Linux can be compared on one workload. Like the first, it decides nothing in architecture 13 on its own.

The code is `spikes/gpui`; the store, the seeder and the fonts both spikes use are `spikes/data`.

## Setup

- **Which GPUI.** The `gpui` crate Zed publishes is 0.2.2 from October 2025, a year behind Zed's tree, and has no accessibility. Applications outside Zed take GPUI one of two ways: a git dependency on the Zed monorepo (540 MB), or `gpui-pre`, snapshots of Zed's crates republished to crates.io by the author of GPUI Kit (13 since September 2026; 0.3.12 is Zed at `d2a4fb6`). The spike uses the second, through [GPUI Kit](https://github.com/longbridge/gpui-kit) 0.9.1 (Longbridge's component library, the gpui-component project, Apache-2.0, which pins `gpui-pre` exactly and re-exports it).
- A workspace of its own on Rust 1.99.1, the compiler the Rune spike uses; Zed pins 1.98.1. The lock is resolved fresh: seeded from the core's lock it does not resolve, because an optional dependency of the kit pins `taffy` to an exact version.
- The core is the same path dependency, linked directly. The store is the Rune spike's: 10 channels of 10,000 messages.
- The shell is the same layout from the same tokens and fonts. The list is GPUI's `list` with a bottom-aligned `ListState`; bodies are the kit's `TextView::markdown`; the composer is its `Textarea`. 353 lines of view code (Rune's is 287), compiling on the second try.
- Apple M2 Ultra, macOS 27.2.1, a 60 Hz display at 2x, the same `perf` profile.

## Results against section 17

| Row | Budget | GPUI | Rune |
|---|---|---|---|
| Cold start to the last-viewed channel's first frame in a window, 100k local messages | 1 s | **370 ms** (342 ms with 200 messages loaded) | **300 ms** (294 ms) |
| Scroll a 10k-message channel at 60 fps: frame CPU, windowless | 16.7 ms | 0.64 ms p50, 1.0 ms p99, 2.6 ms max | 0.12 ms p50, 0.47 ms p99, 0.59 ms max |
| The same in a window: frames begun late, of 900 | none | **2, both at launch**; p95 interval 16.75 ms | **9 to 19, through the run**; p95 interval 18.6 to 19.4 ms |
| The same: process CPU while scrolling | | 43% of one core; main thread busy 19% of the time | 16% of one core; main thread busy 10% |
| Idle RSS | 150 MB with 5 instances | 118 MB (109 MB with 200 messages); footprint 77 MB | 117 MB (108 MB); footprint 74 to 83 MB |
| Idle CPU, 60 s | | 0.00 s | 0.18 s |

The start-up row is a real window for both, timed from process launch to a line the app prints at its first frame (GPUI prints it at the start of the following frame, so its figure includes up to one 16.7 ms interval). The 87 ms in the Rune write-up is Rune's offscreen path; a window costs it about 200 ms more than that.

"Frames begun late" counts intervals over 25 ms between the starts of consecutive frames while the list scrolls about three pages a second with nothing else happening, three runs each, no tracing in either. GPUI's display-link loop held 16.7 ms throughout. Rune started a frame a refresh late one to two times in a hundred, which matches what its own present-time trace and its own `pacing` example showed in the first spike. Its frames are cheaper and its pacing is worse.

Frame costs, windowless at 120 Hz over the 10,000-message channel: this thread's CPU from input dispatch through the built scene, GPU submission excluded. GPUI's harness is its test platform with the real text system; it draws every frame, where a window draws only dirty ones, so in the rows where most frames have nothing to do GPUI's p50 is the cost of redrawing a clean window and the p99 and max are the frames that did the work.

| Workload | GPUI p50 | p99 | max | Rune p50 | p99 | max |
|---|---|---|---|---|---|---|
| Scroll up 4 s, then back, 12 pt a frame | 0.64 ms | 1.03 | 2.58 | 0.12 ms | 0.47 | 0.59 |
| Fling, 400 pt a frame | 1.49 | 2.43 | 3.60 | 0.58 | 1.01 | 1.47 |
| Type a character a frame in the composer | 0.79 | 0.95 | 1.48 | 0.32 | 0.42 | 0.63 |
| A message arrives every 12 frames | 0.60 | 0.81 | 1.09 | 0.10 | 2.29 | 2.48 |
| Switch channel every 24 frames | 0.63 | 11.75 | 12.98 | 0.13 | 10.23 | 10.88 |
| Reveal the oldest message | 0.53 | 0.65 | 4.31 | 0.09 | 0.39 | 1.72 |

GPUI redraws the whole window on every dirty frame and costs two to five times what Rune does for the same frame. Neither comes near the budget: GPUI's worst scrolling frame is 3.6 ms. The channel switch is the same 8 ms SQLite read in both.

Size: 30.2 MB, 23.4 MB stripped (Rune 34.1 and 28.6). A clean `perf` build takes 52 s (Rune 62 s). On macOS the app compiles 592 crates, 411 of them under GPUI Kit (Rune 430 and 208).

## Text, input, accessibility

| | GPUI with GPUI Kit | Rune |
|---|---|---|
| Message bodies | `TextView::markdown`: every block of the subset. Code blocks are plain unless a highlighter feature is enabled | Kit `Markdown`: every block, code highlighted |
| Scripts | Japanese, Chinese, Cyrillic, joined emoji correct. Arabic is shaped but set from the left | The same, Arabic set from the right; the Japanese full stop is drawn centred, as from a Chinese face |
| Selection across messages | **Works as built**: the kit's selection is window-level, so a drag from one Markdown body into the next selects both (a test) | Not with kit Markdown as shipped (each is its own selection); works with Lumia's own text in one group, or with a 13-line patch to the kit (tests) |
| Composition | The kit's `Textarea` through GPUI's input handler: a preedit is a marked range, a commit replaces it, Return then sends (a test) | Preedit drawn apart, Return while composing does not send (a test) |
| Delivery of input methods from the OS | GPUI's own: `NSTextInputClient` on macOS, IME messages on Windows, `text-input-v3` and XIM on Linux, the code Zed ships | Through winit, which Rune's plan calls incomplete |
| Accessibility | AccessKit on all three. The tree exists only once an assistive client connects and the test window cannot connect one, so there was none to read headlessly; the composer reads as a named multi-line text input through the kit's test support (a test). Roles are opt-in per element | AccessKit on all three; the tree is readable headlessly and read correctly |
| Headless tooling | Under `test-support`: a headless window with real text and a screenshot, driven from code | Built into every app: flags for snapshots, scripted input, video, the tree |

None of the hand checks in the Rune write-up have been done for GPUI either: nobody has typed Japanese into this window or run VoiceOver over it.

## Reading

- **Both clear every budget on macOS by a wide margin**, so the budget rows do not choose between them.
- **Rune is cheaper per frame; GPUI is steadier.** A scrolled frame costs Rune a third to a fifth of the CPU, and GPUI still began every frame on time where Rune began one or two in a hundred late. On a 60 Hz display that is the visible difference between them, and it favours GPUI.
- **The pieces a chat client needs exist on both, in different places.** GPUI Kit has cross-message selection, a message scroller, a textarea and Markdown that work together as shipped. Rune has the better list primitive and tooling, and needs a small patch to its kit, or Lumia's own body renderer, for selection.
- **Input methods are the clearest difference in risk.** GPUI's are implemented per platform and exercised daily by Zed's users, including on Wayland and X11. Rune's go through winit on every desktop today, with open issues on Wayland.
- **GPUI's supply chain is the awkward part.** The stack that works is three parties deep: Zed's tree, one person's republished snapshots of it, and Longbridge's kit pinned to a snapshot. Zed's own crate is a year stale. The alternative is a git dependency on the monorepo and building without the kit. Rune's is one repository at a pinned rev, three weeks old, with two patched forks to carry.
- **Maturity runs the same way as in the first reading.** GPUI has shipped an editor on three desktops for years and the kit a commercial app; Rune has not shipped an app yet.

On this evidence GPUI with GPUI Kit is the lower-risk base for the desktop shell and Rune the faster, younger one. What would change that is a hand check: if a Japanese input method and VoiceOver work in both windows, Rune's remaining gap is pacing and age; if either fails in the Rune window, the question is settled for now.

## Not measured here

- The same hand checks as the Rune spike, for both windows.
- Windows and Linux for either.
- Selection across rows GPUI's list has unmounted, and the kit's `MessageScroller` (the spike drives `list` directly).
- A 120 Hz display, where GPUI's main thread would have 8.3 ms a frame against the roughly 3 ms it used here.
- GPUI without the kit, on a git dependency: what Lumia would write itself for the composer, Markdown and selection.
