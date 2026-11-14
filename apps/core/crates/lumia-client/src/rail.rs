//! The server rail as the account arranged it (architecture 6.9): its servers in an order of
//! its own, some gathered in folders. What a shell draws and every edit it makes, as values,
//! so the four shells move tiles by the same rules: each edit takes the rail as it is and
//! answers it as it is to be, which [`App::arrange_rail`](crate::app::App::arrange_rail)
//! then keeps.

use std::collections::HashSet;

use lumia_core::proto::{rail_entry, Rail, RailEntry, RailFolder};
use lumia_core::wire;
use serde::{Deserialize, Serialize};

/// One tile of the rail between Home and **+**: a server, or a folder of servers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RailItem {
    Server {
        server_id: String,
    },
    /// Folders hold servers only.
    Folder {
        folder_id: String,
        /// Empty for none: it reads as its servers ([`folder_words`]).
        name: String,
        /// One of [`TONES`].
        tone: String,
        server_ids: Vec<String>,
    },
}

/// The nine tones a folder may have, the avatar tones in the order every shell hashes an id
/// over ([`tone_of`]).
pub const TONES: [&str; 9] = [
    "peach", "blue", "purple", "mint", "amber", "sky", "stone", "green", "indigo",
];

/// The most characters a folder's name keeps, once trimmed.
pub const NAME_MAX: usize = 50;

/// The tone an id is drawn in: the shells' avatar hash over [`TONES`]. A folder made by a drag
/// takes its first server's.
#[must_use]
pub fn tone_of(id: &str) -> String {
    let hash = id.encode_utf16().fold(0u32, |h, unit| {
        h.wrapping_mul(31).wrapping_add(u32::from(unit))
    });
    TONES[hash as usize % TONES.len()].to_string()
}

/// How a folder with no name reads: "Acme", "Acme and Lumia", "Acme, Lumia and Trail
/// Runners", "Acme, Lumia and 2 more".
#[must_use]
pub fn folder_words(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [a, b, c] => format!("{a}, {b} and {c}"),
        [a, b, rest @ ..] => format!("{a}, {b} and {} more", rest.len()),
    }
}

/// The rail as the instance keeps it, as items.
#[must_use]
pub fn from_proto(rail: &Rail) -> Vec<RailItem> {
    rail.entries
        .iter()
        .filter_map(|entry| match entry.entry.as_ref()? {
            rail_entry::Entry::ServerId(id) => Some(RailItem::Server {
                server_id: wire::display(id),
            }),
            rail_entry::Entry::Folder(f) => Some(RailItem::Folder {
                folder_id: wire::display(&f.folder_id),
                name: f.name.clone(),
                tone: f.tone.clone(),
                server_ids: f.server_ids.iter().map(|id| wire::display(id)).collect(),
            }),
        })
        .collect()
}

/// Items as the instance takes them; an id that does not parse is left out.
#[must_use]
pub fn to_proto(items: &[RailItem]) -> Rail {
    let parse = |id: &str| uuid::Uuid::parse_str(id).ok().map(wire::uuid_bytes);
    Rail {
        entries: items
            .iter()
            .filter_map(|item| match item {
                RailItem::Server { server_id } => Some(RailEntry {
                    entry: Some(rail_entry::Entry::ServerId(parse(server_id)?)),
                }),
                RailItem::Folder {
                    folder_id,
                    name,
                    tone,
                    server_ids,
                } => Some(RailEntry {
                    entry: Some(rail_entry::Entry::Folder(RailFolder {
                        folder_id: parse(folder_id)?,
                        name: name.clone(),
                        tone: tone.clone(),
                        server_ids: server_ids.iter().filter_map(|id| parse(id)).collect(),
                    })),
                }),
            })
            .collect(),
    }
}

