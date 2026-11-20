//! A message body: the protocol's Markdown subset (protocol 14) drawn as Rune rich text,
//! every text of it selectable and in the selection group of whatever lists it. That is
//! the reason this is the shell's own renderer and not the kit's `Markdown`: the kit's is a
//! selection group of its own, so a drag stopped at the end of the message it began in.
//! Here the timeline is the group, and a drag runs from one message into the next.
//!
//! The core's `parse` (`lumia_client::body`, shared with the other clients) reads the source
//! into blocks and decides what is in the subset; this file draws them. A paragraph is one
//! `rich` text; a quote is its blocks beside a rule; a list is rows of a marker and the
//! item's blocks. A word wider than the line breaks inside itself rather than run out of the
//! row. A code block is one text on the `inset` ground, coloured by the kit's highlighter for
//! the languages it knows, whose lines never wrap: the block scrolls sideways inside itself
//! (the wheel with Shift, or a trackpad; there is no bar, and the arrow keys are the text's),
//! and is as tall as its lines. While the pointer is over it, or the keyboard is in it, a
//! "Copy" button in its corner puts its text on the clipboard and reads "Copied" for a moment
//! (`State::copy_code`, which says so to a screen reader too).
//!
//! A body more than [`FOLD_OVER`] lines tall, as drawn at the width it is given ([`Fold`]),
//! is drawn [`FOLDED_LINES`] tall, its last line fading into whatever is behind the row (the
//! mention wash too), with "Show more" under it; "Show less" folds it again. Whether it is
//! open is the open conversation's (`Session::unfolded`). Only the timeline folds a body.
//!
//! What a copy takes, since a selection can now span messages (the timeline adds each
//! message's author and time above its body):
//! - A blank line between paragraphs and blocks, one newline between the items of a list.
//! - A list item after its marker (`- `, `1. `), indented under the item it is nested in,
//!   and each line of a quote after `> `. The markers are not selectable text: Rune writes
//!   them before a text only when the copy takes that text from its start, with others. So
//!   a quoted paragraph is a text per typed line, and a code block inside a quote, being
//!   one text, is marked on its first line only.
//! - A code block as its text, without its fence. Inline code without the spacing drawn
//!   around it.
//! - A link as its text, not its address. (The address shows under the pointer, since a
//!   link's text need not say where it goes.)
//! - A folded body whole: what the fold hides is still its text.
//!
//! A mention (architecture 6.6) is drawn as the design system's pill: the core reads the
//! source with the message's mention ranges (`parse_with`) and gives each paragraph the
//! ranges of its mentions, none inside code or a link.
//!
//! A server's custom emoji (specs/emoji.md 6) is read by the core the same way
//! (`parse_marked`), and drawn as its picture when this computer holds the emoji, at the
//! height of an emoji in that text ([`EMOJI_HEIGHT`] of its size, the boards' 1.3 em) and as
//! wide as its shape. Rune sets no picture in a line of text for an app, but it does set an
//! icon before a word (`El::inline_icon`, in room of its own that moves the text after it
//! aside, centred on that word's letters), and that is what draws one here: the `:name:` the
//! message keeps stays in the text, so a selection and a copy take it, but drawn at no width
//! and in no colour, and the picture goes before it. Two characters that are never copied
//! frame it ([`make_room`]): a word joiner before it, drawn at the text's own size, whose
//! letters the picture is centred on (the name's own are too small to centre on), and a
//! no-break space after it, which ends the word, so that `:a::b:` is two pictures, and binds
//! it to what follows as the typed text was bound. What this costs, and what Rune does not
//! do: the picture is still (Rune draws an inline icon's first frame, never its others: its
//! painter places one with no clock), a selection's highlight does not cover it, and it has
//! no tooltip (a text range has one only as a link). Assistive technology reads the
//! paragraph with each emoji's present `:name:`. One the computer does not hold (removed, or
//! not yet known) is its text as typed.

use std::cell::Cell;
use std::ops::Range;

use rune::text::RichLayoutOptions;
use rune::{
    Align, Color, Cursor, El, InlineIcon, Link, LinkPaint, Pixels, Span, TextStyle, Tx, col, el,
    rich, row, text,
};
use rune_kit::syntax::{self, SyntaxTheme};

use lumia_client::body::PAD;
pub use lumia_client::body::{Block, Inline, Style, is_web, parse, parse_marked, parse_with};

use super::components::Look;
use crate::app::State;
use crate::tokens::{self as t};

thread_local! {
    /// Token colours for light and dark, built once.
    static SYNTAX: [SyntaxTheme; 2] = [t::syntax(false), t::syntax(true)];
    /// The size that sets inline code's padding, and the fonts it was measured in.
    static PAD_SIZE: Cell<(u64, f32)> = const { Cell::new((u64::MAX, 0.0)) };
}

