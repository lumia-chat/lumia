//! Roles and titles in the window (architecture 7.5): the Roles and Members sections of a
//! server's settings, a role's editor, a role moved by its handle or its menu, and the
//! profile card's two small forms. What is offered and every word said come from [`roles`]
//! (the sheet the shells share); what this account may do comes from the core, in the
//! snapshot and the member list; the instance decides every write.
//!
//! How each thing is kept. A role's editor has one Save for its name, colour, tag switch and
//! permissions; turning Administrator on and deleting ask first. Who holds a role is changed
//! there at once, apart from Save: Remove takes it from one member, "Add members" gives it to
//! those ticked. A member's row in Members opens to their roles, their title and Kick, with
//! Save and Cancel; Kick asks nothing first, as the boards have it. A role is moved by its
//! handle or from its menu, drawn moved at once and put back with the refusal said if the
//! instance refuses. Where a save succeeds a form with Cancel beside its Save closes, the row
//! or the card under it showing what was saved; the editor, which has no Cancel, says
//! "Saved."; a refusal is said where the thing was tried.
//!
//! A drag starts once the pointer has gone [`DRAG_START`] from the press on a handle. The
//! role's place stays, dashed; a line in the accent says where it would land, which is before
//! the first row whose middle is under the pointer, when [`roles::lands_before`] allows it.
//! Escape puts it back.

use std::collections::HashMap;

use lumia_client::app::{MemberSummary, ServerSummary};
use lumia_client::roles::RoleSummary;
use rune::{Drag, LiveRegion, Phase, TextEdit, Tx};

use super::rail::DRAG_START;
use super::roles::{self, RoleDraft, Section, Step};
use super::server_settings::Saving;
use super::{Popover, State};
use crate::session::roles::{RoleSave, RoleUpdate};

/// How many members the Members section draws before "Show more members", and how many
/// more each press of it adds.
pub const MEMBERS_PAGE: usize = 50;

/// A role's editor: the role it is open for (none for a new one) and what is chosen in it.
#[derive(Debug, Default)]
pub struct RoleEditor {
    pub role_id: Option<String>,
    pub name: TextEdit,
    /// A tone, or empty for none.
    pub color: String,
    pub hide_tag: bool,
    /// The permissions switched on, by their wire names.
    pub permissions: Vec<String>,
    pub saving: Saving,
    /// The holder being taken out of the role, and why one was not.
    pub removing: Option<String>,
    pub holders_error: String,
}

impl RoleEditor {
    fn of(role: Option<&RoleSummary>) -> Self {
        let draft = roles::draft_of(role);
        Self {
            role_id: role.map(|r| r.role_id.clone()),
            name: TextEdit::new(&draft.name),
            color: draft.color,
            hide_tag: draft.hide_tag,
            permissions: draft.permissions,
            ..Self::default()
        }
    }

    /// What the editor holds, as the sheet compares it.
    pub fn draft(&self) -> RoleDraft {
        RoleDraft {
            name: self.name.text.clone(),
            color: self.color.clone(),
            hide_tag: self.hide_tag,
            permissions: self.permissions.clone(),
        }
    }
}

/// A role being dragged by its handle.
#[derive(Debug, Clone, PartialEq)]
pub struct RoleDrag {
    pub id: String,
    /// The row's rect when the drag began, in window points.
    pub origin: [f32; 4],
    /// How far the pointer has gone since.
    pub by: [f32; 2],
    /// The role it would land before, when dropping it there would move it.
    pub before: Option<String>,
}

/// "Delete Moderator?": the role, and the control the question hangs from.
#[derive(Debug, Default, Clone)]
pub struct Deleting {
    pub role_id: String,
    pub anchor: String,
    pub busy: bool,
    pub error: String,
}

/// "Add members" to the role in the editor.
#[derive(Debug, Default)]
pub struct AddHolders {
    pub filter: TextEdit,
    pub chosen: Vec<String>,
    pub busy: bool,
    pub error: String,
}

/// A member's row opened in Members: their roles as ticked, their title as typed.
#[derive(Debug, Default)]
pub struct MemberEdit {
    pub account_id: String,
    pub roles: Vec<String>,
    pub title: TextEdit,
    pub busy: bool,
    pub error: String,
}

/// Which of the profile card's two forms is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardForm {
    Roles,
    Title,
}

/// The form an Edit on the profile card opened, for `State::person` in the server on screen.
#[derive(Debug)]
pub struct CardEdit {
    pub form: CardForm,
    pub member: MemberEdit,
}

