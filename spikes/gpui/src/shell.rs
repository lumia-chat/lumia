//! The shell: server rail, channel sidebar, and a conversation panel with a virtualized message
//! list and a composer. The same layout as `spikes/rune`, from the same tokens. The list is
//! GPUI's own `list`; message bodies and the composer are GPUI Kit's `TextView` and `Textarea`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::text::TextView;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, ElementId, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ListAlignment, ListState, ParentElement as _, Render, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Window, div, list, px,
};
use lumia_client::app::App as Client;

use crate::data::{self, Manifest};
use crate::theme::{self, DISPLAY, Palette, size};

pub struct Msg {
    pub id: SharedString,
    /// Index into `Shell::authors`.
    pub author: usize,
    pub time: SharedString,
    pub body: SharedString,
}

pub struct Shell {
    client: Arc<Client>,
    pub manifest: Manifest,
    /// The manifest's authors, then "You" for what the composer sends.
    pub authors: Vec<SharedString>,
    author_index: HashMap<String, usize>,
    pub current: usize,
    pub messages: Vec<Msg>,
    pub list: ListState,
    pub composer: Entity<TextareaState>,
    dark: bool,
    frames: Frames,
    _composer_events: Subscription,
}

/// What the environment asks a window to do for a measurement.
struct Frames {
    /// `LUMIA_SPIKE_AUTOSCROLL`: scroll the list up a little on every frame.
    autoscroll: bool,
    /// `LUMIA_SPIKE_FRAMES`: stop after this many frames and print their spacing.
    limit: Option<usize>,
    /// When each frame's `render` ran.
    at: Vec<Instant>,
}

