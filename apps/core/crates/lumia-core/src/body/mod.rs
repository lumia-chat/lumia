//! A message body (protocol 14): its Markdown subset read into blocks, the same for every
//! client, and the size limit a draft is held to.
//!
//! [`parse`] is the grammar (protocol 16 asked for one implementation the clients share), and
//! [`parse_with`] the same with a message's mentions marked by their ranges. The desktop
//! draws its [`Block`]s directly. The other clients draw [`display`]'s blocks:
//! the same, with ranges in UTF-16 code units (what Swift's `NSRange`, Kotlin's
//! `AnnotatedString` and JavaScript's strings count), inline code without the padding the
//! desktop draws around it, and the message's mentions marked as runs of their own.

mod parse;

use std::ops::Range;

use serde::Serialize;
use unicode_segmentation::UnicodeSegmentation;

pub use parse::{is_web, parse, Block, Inline, Style, CLOSE, OPEN, PAD};

/// The largest body an instance takes, in bytes of UTF-8, until its `Welcome` says
/// (`Limits.max_message_bytes`). A longer one is refused with `PAYLOAD_TOO_LARGE`.
pub const MAX_MESSAGE_BYTES: u32 = 16_384;

/// How full a draft may get before its counter shows, in tenths of the limit.
const COUNTER_FROM_TENTHS: usize = 9;

/// How much room a draft has left under a limit, as a composer's counter shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    /// Whether the counter shows: from 90% of the limit.
    pub shown: bool,
    /// Characters left, or when negative, how many too long. Characters are what a person
    /// sees as one (grapheme clusters), so a family emoji is one.
    pub left: i64,
}

impl Room {
    /// Whether the draft is over the limit and cannot be sent.
    pub fn over(self) -> bool {
        self.left < 0
    }
}

/// The room `body` has under `max` bytes. The limit is in bytes and people count
/// characters, so the two are reconciled here once: over the limit, it is exactly how many
/// characters from the end must go; under it, the bytes left at the draft's own bytes per
/// character, so a draft in Japanese counts down three bytes to a character and one in
/// English one. Below 90%, where no counter shows, `left` is the bytes left.
pub fn room(body: &str, max: u32) -> Room {
    let max = max as usize;
    let bytes = body.len();
    let shown = bytes * 10 >= max * COUNTER_FROM_TENTHS;
    // No counter shows below 90%: what is left then is never read, so it is the bytes left,
    // and a draft is not cut into characters on every key typed into it.
    if !shown {
        return Room {
            shown,
            left: i64::try_from(max - bytes).unwrap_or(i64::MAX),
        };
    }
    if bytes > max {
        let mut cut = 0;
        let mut kept = bytes;
        for (at, _) in body.grapheme_indices(true).rev() {
            cut += 1;
            kept = at;
            if kept <= max {
                break;
            }
        }
        debug_assert!(kept <= max);
        return Room { shown, left: -cut };
    }
    let characters = body.graphemes(true).count().max(1);
    let per = bytes.div_ceil(characters).max(1);
    Room {
        shown,
        left: i64::try_from((max - bytes) / per).unwrap_or(i64::MAX),
    }
}

/// Whether `body` fits under `max` bytes.
pub fn fits(body: &str, max: u32) -> bool {
    body.len() <= max as usize
}

/// A block of a body as the clients other than the desktop draw it ([`display`]).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DisplayBlock {
    Para(DisplayInline),
    /// A fenced code block: the first word after its fence, if any, and its lines.
    Code {
        lang: Option<String>,
        text: String,
    },
    Quote {
        blocks: Vec<DisplayBlock>,
    },
    /// A list, ordered from `start` when it has one, each item a run of blocks.
    List {
        start: Option<u64>,
        items: Vec<Vec<DisplayBlock>>,
    },
}