/// Roles and titles in the window.
#[derive(Debug)]
pub struct RoleForms {
    pub editor: Option<RoleEditor>,
    /// Said over the list: why a move was refused, or that the role being edited is gone.
    pub list_error: String,
    /// Each role's row as last laid out, by role id, in window points.
    pub spots: HashMap<String, [f32; 4]>,
    pub drag: Option<RoleDrag>,
    /// The role whose menu is open (`Popover::RoleMenu`).
    pub menu_for: Option<String>,
    pub deleting: Option<Deleting>,
    pub adding: AddHolders,
    /// What "Search members" holds.
    pub search: TextEdit,
    /// How many members are drawn.
    pub shown: usize,
    pub member: Option<MemberEdit>,
    pub card: Option<CardEdit>,
    /// What a field writes into in the frame its form has closed under it.
    scratch: TextEdit,
}

impl RoleForms {
    /// The name in the role's editor.
    pub fn role_name(&mut self) -> &mut TextEdit {
        match &mut self.editor {
            Some(editor) => &mut editor.name,
            None => &mut self.scratch,
        }
    }

    /// The title in the member's row open in Members.
    pub fn member_title(&mut self) -> &mut TextEdit {
        match &mut self.member {
            Some(open) => &mut open.title,
            None => &mut self.scratch,
        }
    }

    /// The title in the profile card's form.
    pub fn card_title(&mut self) -> &mut TextEdit {
        match &mut self.card {
            Some(card) => &mut card.member.title,
            None => &mut self.scratch,
        }
    }
}

impl Default for RoleForms {
    fn default() -> Self {
        Self {
            scratch: TextEdit::new(""),
            editor: None,
            list_error: String::new(),
            spots: HashMap::new(),
            drag: None,
            menu_for: None,
            deleting: None,
            adding: AddHolders::default(),
            search: TextEdit::new(""),
            shown: MEMBERS_PAGE,
            member: None,
            card: None,
        }
    }
}

/// The role a drag with the pointer at `y` (window points) would land before: the first row
/// whose middle is under the pointer, Everyone past the last; none where landing there is not
/// allowed or would move nothing.
pub fn drop_before(
    roles: &[RoleSummary],
    spots: &HashMap<String, [f32; 4]>,
    dragged: &str,
    y: f32,
) -> Option<String> {
    let under = roles
        .iter()
        .find(|role| {
            spots
                .get(&role.role_id)
                .is_some_and(|[_, top, _, height]| y < top + height / 2.0)
        })
        .or_else(|| roles.iter().find(|role| role.is_everyone))?;
    roles::lands_before(roles, dragged, &under.role_id).then(|| under.role_id.clone())
}

/// A field's words kept to the most a role's name or a title holds.
pub fn keep_to_label(edit: &mut TextEdit) {
    if edit.text.chars().count() > roles::LABEL_MAX {
        let kept: String = edit.text.chars().take(roles::LABEL_MAX).collect();
        edit.replace_text(&kept);
    }
}

impl State {
    /// The server whose settings are open, as the snapshot has it.
    pub fn settings_summary(&self) -> Option<&ServerSummary> {
        let id = self.settings_server()?;
        self.session.server(&id)
    }

    /// The members of the server on screen, as its list has them.
    fn members_here(&self) -> &[MemberSummary] {
        self.server_id()
            .and_then(|id| self.session.members.get(id))
            .map_or(&[], Vec::as_slice)
    }

    // ---- Sections -----------------------------------------------------------------------

    /// Shows a section of the settings, when this account has it.
    pub fn choose_section(&mut self, section: Section) {
        let Some(server) = self.settings_summary() else {
            return;
        };
        if !roles::settings_sections(server).contains(&section) {
            return;
        }
        self.close_popover();
        self.server_forms.settings.section = section;
    }

    /// After a snapshot: the settings follow what this account may still do. A section taken
    /// away gives way to the first it has, and to the channel when it has none; an editor
    /// whose role is gone or no longer this account's closes.
    pub(super) fn settle_settings(&mut self) {
        let Some(server) = self.settings_summary() else {
            return;
        };
        let sections = roles::settings_sections(server);
        let Some(first) = sections.first().copied() else {
            self.close_server_settings();
            return;
        };
        let role_of = |id: &str| server.roles.iter().find(|r| r.role_id == id);
        let gone = self
            .roles
            .editor
            .as_ref()
            .and_then(|e| e.role_id.as_deref())
            .is_some_and(|id| role_of(id).is_none_or(roles::locked_role));
        let unlisted = self
            .roles
            .editor
            .as_ref()
            .and_then(|e| e.role_id.as_deref())
            .is_some_and(|id| role_of(id).is_none());
        let now = self.server_forms.settings.section;
        if !sections.contains(&now) {
            self.server_forms.settings.section = first;
        }
        if gone {
            self.roles.editor = None;
            if matches!(
                self.popover,
                Some(Popover::AskAdministrator | Popover::DeleteRole | Popover::AddHolders)
            ) {
                self.close_popover();
            }
            if unlisted {
                self.roles.list_error = "That role is gone. Someone deleted it.".into();
            }
        }
    }

