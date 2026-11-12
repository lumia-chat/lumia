//! The shell: server rail, channel sidebar, and a conversation panel with a virtualized message
//! list and a composer. Layout follows the design tokens' sizes; nothing here is a kit component
//! except the Markdown in a message body.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use lumia_client::app::App;
use rune::{Cx, El, GrowthSpec, ScrollSpec, TextEdit, Tx, col, el, row, text, text_input};
use rune_kit::{Markdown, MarkdownStyle};

use crate::data::{self, Manifest};
use crate::theme::{self, Palette, size, type_};

/// The `bodies` scene's messages.
pub const BODIES: &[&str] = &[
    "Release notes:\n\n- **Gateway** resume is `O(1)` in channels\n- Invites by [email](https://lumia.chat)\n\n1. Tag\n2. Ship\n\n> Two containers, nothing else.",
    "The cursor fix:\n\n```rust\nfn resume(cursor: u64) -> u64 {\n    cursor + 1 // contiguous or refused\n}\n```",
    "了解しました。明日の朝に確認します 🙏\n\n好的，我明天早上看一下。\n\nتمام، سأراجعها غدًا صباحًا ثم أرد هنا.\n\nСпасибо, посмотрю после обеда 👩🏽‍💻",
];

pub struct Msg {
    pub key: String,
    /// Index into `State::authors`.
    pub author: usize,
    pub time: String,
    pub body: String,
}

pub struct State {
    pub app: Arc<App>,
    pub manifest: Manifest,
    /// The manifest's authors, then "You" for what the composer sends.
    pub authors: Vec<String>,
    author_index: HashMap<String, usize>,
    pub current: usize,
    pub messages: Vec<Msg>,
    pub draft: TextEdit,
    /// Bumped whenever `messages` is replaced, so the list knows its keys changed.
    pub generation: u64,
    markdown: [MarkdownStyle; 2],
    /// Bodies as plain selectable text in one selection group over the whole list, in place of
    /// kit Markdown, whose every block is a selection of its own (`LUMIA_SPIKE_PLAIN`).
    pub plain: bool,
    /// Kit Markdown bodies joined into one selection group over the whole list, which the kit
    /// cannot do without `patches/kit-selection-joined.patch` (`LUMIA_SPIKE_JOINED`, and the
    /// `kit-patch` feature).
    pub joined: bool,
    /// When `poll` last paged the list (`LUMIA_SPIKE_AUTOSCROLL`).
    paged_at: f64,
    /// When each frame's `poll` ran, while `LUMIA_SPIKE_FRAMES` counts them.
    frames: Vec<Instant>,
}

impl State {
    /// Reads the channel's newest messages from the local store through the core.
    pub fn open_channel(&mut self, index: usize) {
        let Some(channel) = self.manifest.channels.get(index) else { return };
        let started = Instant::now();
        let rows = match self.app.messages(&channel.id, data::limit()) {
            Ok(rows) => rows,
            Err(error) => {
                eprintln!("lumia-spike: reading #{}: {error}", channel.name);
                Vec::new()
            }
        };
        let fallback = self.authors.len() - 1;
        self.messages = rows
            .into_iter()
            .map(|m| Msg {
                author: self.author_index.get(&m.author_account_id).copied().unwrap_or(fallback),
                time: data::clock(m.created_at_micros),
                key: m.message_id,
                body: m.body,
            })
            .collect();
        self.current = index;
        self.generation += 1;
        if data::timing() {
            let ms = started.elapsed().as_secs_f64() * 1000.0;
            eprintln!("lumia-spike open #{} {} messages {ms:.1} ms", channel.name, self.messages.len());
        }
    }
}

pub fn init() -> State {
    let started = Instant::now();
    let dir = data::dir();
    let manifest = match data::manifest(&dir) {
        Ok(manifest) => manifest,
        Err(error) => panic!("no store in {}: {error}. Run `cargo run --release --bin seed` in spikes/data first.", dir.display()),
    };
    let app = match data::open(&dir) {
        Ok(app) => app,
        Err(error) => panic!("opening the store in {}: {error}", dir.display()),
    };
    let author_index = manifest.authors.iter().enumerate().map(|(i, a)| (a.id.clone(), i)).collect();
    let mut authors: Vec<String> = manifest.authors.iter().map(|a| a.name.clone()).collect();
    authors.push("You".into());
    let last_viewed = manifest.last_viewed;
    let mut state = State {
        app,
        manifest,
        authors,
        author_index,
        current: last_viewed,
        messages: Vec::new(),
        draft: TextEdit::new(""),
        generation: 0,
        markdown: [theme::markdown(false), theme::markdown(true)],
        plain: std::env::var_os("LUMIA_SPIKE_PLAIN").is_some(),
        joined: cfg!(feature = "kit-patch") && std::env::var_os("LUMIA_SPIKE_JOINED").is_some(),
        paged_at: 0.0,
        frames: Vec::new(),
    };
    state.open_channel(last_viewed);
    if data::timing() {
        eprintln!("lumia-spike init {:.1} ms", started.elapsed().as_secs_f64() * 1000.0);
    }
    state
}

