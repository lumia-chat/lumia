//! The rail as an account arranges it (architecture 6.9), through a windowless `Runtime`:
//! folders opened and closed, the menus' moves, Folder settings, a tile dragged between
//! tiles, onto one and back with Escape, and what the instance answers. The arrangement
//! sent is the session's pending rail; no instance is behind it.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::*;
use lumia_client::rail::RailItem;
use lumia_shell::app::{Place, Popover, State};
use lumia_shell::scenes::SCENES;
use lumia_shell::session::rail::RailUpdate;
use lumia_shell::session::{Failure, Update};
use rune::{Key, KeyEvent, Mods, Runtime};

/// Acme alone, a closed folder Work (Northwind, Guild, Systems, Beta) and an open one with
/// no name (Garden, Trail): the `rail-folders` scene.
fn arranged() -> Runtime<State> {
    let mut state = State::new(data_dir());
    let (_, scene) = SCENES
        .iter()
        .find(|(name, _)| *name == "rail-folders")
        .unwrap();
    scene(&mut state);
    runtime(state)
}

/// The rail by ids, a folder as its id and its servers.
fn shape(rt: &Runtime<State>) -> Vec<String> {
    rt.state
        .session
        .rail()
        .iter()
        .map(|item| match item {
            RailItem::Server { server_id } => server_id.clone(),
            RailItem::Folder {
                folder_id,
                server_ids,
                ..
            } => format!("{folder_id}[{}]", server_ids.join(",")),
        })
        .collect()
}

fn right_click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is not on screen"));
    rt.pointer_move(x, y);
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    settle(rt);
}

/// A point in the tile keyed `key`, `f` of the way down it.
fn at(rt: &Runtime<State>, key: &str, f: f32) -> (f32, f32) {
    let ([x, y, w, h], _) = rt
        .presented(key)
        .unwrap_or_else(|| panic!("{key} is not drawn"));
    (x + w / 2.0, y + h * f)
}

/// Presses the tile `from` and moves the pointer to `to` in steps, without letting go.
fn drag_to(rt: &mut Runtime<State>, from: &str, to: (f32, f32)) {
    let (x, y) = at(rt, from, 0.5);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    settle(rt);
    for step in 1..=8 {
        let f = step as f32 / 8.0;
        rt.pointer_move(x + (to.0 - x) * f, y + (to.1 - y) * f);
        rt.frame(1.0 / 60.0);
    }
    settle(rt);
}

fn release(rt: &mut Runtime<State>, to: (f32, f32)) {
    rt.pointer_up(to.0, to.1);
    settle(rt);
}

#[test]
fn the_rail_is_drawn_as_arranged_and_a_folder_opens_and_closes_on_this_computer() {
    let mut rt = arranged();
    assert_eq!(
        shape(&rt),
        [
            "acme",
            "work[northwind,guild,systems,beta]",
            "outdoors[garden,trail]"
        ]
    );
    // Work is closed: its tile, none of its servers. The other is open: its servers.
    let shown = words(&rt);
    assert!(shown.contains("Folder, Work"), "{shown}");
    assert!(rt.find("server-northwind").is_none());
    assert!(rt.find("server-garden").is_some() && rt.find("server-trail").is_some());
    assert!(
        shown.contains("Folder, Garden and Trail Runners"),
        "a folder with no name reads as its servers: {shown}"
    );

    press(&mut rt, "folder-work");
    assert!(rt.find("server-northwind").is_some());
    assert!(rt.state.settings.open_folders.contains(&"work".to_string()));
    press(&mut rt, "folder-work");
    assert!(rt.find("server-northwind").is_none());
    // Opening a folder sends nothing: it is this computer's.
    assert!(rt.state.session.rail.pending.is_none());

    // The rail's tiles are a list of list items, as the board's.
    let list = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("rail-list"))
        .unwrap();
    assert_eq!(list.role, "list");
}

