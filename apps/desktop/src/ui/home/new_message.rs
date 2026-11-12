//! Starting a personal conversation: find someone this account shares a server with, and
//! open the conversation with them, or ask for one when the instance wants a request first.
//!
//! Not here: starting a conversation with several people, or with someone no shared server
//! lists (the instance has no search by handle).

use lumia_client::app::MemberSummary;
use rune::{El, col, row};

use super::{at, no_identity, page, person};
use crate::app::State;
use crate::session::display_name;
use crate::tokens::{self as t, icons};
use crate::ui::components::{Look, empty_state, field, notice, small_button};

/// Everyone the servers list but this account, narrowed by the search, by name.
fn people(s: &State) -> Vec<&MemberSummary> {
    let query = s.home.query.text.trim().to_lowercase();
    let mut found: Vec<&MemberSummary> = s
        .session
        .people
        .values()
        .filter(|p| p.account_id != s.session.me())
        .filter(|p| {
            query.is_empty()
                || [&p.handle, &p.display_name, &p.nickname]
                    .into_iter()
                    .any(|name| name.to_lowercase().contains(&query))
        })
        .collect();
    found.sort_by_cached_key(|p| (display_name(p).to_lowercase(), p.account_id.clone()));
    found
}

pub fn view(s: &State, look: Look) -> El {
    if !s.session.has_identity() {
        return no_identity(look);
    }
    let mut content = col().child(field(
        "find-person",
        "Find a person",
        &s.home.query,
        "Name or handle",
        |s| &mut s.home.query,
        |_, _| {},
        look,
    ));
    if !s.home.note.is_empty() {
        content = content.child(notice(&icons::MENTION, s.home.note.clone(), look).key("note"));
    }
    let found = people(s);
    if found.is_empty() {
        content = content.child(
            empty_state(
                &icons::MENTION,
                "No one to show",
                "Join a server to find people to message.",
                look,
            )
            .py(t::SPACE_40),
        );
    } else {
        // One conversation is opened or asked for at a time.
        let free = s.home.starting.is_none();
        let mut list = col().key("people").w_full().role("list").label("People");
        for p in found {
            let name = display_name(p);
            let id = p.account_id.clone();
            list = list.child(
                row()
                    .key(format!("person-{}", p.account_id))
                    .w_full()
                    .py(t::SPACE_6)
                    .gap(t::SPACE_12)
                    .items_center()
                    .role("listitem")
                    .label(name.clone())
                    .child(person(
                        &name,
                        &at(&p.handle),
                        &p.account_id,
                        t::AVATAR_MD,
                        None,
                        look,
                    ))
                    .child(small_button(
                        format!("message-{}", p.account_id),
                        "Message",
                        free,
                        move |s: &mut State, _| s.start_dm(&id),
                        look,
                    )),
            );
        }
        content = content.child(list);
    }
    page(
        "New message",
        "Someone you share a server with",
        content,
        look,
    )
}
