//! The rail's servers as this account arranged them (architecture 6.9), as the Server folders
//! board draws them: rounded squares, a closed folder as a tile in its tone with its first
//! four servers small, an open one as its glyph over its servers in a tinted column. A tile
//! drags (`app::rail`), and a right-click, Control-click or the Menu key opens its menu.
//!
//! While a tile is dragged it stays in its place at a third of its strength; a copy follows
//! the pointer, a line in the accent says where it would land between tiles, and a tile it
//! would land on is ringed. Both are drawn at the root ([`drag_layer`]) so nothing clips them.

use lumia_client::app::ServerSummary;
use lumia_client::rail::{self, RailItem, TONES};
use rune::{Align, Cursor, Drag, El, PointerPress, Side, Tx, col, el, icon, row, text};

use super::components::{
    Look, drawn, field, initials, menu, menu_glyph, menu_item, menu_rule, popover, primary_button,
    secondary_button, shadowed,
};
use crate::app::rail::{Spot, SpotKind, Target};
use crate::app::{Popover, State};
use crate::tokens::{self as t, icons};

/// A server's tile key, which its menu and a drag's line hang from.
pub fn server_key(id: &str) -> String {
    format!("server-{id}")
}

/// A folder's tile key: a closed folder's tile, or an open one's head.
pub fn folder_key(id: &str) -> String {
    format!("folder-{id}")
}

/// The key of the tile `id` names, a server's or a folder's.
fn tile_key(s: &State, id: &str) -> String {
    if s.session.servers().iter().any(|x| x.server_id == id) {
        server_key(id)
    } else {
        folder_key(id)
    }
}

/// A tile across, at the rail's rounded square: what every tile is before its fill.
fn tile(key: String, label: &str, look: Look) -> El {
    el().key(key)
        .size(t::TILE_SIZE, t::TILE_SIZE)
        .center()
        .radius(t::RADIUS_14)
        .role("button")
        .label(label)
        .help(label)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .focus_ring(2.0, 2.0, look.c.focus)
        .no_window_drag()
}

/// What makes a tile draggable, give it a menu and tell a drag where it is.
fn handled(tile: El, id: &str, kind: SpotKind, folder: Option<&str>) -> El {
    let (drag_id, menu_id, spot_id) = (id.to_string(), id.to_string(), id.to_string());
    let folder = folder.map(str::to_string);
    tile.on_drag(move |s: &mut State, drag: &mut Drag, tx: &mut Tx| {
        s.drag_rail(&drag_id, drag, tx);
    })
    .on_secondary_press(move |s: &mut State, _: PointerPress, _: &mut Tx| {
        s.open_rail_menu(&menu_id);
    })
    .on_layout_tx(move |s: &mut State, rect: [f32; 4], tx: &mut Tx| {
        s.place_spot(
            &spot_id,
            Spot {
                kind,
                folder: folder.clone(),
                rect,
            },
        );
        // Only a drag reads where tiles are; nothing to draw again.
        tx.live_only();
    })
}

/// How a tile looks while a drag is on: its own at a third, the one it would land on
/// ringed.
fn dragged(s: &State, id: &str, tile: El, look: Look) -> El {
    let Some(drag) = &s.rail.drag else {
        return tile;
    };
    if drag.id == id {
        tile.opacity(t::DRAGGED_OPACITY)
    } else if drag.target == Some(Target::Onto(id.to_string())) {
        shadowed(tile, look.shadows.ring_current)
    } else {
        tile
    }
}

/// The servers and folders between Home and **+**, a list.
pub fn servers(s: &State, current: Option<&str>, look: Look) -> El {
    let mut list = col()
        .key("rail-list")
        .gap(t::SPACE_12)
        .items_center()
        .role("list")
        .label("Servers");
    for item in s.session.rail() {
        let entry = match &item {
            RailItem::Server { server_id } => s
                .session
                .servers()
                .iter()
                .find(|x| x.server_id == *server_id)
                .map(|server| server_slot(s, server, current, None, look)),
            RailItem::Folder {
                folder_id,
                tone,
                server_ids,
                ..
            } => Some(folder(s, folder_id, tone, server_ids, current, look)),
        };
        if let Some(entry) = entry {
            list = list.child(entry.role("listitem"));
        }
    }
    list
}

/// What is unread in a server: anything, and what is for this account.
fn unread(s: &State, server_id: &str) -> (bool, u32) {
    s.session.server_unread(server_id)
}