#[test]
fn a_servers_menu_moves_it_and_makes_and_unmakes_folders() {
    let mut rt = arranged();
    right_click(&mut rt, "server-acme");
    assert_eq!(rt.state.popover, Some(Popover::RailMenu));
    // At the top, there is no Move up; outside a folder, no Remove from folder.
    assert!(rt.find("rail-move-up").is_none() && rt.find("rail-out-of-folder").is_none());
    press(&mut rt, "rail-move-down");
    assert_eq!(
        shape(&rt),
        [
            "work[northwind,guild,systems,beta]",
            "acme",
            "outdoors[garden,trail]"
        ]
    );
    assert_eq!(rt.state.popover, None);

    right_click(&mut rt, "server-acme");
    press(&mut rt, "rail-move-to-folder");
    assert_eq!(rt.state.popover, Some(Popover::MoveToFolder));
    press(&mut rt, "move-to-outdoors");
    assert_eq!(
        shape(&rt),
        [
            "work[northwind,guild,systems,beta]",
            "outdoors[garden,trail,acme]"
        ]
    );

    right_click(&mut rt, "server-acme");
    press(&mut rt, "rail-out-of-folder");
    assert_eq!(
        shape(&rt),
        [
            "work[northwind,guild,systems,beta]",
            "outdoors[garden,trail]",
            "acme"
        ]
    );

    right_click(&mut rt, "server-acme");
    press(&mut rt, "rail-new-folder");
    let rail = rt.state.session.rail();
    let Some(RailItem::Folder {
        name,
        tone,
        server_ids,
        ..
    }) = rail.last()
    else {
        panic!("Acme is in a folder of its own: {rail:?}");
    };
    assert_eq!(
        (name.as_str(), server_ids.as_slice()),
        ("", &["acme".to_string()][..])
    );
    assert_eq!(*tone, lumia_client::rail::tone_of("acme"));
    // Folder settings open on it, to name it at once.
    assert_eq!(rt.state.popover, Some(Popover::FolderSettings));
    assert!(rt.find("folder-settings").is_some());
}

#[test]
fn a_folders_menu_names_it_colours_it_and_deletes_it_leaving_its_servers() {
    let mut rt = arranged();
    right_click(&mut rt, "folder-work");
    // A folder's menu has no folder moves of a server's.
    assert!(rt.find("rail-new-folder").is_none());
    press(&mut rt, "rail-folder-settings");
    assert_eq!(rt.state.popover, Some(Popover::FolderSettings));
    press(&mut rt, "folder-name");
    rt.update(|s, _| s.rail.name.replace_text(""));
    type_text(&mut rt, "  Clients  ");
    press(&mut rt, "folder-tone-sky");
    press(&mut rt, "folder-save");
    let rail = rt.state.session.rail();
    assert!(
        rail.iter().any(|item| matches!(
            item,
            RailItem::Folder { folder_id, name, tone, .. }
                if folder_id == "work" && name == "Clients" && tone == "sky"
        )),
        "{rail:?}"
    );
    assert_eq!(rt.state.popover, None);

    right_click(&mut rt, "folder-work");
    press(&mut rt, "rail-delete-folder");
    assert_eq!(
        shape(&rt),
        [
            "acme",
            "northwind",
            "guild",
            "systems",
            "beta",
            "outdoors[garden,trail]"
        ]
    );
}

#[test]
fn a_server_dragged_between_tiles_moves_there_and_onto_one_makes_a_folder() {
    let mut rt = arranged();
    // Acme to the bottom quarter of Trail Runners, the open folder's last: the folder's
    // end. The line shows where; the tile is not opened by the press that ends the drag.
    let to = at(&rt, "server-trail", 0.9);
    drag_to(&mut rt, "server-acme", to);
    assert!(rt.find("rail-line").is_some() && rt.find("rail-ghost").is_some());
    release(&mut rt, to);
    assert_eq!(
        shape(&rt),
        [
            "work[northwind,guild,systems,beta]",
            "outdoors[garden,trail,acme]"
        ]
    );
    assert!(rt.find("rail-ghost").is_none());

    // Garden out of the folder, to the top quarter of Work: before it on the rail.
    let to = at(&rt, "folder-work", 0.1);
    drag_to(&mut rt, "server-garden", to);
    release(&mut rt, to);
    assert_eq!(
        shape(&rt),
        [
            "garden",
            "work[northwind,guild,systems,beta]",
            "outdoors[trail,acme]"
        ]
    );

    // Garden onto the middle of Work: into it, at its end.
    let to = at(&rt, "folder-work", 0.5);
    drag_to(&mut rt, "server-garden", to);
    assert!(rt.find("rail-line").is_none(), "onto a tile, no line");
    release(&mut rt, to);
    assert_eq!(
        shape(&rt),
        [
            "work[northwind,guild,systems,beta,garden]",
            "outdoors[trail,acme]"
        ]
    );
    assert_eq!(rt.state.place, Place::Server("acme".into()));
}

