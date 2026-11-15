//! Link cards (media spec 6), the parts that are the same wherever a card is built: which
//! links in a message get one ([`card_links`]), what a page's head gives for it
//! ([`parse_head`]), which addresses a fetcher may connect to ([`address_allowed`]), and the
//! site a receiver names under a card ([`host_for_display`]).
//!
//! No I/O here. `lumia-client` fetches natively by these rules, and the instance builds
//! server content's cards by the same ones; `proto/vectors/cards/` holds the cases both
//! must agree on.

mod address;
mod head;

pub use address::address_allowed;
pub use head::parse_head;

use crate::body::{parse, Block, Inline};

/// The most cards a message has (6.1).
pub const MAX_CARDS: usize = 3;
/// The longest title, in characters as a person counts them (grapheme clusters), its
/// ellipsis included.
pub const MAX_TITLE: usize = 300;
/// The longest description, counted as [`MAX_TITLE`] is.
pub const MAX_DESCRIPTION: usize = 1_000;
/// The longest site name, counted as [`MAX_TITLE`] is. The spec bounds only the title and
/// description; a site name is held to the title's bound so no field is unbounded.
pub const MAX_SITE_NAME: usize = MAX_TITLE;
/// The most of a page's HTML read and considered (6.3).
pub const MAX_HTML_BYTES: usize = 1 << 20;
/// The largest card image fetched (6.3).
pub const MAX_IMAGE_BYTES: usize = 5 << 20;
/// The most redirects a fetch follows (6.3).
pub const MAX_REDIRECTS: usize = 5;

/// A card as a page's head gives it, before its image is fetched. The fields are the
/// protocol's `Card`'s, empty (or 0) where the page gave nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CardDraft {
    /// The page's address, as [`parse_head`] was given it.
    pub url: String,
    /// Never empty: a page without a title has no card.
    pub title: String,
    pub description: String,
    pub site_name: String,
    /// `#rrggbb`, lowercase, from `theme-color`.
    pub color: String,
    /// An absolute `http` or `https` address.
    pub image_url: String,
    pub image_width: u32,
    pub image_height: u32,
    /// The page is a video (`og:video…` or an `og:type` of `video…`).
    pub video: bool,
}

/// The links in a message body that may have cards (6.1): up to [`MAX_CARDS`] web
/// addresses in order of appearance, each once. They are the links a client draws
/// ([`crate::body::parse`]), so never inside inline code or a code block, without those
/// written in angle brackets (`<https://…>`, which means "no card"), and without those whose
/// address is not in the text as written (`www.example.com`, which is drawn as a link to
/// `http://www.example.com`): a receiver draws a card only under an address the text holds.
pub fn card_links(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    collect(&parse(body), body, &mut out);
    out
}

fn collect(blocks: &[Block], src: &str, out: &mut Vec<String>) {
    for block in blocks {
        match block {
            Block::Para(inline) => from_inline(inline, src, out),
            Block::Code { .. } => {}
            Block::Quote(blocks) => collect(blocks, src, out),
            Block::List { items, .. } => {
                for item in items {
                    collect(item, src, out);
                }
            }
        }
    }
}

fn from_inline(inline: &Inline, src: &str, out: &mut Vec<String>) {
    for (range, url) in &inline.links {
        if out.len() >= MAX_CARDS {
            return;
        }
        if inline.bracketed.contains(range) || !src.contains(url.as_str()) || out.contains(url) {
            continue;
        }
        out.push(url.clone());
    }
}

/// The site a receiver names on a card (6.5): the host of the link in the message text,
/// lowercase and without a leading `www.`; never the card's `site_name`, which a sender
/// could forge. `None` for anything but an `http` or `https` address with a host.
pub fn host_for_display(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed
        .host_str()?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let host = match host.strip_prefix("www.") {
        Some(rest) if rest.contains('.') => rest.to_owned(),
        _ => host,
    };
    (!host.is_empty()).then_some(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_is_named_from_the_link() {
        assert_eq!(
            host_for_display("https://www.Example.COM/a?b"),
            Some("example.com".into())
        );
        assert_eq!(
            host_for_display("http://news.example.co.uk:443/"),
            Some("news.example.co.uk".into())
        );
        assert_eq!(host_for_display("https://www.com/"), Some("www.com".into()));
        assert_eq!(
            host_for_display("https://[2001:db8::1]/"),
            Some("[2001:db8::1]".into())
        );
        assert_eq!(
            host_for_display("https://bücher.example/"),
            Some("xn--bcher-kva.example".into())
        );
        assert_eq!(host_for_display("ftp://example.com/"), None);
        assert_eq!(host_for_display("javascript:alert(1)"), None);
        assert_eq!(host_for_display("not a url"), None);
    }

    #[test]
    fn links_inside_quotes_and_lists_are_found() {
        assert_eq!(
            card_links("> https://a.example\n\n1. https://b.example"),
            ["https://a.example", "https://b.example"]
        );
    }
}