/// The body of a message. `selectable` is off for a send that is still pending: its text
/// cannot be selected and its links do not open.
pub fn view(source: &str, selectable: bool, look: Look) -> El {
    view_with(
        source,
        selectable,
        &Marks::default(),
        Drawn::default(),
        look,
    )
}

/// What of a body is marked beside its source: its mentions, drawn as pills, and its
/// server's custom emoji, each a byte range of the source.
#[derive(Clone, Default)]
pub struct Marks {
    pub mentions: Vec<Range<usize>>,
    pub emoji: Vec<BodyEmoji>,
}

/// One of the server's custom emoji a body uses (specs/emoji.md 6): the bytes of the source
/// its `:name:` takes, and its picture when this computer holds the emoji.
#[derive(Clone, Debug)]
pub struct BodyEmoji {
    pub range: Range<usize>,
    /// None draws the text as typed: the emoji was removed, or is not known here.
    pub picture: Option<EmojiPicture>,
}

/// A held emoji's picture, as a body draws it.
#[derive(Clone, Debug)]
pub struct EmojiPicture {
    /// Empty until it has decoded: its room is kept meanwhile, so the line does not move.
    pub pixels: Pixels,
    /// Its width over its height.
    pub aspect: f32,
    /// Its name now, which assistive technology reads.
    pub name: String,
}

/// An emoji's height in text, over the text's size: the boards' 1.3 em.
pub const EMOJI_HEIGHT: f32 = 1.3;

/// What frames a pictured emoji's `:name:` ([`make_room`]): a word joiner before it, and a
/// no-break space after it.
const JOIN: &str = "\u{2060}";
const BIND: &str = "\u{a0}";
/// The size the `:name:` under a picture is set at: no width a reader could see.
const UNSEEN: f32 = 0.05;

/// Where [`make_room`] put one pictured emoji in the text it made.
struct Placed<'a> {
    /// The word joiner's byte, where the picture goes.
    join: usize,
    /// The `:name:`.
    name: Range<usize>,
    /// The no-break space after it.
    bind: Range<usize>,
    picture: &'a EmojiPicture,
}

/// `inline` with each pictured emoji's `:name:` framed by [`JOIN`] and [`BIND`], every range
/// moved to match, and where each went. `pictures` are ranges of `inline.text`, in order.
fn make_room<'a>(
    inline: &Inline,
    pictures: &[(Range<usize>, &'a EmojiPicture)],
) -> (Inline, Vec<Placed<'a>>) {
    let mut inserts: Vec<(usize, &str)> = Vec::with_capacity(pictures.len() * 2);
    for (range, _) in pictures {
        inserts.push((range.start, JOIN));
        inserts.push((range.end, BIND));
    }
    // A range's start comes after what is put in where it starts; its end before what is
    // put in where it ends.
    let starts = |pos: usize| -> usize {
        pos + inserts
            .iter()
            .filter(|(at, _)| *at <= pos)
            .map(|(_, s)| s.len())
            .sum::<usize>()
    };
    let ends = |pos: usize| -> usize {
        pos + inserts
            .iter()
            .filter(|(at, _)| *at < pos)
            .map(|(_, s)| s.len())
            .sum::<usize>()
    };
    let moved = |r: &Range<usize>| starts(r.start)..ends(r.end).max(starts(r.start));
    let mut text = String::with_capacity(inline.text.len() + inserts.len() * 3);
    let mut at = 0;
    for (pos, put) in &inserts {
        text.push_str(inline.text.get(at..*pos).unwrap_or_default());
        text.push_str(put);
        at = *pos;
    }
    text.push_str(inline.text.get(at..).unwrap_or_default());
    let placed = pictures
        .iter()
        .map(|(range, picture)| {
            let name = starts(range.start)..ends(range.end);
            Placed {
                join: name.start - JOIN.len(),
                bind: name.end..name.end + BIND.len(),
                name,
                picture,
            }
        })
        .collect();
    let made = Inline {
        text,
        runs: inline.runs.iter().map(|(r, s)| (moved(r), *s)).collect(),
        links: inline
            .links
            .iter()
            .map(|(r, url)| (moved(r), url.clone()))
            .collect(),
        bracketed: inline.bracketed.iter().map(moved).collect(),
        mentions: inline
            .mentions
            .iter()
            .map(|(r, i)| (moved(r), *i))
            .collect(),
        emoji: inline.emoji.iter().map(|(r, i)| (moved(r), *i)).collect(),
    };
    (made, placed)
}

/// How a body is drawn besides its source.
#[derive(Clone, Copy)]
pub struct Drawn<'a> {
    /// The type at the message size chosen in Preferences.
    pub ty: &'a t::MessageType,
    /// What a copy writes before the body's first text when the selection takes that from
    /// its start with others: the author and time of a row that draws neither as text a
    /// selection takes (one line, `timeline`).
    pub head: Option<&'a str>,
    /// The message was edited: "(edited)" follows its last paragraph's words, in the small
    /// muted type and left out of a copy ([`edited_inline`] says whether it can).
    pub edited: bool,
    /// The message, for what its buttons act on: each code block's "Copy", and "Show more".
    /// None draws neither, for a body inside something pressed as a whole (a Mentions item),
    /// where a button would be a control inside a control.
    pub id: Option<&'a str>,
    /// How a body too tall to show whole is folded; none shows every body whole.
    pub fold: Option<Fold>,
    /// Which of its code blocks has just been copied, counting from its first: that one's
    /// button reads "Copied".
    pub copied: Option<usize>,
}

