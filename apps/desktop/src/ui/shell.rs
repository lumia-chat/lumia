//! The app shell on the Quiet Split, as the design canvas draws it and `ShellView.swift`
//! builds it: the server rail, the sidebar on the ground with a server's channels, the
//! conversation as a floating panel, and the server's members as a second panel.
//!
//! Not here yet: Home (personal conversations, requests, devices, the profile), adding a
//! server, new channels and invites.

use std::borrow::Cow;

use chrono::{DateTime, Local, NaiveDate};
use lumia_client::app::{MemberSummary, MessageSummary, ServerSummary};
use rune::{Cursor, El, GrowthSpec, ScrollSpec, Tx, col, el, icon, row, text, text_input};
use rune_kit::Markdown;

use super::components::{Look, avatar, empty_state, error_text, initials, link_button, panel};
use crate::app::State;
use crate::session::{Connection, Pending, display_name};
use crate::tokens::{self as t, icons};

/// The rail's top inset, clear of the window's traffic lights under a full-size content view.
const TITLEBAR: f32 = t::SPACE_40 + t::SPACE_4;
const SIDEBAR_WIDTH: f32 = 240.0;
const ASIDE_WIDTH: f32 = 280.0;
/// A message row's height before it is measured: an avatar and its padding.
const ROW_ESTIMATE: f32 = t::AVATAR_LG + t::SPACE_24;

pub fn view(s: &State, look: Look) -> El {
    let server = s
        .server
        .as_deref()
        .and_then(|id| s.session.servers().iter().find(|x| x.server_id == id));
    let mut shell = row()
        .key("shell")
        .w_full()
        .h_full()
        .fill(look.c.bg)
        .child(rail(s, server, look))
        .child(sidebar(s, server, look));
    shell = shell.child(
        col()
            .grow(1.0)
            .min_w(0.0)
            .h_full()
            .py(t::SPACE_8)
            .child(conversation(s, server, look)),
    );
    match server {
        Some(server) => shell.child(
            col()
                .h_full()
                .padding(t::SPACE_8)
                .child(members(s, server, look)),
        ),
        None => shell.child(el().w(t::SPACE_8)),
    }
}

// ---- Rail -------------------------------------------------------------------------------

fn rail(s: &State, current: Option<&ServerSummary>, look: Look) -> El {
    let c = look.c;
    let mut rail = col()
        .key("rail")
        .w(t::RAIL_WIDTH)
        .h_full()
        .pt(TITLEBAR)
        .pb(t::SPACE_12)
        .gap(t::SPACE_12)
        .items_center()
        .fill(c.rail)
        .window_drag();
    for server in s.session.servers() {
        let active = current.is_some_and(|x| x.server_id == server.server_id);
        let id = server.server_id.clone();
        let mut tile = el()
            .key(format!("server-{}", server.server_id))
            .size(t::TILE_SIZE, t::TILE_SIZE)
            .radius(t::TILE_SIZE / 2.0)
            .fill(if active { c.raised } else { c.tile })
            .center()
            .role("button")
            .label(server.name.clone())
            .help(server.name.clone())
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .no_window_drag()
            .on_press(move |s: &mut State| s.select_server(&id))
            .child(
                text(initials(&server.name))
                    .font(if active {
                        t::TITLE_MONOGRAM
                    } else {
                        t::TEXT_AUTHOR
                    })
                    .color(if active { c.ink_selected } else { c.ink }),
            );
        if active {
            for ring in look.shadows.ring_current {
                tile = tile.box_shadow(*ring);
            }
        }
        rail = rail.child(tile);
    }
    let connected = s.session.connection == Connection::Connected;
    let me = el()
        .key("self")
        .role("image")
        .label(format!(
            "{}, {}",
            s.session.self_name(),
            s.session.connection.label()
        ))
        .help(format!(
            "{} · {}",
            s.session.self_name(),
            s.session.connection.label()
        ))
        .child(avatar(
            s.session.self_name(),
            s.session.me(),
            t::AVATAR_SELF,
            look,
        ))
        .child(
            el().absolute(t::AVATAR_SELF - t::SPACE_10, t::AVATAR_SELF - t::SPACE_10)
                .size(t::SPACE_12, t::SPACE_12)
                .radius(t::SPACE_6)
                .fill(if connected { c.online } else { c.ink_muted })
                .border(2.0, c.rail),
        );
    rail.child(el().grow(1.0)).child(me)
}