/// A server's tile with what is unread in it over its corner.
fn server_slot(
    s: &State,
    server: &ServerSummary,
    current: Option<&str>,
    folder: Option<&str>,
    look: Look,
) -> El {
    let c = look.c;
    let active = current == Some(server.server_id.as_str());
    let id = server.server_id.clone();
    let (any, mentions) = unread(s, &server.server_id);
    let named = match (any, mentions) {
        (_, 0) if any => format!("{}, unread messages", server.name),
        _ => super::server::with_unread(&server.name, (mentions, mentions)),
    };
    let mut tile = tile(server_key(&server.server_id), &named, look)
        .fill(if active { c.raised } else { c.tile })
        .on_press(move |s: &mut State| {
            if !s.press_after_drag() {
                s.select_server(&id);
            }
        });
    // The server's picture once this computer holds it; its initials until it is drawn.
    let picture = s.picture_at(&server.icon_url);
    if !picture.is_some_and(drawn) {
        tile = tile.child(
            text(initials(&server.name))
                .font(if active {
                    t::TITLE_MONOGRAM
                } else {
                    t::TEXT_MONOGRAM_REST
                })
                .color(if active {
                    c.ink_selected
                } else {
                    t::monogram_ink(&server.server_id, c)
                }),
        );
    }
    if let Some(picture) = picture {
        tile = tile.child(squared(picture, t::TILE_SIZE, t::RADIUS_14).a11y_hidden());
    }
    let tile = handled(tile, &server.server_id, SpotKind::Server, folder);
    let mut tile = dragged(s, &server.server_id, tile, look);
    if active {
        tile = shadowed(tile, look.shadows.ring_current);
    }
    marked(
        el().child(tile),
        any && !active,
        mentions,
        &server.name,
        look,
    )
}

/// A dot at a tile's top right for unread with nothing for this account, the count there
/// when there is (the board's; the count covers the dot).
fn marked(mut slot: El, any: bool, mentions: u32, name: &str, look: Look) -> El {
    let c = look.c;
    if any && mentions == 0 {
        let dot = t::SPACE_10 + 1.0;
        slot = slot.child(
            el().pin(None, Some(0.0), Some(0.0), None)
                .size(dot, dot)
                .radius(dot / 2.0)
                .fill(c.unread)
                .border(2.0, c.rail)
                .a11y_hidden(),
        );
    }
    if mentions > 0 {
        slot = slot.child(super::shell::rail_badge(
            mentions as usize,
            super::server::with_unread(name, (mentions, mentions)),
            look,
        ));
    }
    slot
}

/// A picture as a rounded square `size` across, over whatever it is a child of.
fn squared(picture: &rune::Image, size: f32, radius: f32) -> El {
    el().key("picture")
        .absolute(0.0, 0.0)
        .size(size, size)
        .radius(radius)
        .pixels_fit(picture.pixels(), rune::SurfaceFit::Fill)
}

