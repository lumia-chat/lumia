//! Devices: everything that holds this identity. The card asking for the recovery phrase
//! while it waits to be saved, the roster with this computer marked, revoking another
//! device with the account password as reauth (identity spec 4.4), the account's passkeys
//! with their removal, and, for a computer not set up with the account's identity, what it
//! needs (`ui::setup`). The same states and wording as the web's `home.tsx`.
//!
//! A device is named by the label its person gave it when it was linked, and by its kind
//! when it has none.
//!
//! A new device of the account is approved by the prompt that comes over the window when it
//! asks (`ui::prompt`); "Approve a device" here is the other way, a copied request and a
//! typed code, for a new device that cannot reach the instance.
//!
//! "Add a passkey" opens the instance's web page signed in as this account, in the system
//! browser (architecture 6.3): a passkey is made there, by the browser, for the instance's
//! own domain.
//!
//! Not here: "revoked on 1 of 2 instances"; this shell knows one instance.

use chrono::{DateTime, Local};
use lumia_client::app::{App, DeviceSummary, LinkRequestSummary, PasskeySummary, RosterSummary};
use rune::{El, Icon, ScrollSpec, Tx, col, el, icon, row, text};

use crate::app::{HomeView, State};
use crate::tokens::{self as t, icons};
use crate::ui::components::{
    Look, card, empty_state, error_text, field, link_button, notice, panel, primary_button,
    section_label, small_button, wide_button,
};
use crate::ui::scroll::WithScrollBar;
use crate::ui::{conversation, setup};

pub fn view(s: &State, look: Look) -> El {
    let mut content = page_column().children(setup::phrase_card(s, look));
    // A roster held here while the account waits on the person (its identity is another,
    // or binding needs the password) is not the account's devices.
    let needed = s.session.setup_needed();
    let ready = s.session.private_ready();
    let roster = s.session.identity.roster.as_ref().filter(|_| !needed);
    if roster.is_some() {
        content = content.child(notice(
            &icons::SHIELD,
            "Your identity lives on these devices, not on any instance. Revoking one takes effect on every instance that holds your roster.",
            look,
        ));
    }
    content = content.child(passkeys(s, look));
    content = match roster {
        Some(roster) => devices(content, s, roster, look),
        // The roster is on its way from this computer's own store.
        None if ready => content.child(empty_state(&icons::SHIELD, "Looking…", "", look)),
        None => content.child(setup::needs_setup(s, look)),
    };
    page(
        "devices",
        "Devices",
        "Everything that holds your identity",
        content,
        look,
    )
}

/// A Home page: the panel, its header with what the page is about, and the page's cards in
/// a column that scrolls. Devices and Profile are both one.
pub(crate) fn page(key: &'static str, title: &str, topic: &str, content: El, look: Look) -> El {
    let header = conversation::header_with(title, None, Some(topic), look);
    panel("panel", look).w_full().child(header).child(
        col()
            .key(key)
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(content)
            .with_scroll_bar(key, look),
    )
}

/// The column a page's cards stand in: a conversation's side padding, 12 between cards.
pub(crate) fn page_column() -> El {
    col()
        .w_full()
        .px(t::SPACE_24)
        .pt(t::SPACE_16)
        .pb(t::SPACE_24)
        .gap(t::SPACE_12)
}

/// A card's primary action at the width of a button, not of the card.
pub(crate) fn action(button: El) -> El {
    row()
        .w_full()
        .child(el().w(t::AUTH_WIDTH / 2.0).child(button))
}

/// A notice with the way to Devices at its end, where this computer is set up.
pub(super) fn setup_notice(key: &'static str, words: &str, look: Look) -> El {
    notice(&icons::SHIELD, words, look)
        .items_center()
        .child(small_button(
            key,
            "Open Devices",
            true,
            |s, _| s.go_home(HomeView::Devices),
            look,
        ))
}

/// A small label beside a name: "This device", "Revoked".
fn tag(words: &str, look: Look) -> El {
    el().px(t::SPACE_6)
        .py(t::SPACE_2)
        .radius(t::RADIUS_6)
        .fill(look.c.chip)
        .child(text(words).font(t::TEXT_TAG).color(look.c.ink_secondary))
}