// ---- Sidebar ----------------------------------------------------------------------------

fn sidebar(s: &State, server: Option<&ServerSummary>, look: Look) -> El {
    let c = look.c;
    let title = row()
        .w_full()
        .h(t::HEADER_HEIGHT - t::SPACE_24)
        .pl(t::SPACE_8)
        .mb(t::SPACE_10)
        .items_center()
        .window_drag()
        .child(
            text(server.map_or("Lumia", |x| x.name.as_str()))
                .font(t::TITLE_SERVER)
                .color(c.ink)
                .truncate(),
        );
    let mut list = col()
        .key("channels")
        .w_full()
        .gap(t::SPACE_2)
        .role("list")
        .label("Channels");
    let mut any = false;
    for channel in server
        .into_iter()
        .flat_map(|x| s.session.channels_of(&x.server_id))
    {
        any = true;
        let active = s.session.channel.as_deref() == Some(channel.channel_id.as_str());
        let id = channel.channel_id.clone();
        let mut item = row()
            .key(format!("channel-{}", channel.channel_id))
            .w_full()
            .h(t::ROW_HEIGHT)
            .px(t::SPACE_10)
            .gap(t::SPACE_8)
            .items_center()
            .radius(t::RADIUS_8)
            .role("button")
            .label(format!("{} channel", channel.name))
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 0.0, c.focus)
            .on_press(move |s: &mut State| s.select_channel(&id))
            .child(icon(&icons::HASH, t::SPACE_14).color(if active {
                c.accent
            } else {
                c.ink_faint
            }))
            .child(
                text(channel.name.clone())
                    .font(if active { t::TEXT_MEMBER } else { t::TEXT_NAV })
                    .color(if active { c.ink_selected } else { c.ink_quiet })
                    .truncate(),
            );
        if active {
            item = item.fill(c.raised);
            for layer in look.shadows.selected {
                item = item.box_shadow(*layer);
            }
        } else {
            item = item.hover(c.hover_row);
        }
        list = list.child(item);
    }
    let section = text("Channels")
        .font(t::TEXT_SECTION)
        .color(c.ink_muted)
        .px(t::SPACE_10)
        .pt(t::SPACE_4)
        .pb(t::SPACE_6);
    let mut body = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(col().w_full().child(section).child(list));
    if server.is_some() && !any {
        body = body.child(
            text("No channels yet.")
                .font(t::TEXT_META)
                .color(c.ink_quiet)
                .padding(t::SPACE_10),
        );
    }
    let account = row()
        .key("account")
        .w_full()
        .px(t::SPACE_10)
        .py(t::SPACE_8)
        .gap(t::SPACE_8)
        .items_center()
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(
                    text(s.session.self_name().to_string())
                        .font(t::TEXT_MEMBER)
                        .color(c.ink)
                        .truncate(),
                )
                .child(
                    text(s.session.address().to_string())
                        .font(t::TEXT_META)
                        .color(c.ink_muted)
                        .truncate(),
                ),
        )
        .child(link_button("sign-out", "Sign out", State::sign_out, look));
    col()
        .key("sidebar")
        .w(SIDEBAR_WIDTH)
        .h_full()
        .pt(t::SPACE_14 + t::SPACE_14)
        .pl(t::SPACE_12)
        .pr(t::SPACE_10)
        .child(title)
        .child(body)
        .child(account)
}

// ---- Conversation ---------------------------------------------------------------------------