/// Every server in `servers` placed once, as the instance keeps it: the arrangement's where it
/// names them, the rest after in `servers`' order; servers not in `servers` dropped, empty
/// folders gone. A kept snapshot from before arrangements reads as `servers` in order.
#[must_use]
pub fn tidy(items: &[RailItem], servers: &[String]) -> Vec<RailItem> {
    let member: HashSet<&str> = servers.iter().map(String::as_str).collect();
    let mut placed: HashSet<String> = HashSet::new();
    let mut take = |id: &String| member.contains(id.as_str()) && placed.insert(id.clone());
    let mut kept = Vec::new();
    for item in items {
        match item {
            RailItem::Server { server_id } => {
                if take(server_id) {
                    kept.push(item.clone());
                }
            }
            RailItem::Folder {
                folder_id,
                name,
                tone,
                server_ids,
            } => {
                let inside: Vec<String> =
                    server_ids.iter().filter(|id| take(id)).cloned().collect();
                if !inside.is_empty() {
                    kept.push(RailItem::Folder {
                        folder_id: folder_id.clone(),
                        name: name.trim().to_string(),
                        tone: tone.clone(),
                        server_ids: inside,
                    });
                }
            }
        }
    }
    for id in servers {
        if placed.insert(id.clone()) {
            kept.push(RailItem::Server {
                server_id: id.clone(),
            });
        }
    }
    kept
}

/// Every server in the rail's order, a folder's in its place: how servers are listed
/// everywhere else (architecture 6.9).
#[must_use]
pub fn server_order(items: &[RailItem]) -> Vec<String> {
    items
        .iter()
        .flat_map(|item| match item {
            RailItem::Server { server_id } => vec![server_id.clone()],
            RailItem::Folder { server_ids, .. } => server_ids.clone(),
        })
        .collect()
}

/// The id a tile goes by: its server's, or its folder's.
#[must_use]
pub fn id_of(item: &RailItem) -> &str {
    match item {
        RailItem::Server { server_id } => server_id,
        RailItem::Folder { folder_id, .. } => folder_id,
    }
}

/// The folder a server is in, if any.
#[must_use]
pub fn folder_of(items: &[RailItem], server_id: &str) -> Option<String> {
    items.iter().find_map(|item| match item {
        RailItem::Folder {
            folder_id,
            server_ids,
            ..
        } if server_ids.iter().any(|id| id == server_id) => Some(folder_id.clone()),
        _ => None,
    })
}

/// Takes a server out of wherever it is, or a folder out of the rail; what was taken, and
/// whether it was found. A folder left empty goes.
fn take(items: &mut Vec<RailItem>, id: &str) -> Option<RailItem> {
    if let Some(at) = items.iter().position(|item| id_of(item) == id) {
        return Some(items.remove(at));
    }
    for at in 0..items.len() {
        if let RailItem::Folder { server_ids, .. } = &mut items[at] {
            if let Some(inner) = server_ids.iter().position(|s| s == id) {
                let server_id = server_ids.remove(inner);
                if server_ids.is_empty() {
                    items.remove(at);
                }
                return Some(RailItem::Server { server_id });
            }
        }
    }
    None
}

/// Moves a server or a folder. With `folder`, a server goes into that folder, before `before`
/// (a server of it) or at its end; without, either goes on the rail itself, before `before`
/// (a tile of the rail) or at its end. A folder never goes into a folder, and a target that is
/// not there leaves the rail as it was.
#[must_use]
pub fn move_to(
    items: &[RailItem],
    id: &str,
    folder: Option<&str>,
    before: Option<&str>,
) -> Vec<RailItem> {
    let mut next = items.to_vec();
    let Some(moving) = take(&mut next, id) else {
        return items.to_vec();
    };
    match (folder, &moving) {
        (Some(folder), RailItem::Server { server_id }) => {
            let Some(RailItem::Folder { server_ids, .. }) = next.iter_mut().find(
                |item| matches!(item, RailItem::Folder { folder_id, .. } if folder_id == folder),
            ) else {
                return items.to_vec();
            };
            let at = before
                .and_then(|b| server_ids.iter().position(|s| s == b))
                .unwrap_or(server_ids.len());
            server_ids.insert(at, server_id.clone());
        }
        (Some(_), RailItem::Folder { .. }) => return items.to_vec(),
        (None, _) => {
            let at = before
                .and_then(|b| next.iter().position(|item| id_of(item) == b))
                .unwrap_or(next.len());
            next.insert(at, moving);
        }
    }
    next
}

