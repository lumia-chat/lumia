//! A page's `<head>` to a card (media spec 6.2), with a small tolerant tag scanner: a card
//! needs a handful of `<meta>` tags, a `<title>` and a `<base>`, and a page is someone
//! else's bytes, so nothing here builds a tree or fails on what it does not expect.

use std::borrow::Cow;

use unicode_segmentation::UnicodeSegmentation;

use super::{CardDraft, MAX_DESCRIPTION, MAX_HTML_BYTES, MAX_SITE_NAME, MAX_TITLE};

/// The card `html`'s head gives for the page at `page_url`, or `None` when it has no title.
///
/// Only the head is read: scanning stops at `</head>` or `<body>`, and at
/// [`MAX_HTML_BYTES`]. Each field comes from Open Graph, failing that from Twitter's tags,
/// failing those from `<title>` and `<meta name="description">`, whichever of `property=`
/// and `name=` names them. Text is decoded (entities), its whitespace collapsed, and cut
/// with an ellipsis past its bound. The image is resolved against the page (or its
/// `<base href>`) and kept only when it is `http` or `https`; `og:image:secure_url` is
/// preferred when it is `https`. Bytes that are not UTF-8 are read with U+FFFD in their
/// place.
pub fn parse_head(html: &[u8], page_url: &str) -> Option<CardDraft> {
    let html = html.get(..MAX_HTML_BYTES).unwrap_or(html);
    let text = String::from_utf8_lossy(html);
    let head = scan(&text);
    build(&head, page_url)
}

/// Whether the head of `html`, a page's start as far as it has arrived, has ended: whether
/// [`parse_head`] would stop at a `</head>` or `<body>` in it rather than at its end. A
/// fetcher reads until this says so (or [`MAX_HTML_BYTES`]); unlike a search for the bytes
/// `</head`, it reads past one in a comment, a script or an attribute, as `parse_head` does.
/// A tag name `html` ends in is not taken for one: `</head` may yet be `</header`.
///
/// It reads all of `html` each time, so a fetcher asks only when what has just arrived
/// holds `</head` or `<body` in some case.
pub fn head_ended(html: &[u8]) -> bool {
    let html = html.get(..MAX_HTML_BYTES).unwrap_or(html);
    scan(&String::from_utf8_lossy(html)).ended
}

/// What the scanner keeps of a head.
#[derive(Default)]
struct Head {
    title: Option<String>,
    base: Option<String>,
    /// Each `<meta>`'s key (its `property`, and its `name` when that differs, lowercase)
    /// and `content`, decoded, in document order.
    metas: Vec<(String, String)>,
    /// Scanning stopped at `</head>` or `<body>`, not at the end of the text.
    ended: bool,
}

impl Head {
    /// The first meta named `key` with text left after cleaning, cleaned.
    fn text(&self, key: &str, max: usize) -> Option<String> {
        self.metas
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| clean(v, max))
            .find(|v| !v.is_empty())
    }
}