/// A day as the other shells write one: `Nov 3`.
fn day(micros: i64) -> String {
    DateTime::from_timestamp_micros(micros).map_or_else(String::new, |at| {
        at.with_timezone(&Local).format("%b %-d").to_string()
    })
}

// ---- Passkeys ---------------------------------------------------------------------------

fn passkey_name(passkey: &PasskeySummary) -> &str {
    if passkey.label.is_empty() {
        "Passkey"
    } else {
        passkey.label.as_str()
    }
}

fn passkey_line(passkey: &PasskeySummary) -> String {
    let name = passkey_name(passkey);
    let added = passkey
        .created_at_micros
        .map_or_else(|| "—".to_string(), day);
    let used = passkey
        .last_used_at_micros
        .map_or_else(|| "never".to_string(), day);
    format!("{name} · added {added} · last used {used}")
}

/// The account's sign-in keys on this instance (architecture 6.3), each with its removal.
fn passkeys(s: &State, look: Look) -> El {
    let c = look.c;
    let quiet = |words: &str| {
        text(words)
            .font(t::TEXT_META)
            .color(c.ink_quiet)
            .wrap()
            .w_full()
    };
    let mut card = card(
        "Passkeys",
        "Sign in to this instance with a key this computer or your phone keeps, instead of a password.",
        look,
    )
    .key("passkeys-card")
    .w_full();
    let held = &s.session.identity.passkeys;
    if held.is_empty() {
        card = card.child(quiet("No passkeys yet."));
    } else {
        let mut list = col()
            .key("passkeys")
            .w_full()
            .gap(t::SPACE_8)
            .role("list")
            .label("Passkeys");
        for passkey in held {
            let line = passkey_line(passkey);
            let id = passkey.passkey_id.clone();
            list = list.child(
                row()
                    .key(format!("passkey-{}", passkey.passkey_id))
                    .w_full()
                    .gap(t::SPACE_8)
                    .items_center()
                    .role("listitem")
                    .label(line.clone())
                    .child(
                        text(line)
                            .font(t::TEXT_META)
                            .color(c.ink_secondary)
                            .truncate()
                            .grow(1.0)
                            .min_w(0.0),
                    )
                    .child(
                        link_button(
                            format!("remove-passkey-{}", passkey.passkey_id),
                            "Remove",
                            move |s| s.remove_passkey(&id),
                            look,
                        )
                        // Which one, for someone who hears the buttons without the rows.
                        .label(format!("Remove {}", passkey_name(passkey))),
                    ),
            );
        }
        card = card.child(list);
    }
    let form = &s.account.devices;
    if !form.passkey_error.is_empty() {
        card = card.child(error_text(&form.passkey_error, look));
    }
    if !s.links.handoff_error.is_empty() {
        card = card.child(error_text(&s.links.handoff_error, look));
    }
    card.child(row().child(small_button(
        "add-passkey",
        "Add a passkey",
        !s.links.handing_off,
        |s, _| s.open_web_signed_in("devices"),
        look,
    )))
    .child(quiet(
        "It is made on the instance's page, which opens in your browser signed in as you.",
    ))
}

// ---- The roster -------------------------------------------------------------------------

/// A device's name: the label it was linked under, else what kind of device it is.
fn name_of(device: &DeviceSummary) -> String {
    if device.this_device {
        return "This computer".into();
    }
    if !device.label.trim().is_empty() {
        return device.label.trim().to_string();
    }
    match device.kind.as_str() {
        "web" => "Browser".into(),
        "native" => "Computer".into(),
        "mobile" => "Phone".into(),
        "" => "Device".into(),
        other => other.to_string(),
    }
}

fn glyph_of(device: &DeviceSummary) -> &'static Icon {
    match device.kind.as_str() {
        "web" => &icons::GLOBE,
        "mobile" => &icons::PHONE,
        "native" => &icons::MONITOR,
        _ => &icons::KEY,
    }
}

/// Where a web device's key is scoped, when it was added, and the start of its id.
fn meta_of(device: &DeviceSummary) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !device.web_origin.is_empty() {
        parts.push(device.web_origin.clone());
    }
    if device.added_at_micros > 0 {
        parts.push(format!("added {}", day(device.added_at_micros)));
    }
    parts.push(device.device_id.chars().take(8).collect());
    parts.join(" · ")
}

