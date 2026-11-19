//! The menus a right-click opens (#25): a message's actions and a person's, where the pointer
//! is, so that neither takes aiming at the toolbar in a row's corner or opening a card first.
//! The Menu key and Shift+F10 open the same menus under the control the keyboard is on (Rune
//! sends both to the handler a secondary press has). The toolbar and the profile card stay as
//! they are, and each item here does what its counterpart there does.
//!
//! A message's menu is offered by the toolbar's rules: not on a notice, a deleted message or
//! one still on its way, and not while a message is being edited. A right-click on a row
//! used to bring Rune's own Copy for the text under it, which a row with a menu of its own
//! takes the place of, so the message's menu leads with what that gave: "Copy selection" when
//! the message is in the timeline's selection, with "Quote reply" when the selection is
//! inside it alone, and "Open link" and "Copy link" on a link.
//!
//! A person's menu is "View profile" and then what their card would offer where the shell is
//! ([`State::ways_to`], which the card asks too): on a member's row, a message's author and a
//! conversation's row when it is with one other person.

use lumia_client::body::is_web;
use rune::Tx;

use super::{Person, Popover, State};

/// The key of the place a menu was asked for, which the menu hangs from (`ui::view` draws
/// it), and the emoji picker after it when "Add reaction" opens one.
pub const AT: &str = "context-at";

#[derive(Debug, Default)]
pub struct Context {
    /// Where the open menu was asked for, in window points: the pointer, or under the
    /// control the keyboard was on.
    pub at: [f32; 2],
    /// The message whose menu is open, and what the press was on.
    pub message: Option<MessageContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageContext {
    pub message_id: String,
    /// What the timeline's selection would copy, when this message is in it.
    pub selection: Option<String>,
    /// The selection is inside this message alone, so "Quote reply" can quote it.
    pub quotable: bool,
    /// The link under the pointer, as the message's words have it.
    pub link: Option<String>,
}

/// How this account can message someone from where the shell is: what their card offers,
/// and their menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ways {
    /// The shell is in a server, where a server DM comes first and a personal conversation
    /// is "Message privately"; at Home the personal one is the only way.
    pub in_server: bool,
    /// A server DM is offered: they take them in this server and this account may start
    /// them.
    pub here: bool,
    /// They have turned server DMs off in this server, which their card says.
    pub off: bool,
}

impl State {
    /// A right-click on a message's row, the Menu key or Shift+F10 inside it: its menu, at
    /// `at`, with `link` when the press was on one.
    pub fn open_message_context(&mut self, message_id: &str, at: [f32; 2], link: Option<&str>) {
        if self.replies.editing.is_some() {
            return;
        }
        let offered = self
            .session
            .entries
            .iter()
            .any(|e| e.id == message_id && !e.deleted && e.notice.is_empty());
        if !offered {
            return;
        }
        let selection = self
            .replies
            .selected
            .as_ref()
            .filter(|(ids, _)| ids.iter().any(|id| id == message_id));
        self.context = Context {
            at,
            message: Some(MessageContext {
                message_id: message_id.to_string(),
                quotable: selection.is_some_and(|(ids, _)| ids.len() == 1),
                selection: selection.map(|(_, text)| text.clone()),
                link: link.map(str::to_string),
            }),
        };
        self.open_popover(Popover::MessageContext);
    }

    /// The open menu's message, taken as the menu closes for one of its items.
    fn context_taken(&mut self) -> Option<MessageContext> {
        self.close_popover();
        self.context.message.take()
    }

    /// "Copy selection": what Copy in the Edit menu would take.
    pub fn context_copy_selection(&mut self, tx: &mut Tx) {
        if let Some(selection) = self.context_taken().and_then(|m| m.selection) {
            tx.copy(selection);
        }
    }

    /// "Quote reply": the selection inside the message, quoted over the composer.
    pub fn context_quote(&mut self, tx: &mut Tx) {
        let Some(message) = self.context_taken() else {
            return;
        };
        if let Some(selection) = &message.selection {
            self.reply_to(&message.message_id, Some(selection), tx);
        }
    }

    /// "Open link": in the browser, and only a web link, as a press on it opens.
    pub fn context_open_link(&mut self, tx: &mut Tx) {
        if let Some(link) = self.context_taken().and_then(|m| m.link)
            && is_web(&link)
        {
            let _ = tx.open_url(&link);
        }
    }

    /// "Copy link": the address, which the link's words need not say.
    pub fn context_copy_link(&mut self, tx: &mut Tx) {
        if let Some(link) = self.context_taken().and_then(|m| m.link) {
            tx.copy(link);
        }
    }

    /// "Reply", as the toolbar's.
    pub fn context_reply(&mut self, tx: &mut Tx) {
        if let Some(message) = self.context_taken() {
            self.reply_to(&message.message_id, None, tx);
        }
    }

    /// "Add reaction": the picker, where the menu was.
    pub fn context_react(&mut self) {
        if let Some(message) = self.context.message.take() {
            self.open_picker(&message.message_id, AT);
        }
    }

    /// A right-click on a person, the Menu key or Shift+F10 on their control: their menu,
    /// at `at`. `anchor` is the control's key, which "View profile" hangs their card from.
    pub fn open_person_menu(&mut self, account_id: &str, anchor: &str, at: [f32; 2]) {
        if account_id.is_empty() {
            return;
        }
        self.channel_forms.dm_error.clear();
        self.person = Some(Person {
            account_id: account_id.to_string(),
            anchor: anchor.to_string(),
        });
        self.context = Context { at, message: None };
        self.open_popover(Popover::PersonMenu);
    }

    /// "View profile": their card, beside the control the menu was asked on.
    pub fn view_person(&mut self) {
        if self.person.is_some() {
            self.open_popover(Popover::Person);
        }
    }

    /// The menu's "Message Ada" in a server: the server DM, as the card's. With no card
    /// open to say why one was refused, a refusal opens it.
    pub fn message_person_from_menu(&mut self) {
        self.close_popover();
        self.channel_forms.dm_from_menu = true;
        self.message_person_here();
    }

    /// How this account can message `account_id` from where the shell is. Whether they take
    /// server DMs is theirs to say in each server, so it is read from the list of the server
    /// on screen ([`Session::member_in`]) and never from what another server said of them;
    /// someone that list does not have is offered one, and the instance decides.
    ///
    /// [`Session::member_in`]: crate::session::Session::member_in
    pub fn ways_to(&self, account_id: &str) -> Ways {
        let server = self.server_id().and_then(|id| self.session.server(id));
        let off = server
            .and_then(|s| self.session.member_in(&s.server_id, account_id))
            .is_some_and(|member| member.server_dms_off);
        Ways {
            in_server: self.server_id().is_some(),
            here: !off && server.is_some_and(|s| s.can_start_dms),
            off,
        }
    }
}