#[test]
fn a_server_dropped_onto_a_server_makes_a_folder_of_the_two() {
    let mut rt = arranged();
    // Close the open folder, then drag Acme onto the middle of the new last tile: first
    // put Garden on the rail by its menu.
    right_click(&mut rt, "server-garden");
    press(&mut rt, "rail-out-of-folder");
    let to = at(&rt, "server-garden", 0.5);
    drag_to(&mut rt, "server-acme", to);
    release(&mut rt, to);
    let rail = rt.state.session.rail();
    let Some(RailItem::Folder {
        name,
        tone,
        server_ids,
        ..
    }) = rail.iter().find(|item| {
        matches!(item, RailItem::Folder { server_ids, .. } if server_ids.contains(&"acme".to_string()))
    })
    else {
        panic!("Acme and Garden are a folder: {rail:?}");
    };
    // No name, the server dropped on's tone, it first; Folder settings do not open.
    assert_eq!(server_ids, &["garden".to_string(), "acme".to_string()]);
    assert!(name.is_empty());
    assert_eq!(*tone, lumia_client::rail::tone_of("garden"));
    assert_eq!(rt.state.popover, None);
}

#[test]
fn a_tile_let_go_off_the_rail_stays_where_it_was() {
    let mut rt = arranged();
    let before = shape(&rt);
    // At the height of a place it could go, but over the conversation.
    let (_, y) = at(&rt, "server-trail", 0.9);
    let to = (400.0, y);
    drag_to(&mut rt, "server-acme", to);
    assert!(rt.find("rail-line").is_none() && rt.find("rail-ghost").is_some());
    release(&mut rt, to);
    assert_eq!(shape(&rt), before);
    assert!(rt.state.session.rail.pending.is_none());
}

#[test]
fn escape_puts_a_dragged_tile_back() {
    let mut rt = arranged();
    let before = shape(&rt);
    let to = at(&rt, "server-trail", 0.9);
    drag_to(&mut rt, "server-acme", to);
    assert!(rt.state.rail.drag.is_some());
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    settle(&mut rt);
    release(&mut rt, to);
    assert_eq!(shape(&rt), before);
    assert!(rt.state.session.rail.pending.is_none());
}

#[test]
fn the_rail_waits_for_the_last_answer_and_goes_back_when_it_is_refused() {
    let mut rt = arranged();
    let kept = shape(&rt);
    right_click(&mut rt, "server-acme");
    press(&mut rt, "rail-move-down");
    let first = rt.state.session.rail.sent();
    right_click(&mut rt, "server-acme");
    press(&mut rt, "rail-move-down");
    let moved = shape(&rt);
    // The first answer is not the rail any more: the second is on its way.
    let snapshot = rt.state.session.snapshot.clone().unwrap();
    rt.update(move |s, _| {
        s.receive(&Update::Rail(RailUpdate::Arranged {
            sent: first,
            result: Ok(snapshot),
        }));
    });
    settle(&mut rt);
    assert_eq!(shape(&rt), moved);
    // The second refused: back to what the instance keeps, and said over the composer.
    let last = rt.state.session.rail.sent();
    rt.update(move |s, _| {
        s.receive(&Update::Rail(RailUpdate::Arranged {
            sent: last,
            result: Err(Failure {
                code: "INVALID_ARGUMENT".into(),
                message: "invalid argument".into(),
                details: [("field".to_string(), "folders".to_string())].into(),
            }),
        }));
    });
    settle(&mut rt);
    assert_eq!(shape(&rt), kept);
    // In the web's words, not the instance's.
    assert_eq!(
        rt.state.session.last_error.as_deref(),
        Some("The rail was not saved: There can be at most 100 folders.")
    );
}
