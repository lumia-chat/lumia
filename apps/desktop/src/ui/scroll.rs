//! Scroll bars. Every pane that scrolls has one: a thin thumb over the pane's end edge that
//! shows how far it has scrolled, can be dragged, and pages or jumps on a press in its track.
//! It is the kit's bar, so it follows the system's "Show scroll bars" setting: there whenever
//! the content overflows under "Always" (or with a mouse), and over the content while it
//! scrolls or the pointer is at the edge, then gone, under "When scrolling" (or with a
//! trackpad). The boards draw none, a browser's own being what they have; the thumb's
//! colours are derived in `tokens`.

use rune::El;
use rune_kit::{ScrollBar, ScrollBarStyle};

use super::components::Look;
use crate::tokens as t;

/// What a scroll viewport gains from [`WithScrollBar::with_scroll_bar`].
pub trait WithScrollBar {
    /// This viewport (an element with `.scroll(..)` and a key) with a scroll bar over its end
    /// edge. Called last: the bar's frame takes the viewport's size and place in its parent.
    fn with_scroll_bar(self, look: Look) -> El;
}

impl WithScrollBar for El {
    fn with_scroll_bar(self, look: Look) -> El {
        let (thumb, held) = t::scroll_thumb(look.c);
        self.scroll_bar_styled(
            ScrollBarStyle::neutral(look.dark)
                .thumb(thumb)
                .thumb_hover(held),
        )
    }
}