/// A paragraph's text and what styles it. Ranges are UTF-16 code units of `text`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInline {
    pub text: String,
    pub runs: Vec<DisplayRun>,
    /// Web links, which a client opens.
    pub links: Vec<DisplayLink>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayRun {
    pub start: u32,
    pub end: u32,
    pub style: DisplayStyle,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayLink {
    pub start: u32,
    pub end: u32,
    pub url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DisplayStyle {
    Emphasis,
    Strong,
    Strike,
    Code,
    /// A mention: `index` is its place in the `mentions` [`display`] was given. Its text
    /// is the mention as typed, which a client replaces with the person's name.
    Mention {
        index: u32,
    },
}

/// The first of the characters that stand in for mentions while the source is parsed:
/// Unicode's supplementary private use area B, which Markdown reads as plain text.
const STAND_IN: u32 = 0x10_0000;
/// How many mentions can stand in (the area's size, less its two noncharacters).
const STAND_INS: usize = 0xFFFE;

/// Parses `src` with the message's mentions: `mentions` are their ranges, in bytes of `src`
/// as the protocol gives them, and each comes out in its paragraph's [`Inline::mentions`]
/// with its index among them, unless it is inside code or a link, where it is that code's or
/// that address's text. A mention is read as one unit, so the Markdown inside a name
/// (`@_ada_`) is not Markdown. A range that is not on character boundaries, overlaps another
/// or runs past the end is not a mention.
pub fn parse_with(src: &str, mentions: &[Range<usize>]) -> Vec<Block> {
    let Some((source, typed)) = stand_ins(src, mentions) else {
        return parse(src);
    };
    parse(&source)
        .into_iter()
        .map(|block| put_back_block(block, &typed))
        .collect()
}

/// Parses `src` for a client other than the desktop: [`parse_with`]'s blocks with their
/// ranges in UTF-16, inline code without its padding, and each mention a run of
/// [`DisplayStyle::Mention`].
pub fn display(src: &str, mentions: &[Range<usize>]) -> Vec<DisplayBlock> {
    parse_with(src, mentions)
        .into_iter()
        .map(display_block)
        .collect()
}

/// `src` with each usable mention replaced by its stand-in, and what each stood for by its
/// index; `None` when nothing stands in.
fn stand_ins(src: &str, mentions: &[Range<usize>]) -> Option<(String, Vec<(u32, String)>)> {
    // A body that already holds stand-in characters would be misread: draw its mentions as
    // the text they are.
    if src.chars().any(|c| u32::from(c) >= STAND_IN) {
        return None;
    }
    let mut order: Vec<(u32, Range<usize>)> = mentions
        .iter()
        .enumerate()
        .take(STAND_INS)
        .filter(|(_, r)| {
            r.start < r.end && src.is_char_boundary(r.start) && src.is_char_boundary(r.end)
        })
        .filter_map(|(i, r)| Some((u32::try_from(i).ok()?, r.clone())))
        .collect();
    order.sort_by_key(|(_, r)| r.start);
    let mut out = String::with_capacity(src.len());
    let mut typed = Vec::new();
    let mut at = 0;
    for (index, range) in order {
        if range.start < at {
            continue;
        }
        let Some(c) = char::from_u32(STAND_IN + index) else {
            continue;
        };
        out.push_str(&src[at..range.start]);
        out.push(c);
        typed.push((index, src[range.clone()].to_owned()));
        at = range.end;
    }
    if typed.is_empty() {
        return None;
    }
    out.push_str(&src[at..]);
    typed.sort_by_key(|(index, _)| *index);
    Some((out, typed))
}

/// What the stand-in `c` stood for, if it is one. `typed` is sorted by index.
fn stood_for(c: char, typed: &[(u32, String)]) -> Option<(u32, &str)> {
    let index = u32::from(c).checked_sub(STAND_IN)?;
    let at = typed.binary_search_by_key(&index, |(i, _)| *i).ok()?;
    typed.get(at).map(|(i, text)| (*i, text.as_str()))
}

/// `text` with each stand-in put back as what it stood for: in code, and in a link's
/// address, a mention is the text it was typed as.
fn put_back(text: &str, typed: &[(u32, String)]) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match stood_for(c, typed) {
            Some((_, words)) => out.push_str(words),
            None => out.push(c),
        }
    }
    out
}

fn put_back_block(block: Block, typed: &[(u32, String)]) -> Block {
    match block {
        Block::Para(inline) => Block::Para(put_back_inline(&inline, typed)),
        Block::Code { lang, text } => Block::Code {
            lang,
            text: put_back(&text, typed),
        },
        Block::Quote(blocks) => Block::Quote(
            blocks
                .into_iter()
                .map(|b| put_back_block(b, typed))
                .collect(),
        ),
        Block::List { start, items } => Block::List {
            start,
            items: items
                .into_iter()
                .map(|item| item.into_iter().map(|b| put_back_block(b, typed)).collect())
                .collect(),
        },
    }
}