/// How [`Body::height`] takes a paragraph's lines: set as Rune will draw them, or the most
/// its characters could take, which costs no text setting.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Wraps {
    Set,
    Most,
}

/// What decides whether a body is folded: the width it is laid out at, which sets how many
/// lines it takes, and whether the reader has asked to see it whole.
#[derive(Clone, Copy)]
pub struct Fold {
    pub width: f32,
    pub open: bool,
}

/// A body drawn taller than this many lines of its type is folded until it is opened.
pub const FOLD_OVER: usize = 14;
/// How many lines a folded body shows.
pub const FOLDED_LINES: usize = 10;

/// How many lines a body `lines` tall shows while folded: [`FOLDED_LINES`] when it is taller
/// than [`FOLD_OVER`], none when it shows whole anyway.
pub fn folded_lines(lines: usize) -> Option<usize> {
    (lines > FOLD_OVER).then_some(FOLDED_LINES)
}

impl Default for Drawn<'_> {
    fn default() -> Self {
        Self {
            ty: &t::MESSAGE_TYPE,
            head: None,
            edited: false,
            id: None,
            fold: None,
            copied: None,
        }
    }
}

/// [`view`] with the message's mentions, drawn as pills, and its server's custom emoji,
/// drawn as their pictures where held.
pub fn view_with(
    source: &str,
    selectable: bool,
    marks: &Marks,
    drawn: Drawn<'_>,
    look: Look,
) -> El {
    let ranges: Vec<Range<usize>> = marks.emoji.iter().map(|e| e.range.clone()).collect();
    let blocks = parse_marked(source, &marks.mentions, &ranges);
    let body = Body {
        look,
        ty: *drawn.ty,
        emoji: marks.emoji.clone(),
        selectable,
        first: Cell::new(true),
        head: Cell::new(drawn.head.map(str::to_owned)),
        edited: Cell::new(false),
        id: drawn.id.map(str::to_owned),
        copied: drawn.copied,
        codes: Cell::new(0),
    };
    let edited = drawn.edited && ends_in_paragraph(&blocks);
    let whole = body.blocks_marked(&blocks, edited);
    let (Some(fold), Some(id)) = (drawn.fold, drawn.id) else {
        return whole;
    };
    match body.lines_at(&blocks, fold.width).and_then(folded_lines) {
        Some(shown) => body.folded(whole, shown, fold.open, id),
        None => whole,
    }
}

/// Whether "(edited)" can follow a body's words on its last line: it ends in a paragraph,
/// not in code, a quote or a list, after which it stands on a line of its own.
pub fn edited_inline(source: &str, mentions: &[Range<usize>]) -> bool {
    ends_in_paragraph(&parse_with(source, mentions))
}

/// Whether a body draws a box of its own (a code block), which in one line needs room from
/// the rows beside it: their spacing there can be none.
pub fn boxed(source: &str) -> bool {
    parse(source)
        .iter()
        .any(|block| matches!(block, Block::Code { .. }))
}

fn ends_in_paragraph(blocks: &[Block]) -> bool {
    matches!(blocks.last(), Some(Block::Para(_)))
}

/// What follows an edited message's words.
const EDITED: &str = " (edited)";

/// A line of the timeline that is not a message's text but belongs in a copy of it: the
/// placeholder of a deleted message. Selectable, and placed as a body's only paragraph is,
/// with [`Drawn::head`] before it in a copy as a body's first text has it.
pub fn note(content: &str, color: Color, drawn: Drawn<'_>, look: Look) -> El {
    let note = text(content)
        .font(drawn.ty.message)
        .color(color)
        .selectable(true)
        .selection_color(look.c.accent_soft);
    match drawn.head {
        Some(head) => note.copy_breaks(2, 2).copy_prefix(format!("{head}\n")),
        None => note.copy_breaks(1, 2),
    }
}

