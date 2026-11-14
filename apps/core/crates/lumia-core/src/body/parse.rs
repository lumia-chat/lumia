//! A message body's source to blocks: the protocol's Markdown subset (protocol 14) and
//! nothing else, read the same way for every client.
//!
//! `pulldown-cmark` reads CommonMark, which is more than the subset, so the walk hands back
//! as typed whatever the subset does not have: a heading keeps its `#`, raw HTML its tags,
//! an image its `![alt](url)`, a rule its dashes, an indented code block its indent. Tables,
//! task lists, footnotes and the other extensions are never switched on, so they are
//! paragraphs and list items to begin with. A link is one only when it is written with its
//! destination and that destination is a web address ([`is_web`]); a link to anything else
//! and a reference-style link stay as typed, and so does a reference's definition, which
//! CommonMark would otherwise swallow.
//!
//! Two departures from CommonMark, both because this is a chat message and not a document:
//! a line break the person typed is a line break (CommonMark joins the lines of a paragraph
//! with a space), and a bare `https://…` is a link without angle brackets around it.

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};

/// A block of a message body.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Para(Inline),
    /// A fenced code block: the first word after its fence, if any, and its lines.
    Code {
        lang: Option<String>,
        text: String,
    },
    Quote(Vec<Block>),
    /// A list, ordered from `start` when it has one, each item a run of blocks.
    List {
        start: Option<u64>,
        items: Vec<Vec<Block>>,
    },
}

/// A paragraph's text as drawn, with what styles it. Ranges are bytes of `text`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inline {
    pub text: String,
    pub runs: Vec<(Range<usize>, Style)>,
    /// Web links: their text's range and where they go.
    pub links: Vec<(Range<usize>, String)>,
    /// The message's mentions ([`super::parse_with`]): each one's range of the text and its
    /// place among the mentions the body was read with. Never inside code or a link.
    pub mentions: Vec<(Range<usize>, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Emphasis,
    Strong,
    Strike,
    Code,
    /// The characters either side of inline code ([`OPEN`], [`CLOSE`]): drawn, never copied.
    Pad,
}

/// What sets inline code apart from the text before it: two narrow spaces, for the room its
/// chip takes, and a left-to-right isolate, so code reads left to right and moves as one
/// inside right-to-left text. [`CLOSE`] ends both.
pub const OPEN: &str = "\u{202f}\u{202f}\u{2066}";
pub const CLOSE: &str = "\u{2069}\u{202f}\u{202f}";
/// The spaces of [`OPEN`] and [`CLOSE`], which the view sizes to the chip's padding.
pub const PAD: &str = "\u{202f}\u{202f}";

/// How deep quotes and lists nest before the rest is read as part of the deepest: a message
/// is someone else's text, and a thousand `>` must not become a thousand nested views.
const MAX_NESTING: usize = 12;

impl Inline {
    /// The text without inline code's padding: what a copy takes.
    pub fn plain(&self) -> String {
        self.text.replace(OPEN, "").replace(CLOSE, "")
    }

    /// Bytes `range` of the text as a paragraph of its own, with the styles and links that
    /// reach into it.
    pub fn slice(&self, range: Range<usize>) -> Inline {
        let clip = |r: &Range<usize>| {
            let (start, end) = (r.start.max(range.start), r.end.min(range.end));
            (start < end).then(|| start - range.start..end - range.start)
        };
        Inline {
            text: self.text.get(range.clone()).unwrap_or_default().to_owned(),
            runs: self
                .runs
                .iter()
                .filter_map(|(r, style)| Some((clip(r)?, *style)))
                .collect(),
            links: self
                .links
                .iter()
                .filter_map(|(r, url)| Some((clip(r)?, url.clone())))
                .collect(),
            mentions: self
                .mentions
                .iter()
                .filter_map(|(r, index)| Some((clip(r)?, *index)))
                .collect(),
        }
    }

    fn literal(text: &str) -> Self {
        let mut inline = Inline {
            text: text.to_owned(),
            ..Inline::default()
        };
        autolink(&mut inline);
        inline
    }
}

/// Whether a link's destination is a web address, the only kind a client opens: other
/// schemes can start other programs, and a relative one has nothing to be relative to.
pub fn is_web(url: &str) -> bool {
    let rest = ["https://", "http://"].iter().find_map(|scheme| {
        url.get(..scheme.len())
            .filter(|head| head.eq_ignore_ascii_case(scheme))
            .map(|_| &url[scheme.len()..])
    });
    rest.is_some_and(|rest| {
        !rest.is_empty() && !rest.contains(|c: char| c.is_whitespace() || c.is_control())
    })
}