/// A folder: closed, a tile; open, its head over its servers.
fn folder(
    s: &State,
    id: &str,
    tone: &str,
    server_ids: &[String],
    current: Option<&str>,
    look: Look,
) -> El {
    let c = look.c;
    let (ground, ink) = t::tone_named(tone, c);
    let name = s.rail_name(id);
    let servers: Vec<&ServerSummary> = server_ids
        .iter()
        .filter_map(|sid| s.session.servers().iter().find(|x| x.server_id == *sid))
        .collect();
    let open = s.folder_open(id);
    let toggled = id.to_string();
    let toggle = move |s: &mut State| {
        if !s.press_after_drag() {
            s.toggle_folder(&toggled);
        }
    };
    if open {
        let head = tile(folder_key(id), &format!("Folder, {name}"), look)
            .h(t::FOLDER_HEAD)
            .radius(t::RADIUS_12)
            .hover(t::folder_edge(ink))
            .expanded(true)
            .on_press(toggle)
            .child(icon(&icons::FOLDER, t::SPACE_18).color(ink));
        let head = dragged(s, id, handled(head, id, SpotKind::Head, None), look);
        let column = col()
            .key(format!("folder-column-{id}"))
            .w(t::FOLDER_COLUMN)
            .pt(t::SPACE_4)
            .pb(t::SPACE_8)
            .gap(t::FOLDER_GAP)
            .items_center()
            .radius(t::RADIUS_18)
            .fill(ground)
            .child(head);
        let mut inside = col()
            .gap(t::FOLDER_GAP)
            .items_center()
            .role("list")
            .label(format!("Folder, {name}"));
        for server in servers {
            inside = inside.child(server_slot(s, server, current, Some(id), look).role("listitem"));
        }
        return column.child(inside);
    }
    // Closed: its first four servers small, what is unread in all of them added up, and
    // ringed while the server on screen is inside.
    let (mut any, mut mentions) = (false, 0);
    for server in &servers {
        let (a, m) = unread(s, &server.server_id);
        any |= a;
        mentions += m;
    }
    let holds_current = servers
        .iter()
        .any(|x| current == Some(x.server_id.as_str()));
    let label = match (any, mentions) {
        (_, 0) if any => format!("Folder, {name}, unread messages"),
        _ => super::server::with_unread(&format!("Folder, {name}"), (mentions, mentions)),
    };
    let mut grid = col().gap(t::SPACE_4);
    for pair in servers.chunks(2).take(2) {
        let mut line = row().gap(t::SPACE_4);
        for server in pair {
            line = line.child(mini(s, server, look));
        }
        grid = grid.child(line);
    }
    let tile = tile(folder_key(id), &label, look)
        .fill(ground)
        .border(1.0, t::folder_edge(ink))
        .expanded(false)
        .on_press(toggle)
        .child(grid.a11y_hidden());
    let mut tile = dragged(s, id, handled(tile, id, SpotKind::Folder, None), look);
    if holds_current {
        tile = shadowed(tile, look.shadows.ring_current);
    }
    marked(
        el().child(tile),
        any && !holds_current,
        mentions,
        &name,
        look,
    )
}

/// A server small in a closed folder: its picture, else its initials, on the tile's colour.
fn mini(s: &State, server: &ServerSummary, look: Look) -> El {
    let c = look.c;
    let mut cell = el()
        .size(t::FOLDER_MINI, t::FOLDER_MINI)
        .radius(t::FOLDER_MINI_RADIUS)
        .fill(c.tile)
        .center();
    let picture = s.picture_at(&server.icon_url);
    if !picture.is_some_and(drawn) {
        cell = cell.child(
            text(initials(&server.name))
                .font(t::TEXT_FOLDER_MINI)
                .color(t::monogram_ink(&server.server_id, c)),
        );
    }
    if let Some(picture) = picture {
        cell = cell.child(squared(picture, t::FOLDER_MINI, t::FOLDER_MINI_RADIUS));
    }
    cell
}

/// The tile following the pointer and the line where it would land, over everything.
pub fn drag_layer(s: &State, look: Look) -> Option<El> {
    let drag = s.rail.drag.as_ref()?;
    let c = look.c;
    let mut layer = el()
        .key("rail-drag")
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .a11y_hidden();
    if let Some(Target::Between { folder, before }) = &drag.target
        && let Some(y) = line_y(s, folder.as_deref(), before.as_deref())
    {
        let x = (t::RAIL_WIDTH - t::RAIL_LINE) / 2.0;
        layer = layer.child(
            el().key("rail-line")
                .absolute(x, y - t::SPACE_2)
                .size(t::RAIL_LINE, t::SPACE_4)
                .radius(t::SPACE_2)
                .fill(c.accent),
        );
    }
    let is_server = s.session.servers().iter().any(|x| x.server_id == drag.id);
    let ink = if is_server {
        t::monogram_ink(&drag.id, c)
    } else {
        c.ink
    };
    let ghost = shadowed(
        el().key("rail-ghost")
            .absolute(drag.at[0], drag.at[1])
            .size(t::TILE_SIZE, t::TILE_SIZE)
            .radius(t::RADIUS_14)
            .fill(c.tile)
            .center()
            .child(
                text(initials(&s.rail_name(&drag.id)))
                    .font(t::TEXT_MONOGRAM_REST)
                    .color(ink),
            ),
        look.shadows.floating,
    );
    Some(layer.child(ghost))
}