/// Opens a link from a message in the browser. Only web links: a message is text someone
/// else wrote, and other schemes can start other programs. The parser makes links of
/// nothing else; this is the same rule where the press lands.
fn open_link(_: &mut State, url: &str, tx: &mut Tx) {
    if is_web(url) {
        let _ = tx.open_url(url);
    }
}

/// What a copy writes before the first text of a list item.
#[derive(Clone, Copy)]
struct Lead<'a> {
    /// The marker, after the indent and quote marks of what the item is in.
    prefix: &'a str,
    /// Whether the item follows the line before it without a blank line: every item but
    /// the first of a list that is not nested in another.
    tight: bool,
}

struct Body {
    look: Look,
    ty: t::MessageType,
    /// [`Marks::emoji`], by the index the core gives each in a paragraph.
    emoji: Vec<BodyEmoji>,
    selectable: bool,
    /// No text of this body has been drawn yet: the first follows the message's author
    /// and time without a blank line.
    first: Cell<bool>,
    /// [`Drawn::head`], until the first text takes it.
    head: Cell<Option<String>>,
    /// The next paragraph drawn ends with "(edited)": set for the body's last.
    edited: Cell<bool>,
    /// [`Drawn::id`].
    id: Option<String>,
    /// [`Drawn::copied`].
    copied: Option<usize>,
    /// How many code blocks have been drawn: the next one's number.
    codes: Cell<usize>,
}

impl Body {
    /// `blocks` in a column. `lead` is for the first of them, when they are a list item's;
    /// `indent` is what a copy writes before each of the others.
    /// The body's top-level blocks, the last paragraph carrying "(edited)" when `edited`.
    fn blocks_marked(&self, blocks: &[Block], edited: bool) -> El {
        let mut column = col().w_full();
        for (i, block) in blocks.iter().enumerate() {
            let after_para = i > 0 && matches!(blocks[i - 1], Block::Para(_));
            let gap = match block {
                _ if i == 0 => 0.0,
                Block::Para(_) if after_para => t::SPACE_4,
                _ => t::SPACE_8,
            };
            self.edited.set(edited && i + 1 == blocks.len());
            let drawn = self.block(block, None, "", false);
            column = column.child(drawn.key(format!("b{i}")).mt(gap));
        }
        column
    }

    fn blocks(&self, blocks: &[Block], lead: Option<Lead>, indent: &str, in_item: bool) -> El {
        let mut column = col().w_full();
        for (i, block) in blocks.iter().enumerate() {
            let lead = lead.filter(|_| i == 0);
            // The board sets a message's paragraphs 4 points apart, and what is not a
            // paragraph (its attachment card, there) 8 from the text around it.
            let after_para = i > 0 && matches!(blocks[i - 1], Block::Para(_));
            let gap = match block {
                _ if i == 0 => 0.0,
                Block::List { .. } if in_item => t::SPACE_4,
                Block::Para(_) if after_para && !in_item => t::SPACE_4,
                _ => t::SPACE_8,
            };
            let drawn = self.block(block, lead, indent, in_item);
            column = column.child(drawn.key(format!("b{i}")).mt(gap));
        }
        column
    }