/// Parses a message body. Whatever the source holds comes out as text somewhere: a body
/// that is not blank never parses to nothing.
pub fn parse(src: &str) -> Vec<Block> {
    // One kind of line ending, so no carriage return is drawn or copied.
    let unix;
    let src = if src.contains('\r') {
        unix = src.replace("\r\n", "\n").replace('\r', "\n");
        unix.as_str()
    } else {
        src
    };
    let mut b = Builder {
        src,
        stack: vec![Frame::new(Kind::Root, 0..0)],
        line: None,
        code: None,
        indented: false,
        heading: None,
        skip: 0,
        over: 0,
        starts: Vec::new(),
    };
    let mut events = Parser::new_ext(src, Options::ENABLE_STRIKETHROUGH).into_offset_iter();
    for (event, range) in events.by_ref() {
        b.event(event, range);
    }
    b.flush();
    while b.stack.len() > 1 {
        b.pop();
    }
    let mut blocks = b.stack.pop().map(|root| root.blocks).unwrap_or_default();
    // A reference's definition (`[1]: https://…`) is no part of the events: put each back
    // where it was typed, as the text it is.
    let mut definitions: Vec<Range<usize>> = events
        .reference_definitions()
        .iter()
        .map(|(_, def)| def.span.clone())
        .collect();
    definitions.sort_by_key(|span| span.start);
    for span in definitions {
        let text = src.get(span.clone()).unwrap_or_default().trim();
        if text.is_empty() {
            continue;
        }
        let at = b.starts.partition_point(|start| *start <= span.start);
        b.starts.insert(at, span.start);
        blocks.insert(at.min(blocks.len()), Block::Para(Inline::literal(text)));
    }
    if blocks.is_empty() && !src.trim().is_empty() {
        blocks.push(Block::Para(Inline::literal(src.trim())));
    }
    blocks
}

#[derive(PartialEq)]
enum Kind {
    Root,
    Quote,
    List(Option<u64>),
    Item,
}

/// An open container: the blocks read into it so far, a list's finished items, and its
/// source.
struct Frame {
    kind: Kind,
    blocks: Vec<Block>,
    items: Vec<Vec<Block>>,
    range: Range<usize>,
}

impl Frame {
    fn new(kind: Kind, range: Range<usize>) -> Self {
        Self {
            kind,
            blocks: Vec::new(),
            items: Vec::new(),
            range,
        }
    }
}

/// The paragraph being read.
struct Line {
    inline: Inline,
    open: Vec<Open>,
    start: usize,
}

/// A run that has started and not ended.
struct Open {
    start: usize,
    what: Opened,
}

enum Opened {
    Style(Style),
    Link(String),
    /// A single `~`, which CommonMark's extension strikes with and the subset does not:
    /// its closing `~` is owed.
    Tilde,
}

/// A heading being read as the paragraph it was typed as.
struct Heading {
    range: Range<usize>,
    /// Where its last piece of text ended, once it has any: the source before the first
    /// piece and after the last is its marker.
    last: Option<usize>,
}

struct Builder<'a> {
    src: &'a str,
    stack: Vec<Frame>,
    line: Option<Line>,
    /// A fenced block being read: its language, its text, where it starts.
    code: Option<(Option<String>, String, usize)>,
    /// Inside an indented code block, which is read as the paragraph it was typed as.
    indented: bool,
    heading: Option<Heading>,
    /// How deep the walk is inside an element taken as typed, whose events are dropped.
    skip: usize,
    /// How many containers past [`MAX_NESTING`] are open.
    over: usize,
    /// Where each block at the root starts in the source.
    starts: Vec<usize>,
}

