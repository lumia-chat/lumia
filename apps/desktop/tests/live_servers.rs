//! Making and managing servers against a real instance, through the session and the state
//! the screens call: one account creates a server and a channel in it and makes an invite;
//! a second account previews the invite, joins, and lands in the server; the first lists
//! its invites, mails one, and revokes them, after which the link leads nowhere. Runs only
//! with `LUMIA_DEV_INSTANCE` set; without it it passes having checked nothing, and says so.
//! It makes its own accounts, so it needs an instance with open registration.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lumia_shell::app::{HomeView, Place, Popover, Route, State};
use lumia_shell::session::Connection;
use rune::TextEdit;

const PASSWORD: &str = "correct-horse-battery-staple";

fn wait(state: &mut State, what: &str, done: impl Fn(&State) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !done(state) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; session error {:?}, sign-in {:?}, add {:?} / {:?}, channel {:?}, invite {:?}",
            state.session.last_error,
            state.sign_in.error,
            state.server_forms.add.create_error,
            state.server_forms.add.error,
            state.server_forms.channel.error,
            state.server_forms.invite.error,
        );
        state.drain();
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A state over `dir`, signed in to a new account on `instance`.
fn new_account(instance: &str, dir: &Path, handle: &str) -> State {
    let mut state = State::new(dir.to_path_buf());
    state.sign_in.instance = TextEdit::new(instance);
    state.connect();
    wait(&mut state, "the trust card", |s| s.session.info.is_some());
    state.sign_in.creating = true;
    state.sign_in.handle = TextEdit::new(handle);
    state.sign_in.password.replace_text(PASSWORD);
    state.submit();
    wait(&mut state, "the shell", |s| s.route == Route::Shell);
    wait(&mut state, "the first snapshot", |s| {
        s.session.snapshot.is_some()
    });
    state
}

