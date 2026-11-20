//! A server's custom emoji (specs/emoji.md): what a name may be, where a body uses one,
//! which a typed `:` prefix matches, and a name made from a picture's file name. The same
//! for every client, so no shell matches `:name:` itself.

use std::ops::Range;

use crate::body::{self, Block};

/// The shortest and longest a name may be (specs/emoji.md 2).
pub const NAME_MIN: usize = 2;
pub const NAME_MAX: usize = 32;
/// The most a message's body names (specs/emoji.md 6); the instance drops the rest.
pub const REFS_MAX: usize = 200;
/// The most [`matches`] answers.
pub const MATCHES_MAX: usize = 25;

/// Whether `name` is a name as kept: 2 to 32 of a-z, 0-9 and `_`.
pub fn is_name(name: &str) -> bool {
    (NAME_MIN..=NAME_MAX).contains(&name.len()) && name.bytes().all(name_byte)
}

fn name_byte(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'
}

/// Where `body` uses one of `names` (a server's live emoji, each a name and what it stands
/// for): each `:name:` that names one, outside inline code, code blocks and links, read
/// left to right without overlapping, at most [`REFS_MAX`]. Ranges are bytes of `body`,
/// as the protocol keeps them.
pub fn find_refs<T: Clone>(body: &str, names: &[(String, T)]) -> Vec<(Range<usize>, T)> {
    let candidates = candidates(body, names);
    if candidates.is_empty() {
        return Vec::new();
    }
    let ranges: Vec<Range<usize>> = candidates.iter().map(|(r, _)| r.clone()).collect();
    // Read as the clients draw it: what comes out as a span is outside code and links.
    let mut kept = vec![false; candidates.len()];
    for block in body::parse_marked(body, &[], &ranges) {
        mark(&block, &mut kept);
    }
    candidates
        .into_iter()
        .zip(kept)
        .filter_map(|(c, k)| k.then_some(c))
        .take(REFS_MAX)
        .collect()
}

fn mark(block: &Block, kept: &mut [bool]) {
    match block {
        Block::Para(inline) => {
            for (_, index) in &inline.emoji {
                if let Some(k) = kept.get_mut(*index as usize) {
                    *k = true;
                }
            }
        }
        Block::Code { .. } => {}
        Block::Quote(blocks) => blocks.iter().for_each(|b| mark(b, kept)),
        Block::List { items, .. } => items.iter().flatten().for_each(|b| mark(b, kept)),
    }
}

/// Every `:name:` of `body` that names one of `names`, left to right, none overlapping.
fn candidates<T: Clone>(body: &str, names: &[(String, T)]) -> Vec<(Range<usize>, T)> {
    let bytes = body.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(offset) = bytes
        .get(at..)
        .and_then(|rest| rest.iter().position(|&b| b == b':'))
    {
        let open = at + offset;
        let name_start = open + 1;
        let name_len = bytes
            .get(name_start..)
            .map_or(0, |rest| rest.iter().take_while(|&&b| name_byte(b)).count());
        let close = name_start + name_len;
        if bytes.get(close) == Some(&b':') && (NAME_MIN..=NAME_MAX).contains(&name_len) {
            let name = &body[name_start..close];
            if let Some((_, value)) = names.iter().find(|(n, _)| n == name) {
                found.push((open..close + 1, value.clone()));
                at = close + 1;
                continue;
            }
        }
        // Not one: the colon that ended the attempt may open the next.
        at = if close > name_start && bytes.get(close) == Some(&b':') {
            close
        } else {
            open + 1
        };
    }
    found
}

/// The names of `names` that a typed `prefix` (what follows a `:` in the composer) matches:
/// those that begin with it, then those that hold it elsewhere, each group in name order,
/// at most [`MATCHES_MAX`]. Nothing for a prefix shorter than two (specs/emoji.md 10).
/// Capitals typed match, since names are lower case.
pub fn matches<'a>(prefix: &str, names: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let prefix = prefix.to_ascii_lowercase();
    if prefix.len() < NAME_MIN || !prefix.bytes().all(name_byte) {
        return Vec::new();
    }
    let (mut starting, mut holding): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
    for name in names {
        if name.starts_with(&prefix) {
            starting.push(name);
        } else if name.contains(&prefix) {
            holding.push(name);
        }
    }
    starting.sort_unstable();
    holding.sort_unstable();
    starting
        .into_iter()
        .chain(holding)
        .take(MATCHES_MAX)
        .collect()
}