fn devices(mut content: El, s: &State, roster: &RosterSummary, look: Look) -> El {
    let form = &s.account.devices;
    let mut list = col()
        .key("devices-list")
        .w_full()
        .gap(t::SPACE_8)
        .role("list")
        .label("Devices");
    for device in roster.devices.iter().filter(|d| !d.revoked) {
        list = list.child(device_row(
            device,
            name_of(device),
            meta_of(device),
            true,
            look,
        ));
    }
    content = content.child(list);
    content = content.child(approve_card(s, look));

    let revoking = form
        .revoking
        .as_ref()
        .and_then(|id| roster.devices.iter().find(|d| d.device_id == *id));
    if let Some(device) = revoking {
        content = content.child(revoke_card(s, device, look));
    }

    let revoked: Vec<&DeviceSummary> = roster.devices.iter().filter(|d| d.revoked).collect();
    if !revoked.is_empty() {
        let mut list = col()
            .key("revoked-list")
            .w_full()
            .gap(t::SPACE_8)
            .role("list")
            .label("Revoked devices");
        for device in revoked {
            list = list.child(device_row(
                device,
                device.device_id.chars().take(8).collect(),
                String::new(),
                false,
                look,
            ));
        }
        content = content
            .child(section_label("Revoked", t::SPACE_8, look))
            .child(list);
    }
    content
}

fn device_row(
    device: &DeviceSummary,
    name: String,
    meta: String,
    can_revoke: bool,
    look: Look,
) -> El {
    let c = look.c;
    let mut title = row().gap(t::SPACE_6).items_center().child(
        text(name.clone())
            .font(t::TEXT_AUTHOR)
            .color(if device.revoked { c.ink_quiet } else { c.ink }),
    );
    let mut spoken = name.clone();
    if device.this_device {
        title = title.child(tag("This device", look));
        spoken.push_str(", this device");
    }
    if device.revoked {
        title = title.child(tag("Revoked", look));
        spoken.push_str(", revoked");
    }
    let mut words = col().grow(1.0).min_w(0.0).gap(t::SPACE_2).child(title);
    if !meta.is_empty() {
        words = words.child(text(meta).font(t::TEXT_META).color(c.ink_quiet).truncate());
    }
    // A bordered row with its glyph on a tile, as the web client's device list; this
    // computer's is marked in the accent.
    let (edge, ground, ink) = if device.this_device {
        (c.accent, c.accent_soft, c.accent_ink)
    } else {
        (c.line, c.inset, c.ink_secondary)
    };
    let mut item = row()
        .key(format!("device-{}", device.device_id))
        .w_full()
        .px(t::SPACE_14)
        .py(t::SPACE_12)
        .gap(t::SPACE_14)
        .items_center()
        .radius(t::RADIUS_14)
        .fill(c.surface)
        .border(1.0, edge)
        .role("listitem")
        .label(spoken)
        .child(
            el().size(t::GLYPH_TILE, t::GLYPH_TILE)
                .radius(t::RADIUS_12)
                .fill(ground)
                .center()
                .child(icon(glyph_of(device), t::SPACE_18).color(ink)),
        )
        .child(words);
    if can_revoke && !device.this_device && !device.revoked {
        let id = device.device_id.clone();
        item = item.child(
            small_button(
                format!("revoke-{}", device.device_id),
                "Revoke",
                true,
                // The confirmation opens under the list, which may be below the fold.
                move |s: &mut State, tx: &mut Tx| {
                    s.ask_revoke(&id);
                    tx.scroll_into_view("revoke-card");
                },
                look,
            )
            .label(format!("Revoke {name}")),
        );
    }
    item
}

// ---- Approving another device -------------------------------------------------------

/// What a request would add, in words: "a browser on my.lumia.chat", "a device named
/// “Pixel 9”".
fn adds(summary: &LinkRequestSummary) -> String {
    let label = summary.label.trim();
    match (summary.kind.as_str(), label.is_empty()) {
        ("web", _) => format!("a browser on {}", summary.web_origin),
        (_, false) => format!("a device named “{label}”"),
        (_, true) => "a new device".into(),
    }
}