fn conversation(s: &State, server: Option<&ServerSummary>, look: Look) -> El {
    let c = look.c;
    let pane = panel("panel", look).w_full();
    let channel = s
        .session
        .channel
        .as_deref()
        .and_then(|id| s.session.channel_summary(id));
    let Some(channel) = channel else {
        let (title, body) = match server {
            Some(_) => ("No channel yet", "This server has no channel you can see."),
            None if s.session.snapshot.is_none() => {
                ("Loading", "Fetching your servers from the instance.")
            }
            None => (
                "No servers yet",
                "Join or create a server from the web app for now; this app lists it once you have.",
            ),
        };
        return pane
            .child(header(
                server.map_or("Lumia", |x| x.name.as_str()),
                false,
                look,
            ))
            .child(empty_state(&icons::HASH, title, body, look));
    };
    let name = channel.name.as_str();

    let stored = s.session.messages.len();
    let count = stored + s.session.pending_here().count();
    let dark = look.dark;
    let timeline = if count == 0 {
        empty_state(&icons::HASH, "Nothing here yet", "Say something.", look)
    } else {
        let rows = col()
            .key("rows")
            .w_full()
            .py(t::SPACE_14)
            .px(t::SPACE_14)
            .growth(GrowthSpec::default())
            .role("list")
            .label(format!("Messages in {name}"))
            .virtual_rows(
                count,
                ROW_ESTIMATE,
                |s: &State, i| match s.session.messages.get(i) {
                    Some(message) => Cow::Borrowed(message.message_id.as_str()),
                    None => s
                        .session
                        .pending_here()
                        .nth(i - s.session.messages.len())
                        .map_or(Cow::Borrowed(""), |p| Cow::Borrowed(p.id.as_str())),
                },
                move |s: &State, i| {
                    let look = Look::of(dark);
                    match s.session.messages.get(i) {
                        Some(message) => stored_row(s, i, message, look),
                        None => match s.session.pending_here().nth(i - s.session.messages.len()) {
                            Some(pending) => pending_row(s, pending, look),
                            None => el(),
                        },
                    }
                },
            )
            .virtual_rows_version(s.session.revision);
        col()
            .key("messages")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: true })
            .child(rows)
    };

    let mut pane = pane.child(header(name, true, look)).child(timeline);
    if let Some(error) = &s.session.last_error {
        pane = pane.child(el().w_full().px(t::SPACE_24).child(error_text(error, look)));
    }
    pane.child(composer(s, name, look, c.accent))
}

fn header(title: &str, channel: bool, look: Look) -> El {
    let c = look.c;
    let mut bar = row()
        .key("header")
        .w_full()
        .h(t::HEADER_HEIGHT)
        .pl(t::SPACE_24)
        .pr(t::SPACE_14)
        .gap(t::SPACE_8)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.line)
        .window_drag();
    if channel {
        bar = bar.child(icon(&icons::HASH, t::SPACE_18).color(c.accent));
    }
    bar.child(
        text(title)
            .font(t::TITLE_CHANNEL)
            .color(c.ink)
            .truncate()
            .role("heading"),
    )
}

fn local(micros: i64) -> DateTime<Local> {
    DateTime::from_timestamp_micros(micros)
        .unwrap_or_default()
        .with_timezone(&Local)
}

fn clock(micros: i64) -> String {
    local(micros).format("%-I:%M %p").to_string()
}

/// How a day reads above its first message.
fn day_label(day: NaiveDate) -> String {
    let today = Local::now().date_naive();
    if day == today {
        "Today".into()
    } else if Some(day) == today.pred_opt() {
        "Yesterday".into()
    } else {
        day.format("%b %-d").to_string()
    }
}

fn day_divider(label: String, look: Look) -> El {
    let c = look.c;
    let rule = || el().grow(1.0).h(1.0).fill(c.line);
    row()
        .w_full()
        .py(t::SPACE_8)
        .gap(t::SPACE_12)
        .items_center()
        .child(rule())
        .child(text(label).font(t::TEXT_SECTION).color(c.ink_quiet))
        .child(rule())
}

/// An avatar beside a name, a time, and whatever the message is.
fn message_frame(
    key: &str,
    author: &str,
    author_id: &str,
    time: String,
    content: El,
    look: Look,
) -> El {
    let c = look.c;
    let head = row()
        .gap(t::SPACE_8)
        .items_center()
        .child(text(author).font(t::TEXT_AUTHOR).color(c.ink_emphasis))
        .child(text(time).font(t::TEXT_META).color(c.ink_muted));
    row()
        .key(key)
        .w_full()
        .px(t::SPACE_12)
        .py(t::SPACE_8)
        .gap(t::SPACE_14)
        .radius(t::RADIUS_12)
        .hover(c.hover_message)
        .role("listitem")
        .child(avatar(author, author_id, t::AVATAR_LG, look))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(t::SPACE_2)
                .child(head)
                .child(content),
        )
}

/// Opens a link from a message in the browser. Only web links: a message is text someone
/// else wrote, and other schemes can start other programs.
fn open_link(_: &mut State, url: &str, tx: &mut Tx) {
    if url.starts_with("https://") || url.starts_with("http://") {
        let _ = tx.open_url(url);
    }
}