    // ---- Tag beside names ---------------------------------------------------------------

    /// What the tag beside a name shows, as the page draws it: the choice on its way, else
    /// the server's.
    pub fn tag_titles_shown(&self, server: &ServerSummary) -> bool {
        self.server_forms
            .settings
            .tag_choice
            .unwrap_or(server.tag_titles)
    }

    /// Chooses titles or roles as the tag beside names, saved as it is chosen.
    pub fn choose_tag_titles(&mut self, titles: bool) {
        let Some(server) = self.settings_summary() else {
            return;
        };
        let (server_id, now) = (server.server_id.clone(), server.tag_titles);
        let form = &mut self.server_forms.settings;
        if form.tag.busy || titles == now {
            return;
        }
        form.tag = Saving {
            busy: true,
            ..Saving::default()
        };
        form.tag_choice = Some(titles);
        self.session.set_tag_titles(&server_id, titles);
    }

    // ---- The list -----------------------------------------------------------------------

    /// The roles of the server whose settings are open, as the list draws them.
    pub fn roles_listed(&self) -> Vec<RoleSummary> {
        self.settings_server()
            .map(|id| self.session.roles_of(&id))
            .unwrap_or_default()
    }

    /// Opens a role's editor; a role that is not this account's to change does not open.
    pub fn open_role(&mut self, role_id: &str) {
        let Some(server) = self.settings_summary() else {
            return;
        };
        let Some(role) = server.roles.iter().find(|r| r.role_id == role_id) else {
            return;
        };
        if roles::locked_role(role) {
            return;
        }
        let editor = RoleEditor::of(Some(role));
        self.close_popover();
        self.roles.menu_for = None;
        self.roles.list_error.clear();
        self.roles.editor = Some(editor);
        self.server_forms.settings.section = Section::Roles;
    }

    /// "New role": the editor with nothing in it yet.
    pub fn new_role(&mut self) {
        if self
            .settings_summary()
            .is_none_or(|server| !server.can_manage_roles)
        {
            return;
        }
        self.close_popover();
        self.roles.list_error.clear();
        self.roles.editor = Some(RoleEditor::of(None));
    }

    /// Back to the list; what was not saved is let go.
    pub fn close_role(&mut self) {
        self.close_popover();
        self.roles.editor = None;
        self.roles.deleting = None;
    }

    /// The role the editor is open for, as the snapshot has it; none for a new role.
    pub fn edited_role(&self) -> Option<&RoleSummary> {
        let id = self.roles.editor.as_ref()?.role_id.as_deref()?;
        self.settings_summary()?
            .roles
            .iter()
            .find(|r| r.role_id == id)
    }

    /// Whether the editor's Save is offered.
    pub fn role_savable(&self) -> bool {
        self.roles
            .editor
            .as_ref()
            .is_some_and(|e| !e.saving.busy && roles::savable(&e.draft(), self.edited_role()))
    }

    pub fn choose_role_color(&mut self, color: &str) {
        if let Some(editor) = &mut self.roles.editor {
            editor.color = color.to_string();
            editor.saving.saved = false;
        }
    }

    /// Turns "Show as tag" over.
    pub fn toggle_role_tag(&mut self) {
        if let Some(editor) = &mut self.roles.editor {
            editor.hide_tag = !editor.hide_tag;
            editor.saving.saved = false;
        }
    }

    /// Turns a permission's switch. One this account does not hold is not turned, and
    /// Administrator asks before it is turned on.
    pub fn toggle_permission(&mut self, name: &str) {
        let Some(server) = self.settings_summary() else {
            return;
        };
        if roles::withheld(server, name) {
            return;
        }
        let Some(editor) = &mut self.roles.editor else {
            return;
        };
        let on = editor.permissions.iter().any(|p| p == name);
        if !on && name == roles::ADMINISTRATOR {
            self.open_popover(Popover::AskAdministrator);
            return;
        }
        if on {
            editor.permissions.retain(|p| p != name);
        } else {
            editor.permissions.push(name.to_string());
        }
        editor.saving.saved = false;
    }

