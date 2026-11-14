//! The rail as an account arranged it (architecture 6.9): Acme on its own, a closed folder
//! named Work with four servers, and an open one with no name holding Garden and Trail
//! Runners, as the Server folders board has them; and the menus and Folder settings.

use lumia_client::app::ServerSummary;
use lumia_client::rail::RailItem;

use super::servers_sample;
use crate::app::State;
use crate::session::Update;

/// The folder named Work, closed.
pub const WORK: &str = "work";
/// The folder with no name, open.
pub const OUTDOORS: &str = "outdoors";

pub fn folders(state: &mut State) {
    servers_sample(state);
    let Some(mut snapshot) = state.session.snapshot.clone() else {
        return;
    };
    let server = |id: &str, name: &str| ServerSummary {
        server_id: id.into(),
        name: name.into(),
        can_manage: false,
        can_manage_channels: false,
        ..snapshot.servers[1].clone()
    };
    snapshot.servers.extend([
        server("northwind", "Northwind Studio"),
        server("guild", "Open Source Guild"),
        server("systems", "Design Systems"),
        server("beta", "Lumia Beta"),
        server("trail", "Trail Runners"),
    ]);
    let folder = |id: &str, name: &str, tone: &str, servers: &[&str]| RailItem::Folder {
        folder_id: id.into(),
        name: name.into(),
        tone: tone.into(),
        server_ids: servers.iter().map(|s| (*s).to_string()).collect(),
    };
    snapshot.rail = vec![
        RailItem::Server {
            server_id: "acme".into(),
        },
        folder(
            WORK,
            "Work",
            "mint",
            &["northwind", "guild", "systems", "beta"],
        ),
        folder(OUTDOORS, "", "indigo", &["garden", "trail"]),
    ];
    state.receive(&Update::Snapshot(Ok(snapshot)));
    state.settings.open_folders = vec![OUTDOORS.into()];
    state.select_server("acme");
}

/// A server's menu, Garden's, in the open folder.
pub fn server_menu(state: &mut State) {
    folders(state);
    state.open_rail_menu("garden");
}

/// The closed folder's menu.
pub fn folder_menu(state: &mut State) {
    folders(state);
    state.open_rail_menu(WORK);
}

/// Folder settings for Work, a colour chosen and not yet saved.
pub fn folder_settings(state: &mut State) {
    folder_menu(state);
    state.open_folder_settings();
    state.choose_folder_tone("sky");
}
