//! The requests to start a personal conversation that wait for an answer: who is asking,
//! what they share with this person, and Accept, Decline or Block.

use lumia_client::app::DmRequestSummary;
use rune::{El, col, row, text};

use super::{action_button, at, no_identity, page};
use crate::app::State;
use crate::session::dms::Answer;
use crate::tokens::{self as t, icons};
use crate::ui::components::{Look, avatar, empty_state, error_text, notice, secondary_button};

pub fn view(s: &State, look: Look) -> El {
    if !s.session.has_identity() {
        return no_identity(look);
    }
    let requests = s
        .session
        .snapshot
        .as_ref()
        .map_or(&[][..], |snap| snap.pending_requests.as_slice());
    let mut content = col().child(notice(
        &icons::SHIELD,
        "A request shows who is asking and what you share, nothing else. Accepting opens an \
         encrypted conversation; the sender cannot see that you looked.",
        look,
    ));
    if !s.home.request_error.is_empty() {
        content = content.child(error_text(&s.home.request_error, look));
    }
    if requests.is_empty() {
        content = content.child(
            empty_state(
                &icons::MENTION,
                "Nothing waiting",
                "Declined requests disappear; blocked people cannot ask again.",
                look,
            )
            .py(t::SPACE_40),
        );
    } else {
        let mut list = col()
            .key("requests")
            .w_full()
            .gap(t::SPACE_12)
            .role("list")
            .label("Requests");
        for request in requests {
            list = list.child(card(s, request, look));
        }
        content = content.child(list);
    }
    page("Requests", "People who want to message you", content, look)
}

/// What the asker shares with this person, as the instance counted it.
fn meta(request: &DmRequestSummary) -> String {
    let mut parts = Vec::new();
    match request.shared_server_ids.len() {
        0 => {}
        1 => parts.push("Shares a server with you".to_string()),
        n => parts.push(format!("Shares {n} servers with you")),
    }
    match request.mutual_friend_count {
        0 => {}
        1 => parts.push("1 mutual friend".to_string()),
        n => parts.push(format!("{n} mutual friends")),
    }
    if parts.is_empty() {
        "Nothing in common yet".into()
    } else {
        parts.join(" · ")
    }
}

fn card(s: &State, request: &DmRequestSummary, look: Look) -> El {
    let c = look.c;
    let name = s.session.name_of(&request.from_account_id);
    let handle = s
        .session
        .person(&request.from_account_id)
        .map(|p| at(&p.handle))
        .unwrap_or_default();
    // One answer at a time: the others wait for it.
    let free = s.home.answering.is_none();
    let answer = |key: &str, answer: Answer| {
        let id = request.request_id.clone();
        (
            format!("{key}-{}", request.request_id),
            move |s: &mut State| s.answer_request(&id, answer),
        )
    };
    let (accept_key, accept) = answer("accept", Answer::Accept);
    let (decline_key, decline) = answer("decline", Answer::Decline);
    let (block_key, block) = answer("block", Answer::Block);
    // The name, the handle, then what is shared.
    let mut words = col().grow(1.0).min_w(0.0).gap(t::SPACE_2).child(
        text(name.clone())
            .font(t::TEXT_AUTHOR)
            .color(c.ink)
            .truncate(),
    );
    if !handle.is_empty() {
        words = words.child(
            text(handle)
                .font(t::TEXT_META)
                .color(c.ink_muted)
                .truncate(),
        );
    }
    words = words.child(
        text(meta(request))
            .font(t::TEXT_META)
            .color(c.ink_secondary)
            .truncate(),
    );
    let who = row()
        .grow(1.0)
        .min_w(0.0)
        .gap(t::SPACE_14)
        .items_center()
        .child(avatar(&name, &request.from_account_id, t::AVATAR_LG, look))
        .child(words);
    row()
        .key(format!("request-{}", request.request_id))
        .w_full()
        .padding(t::SPACE_16)
        .gap(t::SPACE_8)
        .items_center()
        .radius(t::RADIUS_14)
        .fill(c.surface)
        .border(1.0, c.line)
        .role("listitem")
        .label(format!("Request from {name}"))
        .child(who)
        .child(action_button(accept_key, "Accept", free, accept, look))
        .child(secondary_button(
            decline_key,
            "Decline",
            free,
            decline,
            look,
        ))
        .child(secondary_button(block_key, "Block", free, block, look))
}