    /// "Turn on", under "Turn on Administrator?".
    pub fn confirm_administrator(&mut self) {
        self.close_popover();
        if let Some(editor) = &mut self.roles.editor
            && !editor.permissions.iter().any(|p| p == roles::ADMINISTRATOR)
        {
            editor.permissions.push(roles::ADMINISTRATOR.to_string());
            editor.saving.saved = false;
        }
    }

    /// The editor's Save: the role created, or changed whole.
    pub fn save_role(&mut self) {
        if !self.role_savable() {
            return;
        }
        let Some(server_id) = self.settings_server() else {
            return;
        };
        let Some(editor) = &mut self.roles.editor else {
            return;
        };
        editor.saving = Saving {
            busy: true,
            ..Saving::default()
        };
        let role = RoleSave {
            name: editor.name.text.trim().to_string(),
            color: editor.color.clone(),
            hide_tag: editor.hide_tag,
            permissions: editor.permissions.clone(),
        };
        self.session
            .save_role(&server_id, editor.role_id.as_deref(), role);
    }

    /// The members of the server whose settings are open, once they are known: none while
    /// its list has not been read (still on its way, or it could not be), and none for a list
    /// with nobody in it, which no server's is, this account being in it.
    pub fn members_known(&self) -> Option<&[MemberSummary]> {
        let id = self.settings_server()?;
        self.session
            .members
            .get(&id)
            .map(Vec::as_slice)
            .filter(|all| !all.is_empty())
    }

    /// How many members hold a role; none while the server's members are not known, which
    /// is not the same as nobody holding it.
    pub fn holders_of(&self, role: &RoleSummary) -> Option<usize> {
        roles::holders(role, self.members_known())
    }

    // ---- Deleting -----------------------------------------------------------------------

    /// "Delete role": asked first, against the control keyed `anchor`.
    pub fn ask_delete_role(&mut self, role_id: &str, anchor: &str) {
        let manages = self.settings_summary().is_some_and(|server| {
            server
                .roles
                .iter()
                .any(|r| r.role_id == role_id && !r.is_everyone && r.can_manage)
        });
        if !manages {
            return;
        }
        self.roles.menu_for = None;
        self.roles.deleting = Some(Deleting {
            role_id: role_id.to_string(),
            anchor: anchor.to_string(),
            ..Deleting::default()
        });
        self.open_popover(Popover::DeleteRole);
    }

    /// "Delete role", under the question.
    pub fn confirm_delete_role(&mut self) {
        let Some(server_id) = self.settings_server() else {
            return;
        };
        let Some(deleting) = &mut self.roles.deleting else {
            return;
        };
        if deleting.busy {
            return;
        }
        deleting.busy = true;
        deleting.error.clear();
        self.session.delete_role(&server_id, &deleting.role_id);
    }

    // ---- Moving -------------------------------------------------------------------------

    /// Records where a role's row was laid out, for a drag to find it.
    pub fn place_role(&mut self, role_id: &str, rect: [f32; 4]) {
        if self.roles.spots.get(role_id) != Some(&rect) {
            self.roles.spots.insert(role_id.to_string(), rect);
        }
    }

    /// What a drag of a role's handle does at each phase.
    pub fn drag_role(&mut self, role_id: &str, drag: &Drag, tx: &mut Tx) {
        match drag.phase {
            Phase::Down => self.roles.drag = None,
            Phase::Move if drag.cancelled() => self.roles.drag = None,
            Phase::Move => {
                if self.roles.drag.is_none() && drag.dx.hypot(drag.dy) < DRAG_START {
                    return;
                }
                let Some(origin) = self
                    .roles
                    .drag
                    .as_ref()
                    .map(|held| held.origin)
                    .or_else(|| self.roles.spots.get(role_id).copied())
                else {
                    return;
                };
                let [_, y] = drag.window_point();
                let before = drop_before(&self.roles_listed(), &self.roles.spots, role_id, y);
                self.popover = None;
                self.roles.drag = Some(RoleDrag {
                    id: role_id.to_string(),
                    origin,
                    by: [drag.dx, drag.dy],
                    before,
                });
            }
            Phase::Up => {
                let Some(held) = self.roles.drag.take() else {
                    return;
                };
                if drag.cancelled() {
                    return;
                }
                if let Some(before) = held.before {
                    self.move_role(role_id, &before, tx);
                }
            }
        }
    }