    fn block(&self, block: &Block, lead: Option<Lead>, indent: &str, in_item: bool) -> El {
        let c = self.look.c;
        match block {
            Block::Para(inline) => self.lines(inline, lead, indent),
            Block::Code { lang, text } => {
                let (before, after) = self.breaks(lead);
                let prefix = lead.map_or(indent, |l| l.prefix);
                let code = self.code(lang.as_deref(), text);
                let code = self.copied(code, prefix, before, after);
                // The lines scroll sideways under the padding, so they run on to the frame's
                // edges, which fade while there is more past them.
                let lines = col()
                    .key("scroll")
                    .w_full()
                    .scroll_x()
                    .scroll_edge_fade(t::SPACE_16)
                    .child(
                        col()
                            .px(t::SPACE_12)
                            .py(t::SPACE_10)
                            .child(code.key("code")),
                    );
                let mut block = col()
                    .role("code")
                    .w_full()
                    .radius(t::RADIUS_12)
                    .fill(c.inset)
                    .border(1.0, c.line)
                    .border_layout()
                    .child(lines);
                let n = self.codes.replace(self.codes.get() + 1);
                if let Some(id) = &self.id {
                    block = block.hover_group().child(self.copy_button(id, n, text));
                }
                block
            }
            Block::Quote(inner) => {
                let quoted = format!("{indent}> ");
                let lead_quoted = lead.map(|l| format!("{}> ", l.prefix));
                let lead = lead.zip(lead_quoted.as_deref()).map(|(l, prefix)| Lead {
                    prefix,
                    tight: l.tight,
                });
                row()
                    .role("blockquote")
                    .w_full()
                    .child(
                        el().w(t::SPACE_2)
                            .h_full()
                            .radius(t::SPACE_2)
                            .fill(c.line_strong),
                    )
                    .child(
                        self.blocks(inner, lead, &quoted, false)
                            .grow(1.0)
                            .shrink()
                            .pl(t::SPACE_12),
                    )
            }
            Block::List { start, items } => {
                let mut list = col().role("list").w_full();
                for (k, item) in items.iter().enumerate() {
                    let marker = match start {
                        Some(first) => format!("{}.", first + k as u64),
                        None => "•".to_owned(),
                    };
                    let copied = match start {
                        Some(first) => format!("{}. ", first + k as u64),
                        None => "- ".to_owned(),
                    };
                    // The first item sits where the list does: after whatever the list
                    // itself leads with.
                    let before = lead.filter(|_| k == 0).map_or(indent, |l| l.prefix);
                    let prefix = format!("{before}{copied}");
                    let under = format!("{indent}{}", " ".repeat(copied.len()));
                    let item_lead = Lead {
                        prefix: &prefix,
                        tight: k > 0 || in_item,
                    };
                    let marker = text(marker)
                        .font(self.ty.message)
                        .color(c.ink_secondary)
                        .w(t::SPACE_28)
                        .pr(t::SPACE_8)
                        .align(Align::End)
                        .a11y_hidden();
                    list = list.child(
                        row()
                            .key(format!("i{k}"))
                            .role("listitem")
                            .position_in_set(k + 1, items.len())
                            .w_full()
                            .mt(if k == 0 { 0.0 } else { t::SPACE_4 })
                            .child(marker)
                            .child(
                                self.blocks(item, Some(item_lead), &under, true)
                                    .grow(1.0)
                                    .shrink(),
                            ),
                    );
                }
                list
            }
        }
    }

    /// A paragraph as selectable text. In a quote, one text for each line that was typed,
    /// so a copy marks every line of it as quoted and not the first alone.
    fn lines(&self, inline: &Inline, lead: Option<Lead>, indent: &str) -> El {
        let (before, after) = self.breaks(lead);
        let prefix = lead.map_or(indent, |l| l.prefix);
        let lines: Vec<&str> = inline.text.split('\n').collect();
        let split = self.selectable
            && indent.contains('>')
            && lines.len() > 1
            && lines.iter().all(|line| !line.is_empty());
        if !split {
            return self.copied(self.para(inline), prefix, before, after);
        }
        let mut column = col().w_full();
        let mut at = 0;
        for (n, line) in lines.iter().enumerate() {
            let part = inline.slice(at..at + line.len());
            at += line.len() + 1;
            let last = n + 1 == lines.len();
            let line = self.copied(
                self.para(&part),
                if n == 0 { prefix } else { indent },
                if n == 0 { before } else { 1 },
                if last { after } else { 1 },
            );
            column = column.child(line.key(format!("l{n}")));
        }
        column
    }

    /// A paragraph: its text with its styles, its links pressable, its held emoji pictured.
    fn para(&self, inline: &Inline) -> El {
        let pictures: Vec<(Range<usize>, &EmojiPicture)> = inline
            .emoji
            .iter()
            .filter_map(|(range, index)| {
                let picture = self.emoji.get(*index as usize)?.picture.as_ref()?;
                Some((range.clone(), picture))
            })
            .collect();
        if pictures.is_empty() {
            return self.para_drawn(inline, &[], None);
        }
        // What assistive technology reads: the words, each emoji by its name now.
        let mut label = String::with_capacity(inline.text.len());
        let mut at = 0;
        for (range, picture) in &pictures {
            label.push_str(inline.text.get(at..range.start).unwrap_or_default());
            label.push(':');
            label.push_str(&picture.name);
            label.push(':');
            at = range.end;
        }
        label.push_str(inline.text.get(at..).unwrap_or_default());
        let label = label.replace(lumia_client::body::OPEN, "");
        let label = label.replace(lumia_client::body::CLOSE, "");
        let (made, placed) = make_room(inline, &pictures);
        self.para_drawn(&made, &placed, Some(label))
    }