/// One server dropped onto another on the rail: the two become a folder where `onto` was,
/// `onto` first, with no name and `onto`'s tone. Onto a server inside a folder, `dragged` joins
/// that folder before it instead.
#[must_use]
pub fn merge(items: &[RailItem], dragged: &str, onto: &str, folder_id: &str) -> Vec<RailItem> {
    if dragged == onto {
        return items.to_vec();
    }
    if let Some(folder) = folder_of(items, onto) {
        return move_to(items, dragged, Some(&folder), Some(onto));
    }
    let mut next = items.to_vec();
    if take(&mut next, dragged).is_none() {
        return items.to_vec();
    }
    let Some(at) = next
        .iter()
        .position(|item| matches!(item, RailItem::Server { server_id } if server_id == onto))
    else {
        return items.to_vec();
    };
    next[at] = RailItem::Folder {
        folder_id: folder_id.to_string(),
        name: String::new(),
        tone: tone_of(onto),
        server_ids: vec![onto.to_string(), dragged.to_string()],
    };
    next
}

/// A server put at the end of a folder.
#[must_use]
pub fn into_folder(items: &[RailItem], server_id: &str, folder: &str) -> Vec<RailItem> {
    move_to(items, server_id, Some(folder), None)
}

/// A server taken out of its folder, to sit just after it on the rail (where the folder was,
/// if that was its last server).
#[must_use]
pub fn out_of_folder(items: &[RailItem], server_id: &str) -> Vec<RailItem> {
    let Some(folder) = folder_of(items, server_id) else {
        return items.to_vec();
    };
    let at = items
        .iter()
        .position(|item| id_of(item) == folder)
        .unwrap_or(items.len());
    let mut next = items.to_vec();
    let _ = take(&mut next, server_id);
    let at = if next.len() < items.len() { at } else { at + 1 };
    next.insert(
        at.min(next.len()),
        RailItem::Server {
            server_id: server_id.to_string(),
        },
    );
    next
}

/// A server put in a folder of its own where it is ("New folder"): out of the folder it was in
/// first, if any, then wrapped. No name, its own tone.
#[must_use]
pub fn new_folder(items: &[RailItem], server_id: &str, folder_id: &str) -> Vec<RailItem> {
    let mut next = out_of_folder(items, server_id);
    if let Some(at) = next
        .iter()
        .position(|item| matches!(item, RailItem::Server { server_id: s } if s == server_id))
    {
        next[at] = RailItem::Folder {
            folder_id: folder_id.to_string(),
            name: String::new(),
            tone: tone_of(server_id),
            server_ids: vec![server_id.to_string()],
        };
    }
    next
}

/// A folder taken away, its servers left in its place, in their order.
#[must_use]
pub fn delete_folder(items: &[RailItem], folder: &str) -> Vec<RailItem> {
    items
        .iter()
        .flat_map(|item| match item {
            RailItem::Folder {
                folder_id,
                server_ids,
                ..
            } if folder_id == folder => server_ids
                .iter()
                .map(|id| RailItem::Server {
                    server_id: id.clone(),
                })
                .collect(),
            other => vec![other.clone()],
        })
        .collect()
}

/// A folder's name (trimmed, cut to [`NAME_MAX`]) and tone, from Folder settings. A tone not in
/// [`TONES`] leaves its tone as it was.
#[must_use]
pub fn set_folder(items: &[RailItem], folder: &str, name: &str, tone: &str) -> Vec<RailItem> {
    let name: String = name.trim().chars().take(NAME_MAX).collect();
    items
        .iter()
        .map(|item| match item {
            RailItem::Folder {
                folder_id,
                tone: was,
                server_ids,
                ..
            } if folder_id == folder => RailItem::Folder {
                folder_id: folder_id.clone(),
                name: name.clone(),
                tone: if TONES.contains(&tone) {
                    tone.to_string()
                } else {
                    was.clone()
                },
                server_ids: server_ids.clone(),
            },
            other => other.clone(),
        })
        .collect()
}