    /// Sends the order with `role_id` before `before_id`, drawn at once, and says so.
    fn move_role(&mut self, role_id: &str, before_id: &str, tx: &mut Tx) {
        let Some(server_id) = self.settings_server() else {
            return;
        };
        let listed = self.roles_listed();
        if !roles::lands_before(&listed, role_id, before_id) {
            return;
        }
        let order = roles::moved_before(&listed, role_id, before_id);
        let name = listed
            .iter()
            .find(|r| r.role_id == role_id)
            .map(|r| r.name.clone())
            .unwrap_or_default();
        let place = order
            .iter()
            .position(|id| id == role_id)
            .map_or(0, |i| i + 1);
        self.roles.list_error.clear();
        self.session.order_roles(&server_id, order);
        tx.announce(
            format!("{name} moved to position {place}"),
            LiveRegion::Polite,
        );
    }

    /// Opens a role's menu where it was asked for (a right-click, the Menu key, Shift+F10).
    pub fn open_role_menu(&mut self, role_id: &str, at: [f32; 2]) {
        let moves = self
            .roles_listed()
            .iter()
            .any(|r| r.role_id == role_id && !r.is_everyone && r.can_manage);
        if !moves {
            return;
        }
        self.roles.drag = None;
        self.roles.menu_for = Some(role_id.to_string());
        self.context = super::context::Context { at, message: None };
        self.open_popover(Popover::RoleMenu);
    }

    /// The menu's "Edit role".
    pub fn role_menu_edit(&mut self) {
        if let Some(id) = self.roles.menu_for.take() {
            self.open_role(&id);
        }
    }

    /// The menu's "Move up" and "Move down".
    pub fn role_menu_step(&mut self, step: Step, tx: &mut Tx) {
        self.popover = None;
        let Some(id) = self.roles.menu_for.take() else {
            return;
        };
        if let Some(before) = roles::step_before(&self.roles_listed(), &id, step) {
            self.move_role(&id, &before, tx);
        }
    }

    /// The menu's "Delete role": asked first, against the role's row.
    pub fn role_menu_delete(&mut self, anchor: &str) {
        if let Some(id) = self.roles.menu_for.clone() {
            self.ask_delete_role(&id, anchor);
        }
    }

    // ---- Who holds a role ---------------------------------------------------------------

    /// Takes the role in the editor away from one of its holders, at once.
    pub fn remove_holder(&mut self, account_id: &str) {
        let Some(server_id) = self.settings_server() else {
            return;
        };
        let Some(role_id) = self.edited_role().map(|r| r.role_id.clone()) else {
            return;
        };
        let Some(member) = self.session.member_in(&server_id, account_id) else {
            return;
        };
        let rest: Vec<String> = member
            .role_ids
            .iter()
            .filter(|id| **id != role_id)
            .cloned()
            .collect();
        let Some(editor) = &mut self.roles.editor else {
            return;
        };
        if editor.removing.is_some() {
            return;
        }
        editor.removing = Some(account_id.to_string());
        editor.holders_error.clear();
        self.session
            .save_member(&server_id, account_id, Some(rest), None);
    }

    /// "Add members": the server's members who do not hold the role in the editor.
    pub fn open_add_holders(&mut self) {
        if self.edited_role().is_none() || self.members_known().is_none() {
            return;
        }
        self.roles.adding = AddHolders::default();
        self.open_popover(Popover::AddHolders);
    }

    pub fn toggle_holder(&mut self, account_id: &str) {
        let chosen = &mut self.roles.adding.chosen;
        match chosen.iter().position(|id| id == account_id) {
            Some(at) => {
                chosen.remove(at);
            }
            None => chosen.push(account_id.to_string()),
        }
    }

    /// Gives the role in the editor to everyone ticked.
    pub fn add_holders(&mut self) {
        let Some(server_id) = self.settings_server() else {
            return;
        };
        let Some(role_id) = self.edited_role().map(|r| r.role_id.clone()) else {
            return;
        };
        let form = &mut self.roles.adding;
        if form.busy || form.chosen.is_empty() {
            return;
        }
        form.busy = true;
        form.error.clear();
        self.session.give_role(&server_id, &role_id, &form.chosen);
    }

    // ---- Members ------------------------------------------------------------------------

    /// "Show more members".
    pub fn show_more_members(&mut self) {
        self.roles.shown += MEMBERS_PAGE;
    }