    fn para_drawn(&self, inline: &Inline, placed: &[Placed<'_>], label: Option<String>) -> El {
        let c = self.look.c;
        let edited = self.edited.replace(false);
        let (words, mut spans) = (inline.text.clone(), self.spans(inline));
        // A pictured emoji's name takes no room and shows nothing; its frame is the text's.
        for p in placed {
            spans.push(
                Span::new(p.name.start, p.name.end)
                    .size(UNSEEN)
                    .color(Color::TRANSPARENT),
            );
            spans.push(Span::new(p.bind.start, p.bind.end).size(UNSEEN));
        }
        let words = if edited {
            spans.push(
                Span::new(words.len(), words.len() + EDITED.len())
                    .size(self.ty.meta.size)
                    .color(c.ink_muted),
            );
            format!("{words}{EDITED}")
        } else {
            words
        };
        let end = words.len();
        let mut para = rich(words, spans)
            .font(self.ty.message)
            .color(c.ink_body)
            .wrap_anywhere()
            .w_full();
        for (range, style) in &inline.runs {
            if *style == Style::Strike {
                para = para.strike(range.start, range.end, None);
            }
        }
        if self.selectable && !inline.links.is_empty() {
            for (range, url) in &inline.links {
                // The address under the pointer: a link's text need not say where it goes.
                let link = Link::new(range.clone(), url.clone())
                    .hover(LinkPaint::new().underline(c.link))
                    .focused(LinkPaint::new().bg(c.accent_soft).ring(c.focus, 2.0))
                    .help(url.clone());
                para = para.link(link);
            }
            para = para.on_link(open_link);
            // A right-click on one: the message's menu, with the link's own items first.
            if let Some(id) = self.id.clone() {
                para = para.on_link_secondary(
                    move |s: &mut State, url: &str, at: rune::PointerPress, _: &mut Tx| {
                        s.open_message_context(&id, [at.x, at.y], Some(url));
                    },
                );
            }
        }
        let mut padded = false;
        for (range, style) in &inline.runs {
            if *style == Style::Pad {
                padded = true;
                if self.selectable {
                    para = para.uncopied(range.clone());
                }
            }
        }
        let height = self.ty.message.size * EMOJI_HEIGHT;
        for p in placed {
            // As wide as its shape at the emoji height: a wide one's square is as wide as
            // it is, and the picture fits it, centred.
            let side = height * p.picture.aspect.max(1.0);
            let icon = InlineIcon::image(p.picture.pixels.clone())
                .size(side)
                .gap(0.0);
            para = para.inline_icon(p.join, icon);
            if self.selectable {
                para = para
                    .uncopied(p.join..p.join + JOIN.len())
                    .uncopied(p.bind.clone());
            }
        }
        // Assistive technology reads the words, not the spacing drawn around inline code,
        // and an emoji by its name now.
        let label = label.or_else(|| padded.then(|| inline.plain()));
        if let Some(label) = label {
            para = para.label(if edited {
                format!("{label}{EDITED}")
            } else {
                label
            });
        }
        // A copy takes what was said, not that it was changed.
        if edited && self.selectable {
            para = para.uncopied(end - EDITED.len()..end);
        }
        para
    }

    fn spans(&self, inline: &Inline) -> Vec<Span> {
        let c = self.look.c;
        let mut spans = Vec::with_capacity(inline.links.len() + inline.runs.len());
        // Links first: where spans overlap the later one wins, and code in a link keeps
        // its own look.
        for (range, _) in &inline.links {
            let link = Span::new(range.start, range.end)
                .color(c.link)
                .underline(t::link_underline(c));
            spans.push(link);
        }
        for (range, style) in &inline.runs {
            let span = Span::new(range.start, range.end);
            let linked = inline
                .links
                .iter()
                .any(|(link, _)| link.start <= range.start && range.end <= link.end);
            let span = match style {
                Style::Emphasis => span.italic(),
                Style::Strong => span.weight(t::WEIGHT_STRONG),
                Style::Code => span
                    .mono()
                    .size(self.ty.code_inline)
                    .bg(c.inset, t::CODE_PAD),
                Style::Pad => span.size(pad_size()),
                // A line through the text, drawn by the paragraph.
                Style::Strike => continue,
            };
            // A chip in a link is not underlined as well.
            let chip = matches!(style, Style::Code | Style::Pad) && linked;
            spans.push(if chip {
                span.underline(Color::TRANSPARENT)
            } else {
                span
            });
        }
        // Mentions last, over what they stand in; the core marks none inside code or a link.
        for (range, _) in &inline.mentions {
            spans.push(
                Span::new(range.start, range.end)
                    .color(c.accent_ink)
                    .weight(t::TEXT_LINK_SM.weight)
                    .bg(c.accent_soft, t::CODE_PAD),
            );
        }
        spans
    }

    /// A code block's text, coloured when the highlighter knows the language.
    fn code(&self, lang: Option<&str>, code: &str) -> El {
        let spans = match lang {
            Some(lang) if syntax::known(lang) => {
                let theme = SYNTAX.with(|themes| themes[usize::from(self.look.dark)]);
                theme.spans(&syntax::highlight(Some(lang), code), 0)
            }
            _ => Vec::new(),
        };
        rich(code.to_owned(), spans)
            .font(self.ty.code)
            .color(self.look.c.ink_body)
    }

    /// A code block's "Copy", in its top-right corner: shown while the pointer is over the
    /// block or the keyboard is in it, and reading "Copied" just after a press.
    fn copy_button(&self, id: &str, n: usize, code: &str) -> El {
        let c = self.look.c;
        let words = if self.copied == Some(n) {
            "Copied"
        } else {
            "Copy"
        };
        let (id, code) = (id.to_owned(), code.to_owned());
        el().pin(None, Some(t::SPACE_6), Some(t::SPACE_6), None)
            .group_opacity(0.0, 1.0)
            .child(
                row()
                    .key("copy")
                    .h(t::SPACE_24)
                    .px(t::SPACE_8)
                    .center()
                    .radius(t::RADIUS_6)
                    .fill(c.surface)
                    .border(1.0, c.line)
                    .hover(c.hover_icon)
                    .role("button")
                    .label("Copy code")
                    .cursor(Cursor::Pointer)
                    .focusable(true)
                    .focus_ring(2.0, 2.0, c.focus)
                    // A press here leaves a selection in the conversation as it is.
                    .keeps_text_selection()
                    .on_press_tx(move |s: &mut State, tx| s.copy_code(&id, n, &code, tx))
                    .child(text(words).font(self.ty.meta).color(c.ink_secondary)),
            )
    }

    /// A body too tall to show whole: `shown` lines of it, the last fading into the row,
    /// with the button that opens it; or, `open`, all of it with the button that folds it.
    fn folded(&self, whole: El, shown: usize, open: bool, id: &str) -> El {
        let c = self.look.c;
        let line = self.ty.message.line_height;
        let mut fold = col().w_full();
        if !open {
            fold = fold.h(line * shown as f32).clip_fade(0.0, 0.0, 0.0, line);
        }
        let words = if open { "Show less" } else { "Show more" };
        let id = id.to_owned();
        let button = el()
            .key("more")
            .role("button")
            .label(words)
            .expanded(open)
            .cursor(Cursor::Pointer)
            .focusable(true)
            .focus_ring(2.0, 2.0, c.focus)
            .radius(t::RADIUS_5)
            .on_press(move |s: &mut State| s.unfold(&id))
            .child(text(words).font(self.ty.link_sm).color(c.accent_ink));
        col()
            .w_full()
            .gap(t::SPACE_4)
            .child(fold.child(whole))
            .child(row().child(button))
    }

    /// How many lines of its type `blocks` take at `width`, as [`Body::blocks_marked`] lays
    /// them out; none when they cannot take more than [`FOLD_OVER`] however their words
    /// wrap, which spares setting the text of the short messages most are.
    fn lines_at(&self, blocks: &[Block], width: f32) -> Option<usize> {
        let line = self.ty.message.line_height;
        let lines = |height: f32| (height / line).round() as usize;
        if lines(self.height(blocks, width, false, Wraps::Most)) <= FOLD_OVER {
            return None;
        }
        Some(lines(self.height(blocks, width, false, Wraps::Set)))
    }

    /// The height of `blocks` at `width`: [`Body::blocks`] measured rather than drawn, its
    /// paragraphs set as they will be drawn or, cheaply, at the most lines they can take.
    fn height(&self, blocks: &[Block], width: f32, in_item: bool, wraps: Wraps) -> f32 {
        let line = self.ty.message.line_height;
        let mut height = 0.0;
        for (i, block) in blocks.iter().enumerate() {
            let after_para = i > 0 && matches!(blocks[i - 1], Block::Para(_));
            height += match block {
                _ if i == 0 => 0.0,
                Block::List { .. } if in_item => t::SPACE_4,
                Block::Para(_) if after_para && !in_item => t::SPACE_4,
                _ => t::SPACE_8,
            };
            height += match block {
                Block::Para(inline) if wraps == Wraps::Most => {
                    // No character in the faces a message is drawn in is wider than an em
                    // and a quarter (an ideograph is an em, an emoji a little more).
                    let size = self.ty.message.size;
                    let per_line = (width / (size * 1.25)).floor().max(1.0) as usize;
                    // A typed line that fits is one line. One that wraps wraps at its words,
                    // so a line can end well short of the edge, but any two lines in a row
                    // hold more than a line's worth (the word moved down did not fit in what
                    // was left): twice its characters over a line, and one more, bound it.
                    // A tab counts as the eight it can take.
                    let most: usize = inline
                        .text
                        .split('\n')
                        .map(|words| {
                            let chars = words
                                .chars()
                                .map(|c| if c == '\t' { 8 } else { 1 })
                                .sum::<usize>();
                            if chars <= per_line {
                                1
                            } else {
                                (2 * chars).div_ceil(per_line) + 1
                            }
                        })
                        .sum();
                    most as f32 * line
                }
                Block::Para(inline) => {
                    let options = RichLayoutOptions::new().wrap_anywhere();
                    let spans = self.spans(inline);
                    let style = self.ty.message;
                    rune::text::measure_with(&inline.text, &spans, style, Some(width), options).1
                }
                // Its lines, never wrapped, inside its padding and its border.
                Block::Code { text, .. } => {
                    let lines = text.split('\n').count() as f32;
                    lines * self.ty.code.line_height + 2.0 * (t::SPACE_10 + 1.0)
                }
                Block::Quote(inner) => {
                    self.height(inner, width - t::SPACE_2 - t::SPACE_12, false, wraps)
                }
                Block::List { items, .. } => items
                    .iter()
                    .enumerate()
                    .map(|(k, item)| {
                        let gap = if k == 0 { 0.0 } else { t::SPACE_4 };
                        gap + self
                            .height(item, width - t::SPACE_28, true, wraps)
                            .max(line)
                    })
                    .sum(),
            };
        }
        height
    }

    /// The newlines a copy leaves before and after a block: a blank line around it, but one
    /// newline around the text that opens a list item, and after the message's author and
    /// time, which the first text of a body follows. A body that carries its author and time
    /// itself ([`Drawn::head`]) has the blank line before them.
    fn breaks(&self, lead: Option<Lead>) -> (u8, u8) {
        let head = self.head.take();
        let headed = head.is_some();
        self.head.set(head);
        let first = self.first.replace(false) && !headed;
        let tight = first || lead.is_some_and(|l| l.tight);
        (
            if tight { 1 } else { 2 },
            if lead.is_some() { 1 } else { 2 },
        )
    }

    /// `drawn`, a text, selectable, with what a copy writes around it: the body's head
    /// before the first text, then its own prefix.
    fn copied(&self, drawn: El, prefix: &str, before: u8, after: u8) -> El {
        if !self.selectable {
            return drawn;
        }
        let drawn = drawn
            .selectable(true)
            .selection_color(self.look.c.accent_soft)
            .copy_breaks(before, after);
        match self.head.take() {
            Some(head) => drawn.copy_prefix(format!("{head}\n{prefix}")),
            None if prefix.is_empty() => drawn,
            None => drawn.copy_prefix(prefix),
        }
    }
}

/// The size that sets the narrow spaces either side of inline code to the chip's padding,
/// whatever the face. Solved twice: a variable font's optical size moves its spaces'
/// widths with the size.
fn pad_size() -> f32 {
    let epoch = rune::text::font_epoch();
    let (measured, size) = PAD_SIZE.get();
    if measured == epoch {
        return size;
    }
    let font = t::TEXT_MESSAGE;
    let probe = PAD.repeat(64);
    let wide = |size: f32| {
        let style = TextStyle { size, ..font };
        (rune::text::measure(&probe, &[], style, None).0 / 64.0).max(0.1)
    };
    let first = font.size * t::CODE_PAD / wide(font.size);
    let size = first * t::CODE_PAD / wide(first);
    PAD_SIZE.set((epoch, size));
    size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pictured_emoji_is_framed_and_every_range_moves_with_it() {
        // "hi :a::b: **x**", two emoji side by side, then strong text.
        let inline = Inline {
            text: "hi :a::b: x".into(),
            runs: vec![(10..11, Style::Strong)],
            links: Vec::new(),
            bracketed: Vec::new(),
            mentions: Vec::new(),
            emoji: vec![(3..6, 0), (6..9, 1)],
        };
        let picture = EmojiPicture {
            pixels: Pixels::default(),
            aspect: 1.0,
            name: "a".into(),
        };
        let (made, placed) = make_room(&inline, &[(3..6, &picture), (6..9, &picture)]);
        assert_eq!(made.text, "hi \u{2060}:a:\u{a0}\u{2060}:b:\u{a0} x");
        // Each name is its own text still, between its frame.
        let names: Vec<&str> = placed.iter().map(|p| &made.text[p.name.clone()]).collect();
        assert_eq!(names, [":a:", ":b:"]);
        for p in &placed {
            assert_eq!(&made.text[p.join..p.join + JOIN.len()], JOIN);
            assert_eq!(&made.text[p.bind.clone()], BIND);
        }
        assert_eq!(
            made.emoji
                .iter()
                .map(|(r, _)| &made.text[r.clone()])
                .collect::<Vec<_>>(),
            [":a:", ":b:"]
        );
        assert_eq!(&made.text[made.runs[0].0.clone()], "x");
    }

    #[test]
    fn a_body_folds_to_ten_lines_only_when_it_is_taller_than_fourteen() {
        for lines in 0..=14 {
            assert_eq!(folded_lines(lines), None, "{lines} lines show whole");
        }
        for lines in [15, 16, 30, 1_000] {
            assert_eq!(folded_lines(lines), Some(10), "{lines} lines fold");
        }
    }
}