/// Where the line between tiles goes, in window points: in the middle of the gap over
/// `before`, or under the last tile of the rail or of the folder.
fn line_y(s: &State, folder: Option<&str>, before: Option<&str>) -> Option<f32> {
    let spots = &s.rail.spots;
    if let Some(before) = before {
        let gap = if folder.is_some() {
            t::FOLDER_GAP
        } else {
            t::SPACE_12
        };
        return spots.get(before).map(|spot| spot.rect[1] - gap / 2.0);
    }
    let items = s.session.rail();
    let last = match folder {
        Some(folder) => items.iter().find_map(|item| match item {
            RailItem::Folder {
                folder_id,
                server_ids,
                ..
            } if folder_id == folder => server_ids.last().cloned(),
            _ => None,
        })?,
        None => {
            let item = items.last()?;
            let id = rail::id_of(item).to_string();
            // Under an open folder, under its column's last server.
            match item {
                RailItem::Folder { server_ids, .. } if s.folder_open(&id) => {
                    let last = server_ids.last()?;
                    return spots
                        .get(last)
                        .map(|spot| spot.rect[1] + spot.rect[3] + t::SPACE_8 + t::SPACE_12 / 2.0);
                }
                _ => id,
            }
        }
    };
    let gap = if folder.is_some() {
        t::FOLDER_GAP
    } else {
        t::SPACE_12
    };
    spots
        .get(&last)
        .map(|spot| spot.rect[1] + spot.rect[3] + gap / 2.0)
}

// ---- The menus --------------------------------------------------------------------------

/// A server's or a folder's menu, beside its tile.
pub fn rail_menu(s: &State, look: Look) -> Option<El> {
    let id = s.rail.menu_for.clone()?;
    let items = s.session.rail();
    let in_folder = rail::folder_of(items, &id);
    let is_folder = items
        .iter()
        .any(|item| matches!(item, RailItem::Folder { folder_id, .. } if *folder_id == id));
    // Where it stands where it is, to leave out a move it cannot make.
    let (at, of) = match &in_folder {
        Some(folder) => items
            .iter()
            .find_map(|item| match item {
                RailItem::Folder {
                    folder_id,
                    server_ids,
                    ..
                } if folder_id == folder => {
                    Some((server_ids.iter().position(|s| *s == id)?, server_ids.len()))
                }
                _ => None,
            })
            .unwrap_or((0, 1)),
        None => (
            items
                .iter()
                .position(|item| rail::id_of(item) == id)
                .unwrap_or(0),
            items.len(),
        ),
    };
    let name = s.rail_name(&id);
    let mut list = menu(
        "rail-menu",
        &tile_key(s, &id),
        &if is_folder {
            format!("Folder, {name}")
        } else {
            name
        },
        Side::Right,
        Align::Start,
        look,
    );
    if at > 0 {
        list = list.child(
            menu_item(
                "rail-move-up",
                Some(menu_glyph(&icons::ARROW_UP, look)),
                "Move up",
                None,
                |_: &mut State| {},
                look,
            )
            .on_press_tx(|s: &mut State, tx| s.rail_step(-1, tx)),
        );
    }
    if at + 1 < of {
        list = list.child(
            menu_item(
                "rail-move-down",
                Some(menu_glyph(&icons::ARROW_DOWN, look)),
                "Move down",
                None,
                |_: &mut State| {},
                look,
            )
            .on_press_tx(|s: &mut State, tx| s.rail_step(1, tx)),
        );
    }
    if of > 1 {
        list = list.child(menu_rule(look));
    }
    if is_folder {
        return Some(
            list.child(menu_item(
                "rail-folder-settings",
                Some(menu_glyph(&icons::SETTINGS, look)),
                "Folder settings…",
                None,
                State::open_folder_settings,
                look,
            ))
            .child(menu_rule(look))
            .child(
                menu_item(
                    "rail-delete-folder",
                    Some(icon(&icons::TRASH, t::SPACE_16).color(look.c.danger)),
                    "Delete folder",
                    None,
                    |_: &mut State| {},
                    look,
                )
                .on_press_tx(|s: &mut State, tx| s.rail_delete_folder(tx)),
            ),
        );
    }
    list = list.child(
        menu_item(
            "rail-new-folder",
            Some(menu_glyph(&icons::FOLDER_PLUS, look)),
            "New folder",
            None,
            |_: &mut State| {},
            look,
        )
        .on_press_tx(|s: &mut State, tx| s.rail_new_folder(tx)),
    );
    let elsewhere = items.iter().any(|item| {
        matches!(item, RailItem::Folder { folder_id, .. } if Some(folder_id) != in_folder.as_ref())
    });
    if elsewhere {
        list = list.child(
            menu_item(
                "rail-move-to-folder",
                Some(menu_glyph(&icons::FOLDER, look)),
                "Move to folder",
                Some(icon(&icons::CHEVRON_RIGHT, t::SPACE_14).color(look.c.ink_faint)),
                State::open_move_to_folder,
                look,
            )
            .expanded(s.popover == Some(Popover::MoveToFolder)),
        );
    }
    if in_folder.is_some() {
        list = list.child(
            menu_item(
                "rail-out-of-folder",
                Some(menu_glyph(&icons::FOLDER_REMOVE, look)),
                "Remove from folder",
                None,
                |_: &mut State| {},
                look,
            )
            .on_press_tx(|s: &mut State, tx| s.rail_out_of_folder(tx)),
        );
    }
    Some(list)
}