    /// A member's editing state from what the server's list says of them.
    fn member_edit(member: &MemberSummary) -> MemberEdit {
        MemberEdit {
            account_id: member.account_id.clone(),
            roles: member.role_ids.clone(),
            title: TextEdit::new(&member.title),
            ..MemberEdit::default()
        }
    }

    /// Opens a member's row in Members to what may be changed of them, or closes it.
    pub fn toggle_member(&mut self, account_id: &str) {
        if self
            .roles
            .member
            .as_ref()
            .is_some_and(|open| open.account_id == account_id)
        {
            self.roles.member = None;
            return;
        }
        let Some(server) = self.settings_summary() else {
            return;
        };
        let Some(member) = self.session.member_in(&server.server_id, account_id) else {
            return;
        };
        let me = account_id == self.session.me();
        if !roles::offers_any(roles::member_offers(server, member, me)) {
            return;
        }
        self.roles.member = Some(Self::member_edit(member));
    }

    /// Cancel: the row closes with nothing kept.
    pub fn cancel_member(&mut self) {
        self.roles.member = None;
    }

    /// Ticks a role in the open member's row.
    pub fn tick_member_role(&mut self, role_id: &str) {
        let listed = self.roles_listed();
        if let Some(open) = &mut self.roles.member {
            open.roles = roles::toggled_role(&listed, &open.roles, role_id);
            open.error.clear();
        }
    }

    /// What of `edit` differs from the member as the server has them, among what this account
    /// is offered: the roles to send, and the title.
    fn member_changes(
        server: &ServerSummary,
        member: &MemberSummary,
        me: bool,
        edit: &MemberEdit,
    ) -> (Option<Vec<String>>, Option<String>) {
        let offers = roles::member_offers(server, member, me);
        let same = edit.roles.len() == member.role_ids.len()
            && edit.roles.iter().all(|id| member.role_ids.contains(id));
        let title = edit.title.text.trim();
        (
            (offers.roles && !same).then(|| edit.roles.clone()),
            (offers.title && title != member.title).then(|| title.to_string()),
        )
    }

    /// Save, in a member's row: what changed is sent; nothing changed, and the row closes.
    pub fn save_member(&mut self) {
        let Some(server) = self.settings_summary() else {
            return;
        };
        let Some(open) = &self.roles.member else {
            return;
        };
        if open.busy {
            return;
        }
        let Some(member) = self.session.member_in(&server.server_id, &open.account_id) else {
            self.roles.member = None;
            return;
        };
        let me = open.account_id == self.session.me();
        let (roles, title) = Self::member_changes(server, member, me, open);
        if roles.is_none() && title.is_none() {
            self.roles.member = None;
            return;
        }
        let (server_id, account_id) = (server.server_id.clone(), open.account_id.clone());
        if let Some(open) = &mut self.roles.member {
            open.busy = true;
            open.error.clear();
        }
        self.session
            .save_member(&server_id, &account_id, roles, title);
    }

    /// Kick: the member is removed from the server. Nothing is asked first.
    pub fn kick_member(&mut self) {
        let Some(server) = self.settings_summary() else {
            return;
        };
        let Some(open) = &self.roles.member else {
            return;
        };
        let me = open.account_id == self.session.me();
        let offered = self
            .session
            .member_in(&server.server_id, &open.account_id)
            .is_some_and(|member| roles::member_offers(server, member, me).kick);
        if open.busy || !offered {
            return;
        }
        let (server_id, account_id) = (server.server_id.clone(), open.account_id.clone());
        if let Some(open) = &mut self.roles.member {
            open.busy = true;
            open.error.clear();
        }
        self.session.kick(&server_id, &account_id);
    }

    // ---- The profile card ---------------------------------------------------------------

    /// The server on screen and the person on the card as its member list has them.
    pub fn card_member(&self) -> Option<(&ServerSummary, &MemberSummary)> {
        let server = self.session.server(self.server_id()?)?;
        let person = self.person.as_ref()?;
        let member = self
            .members_here()
            .iter()
            .find(|m| m.account_id == person.account_id)?;
        Some((server, member))
    }

    /// Edit, beside the card's title or its roles: the small form, in the card's place.
    pub fn edit_on_card(&mut self, form: CardForm) {
        let Some((server, member)) = self.card_member() else {
            return;
        };
        let me = member.account_id == self.session.me();
        let offers = roles::member_offers(server, member, me);
        let offered = match form {
            CardForm::Roles => offers.roles,
            CardForm::Title => offers.title,
        };
        if !offered {
            return;
        }
        self.roles.card = Some(CardEdit {
            form,
            member: Self::member_edit(member),
        });
        self.open_popover(match form {
            CardForm::Roles => Popover::CardRoles,
            CardForm::Title => Popover::CardTitle,
        });
    }