fn build(head: &Head, page_url: &str) -> Option<CardDraft> {
    let title = head
        .text("og:title", MAX_TITLE)
        .or_else(|| head.text("twitter:title", MAX_TITLE))
        .or_else(|| {
            head.title
                .as_deref()
                .map(|t| clean(t, MAX_TITLE))
                .filter(|t| !t.is_empty())
        })?;
    let description = head
        .text("og:description", MAX_DESCRIPTION)
        .or_else(|| head.text("twitter:description", MAX_DESCRIPTION))
        .or_else(|| head.text("description", MAX_DESCRIPTION))
        .unwrap_or_default();
    let site_name = head.text("og:site_name", MAX_SITE_NAME).unwrap_or_default();
    let color = head
        .metas
        .iter()
        .filter(|(k, _)| k == "theme-color")
        .find_map(|(_, v)| color(v))
        .unwrap_or_default();

    let page = url::Url::parse(page_url.trim()).ok();
    let base = match (&page, &head.base) {
        (Some(page), Some(href)) => page.join(href.trim()).ok().or_else(|| Some(page.clone())),
        (page, _) => page.clone(),
    };
    let resolve = |href: &str| web(base.as_ref(), href);
    let og = og_image(head);
    let (image_url, image_width, image_height) = og
        .as_ref()
        .and_then(|og| {
            let secure = og
                .secure
                .as_deref()
                .and_then(resolve)
                .filter(|u| u.starts_with("https:"));
            let url = secure.or_else(|| og.url.as_deref().and_then(resolve))?;
            Some((url, og.width, og.height))
        })
        .or_else(|| {
            head.metas
                .iter()
                .filter(|(k, _)| k == "twitter:image" || k == "twitter:image:src")
                .find_map(|(_, v)| resolve(v))
                .map(|url| (url, 0, 0))
        })
        .unwrap_or_default();

    let video = head.metas.iter().any(|(k, v)| {
        k.starts_with("og:video")
            || (k == "og:type" && v.trim().to_ascii_lowercase().starts_with("video"))
    });

    Some(CardDraft {
        url: page_url.to_owned(),
        title,
        description,
        site_name,
        color,
        image_url,
        image_width,
        image_height,
        video,
    })
}

/// The first Open Graph image and what its structured properties say of it.
#[derive(Default)]
struct OgImage {
    url: Option<String>,
    secure: Option<String>,
    width: u32,
    height: u32,
}

/// The first `og:image` (or `og:image:url`) with the `og:image:…` properties that follow it
/// up to the next image: those describe the first image, those after another describe that.
fn og_image(head: &Head) -> Option<OgImage> {
    let mut image: Option<OgImage> = None;
    for (key, value) in &head.metas {
        let value = value.trim();
        match (key.as_str(), image.as_mut()) {
            ("og:image" | "og:image:url", None) => {
                image = Some(OgImage {
                    url: Some(value.to_owned()),
                    ..OgImage::default()
                });
            }
            ("og:image" | "og:image:url", Some(_)) => break,
            ("og:image:secure_url", Some(image)) if image.secure.is_none() => {
                image.secure = Some(value.to_owned());
            }
            ("og:image:width", Some(image)) if image.width == 0 => {
                image.width = value.parse().unwrap_or(0);
            }
            ("og:image:height", Some(image)) if image.height == 0 => {
                image.height = value.parse().unwrap_or(0);
            }
            _ => {}
        }
    }
    image
}

/// `href` resolved against `base`, if it is then an `http` or `https` address.
fn web(base: Option<&url::Url>, href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() {
        return None;
    }
    let url = url::Url::options().base_url(base).parse(href).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.into())
}

/// `#rrggbb`, lowercase, from `#rgb` or `#rrggbb` in any case.
fn color(value: &str) -> Option<String> {
    let hex = value.trim().strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let hex = hex.to_ascii_lowercase();
    match hex.len() {
        3 => Some(hex.chars().fold(String::from("#"), |mut out, c| {
            out.push(c);
            out.push(c);
            out
        })),
        6 => Some(format!("#{hex}")),
        _ => None,
    }
}

/// Text as a card holds it: whitespace runs as one space, no other control characters,
/// trimmed, and at most `max` characters, the last an ellipsis when cut.
fn clean(text: &str, max: usize) -> String {
    let joined = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(|c: char| c.is_control(), "");
    let mut graphemes = joined.grapheme_indices(true);
    let Some((cut, _)) = graphemes.nth(max.saturating_sub(1)) else {
        return joined;
    };
    if graphemes.next().is_none() {
        return joined;
    }
    let mut out = joined[..cut].trim_end().to_owned();
    out.push('…');
    out
}

// The scanner.