#[test]
fn a_server_is_made_given_a_channel_and_joined_by_invite() {
    let Ok(instance) = std::env::var("LUMIA_DEV_INSTANCE") else {
        eprintln!("skipped: no LUMIA_DEV_INSTANCE");
        return;
    };
    let dir = std::env::temp_dir().join(format!("lumia-shell-servers-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // Handles nobody else on a shared instance has.
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        % 100_000_000;
    let server_name = format!("Shell servers {stamp}");

    // ---- The owner makes a server, which opens once the snapshot lists it -------------
    let mut owner = new_account(&instance, &dir.join("owner"), &format!("srvown{stamp}"));
    assert!(owner.session.servers().is_empty(), "a new account");
    owner.go_home(HomeView::AddServer);
    owner.server_forms.add.name.replace_text(&server_name);
    owner.create_server();
    assert!(owner.server_forms.add.creating);
    wait(&mut owner, "the new server on screen", |s| {
        matches!(s.place, Place::Server(_))
    });
    let server_id = owner.server_id().unwrap().to_string();
    assert_eq!(owner.session.servers().len(), 1);
    assert_eq!(owner.session.servers()[0].name, server_name);
    assert!(!owner.server_forms.add.creating);
    assert_eq!(owner.server_forms.add.name.text, "");
    assert_eq!(
        owner.session.channel(),
        None,
        "a new server has no channel yet"
    );

    // ---- A channel in it, which opens once the snapshot lists it ----------------------
    owner.open_new_channel();
    assert_eq!(owner.popover, Some(Popover::NewChannel));
    owner.server_forms.channel.name.replace_text("planning");
    owner.create_channel();
    wait(&mut owner, "the new channel on screen", |s| {
        s.session.channel().is_some()
    });
    let channel_id = owner.session.channel().unwrap().to_string();
    assert_eq!(
        owner.session.channel_summary(&channel_id).unwrap().name,
        "planning"
    );
    assert_eq!(owner.popover, None, "the popover closed");
    assert_eq!(
        owner.session.channels_of(&server_id).count(),
        1,
        "listed in the sidebar"
    );

    // The gateway follows a channel that appeared after it started: a message lands.
    wait(&mut owner, "the gateway", |s| {
        s.session.connection == Connection::Connected
    });
    let body = format!("First words in planning, {stamp}");
    assert!(
        owner
            .session
            .send(&body, chrono::Utc::now().timestamp_micros()),
        "{:?}",
        owner.session.last_error
    );
    wait(&mut owner, "the message to land", |s| {
        s.session.pending.is_empty() && s.session.entries.iter().any(|e| e.body == body)
    });

    // ---- An invite: a link good for a day, for one use --------------------------------
    owner.open_invites();
    assert_eq!(owner.popover, Some(Popover::Invite));
    owner.set_invite_expiry(24);
    owner.set_invite_uses(1);
    owner.make_invite();
    assert!(owner.server_forms.invite.busy);
    wait(&mut owner, "the invite link", |s| {
        s.server_forms.invite.made.is_some()
    });
    let made = owner.server_forms.invite.made.clone().unwrap();
    assert!(
        made.url.starts_with(instance.trim_end_matches('/')) && made.url.ends_with(&made.code),
        "{}",
        made.url
    );
    assert_eq!((made.max_uses, made.uses), (1, 0));
    let in_a_day = chrono::Utc::now().timestamp_micros() + 24 * 3_600_000_000;
    let expires = made.expires_at_micros.expect("it expires");
    assert!(
        (expires - in_a_day).abs() < 120_000_000,
        "expires in a day: {expires}"
    );
    assert_eq!(owner.server_forms.invite.list[0].code, made.code);
    assert_eq!(owner.server_forms.invite.error, "");

    // ---- A second account sees what the link leads to, then joins ---------------------
    let mut guest = new_account(&instance, &dir.join("guest"), &format!("srvjoin{stamp}"));
    guest.go_home(HomeView::AddServer);
    guest.server_forms.add.invite.replace_text(&made.url);
    guest.look_up_invite();
    wait(&mut guest, "the invite's preview", |s| {
        s.server_forms.add.preview.is_some()
    });
    let preview = guest.server_forms.add.preview.clone().unwrap();
    assert_eq!(preview.server_name, server_name);
    assert_eq!(preview.server_id, server_id);
    assert_eq!(preview.member_count, 1);
    assert!(!preview.host.is_empty(), "the instance is named");
    assert!(
        guest.session.servers().is_empty() && guest.place == Place::Home,
        "a preview joins nothing"
    );

    guest.join_invite();
    wait(&mut guest, "the joined server on screen", |s| {
        s.server_id() == Some(server_id.as_str())
    });
    assert_eq!(
        guest.session.channel(),
        Some(channel_id.as_str()),
        "on its channel"
    );
    assert_eq!(guest.server_forms.add.invite.text, "");
    assert!(guest.server_forms.add.preview.is_none());
    wait(&mut guest, "the channel's history", |s| {
        s.session.entries.iter().any(|e| e.body == body)
    });

    // The link worked once and is used up: looking it up again says so.
    guest.go_home(HomeView::AddServer);
    guest.server_forms.add.invite.replace_text(&made.code);
    guest.look_up_invite();
    wait(&mut guest, "the spent invite's answer", |s| {
        !s.server_forms.add.busy
    });
    assert_eq!(
        guest.server_forms.add.error,
        "That invite is unknown, expired or used up."
    );
    assert!(guest.server_forms.add.preview.is_none());

    // Someone who joined by invite may not add channels, and is told so.
    guest.select_server(&server_id);
    guest.open_new_channel();
    guest.server_forms.channel.name.replace_text("mine");
    guest.create_channel();
    wait(&mut guest, "the refusal", |s| !s.server_forms.channel.busy);
    assert_eq!(
        guest.server_forms.channel.error,
        "You cannot add channels to this server."
    );
    assert_eq!(guest.popover, Some(Popover::NewChannel));
    guest.close_popover();

    // ---- The owner's list shows the use; a mailed invite joins it ---------------------
    owner.close_popover();
    owner.open_invites();
    wait(&mut owner, "the invite's use in the list", |s| {
        s.server_forms
            .invite
            .list
            .iter()
            .any(|i| i.code == made.code && i.uses == 1)
    });
    let address = format!("pat{stamp}@example.com");
    owner.server_forms.invite.email.replace_text(&address);
    owner.email_invite();
    wait(&mut owner, "the mailed invite", |s| {
        !s.server_forms.invite.busy
    });
    assert_eq!(owner.server_forms.invite.error, "");
    assert_eq!(owner.server_forms.invite.emailed, address);
    assert_eq!(owner.server_forms.invite.email.text, "");
    let mailed = owner.server_forms.invite.list[0].clone();
    assert_eq!(
        (mailed.email.as_str(), mailed.max_uses, mailed.uses),
        (address.as_str(), 1, 0)
    );
    assert!(mailed.expires_at_micros.is_some(), "good for a week");
    assert_eq!(owner.server_forms.invite.list.len(), 2);

    // An address that is not one is refused, in the popover's words.
    owner
        .server_forms
        .invite
        .email
        .replace_text("not an address");
    owner.email_invite();
    wait(&mut owner, "the refused address", |s| {
        !s.server_forms.invite.busy
    });
    assert_eq!(
        owner.server_forms.invite.error,
        "That is not an email address."
    );

    // ---- Revoking takes an invite off the list and stops it working -------------------
    owner.revoke_invite(&mailed.code);
    wait(&mut owner, "the revocation", |s| {
        !s.server_forms.invite.busy
    });
    assert_eq!(owner.server_forms.invite.error, "");
    assert!(
        owner
            .server_forms
            .invite
            .list
            .iter()
            .all(|i| i.code != mailed.code)
    );
    owner.revoke_invite(&made.code);
    wait(&mut owner, "the second revocation", |s| {
        !s.server_forms.invite.busy
    });
    assert!(owner.server_forms.invite.list.is_empty());
    assert!(
        owner.server_forms.invite.made.is_none(),
        "the revoked link is no longer offered to copy"
    );

    // The instance agrees: asked again, it lists none, and the mailed code leads nowhere.
    owner.close_popover();
    owner.open_invites();
    guest.go_home(HomeView::AddServer);
    guest.server_forms.add.invite.replace_text(&mailed.code);
    guest.look_up_invite();
    wait(&mut guest, "the revoked invite's answer", |s| {
        !s.server_forms.add.busy
    });
    assert_eq!(
        guest.server_forms.add.error,
        "That invite is unknown, expired or used up."
    );
    // The list answer has had the time of a round trip by now; drain it.
    std::thread::sleep(Duration::from_millis(500));
    owner.drain();
    assert!(owner.server_forms.invite.list.is_empty());
    assert_eq!(owner.server_forms.invite.error, "");
}

/// A server's owner changes its name, retention and picture from its settings (architecture
/// 6.7); a member, told through the account log, sees the new name and draws the picture,
/// and is not offered the settings.
#[test]
fn a_servers_settings_are_changed_and_every_member_sees_them() {
    let Ok(instance) = std::env::var("LUMIA_DEV_INSTANCE") else {
        eprintln!("skipped: no LUMIA_DEV_INSTANCE");
        return;
    };
    let dir = std::env::temp_dir().join(format!("lumia-shell-settings-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        % 100_000_000;

    let mut owner = new_account(&instance, &dir.join("owner"), &format!("setown{stamp}"));
    owner.go_home(HomeView::AddServer);
    owner
        .server_forms
        .add
        .name
        .replace_text(format!("Before {stamp}"));
    owner.create_server();
    wait(&mut owner, "the new server on screen", |s| {
        matches!(s.place, Place::Server(_))
    });
    let server_id = owner.server_id().unwrap().to_string();
    owner.open_invites();
    owner.make_invite();
    wait(&mut owner, "the invite link", |s| {
        s.server_forms.invite.made.is_some()
    });
    let link = owner.server_forms.invite.made.clone().unwrap().url;
    owner.close_popover();

    let mut member = new_account(&instance, &dir.join("member"), &format!("setmem{stamp}"));
    member.go_home(HomeView::AddServer);
    member.server_forms.add.invite.replace_text(&link);
    member.look_up_invite();
    wait(&mut member, "the invite's preview", |s| {
        s.server_forms.add.preview.is_some()
    });
    member.join_invite();
    wait(&mut member, "the joined server on screen", |s| {
        s.server_id() == Some(server_id.as_str())
    });
    let theirs = |s: &State| {
        s.session
            .servers()
            .iter()
            .find(|x| x.server_id == server_id)
            .cloned()
            .unwrap()
    };
    assert!(!theirs(&member).can_manage, "a member is not offered them");
    assert!(theirs(&owner).can_manage, "the owner is");

    // ---- The owner renames the server and keeps its messages for 90 days ------------------
    owner.open_server_settings();
    assert!(owner.server_settings_open());
    owner
        .server_forms
        .settings
        .name
        .replace_text(format!("After {stamp}"));
    owner.save_server_name();
    wait(&mut owner, "the name saved", |s| {
        s.server_forms.settings.name_saving.saved
    });
    owner.choose_retention(90);
    owner.save_server_retention();
    wait(&mut owner, "the retention saved", |s| {
        s.server_forms.settings.retention_saving.saved
    });
    assert_eq!(theirs(&owner).name, format!("After {stamp}"));
    assert_eq!(theirs(&owner).retention_days, 90);

    // ---- A picture, from a file, as a person chooses one -----------------------------
    let photo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../instance/test/support/fixtures/media/sideways.jpg");
    owner.set_server_picture(&photo);
    wait(&mut owner, "the picture saved", |s| {
        s.server_forms.settings.picture.saved
    });
    let icon = theirs(&owner).icon_url;
    assert!(icon.starts_with("/api/v1/avatars/"), "{icon}");

    // ---- The member hears of it through the account log, and draws the picture ----------
    wait(&mut member, "the member's snapshot to change", |s| {
        theirs(s).name == format!("After {stamp}") && theirs(s).icon_url == icon
    });
    assert_eq!(theirs(&member).retention_days, 90);
    wait(&mut member, "the picture's bytes", |s| {
        s.session
            .avatars
            .held()
            .any(|(url, _)| url == icon.as_str())
    });
}
