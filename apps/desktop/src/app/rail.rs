//! The rail as this account arranges it (architecture 6.9): its servers dragged into an
//! order, gathered in folders, and the menus that do the same without a pointer.
//!
//! Every edit is one of `lumia_client::rail`'s, made on the rail as it is and sent whole
//! ([`Session::arrange_rail`](crate::session::Session::arrange_rail)), so the shells move
//! tiles by the same rules. Which folders are open is this computer's own
//! (`settings.open_folders`).
//!
//! A drag starts once the pointer has gone [`DRAG_START`] from the press, and is over the
//! tile the pointer is over ([`target`]): its top or bottom quarter is between tiles, its
//! middle half onto it. Escape puts the tile back. A move is said aloud, for a screen reader.

use std::collections::HashMap;

use lumia_client::rail::{self, RailItem};
use rune::{Drag, LiveRegion, Phase, TextEdit, Tx};

use super::{Popover, State};

/// How far, in points, the pointer goes from a press before it is a drag.
pub const DRAG_START: f32 = 4.0;

/// What a tile on the rail is, for finding what a drag is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpotKind {
    Server,
    /// A closed folder's tile.
    Folder,
    /// An open folder's head, over its servers.
    Head,
}

/// A tile as last laid out: what it is, the open folder it is in, and its rect in window
/// points.
#[derive(Debug, Clone, PartialEq)]
pub struct Spot {
    pub kind: SpotKind,
    pub folder: Option<String>,
    pub rect: [f32; 4],
}

/// Where a dragged tile would go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Between tiles: before `before` (at the end without one), on the rail itself or in
    /// the open folder `folder`.
    Between {
        folder: Option<String>,
        before: Option<String>,
    },
    /// Onto a server (the two become a folder, or it joins the folder that one is in) or
    /// onto a folder (into it, at its end).
    Onto(String),
}

/// A tile being dragged.
#[derive(Debug, Clone, PartialEq)]
pub struct RailDrag {
    /// The server's or the folder's id.
    pub id: String,
    /// Where the tile's top left corner follows the pointer to, in window points.
    pub at: [f32; 2],
    /// What it is over, when dropping it there would change the rail.
    pub target: Option<Target>,
}

/// The rail's part of the window.
#[derive(Debug, Default)]
pub struct RailUi {
    /// Every tile as last laid out, by its server's or folder's id.
    pub spots: HashMap<String, Spot>,
    pub drag: Option<RailDrag>,
    /// A drag just ended on its own tile: the press that ends it too opens nothing.
    pub dropped: bool,
    /// The tile whose menu is open (`Popover::RailMenu` and `Popover::MoveToFolder`).
    pub menu_for: Option<String>,
    /// The folder whose settings are open (`Popover::FolderSettings`), its name and tone
    /// as chosen there.
    pub editing: Option<String>,
    pub name: TextEdit,
    pub tone: String,
}

/// The rail as `items` would be with `dragged` dropped on `target`; `folder_id` names a
/// folder the drop makes.
pub fn dropped(
    items: &[RailItem],
    dragged: &str,
    target: &Target,
    folder_id: &str,
) -> Vec<RailItem> {
    match target {
        Target::Between { before, .. } if before.as_deref() == Some(dragged) => items.to_vec(),
        Target::Between { folder, before } => {
            rail::move_to(items, dragged, folder.as_deref(), before.as_deref())
        }
        Target::Onto(onto) if is_folder(items, onto) => rail::into_folder(items, dragged, onto),
        Target::Onto(onto) => rail::merge(items, dragged, onto, folder_id),
    }
}

fn is_folder(items: &[RailItem], id: &str) -> bool {
    items
        .iter()
        .any(|item| matches!(item, RailItem::Folder { folder_id, .. } if folder_id == id))
}

/// The tile after `id` on the rail itself.
fn next_on_rail(items: &[RailItem], id: &str) -> Option<String> {
    let at = items.iter().position(|item| rail::id_of(item) == id)?;
    items.get(at + 1).map(|item| rail::id_of(item).to_string())
}

/// The server after `id` in its folder.
fn next_in_folder(items: &[RailItem], id: &str) -> Option<String> {
    items.iter().find_map(|item| match item {
        RailItem::Folder { server_ids, .. } => {
            let at = server_ids.iter().position(|s| s == id)?;
            server_ids.get(at + 1).cloned()
        }
        RailItem::Server { .. } => None,
    })
}