/// Reads the head of `text`: its `<meta>`s, its first `<title>` and `<base href>`.
fn scan(text: &str) -> Head {
    let mut head = Head::default();
    let mut i = 0;
    while let Some(off) = text.get(i..).and_then(|rest| rest.find('<')) {
        let at = i + off;
        let rest = &text[at..];
        if rest.starts_with("<!--") {
            i = find(text, at + 4, "-->").map_or(text.len(), |end| end + 3);
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            i = find(text, at, ">").map_or(text.len(), |end| end + 1);
            continue;
        }
        if rest.starts_with("</") {
            let (name, after) = tag_name(text, at + 2);
            if name.is_empty() {
                i = at + 2;
                continue;
            }
            if name == "head" {
                // A name the text ends in may go on (`</header`).
                head.ended = after < text.len();
                break;
            }
            i = find(text, after, ">").map_or(text.len(), |end| end + 1);
            continue;
        }
        let (name, after) = tag_name(text, at + 1);
        if name.is_empty() {
            i = at + 1;
            continue;
        }
        let (attrs, end) = attributes(text, after);
        i = end;
        match name.as_str() {
            "body" => {
                head.ended = after < text.len();
                break;
            }
            "meta" => meta(&attrs, &mut head),
            "base" if head.base.is_none() => {
                head.base = attr(&attrs, "href").map(str::to_owned);
            }
            "title" => {
                let (raw, next) = raw_text(text, end, "title");
                if head.title.is_none() {
                    head.title = Some(decode(raw).into_owned());
                }
                i = next;
            }
            // Text whose tags are not tags.
            "script" | "style" | "template" | "textarea" | "xmp" | "iframe" | "noembed"
            | "noframes" => i = raw_text(text, end, &name).1,
            _ => {}
        }
    }
    head
}

fn meta(attrs: &[(String, String)], head: &mut Head) {
    let Some(content) = attr(attrs, "content") else {
        return;
    };
    let key = |name: &str| {
        attr(attrs, name)
            .map(|k| k.trim().to_ascii_lowercase())
            .filter(|k| !k.is_empty())
    };
    let property = key("property");
    let name = key("name");
    if let Some(property) = &property {
        head.metas.push((property.clone(), content.to_owned()));
    }
    if let Some(name) = name.filter(|n| Some(n) != property.as_ref()) {
        head.metas.push((name, content.to_owned()));
    }
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

/// Where `needle` (ASCII) next starts in `text` from `from`, in any case.
fn find(text: &str, from: usize, needle: &str) -> Option<usize> {
    let hay = text.as_bytes().get(from..)?;
    let needle = needle.as_bytes();
    hay.windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle))
        .map(|p| from + p)
}

/// The tag name starting at `at`, lowercase, and where it ends; empty when what is there
/// is not a tag name (a tag name starts with an ASCII letter).
fn tag_name(text: &str, at: usize) -> (String, usize) {
    let bytes = text.as_bytes();
    if !bytes.get(at).is_some_and(u8::is_ascii_alphabetic) {
        return (String::new(), at);
    }
    let mut end = at;
    while bytes
        .get(end)
        .is_some_and(|b| !b.is_ascii_whitespace() && !matches!(b, b'/' | b'>'))
    {
        end += 1;
    }
    (text[at..end].to_ascii_lowercase(), end)
}

/// The attributes from `at` to the end of the tag, names lowercase and values decoded, and
/// where the tag ends (after its `>`, or the end of the text).
fn attributes(text: &str, mut at: usize) -> (Vec<(String, String)>, usize) {
    let bytes = text.as_bytes();
    let mut attrs = Vec::new();
    loop {
        while bytes
            .get(at)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'/')
        {
            at += 1;
        }
        match bytes.get(at) {
            None => return (attrs, text.len()),
            Some(b'>') => return (attrs, at + 1),
            Some(_) => {}
        }
        let start = at;
        // A name may begin with `=`; after that, `=` ends it.
        at += 1;
        while bytes
            .get(at)
            .is_some_and(|b| !b.is_ascii_whitespace() && !matches!(b, b'/' | b'>' | b'='))
        {
            at += 1;
        }
        let name = text[start..at].to_ascii_lowercase();
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        let mut value = String::new();
        if bytes.get(at) == Some(&b'=') {
            at += 1;
            while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                at += 1;
            }
            match bytes.get(at) {
                Some(&quote @ (b'"' | b'\'')) => {
                    let from = at + 1;
                    let to = text
                        .get(from..)
                        .and_then(|rest| rest.find(char::from(quote)))
                        .map_or(text.len(), |p| from + p);
                    value = decode(&text[from..to]).into_owned();
                    at = (to + 1).min(text.len());
                }
                _ => {
                    let from = at;
                    while bytes
                        .get(at)
                        .is_some_and(|b| !b.is_ascii_whitespace() && *b != b'>')
                    {
                        at += 1;
                    }
                    value = decode(&text[from..at]).into_owned();
                }
            }
        }
        attrs.push((name, value));
    }
}

