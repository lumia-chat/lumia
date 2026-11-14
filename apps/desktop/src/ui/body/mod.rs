//! A message body: the protocol's Markdown subset (protocol 14) drawn as Rune rich text,
//! every text of it selectable and in the selection group of whatever lists it. That is
//! the reason this is the shell's own renderer and not the kit's `Markdown`: the kit's is a
//! selection group of its own, so a drag stopped at the end of the message it began in.
//! Here the timeline is the group, and a drag runs from one message into the next.
//!
//! `parse` reads the source into blocks and decides what is in the subset; this file draws
//! them. A paragraph is one `rich` text; a code block is one text on the `inset` ground,
//! wrapped at the block's width and coloured by the kit's highlighter for the languages it
//! knows; a quote is its blocks beside a rule; a list is rows of a marker and the item's
//! blocks. A word wider than the line breaks inside itself rather than run out of the row.
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
//!
//! A mention (architecture 6.6) is drawn as the design system's pill: the message gives
//! each one's range in its source, which the parser does not keep, so the words of each
//! (`@ada`) are found again in the paragraphs drawn, where they stand as a word and outside
//! code.

mod parse;

use std::cell::Cell;

use rune::{Align, Color, El, Link, LinkPaint, Span, TextStyle, Tx, col, el, rich, row, text};
use rune_kit::syntax::{self, SyntaxTheme};

pub use parse::{Block, Inline, Style, is_web, parse};

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
    view_with(source, selectable, &[], Drawn::default(), look)
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
}

impl Default for Drawn<'_> {
    fn default() -> Self {
        Self {
            ty: &t::MESSAGE_TYPE,
            head: None,
            edited: false,
        }
    }
}

/// [`view`] with the words of the message's mentions (`@ada`), drawn as pills.
pub fn view_with(
    source: &str,
    selectable: bool,
    mentions: &[String],
    drawn: Drawn<'_>,
    look: Look,
) -> El {
    let blocks = parse(source);
    let body = Body {
        look,
        ty: *drawn.ty,
        selectable,
        first: Cell::new(true),
        head: Cell::new(drawn.head.map(str::to_owned)),
        mentions: mentions.to_vec(),
        edited: Cell::new(false),
    };
    let edited = drawn.edited && ends_in_paragraph(&blocks);
    body.blocks_marked(&blocks, edited)
}

/// Whether "(edited)" can follow a body's words on its last line: it ends in a paragraph,
/// not in code, a quote or a list, after which it stands on a line of its own.
pub fn edited_inline(source: &str) -> bool {
    ends_in_paragraph(&parse(source))
}

fn ends_in_paragraph(blocks: &[Block]) -> bool {
    matches!(blocks.last(), Some(Block::Para(_)))
}

/// What follows an edited message's words.
const EDITED: &str = " (edited)";

/// Where `words` stand in `text` as a word of their own: not inside another word, and not
/// running on into the characters a handle has.
fn standing(text: &str, words: &str) -> Vec<std::ops::Range<usize>> {
    let handle = |c: char| c.is_alphanumeric() || matches!(c, '_' | '.' | '-');
    let mut found = Vec::new();
    let mut from = 0;
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
    selectable: bool,
    /// No text of this body has been drawn yet: the first follows the message's author
    /// and time without a blank line.
    first: Cell<bool>,
    /// [`Drawn::head`], until the first text takes it.
    head: Cell<Option<String>>,
    /// The words of the message's mentions, drawn as pills.
    mentions: Vec<String>,
    /// The next paragraph drawn ends with "(edited)": set for the body's last.
    edited: Cell<bool>,
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
                col()
                    .role("code")
                    .w_full()
                    .px(t::SPACE_12)
                    .py(t::SPACE_10)
                    .radius(t::RADIUS_12)
                    .fill(c.inset)
                    .border(1.0, c.line)
                    .child(code.key("code"))
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

    /// A paragraph: its text with its styles, its links pressable.
    fn para(&self, inline: &Inline) -> El {
        let c = self.look.c;
        let edited = self.edited.replace(false);
        let (words, mut spans) = (inline.text.clone(), self.spans(inline));
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
        // Assistive technology reads the words, not the spacing drawn around inline code.
        if padded {
            para = para.label(if edited {
                format!("{}{EDITED}", inline.plain())
            } else {
                inline.plain()
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
        // Mentions last, over what they stand in, but never inside code.
        for words in &self.mentions {
            for range in standing(&inline.text, words) {
                let in_code = inline.runs.iter().any(|(run, style)| {
                    matches!(style, Style::Code) && run.start < range.end && range.start < run.end
                });
                if !in_code {
                    spans.push(
                        Span::new(range.start, range.end)
                            .color(c.accent_ink)
                            .weight(t::TEXT_LINK_SM.weight)
                            .bg(c.accent_soft, t::CODE_PAD),
                    );
                }
            }
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
            .wrap_anywhere()
            .w_full()
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
    let probe = parse::PAD.repeat(64);
    let wide = |size: f32| {
        let style = TextStyle { size, ..font };
        (rune::text::measure(&probe, &[], style, None).0 / 64.0).max(0.1)
    };
    let first = font.size * t::CODE_PAD / wide(font.size);
    let size = first * t::CODE_PAD / wide(first);
    PAD_SIZE.set((epoch, size));
    size
}