fn stored_row(s: &State, i: usize, message: &MessageSummary, look: Look) -> El {
    let c = look.c;
    let author = s.session.name_of(&message.author_account_id);
    let content = if message.deleted {
        text("(deleted)")
            .font(t::TEXT_MESSAGE)
            .color(c.ink_quiet)
            .key("body")
    } else {
        Markdown::new(message.body.as_str())
            .selectable(true)
            .on_link_tx(open_link)
            .render(&s.markdown[usize::from(look.dark)])
            .key("body")
    };
    let row = message_frame(
        &message.message_id,
        &author,
        &message.author_account_id,
        clock(message.created_at_micros),
        content,
        look,
    );
    // The divider lives in the first row of its day: a row of its own would vanish when the
    // list unmounts it.
    let day = local(message.created_at_micros).date_naive();
    let previous = i
        .checked_sub(1)
        .and_then(|p| s.session.messages.get(p))
        .map(|m| local(m.created_at_micros).date_naive());
    if previous == Some(day) {
        return row;
    }
    col()
        .key(format!("day-{}", message.message_id))
        .w_full()
        .child(day_divider(day_label(day), look))
        .child(row)
}

/// A send the store does not hold yet: dimmed until it does, with the refusal when there was one.
fn pending_row(s: &State, pending: &Pending, look: Look) -> El {
    let c = look.c;
    let body = Markdown::new(pending.body.as_str())
        .render(&s.markdown[usize::from(look.dark)])
        .key("body");
    let content = match &pending.failed {
        None => col().w_full().opacity(0.5).child(body),
        Some(why) => {
            let id = pending.id.clone();
            let dismiss = el()
                .key("dismiss")
                .role("button")
                .label("Dismiss")
                .cursor(Cursor::Pointer)
                .focusable(true)
                .focus_ring(2.0, 2.0, c.focus)
                .on_press(move |s: &mut State| s.session.dismiss(&id))
                .child(text("Dismiss").font(t::TEXT_LINK_SM).color(c.link));
            col()
                .w_full()
                .gap(t::SPACE_4)
                .child(el().w_full().opacity(0.5).child(body))
                .child(
                    row()
                        .gap(t::SPACE_8)
                        .items_center()
                        .child(
                            text(format!("Not sent: {why}"))
                                .font(t::TEXT_META)
                                .color(c.dnd),
                        )
                        .child(dismiss),
                )
        }
    };
    let time = if pending.failed.is_some() {
        clock(pending.sent_at_micros)
    } else {
        "Sending…".into()
    };
    message_frame(
        &pending.id,
        s.session.self_name(),
        s.session.me(),
        time,
        content,
        look,
    )
}

fn composer(s: &State, channel: &str, look: Look, accent: rune::Color) -> El {
    let c = look.c;
    let can_send = !s.draft.text.trim().is_empty();
    let input = text_input(&s.draft, t::TEXT_MESSAGE)
        .key("composer")
        .label(format!("Message {channel}"))
        .placeholder(format!("Message #{channel}"), c.ink_muted)
        .color(c.ink_field)
        .caret(accent, c.accent_soft)
        .grow(1.0)
        .min_h(t::SPACE_24)
        .max_h(t::SPACE_24 * 8.0)
        .on_edit(|s: &mut State| &mut s.draft)
        .on_submit(|s: &mut State, tx| s.send(tx));
    let mut send = el()
        .key("send")
        .size(t::SPACE_40, t::SPACE_40)
        .radius(t::SPACE_40 / 2.0)
        .fill(accent)
        .center()
        .role("button")
        .label("Send")
        .disabled(!can_send)
        .child(icon(&icons::SEND, t::SPACE_18).color(c.on_accent));
    send = if can_send {
        send.cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .on_press_tx(|s: &mut State, tx| s.send(tx))
    } else {
        send.opacity(0.5)
    };
    let mut frame = row()
        .key("composer-box")
        .w_full()
        .items_end()
        .gap(t::SPACE_8)
        .pl(t::SPACE_16)
        .pr(t::SPACE_8)
        .py(t::SPACE_8)
        .radius(t::RADIUS_16)
        .fill(c.field)
        .border(1.0, c.border_composer)
        .focus_within_ring(1.0, 0.0, c.border_active)
        .child(col().grow(1.0).min_w(0.0).py(t::SPACE_8).child(input))
        .child(send);
    for layer in look.shadows.composer {
        frame = frame.box_shadow(*layer);
    }
    col()
        .w_full()
        .px(t::SPACE_24)
        .pt(t::SPACE_4)
        .pb(t::SPACE_16)
        .child(frame)
}