fn send(s: &mut State, tx: &mut Tx) {
    let body = s.draft.text.trim().to_string();
    if body.is_empty() {
        return;
    }
    // Local only: the spike never signs in, so nothing goes through `App::send_message`.
    s.messages.push(Msg { key: format!("local-{}", s.messages.len()), author: s.authors.len() - 1, time: "now".into(), body });
    s.draft.clear();
    tx.scroll_to_bottom("messages");
}

/// With `LUMIA_SPIKE_AUTOSCROLL` set, pages the list up three times a second, so a window can
/// be measured scrolling with nobody at the trackpad. With `LUMIA_SPIKE_FRAMES=N` as well, it
/// stops after N frames and prints the spacing of their starts, as the GPUI spike does.
pub fn poll(s: &mut State, now: f64, tx: &mut Tx) -> bool {
    if std::env::var_os("LUMIA_SPIKE_AUTOSCROLL").is_none() {
        return false;
    }
    if let Some(limit) = std::env::var("LUMIA_SPIKE_FRAMES").ok().and_then(|v| v.parse::<usize>().ok()) {
        s.frames.push(Instant::now());
        if s.frames.len() > limit {
            report(&s.frames);
            std::process::exit(0);
        }
    }
    if now - s.paged_at >= 1.0 / 3.0 {
        s.paged_at = now;
        tx.scroll_page("messages", -1);
    }
    tx.wake_after(std::time::Duration::ZERO);
    false
}

/// The spacing of the frames' starts: when Rune began each frame, not when the display
/// showed it.
fn report(at: &[Instant]) {
    let mut gaps: Vec<f64> = at.windows(2).map(|w| (w[1] - w[0]).as_secs_f64() * 1000.0).collect();
    let long: Vec<usize> = gaps.iter().enumerate().filter(|(_, gap)| **gap > 25.0).map(|(i, _)| i).collect();
    gaps.sort_by(f64::total_cmp);
    let q = |q: f64| gaps[((gaps.len() - 1) as f64 * q).round() as usize];
    println!(
        "lumia-frames {} frames; interval p50 {:.2} p95 {:.2} max {:.2} ms; {} over 25 ms, after frames {long:?}",
        at.len(),
        q(0.5),
        q(0.95),
        q(1.0),
        long.len(),
    );
}

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    let dark = cx.system_dark();
    let p = theme::palette(dark);
    row().w_full().h_full().fill(p.bg).child(rail(s, p)).child(sidebar(s, p)).child(conversation(s, p, dark))
}

fn rail(s: &State, p: &Palette) -> El {
    let monogram = s.manifest.server.chars().next().unwrap_or('L').to_string();
    let home = el().key("home").size(size::TILE, size::TILE).radius(14.0).fill(p.tile).center()
        .role("button").label("Home")
        .child(el().size(16.0, 16.0).radius(5.0).fill(p.accent));
    let server = el().key("server").size(size::TILE, size::TILE).radius(size::TILE / 2.0).fill(p.raised).center()
        .role("button").label(s.manifest.server.clone())
        .child(text(monogram).font(type_::MONOGRAM).color(p.ink));
    // The top inset clears the traffic lights under a full-size content view.
    col().key("rail").w(size::RAIL).h_full().pt(44.0).pb(16.0).gap(12.0).items_center().fill(p.rail).window_drag()
        .child(home)
        .child(el().w(24.0).h(1.0).fill(p.line))
        .child(server)
}

fn sidebar(s: &State, p: &Palette) -> El {
    let title = row().w_full().h(size::HEADER).px(12.0).items_center().window_drag()
        .child(text(s.manifest.server.clone()).font(type_::TITLE_SERVER).color(p.ink));
    let label = text("Channels").font(type_::SECTION).color(p.ink_muted).px(10.0).py(6.0);
    let rows = s.manifest.channels.iter().enumerate().map(|(i, channel)| {
        let active = i == s.current;
        let mut item = row().key(format!("channel-{}", channel.name)).w_full().h(size::ROW).px(10.0).gap(8.0)
            .items_center().radius(8.0).hover(p.hover_row)
            .role("button").label(format!("{} channel", channel.name)).focusable(true)
            .on_press(move |s: &mut State| s.open_channel(i))
            .child(text("#").font(type_::NAV).color(p.ink_muted))
            .child(text(channel.name.clone())
                .font(if active { type_::NAV_ACTIVE } else { type_::NAV })
                .color(if active { p.ink_selected } else { p.ink_secondary }));
        if active {
            item = item.fill(p.raised);
        }
        item
    });
    // The top inset clears the traffic lights, which reach past the rail.
    col().key("sidebar").w(size::SIDEBAR).h_full().pt(28.0).pl(12.0).pr(4.0).pb(14.0).gap(2.0)
        .child(title)
        .child(label)
        .child(col().key("channels").w_full().gap(2.0).role("list").label("Channels").children(rows))
}

