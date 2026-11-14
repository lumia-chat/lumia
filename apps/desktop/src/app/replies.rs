//! Quote replies, reactions and mentions as the window holds them (architecture 6.6; the
//! screens and their words are the web's too): the quote over the composer, the selection a
//! "Quote reply" would quote, the emoji picker's message, the composer's @ list, the bar
//! after a send that names those mentioned who are not in the channel, the message a jump
//! flashed, and whether the newest message is on screen, which is when a conversation is
//! marked read (`State::tick`).

use std::time::{Duration, Instant};

use lumia_client::app::{MemberSummary, QuoteSummary};
use rune::{Key, KeyEvent, LiveRegion, TextEdit, Tx};

use super::{Popover, State};
use crate::session::display_name;
use crate::session::reads::quote_of;

/// The quick reactions, in the toolbar over a message and first in the picker's
/// "Frequently used".
pub const QUICK: [&str; 6] = ["👍", "❤️", "😂", "🎉", "😮", "😢"];
/// How many emoji "Frequently used" keeps.
const RECENT: usize = 16;
/// How many rows the @ list shows.
const LIST_ROWS: usize = 8;
/// How long a message a jump reached stays marked.
const FLASH: Duration = Duration::from_secs(1);
/// How long the bar says who was invited before it goes.
const SAID_FOR: Duration = Duration::from_secs(4);
/// How long a code block's button reads "Copied".
pub const COPIED: Duration = Duration::from_millis(1500);

#[derive(Debug, Default)]
pub struct Replies {
    /// The quote over the composer, sent with the next message.
    pub replying: Option<QuoteSummary>,
    /// A selection inside one message's body: the message, the text selected, and whether it
    /// was made backwards; the "Quote reply" button over it quotes it.
    pub selection: Option<(String, String, bool)>,
    /// The `@word` being typed at the caret, which the @ list completes.
    pub mention: Option<MentionQuery>,
    /// The bar after a send naming those mentioned who are not in the channel.
    pub outside: Option<Outside>,
    /// The message the emoji picker reacts to, the control it hangs from, and its search.
    pub picker: Option<Picker>,
    /// The message a jump reached, marked until then.
    pub flash: Option<(String, Instant)>,
    /// The open conversation's newest message is on screen.
    pub newest_visible: bool,
    /// The message under the pointer, which the toolbar is over.
    pub hovered: Option<String>,
    /// The message being edited in place, and what its field holds.
    pub editing: Option<(String, TextEdit)>,
    /// What the field being edited holds is over the limit on a message, as a screen reader
    /// was last told.
    pub edit_over: bool,
    /// The code block whose text was just copied: its message, its number in the body, and
    /// when. Its button reads "Copied" until [`COPIED`] has passed.
    pub copied: Option<(String, usize, Instant)>,
    /// The message whose "More actions" menu is open.
    pub menu_for: Option<String>,
    /// The message "Delete this message?" asks about.
    pub deleting: Option<String>,
}

/// An `@word` in the draft, from its `@` to the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionQuery {
    pub start: usize,
    pub end: usize,
    /// What follows the `@`.
    pub query: String,
    /// The row Up and Down have moved to.
    pub chosen: usize,
}

/// One row of the @ list.
#[derive(Debug, Clone)]
pub enum Choice {
    Everyone,
    Member {
        member: MemberSummary,
        /// In the channel; one who is not is marked so.
        inside: bool,
    },
}

/// Those a message mentioned who are not in its channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outside {
    pub channel_id: String,
    pub account_ids: Vec<String>,
    pub adding: bool,
    /// What the bar says after an add: who was invited, until it goes.
    pub said: Option<(String, Instant)>,
    pub error: String,
}

#[derive(Debug)]
pub struct Picker {
    pub message_id: String,
    pub anchor: String,
    pub search: TextEdit,
}