/// Move up (`by` -1) or down (+1): a server or a folder swaps with its neighbour where it is,
/// on the rail or in its folder; at an end it stays.
#[must_use]
pub fn step(items: &[RailItem], id: &str, by: i32) -> Vec<RailItem> {
    let mut next = items.to_vec();
    let swap = |len: usize, at: usize| -> Option<usize> {
        let to = i64::try_from(at).ok()? + i64::from(by);
        usize::try_from(to).ok().filter(|to| *to < len)
    };
    if let Some(at) = next.iter().position(|item| id_of(item) == id) {
        if let Some(to) = swap(next.len(), at) {
            next.swap(at, to);
        }
        return next;
    }
    for item in &mut next {
        if let RailItem::Folder { server_ids, .. } = item {
            if let Some(at) = server_ids.iter().position(|s| s == id) {
                if let Some(to) = swap(server_ids.len(), at) {
                    server_ids.swap(at, to);
                }
                break;
            }
        }
    }
    next
}

/// Where a tile stands, 1-based, for what is announced: its place on the rail, or in its folder.
#[must_use]
pub fn position_of(items: &[RailItem], id: &str) -> Option<usize> {
    if let Some(at) = items.iter().position(|item| id_of(item) == id) {
        return Some(at + 1);
    }
    items.iter().find_map(|item| match item {
        RailItem::Folder { server_ids, .. } => {
            server_ids.iter().position(|s| s == id).map(|at| at + 1)
        }
        RailItem::Server { .. } => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(id: &str) -> RailItem {
        RailItem::Server {
            server_id: id.into(),
        }
    }

    fn f(id: &str, servers: &[&str]) -> RailItem {
        RailItem::Folder {
            folder_id: id.into(),
            name: String::new(),
            tone: "mint".into(),
            server_ids: servers.iter().map(|x| (*x).to_string()).collect(),
        }
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn tidy_places_every_server_once() {
        let rail = vec![
            s("x"),
            s("b"),
            f("F", &["b", "a", "x"]),
            f("G", &["x"]),
            s("b"),
        ];
        assert_eq!(
            tidy(&rail, &ids(&["a", "b", "c"])),
            vec![s("b"), f("F", &["a"]), s("c")]
        );
        // Nothing arranged: the servers as given.
        assert_eq!(tidy(&[], &ids(&["a", "b"])), vec![s("a"), s("b")]);
    }

    #[test]
    fn moves_between_tiles_and_into_and_out_of_folders() {
        let rail = vec![s("a"), f("F", &["b", "c"]), s("d")];
        assert_eq!(
            move_to(&rail, "d", None, Some("a")),
            vec![s("d"), s("a"), f("F", &["b", "c"])]
        );
        assert_eq!(
            move_to(&rail, "a", Some("F"), Some("c")),
            vec![f("F", &["b", "a", "c"]), s("d")]
        );
        // Out of a folder onto the rail; the last one out leaves no folder.
        assert_eq!(
            move_to(&rail, "b", None, None),
            vec![s("a"), f("F", &["c"]), s("d"), s("b")]
        );
        let one = vec![f("F", &["b"]), s("a")];
        assert_eq!(move_to(&one, "b", None, Some("a")), vec![s("b"), s("a")]);
        // A folder moves whole and never into a folder.
        assert_eq!(
            move_to(&rail, "F", None, None),
            vec![s("a"), s("d"), f("F", &["b", "c"])]
        );
        assert_eq!(move_to(&rail, "F", Some("F"), None), rail);
        assert_eq!(move_to(&rail, "nope", None, None), rail);
    }

    #[test]
    fn a_drop_onto_a_server_makes_a_folder_and_onto_a_folders_server_joins_it() {
        let rail = vec![s("a"), s("b"), f("F", &["c"])];
        let merged = merge(&rail, "b", "a", "G");
        assert_eq!(
            merged,
            vec![
                RailItem::Folder {
                    folder_id: "G".into(),
                    name: String::new(),
                    tone: tone_of("a"),
                    server_ids: ids(&["a", "b"]),
                },
                f("F", &["c"]),
            ]
        );
        assert_eq!(
            merge(&rail, "a", "c", "G"),
            vec![s("b"), f("F", &["a", "c"])]
        );
        assert_eq!(merge(&rail, "a", "a", "G"), rail);
    }

    #[test]
    fn the_menus_edits() {
        let rail = vec![s("a"), f("F", &["b", "c"]), s("d")];
        assert_eq!(
            out_of_folder(&rail, "b"),
            vec![s("a"), f("F", &["c"]), s("b"), s("d")]
        );
        assert_eq!(
            out_of_folder(&[f("F", &["b"]), s("a")], "b"),
            vec![s("b"), s("a")]
        );
        assert_eq!(
            into_folder(&rail, "d", "F"),
            vec![s("a"), f("F", &["b", "c", "d"])]
        );
        assert_eq!(
            delete_folder(&rail, "F"),
            vec![s("a"), s("b"), s("c"), s("d")]
        );
        assert_eq!(
            step(&rail, "d", -1),
            vec![s("a"), s("d"), f("F", &["b", "c"])]
        );
        assert_eq!(step(&rail, "a", -1), rail);
        assert_eq!(
            step(&rail, "b", 1),
            vec![s("a"), f("F", &["c", "b"]), s("d")]
        );
        let wrapped = new_folder(&rail, "c", "G");
        assert_eq!(
            wrapped[2],
            RailItem::Folder {
                folder_id: "G".into(),
                name: String::new(),
                tone: tone_of("c"),
                server_ids: ids(&["c"]),
            }
        );
        let named = set_folder(&rail, "F", "  Work  ", "sky");
        assert!(
            matches!(&named[1], RailItem::Folder { name, tone, .. } if name == "Work" && tone == "sky")
        );
        let kept = set_folder(&rail, "F", &"x".repeat(60), "red");
        assert!(
            matches!(&kept[1], RailItem::Folder { name, tone, .. } if name.len() == NAME_MAX && tone == "mint")
        );
        assert_eq!(position_of(&rail, "d"), Some(3));
        assert_eq!(position_of(&rail, "c"), Some(2));
        assert_eq!(server_order(&rail), ids(&["a", "b", "c", "d"]));
    }

    #[test]
    fn a_folder_with_no_name_reads_as_its_servers_and_tones_match_the_shells() {
        assert_eq!(folder_words(&ids(&["Acme"])), "Acme");
        assert_eq!(folder_words(&ids(&["Acme", "Lumia"])), "Acme and Lumia");
        assert_eq!(folder_words(&ids(&["A", "B", "C"])), "A, B and C");
        assert_eq!(folder_words(&ids(&["A", "B", "C", "D"])), "A, B and 2 more");
        // The shells' hash: h * 31 + unit over UTF-16, mod 9.
        assert_eq!(tone_of(""), "peach");
        assert_eq!(tone_of("a"), TONES[97 % 9]);
    }

    #[test]
    fn it_crosses_the_wire_and_back() {
        let a = "018f0000-0000-7000-8000-000000000001";
        let b = "018f0000-0000-7000-8000-000000000002";
        let folder = "018f0000-0000-7000-8000-0000000000ff";
        let rail = vec![
            s(a),
            RailItem::Folder {
                folder_id: folder.into(),
                name: "Work".into(),
                tone: "sky".into(),
                server_ids: ids(&[b]),
            },
        ];
        assert_eq!(from_proto(&to_proto(&rail)), rail);
    }
}