impl Builder<'_> {
    fn event(&mut self, event: Event, range: Range<usize>) {
        if self.skip > 0 {
            match event {
                Event::Start(_) => self.skip += 1,
                Event::End(_) => {
                    self.skip -= 1;
                    if self.skip == 0 {
                        self.after(range.end);
                    }
                }
                _ => {}
            }
            return;
        }
        match event {
            Event::Start(tag) => self.start(tag, range),
            Event::End(tag) => self.end(tag, range),
            Event::Text(text) => {
                if let Some((_, code, _)) = &mut self.code {
                    code.push_str(&text);
                } else if self.indented {
                    for line in text.split_inclusive('\n') {
                        self.push("    ");
                        self.push(line);
                    }
                } else {
                    self.text(&text, &range);
                }
            }
            Event::Code(code) => {
                self.before(range.start);
                let inline = &mut self.line(range.start).inline;
                let a = inline.text.len();
                inline.text.push_str(OPEN);
                let b = inline.text.len();
                inline.text.push_str(&code);
                let c = inline.text.len();
                inline.text.push_str(CLOSE);
                let d = inline.text.len();
                inline
                    .runs
                    .extend([(a..b, Style::Pad), (b..c, Style::Code), (c..d, Style::Pad)]);
                self.after(range.end);
            }
            Event::Html(text) | Event::InlineHtml(text) => self.text(&text, &range),
            Event::SoftBreak | Event::HardBreak => self.text("\n", &range),
            Event::Rule => {
                self.flush();
                let typed = self.src.get(range.clone()).unwrap_or_default().trim();
                self.block(Block::Para(Inline::literal(typed)), range.start);
            }
            // Extensions that are never switched on; as typed if one ever arrives.
            Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::FootnoteReference(_)
            | Event::TaskListMarker(_) => {
                let typed = self.src.get(range.clone()).unwrap_or_default().to_owned();
                self.text(&typed, &range);
            }
        }
    }

    fn start(&mut self, tag: Tag, range: Range<usize>) {
        match tag {
            Tag::Paragraph | Tag::HtmlBlock => {
                self.flush();
                self.line(range.start);
            }
            Tag::Heading { .. } => {
                self.flush();
                self.line(range.start);
                self.heading = Some(Heading { range, last: None });
            }
            Tag::CodeBlock(CodeBlockKind::Fenced(info)) => {
                self.flush();
                // `rust,ignore` and `ts title="a.ts"` name their language first.
                let lang = info
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .next()
                    .filter(|word| !word.is_empty())
                    .map(str::to_owned);
                self.code = Some((lang, String::new(), range.start));
            }
            Tag::CodeBlock(CodeBlockKind::Indented) => {
                self.flush();
                self.line(range.start);
                self.indented = true;
            }
            Tag::BlockQuote(_) => self.open_frame(Kind::Quote, range),
            Tag::List(start) => self.open_frame(Kind::List(start), range),
            Tag::Item => self.open_frame(Kind::Item, range),
            Tag::Emphasis => self.open(Opened::Style(Style::Emphasis), &range),
            Tag::Strong => self.open(Opened::Style(Style::Strong), &range),
            Tag::Strikethrough => {
                if self
                    .src
                    .get(range.clone())
                    .is_some_and(|s| s.starts_with("~~"))
                {
                    self.open(Opened::Style(Style::Strike), &range);
                } else {
                    self.before(range.start);
                    self.push("~");
                    self.open(Opened::Tilde, &range);
                }
            }
            Tag::Link {
                link_type: LinkType::Inline | LinkType::Autolink,
                dest_url,
                ..
            } if is_web(&dest_url) => self.open(Opened::Link(dest_url.into_string()), &range),
            Tag::Link { .. } | Tag::Image { .. } => {
                self.before(range.start);
                let typed = self.src.get(range).unwrap_or_default().to_owned();
                self.push(&typed);
                self.skip = 1;
            }
            // Extensions that are never switched on.
            Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::Table(_)
            | Tag::TableHead
            | Tag::TableRow
            | Tag::TableCell
            | Tag::Superscript
            | Tag::Subscript
            | Tag::MetadataBlock(_) => {}
        }
    }

    fn end(&mut self, tag: TagEnd, range: Range<usize>) {
        match tag {
            TagEnd::Paragraph | TagEnd::HtmlBlock => self.flush(),
            TagEnd::Heading(_) => {
                if let Some(heading) = self.heading.take() {
                    let from = heading.last.unwrap_or(heading.range.start);
                    let tail = self.src.get(from..heading.range.end).unwrap_or_default();
                    let tail = match heading.last {
                        Some(_) => tail.trim_end(),
                        None => tail.trim(),
                    };
                    self.push(tail);
                }
                self.flush();
            }
            TagEnd::CodeBlock => {
                self.indented = false;
                match self.code.take() {
                    Some((lang, mut text, start)) => {
                        if text.ends_with('\n') {
                            text.pop();
                        }
                        self.block(Block::Code { lang, text }, start);
                    }
                    None => self.flush(),
                }
            }
            TagEnd::BlockQuote(_) | TagEnd::List(_) | TagEnd::Item => {
                if self.over > 0 {
                    self.flush();
                    self.over -= 1;
                } else {
                    self.pop();
                }
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                self.close(range.end);
            }
            _ => {}
        }
    }

    /// The paragraph being read, begun at `start` if none is: a tight list item has text
    /// without a paragraph around it.
    fn line(&mut self, start: usize) -> &mut Line {
        self.line.get_or_insert_with(|| Line {
            inline: Inline::default(),
            open: Vec::new(),
            start,
        })
    }

    fn push(&mut self, text: &str) {
        self.line(0).inline.text.push_str(text);
    }

    fn text(&mut self, text: &str, range: &Range<usize>) {
        self.before(range.start);
        self.line(range.start).inline.text.push_str(text);
        self.after(range.end);
    }

    /// Whether the walk is directly in the paragraph, inside no run.
    fn level(&self) -> bool {
        self.line.as_ref().is_none_or(|line| line.open.is_empty())
    }

    /// Before a piece of a heading's text at `at`: what was typed before its first piece
    /// is its marker (`# `), kept.
    fn before(&mut self, at: usize) {
        if !self.level() {
            return;
        }
        let Some(heading) = self.heading.as_mut().filter(|h| h.last.is_none()) else {
            return;
        };
        heading.last = Some(at);
        let marker = self.src.get(heading.range.start..at).unwrap_or_default();
        let marker = marker.trim_start().to_owned();
        self.push(&marker);
    }

    /// After a piece of a heading's text ending at `at`.
    fn after(&mut self, at: usize) {
        if !self.level() {
            return;
        }
        if let Some(heading) = &mut self.heading {
            heading.last = Some(at);
        }
    }

    fn open(&mut self, what: Opened, range: &Range<usize>) {
        self.before(range.start);
        let line = self.line(range.start);
        let start = line.inline.text.len();
        line.open.push(Open { start, what });
    }

    fn close(&mut self, at: usize) {
        if let Some(line) = &mut self.line {
            if let Some(open) = line.open.pop() {
                end_run(&mut line.inline, open);
            }
        }
        self.after(at);
    }

    /// Ends the paragraph being read and adds it to the container it is in.
    fn flush(&mut self) {
        let Some(mut line) = self.line.take() else {
            return;
        };
        while let Some(open) = line.open.pop() {
            end_run(&mut line.inline, open);
        }
        let inline = &mut line.inline;
        // Trailing blanks go; the narrow spaces that close inline code are not blanks.
        let blank = |c: char| c.is_whitespace() && c != '\u{202f}';
        let len = inline.text.trim_end_matches(blank).len();
        inline.text.truncate(len);
        inline.runs.retain_mut(|(range, _)| {
            range.end = range.end.min(len);
            range.start < range.end
        });
        inline.links.retain_mut(|(range, _)| {
            range.end = range.end.min(len);
            range.start < range.end
        });
        if inline.text.is_empty() {
            return;
        }
        autolink(inline);
        self.block(Block::Para(line.inline), line.start);
    }

    fn block(&mut self, block: Block, start: usize) {
        if self.stack.len() == 1 {
            self.starts.push(start);
        }
        if let Some(frame) = self.stack.last_mut() {
            frame.blocks.push(block);
        }
    }

    fn open_frame(&mut self, kind: Kind, range: Range<usize>) {
        self.flush();
        if self.stack.len() > MAX_NESTING {
            self.over += 1;
        } else {
            self.stack.push(Frame::new(kind, range));
        }
    }

    /// Closes the innermost container into the one around it.
    fn pop(&mut self) {
        self.flush();
        if self.stack.len() < 2 {
            return;
        }
        let Some(frame) = self.stack.pop() else {
            return;
        };
        let start = frame.range.start;
        match frame.kind {
            Kind::Item => match self.stack.last_mut() {
                Some(list) if matches!(list.kind, Kind::List(_)) => list.items.push(frame.blocks),
                // An item past the nesting limit, its list not opened: its blocks alone.
                Some(other) => other.blocks.extend(frame.blocks),
                None => {}
            },
            Kind::List(first) => {
                let mut items = frame.items;
                // Blocks past the nesting limit, read into the list itself.
                if !frame.blocks.is_empty() {
                    items.push(frame.blocks);
                }
                if !items.is_empty() {
                    let list = Block::List {
                        start: first,
                        items,
                    };
                    self.block(list, start);
                }
            }
            // A `>` with nothing after it is the character, not an empty quote; `>>>` is
            // three of them, which the outermost of the three writes.
            Kind::Quote if frame.blocks.is_empty() => {
                let within = self
                    .stack
                    .last()
                    .is_some_and(|parent| parent.kind == Kind::Quote && parent.blocks.is_empty());
                let typed = self.src.get(frame.range).unwrap_or_default().trim();
                if !within && !typed.is_empty() {
                    self.block(Block::Para(Inline::literal(typed)), start);
                }
            }
            Kind::Quote => self.block(Block::Quote(frame.blocks), start),
            Kind::Root => {}
        }
    }
}