    /// Ticks a role in the card's form.
    pub fn tick_card_role(&mut self, role_id: &str) {
        let listed = self
            .server_id()
            .map(|id| self.session.roles_of(id))
            .unwrap_or_default();
        if let Some(card) = &mut self.roles.card {
            card.member.roles = roles::toggled_role(&listed, &card.member.roles, role_id);
            card.member.error.clear();
        }
    }

    /// Cancel, and what a save that changed nothing does: back to the card.
    pub fn close_card_form(&mut self) {
        self.roles.card = None;
        if self.person.is_some() {
            self.open_popover(Popover::Person);
        } else {
            self.close_popover();
        }
    }

    /// Save, in the card's form.
    pub fn save_card_form(&mut self) {
        let Some((server, member)) = self.card_member() else {
            return;
        };
        let Some(card) = &self.roles.card else {
            return;
        };
        if card.member.busy {
            return;
        }
        let me = member.account_id == self.session.me();
        let (roles, title) = Self::member_changes(server, member, me, &card.member);
        let (roles, title) = match card.form {
            CardForm::Roles => (roles, None),
            CardForm::Title => (None, title),
        };
        if roles.is_none() && title.is_none() {
            self.close_card_form();
            return;
        }
        let (server_id, account_id) = (server.server_id.clone(), member.account_id.clone());
        if let Some(card) = &mut self.roles.card {
            card.member.busy = true;
            card.member.error.clear();
        }
        self.session
            .save_member(&server_id, &account_id, roles, title);
    }

    // ---- What the session reports -------------------------------------------------------