/// A code as it is read out and typed: two groups of four.
fn spaced(code: &str) -> String {
    let chars: Vec<char> = code.chars().collect();
    chars
        .chunks(4)
        .map(|group| group.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

fn approve(s: &mut State, _: &mut Tx) {
    s.approve_device();
}

/// Approving another device (identity spec 4.2): the new device shows a request, this
/// computer reads what it would add, approves it, and gives the code to type there. Closed,
/// it is the button that opens it.
fn approve_card(s: &State, look: Look) -> El {
    let form = &s.account.devices.approve;
    if !form.open {
        return row().key("approve-open").child(small_button(
            "approve-device",
            "Approve a device",
            true,
            // The card opens under the list, which may be below the fold.
            |s: &mut State, tx: &mut Tx| {
                s.open_approve();
                tx.scroll_into_view("approve-card");
            },
            look,
        ));
    }
    let c = look.c;
    let mut card = card(
        "Approve a device",
        "On the new device, choose Link another way, copy the request it shows and paste it here. It gets a key of its own and reads your private messages from then on.",
        look,
    )
    .key("approve-card")
    .w_full();
    if !form.code.is_empty() {
        return card
            .child(
                text(spaced(&form.code))
                    .font(t::TEXT_LINK_CODE)
                    .color(c.ink)
                    .selectable(true)
                    .selection_color(c.accent_soft)
                    .label(format!("Code {}", form.code)),
            )
            .child(
                text(format!(
                    "Type this code on the new device, which claims it from {}. It works once, within ten minutes.",
                    s.session.address()
                ))
                .font(t::TEXT_META)
                .color(c.ink_secondary)
                .wrap()
                .w_full(),
            )
            .child(action(primary_button(
                "approve-done",
                "Done",
                true,
                State::close_approve,
                look,
            )));
    }
    card = card.child(field(
        "approve-request",
        "Request",
        &form.request,
        "Paste the request here",
        |s| &mut s.account.devices.approve.request,
        approve,
        look,
    ));
    let typed = form.request.text.trim();
    let summary = (!typed.is_empty()).then(|| App::identity_link_request_summary(typed));
    match &summary {
        Some(Ok(summary)) => {
            card = card.child(notice(
                &icons::SHIELD,
                format!(
                    "This adds {} to your identity. Approve it only if it is yours.",
                    adds(summary)
                ),
                look,
            ));
        }
        Some(Err(_)) => card = card.child(error_text("That isn't a link request.", look)),
        None => {}
    }
    if !form.error.is_empty() {
        card = card.child(error_text(&form.error, look));
    }
    let ready = matches!(summary, Some(Ok(_))) && !form.busy;
    card.child(
        col()
            .w_full()
            .gap(t::SPACE_8)
            .child(primary_button(
                "approve-confirm",
                if form.busy { "Approving…" } else { "Approve" },
                ready,
                State::approve_device,
                look,
            ))
            .child(wide_button(
                "approve-cancel",
                None,
                "Cancel",
                true,
                |s, _| s.close_approve(),
                look,
            )),
    )
}

fn revoke(s: &mut State, _: &mut Tx) {
    s.revoke();
}

/// The confirmation: the account password is the reauth a revocation needs, so a stolen
/// device cannot revoke the others.
fn revoke_card(s: &State, device: &DeviceSummary, look: Look) -> El {
    let form = &s.account.devices;
    let name = name_of(device);
    let mut card = card(
        &format!("Revoke {name}?"),
        "It stops reading new messages the moment every instance has the change. Confirm with your password.",
        look,
    )
    .key("revoke-card")
    .child(field(
        "revoke-password",
        "Password",
        &form.password,
        "Your password",
        |s| &mut s.account.devices.password,
        revoke,
        look,
    ));
    if !form.error.is_empty() {
        card = card.child(error_text(&form.error, look));
    }
    // The primary button over the other way out, as the sign-in card stacks them.
    card.child(
        col()
            .w_full()
            .gap(t::SPACE_8)
            .child(primary_button(
                "revoke-confirm",
                &format!("Revoke {name}"),
                !form.busy && !form.password.text.is_empty(),
                State::revoke,
                look,
            ))
            .child(wide_button(
                "revoke-keep",
                None,
                "Keep it",
                true,
                |s, _| s.keep_device(),
                look,
            )),
    )
}