/// The `@word` ending at the caret of `draft`: an `@` at the start or after whitespace, and
/// what can be in a handle after it. None while a selection is made or the caret is
/// elsewhere.
pub fn mention_at(draft: &TextEdit) -> Option<MentionQuery> {
    let (from, to) = draft.range();
    if from != to {
        return None;
    }
    let before = &draft.text[..to];
    let start = before.rfind('@')?;
    let query = &before[start + 1..];
    let handle = query
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    let opens = before[..start]
        .chars()
        .next_back()
        .is_none_or(char::is_whitespace);
    (handle && opens).then(|| MentionQuery {
        start,
        end: to,
        query: query.to_string(),
        chosen: 0,
    })
}

impl State {
    /// "Reply", or "Quote reply" over a selection: puts the message's quote over the composer,
    /// the whole of it or `part`, and the caret in the composer.
    pub fn reply_to(&mut self, message_id: &str, part: Option<&str>, tx: &mut Tx) {
        let Some(entry) = self.session.entries.iter().find(|e| e.id == message_id) else {
            return;
        };
        self.replies.replying = Some(quote_of(entry, part));
        self.replies.selection = None;
        tx.clear_text_selection();
        tx.focus("composer", false);
    }

    /// The pointer came onto a message's row, or left it.
    pub fn hovered(&mut self, message_id: &str, inside: bool) {
        if inside {
            self.replies.hovered = Some(message_id.to_string());
        } else if self.replies.hovered.as_deref() == Some(message_id) {
            self.replies.hovered = None;
        }
    }

    /// The message the toolbar is over: the one whose picker or menu it opened (which hangs
    /// from it), the one under the pointer, or the one being replied to. None while a message
    /// is being edited.
    pub fn toolbar_for(&self) -> Option<&str> {
        if self.replies.editing.is_some() {
            return None;
        }
        match (&self.popover, &self.replies.picker) {
            (Some(Popover::EmojiPicker), Some(picker)) if picker.anchor.starts_with("react-") => {
                Some(picker.message_id.as_str())
            }
            (Some(Popover::MessageMenu | Popover::DeleteMessage), _) => self
                .replies
                .menu_for
                .as_deref()
                .or(self.replies.deleting.as_deref()),
            (None, _) => self.replies.hovered.as_deref().or(self
                .replies
                .replying
                .as_ref()
                .map(|q| q.message_id.as_str())),
            _ => None,
        }
    }

    /// "Cancel reply", or Escape in the composer.
    pub fn cancel_reply(&mut self) {
        self.replies.replying = None;
    }

    /// The reply bar's "@ On" switch: whether the reply notifies the person it quotes.
    pub fn toggle_reply_mention(&mut self) {
        if let Some(quote) = &mut self.replies.replying {
            quote.silent = !quote.silent;
        }
    }

    /// "More actions" on a message: its menu, under the button.
    pub fn open_message_menu(&mut self, message_id: &str) {
        self.replies.menu_for = Some(message_id.to_string());
        self.open_popover(Popover::MessageMenu);
    }

    /// "Edit message": the message's body becomes a field where it is, holding the body.
    pub fn begin_edit(&mut self, message_id: &str, tx: &mut Tx) {
        let Some(entry) = self.session.entries.iter().find(|e| e.id == message_id) else {
            return;
        };
        if entry.author_account_id != self.session.me() || entry.deleted || !entry.notice.is_empty()
        {
            return;
        }
        let mut field = TextEdit::new(entry.body.clone());
        field.set_caret(entry.body.len());
        self.replies.editing = Some((message_id.to_string(), field));
        self.replies.edit_over = false;
        self.close_popover();
        // The row shows the field now: rebuilt.
        self.session.revision += 1;
        tx.focus(format!("edit-{message_id}"), false);
    }

    /// Escape in the field: the message as it was.
    pub fn cancel_edit(&mut self, tx: &mut Tx) {
        if self.replies.editing.take().is_some() {
            self.session.revision += 1;
            tx.focus("composer", false);
        }
    }