/// Ends `open` at the current end of the text.
fn end_run(inline: &mut Inline, open: Open) {
    let range = open.start..inline.text.len();
    match open.what {
        Opened::Tilde => inline.text.push('~'),
        _ if range.is_empty() => {}
        Opened::Style(style) => inline.runs.push((range, style)),
        Opened::Link(url) => inline.links.push((range, url)),
    }
}

/// Makes links of the bare web addresses in `inline` (`https://…`, `http://…`, `www.…`)
/// outside its inline code and the links it already has, by the rules of GitHub's
/// autolinks: an address starts a word, and the punctuation that ends a sentence is not
/// part of it.
fn autolink(inline: &mut Inline) {
    if !inline.text.contains("://") && !inline.text.contains("www.") {
        return;
    }
    let mut taken: Vec<Range<usize>> = inline
        .runs
        .iter()
        .filter(|(_, style)| matches!(style, Style::Code | Style::Pad))
        .map(|(range, _)| range.clone())
        .chain(inline.links.iter().map(|(range, _)| range.clone()))
        .collect();
    taken.sort_by_key(|range| range.start);
    let mut found = Vec::new();
    let mut from = 0;
    for range in taken {
        if from < range.start {
            scan(&inline.text, from..range.start, &mut found);
        }
        from = from.max(range.end);
    }
    scan(&inline.text, from..inline.text.len(), &mut found);
    if !found.is_empty() {
        inline.links.extend(found);
        inline.links.sort_by_key(|(range, _)| range.start);
    }
}