/// "Move to folder": every folder but the one the server is in, in its tone.
pub fn move_to_folder(s: &State, look: Look) -> Option<El> {
    let id = s.rail.menu_for.clone()?;
    let items = s.session.rail();
    let in_folder = rail::folder_of(items, &id);
    let mut list = menu(
        "move-to-folder-menu",
        &tile_key(s, &id),
        "Move to folder",
        Side::Right,
        Align::Start,
        look,
    );
    for item in items {
        let RailItem::Folder {
            folder_id, tone, ..
        } = item
        else {
            continue;
        };
        if Some(folder_id) == in_folder.as_ref() {
            continue;
        }
        let (_, ink) = t::tone_named(tone, look.c);
        let folder = folder_id.clone();
        list = list.child(
            menu_item(
                format!("move-to-{folder_id}"),
                Some(icon(&icons::FOLDER, t::SPACE_16).color(ink)),
                &s.rail_name(folder_id),
                None,
                |_: &mut State| {},
                look,
            )
            .on_press_tx(move |s: &mut State, tx| s.rail_into_folder(&folder, tx)),
        );
    }
    Some(list)
}

/// Folder settings: its name, its colour, and Save.
pub fn folder_settings(s: &State, look: Look) -> Option<El> {
    let id = s.rail.editing.clone()?;
    let c = look.c;
    let names: Vec<String> = s
        .session
        .rail()
        .iter()
        .find_map(|item| match item {
            RailItem::Folder {
                folder_id,
                server_ids,
                ..
            } if *folder_id == id => Some(server_ids.clone()),
            _ => None,
        })?
        .iter()
        .filter_map(|sid| {
            s.session
                .servers()
                .iter()
                .find(|x| x.server_id == *sid)
                .map(|x| x.name.clone())
        })
        .collect();
    let mut swatches = row()
        .key("folder-tones")
        .gap(t::SPACE_6)
        .wrap()
        .role("radiogroup")
        .label("Colour");
    for tone in TONES {
        let (ground, ink) = t::tone_named(tone, c);
        let on = s.rail.tone == tone;
        let mut swatch = el()
            .key(format!("folder-tone-{tone}"))
            .size(t::TONE_SWATCH, t::TONE_SWATCH)
            .radius(t::TONE_SWATCH / 2.0)
            .fill(ground)
            .center()
            .role("radio")
            .label(capitalized(tone))
            .checked(on)
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .on_press(move |s: &mut State| s.choose_folder_tone(tone));
        swatch = if on {
            shadowed(swatch, look.shadows.ring_current)
                .child(icon(&icons::CHECK, t::SPACE_14).color(ink))
        } else {
            swatch.border(1.0, c.line_strong)
        };
        swatches = swatches.child(swatch);
    }
    Some(
        popover(
            "folder-settings",
            &folder_key(&id),
            "Folder settings",
            "Without a name it reads as its servers.",
            t::FOLDER_SETTINGS_WIDTH,
            look,
        )
        .anchored_fit(folder_key(&id), Side::Right, Align::Start, t::SPACE_6)
        .child(field(
            "folder-name",
            "Name",
            &s.rail.name,
            &rail::folder_words(&names),
            |s| &mut s.rail.name,
            |s, tx| s.save_folder_settings(tx),
            look,
        ))
        .child(
            col()
                .gap(t::SPACE_8)
                .child(text("Colour").font(t::TEXT_LABEL).color(c.ink_body))
                .child(swatches),
        )
        .child(
            row()
                .w_full()
                .gap(t::SPACE_8)
                .child(
                    secondary_button("folder-cancel", "Cancel", true, State::close_popover, look)
                        .h(t::CONTROL_PANE),
                )
                .child(
                    primary_button("folder-save", "Save", true, |_: &mut State| {}, look)
                        .on_press_tx(|s: &mut State, tx| s.save_folder_settings(tx))
                        .h(t::CONTROL_PANE)
                        .shrink()
                        .min_w(0.0),
                ),
        ),
    )
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