/// Marks the bytes `ranges` cover, in one pass over each range.
fn covered<'a>(len: usize, ranges: impl Iterator<Item = &'a Range<usize>>) -> Vec<bool> {
    let mut marks = vec![false; len];
    for r in ranges {
        if let Some(bytes) = marks.get_mut(r.start..r.end.min(len)) {
            bytes.fill(true);
        }
    }
    marks
}

/// A paragraph with its stand-ins put back: their words in the text, every range moved to
/// match, and each one outside code and links in [`Inline::mentions`].
fn put_back_inline(inline: &Inline, typed: &[(u32, String)]) -> Inline {
    let src = &inline.text;
    let len = src.len();
    let code = covered(
        len,
        inline
            .runs
            .iter()
            .filter(|(_, style)| matches!(style, Style::Code))
            .map(|(r, _)| r),
    );
    let linked = covered(len, inline.links.iter().map(|(r, _)| r));
    // Where each byte boundary of the paragraph lands in the new text.
    let mut at = vec![0usize; len + 1];
    let mut text = String::with_capacity(len);
    let mut mentions = Vec::new();
    for (i, c) in src.char_indices() {
        at[i] = text.len();
        match stood_for(c, typed) {
            Some((index, words)) => {
                let start = text.len();
                text.push_str(words);
                let quoted = code.get(i).copied().unwrap_or(false)
                    || linked.get(i).copied().unwrap_or(false);
                if !quoted {
                    mentions.push((start..text.len(), index));
                }
            }
            None => text.push(c),
        }
    }
    at[len] = text.len();
    let moved = |r: &Range<usize>| {
        at.get(r.start).copied().unwrap_or(0)..at.get(r.end).copied().unwrap_or(0)
    };
    Inline {
        runs: inline
            .runs
            .iter()
            .map(|(r, style)| (moved(r), *style))
            .collect(),
        links: inline
            .links
            .iter()
            .map(|(r, url)| (moved(r), put_back(url, typed)))
            .collect(),
        mentions,
        text,
    }
}

fn display_block(block: Block) -> DisplayBlock {
    match block {
        Block::Para(inline) => DisplayBlock::Para(display_inline(&inline)),
        Block::Code { lang, text } => DisplayBlock::Code { lang, text },
        Block::Quote(blocks) => DisplayBlock::Quote {
            blocks: blocks.into_iter().map(display_block).collect(),
        },
        Block::List { start, items } => DisplayBlock::List {
            start,
            items: items
                .into_iter()
                .map(|item| item.into_iter().map(display_block).collect())
                .collect(),
        },
    }
}

