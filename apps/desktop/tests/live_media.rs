//! Attachments against a real instance (specs/media.md): two accounts in one channel, each
//! a shell of its own. One attaches a picture from a file, gives it alt text and sends it;
//! the other sees it arrive, has its picture fetched, opened and decoded, opens the viewer
//! (the display picture fetched), and saves it; then the sender's edit takes the picture out
//! and it goes from the other's screen. Runs only with `LUMIA_DEV_INSTANCE` set; without it
//! it passes having checked nothing, and says so. It makes its own accounts, so it needs an
//! instance with open registration that keeps attachments (a directory store will do).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lumia_shell::app::{HomeView, Place, Route, State};
use lumia_shell::session::Connection;
use lumia_shell::session::media::{Stage, blob_key, still_key};
use rune::TextEdit;

const PASSWORD: &str = "correct-horse-battery-staple";
const ICON: &[u8] = include_bytes!("../../../assets/brand/lumia-original-icon.png");

fn turn(state: &mut State) {
    state.drain();
    state.tick(chrono::Utc::now().timestamp_micros());
}

fn wait(state: &mut State, what: &str, done: impl Fn(&State) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(state) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; session error {:?}, sign-in {:?}",
            state.session.last_error,
            state.sign_in.error,
        );
        turn(state);
        std::thread::sleep(Duration::from_millis(20));
    }
}

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
fn a_picture_sent_from_one_shell_is_seen_opened_and_saved_in_another() {
    let Ok(instance) = std::env::var("LUMIA_DEV_INSTANCE") else {
        eprintln!("skipped: no LUMIA_DEV_INSTANCE");
        return;
    };
    let dir = std::env::temp_dir().join(format!("lumia-shell-media-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        % 100_000_000;

    // ---- Two accounts in one channel ----------------------------------------------------
    let mut owner = new_account(&instance, &dir.join("owner"), &format!("medada{stamp}"));
    owner.go_home(HomeView::AddServer);
    owner
        .server_forms
        .add
        .name
        .replace_text(format!("Shell media {stamp}"));
    owner.create_server();
    wait(&mut owner, "the new server on screen", |s| {
        matches!(s.place, Place::Server(_))
    });
    owner.open_new_channel();
    owner.server_forms.channel.name.replace_text("general");
    owner.create_channel();
    wait(&mut owner, "the new channel on screen", |s| {
        s.session.channel().is_some()
    });
    let channel_id = owner.session.channel().unwrap().to_string();
    wait(&mut owner, "the gateway", |s| {
        s.session.connection == Connection::Connected
    });
    owner.open_invites();
    owner.make_invite();
    wait(&mut owner, "the invite link", |s| {
        s.server_forms.invite.made.is_some()
    });
    let link = owner.server_forms.invite.made.clone().unwrap().url;
    owner.close_popover();
    let mut guest = new_account(&instance, &dir.join("guest"), &format!("medgrace{stamp}"));
    guest.go_home(HomeView::AddServer);
    guest.server_forms.add.invite.replace_text(&link);
    guest.look_up_invite();
    wait(&mut guest, "the invite's preview", |s| {
        s.server_forms.add.preview.is_some()
    });
    guest.join_invite();
    wait(&mut guest, "the joined channel on screen", |s| {
        s.session.channel() == Some(channel_id.as_str())
    });
    wait(&mut guest, "the gateway", |s| {
        s.session.connection == Connection::Connected
    });
    // The instance said what it takes: the composer offers attaching.
    wait(&mut guest, "the upload limits", |s| s.session.can_attach());

    // ---- The guest attaches a picture from a file and sends it -------------------------
    let file = dir.join("mark.png");
    std::fs::write(&file, ICON).unwrap();
    guest.session.attach_path(&file);
    assert!(guest.session.uploading_here());
    assert!(!guest.can_send(), "nothing goes while it uploads");
    wait(&mut guest, "the upload", |s| {
        s.session
            .waiting_here()
            .all(|w| matches!(w.stage, Stage::Ready(_)))
    });
    let local = guest.session.waiting_here().next().unwrap().local;
    guest.alt_field(local).replace_text("The Lumia mark");
    guest.alt_changed(local);
    let words = format!("The mark, {stamp}");
    assert!(guest.can_send());
    assert!(
        guest
            .session
            .send(&words, chrono::Utc::now().timestamp_micros()),
        "sent: {:?}",
        guest.session.last_error
    );
    assert_eq!(
        guest.session.waiting_here().count(),
        0,
        "the strip is empty"
    );
    wait(&mut guest, "the send to land", |s| {
        s.session.pending.is_empty()
            && s.session
                .entries
                .iter()
                .any(|e| e.body == words && e.attachments.len() == 1)
    });

    // ---- The owner sees it, its picture decoded and made small -------------------------
    wait(&mut owner, "the picture at the owner", |s| {
        s.session
            .entries
            .iter()
            .any(|e| e.body == words && e.attachments.len() == 1)
    });
    let entry = owner
        .session
        .entries
        .iter()
        .find(|e| e.body == words)
        .unwrap()
        .clone();
    let picture = entry.attachments[0].clone();
    assert_eq!(picture.kind, "image");
    assert_eq!(picture.alt, "The Lumia mark");
    assert!(picture.width > 0 && picture.height > 0 && !picture.placeholder.is_empty());
    assert_eq!(picture.thumbnail, None, "an image has no thumbnail");
    let shown = still_key(&picture).expect("an image is drawn from its display picture");
    wait(&mut owner, "the picture, opened and decoded", |s| {
        s.media
            .image(&shown)
            .is_some_and(|i| i.is_ready() && i.error().is_none())
    });
    assert!(owner.media.placeholder(&picture.attachment_id).is_some());

    // ---- The viewer fetches the display picture; Save writes it -----------------------
    owner.open_viewer(&picture);
    let main = blob_key(&picture.main);
    wait(&mut owner, "the display picture", |s| {
        s.session.media.bytes(&main).is_some()
    });
    owner.close_viewer();
    let saved = dir.join("saved.png");
    owner.session.save_attachment(&picture, &saved);
    wait(&mut owner, "the saved file", |_| {
        std::fs::metadata(&saved).is_ok_and(|m| m.len() > 0)
    });
    let bytes = std::fs::read(&saved).unwrap();
    assert_eq!(
        bytes.as_slice(),
        &owner.session.media.bytes(&main).unwrap()[..],
        "what was saved is the picture the viewer drew"
    );
    assert!(
        bytes.starts_with(b"\x89PNG"),
        "the mark keeps its transparency as a PNG"
    );

    // ---- The guest's edit takes it out ---------------------------------------------------
    let id = entry.id.clone();
    guest.session.edit(
        &id,
        &words,
        std::slice::from_ref(&picture.attachment_id),
        &[],
    );
    wait(&mut owner, "the edit at the owner", |s| {
        s.session
            .entries
            .iter()
            .any(|e| e.id == id && e.attachments.is_empty())
    });
    let _ = std::fs::remove_dir_all(&dir);
}
