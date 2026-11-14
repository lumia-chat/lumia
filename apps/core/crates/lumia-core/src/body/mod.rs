//! A message body (protocol 14): its Markdown subset read into blocks, the same for every
//! client, and the size limit a draft is held to.
//!
//! [`parse`] is the grammar (protocol 16 asked for one implementation the clients share).
//! The desktop draws its [`Block`]s directly. The other clients draw [`display`]'s blocks:
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
/// English one.
pub fn room(body: &str, max: u32) -> Room {
    let max = max as usize;
    let bytes = body.len();
    let shown = bytes * 10 >= max * COUNTER_FROM_TENTHS;
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

/// Parses `src` for a client other than the desktop. `mentions` are the message's mention
/// ranges, in bytes of `src` as the protocol gives them; each comes out as a run of
/// [`DisplayStyle::Mention`], unless it is inside code, where it is code. A mention is read
/// as one unit, so the Markdown inside a name (`@a_b_`) is not Markdown. A range that is not
/// on character boundaries, overlaps another or runs past the end is not marked.
pub fn display(src: &str, mentions: &[Range<usize>]) -> Vec<DisplayBlock> {
    let marked = stand_ins(src, mentions);
    let blocks = match &marked {
        Some((source, _)) => parse(source),
        None => parse(src),
    };
    let typed = marked
        .as_ref()
        .map_or(&[][..], |(_, typed)| typed.as_slice());
    blocks
        .into_iter()
        .map(|block| display_block(block, typed))
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
        out.push_str(&src[at..range.start]);
        let Some(c) = char::from_u32(STAND_IN + index) else {
            continue;
        };
        out.push(c);
        typed.push((index, src[range.clone()].to_owned()));
        at = range.end;
    }
    if typed.is_empty() {
        return None;
    }
    out.push_str(&src[at..]);
    Some((out, typed))
}

/// What the stand-in `c` stood for, if it is one.
fn stood_for(c: char, typed: &[(u32, String)]) -> Option<(u32, &str)> {
    let index = u32::from(c).checked_sub(STAND_IN)?;
    typed
        .iter()
        .find(|(i, _)| *i == index)
        .map(|(i, text)| (*i, text.as_str()))
}

/// `text` with each stand-in put back as what it stood for: in code, and in a link's
/// address, a mention is the text it was typed as.
fn put_back(text: &str, typed: &[(u32, String)]) -> String {
    if typed.is_empty() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match stood_for(c, typed) {
            Some((_, words)) => out.push_str(words),
            None => out.push(c),
        }
    }
    out
}

fn display_block(block: Block, typed: &[(u32, String)]) -> DisplayBlock {
    match block {
        Block::Para(inline) => DisplayBlock::Para(display_inline(&inline, typed)),
        Block::Code { lang, text } => DisplayBlock::Code {
            lang,
            text: put_back(&text, typed),
        },
        Block::Quote(blocks) => DisplayBlock::Quote {
            blocks: blocks
                .into_iter()
                .map(|b| display_block(b, typed))
                .collect(),
        },
        Block::List { start, items } => DisplayBlock::List {
            start,
            items: items
                .into_iter()
                .map(|item| item.into_iter().map(|b| display_block(b, typed)).collect())
                .collect(),
        },
    }
}

/// An inline in UTF-16, without inline code's padding, its stand-ins put back.
fn display_inline(inline: &Inline, typed: &[(u32, String)]) -> DisplayInline {
    let src = &inline.text;
    // Where each byte boundary of the source lands in the output, in UTF-16 units; the
    // padding's bytes land where the padding would have started.
    let mut at = vec![0u32; src.len() + 1];
    let mut text = String::with_capacity(src.len());
    let mut units = 0u32;
    let mut mentions = Vec::new();
    let pads: Vec<Range<usize>> = inline
        .runs
        .iter()
        .filter(|(_, style)| *style == Style::Pad)
        .map(|(r, _)| r.clone())
        .collect();
    let in_code = |i: usize| {
        inline
            .runs
            .iter()
            .any(|(r, style)| *style == Style::Code && r.contains(&i))
    };
    for (i, c) in src.char_indices() {
        at[i] = units;
        if pads.iter().any(|r| r.contains(&i)) {
            continue;
        }
        let piece = match stood_for(c, typed) {
            Some((index, words)) => {
                let start = units;
                let len = utf16_len(words);
                if !in_code(i) {
                    mentions.push(DisplayRun {
                        start,
                        end: start + len,
                        style: DisplayStyle::Mention { index },
                    });
                }
                text.push_str(words);
                units += len;
                continue;
            }
            None => c,
        };
        text.push(piece);
        units += u32::try_from(piece.len_utf16()).unwrap_or(0);
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
    runs.extend(mentions);
    let links = inline
        .links
        .iter()
        .filter_map(|(r, url)| {
            let (start, end) = span(r)?;
            Some(DisplayLink {
                start,
                end,
                url: put_back(url, typed),
            })
        })
        .collect();
    DisplayInline { text, runs, links }
}

fn utf16_len(s: &str) -> u32 {
    u32::try_from(s.encode_utf16().count()).unwrap_or(u32::MAX)
}

/// Where `words` stand in `text` as a word of their own: not inside another word, and not
/// running on into the characters a handle has. The desktop finds a mention's words in
/// the paragraphs it draws this way.
pub fn standing(text: &str, words: &str) -> Vec<Range<usize>> {
    let handle = |c: char| c.is_alphanumeric() || matches!(c, '_' | '.' | '-');
    let mut found = Vec::new();
    let mut from = 0;
    if words.is_empty() {
        return found;
    }
    while let Some(at) = text[from..].find(words) {
        let start = from + at;
        let end = start + words.len();
        let opens = text[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let closes = text[end..]
            .chars()
            .next()
            .is_none_or(|c| !handle(c) || matches!(c, '.' | '-'));
        if opens && closes {
            found.push(start..end);
        }
        from = end;
    }
    found
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

    #[test]
    fn standing_finds_words_on_their_own() {
        assert_eq!(standing("@ada and @adam", "@ada"), vec![0..4]);
        assert_eq!(standing("@ada.", "@ada"), vec![0..4]);
        assert!(standing("x@ada", "@ada").is_empty());
        assert!(standing("anything", "").is_empty());
    }
}
