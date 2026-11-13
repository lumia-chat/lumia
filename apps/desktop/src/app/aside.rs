//! The aside: a server's member list, or a personal conversation's details. A wide window
//! has it in the row beside the conversation, put away and brought back by the toggle in
//! the conversation's header, which this computer remembers (`settings.aside_hidden`). A
//! window narrower than [`ROW_FROM`] has no room for it: the toggle slides it over the
//! conversation's end, and a press outside it or Escape puts it away again. The web's
//! frame does the same at the same width (`apps/web/src/design/components.css`).

use super::State;

/// The narrowest window that has the aside in the row, in points: the web's breakpoint.
pub const ROW_FROM: f32 = 1100.0;

impl State {
    /// The shell's layout says how wide the window is. A window that widens past the
    /// breakpoint takes the aside back into the row, so one sliding over is put away.
    pub fn set_window_width(&mut self, width: f32) {
        self.window_width = Some(width);
        if !self.narrow() {
            self.aside_open = false;
        }
    }

    /// Whether the window is too narrow for the aside in the row. Until the layout has said,
    /// it is taken to be wide.
    pub fn narrow(&self) -> bool {
        self.window_width.is_some_and(|width| width < ROW_FROM)
    }

    /// Whether the aside stands in the row.
    pub fn aside_in_row(&self) -> bool {
        !self.narrow() && !self.settings.aside_hidden
    }

    /// Whether the aside slides over the conversation.
    pub fn aside_over(&self) -> bool {
        self.narrow() && self.aside_open
    }

    /// Whether the aside shows at all: what the toggle says it is.
    pub fn aside_shown(&self) -> bool {
        self.aside_in_row() || self.aside_over()
    }

    /// The toggle in the conversation's header: in a wide window, the aside in the row or
    /// not, remembered; in a narrow one, over the conversation or not.
    pub fn toggle_aside(&mut self) {
        if self.narrow() {
            self.aside_open = !self.aside_open;
        } else {
            self.settings.aside_hidden = !self.settings.aside_hidden;
            self.settings.save(&self.data_dir);
        }
    }

    /// A press outside the aside over the conversation, or Escape.
    pub fn close_aside(&mut self) {
        self.aside_open = false;
    }
}