fn scan(text: &str, within: Range<usize>, out: &mut Vec<(Range<usize>, String)>) {
    let bytes = text.as_bytes();
    let mut i = within.start;
    while i < within.end {
        let Some(rest) = text.get(i..within.end) else {
            i += 1;
            continue;
        };
        let starts = |scheme: &str| {
            rest.get(..scheme.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(scheme))
        };
        let (prefix, www) = if starts("https://") {
            (8, false)
        } else if starts("http://") {
            (7, false)
        } else if rest.starts_with("www.") {
            (4, true)
        } else {
            i += rest.chars().next().map_or(1, char::len_utf8);
            continue;
        };
        // After a space or the punctuation that opens a phrase; not inside a word, and not
        // as the destination of something written like a link (`](https://…`).
        let before = i.checked_sub(1).map(|p| bytes[p]);
        let opens = match before {
            None => true,
            Some(b' ' | b'\t' | b'\n' | b'*' | b'_' | b'~' | b'[') => true,
            Some(b'(') => i < 2 || bytes[i - 2] != b']',
            Some(_) => false,
        };
        if !opens {
            i += prefix;
            continue;
        }
        let raw = rest
            .find(|c: char| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>'))
            .unwrap_or(rest.len());
        let end = i + trimmed(&rest[..raw], prefix);
        let address = &text[i..end];
        if valid_host(&address[prefix..], www) {
            let url = if www {
                format!("http://{address}")
            } else {
                address.to_owned()
            };
            out.push((i..end, url));
            i = end;
        } else {
            i += prefix;
        }
    }
}

/// The length of `address` without the punctuation after it: a full stop, a comma, a
/// closing bracket its own text never opened, an entity (`&amp;`).
fn trimmed(address: &str, prefix: usize) -> usize {
    let mut end = address.len();
    while end > prefix {
        let slice = &address[..end];
        let Some(last) = slice.chars().next_back() else {
            break;
        };
        let unopened = |open: u8, close: u8| {
            let count = |b: u8| slice[prefix..].bytes().filter(|x| *x == b).count();
            count(close) > count(open)
        };
        match last {
            '?' | '!' | '.' | ',' | ':' | '*' | '_' | '~' | '\'' | '"' => end -= last.len_utf8(),
            ')' if unopened(b'(', b')') => end -= 1,
            ']' if unopened(b'[', b']') => end -= 1,
            ';' => {
                let body = &slice[..end - 1];
                let entity = body.rfind('&').filter(|amp| {
                    let name = &body[amp + 1..];
                    *amp >= prefix
                        && !name.is_empty()
                        && name.bytes().all(|b| b.is_ascii_alphanumeric())
                });
                end = entity.unwrap_or(end - 1);
            }
            _ => break,
        }
    }
    end
}

/// Whether what follows the scheme (or `www.`) begins with a host name.
fn valid_host(rest: &str, www: bool) -> bool {
    let host = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    let host = match host.rsplit_once(':') {
        Some((name, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => name,
        _ => host,
    };
    let labels: Vec<&str> = host.split('.').collect();
    let plain = |label: &&str| {
        !label.is_empty()
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    };
    host.bytes()
        .next()
        .is_some_and(|b| b.is_ascii_alphanumeric())
        && !host.ends_with('-')
        && labels.iter().all(plain)
        && (!www || labels.len() > 1)
        && !labels
            .iter()
            .rev()
            .take(2)
            .any(|l| labels.len() > 1 && l.contains('_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one paragraph `src` parses to.
    fn para(src: &str) -> Inline {
        match parse(src).as_slice() {
            [Block::Para(inline)] => inline.clone(),
            other => panic!("{src:?} is not one paragraph: {other:?}"),
        }
    }

    /// The text of `inline`'s runs in `style`.
    fn styled(inline: &Inline, style: Style) -> Vec<&str> {
        inline
            .runs
            .iter()
            .filter(|(_, s)| *s == style)
            .map(|(range, _)| &inline.text[range.clone()])
            .collect()
    }

    /// `inline`'s links: their text and where they go.
    fn links(inline: &Inline) -> Vec<(&str, &str)> {
        inline
            .links
            .iter()
            .map(|(range, url)| (&inline.text[range.clone()], url.as_str()))
            .collect()
    }

    /// What `src` reads as when all of it is text: one paragraph, nothing styled or linked.
    fn typed(src: &str) -> String {
        let inline = para(src);
        assert!(
            inline.runs.is_empty(),
            "{src:?} has runs: {:?}",
            inline.runs
        );
        assert!(
            inline.links.is_empty(),
            "{src:?} has links: {:?}",
            inline.links
        );
        inline.text
    }

    fn plain(text: &str) -> Block {
        Block::Para(Inline {
            text: text.into(),
            ..Inline::default()
        })
    }

    #[test]
    fn inline_styles_cover_their_text() {
        let p = para("An *emphasis*, a **strong**, a ~~strike~~ and some `code`.");
        assert_eq!(p.plain(), "An emphasis, a strong, a strike and some code.");
        assert_eq!(styled(&p, Style::Emphasis), ["emphasis"]);
        assert_eq!(styled(&p, Style::Strong), ["strong"]);
        assert_eq!(styled(&p, Style::Strike), ["strike"]);
        assert_eq!(styled(&p, Style::Code), ["code"]);
        assert_eq!(styled(&p, Style::Pad), [OPEN, CLOSE]);

        let nested = para("**strong with *emphasis* inside**");
        assert_eq!(
            styled(&nested, Style::Strong),
            ["strong with emphasis inside"]
        );
        assert_eq!(styled(&nested, Style::Emphasis), ["emphasis"]);

        // Code is never read for anything else.
        let code = para("`**not strong** <b> [x](https://example.com)`");
        assert_eq!(code.plain(), "**not strong** <b> [x](https://example.com)");
        assert!(styled(&code, Style::Strong).is_empty() && code.links.is_empty());
    }

    #[test]
    fn a_slice_of_a_paragraph_keeps_what_styles_it() {
        let p = para("a **strong\nline** and [a\nlink](https://example.com)");
        assert_eq!(p.text, "a strong\nline and a\nlink");
        let first = p.slice(0..8);
        assert_eq!(
            (first.text.as_str(), styled(&first, Style::Strong)),
            ("a strong", vec!["strong"])
        );
        let second = p.slice(9..19);
        assert_eq!(second.text, "line and a");
        assert_eq!(styled(&second, Style::Strong), ["line"]);
        assert_eq!(links(&second), [("a", "https://example.com")]);
        let third = p.slice(20..24);
        assert_eq!(links(&third), [("link", "https://example.com")]);
        assert!(third.runs.is_empty());
    }

    #[test]
    fn paragraphs_keep_the_line_breaks_that_were_typed() {
        assert_eq!(
            parse("one\ntwo\n\nthree  \nfour\\\nfive"),
            [plain("one\ntwo"), plain("three\nfour\nfive")]
        );
        assert_eq!(parse("  \n\n"), []);
    }

    #[test]
    fn escapes_entities_and_line_endings_read_as_the_characters_they_stand_for() {
        assert_eq!(
            typed("\\*not emphasis\\* &lt;b&gt; &amp; 2 \\> 1"),
            "*not emphasis* <b> & 2 > 1"
        );
        assert_eq!(
            parse("one\r\ntwo\r\n\r\n```\r\ncode\r\nmore\r\n```\r\n"),
            [
                plain("one\ntwo"),
                Block::Code {
                    lang: None,
                    text: "code\nmore".into()
                }
            ]
        );
        // Offsets are bytes; text before a run need not be ASCII.
        let p = para("naïve 日本語 **強い** `コード` https://example.com/日本");
        assert_eq!(styled(&p, Style::Strong), ["強い"]);
        assert_eq!(styled(&p, Style::Code), ["コード"]);
        assert_eq!(
            links(&p),
            [("https://example.com/日本", "https://example.com/日本")]
        );
    }

    #[test]
    fn a_heading_between_paragraphs_is_a_paragraph_between_them() {
        assert_eq!(
            parse("above\n# 1 of 3\nbelow\n\n> # quoted\n\n- # item"),
            [
                plain("above"),
                plain("# 1 of 3"),
                plain("below"),
                Block::Quote(vec![plain("# quoted")]),
                Block::List {
                    start: None,
                    items: vec![vec![plain("# item")]]
                },
            ]
        );
    }

    #[test]
    fn fenced_code_keeps_its_text_and_names_its_language() {
        assert_eq!(
            parse(
                "```rust,ignore\nfn main() {\n    let **x** = 1;\n}\n```\n\n~~~\n<b>plain</b>\n~~~"
            ),
            [
                Block::Code {
                    lang: Some("rust".into()),
                    text: "fn main() {\n    let **x** = 1;\n}".into()
                },
                Block::Code {
                    lang: None,
                    text: "<b>plain</b>".into()
                },
            ]
        );
        // A fence nobody closed runs to the end of the message.
        assert_eq!(
            parse("```\nopen"),
            [Block::Code {
                lang: None,
                text: "open".into()
            }]
        );
    }

    #[test]
    fn quotes_and_lists_nest() {
        assert_eq!(
            parse("> quoted\n>\n> - in a list\n\nafter"),
            [
                Block::Quote(vec![
                    plain("quoted"),
                    Block::List {
                        start: None,
                        items: vec![vec![plain("in a list")]]
                    },
                ]),
                plain("after"),
            ]
        );
        assert_eq!(
            parse("3. three\n4. four\n   - nested\n   - twice\n\n     with a paragraph"),
            [Block::List {
                start: Some(3),
                items: vec![
                    vec![plain("three")],
                    vec![
                        plain("four"),
                        Block::List {
                            start: None,
                            items: vec![
                                vec![plain("nested")],
                                vec![plain("twice"), plain("with a paragraph")],
                            ]
                        },
                    ],
                ]
            }]
        );
    }

    #[test]
    fn a_link_is_one_only_with_a_web_address() {
        let p = para("See [the plan](https://example.com/plan) or <http://example.com>.");
        assert_eq!(p.text, "See the plan or http://example.com.");
        assert_eq!(
            links(&p),
            [
                ("the plan", "https://example.com/plan"),
                ("http://example.com", "http://example.com"),
            ]
        );
        // Its text keeps its styles.
        let p = para("[**bold** and `code`](https://example.com)");
        assert_eq!(links(&p), [(p.text.as_str(), "https://example.com")]);
        assert_eq!(styled(&p, Style::Strong), ["bold"]);
        assert_eq!(styled(&p, Style::Code), ["code"]);

        for other in [
            "[run](javascript:alert(1))",
            "[file](file:///etc/passwd)",
            "[mail](mailto:ada@example.com)",
            "[call](tel:+15550100)",
            "[app](lumia://join/abc)",
            "[data](data:text/html;base64,PGI+)",
            "[relative](/settings)",
            "[bare](example.com)",
            "[empty]()",
            "[scheme only](https://)",
            "[spaced](<https://example.com/a b>)",
            "<ada@example.com>",
            "<ftp://example.com/file>",
        ] {
            assert_eq!(
                typed(other),
                other,
                "a link a client will not open stays as typed"
            );
        }
        // Around text, and with its own text unread.
        assert_eq!(
            typed("before [**x**](smb://host/share) after"),
            "before [**x**](smb://host/share) after"
        );
    }

    #[test]
    fn web_addresses_are_told_from_the_rest() {
        for web in [
            "https://example.com",
            "http://localhost:4000/x",
            "HTTPS://EXAMPLE.COM",
        ] {
            assert!(is_web(web), "{web}");
        }
        for other in [
            "",
            "https://",
            "http:/example.com",
            "//example.com",
            "example.com",
            " https://example.com",
            "https://exa mple.com",
            "https://example.com\n",
            "javascript:https://example.com",
            "file:///etc/passwd",
            "mailto:ada@example.com",
            "lumia://x",
        ] {
            assert!(!is_web(other), "{other:?}");
        }
    }

    #[test]
    fn bare_web_addresses_are_links() {
        let p = para(
            "Read https://example.com/a_(b), then www.example.org. Not http://, nor xhttps://example.com.",
        );
        assert_eq!(
            links(&p),
            [
                ("https://example.com/a_(b)", "https://example.com/a_(b)"),
                ("www.example.org", "http://www.example.org"),
            ]
        );
        // Not inside code, not twice inside a link, and not the address of what stays as typed.
        assert!(para("`https://example.com`").links.is_empty());
        assert_eq!(
            links(&para("[https://example.com](https://example.org)")),
            [("https://example.com", "https://example.org")]
        );
        assert!(para("![cat](https://example.com/cat.png)").links.is_empty());
        // In a list item and a quote too.
        let blocks = parse("- https://example.com\n\n> (https://example.org)");
        let [Block::List { items, .. }, Block::Quote(quoted)] = blocks.as_slice() else {
            panic!("{blocks:?}");
        };
        let (Block::Para(item), Block::Para(quote)) = (&items[0][0], &quoted[0]) else {
            panic!("{blocks:?}");
        };
        assert_eq!(
            links(item),
            [("https://example.com", "https://example.com")]
        );
        assert_eq!(
            links(quote),
            [("https://example.org", "https://example.org")]
        );
    }

    #[test]
    fn what_the_subset_does_not_have_stays_as_typed() {
        // Headings, both ways of writing one.
        assert_eq!(typed("# Not a heading"), "# Not a heading");
        assert_eq!(typed("### Closed ###"), "### Closed ###");
        assert_eq!(typed("#"), "#");
        assert_eq!(typed("Title\n====="), "Title\n=====");
        assert_eq!(typed("Title\n-----"), "Title\n-----");
        // The text of one is still the subset's.
        let heading = para("## of **course** `it` is");
        assert_eq!(heading.plain(), "## of course it is");
        assert_eq!(styled(&heading, Style::Strong), ["course"]);
        assert_eq!(styled(&heading, Style::Code), ["it"]);
        let heading = para("# **all strong**");
        assert_eq!(heading.text, "# all strong");
        assert_eq!(styled(&heading, Style::Strong), ["all strong"]);

        // Raw HTML, inline and as a block.
        assert_eq!(
            typed("<b>not bold</b> and <img src=x onerror=alert(1)>"),
            "<b>not bold</b> and <img src=x onerror=alert(1)>"
        );
        assert_eq!(
            typed("<div>\n<script>alert(1)</script>\n</div>"),
            "<div>\n<script>alert(1)</script>\n</div>"
        );
        assert_eq!(typed("<!-- a comment -->"), "<!-- a comment -->");

        // Images: nothing a message names is fetched, so there is nothing to draw.
        assert_eq!(
            typed("![a cat](https://example.com/cat.png \"title\")"),
            "![a cat](https://example.com/cat.png \"title\")"
        );
        assert_eq!(
            typed("look: ![*alt*](cat.png)!"),
            "look: ![*alt*](cat.png)!"
        );
        let linked = para("[![cat](https://example.com/cat.png)](https://example.com)");
        assert_eq!(
            links(&linked),
            [("![cat](https://example.com/cat.png)", "https://example.com")]
        );

        // Tables, rules, task lists, footnotes, indented code.
        assert_eq!(
            typed("| not | a table |\n|-----|---------|\n| 1 | 2 |"),
            "| not | a table |\n|-----|---------|\n| 1 | 2 |"
        );
        assert_eq!(
            parse("above\n\n---\n\n***"),
            [plain("above"), plain("---"), plain("***")]
        );
        assert_eq!(
            parse("- [ ] to do\n- [x] done"),
            [Block::List {
                start: None,
                items: vec![vec![plain("[ ] to do")], vec![plain("[x] done")]]
            }]
        );
        assert_eq!(typed("a note[^1]"), "a note[^1]");
        assert_eq!(
            typed("    four spaces\n    **in**"),
            "    four spaces\n    **in**"
        );
        // One tilde is a tilde.
        assert_eq!(typed("~about~ 5"), "~about~ 5");
        assert_eq!(typed("H~2~O and x^2^ and $x$"), "H~2~O and x^2^ and $x$");
    }

    #[test]
    fn a_reference_and_its_definition_stay_as_typed() {
        assert_eq!(
            parse("See [the plan][1] and [1].\n\n[1]: https://example.com/plan"),
            [
                plain("See [the plan][1] and [1]."),
                Block::Para(Inline {
                    text: "[1]: https://example.com/plan".into(),
                    runs: Vec::new(),
                    links: vec![(5..29, "https://example.com/plan".into())],
                    mentions: Vec::new(),
                }),
            ]
        );
        // CommonMark reads this as a definition and shows nothing.
        assert_eq!(
            typed("[sic]: <javascript:alert(1)>"),
            "[sic]: <javascript:alert(1)>"
        );
        assert_eq!(
            parse("[a]: /first\nthen text\n\n[b]: /second"),
            [
                plain("[a]: /first"),
                plain("then text"),
                plain("[b]: /second")
            ]
        );
    }

    #[test]
    fn a_body_that_is_not_blank_never_parses_to_nothing() {
        for src in [
            ">",
            "> ",
            ">>>",
            "-",
            "1.",
            "*",
            "```",
            "<!---->",
            "[x]: y",
            "\\",
            "&nbsp;",
            "#",
            "===",
            "\u{feff}x",
        ] {
            assert!(!parse(src).is_empty(), "{src:?} shows nothing");
        }
        assert_eq!(typed(">"), ">");
        assert_eq!(typed(">>>"), ">>>");
    }

    #[test]
    fn nesting_stops_at_a_depth() {
        fn depth(blocks: &[Block]) -> usize {
            blocks
                .iter()
                .map(|b| match b {
                    Block::Quote(inner) => 1 + depth(inner),
                    Block::List { items, .. } => {
                        1 + items.iter().map(|item| depth(item)).max().unwrap_or(0)
                    }
                    _ => 0,
                })
                .max()
                .unwrap_or(0)
        }
        fn text(blocks: &[Block], out: &mut String) {
            for b in blocks {
                match b {
                    Block::Para(inline) => out.push_str(&inline.text),
                    Block::Code { text, .. } => out.push_str(text),
                    Block::Quote(inner) => text(inner, out),
                    Block::List { items, .. } => items.iter().for_each(|item| text(item, out)),
                }
            }
        }
        let quotes = format!("{} deep", ">".repeat(3_000));
        let lists = format!("{}deep", "- ".repeat(3_000));
        for src in [quotes, lists] {
            let blocks = parse(&src);
            assert!(depth(&blocks) <= MAX_NESTING, "{} levels", depth(&blocks));
            let mut shown = String::new();
            text(&blocks, &mut shown);
            assert!(
                shown.ends_with("deep"),
                "the text is still there: {shown:?}"
            );
        }
    }
}