    /// Return in the field: the edit is sent; emptied, it asks to delete instead. Over the
    /// limit on a message it is not, and the field stays as it is.
    pub fn save_edit(&mut self, tx: &mut Tx) {
        let over = |(_, field): &(String, TextEdit)| self.session.room(&field.text).over();
        if self.replies.editing.as_ref().is_some_and(over) {
            return;
        }
        let Some((id, field)) = self.replies.editing.take() else {
            return;
        };
        self.replies.edit_over = false;
        self.session.revision += 1;
        let body = field.text.trim();
        let unchanged = self
            .session
            .entries
            .iter()
            .any(|e| e.id == id && e.body.trim() == body);
        if body.is_empty() {
            self.ask_delete(&id);
        } else if !unchanged {
            self.session.edit(&id, body);
        }
        tx.focus("composer", false);
    }

    /// Keys in the field being edited: Return saves, Escape cancels (Shift-Return a new line).
    pub fn edit_key(&mut self, key: &KeyEvent, tx: &mut Tx) -> bool {
        match key.key {
            Key::Enter if !key.mods.shift => {
                self.save_edit(tx);
                true
            }
            Key::Escape => {
                self.cancel_edit(tx);
                true
            }
            _ => false,
        }
    }

    /// After the field being edited changes: a screen reader is told when it goes over the
    /// limit on a message, once, not at every key after.
    pub fn edit_changed(&mut self, tx: &mut Tx) {
        let Some((_, field)) = &self.replies.editing else {
            return;
        };
        let room = self.session.room(&field.text);
        if room.over() && !self.replies.edit_over {
            tx.announce(super::room_words(room), LiveRegion::Polite);
        }
        self.replies.edit_over = room.over();
    }

    /// A code block's "Copy": its text on the clipboard, its button reading "Copied" for a
    /// moment, and a screen reader told so.
    pub fn copy_code(&mut self, message_id: &str, block: usize, code: &str, tx: &mut Tx) {
        tx.copy(code.to_string());
        tx.announce("Copied", LiveRegion::Polite);
        self.replies.copied = Some((message_id.to_string(), block, Instant::now()));
        // The row reads "Copied" now: rebuilt.
        self.session.revision += 1;
    }

    /// "Show more" under a folded message, and "Show less" under one shown whole.
    pub fn unfold(&mut self, message_id: &str) {
        let unfolded = &mut self.session.unfolded;
        if !unfolded.remove(message_id) {
            unfolded.insert(message_id.to_string());
        }
        // The row is drawn otherwise, and measured again.
        self.session.revision += 1;
    }

    /// The text field of the message being edited, for its `on_edit`.
    pub fn edit_field(&mut self) -> &mut TextEdit {
        match &mut self.replies.editing {
            Some((_, field)) => field,
            // Gone between the frame and the edit: an edit to nothing kept.
            None => &mut self.member_filter,
        }
    }

    /// "Delete message": asked first, unless Shift is held.
    pub fn delete_from_menu(&mut self, message_id: &str, tx: &mut Tx) {
        if tx.mods().shift {
            self.close_popover();
            self.session.delete(message_id);
        } else {
            self.ask_delete(message_id);
        }
    }

    fn ask_delete(&mut self, message_id: &str) {
        self.replies.deleting = Some(message_id.to_string());
        self.open_popover(Popover::DeleteMessage);
    }

    /// "Delete" in "Delete this message?".
    pub fn confirm_delete(&mut self) {
        if let Some(id) = self.replies.deleting.take() {
            self.session.delete(&id);
        }
        self.close_popover();
    }

    /// "Copy text" in the menu.
    pub fn copy_message(&mut self, message_id: &str, tx: &mut Tx) {
        if let Some(entry) = self.session.entries.iter().find(|e| e.id == message_id) {
            tx.copy(entry.body.clone());
        }
        self.close_popover();
    }

    /// This account's newest message in the open conversation that can be edited: what Up in
    /// an empty composer edits.
    fn last_own(&self) -> Option<String> {
        let me = self.session.me();
        self.session
            .entries
            .iter()
            .rev()
            .find(|e| e.author_account_id == me && !e.deleted && e.notice.is_empty())
            .map(|e| e.id.clone())
    }