/// The text of an element whose content is not markup, from `at` to its `</name`, and where
/// scanning goes on (after the end tag's `>`, or the end of the text).
fn raw_text<'a>(text: &'a str, at: usize, name: &str) -> (&'a str, usize) {
    let close = format!("</{name}");
    let mut from = at;
    loop {
        let Some(end) = find(text, from, &close) else {
            return (text.get(at..).unwrap_or_default(), text.len());
        };
        // `</titles` does not close `<title>`.
        let after = text.as_bytes().get(end + close.len());
        if after.is_none_or(|b| b.is_ascii_whitespace() || matches!(b, b'/' | b'>')) {
            let next = find(text, end, ">").map_or(text.len(), |gt| gt + 1);
            return (&text[at..end], next);
        }
        from = end + close.len();
    }
}

// Character references.

/// `text` with its character references decoded: `&#…;`, `&#x…;` and the named ones in
/// [`named`]. Each must end with `;`; anything else is left as written.
fn decode(text: &str) -> Cow<'_, str> {
    if !text.contains('&') {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        match reference(rest, &mut out) {
            Some(len) => rest = &rest[len..],
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// Decodes the reference `text` starts with (at its `&`) onto `out`, and gives its length;
/// `None`, with nothing written, when it is not one.
fn reference(text: &str, out: &mut String) -> Option<usize> {
    let window = text.as_bytes().get(..text.len().min(40))?;
    let semi = window.iter().position(|b| *b == b';')?;
    let body = text.get(1..semi)?;
    if let Some(number) = body.strip_prefix('#') {
        let code = match number.strip_prefix(['x', 'X']) {
            Some(hex) if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) => {
                u32::from_str_radix(hex, 16).unwrap_or(u32::MAX)
            }
            None if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) => {
                number.parse().unwrap_or(u32::MAX)
            }
            _ => return None,
        };
        out.push(numeric(code));
    } else {
        out.push_str(named(body)?);
    }
    Some(semi + 1)
}

/// What a numeric reference stands for, as HTML reads it: 0, a surrogate or past
/// U+10FFFF is U+FFFD, and 0x80–0x9F are Windows-1252's characters.
fn numeric(code: u32) -> char {
    const WINDOWS_1252: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    if let Some(c) = code
        .checked_sub(0x80)
        .and_then(|i| WINDOWS_1252.get(usize::try_from(i).ok()?))
    {
        return *c;
    }
    char::from_u32(code)
        .filter(|c| *c != '\0')
        .unwrap_or('\u{fffd}')
}