fn conversation(s: &State, p: &Palette, dark: bool) -> El {
    let name = s.manifest.channels.get(s.current).map_or("", |c| c.name.as_str());
    let header = row().key("header").w_full().h(size::HEADER).px(24.0).gap(10.0).items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], p.line).window_drag()
        .child(text(format!("# {name}")).font(type_::TITLE_CHANNEL).color(p.ink))
        .child(text(format!("{} messages", s.messages.len())).font(type_::META).color(p.ink_muted));

    let rows = col().key("rows").w_full().py(14.0).px(12.0)
        .growth(GrowthSpec::default())
        .role("list").label(format!("Messages in {name}"))
        .selection_group(s.plain || s.joined)
        .virtual_rows(
            s.messages.len(),
            64.0,
            |s: &State, i| Cow::Borrowed(s.messages[i].key.as_str()),
            move |s: &State, i| message(s, i, dark),
        )
        .virtual_rows_version((s.generation, s.messages.len(), s.plain, s.joined));
    let list = col().key("messages").w_full().grow(1.0).min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: true })
        .child(rows);

    let input = text_input(&s.draft, type_::BODY).key("composer")
        .label(format!("Message {name}"))
        .placeholder(format!("Message #{name}"), p.ink_muted).color(p.ink_field)
        .caret(p.accent, p.accent.alpha(0.28))
        .grow(1.0).min_h(22.0).max_h(180.0)
        .on_edit(|s: &mut State| &mut s.draft)
        .on_submit(send);
    let send_button = el().key("send").size(34.0, 34.0).radius(17.0).fill(p.accent).center()
        .role("button").label("Send").focusable(true)
        .child(text("↑").font(type_::AUTHOR).color(p.on_accent))
        .on_press_tx(send);
    let composer = col().w_full().px(24.0).pt(4.0).pb(16.0).child(
        row().key("composer-box").w_full().items_end().gap(8.0).pl(16.0).pr(8.0).py(8.0).radius(16.0)
            .fill(p.field).border(1.0, p.border_composer)
            .child(col().grow(1.0).min_w(0.0).py(6.0).child(input))
            .child(send_button),
    );

    let panel = col().key("panel").w_full().h_full().radius(18.0).fill(p.surface).border(1.0, p.panel_edge)
        .clip_rounded(18.0)
        .child(header)
        .child(list)
        .child(composer);
    col().grow(1.0).min_w(0.0).h_full().padding(8.0).child(panel)
}

fn message(s: &State, i: usize, dark: bool) -> El {
    let p = theme::palette(dark);
    let m = &s.messages[i];
    let author = s.authors[m.author].as_str();
    let (ground, ink) = p.tones[m.author % p.tones.len()];
    let initial = author.chars().next().unwrap_or('?').to_string();
    let avatar = el().size(size::AVATAR, size::AVATAR).radius(size::AVATAR / 2.0).fill(ground).center()
        .a11y_hidden()
        .child(text(initial).font(type_::AUTHOR).color(ink));
    let head = row().gap(8.0).items_center()
        .child(text(author.to_string()).font(type_::AUTHOR).color(p.ink))
        .child(text(m.time.clone()).font(type_::META).color(p.ink_muted));
    let body = if s.plain {
        text(m.body.clone()).key("body").font(type_::MESSAGE).color(p.ink_body).w_full()
            .selectable(true).selection_color(p.accent.alpha(0.28))
    } else {
        let markdown = Markdown::new(m.body.as_str()).selectable(true);
        #[cfg(feature = "kit-patch")]
        let markdown = markdown.selection_joined(s.joined);
        markdown.render(&s.markdown[dark as usize]).key("body")
    };
    row().key(m.key.clone()).w_full().px(12.0).py(8.0).gap(14.0).radius(12.0).hover(p.hover_message)
        .role("listitem")
        .child(avatar)
        .child(col().grow(1.0).min_w(0.0).gap(2.0).child(head).child(body))
}