/// An inline in UTF-16, without inline code's padding.
fn display_inline(inline: &Inline) -> DisplayInline {
    let src = &inline.text;
    let pad = covered(
        src.len(),
        inline
            .runs
            .iter()
            .filter(|(_, style)| matches!(style, Style::Pad))
            .map(|(r, _)| r),
    );
    // Where each byte boundary of the source lands in the output, in UTF-16 units; the
    // padding's bytes land where the padding would have started.
    let mut at = vec![0u32; src.len() + 1];
    let mut text = String::with_capacity(src.len());
    let mut units = 0u32;
    for (i, c) in src.char_indices() {
        at[i] = units;
        if pad.get(i).copied().unwrap_or(false) {
            continue;
        }
        text.push(c);
        units += u32::try_from(c.len_utf16()).unwrap_or(0);
    }
    at[src.len()] = units;
    let span = |r: &Range<usize>| -> Option<(u32, u32)> {
        let (start, end) = (*at.get(r.start)?, *at.get(r.end)?);
        (start < end).then_some((start, end))
    };
    let mut runs: Vec<DisplayRun> = inline
        .runs
        .iter()
        .filter_map(|(r, style)| {
            let style = match style {
                Style::Emphasis => DisplayStyle::Emphasis,
                Style::Strong => DisplayStyle::Strong,
                Style::Strike => DisplayStyle::Strike,
                Style::Code => DisplayStyle::Code,
                Style::Pad => return None,
            };
            let (start, end) = span(r)?;
            Some(DisplayRun { start, end, style })
        })
        .collect();
    runs.extend(inline.mentions.iter().filter_map(|(r, index)| {
        let (start, end) = span(r)?;
        Some(DisplayRun {
            start,
            end,
            style: DisplayStyle::Mention { index: *index },
        })
    }));
    let links = inline
        .links
        .iter()
        .filter_map(|(r, url)| {
            let (start, end) = span(r)?;
            Some(DisplayLink {
                start,
                end,
                url: url.clone(),
            })
        })
        .collect();
    DisplayInline { text, runs, links }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn para(blocks: &[DisplayBlock]) -> &DisplayInline {
        match blocks.first() {
            Some(DisplayBlock::Para(inline)) => inline,
            other => panic!("not a paragraph: {other:?}"),
        }
    }

    #[test]
    fn room_counts_characters_left_at_the_drafts_own_bytes_per_character() {
        assert_eq!(
            room("", 100),
            Room {
                shown: false,
                left: 100
            }
        );
        assert_eq!(
            room(&"a".repeat(89), 100),
            Room {
                shown: false,
                left: 11
            }
        );
        assert_eq!(
            room(&"a".repeat(90), 100),
            Room {
                shown: true,
                left: 10
            }
        );
        assert_eq!(
            room(&"a".repeat(100), 100),
            Room {
                shown: true,
                left: 0
            }
        );
        // Three bytes a character: 30 characters use 90 bytes, 10 bytes are 3 characters.
        assert_eq!(
            room(&"日".repeat(30), 100),
            Room {
                shown: true,
                left: 3
            }
        );
    }

    #[test]
    fn room_over_the_limit_is_exactly_the_characters_to_cut() {
        assert_eq!(
            room(&"a".repeat(101), 100),
            Room {
                shown: true,
                left: -1
            }
        );
        assert!(room(&"a".repeat(101), 100).over());
        // 34 × 3 = 102 bytes: one character must go, and that leaves 99.
        assert_eq!(room(&"日".repeat(34), 100).left, -1);
        // A family emoji is one character of 18 bytes.
        let family = "👨‍👩‍👧";
        assert_eq!(family.len(), 18);
        assert_eq!(room(&format!("{}{family}", "a".repeat(90)), 100).left, -1);
        assert!(fits(&"a".repeat(100), 100));
        assert!(!fits(&"a".repeat(101), 100));
    }

    #[test]
    fn display_ranges_are_utf16_and_inline_code_has_no_padding() {
        let blocks = display("😀 **bold** `x`", &[]);
        let inline = para(&blocks);
        assert_eq!(inline.text, "😀 bold x");
        assert_eq!(
            inline.runs,
            vec![
                DisplayRun {
                    start: 3,
                    end: 7,
                    style: DisplayStyle::Strong
                },
                DisplayRun {
                    start: 8,
                    end: 9,
                    style: DisplayStyle::Code
                },
            ]
        );
    }

    #[test]
    fn display_links_are_in_utf16() {
        let blocks = display("é [here](https://lumia.chat)", &[]);
        assert_eq!(
            para(&blocks).links,
            vec![DisplayLink {
                start: 2,
                end: 6,
                url: "https://lumia.chat".into()
            }]
        );
    }

    #[test]
    fn mentions_are_runs_of_their_own_and_their_markdown_is_text() {
        let src = "hi @a_b_ and **@ada**";
        let blocks = display(src, &[3..8, 15..19]);
        let inline = para(&blocks);
        assert_eq!(inline.text, "hi @a_b_ and @ada");
        assert!(inline.runs.contains(&DisplayRun {
            start: 3,
            end: 8,
            style: DisplayStyle::Mention { index: 0 }
        }));
        assert!(inline.runs.contains(&DisplayRun {
            start: 13,
            end: 17,
            style: DisplayStyle::Mention { index: 1 }
        }));
        assert!(inline.runs.contains(&DisplayRun {
            start: 13,
            end: 17,
            style: DisplayStyle::Strong
        }));
    }

    #[test]
    fn a_mention_in_code_is_code() {
        let blocks = display("`@ada` and\n```\n@bo\n```", &[1..5, 15..18]);
        let inline = para(&blocks);
        assert_eq!(inline.text, "@ada and");
        assert!(!inline
            .runs
            .iter()
            .any(|r| matches!(r.style, DisplayStyle::Mention { .. })));
        assert_eq!(
            blocks.get(1),
            Some(&DisplayBlock::Code {
                lang: None,
                text: "@bo".into()
            })
        );
    }

    #[test]
    fn a_mention_inside_a_web_address_is_part_of_its_address() {
        let src = "see https://lumia.chat/@ada now";
        let blocks = display(src, &[src.find('@').map_or(0..0, |at| at..at + 4)]);
        let inline = para(&blocks);
        assert_eq!(inline.text, src);
        assert_eq!(
            inline.links.first().map(|l| l.url.as_str()),
            Some("https://lumia.chat/@ada")
        );
        // Its words are the address's, not a pill inside the link.
        assert!(
            !inline
                .runs
                .iter()
                .any(|r| matches!(r.style, DisplayStyle::Mention { .. })),
            "{:?}",
            inline.runs
        );
    }

    #[test]
    fn parse_with_marks_mentions_by_their_bytes_for_the_desktop() {
        let src = "**@_ada_** and `@bo` and @é";
        let blocks = parse_with(src, &[2..8, 16..19, 25..28]);
        let Some(Block::Para(inline)) = blocks.first() else {
            panic!("{blocks:?}");
        };
        // The name's underscores are not emphasis, and the mention in code is code.
        assert!(inline.text.starts_with("@_ada_ and "), "{:?}", inline.text);
        let marked: Vec<&str> = inline
            .mentions
            .iter()
            .map(|(r, _)| &inline.text[r.clone()])
            .collect();
        assert_eq!(marked, ["@_ada_", "@é"]);
        assert_eq!(
            inline.mentions.iter().map(|(_, i)| *i).collect::<Vec<_>>(),
            [0, 2]
        );
        assert!(inline
            .runs
            .iter()
            .any(|(r, style)| *style == Style::Strong && &inline.text[r.clone()] == "@_ada_"));
    }

    #[test]
    fn many_inline_code_spans_parse_in_a_moment() {
        let src = "`x` ".repeat(4_000);
        let started = std::time::Instant::now();
        let blocks = display(&src, &[]);
        assert!(!blocks.is_empty());
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn below_ninety_percent_the_room_is_the_bytes_left() {
        assert_eq!(
            room(&"日".repeat(10), 100),
            Room {
                shown: false,
                left: 70
            }
        );
    }

    #[test]
    fn unusable_mention_ranges_are_left_as_text() {
        // Off a character boundary, empty, past the end, and overlapping the first.
        let blocks = display("é @ada", &[1..2, 3..3, 3..99, 3..7, 4..6]);
        let inline = para(&blocks);
        assert_eq!(inline.text, "é @ada");
        let mentions: Vec<_> = inline
            .runs
            .iter()
            .filter(|r| matches!(r.style, DisplayStyle::Mention { .. }))
            .collect();
        assert_eq!(
            mentions,
            vec![&DisplayRun {
                start: 2,
                end: 6,
                style: DisplayStyle::Mention { index: 3 }
            }]
        );
    }

    #[test]
    fn a_body_holding_stand_in_characters_marks_no_mentions() {
        let src = "\u{100001} @ada";
        let blocks = display(src, std::slice::from_ref(&(5..9)));
        assert_eq!(para(&blocks).text, src);
        assert!(para(&blocks).runs.is_empty());
    }

    #[test]
    fn quotes_lists_and_code_come_through() {
        let blocks = display("> quoted\n\n- one\n- two\n\n```rust\nfn x() {}\n```", &[]);
        assert!(matches!(blocks.first(), Some(DisplayBlock::Quote { .. })));
        assert!(matches!(
            blocks.get(1),
            Some(DisplayBlock::List { start: None, items }) if items.len() == 2
        ));
        assert_eq!(
            blocks.get(2),
            Some(&DisplayBlock::Code {
                lang: Some("rust".into()),
                text: "fn x() {}".into()
            })
        );
    }
}