    /// The selection in the timeline changed: one inside a single message's body can be
    /// quoted.
    pub fn selection_in(&mut self, selection: Option<(String, String, bool)>) {
        self.replies.selection = selection.filter(|(_, text, _)| !text.trim().is_empty());
    }

    /// A reaction from the toolbar, a chip or the picker: added, or taken away when this
    /// account has it already. Kept in "Frequently used".
    pub fn react_with(&mut self, message_id: &str, emoji: &str) {
        self.session.react(message_id, emoji);
        let recent = &mut self.settings.recent_emoji;
        recent.retain(|e| e != emoji);
        recent.insert(0, emoji.to_string());
        recent.truncate(RECENT);
        self.settings.save(&self.data_dir);
    }

    /// "Add reaction": the picker for a message, hung from the control keyed `anchor`.
    pub fn open_picker(&mut self, message_id: &str, anchor: &str) {
        self.replies.picker = Some(Picker {
            message_id: message_id.to_string(),
            anchor: anchor.to_string(),
            search: TextEdit::new(""),
        });
        self.open_popover(Popover::EmojiPicker);
    }

    /// An emoji chosen in the picker.
    pub fn pick_emoji(&mut self, emoji: &str) {
        if let Some(picker) = self.replies.picker.take() {
            self.react_with(&picker.message_id, emoji);
        }
        self.close_popover();
    }

    /// "Frequently used": what this computer reacted with, then the quick six.
    pub fn frequent_emoji(&self) -> Vec<String> {
        let mut all: Vec<String> = self.settings.recent_emoji.clone();
        for quick in QUICK {
            if !all.iter().any(|e| e == quick) {
                all.push(quick.to_string());
            }
        }
        all.truncate(RECENT);
        all
    }

    /// The @ list's rows for what is being typed: everyone first for one who may mention
    /// everyone, then the server's members whose handle, display name or nickname starts with
    /// it, those in the channel first, then by name. None outside a server's channel.
    pub fn mention_choices(&self) -> Vec<Choice> {
        let (Some(query), Some(channel_id)) = (&self.replies.mention, self.session.channel())
        else {
            return Vec::new();
        };
        let Some(channel) = self.session.channel_summary(channel_id) else {
            return Vec::new();
        };
        if channel.kind != "TEXT" {
            return Vec::new();
        }
        let wanted = query.query.to_lowercase();
        let starts = |name: &str| name.to_lowercase().starts_with(&wanted);
        let inside = self.session.channel_members.get(channel_id);
        let mut people: Vec<(bool, String, MemberSummary)> = self
            .session
            .members
            .get(&channel.server_id)
            .into_iter()
            .flatten()
            .filter(|m| m.account_id != self.session.me())
            .filter(|m| starts(&m.handle) || starts(&m.display_name) || starts(&m.nickname))
            .map(|m| {
                let inside = inside.is_none_or(|ids| ids.contains(&m.account_id));
                (inside, display_name(m).to_lowercase(), m.clone())
            })
            .collect();
        people.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let everyone = self
            .session
            .servers()
            .iter()
            .any(|s| s.server_id == channel.server_id && s.can_mention_everyone)
            && "everyone".starts_with(&wanted);
        everyone
            .then_some(Choice::Everyone)
            .into_iter()
            .chain(
                people
                    .into_iter()
                    .map(|(inside, _, member)| Choice::Member { member, inside }),
            )
            .take(LIST_ROWS)
            .collect()
    }

    /// Picks row `index` of the @ list: the `@word` becomes the handle and a space.
    pub fn pick_mention(&mut self, index: usize) {
        let choices = self.mention_choices();
        let (Some(query), Some(choice)) =
            (self.replies.mention.take(), choices.into_iter().nth(index))
        else {
            return;
        };
        let handle = match choice {
            Choice::Everyone => "everyone".to_string(),
            Choice::Member { member, .. } => member.handle,
        };
        let mut text = self.draft.text.clone();
        let words = format!("@{handle} ");
        text.replace_range(query.start..query.end, &words);
        self.draft.replace_text(text);
        self.draft.set_caret(query.start + words.len());
        // A pick is not a new `@word` to complete.
        self.draft_seen.clone_from(&self.draft.text);
    }

