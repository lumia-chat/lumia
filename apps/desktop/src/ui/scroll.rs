//! Scroll bars. Every pane that scrolls has one: a thin thumb over the pane's end edge that
//! shows how far it has scrolled, can be dragged, and pages or jumps on a press in its track.
//! It is the kit's bar, so it follows the system's "Show scroll bars" setting: there whenever
//! the content overflows under "Always" (or with a mouse), and over the content while it
//! scrolls or the pointer is at the edge, then gone, under "When scrolling" (or with a
//! trackpad). The boards draw none, a browser's own being what they have; the thumb's
//! colours are derived in `tokens`.

//!
//! Every pane also scrolls with the middle button held and the pointer moved away from where
//! it was pressed (`app::autoscroll`): the pane says where it has scrolled, and takes the
//! middle button's drags.

use rune::{Drag, El, Phase, PointerButton, ScrollPos, Tx};
use rune_kit::{ScrollBar, ScrollBarStyle};

use super::components::Look;
use crate::app::{State, autoscroll};
use crate::tokens as t;

/// What a scroll viewport gains from [`WithScrollBar::with_scroll_bar`].
pub trait WithScrollBar {
    /// This viewport (an element with `.scroll(..)` keyed `key`) with a scroll bar over its
    /// end edge, and autoscroll with the middle button. Called last: the bar's frame takes the
    /// viewport's size and place in its parent.
    fn with_scroll_bar(self, key: &str, look: Look) -> El;
}

impl WithScrollBar for El {
    fn with_scroll_bar(self, key: &str, look: Look) -> El {
        let (thumb, held) = t::scroll_thumb(look.c);
        let (reported, dragged) = (key.to_string(), key.to_string());
        self.on_scroll_offset(move |at: ScrollPos| {
            autoscroll::report(&reported, at.offset[1], at.max[1]);
        })
        // A hover handler that does nothing makes the pane where a press on its content
        // stops, as Rune looks up from what was pressed for an element that takes presses;
        // it takes no drag, so selecting text across messages is the pane's as before.
        .on_hover(|_: &mut State| {})
        .scroll_bar_styled(
            ScrollBarStyle::neutral(look.dark)
                .thumb(thumb)
                .thumb_hover(held),
        )
        // On the bar's frame around the pane, not on the pane: Rune gives an element with a
        // drag handler the primary button's presses too, and turns off selecting text in it.
        // A primary press stops at the pane below; the middle button's goes on up to here.
        .pointer_buttons([PointerButton::Middle])
        .on_drag(move |s: &mut State, drag: &mut Drag, tx: &mut Tx| {
            if tx.button() != PointerButton::Middle {
                return;
            }
            match drag.phase {
                Phase::Down => s.autoscroll_start(&dragged, drag.window_point()),
                Phase::Move if !drag.cancelled() => s.autoscroll_move(drag.dy),
                Phase::Move | Phase::Up => s.autoscroll_stop(),
            }
        })
    }
}