/// The tiles as they are drawn, top to bottom: each item's, and an open folder's head
/// followed by its servers.
fn drawn_order(items: &[RailItem], open: &[String]) -> Vec<String> {
    let mut order = Vec::new();
    for item in items {
        order.push(rail::id_of(item).to_string());
        if let RailItem::Folder {
            folder_id,
            server_ids,
            ..
        } = item
            && open.contains(folder_id)
        {
            order.extend(server_ids.iter().cloned());
        }
    }
    order
}

/// What a tile dragged with the pointer at `y` (window points) is over: the first tile
/// drawn whose bottom, with half the gap under it, is below the pointer; past the last,
/// the rail's end.
pub fn target(
    items: &[RailItem],
    open: &[String],
    spots: &HashMap<String, Spot>,
    dragged: &str,
    y: f32,
) -> Option<Target> {
    let half_gap = crate::tokens::SPACE_12 / 2.0;
    let dragging_folder = is_folder(items, dragged);
    let rail_end = Target::Between {
        folder: None,
        before: None,
    };
    let Some((id, spot)) = drawn_order(items, open).into_iter().find_map(|id| {
        let spot = spots.get(&id)?;
        (y < spot.rect[1] + spot.rect[3] + half_gap).then(|| (id, spot.clone()))
    }) else {
        return Some(rail_end);
    };
    let [_, top, _, height] = spot.rect;
    let f = ((y - top) / height.max(1.0)).clamp(0.0, 1.0);
    let between =
        |folder: Option<String>, before: Option<String>| Some(Target::Between { folder, before });
    match (spot.kind, &spot.folder) {
        // A tile on the rail itself.
        (SpotKind::Server | SpotKind::Folder, None) => {
            let after = next_on_rail(items, &id);
            if f < 0.25 {
                between(None, Some(id))
            } else if f > 0.75 {
                between(None, after)
            } else if id == dragged {
                None
            } else if dragging_folder {
                // A folder goes between tiles only.
                if f < 0.5 {
                    between(None, Some(id))
                } else {
                    between(None, after)
                }
            } else {
                Some(Target::Onto(id))
            }
        }
        // An open folder's head: its top quarter is before the folder, the rest the
        // folder's first place.
        (SpotKind::Head, _) => {
            if f < 0.25 || dragging_folder {
                between(None, Some(id))
            } else {
                let first = items.iter().find_map(|item| match item {
                    RailItem::Folder {
                        folder_id,
                        server_ids,
                        ..
                    } if *folder_id == id => server_ids.first().cloned(),
                    _ => None,
                });
                between(Some(id), first)
            }
        }
        // A server in an open folder. A folder dragged there goes after the folder.
        (_, Some(folder)) => {
            if dragging_folder {
                between(None, next_on_rail(items, folder))
            } else if f < 0.25 {
                between(Some(folder.clone()), Some(id))
            } else if f > 0.75 {
                between(Some(folder.clone()), next_in_folder(items, &id))
            } else if id == dragged {
                None
            } else {
                Some(Target::Onto(id))
            }
        }
    }
}

impl State {
    /// The rail as the session has it.
    fn rail_items(&self) -> Vec<RailItem> {
        self.session.rail()
    }