    /// Keys in the composer before it takes them: the @ list's Up, Down, Return, Tab and
    /// Escape while it shows, and Escape to cancel a reply. True when taken.
    pub fn composer_key(&mut self, key: &KeyEvent) -> bool {
        let rows = self.mention_choices().len();
        if rows > 0
            && let Some(query) = &mut self.replies.mention
        {
            match key.key {
                Key::Up => {
                    query.chosen = (query.chosen + rows - 1) % rows;
                    return true;
                }
                Key::Down => {
                    query.chosen = (query.chosen + 1) % rows;
                    return true;
                }
                Key::Enter | Key::Tab if !key.mods.shift => {
                    let chosen = query.chosen.min(rows - 1);
                    self.pick_mention(chosen);
                    return true;
                }
                Key::Escape => {
                    self.replies.mention = None;
                    return true;
                }
                _ => {}
            }
        }
        if key.key == Key::Escape && self.replies.replying.is_some() {
            self.cancel_reply();
            return true;
        }
        false
    }

    /// Up in an empty composer: this account's newest message, edited in place.
    pub fn composer_up(&mut self, key: &KeyEvent, tx: &mut Tx) -> bool {
        if key.key != Key::Up || !self.draft.text.is_empty() || self.replies.mention.is_some() {
            return false;
        }
        match self.last_own() {
            Some(id) => {
                self.begin_edit(&id, tx);
                true
            }
            None => false,
        }
    }

    /// The draft changed: what the @ list completes follows the caret.
    pub fn follow_mention(&mut self) {
        let found = mention_at(&self.draft);
        self.replies.mention = match (found, self.replies.mention.take()) {
            (Some(mut found), Some(was)) if found.start == was.start => {
                if found.query == was.query {
                    found.chosen = was.chosen;
                }
                Some(found)
            }
            (found, _) => found,
        };
    }

    /// After a send: those it mentioned who are not in the channel get the bar.
    pub(super) fn sent_outside(&mut self, account_ids: Vec<String>) {
        self.replies.outside = match (self.session.channel(), account_ids.is_empty()) {
            (Some(channel), false) => Some(Outside {
                channel_id: channel.to_string(),
                account_ids,
                adding: false,
                said: None,
                error: String::new(),
            }),
            _ => None,
        };
    }

    /// The bar's "Add to #name".
    pub fn add_outside(&mut self) {
        let Some(outside) = &mut self.replies.outside else {
            return;
        };
        outside.adding = true;
        outside.error.clear();
        let (channel, ids) = (outside.channel_id.clone(), outside.account_ids.clone());
        self.session.add_to_channel(&channel, &ids);
    }

    /// The bar's "Dismiss".
    pub fn dismiss_outside(&mut self) {
        self.replies.outside = None;
    }