/// A name made from a picture's file name, for the upload form to start from
/// (specs/emoji.md 10): the name without its extension, lower case, every run of what a
/// name may not hold made one `_`, `_` trimmed from both ends, cut to 32. `emoji` when
/// fewer than two characters are left.
pub fn name_from_file(file_name: &str) -> String {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let stem = match base.rfind('.') {
        Some(dot) if dot > 0 => &base[..dot],
        _ => base,
    };
    let mut name = String::with_capacity(stem.len());
    for c in stem.chars().flat_map(char::to_lowercase) {
        if c.is_ascii() && name_byte(c as u8) && c != '_' {
            name.push(c);
        } else if !name.ends_with('_') {
            name.push('_');
        }
    }
    let name = name.trim_matches('_');
    let name: String = name.chars().take(NAME_MAX).collect();
    let name = name.trim_end_matches('_').to_owned();
    if name.len() < NAME_MIN {
        "emoji".to_owned()
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<(String, u8)> {
        vec![
            ("parrot".into(), 1),
            ("party_parrot".into(), 2),
            ("ok".into(), 3),
        ]
    }

    fn found(body: &str) -> Vec<(&str, u8)> {
        find_refs(body, &names())
            .into_iter()
            .map(|(r, v)| (&body[r], v))
            .collect()
    }

    #[test]
    fn names_are_two_to_thirty_two_of_lower_case_letters_digits_and_underscores() {
        assert!(is_name("ok"));
        assert!(is_name("party_parrot_2"));
        assert!(is_name(&"a".repeat(32)));
        for bad in [
            "a",
            "",
            "Parrot",
            "has space",
            "dash-ed",
            "émoji",
            &"a".repeat(33),
        ] {
            assert!(!is_name(bad), "{bad}");
        }
    }

    #[test]
    fn refs_are_the_names_held_outside_code_with_their_byte_ranges() {
        assert_eq!(found("hi :parrot: there"), vec![(":parrot:", 1)]);
        assert_eq!(
            found(":party_parrot::ok:"),
            vec![(":party_parrot:", 2), (":ok:", 3)]
        );
        // After text in other scripts, the ranges are bytes.
        let body = "日本 :ok:";
        let refs = find_refs(body, &names());
        assert_eq!(refs[0].0, 7..11);
        assert_eq!(found("`:parrot:` and :ok:"), vec![(":ok:", 3)]);
        assert_eq!(found("```\n:parrot:\n```"), vec![]);
        assert_eq!(
            found("> quoted :parrot:\n\n- listed :ok:"),
            vec![(":parrot:", 1), (":ok:", 3)]
        );
        assert_eq!(
            found("[:parrot:](https://example.com) :ok:"),
            vec![(":ok:", 3)]
        );
        // Unknown, upper case, or too short: text.
        assert_eq!(found(":nope: :Parrot: :a:"), vec![]);
        // The colon that ends a miss may open the next.
        assert_eq!(found(":nope:ok:"), vec![(":ok:", 3)]);
        assert_eq!(found("time 10:30:parrot:"), vec![(":parrot:", 1)]);
    }

    #[test]
    fn underscores_in_a_name_are_not_read_as_emphasis() {
        let body = "_:party_parrot:_ wow";
        let emoji = std::slice::from_ref(&(1..15)).to_vec();
        let blocks = body::parse_marked(body, &[], &emoji);
        let Block::Para(inline) = &blocks[0] else {
            panic!("a paragraph");
        };
        assert_eq!(inline.emoji.len(), 1);
        let (range, index) = &inline.emoji[0];
        assert_eq!((&inline.text[range.clone()], *index), (":party_parrot:", 0));
    }

    #[test]
    fn at_most_two_hundred_refs() {
        let body = ":ok:".repeat(250);
        assert_eq!(find_refs(&body, &names()).len(), REFS_MAX);
    }

    #[test]
    fn mentions_and_emoji_keep_their_own_indices() {
        let body = "@ada :ok: @grace";
        let emoji = std::slice::from_ref(&(5..9)).to_vec();
        let blocks = body::display_marked(body, &[0..4, 10..16], &emoji);
        let body::DisplayBlock::Para(inline) = &blocks[0] else {
            panic!("a paragraph");
        };
        let styles: Vec<_> = inline.runs.iter().map(|r| r.style).collect();
        assert!(styles.contains(&body::DisplayStyle::Mention { index: 0 }));
        assert!(styles.contains(&body::DisplayStyle::Mention { index: 1 }));
        assert!(styles.contains(&body::DisplayStyle::Emoji { index: 0 }));
    }

    #[test]
    fn matches_begin_with_the_prefix_first_then_hold_it() {
        let all = ["sad_parrot", "parrot", "party_parrot", "cat", "parrots_hat"];
        assert_eq!(
            matches("par", all),
            vec!["parrot", "parrots_hat", "party_parrot", "sad_parrot"]
        );
        assert_eq!(matches("PAR", all)[0], "parrot");
        assert!(matches("p", all).is_empty());
        assert!(matches("p r", all).is_empty());
        let many: Vec<String> = (0..40).map(|i| format!("cat{i:02}")).collect();
        assert_eq!(
            matches("cat", many.iter().map(String::as_str)).len(),
            MATCHES_MAX
        );
    }

    #[test]
    fn a_name_from_a_file_name() {
        assert_eq!(name_from_file("Party Parrot.gif"), "party_parrot");
        assert_eq!(name_from_file("/Users/ada/Pictures/lgtm!!.png"), "lgtm");
        assert_eq!(name_from_file("ship--it  (2).webp"), "ship_it_2");
        assert_eq!(name_from_file("café.png"), "caf");
        assert_eq!(name_from_file("日本.png"), "emoji");
        assert_eq!(name_from_file("a.png"), "emoji");
        assert_eq!(name_from_file(".hidden"), "hidden");
        let long = format!("{}_tail.png", "x".repeat(31));
        assert_eq!(name_from_file(&long), "x".repeat(31));
    }
}