/// The named references a card is likely to meet: HTML's five, Latin-1's (U+00A0 to
/// U+00FF) and the common typographic ones. HTML's full table has over two thousand; a
/// name outside these is left as written.
fn named(name: &str) -> Option<&'static str> {
    const NAMES: [(&str, &str); 133] = [
        ("amp", "&"),
        ("lt", "<"),
        ("gt", ">"),
        ("quot", "\""),
        ("apos", "'"),
        ("nbsp", "\u{a0}"),
        ("iexcl", "¡"),
        ("cent", "¢"),
        ("pound", "£"),
        ("curren", "¤"),
        ("yen", "¥"),
        ("brvbar", "¦"),
        ("sect", "§"),
        ("uml", "¨"),
        ("copy", "©"),
        ("ordf", "ª"),
        ("laquo", "«"),
        ("not", "¬"),
        ("shy", "\u{ad}"),
        ("reg", "®"),
        ("macr", "¯"),
        ("deg", "°"),
        ("plusmn", "±"),
        ("sup2", "²"),
        ("sup3", "³"),
        ("acute", "´"),
        ("micro", "µ"),
        ("para", "¶"),
        ("middot", "·"),
        ("cedil", "¸"),
        ("sup1", "¹"),
        ("ordm", "º"),
        ("raquo", "»"),
        ("frac14", "¼"),
        ("frac12", "½"),
        ("frac34", "¾"),
        ("iquest", "¿"),
        ("Agrave", "À"),
        ("Aacute", "Á"),
        ("Acirc", "Â"),
        ("Atilde", "Ã"),
        ("Auml", "Ä"),
        ("Aring", "Å"),
        ("AElig", "Æ"),
        ("Ccedil", "Ç"),
        ("Egrave", "È"),
        ("Eacute", "É"),
        ("Ecirc", "Ê"),
        ("Euml", "Ë"),
        ("Igrave", "Ì"),
        ("Iacute", "Í"),
        ("Icirc", "Î"),
        ("Iuml", "Ï"),
        ("ETH", "Ð"),
        ("Ntilde", "Ñ"),
        ("Ograve", "Ò"),
        ("Oacute", "Ó"),
        ("Ocirc", "Ô"),
        ("Otilde", "Õ"),
        ("Ouml", "Ö"),
        ("times", "×"),
        ("Oslash", "Ø"),
        ("Ugrave", "Ù"),
        ("Uacute", "Ú"),
        ("Ucirc", "Û"),
        ("Uuml", "Ü"),
        ("Yacute", "Ý"),
        ("THORN", "Þ"),
        ("szlig", "ß"),
        ("agrave", "à"),
        ("aacute", "á"),
        ("acirc", "â"),
        ("atilde", "ã"),
        ("auml", "ä"),
        ("aring", "å"),
        ("aelig", "æ"),
        ("ccedil", "ç"),
        ("egrave", "è"),
        ("eacute", "é"),
        ("ecirc", "ê"),
        ("euml", "ë"),
        ("igrave", "ì"),
        ("iacute", "í"),
        ("icirc", "î"),
        ("iuml", "ï"),
        ("eth", "ð"),
        ("ntilde", "ñ"),
        ("ograve", "ò"),
        ("oacute", "ó"),
        ("ocirc", "ô"),
        ("otilde", "õ"),
        ("ouml", "ö"),
        ("divide", "÷"),
        ("oslash", "ø"),
        ("ugrave", "ù"),
        ("uacute", "ú"),
        ("ucirc", "û"),
        ("uuml", "ü"),
        ("yacute", "ý"),
        ("thorn", "þ"),
        ("yuml", "ÿ"),
        ("OElig", "Œ"),
        ("oelig", "œ"),
        ("Scaron", "Š"),
        ("scaron", "š"),
        ("Yuml", "Ÿ"),
        ("fnof", "ƒ"),
        ("circ", "ˆ"),
        ("tilde", "˜"),
        ("ensp", "\u{2002}"),
        ("emsp", "\u{2003}"),
        ("thinsp", "\u{2009}"),
        ("zwnj", "\u{200c}"),
        ("zwj", "\u{200d}"),
        ("lrm", "\u{200e}"),
        ("rlm", "\u{200f}"),
        ("ndash", "–"),
        ("mdash", "—"),
        ("lsquo", "‘"),
        ("rsquo", "’"),
        ("sbquo", "‚"),
        ("ldquo", "“"),
        ("rdquo", "”"),
        ("bdquo", "„"),
        ("dagger", "†"),
        ("Dagger", "‡"),
        ("bull", "•"),
        ("hellip", "…"),
        ("permil", "‰"),
        ("lsaquo", "‹"),
        ("rsaquo", "›"),
        ("euro", "€"),
        ("trade", "™"),
    ];
    NAMES.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_decode_and_the_rest_stays_as_written() {
        assert_eq!(
            decode("Tom &amp; Jerry&#39;s &#x1F600; caf&eacute; &#150; &bogus; & &amp"),
            "Tom & Jerry's 😀 café – &bogus; & &amp"
        );
        assert_eq!(
            decode("&#0;&#xD800;&#x110000;&#;"),
            "\u{fffd}\u{fffd}\u{fffd}&#;"
        );
        assert_eq!(decode("&é;&amp;"), "&é;&");
    }

    #[test]
    fn text_is_collapsed_and_cut_with_an_ellipsis() {
        assert_eq!(clean("  a \n\t b\u{a0}c  ", 300), "a b c");
        assert_eq!(clean("abcdef", 6), "abcdef");
        assert_eq!(clean("abcdefg", 6), "abcde…");
        assert_eq!(clean("abcd efg", 6), "abcd…");
        assert_eq!(clean("👨‍👩‍👧👨‍👩‍👧👨‍👩‍👧", 2), "👨‍👩‍👧…");
    }

    #[test]
    fn colors_are_normalized_or_dropped() {
        assert_eq!(color(" #ABC "), Some("#aabbcc".into()));
        assert_eq!(color("#1A2b3C"), Some("#1a2b3c".into()));
        assert_eq!(color("#12345"), None);
        assert_eq!(color("red"), None);
        assert_eq!(color("#ggg"), None);
    }

    #[test]
    fn a_page_cut_off_anywhere_is_read_as_far_as_it_goes() {
        assert_eq!(
            parse_head(b"<head><title>x", "https://a.example/"),
            Some(CardDraft {
                url: "https://a.example/".into(),
                title: "x".into(),
                ..CardDraft::default()
            })
        );
        assert_eq!(
            parse_head(b"<meta property=\"og:title\" content=\"", "x"),
            None
        );
        assert_eq!(parse_head(b"<", "x"), None);
        assert_eq!(parse_head(b"<title", "x"), None);
        assert_eq!(parse_head(b"<!--", "x"), None);
        assert_eq!(
            parse_head("<title>é</".as_bytes(), "x").map(|c| c.title),
            Some("é</".into())
        );
        assert_eq!(
            parse_head(b"<meta property=og:title content=a", "x").map(|c| c.title),
            Some("a".into())
        );
    }

    #[test]
    fn only_the_first_megabyte_is_read() {
        let mut html = b"<head>".to_vec();
        html.resize(MAX_HTML_BYTES, b' ');
        html.extend_from_slice(b"<title>late</title>");
        assert_eq!(parse_head(&html, "https://a.example/"), None);
    }

    #[test]
    fn the_head_ends_where_the_scanner_says_and_not_at_the_bytes() {
        for ended in [
            "<head><title>x</title></head>",
            "<head><title>x</title></HEAD >",
            "<head><title>x</title><body>",
            "<title>x</title><BODY class=a",
            "<head><!-- </head> --></head>",
            "<head><script>if (a</head>b) {}</script></head>",
        ] {
            assert!(head_ended(ended.as_bytes()), "{ended}");
        }
        for open in [
            "",
            "<head><title>x</title>",
            "<head><!-- </head> -->",
            "<head><!-- </head>",
            "<head><script>document.write('<body>')",
            "<head><script>x</script><meta content=\"</head><body>\"",
            "<head><title></head></title>",
            "<head><style>/* </head> */</style>",
            // Not yet a whole name: `</header`, `<bodyx` may follow.
            "<head></head",
            "<head><body",
            "<head></header><bodyx>",
        ] {
            assert!(!head_ended(open.as_bytes()), "{open}");
        }
    }
}