impl Shell {
    pub fn new(dark: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let started = Instant::now();
        let dir = data::dir();
        let manifest = match data::manifest(&dir) {
            Ok(manifest) => manifest,
            Err(error) => {
                panic!("no store in {}: {error}. Run `cargo run --release --bin seed` in spikes/data first.", dir.display())
            }
        };
        let client = match data::open(&dir) {
            Ok(client) => client,
            Err(error) => panic!("opening the store in {}: {error}", dir.display()),
        };
        let author_index = manifest.authors.iter().enumerate().map(|(i, a)| (a.id.clone(), i)).collect();
        let mut authors: Vec<SharedString> = manifest.authors.iter().map(|a| a.name.clone().into()).collect();
        authors.push("You".into());

        let composer = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 8).submit_on_enter(true));
        let _composer_events = cx.subscribe_in(&composer, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { shift: false, .. }) {
                this.send(window, cx);
            }
        });

        let last_viewed = manifest.last_viewed;
        let mut shell = Shell {
            client,
            manifest,
            authors,
            author_index,
            current: last_viewed,
            messages: Vec::new(),
            // Rows two viewports either side stay laid out, as Rune's virtual rows do.
            list: ListState::new(0, ListAlignment::Bottom, px(1500.)),
            composer,
            dark,
            frames: Frames {
                autoscroll: std::env::var_os("LUMIA_SPIKE_AUTOSCROLL").is_some(),
                limit: std::env::var("LUMIA_SPIKE_FRAMES").ok().and_then(|v| v.parse().ok()),
                at: Vec::new(),
            },
            _composer_events,
        };
        shell.open_channel(last_viewed, window, cx);
        if data::timing() {
            eprintln!("lumia-spike init {:.1} ms", started.elapsed().as_secs_f64() * 1000.0);
        }
        shell
    }

    /// Reads the channel's newest messages from the local store through the core.
    pub fn open_channel(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(channel) = self.manifest.channels.get(index) else { return };
        let started = Instant::now();
        let rows = match self.client.messages(&channel.id, data::limit()) {
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
                time: data::clock(m.created_at_micros).into(),
                id: m.message_id.into(),
                body: m.body.into(),
            })
            .collect();
        self.current = index;
        self.list.reset(self.messages.len());
        let placeholder = format!("Message #{}", channel.name);
        self.composer.update(cx, |composer, cx| composer.set_placeholder(placeholder, window, cx));
        if data::timing() {
            let ms = started.elapsed().as_secs_f64() * 1000.0;
            eprintln!("lumia-spike open #{} {} messages {ms:.1} ms", channel.name, self.messages.len());
        }
        cx.notify();
    }

    /// Appends a message and keeps the list at its end.
    pub fn push(&mut self, author: usize, body: impl Into<SharedString>, cx: &mut Context<Self>) {
        let n = self.messages.len();
        self.messages.push(Msg { id: format!("local-{n}").into(), author, time: "now".into(), body: body.into() });
        self.list.splice(n..n, 1);
        self.list.scroll_to_end();
        cx.notify();
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let body = self.composer.read(cx).value().trim().to_string();
        if body.is_empty() {
            return;
        }
        // Local only: the spike never signs in, so nothing goes through `App::send_message`.
        self.push(self.authors.len() - 1, body, cx);
        self.composer.update(cx, |composer, cx| composer.set_value("", window, cx));
    }

    fn rail(&self, p: &Palette) -> impl IntoElement {
        let monogram: SharedString = self.manifest.server.chars().next().unwrap_or('L').to_string().into();
        // The top inset clears the traffic lights under a transparent titlebar.
        div().id("rail").w(px(size::RAIL)).h_full().pt(px(44.)).pb(px(16.)).flex().flex_col().items_center().gap(px(12.))
            .bg(p.rail)
            .child(
                div().id("home").size(px(size::TILE)).rounded(px(14.)).bg(p.tile).flex().items_center().justify_center()
                    .role(Role::Button).aria_label("Home")
                    .child(div().size(px(16.)).rounded(px(5.)).bg(p.accent)),
            )
            .child(div().w(px(24.)).h(px(1.)).bg(p.line))
            .child(
                div().id("server").size(px(size::TILE)).rounded_full().bg(p.raised).flex().items_center().justify_center()
                    .role(Role::Button).aria_label(SharedString::from(self.manifest.server.clone()))
                    .font_family(DISPLAY).text_size(px(18.)).font_weight(FontWeight::EXTRA_BOLD).text_color(p.ink)
                    .child(monogram),
            )
    }

    fn sidebar(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.manifest.channels.iter().enumerate().map(|(i, channel)| {
            let active = i == self.current;
            let hover = p.hover_row;
            div().id(SharedString::from(format!("channel-{}", channel.name)))
                .w_full().h(px(size::ROW)).px(px(10.)).flex().items_center().gap(px(8.)).rounded(px(8.))
                .role(Role::Button).aria_label(SharedString::from(format!("{} channel", channel.name)))
                .text_size(px(14.5))
                .when(active, |row| row.bg(p.raised).font_weight(FontWeight::BOLD))
                .hover(move |style| style.bg(hover))
                .on_click(cx.listener(move |this, _, window, cx| this.open_channel(i, window, cx)))
                .child(div().text_color(p.ink_muted).child("#"))
                .child(
                    div().text_color(if active { p.ink_selected } else { p.ink_secondary })
                        .child(SharedString::from(channel.name.clone())),
                )
        });
        // The top inset clears the traffic lights, which reach past the rail.
        div().id("sidebar").w(px(size::SIDEBAR)).h_full().pt(px(28.)).pl(px(12.)).pr(px(4.)).pb(px(14.))
            .flex().flex_col().gap(px(2.))
            .child(
                div().w_full().h(px(size::HEADER)).px(px(12.)).flex().items_center()
                    .font_family(DISPLAY).text_size(px(19.)).font_weight(FontWeight::BOLD).text_color(p.ink)
                    .child(SharedString::from(self.manifest.server.clone())),
            )
            .child(
                div().px(px(10.)).py(px(6.)).text_size(px(12.5)).font_weight(FontWeight::SEMIBOLD).text_color(p.ink_muted)
                    .child("Channels"),
            )
            .child(
                div().id("channels").w_full().flex().flex_col().gap(px(2.)).role(Role::List).aria_label("Channels")
                    .children(rows),
            )
    }

    fn conversation(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self.manifest.channels.get(self.current).map_or("", |c| c.name.as_str());
        let header = div().id("header").w_full().h(px(size::HEADER)).flex_none().px(px(24.)).flex().items_center().gap(px(10.))
            .border_b_1().border_color(p.line)
            .child(
                div().font_family(DISPLAY).text_size(px(21.)).font_weight(FontWeight::BOLD).text_color(p.ink)
                    .child(SharedString::from(format!("# {name}"))),
            )
            .child(
                div().text_size(px(12.5)).text_color(p.ink_muted)
                    .child(SharedString::from(format!("{} messages", self.messages.len()))),
            );

        let rows = list(self.list.clone(), cx.processor(|this, ix: usize, _window, _cx| this.message(ix)))
            .size_full();
        let messages = div().id("messages").w_full().flex_1().min_h_0().py(px(14.))
            .role(Role::List).aria_label(SharedString::from(format!("Messages in {name}")))
            .child(rows);

        let send = div().id("send").size(px(34.)).flex_none().rounded_full().bg(p.accent).flex().items_center().justify_center()
            .role(Role::Button).aria_label("Send")
            .text_color(p.on_accent).font_weight(FontWeight::BOLD)
            .on_click(cx.listener(|this, _, window, cx| this.send(window, cx)))
            .child("↑");
        let composer = div().w_full().flex_none().px(px(24.)).pt(px(4.)).pb(px(16.)).child(
            div().id("composer-box").w_full().flex().items_end().gap(px(8.)).pl(px(8.)).pr(px(8.)).py(px(8.)).rounded(px(16.))
                .bg(p.field).border_1().border_color(p.border_composer)
                .child(
                    div().flex_1().min_w_0().text_color(p.ink_field)
                        .child(Textarea::new(&self.composer).appearance(false).aria_label(SharedString::from(format!("Message {name}")))),
                )
                .child(send),
        );

        let panel = div().id("panel").size_full().flex().flex_col().rounded(px(18.)).bg(p.surface).border_1().border_color(p.panel_edge)
            .overflow_hidden()
            .child(header)
            .child(messages)
            .child(composer);
        div().flex_1().min_w_0().h_full().p(px(8.)).child(panel)
    }

    fn message(&self, ix: usize) -> AnyElement {
        let p = theme::palette(self.dark);
        let Some(m) = self.messages.get(ix) else { return div().into_any_element() };
        let author = self.authors[m.author].clone();
        let (ground, ink) = p.tones[m.author % p.tones.len()];
        let initial: SharedString = author.chars().next().unwrap_or('?').to_string().into();
        let hover = p.hover_message;
        let avatar = div().size(px(size::AVATAR)).flex_none().rounded_full().bg(ground).flex().items_center().justify_center()
            .text_color(ink).font_weight(FontWeight::BOLD)
            .child(initial);
        let head = div().flex().items_center().gap(px(8.))
            .child(div().font_weight(FontWeight::BOLD).text_color(p.ink).child(author))
            .child(div().text_size(px(12.5)).text_color(p.ink_muted).child(m.time.clone()));
        let body = TextView::markdown(ElementId::Name(m.id.clone()), m.body.clone()).selectable(true);
        div().px(px(12.)).child(
            div().id(ElementId::Name(format!("row-{}", m.id).into()))
                .w_full().px(px(12.)).py(px(8.)).flex().gap(px(14.)).rounded(px(12.))
                .role(Role::ListItem)
                .hover(move |style| style.bg(hover))
                .child(avatar)
                .child(
                    div().flex_1().min_w_0().flex().flex_col().gap(px(2.))
                        .text_size(px(15.)).line_height(px(24.)).text_color(p.ink_body)
                        .child(head)
                        .child(body),
                ),
        )
        .into_any_element()
    }

    /// The measurement hooks a window honours: a marker after the first frame, a steady
    /// scroll, and a count of frames after which it reports their spacing and quits.
    fn measure(&mut self, window: &mut Window) {
        if self.frames.at.is_empty() {
            window.on_next_frame(|_, cx| {
                println!("lumia-first-frame");
                if std::env::var_os("LUMIA_SPIKE_QUIT_AFTER_FIRST_FRAME").is_some() {
                    cx.quit();
                }
            });
        }
        // `LUMIA_SPIKE_TREE=path`: once an assistive client (VoiceOver) has connected, write the
        // accessibility tree GPUI hands it. There is none to write before that.
        if let Some(path) = std::env::var_os("LUMIA_SPIKE_TREE")
            && window.is_a11y_active()
            && let Some(json) = window.debug_a11y_tree_json()
        {
            let _ = std::fs::write(path, json);
        }
        if !self.frames.autoscroll && self.frames.limit.is_none() {
            if self.frames.at.is_empty() {
                self.frames.at.push(Instant::now());
            }
            return;
        }
        self.frames.at.push(Instant::now());
        if self.frames.autoscroll {
            // 26 pt a frame at 60 Hz is the three pages a second the Rune spike glides at. A
            // list resting at its end measures `scroll_by` from the end of its content, not
            // from the top of its viewport, so the first step also crosses the viewport.
            let at_end = self.list.logical_scroll_top().item_ix == self.list.item_count();
            let viewport = if at_end { self.list.viewport_bounds().size.height } else { px(0.) };
            self.list.scroll_by(-(viewport + px(26.)));
        }
        if self.frames.limit.is_some_and(|limit| self.frames.at.len() > limit) {
            println!("lumia-scrolled to message {} of {}", self.list.logical_scroll_top().item_ix, self.list.item_count());
            report(&self.frames.at);
            window.on_next_frame(|_, cx| cx.quit());
        } else {
            window.request_animation_frame();
        }
    }
}

/// The spacing of the frames' `render` calls: when GPUI began each frame, not when the
/// display showed it.
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

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.measure(window);
        let p = theme::palette(self.dark);
        div().id("shell").size_full().flex().bg(p.bg).font_family(theme::SANS).text_size(px(15.)).text_color(p.ink_body)
            .child(self.rail(p))
            .child(self.sidebar(p, cx))
            .child(self.conversation(p, cx))
    }
}