    /// A server's name, or a folder's (its own, else its servers').
    pub fn rail_name(&self, id: &str) -> String {
        let server_name = |id: &str| {
            self.session
                .servers()
                .iter()
                .find(|s| s.server_id == id)
                .map(|s| s.name.clone())
        };
        if let Some(name) = server_name(id) {
            return name;
        }
        self.rail_items()
            .iter()
            .find_map(|item| match item {
                RailItem::Folder {
                    folder_id,
                    name,
                    server_ids,
                    ..
                } if folder_id == id => Some(if name.is_empty() {
                    let names: Vec<String> =
                        server_ids.iter().filter_map(|s| server_name(s)).collect();
                    rail::folder_words(&names)
                } else {
                    name.clone()
                }),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Whether the folder `id` is open on this computer.
    pub fn folder_open(&self, id: &str) -> bool {
        self.settings.open_folders.iter().any(|f| f == id)
    }

    /// Opens a closed folder, closes an open one; this computer remembers which.
    pub fn toggle_folder(&mut self, id: &str) {
        let live: Vec<String> = self
            .rail_items()
            .iter()
            .filter(|item| matches!(item, RailItem::Folder { .. }))
            .map(|item| rail::id_of(item).to_string())
            .collect();
        let open = &mut self.settings.open_folders;
        // A folder gone from the rail is forgotten here too.
        open.retain(|f| live.contains(f));
        match open.iter().position(|f| f == id) {
            Some(at) => {
                open.remove(at);
            }
            None => open.push(id.to_string()),
        }
        self.settings.save(&self.data_dir);
    }

    /// Records where a tile was laid out, for a drag to find it.
    pub fn place_spot(&mut self, id: &str, spot: Spot) {
        if self.rail.spots.get(id) != Some(&spot) {
            self.rail.spots.insert(id.to_string(), spot);
        }
    }

    /// What a drag of the tile `id` does at each phase: past [`DRAG_START`] the tile
    /// follows the pointer and says what it is over; let go there, the rail is arranged.
    pub fn drag_rail(&mut self, id: &str, drag: &Drag, tx: &mut Tx) {
        match drag.phase {
            Phase::Down => {
                self.rail.drag = None;
                self.rail.dropped = false;
            }
            Phase::Move if drag.cancelled() => self.rail.drag = None,
            Phase::Move => {
                if self.rail.drag.is_none() && drag.dx.hypot(drag.dy) < DRAG_START {
                    return;
                }
                let [x, y] = drag.window_point();
                let items = self.rail_items();
                let found = target(&items, &self.settings.open_folders, &self.rail.spots, id, y)
                    // Over somewhere that changes nothing, there is nothing to show.
                    .filter(|t| dropped(&items, id, t, "") != items);
                self.popover = None;
                self.rail.drag = Some(RailDrag {
                    id: id.to_string(),
                    at: [x - drag.start_x, y - drag.start_y],
                    target: found,
                });
            }
            Phase::Up => {
                let Some(dragging) = self.rail.drag.take() else {
                    return;
                };
                // The press this release makes over the tile still is the drag's end, not
                // a press.
                let [x, y] = drag.window_point();
                self.rail.dropped = self.rail.spots.get(id).is_some_and(|spot| {
                    let [left, top, w, h] = spot.rect;
                    (left..left + w).contains(&x) && (top..top + h).contains(&y)
                });
                if drag.cancelled() {
                    return;
                }
                if let Some(target) = dragging.target {
                    self.drop_on(id, &target, tx);
                }
            }
        }
    }

    /// Whether a press on a rail tile is the end of a drag, which opens nothing.
    pub fn press_after_drag(&mut self) -> bool {
        std::mem::take(&mut self.rail.dropped)
    }

    fn drop_on(&mut self, id: &str, target: &Target, tx: &mut Tx) {
        let items = self.rail_items();
        let folder_id = rail::new_folder_id();
        let next = dropped(&items, id, target, &folder_id);
        let name = self.rail_name(id);
        let said = match target {
            Target::Onto(onto) if is_folder(&items, onto) => {
                format!("{name} moved into {}", self.rail_name(onto))
            }
            Target::Onto(onto) => match rail::folder_of(&items, onto) {
                Some(folder) => format!("{name} moved into {}", self.rail_name(&folder)),
                None => format!("New folder with {} and {name}", self.rail_name(onto)),
            },
            Target::Between { .. } => String::new(),
        };
        self.arrange(next, id, said, tx);
    }

    /// Sends `next` as the rail and says what became of `id`: `said`, or its new place.
    fn arrange(&mut self, next: Vec<RailItem>, id: &str, said: String, tx: &mut Tx) {
        self.session.arrange_rail(next);
        let items = self.rail_items();
        let said = if said.is_empty() {
            let name = self.rail_name(id);
            let at = rail::position_of(&items, id).unwrap_or(1);
            match rail::folder_of(&items, id) {
                Some(folder) => {
                    let total = items
                        .iter()
                        .find_map(|item| match item {
                            RailItem::Folder {
                                folder_id,
                                server_ids,
                                ..
                            } if *folder_id == folder => Some(server_ids.len()),
                            _ => None,
                        })
                        .unwrap_or(at);
                    format!("{name}, {at} of {total} in {}", self.rail_name(&folder))
                }
                None => format!("{name}, {at} of {} on the rail", items.len()),
            }
        } else {
            said
        };
        tx.announce(said, LiveRegion::Polite);
    }

    // ---- The menus ----------------------------------------------------------------------

    /// Opens the menu of the tile `id` (a right-click, Control-click, the Menu key).
    pub fn open_rail_menu(&mut self, id: &str) {
        self.rail.drag = None;
        self.rail.menu_for = Some(id.to_string());
        self.open_popover(Popover::RailMenu);
    }

    /// "Move to folder ›": the folders, under the same tile.
    pub fn open_move_to_folder(&mut self) {
        if self.rail.menu_for.is_some() {
            self.open_popover(Popover::MoveToFolder);
        }
    }

    fn menu_target(&mut self) -> Option<String> {
        self.popover = None;
        self.rail.menu_for.take()
    }

    /// Move up (`by` -1) or down (+1), where the tile is.
    pub fn rail_step(&mut self, by: i32, tx: &mut Tx) {
        let Some(id) = self.menu_target() else { return };
        let next = rail::step(&self.rail_items(), &id, by);
        self.arrange(next, &id, String::new(), tx);
    }

    /// "New folder": the server in a folder of its own, where it is, with its Folder
    /// settings open to name it at once (a folder made by a drag opens none).
    pub fn rail_new_folder(&mut self, tx: &mut Tx) {
        let Some(id) = self.menu_target() else { return };
        let folder = rail::new_folder_id();
        let next = rail::new_folder(&self.rail_items(), &id, &folder);
        let said = format!("New folder with {}", self.rail_name(&id));
        self.arrange(next, &id, said, tx);
        self.rail.menu_for = Some(folder);
        self.open_folder_settings();
    }

    /// The server into the folder `folder`, at its end.
    pub fn rail_into_folder(&mut self, folder: &str, tx: &mut Tx) {
        let Some(id) = self.menu_target() else { return };
        let next = rail::into_folder(&self.rail_items(), &id, folder);
        let said = format!(
            "{} moved into {}",
            self.rail_name(&id),
            self.rail_name(folder)
        );
        self.arrange(next, &id, said, tx);
    }

    /// "Remove from folder": the server just after its folder on the rail.
    pub fn rail_out_of_folder(&mut self, tx: &mut Tx) {
        let Some(id) = self.menu_target() else { return };
        let items = self.rail_items();
        let Some(folder) = rail::folder_of(&items, &id) else {
            return;
        };
        let said = format!(
            "{} taken out of {}",
            self.rail_name(&id),
            self.rail_name(&folder)
        );
        let next = rail::out_of_folder(&items, &id);
        self.arrange(next, &id, said, tx);
    }

    /// "Delete folder": its servers stay where it was.
    pub fn rail_delete_folder(&mut self, tx: &mut Tx) {
        let Some(id) = self.menu_target() else { return };
        let said = format!("{} deleted, its servers kept", self.rail_name(&id));
        let next = rail::delete_folder(&self.rail_items(), &id);
        self.settings.open_folders.retain(|f| *f != id);
        self.arrange(next, &id, said, tx);
    }

    /// "Folder settings…": its name and tone, to change.
    pub fn open_folder_settings(&mut self) {
        let Some(id) = self.menu_target() else { return };
        let Some((name, tone)) = self.rail_items().iter().find_map(|item| match item {
            RailItem::Folder {
                folder_id,
                name,
                tone,
                ..
            } if *folder_id == id => Some((name.clone(), tone.clone())),
            _ => None,
        }) else {
            return;
        };
        self.rail.name = TextEdit::new(name);
        self.rail.tone = tone;
        self.rail.menu_for = Some(id.clone());
        self.rail.editing = Some(id);
        self.open_popover(Popover::FolderSettings);
    }

    pub fn choose_folder_tone(&mut self, tone: &str) {
        self.rail.tone = tone.to_string();
    }

    /// Saves Folder settings and closes them.
    pub fn save_folder_settings(&mut self, tx: &mut Tx) {
        let Some(id) = self.rail.editing.take() else {
            return;
        };
        self.popover = None;
        self.rail.menu_for = None;
        let next = rail::set_folder(
            &self.rail_items(),
            &id,
            &self.rail.name.text,
            &self.rail.tone,
        );
        let said = format!("Folder saved as {}", {
            let name = self.rail.name.text.trim();
            if name.is_empty() {
                "its servers' names".to_string()
            } else {
                name.to_string()
            }
        });
        self.arrange(next, &id, said, tx);
    }

    /// Escape on the rail while a tile is dragged: back where it was.
    pub fn cancel_rail_drag(&mut self) {
        self.rail.drag = None;
    }
}