    /// What an add from the bar came to: those added are in the channel, and the bar goes;
    /// those invited are named for a moment.
    pub(super) fn outside_added(
        &mut self,
        channel_id: &str,
        result: Result<&lumia_client::app::ChannelAddSummary, &str>,
    ) {
        let Some(outside) = &mut self.replies.outside else {
            return;
        };
        if outside.channel_id != channel_id || !outside.adding {
            return;
        }
        outside.adding = false;
        match result {
            Ok(added) if added.invited_account_ids.is_empty() => self.replies.outside = None,
            Ok(added) => {
                let channel = self
                    .session
                    .channel_summary(channel_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                let words = match added.invited_account_ids.as_slice() {
                    [one] => format!("Invited {} to #{channel}.", self.session.name_of(one)),
                    many => format!("Invited {} people to #{channel}.", many.len()),
                };
                outside.said = Some((words, Instant::now()));
            }
            Err(error) => outside.error = error.to_string(),
        }
    }

    /// What the bar names: "Ada isn’t in #design, so they won’t be notified."
    pub fn outside_words(&self) -> Option<String> {
        let outside = self.replies.outside.as_ref()?;
        if let Some((said, _)) = &outside.said {
            return Some(said.clone());
        }
        let channel = self
            .session
            .channel_summary(&outside.channel_id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let names: Vec<String> = outside
            .account_ids
            .iter()
            .map(|id| self.session.name_of(id))
            .collect();
        Some(match names.as_slice() {
            [one] => format!("{one} isn’t in #{channel}, so they won’t be notified."),
            [one, two] => {
                format!("{one} and {two} aren’t in #{channel}, so they won’t be notified.")
            }
            [one, two, rest @ ..] => format!(
                "{one}, {two} and {} aren’t in #{channel}, so they won’t be notified.",
                crate::ui::others(rest.len())
            ),
            [] => String::new(),
        })
    }

    /// What the passing of time does to these: the flash and the bar's words run out, and
    /// the open conversation is marked read while its newest message is on screen in front
    /// of the person. True when something on screen changed.
    pub(super) fn tick_replies(&mut self) -> bool {
        let mut changed = false;
        if self
            .replies
            .flash
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() >= FLASH)
        {
            self.replies.flash = None;
            changed = true;
        }
        if self
            .replies
            .outside
            .as_ref()
            .and_then(|o| o.said.as_ref())
            .is_some_and(|(_, at)| at.elapsed() >= SAID_FOR)
        {
            self.replies.outside = None;
            changed = true;
        }
        if self
            .replies
            .copied
            .as_ref()
            .is_some_and(|(_, _, at)| at.elapsed() >= COPIED)
        {
            self.replies.copied = None;
            self.session.revision += 1;
            changed = true;
        }
        if self.notices.in_front
            && self.replies.newest_visible
            && self.session.previewing.is_none()
            && let (Some(conversation), Some(newest)) = (
                self.session.conversation.clone(),
                self.session.entries.last(),
            )
        {
            let seq = newest.seq;
            self.session.mark_read(conversation.id(), seq);
        }
        changed
    }

    /// A conversation just opened with unread messages shows its "New messages" line: the
    /// list, at its end, scrolls up to it when it is above what shows.
    pub(super) fn land(&mut self, tx: &mut Tx) {
        if !self.session.reads.land {
            return;
        }
        if let Some(id) = self.session.new_line_at() {
            tx.scroll_into_view_in("rows", id.to_string());
            self.session.reads.land = false;
        }
    }

    /// A message the session found for a jump: scrolled to the middle and marked.
    pub(super) fn show_found(&mut self, tx: &mut Tx) -> bool {
        let Some(id) = self.session.reads.found.take() else {
            return false;
        };
        tx.scroll_center_in("rows", id.clone());
        self.replies.flash = Some((id, Instant::now()));
        true
    }

    /// Home's Mentions: the messages in every server that notify this account.
    pub fn open_home_mentions(&mut self) {
        self.go_home(super::HomeView::Mentions);
        self.session.load_mentions(None, false);
    }

    /// A mention pressed on a Mentions page: its channel, at it.
    pub fn open_mention(&mut self, channel_id: &str, message_id: &str, tx: &mut Tx) {
        let Some(server_id) = self
            .session
            .channel_summary(channel_id)
            .map(|c| c.server_id.clone())
        else {
            return;
        };
        if self.server_id() != Some(server_id.as_str()) {
            self.select_server(&server_id);
        }
        self.select_channel(channel_id);
        self.session.reveal(message_id);
        self.show_found(tx);
    }

    /// Jumps to a message of the open conversation: a quote's original. One already read is
    /// shown in the frame of the press: `poll`, which shows what a later page finds, runs
    /// before a frame's input, and no frame might follow for a second or more (#14).
    pub fn jump_to(&mut self, message_id: &str, tx: &mut Tx) {
        self.session.reveal(message_id);
        self.show_found(tx);
    }

    /// Whether the message is the one a jump just reached.
    pub fn flashed(&self, message_id: &str) -> bool {
        self.replies
            .flash
            .as_ref()
            .is_some_and(|(id, _)| id == message_id)
    }
}