// ---- Members ----------------------------------------------------------------------------

fn members(s: &State, server: &ServerSummary, look: Look) -> El {
    let c = look.c;
    let mut all: Vec<&MemberSummary> = s
        .session
        .members
        .get(&server.server_id)
        .map(|m| m.iter().collect())
        .unwrap_or_default();
    all.sort_by_cached_key(|m| display_name(m).to_lowercase());
    let (here, away): (Vec<_>, Vec<_>) = all.iter().copied().partition(|m| {
        s.session
            .presence_of(&server.server_id, &m.account_id)
            .is_some()
    });

    let heading = row()
        .w_full()
        .h(t::HEADER_HEIGHT)
        .px(t::SPACE_18)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.line)
        .child(
            text(format!("Members · {}", all.len()))
                .font(t::TEXT_MEMBER)
                .color(c.ink)
                .role("heading"),
        );
    let section = |label: String| {
        text(label)
            .font(t::TEXT_SECTION)
            .color(c.ink_muted)
            .px(t::SPACE_10)
            .pt(t::SPACE_8)
            .pb(t::SPACE_6)
    };
    let mut list = col()
        .key("members")
        .w_full()
        .px(t::SPACE_8)
        .pb(t::SPACE_14)
        .role("list")
        .label("Members");
    if !here.is_empty() {
        list = list.child(section(format!("Online · {}", here.len())));
        for member in &here {
            list = list.child(member_row(s, server, member, look));
        }
    }
    if !away.is_empty() {
        list = list.child(section(format!(
            "{} · {}",
            if here.is_empty() {
                "Members"
            } else {
                "Offline"
            },
            away.len()
        )));
        for member in &away {
            list = list.child(member_row(s, server, member, look));
        }
    }
    panel("aside", look).w(ASIDE_WIDTH).child(heading).child(
        col()
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(list),
    )
}

fn member_row(s: &State, server: &ServerSummary, member: &MemberSummary, look: Look) -> El {
    let c = look.c;
    let name = display_name(member);
    let presence = s.session.presence_of(&server.server_id, &member.account_id);
    // The handle and, from the verified profile, the pronouns; then the custom status.
    let handle = if member.handle.is_empty() {
        String::new()
    } else {
        format!("@{}", member.handle)
    };
    let custom = presence.map_or("", |(_, custom)| custom.as_str());
    let subtitle = [handle.as_str(), member.pronouns.as_str(), custom]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let dot = presence.map(|(status, _)| match status.as_str() {
        "AWAY" => c.away,
        "DO_NOT_DISTURB" => c.dnd,
        _ => c.online,
    });
    let mut picture = el().child(avatar(&name, &member.account_id, t::AVATAR_MD, look));
    if let Some(dot) = dot {
        picture = picture.child(
            el().absolute(t::AVATAR_MD - t::SPACE_8, t::AVATAR_MD - t::SPACE_8)
                .size(t::SPACE_10, t::SPACE_10)
                .radius(t::SPACE_10 / 2.0)
                .fill(dot)
                .border(2.0, c.surface),
        );
    }
    let mut words = col().grow(1.0).min_w(0.0).child(
        text(name.clone())
            .font(t::TEXT_MEMBER)
            .color(c.ink)
            .truncate(),
    );
    if !subtitle.is_empty() {
        words = words.child(
            text(subtitle.clone())
                .font(t::TEXT_META)
                .color(c.ink_muted)
                .truncate(),
        );
    }
    let status = match presence.map(|(status, _)| status.as_str()) {
        Some("AWAY") => "away",
        Some("DO_NOT_DISTURB") => "do not disturb",
        Some(_) => "online",
        None => "offline",
    };
    let mut item = row()
        .key(format!("member-{}", member.account_id))
        .w_full()
        .px(t::SPACE_10)
        .py(t::SPACE_6)
        .gap(t::SPACE_10)
        .items_center()
        .radius(t::RADIUS_10)
        .hover(c.hover_member)
        .role("listitem")
        .label(format!("{name}, {status}"))
        .child(picture)
        .child(words);
    if member.account_id == s.session.me() {
        item = item.child(
            el().px(t::SPACE_6)
                .py(t::SPACE_2)
                .radius(t::RADIUS_5)
                .fill(c.accent_soft)
                .child(text("You").font(t::TEXT_TAG).color(c.accent_ink)),
        );
    }
    item
}
