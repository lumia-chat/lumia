# Spike: the desktop shell on Rune, over the core in-process

Date: 2026-11-11. Architecture 13 puts macOS on SwiftUI and Windows and Linux on the Tauri shell, and rejects the Rust-native toolkits as weak on rich text, input methods and accessibility. The question here: could one Rust-native shell on [Rune](https://github.com/anomalyco/rune) carry all three desktops, starting with macOS? This spike builds the shell on Rune over `lumia_client::app::App` and measures it against section 17's client rows on a Mac. It decides nothing in section 13 on its own; the reading at the end says what it does and does not support.

The code is `spikes/rune`, with its own README for running it; the store, the seeder and the fonts are `spikes/data`, shared with [the GPUI spike](2026-11-11-gpui-desktop.md), which builds the same shell on GPUI and sets the two side by side.

## Setup

- Rune at `1f1b6c2` of its `dev` branch (2026-11-11), version 0.0.1. The repository was created on 2026-10-22 and calls itself pre-release: an app pins a rev. It builds against its own patched `wgpu-hal` and `winit`, and since a dependency's `[patch]` does not reach the workspace that depends on it, the app's manifest repeats both patches.
- A workspace of its own on Rust 1.99.1, Rune's pin. The core is on 1.96 and builds unchanged under the newer compiler.
- The core is a path dependency: `lumia-client` and `lumia-core`, linked directly, no UniFFI and no binding crate. The shell calls `App::open` with an opener over four SQLite files and reads a channel with `App::messages`.
- The store is seeded through `Store::apply` with `MessageCreated` events, the path a synced event takes: 10 channels of 10,000 messages, 100,000 in all, a 54 MB `instance.db`. Bodies are in the protocol's Markdown subset: mostly one line, some paragraphs, and a tail of lists, code blocks, quotes, and lines in Japanese, Chinese, Korean, Arabic, Russian and emoji.
- The shell: server rail, channel sidebar, a conversation panel with the message list and a composer, laid out from `assets/design/tokens.json` (colours read from the file at start) in Albert Sans and Bricolage Grotesque. Rune reads TrueType, so the WOFF2 files are unpacked first. The list is Rune's `virtual_rows` in a bottom-pinned scroll viewport, bodies are `rune-kit`'s `Markdown`, the composer is a multi-line `text_input`. It is 287 lines of view code, and it compiled against the API on the second try.
- Apple M2 Ultra, macOS 27.2.1, a 60 Hz display at 2x, Rune's `perf` profile (release, no incremental).

## Results against section 17

| Row | Budget | Measured |
|---|---|---|
| Cold start to interactive, last-viewed channel rendered from SQLite, 100k local messages | 1 s | **300 ms** from process launch to a window's first frame with the channel's 10,000 messages loaded (294 ms with its newest 200); **87 ms** to the first readable pixels offscreen (78 ms). Of either, opening the stores and reading the channel through the core is 8 to 10 ms (2 to 3 ms). OS file cache warm, one instance where the row says five |
| Scroll a 10k-message channel at 60 fps | 16.7 ms a frame | Frame CPU **0.12 ms p50, 0.47 ms p99, 0.59 ms max** scrolling at 12 pt a frame; 0.58 ms p50 and 1.5 ms max flinging at 400 pt a frame into rows never measured. In a real window paging continuously, 896 to 899 of 900 frames were presented and 4 to 26 of the intervals between them were a doubled 33 ms (0.4% to 2.9%, nine runs) |
| Idle desktop client RSS, 5 instances connected | 150 MB | **117 MB** RSS (74 to 83 MB physical footprint) with one store open, 10,000 messages in the list and no connection. 108 MB with 200 messages, so the list's data is about 9 MB and the rest is the window: 41 MB of it is IOSurface |

The doubled intervals are not the list's doing: Rune's own `pacing` example, a few springs and no list, doubled 0.7% to 2.2% of its intervals in runs interleaved with the spike's. They are not the trace's either: with tracing off, 9 to 19 of 900 frames still began more than 25 ms after the one before. And they are not this desktop session's: the GPUI spike, on the same display, began every frame on time. It is Rune's frame pacing.

Frame costs, windowless at 120 Hz over the 10,000-message channel (this thread's CPU from input dispatch through the draw list; GPU submission excluded, text in Rune's built-in Inter):

| Workload | p50 | p99 | max | Note |
|---|---|---|---|---|
| Scroll up 4 s, then back, 12 pt a frame | 0.12 ms | 0.47 ms | 0.59 ms | A commit every 22 frames as rows mount |
| Fling, 400 pt a frame | 0.58 ms | 1.01 ms | 1.47 ms | Every frame mounts rows it has never measured |
| Type a character a frame in the composer | 0.32 ms | 0.42 ms | 0.63 ms | The view re-runs on every key; the list costs it nothing |
| A message arrives every 12 frames, pinned to the end | 0.10 ms | 2.29 ms | 2.48 ms | The arrival frames: one key call per row, 10,001 rows |
| Switch channel every 24 frames | 0.13 ms | 10.23 ms | 10.88 ms | The switch frame: 10,000 rows read from SQLite (about 8 ms) and the list remounted |
| Reveal the oldest message from the newest | 0.09 ms | 0.39 ms | 1.72 ms | |

The same script run headless with the GPU in the loop (scroll, fling, type, send; 1,009 frames at 120 fps) cost 0.37 ms p50 and 1.1 ms p95 a frame including encode and submit. The first frame is 15 ms. The plain window used 16% of one core while paging at 60 fps and 0.18 s of CPU in 60 s idle: it draws nothing at rest.

Size: the binary is 34.1 MB, 28.6 MB stripped, with the core, SQLite, OpenMLS, rustls and the fonts inside. A clean `perf` build takes 62 s on this machine. On macOS the app compiles 430 crates, 208 of them under Rune.

## Text, input, accessibility

These are the three things architecture 13 rejected the Rust-native toolkits over.

- **Rich text.** Message bodies render through `rune-kit`'s Markdown: bold, inline code, links, bulleted and numbered lists, quotes and highlighted code blocks (the `bodies` scene shows each). Japanese, Chinese, Cyrillic and emoji with skin tones and joiners fall back to system fonts beside Albert Sans, and an Arabic paragraph is shaped and set from the right. One flaw seen: the full stop in the Japanese line is drawn centred, as a Traditional Chinese face draws it, so the fallback does not appear to choose a face by language. The composer takes Markdown source as plain text here; Rune also has an attributed rich-text field (`TextEdit::rich`) that the spike did not use.
- **Selection.** A drag selects within a message. Across messages it depends on how the body is built: each kit `Markdown` is a selection group of its own, so a drag stops at the end of the message it began in. With bodies as selectable text in one group over the whole list, a drag runs from one message into the next and copies them joined by a newline, and Rune collects the text of rows scrolled out of the mounted window. Both are tests in `spikes/rune/tests`. A group around the list does not change this (a group inside a group stays its own, also a test), and Rune documents it: the kit's selectable Markdown is one block that Cmd+A selects and that can copy its own source. So cross-message selection takes one of two things. Either Lumia's own renderer for the body subset over Rune's `rich` text, or a 13-line patch to the kit (`spikes/rune/patches/kit-selection-joined.patch`) that lets a Markdown's texts join the group around it. With the patch a drag runs from one message through a code block into the next, Cmd+A takes the whole channel including rows never mounted, and frame costs are unchanged (tests and a bench workload behind the spike's `kit-patch` feature). What it costs: a fork of `rune-kit` to carry, since Rune takes no pull requests; copies put the same blank line between messages as between a message's paragraphs; and a Markdown's own copy-as-source no longer answers once a selection leaves it. Two limits hold for either route, being Rune's selection over virtual rows and not the kit's: with a whole 10,000-message channel selected, each scrolled frame that mounts rows costs 43 to 48 ms with Markdown bodies and 16 ms with plain text (5 ms at 1,000 messages), and a copy across unbuilt rows is cut without notice at 10,000 rows or 4 MiB (Rune's issue 550).
- **Input methods.** Driven through the runtime with composition events: marked text is drawn and not inserted, Return while composing goes to the input method and does not send, a commit inserts, and Return then sends (a test). That is Rune's handling of the events. Their delivery from macOS goes through winit, which Rune's own plan calls incomplete and means to replace with a direct `NSTextInputClient`; nobody has typed Japanese into this window yet.
- **Accessibility.** The tree Rune hands AccessKit for the window reads as the interface: the channel list as a list of buttons with click and focus actions, the message list as a list with a set size of 10,000 whose mounted items carry their position, each item named by author, time and text, and the composer as a multi-line text input with set-value and selection actions. VoiceOver itself was not run.

## Reading

- **Performance is not the question on macOS, pacing is.** Cold start is under a third of its budget and a scrolled frame costs a hundredth of a 60 Hz frame on the CPU. The numbers are from a fast desktop; the margin is wide enough that a slower Mac does not change the answer. But one or two frames in a hundred start late while scrolling, however little they cost.
- **Memory is the tight row.** A window holding 200 messages is already 108 MB of the 150 MB that five connected instances are budgeted. The SwiftUI app and the Tauri shell have not been measured the same way, so this is a number without a baseline yet.
- **The core fits without glue.** `App` is a Rust API and Rune's state is a Rust struct; the 1,355 lines of `lumia-ffi` have no counterpart on this path.
- **Two gaps showed up in the core, whatever the shell.** It keeps no channel list locally (`App::snapshot` asks the instance), so the spike had to write one beside the store to start offline; and `App::messages` reads only the newest N, with no page before a sequence number.
- **Nothing here reduces the risk of the dependency itself.** Rune is three weeks old, has most of its commits from one author, is by its own plan written and maintained by agents, takes no pull requests, and had 302 open issues to 14 closed on the day of the spike. Its documentation trails its code in places (the README says view construction visits every row and that there is no AccessKit; both are out of date). The spike pins one rev and carries two patched crates to build at all.

## Not measured here

- A person at the keyboard: a Japanese, Chinese or Korean input method, dead keys, the emoji picker, dictation; VoiceOver; trackpad momentum and rubber-banding; text beside the SwiftUI app's. The spike's README lists these as checks to do by hand.
- Windows and Linux. Rune's open input issues cluster on Wayland (input methods, dead keys, file drops), and Linux is where architecture 13's trigger sits.
- The gateway loop running in the same process, five instances, avatars and attachments, a 120 Hz display.
- What the macOS app has today through AppKit and SwiftUI: passkeys, the Sparkle updater, the keychain, the notification extension. Rune has native menus, link handling, file dialogs, spellcheck and (in `rune-notify`) notifications; passkeys and Sparkle would be bridged by hand.
- GPUI on the same workload is [its own write-up](2026-11-11-gpui-desktop.md), with the comparison and a reading across both.