    pub(super) fn react_role(&mut self, update: &RoleUpdate) {
        let here = self.settings_server();
        let server_name = |s: &Self, id: &str| {
            s.session
                .server(id)
                .map(|server| server.name.clone())
                .unwrap_or_default()
        };
        match update {
            RoleUpdate::Saved {
                server_id,
                role_id,
                result,
            } => {
                if here.as_deref() != Some(server_id.as_str()) {
                    return;
                }
                let name = server_name(self, server_id);
                let Some(editor) = self
                    .roles
                    .editor
                    .as_mut()
                    .filter(|e| e.saving.busy && e.role_id == *role_id)
                else {
                    return;
                };
                editor.saving = Saving::default();
                match result {
                    Ok(snapshot) => {
                        // The role as the instance now has it: by its id, and a new one by
                        // its name, which is its own in the server whatever its capitals.
                        let named = editor.name.text.trim().to_lowercase();
                        let role = snapshot
                            .servers
                            .iter()
                            .find(|s| s.server_id == *server_id)
                            .and_then(|server| {
                                server.roles.iter().find(|r| match role_id {
                                    Some(id) => r.role_id == *id,
                                    None => !r.is_everyone && r.name.to_lowercase() == named,
                                })
                            });
                        match role {
                            Some(role) => {
                                let mut now = RoleEditor::of(Some(role));
                                now.saving.saved = true;
                                self.roles.editor = Some(now);
                            }
                            None => self.roles.editor = None,
                        }
                    }
                    Err(failure) => editor.saving.error = roles::role_refusal(failure, &name),
                }
            }
            RoleUpdate::Deleted {
                server_id,
                role_id,
                result,
            } => {
                let asked = self
                    .roles
                    .deleting
                    .as_ref()
                    .is_some_and(|d| d.role_id == *role_id);
                match result {
                    Ok(_) => {
                        if asked {
                            self.roles.deleting = None;
                            if self.popover == Some(Popover::DeleteRole) {
                                self.close_popover();
                            }
                        }
                        if self
                            .roles
                            .editor
                            .as_ref()
                            .is_some_and(|e| e.role_id.as_deref() == Some(role_id.as_str()))
                        {
                            self.roles.editor = None;
                        }
                    }
                    Err(failure) => {
                        let why = roles::role_refusal(failure, &server_name(self, server_id));
                        if let Some(deleting) = self.roles.deleting.as_mut().filter(|_| asked) {
                            deleting.busy = false;
                            deleting.error = why;
                        }
                    }
                }
            }
            RoleUpdate::Ordered {
                server_id,
                sent,
                result,
            } => {
                if *sent != self.session.roles.sent() {
                    return;
                }
                self.roles.list_error = match result {
                    Ok(_) => String::new(),
                    Err(failure) => roles::role_refusal(failure, &server_name(self, server_id)),
                };
            }
            RoleUpdate::Member {
                account_id, result, ..
            } => {
                let why = result.as_ref().err().map(roles::member_refusal);
                if let Some(editor) = self
                    .roles
                    .editor
                    .as_mut()
                    .filter(|e| e.removing.as_deref() == Some(account_id.as_str()))
                {
                    editor.removing = None;
                    editor.holders_error = why.clone().unwrap_or_default();
                }
                if let Some(open) = self
                    .roles
                    .member
                    .as_mut()
                    .filter(|open| open.busy && open.account_id == *account_id)
                {
                    match &why {
                        None => self.roles.member = None,
                        Some(why) => {
                            open.busy = false;
                            open.error.clone_from(why);
                        }
                    }
                }
                if let Some(card) = self
                    .roles
                    .card
                    .as_mut()
                    .filter(|card| card.member.busy && card.member.account_id == *account_id)
                {
                    match &why {
                        None => self.close_card_form(),
                        Some(why) => {
                            card.member.busy = false;
                            card.member.error.clone_from(why);
                        }
                    }
                }
            }
            RoleUpdate::Kicked {
                account_id, result, ..
            } => {
                let Some(open) = self
                    .roles
                    .member
                    .as_mut()
                    .filter(|open| open.busy && open.account_id == *account_id)
                else {
                    return;
                };
                match result {
                    Ok(()) => self.roles.member = None,
                    Err(failure) => {
                        open.busy = false;
                        open.error = roles::member_refusal(failure);
                    }
                }
            }
            RoleUpdate::Given { result, .. } => {
                let form = &mut self.roles.adding;
                if !form.busy {
                    return;
                }
                form.busy = false;
                match result {
                    Ok(()) => {
                        form.chosen.clear();
                        if self.popover == Some(Popover::AddHolders) {
                            self.close_popover();
                        }
                    }
                    Err(failure) => form.error = roles::member_refusal(failure),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role(id: &str, can_manage: bool, is_everyone: bool) -> RoleSummary {
        RoleSummary {
            role_id: id.into(),
            name: id.into(),
            color: String::new(),
            position: 0,
            is_everyone,
            hide_tag: false,
            permissions: Vec::new(),
            can_manage,
        }
    }

    /// Five rows 56 tall and 6 apart from y 100, as the list lays them out: Admin (locked),
    /// Design, Engineering, Can invite, Everyone.
    fn listed() -> (Vec<RoleSummary>, HashMap<String, [f32; 4]>) {
        let roles = vec![
            role("admin", false, false),
            role("design", true, false),
            role("eng", true, false),
            role("invite", true, false),
            role("everyone", true, true),
        ];
        let spots = roles
            .iter()
            .enumerate()
            .map(|(i, r)| {
                (
                    r.role_id.clone(),
                    [500.0, 100.0 + 62.0 * i as f32, 640.0, 56.0],
                )
            })
            .collect();
        (roles, spots)
    }

    #[test]
    fn a_dragged_role_lands_before_the_row_whose_middle_is_under_the_pointer() {
        let (roles, spots) = listed();
        let at = |y| drop_before(&roles, &spots, "eng", y);
        // Over Design's upper half: before Design.
        assert_eq!(at(170.0).as_deref(), Some("design"));
        // Over Design's lower half is before Engineering itself, and over its own row: nothing.
        assert_eq!(at(200.0), None);
        assert_eq!(at(240.0), None);
        assert_eq!(at(270.0), None);
        // Past its own middle and Can invite's: before Everyone, the last place there is.
        assert_eq!(at(330.0).as_deref(), Some("everyone"));
        assert_eq!(at(900.0).as_deref(), Some("everyone"));
        // Over Admin, which is at or above this account's highest: nowhere.
        assert_eq!(at(110.0), None);
        // A locked role does not move at all.
        assert_eq!(drop_before(&roles, &spots, "admin", 330.0), None);
    }

    #[test]
    fn a_fields_words_are_kept_to_thirty_two_characters() {
        let mut edit = TextEdit::new("é".repeat(40));
        keep_to_label(&mut edit);
        assert_eq!(edit.text.chars().count(), 32);
        let mut short = TextEdit::new("Staff Engineer");
        keep_to_label(&mut short);
        assert_eq!(short.text, "Staff Engineer");
    }
}
