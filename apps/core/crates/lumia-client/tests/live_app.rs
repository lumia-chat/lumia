//! The shell-facing client against a running instance: what a device does
//! across sign-ins, which no fake instance here can stand in for. Each test
//! skips itself without `LUMIA_TEST_INSTANCE` (an instance's base URL, such
//! as `http://localhost:4000`) and makes its own accounts there.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lumia_client::api::Api;
use lumia_client::app::{App, AppEvent, EventSink, Opener, Place, StoreName};
use lumia_client::sqlite::SqliteStore;

const PASSWORD: &str = "correct-horse-battery-staple";

fn instance() -> Option<String> {
    let base = std::env::var("LUMIA_TEST_INSTANCE").ok();
    if base.is_none() {
        eprintln!("LUMIA_TEST_INSTANCE is not set: skipped");
    }
    base
}

fn stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

/// A device: its stores in a directory of its own, as a shell keeps them.
struct Device {
    dir: PathBuf,
    app: Arc<App>,
    events: Arc<Mutex<Vec<AppEvent>>>,
}

impl Device {
    fn new(base: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("lumia-live-{}", stamp()));
        std::fs::create_dir_all(&dir).unwrap();
        let at = dir.clone();
        let opener: Opener = Arc::new(move |name| {
            SqliteStore::open(&at.join(match name {
                StoreName::Instance => "store.sqlite",
                StoreName::Keys => "keys.sqlite",
                StoreName::IdentityStore => "identity.sqlite",
                StoreName::IdentityKeys => "identity-keys.sqlite",
            }))
        });
        let app = App::open(base, Place::Native, opener).unwrap();
        Self {
            dir,
            app,
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Starts the gateway loop, as the shells do, with what it reports kept.
    fn start(&self, channels: &[String]) -> Result<(), lumia_client::ClientError> {
        let events = Arc::clone(&self.events);
        let sink: EventSink = Arc::new(move |event| events.lock().unwrap().push(event));
        self.app.start(channels, sink)
    }

    /// The first event since `from` that `wanted` picks, within `within`.
    async fn wait_for<T>(
        &self,
        from: usize,
        within: Duration,
        wanted: impl Fn(&AppEvent) -> Option<T>,
    ) -> Option<T> {
        let until = Instant::now() + within;
        while Instant::now() < until {
            if let Some(found) = self.events.lock().unwrap()[from..].iter().find_map(&wanted) {
                return Some(found);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        None
    }

    fn seen(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    /// Everything since `from` that says the loop is not well.
    fn troubles(&self, from: usize) -> Vec<String> {
        self.events.lock().unwrap()[from..]
            .iter()
            .filter_map(|event| match event {
                AppEvent::Reconnecting { reason, .. } => Some(format!("reconnecting: {reason}")),
                AppEvent::Closed { reason } => Some(format!("closed: {reason}")),
                _ => None,
            })
            .collect()
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        self.app.stop();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// An event in an account's log, made beside the device: a session of the
/// account's own, signed in on another device and signed out again, which
/// the instance records as `SessionRevoked` (making a server records
/// nothing there for its owner).
async fn account_event(base: &str, handle: &str) {
    let api = Api::new(base);
    let pair = api
        .sign_in(handle, PASSWORD, lumia_core::wire::new_id())
        .await
        .unwrap();
    api.take_pair(&pair);
    api.sign_out().await.unwrap();
}

fn account_applied(event: &AppEvent) -> Option<()> {
    matches!(event, AppEvent::AccountApplied { .. }).then_some(())
}

fn welcomed(event: &AppEvent) -> Option<String> {
    match event {
        AppEvent::Welcomed { account_id } => Some(account_id.clone()),
        _ => None,
    }
}

/// Starts the loop once the last one has given the store back: until then
/// a start is refused as "already started". Within five seconds.
async fn start_when_free(device: &Device, channels: &[String]) {
    let until = Instant::now() + Duration::from_secs(5);
    while let Err(e) = device.start(channels) {
        assert!(
            Instant::now() < until,
            "the last loop did not end within 5 s: {e}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Stops the loop and shows that it ended, by starting another (and
/// stopping that one).
async fn stopped(device: &Device) {
    device.app.stop();
    start_when_free(device, &[]).await;
    device.app.stop();
}

/// One account signs out and another signs in on the same device. The
/// account log's cursor of the first stayed in the store and the second's
/// loop resumed its own log from it: the second's events past that point
/// were refused as a gap, and the loop reconnected for ever.
#[tokio::test(flavor = "multi_thread")]
async fn a_second_account_on_the_same_device_follows_its_own_log() {
    let Some(base) = instance() else { return };
    let device = Device::new(&base);
    let id = stamp();

    let (one, two) = (format!("one{id}"), format!("two{id}"));
    device
        .app
        .create_account(&one, PASSWORD, None)
        .await
        .unwrap();
    let from = device.seen();
    device.start(&[]).unwrap();
    let first = device
        .wait_for(from, Duration::from_secs(10), welcomed)
        .await
        .expect("the first account is welcomed");
    // An account event while the loop runs, which it applies: the store's
    // account cursor is then past the start.
    let live = device.seen();
    account_event(&base, &one).await;
    device
        .wait_for(live, Duration::from_secs(10), account_applied)
        .await
        .expect("the first account's event is applied");
    stopped(&device).await;
    device.app.sign_out().await.unwrap();

    // The second account's log is longer than the first's was.
    device
        .app
        .create_account(&two, PASSWORD, None)
        .await
        .unwrap();
    for _ in 0..3 {
        account_event(&base, &two).await;
    }
    let from = device.seen();
    start_when_free(&device, &[]).await;
    let second = device
        .wait_for(from, Duration::from_secs(10), welcomed)
        .await
        .expect("the second account is welcomed");
    assert_ne!(first, second);
    // And it is well: a live event of its own is applied.
    let live = device.seen();
    account_event(&base, &two).await;
    device
        .wait_for(live, Duration::from_secs(10), account_applied)
        .await
        .expect("the second account's live event is applied");
    assert_eq!(device.troubles(from), Vec::<String>::new());
}

/// Another account signs in while the first one's loop is running, with no
/// sign-out between: the loop, told its session was revoked, comes back on
/// the new one with the first account's cursor still in the store. The
/// welcome names the second account, so the loop starts that account's log
/// afresh, once, and goes on.
#[tokio::test(flavor = "multi_thread")]
async fn another_account_signing_in_over_a_running_loop_starts_its_own_log() {
    let Some(base) = instance() else { return };
    let device = Device::new(&base);
    let id = stamp();
    let (one, two) = (format!("first{id}"), format!("second{id}"));

    // The second account first, made elsewhere, with a longer log.
    let elsewhere = Device::new(&base);
    elsewhere
        .app
        .create_account(&two, PASSWORD, None)
        .await
        .unwrap();
    for _ in 0..3 {
        account_event(&base, &two).await;
    }

    device
        .app
        .create_account(&one, PASSWORD, None)
        .await
        .unwrap();
    let from = device.seen();
    device.start(&[]).unwrap();
    let first = device
        .wait_for(from, Duration::from_secs(10), welcomed)
        .await
        .expect("the first account is welcomed");
    let live = device.seen();
    account_event(&base, &one).await;
    device
        .wait_for(live, Duration::from_secs(10), account_applied)
        .await
        .expect("the first account's event is applied");

    let switched = device.seen();
    device.app.sign_in(&two, PASSWORD, None).await.unwrap();
    let second = device
        .wait_for(switched, Duration::from_secs(15), |e| {
            welcomed(e).filter(|account| *account != first)
        })
        .await
        .expect("the loop comes back as the second account");
    let live = device.seen();
    account_event(&base, &two).await;
    device
        .wait_for(live, Duration::from_secs(10), account_applied)
        .await
        .expect("the second account's live event is applied");
    assert_ne!(first, second);
    let troubles: Vec<_> = device
        .troubles(switched)
        .into_iter()
        .filter(|t| t != "reconnecting: the store followed another account")
        .collect();
    assert_eq!(troubles, Vec::<String>::new());
}

/// A device signs in again over the session it holds (a phone linked after
/// a password sign-in). The held session is revoked, and the gateway loop
/// running on it used to end for good on that revocation; it now comes back
/// on the new session, and sending works.
#[tokio::test(flavor = "multi_thread")]
async fn a_sign_in_over_a_held_session_keeps_the_gateway() {
    let Some(base) = instance() else { return };
    let device = Device::new(&base);
    let handle = format!("over{}", stamp());

    device
        .app
        .create_account(&handle, PASSWORD, None)
        .await
        .unwrap();
    let server = device.app.create_server("Over").await.unwrap();
    let channel = device
        .app
        .create_channel(&server.server_id, "general", false)
        .await
        .unwrap();
    let from = device.seen();
    device
        .start(std::slice::from_ref(&channel.channel_id))
        .unwrap();
    device
        .wait_for(from, Duration::from_secs(10), welcomed)
        .await
        .expect("welcomed on the first session");

    let again = device.seen();
    device.app.sign_in(&handle, PASSWORD, None).await.unwrap();
    device
        .wait_for(again, Duration::from_secs(15), welcomed)
        .await
        .expect("welcomed again on the new session");

    let sent = device.seen();
    let command = device
        .app
        .send_message(&channel.channel_id, "after the second sign-in")
        .unwrap();
    device
        .wait_for(sent, Duration::from_secs(10), |e| match e {
            AppEvent::Acked { command_id, .. } if *command_id == command => Some(()),
            _ => None,
        })
        .await
        .expect("a message sent afterwards is acked");
    let closed: Vec<_> = device
        .troubles(again)
        .into_iter()
        .filter(|t| t.starts_with("closed"))
        .collect();
    assert_eq!(closed, Vec::<String>::new());
}

/// `stop` ends the loop. The event task held a sender of its own, so the
/// loop went on until the instance closed the socket: a device signing out
/// kept its gateway open, and could not start one again meanwhile.
#[tokio::test(flavor = "multi_thread")]
async fn stop_ends_the_loop() {
    let Some(base) = instance() else { return };
    let device = Device::new(&base);
    device
        .app
        .create_account(&format!("stop{}", stamp()), PASSWORD, None)
        .await
        .unwrap();
    let from = device.seen();
    device.start(&[]).unwrap();
    device
        .wait_for(from, Duration::from_secs(10), welcomed)
        .await
        .expect("welcomed");
    stopped(&device).await;
}

/// A server's owner changes its name, retention and picture (architecture
/// 6.7): another member, live, is told through its account log and finds
/// the server as it now is in its snapshot, without the settings offered.
#[tokio::test(flavor = "multi_thread")]
async fn a_servers_settings_reach_its_members() {
    let Some(base) = instance() else { return };
    let (owner, member) = (Device::new(&base), Device::new(&base));
    let id = stamp();
    owner
        .app
        .create_account(&format!("owner{id}"), PASSWORD, None)
        .await
        .unwrap();
    let server = owner.app.create_server("Before").await.unwrap();
    assert!(server.can_manage);
    let invite = owner
        .app
        .create_invite(&server.server_id, 0, None)
        .await
        .unwrap();
    member
        .app
        .create_account(&format!("member{id}"), PASSWORD, Some(&invite.url))
        .await
        .unwrap();
    let theirs = |snapshot: lumia_client::app::SnapshotSummary| {
        snapshot
            .servers
            .into_iter()
            .find(|s| s.server_id == server.server_id)
            .expect("the member is in the server")
    };
    let before = theirs(member.app.snapshot().await.unwrap());
    assert_eq!(before.name, "Before");
    assert!(!before.can_manage, "a member is not offered the settings");

    let from = member.seen();
    member.start(&[]).unwrap();
    member
        .wait_for(from, Duration::from_secs(10), welcomed)
        .await
        .expect("the member is welcomed");

    let live = member.seen();
    let mine = owner
        .app
        .update_server(&server.server_id, Some("  After  "), Some(30))
        .await
        .unwrap();
    let mine = mine
        .servers
        .into_iter()
        .find(|s| s.server_id == server.server_id)
        .unwrap();
    assert_eq!((mine.name.as_str(), mine.retention_days), ("After", 30));
    member
        .wait_for(live, Duration::from_secs(15), account_applied)
        .await
        .expect("the member's account log says the server changed");
    let after = theirs(member.app.snapshot().await.unwrap());
    assert_eq!((after.name.as_str(), after.retention_days), ("After", 30));

    // A member may not change it: the instance refuses, whatever is offered.
    let refused = member
        .app
        .update_server(&server.server_id, Some("Mine"), None)
        .await;
    assert!(
        matches!(&refused, Err(lumia_client::ClientError::Api { code, .. }) if code == "PERMISSION_DENIED"),
        "{refused:?}"
    );
}

/// Two accounts in one server, the second through an invite: the owner and
/// the member's device with its loop started.
async fn owner_and_member(base: &str) -> (Device, Device, String, String) {
    let (owner, member) = (Device::new(base), Device::new(base));
    let id = stamp();
    owner
        .app
        .create_account(&format!("owner{id}"), PASSWORD, None)
        .await
        .unwrap();
    let server = owner.app.create_server("Acme").await.unwrap();
    owner
        .app
        .create_channel(&server.server_id, "general", false)
        .await
        .unwrap();
    let invite = owner
        .app
        .create_invite(&server.server_id, 0, None)
        .await
        .unwrap();
    member
        .app
        .create_account(&format!("member{id}"), PASSWORD, Some(&invite.url))
        .await
        .unwrap();
    let member_id = member.app.snapshot().await.unwrap().account_id;
    let from = member.seen();
    member.start(&[]).unwrap();
    member
        .wait_for(from, Duration::from_secs(10), welcomed)
        .await
        .expect("the member is welcomed");
    (owner, member, server.server_id, member_id)
}

fn names(snapshot: &lumia_client::app::SnapshotSummary, server_id: &str) -> Vec<String> {
    snapshot
        .channels
        .iter()
        .filter(|c| c.server_id == server_id && c.kind == "TEXT")
        .map(|c| c.name.clone())
        .collect()
}

/// A subscribe's first answer for a channel: its snapshot, or for a short
/// log its events replayed from the start.
fn first_read(channel: &str) -> impl Fn(&AppEvent) -> Option<()> + '_ {
    move |event| match event {
        AppEvent::SnapshotTaken { channel_id, .. } | AppEvent::Applied { channel_id, .. } => {
            (channel_id == channel).then_some(())
        }
        _ => None,
    }
}

/// Channel membership as a member's device sees it (architecture 6.8): only
/// the default channel at first, Browse with the rest, a join with its
/// notice in the store, an add to a private channel reaching the account
/// log, and an add that waits as an invitation once the member says so.
#[tokio::test(flavor = "multi_thread")]
async fn channel_membership_reaches_the_member() {
    let Some(base) = instance() else { return };
    let (owner, member, server_id, member_id) = owner_and_member(&base).await;
    let design = owner
        .app
        .create_channel(&server_id, "design", false)
        .await
        .unwrap();
    let secret = owner
        .app
        .create_channel(&server_id, "secret", true)
        .await
        .unwrap();
    assert!(secret.private);

    let snapshot = member.app.snapshot().await.unwrap();
    assert_eq!(names(&snapshot, &server_id), ["general"]);
    let browse = member.app.browse_channels(&server_id).await.unwrap();
    let listed: Vec<_> = browse
        .iter()
        .map(|c| (c.name.as_str(), c.member_count))
        .collect();
    assert_eq!(
        listed,
        [("design", 1)],
        "a private channel is never browsed"
    );

    // Joined: in the snapshot, and its "joined" notice in the store once read.
    let after = member.app.join_channel(&design.channel_id).await.unwrap();
    assert_eq!(names(&after, &server_id), ["general", "design"]);
    let from = member.seen();
    member.app.subscribe(&design.channel_id).unwrap();
    member
        .wait_for(
            from,
            Duration::from_secs(10),
            first_read(&design.channel_id),
        )
        .await
        .expect("the channel is read");
    let notices: Vec<_> = member
        .app
        .messages(&design.channel_id, 50)
        .unwrap()
        .into_iter()
        .filter(|m| !m.notice.is_empty())
        .map(|m| (m.notice, m.author_account_id, m.notice_account_id))
        .collect();
    assert_eq!(
        notices,
        [("joined".to_string(), member_id.clone(), member_id.clone())]
    );

    // Added to the private channel: the account log brings it.
    let live = member.seen();
    let added = owner
        .app
        .add_channel_members(&secret.channel_id, std::slice::from_ref(&member_id))
        .await
        .unwrap();
    assert_eq!(added.added_account_ids, std::slice::from_ref(&member_id));
    member
        .wait_for(live, Duration::from_secs(15), account_applied)
        .await
        .expect("the member's account log says it was added");
    let snapshot = member.app.snapshot().await.unwrap();
    assert_eq!(
        names(&snapshot, &server_id),
        ["general", "design", "secret"]
    );
    let in_secret = owner.app.channel_members(&secret.channel_id).await.unwrap();
    assert_eq!(in_secret.len(), 2);

    // Left, and out of the snapshot.
    let after = member.app.leave_channel(&design.channel_id).await.unwrap();
    assert_eq!(names(&after, &server_id), ["general", "secret"]);

    // Taking adds as invitations: the next add waits for a yes.
    let switched = member
        .app
        .update_membership(&server_id, Some(true), None)
        .await
        .unwrap();
    let mine = switched
        .servers
        .iter()
        .find(|s| s.server_id == server_id)
        .unwrap();
    assert!(mine.channel_adds_by_invitation && !mine.server_dms_off);
    assert!(mine.can_start_dms && !mine.can_manage_channels);
    let invited = owner
        .app
        .add_channel_members(&design.channel_id, std::slice::from_ref(&member_id))
        .await
        .unwrap();
    assert_eq!(
        invited.invited_account_ids,
        std::slice::from_ref(&member_id)
    );
    let snapshot = member.app.snapshot().await.unwrap();
    assert_eq!(snapshot.channel_invitations.len(), 1);
    assert_eq!(snapshot.channel_invitations[0].channel.name, "design");
    let after = member
        .app
        .dismiss_channel_invitation(&design.channel_id)
        .await
        .unwrap();
    assert!(after.channel_invitations.is_empty());
    assert_eq!(names(&after, &server_id), ["general", "secret"]);
}

/// A server DM (architecture 6.4): started from one side, shown on both,
/// a message in it read by the other, and one refused when its other side
/// has turned server DMs off.
#[tokio::test(flavor = "multi_thread")]
async fn a_server_dm_reaches_both() {
    let Some(base) = instance() else { return };
    let (owner, member, server_id, member_id) = owner_and_member(&base).await;
    let owner_id = owner.app.snapshot().await.unwrap().account_id;

    let dm = member
        .app
        .open_server_dm(&server_id, &owner_id)
        .await
        .unwrap();
    assert_eq!(dm.kind, "SERVER_DM");
    let mut participants = dm.participant_account_ids.clone();
    participants.sort();
    let mut both = vec![owner_id.clone(), member_id.clone()];
    both.sort();
    assert_eq!(participants, both);
    let again = owner
        .app
        .open_server_dm(&server_id, &member_id)
        .await
        .unwrap();
    assert_eq!(again.channel_id, dm.channel_id);
    let shown = owner.app.snapshot().await.unwrap();
    assert!(shown.channels.iter().any(|c| c.channel_id == dm.channel_id));

    // A new conversation's log is empty: subscribed, there is nothing to read yet.
    member.app.subscribe(&dm.channel_id).unwrap();
    let sent = member.seen();
    member.app.send_message(&dm.channel_id, "hello").unwrap();
    member
        .wait_for(sent, Duration::from_secs(10), |e| {
            matches!(e, AppEvent::Acked { .. }).then_some(())
        })
        .await
        .expect("the message is acked");

    owner.start(std::slice::from_ref(&dm.channel_id)).unwrap();
    let read = owner.seen();
    owner
        .wait_for(read, Duration::from_secs(10), first_read(&dm.channel_id))
        .await
        .expect("the owner reads the DM");
    let bodies: Vec<_> = owner
        .app
        .messages(&dm.channel_id, 10)
        .unwrap()
        .into_iter()
        .map(|m| m.body)
        .collect();
    assert_eq!(bodies, ["hello"]);

    // Turned off: nobody new may start one.
    owner
        .app
        .update_membership(&server_id, None, Some(true))
        .await
        .unwrap();
    let third = Device::new(&base);
    let invite = owner.app.create_invite(&server_id, 0, None).await.unwrap();
    third
        .app
        .create_account(&format!("third{}", stamp()), PASSWORD, Some(&invite.url))
        .await
        .unwrap();
    let refused = third.app.open_server_dm(&server_id, &owner_id).await;
    assert!(
        matches!(&refused, Err(lumia_client::ClientError::Api { code, details, .. })
            if code == "FAILED_PRECONDITION" && details.get("state").map(String::as_str) == Some("server_dms_off")),
        "{refused:?}"
    );
}

/// Quote replies, mentions, reactions and read state as a member's device
/// sees them (architecture 6.6): the owner quotes the member and mentions
/// it, which is unread and notifies it; the member reacts and reads it.
#[tokio::test(flavor = "multi_thread")]
async fn a_quote_a_mention_a_reaction_and_a_read() {
    use lumia_client::app::{MentionSummary, QuoteSummary};

    let Some(base) = instance() else { return };
    let (owner, member, server_id, member_id) = owner_and_member(&base).await;
    let snapshot = member.app.snapshot().await.unwrap();
    let general = snapshot
        .channels
        .iter()
        .find(|c| c.server_id == server_id && c.name == "general")
        .unwrap()
        .channel_id
        .clone();
    let from = member.seen();
    member.app.subscribe(&general).unwrap();
    member
        .wait_for(from, Duration::from_secs(10), first_read(&general))
        .await
        .expect("the channel is read");
    let acked = |e: &AppEvent| matches!(e, AppEvent::Acked { .. }).then_some(());
    let sent = member.seen();
    let lunch = member.app.send_message(&general, "lunch?").unwrap();
    member
        .wait_for(sent, Duration::from_secs(10), acked)
        .await
        .expect("the member's message is acked");

    let started = owner.seen();
    owner.start(std::slice::from_ref(&general)).unwrap();
    owner
        .wait_for(started, Duration::from_secs(10), welcomed)
        .await
        .expect("the owner is welcomed");
    let handle = format!("@{}", snapshot.handle);
    let body = format!("{handle} yes");
    let mention = MentionSummary {
        account_id: member_id.clone(),
        everyone: false,
        start: 0,
        end: u32::try_from(handle.len()).unwrap(),
    };
    let quote = QuoteSummary {
        message_id: lunch.clone(),
        text: "lunch?".into(),
        ..QuoteSummary::default()
    };
    let arrived = member.seen();
    let sent = owner.seen();
    let reply_id = owner
        .app
        .send_message_with(&general, &body, &[mention], Some(&quote))
        .unwrap();
    owner
        .wait_for(sent, Duration::from_secs(10), acked)
        .await
        .expect("the owner's reply is acked");
    let reply = member
        .wait_for(arrived, Duration::from_secs(10), |_| {
            member
                .app
                .messages(&general, 50)
                .unwrap()
                .into_iter()
                .find(|m| m.message_id == reply_id)
        })
        .await
        .expect("the reply reaches the member");
    let quoted = reply.reply.clone().unwrap();
    assert_eq!(
        (
            quoted.message_id.as_str(),
            quoted.author_account_id.as_str()
        ),
        (lunch.as_str(), member_id.as_str())
    );
    assert_eq!(quoted.text, "lunch?");
    assert!(reply.mentions_me);
    assert_eq!(reply.mentions[0].account_id, member_id);

    let state = member
        .app
        .read_states()
        .into_iter()
        .find(|r| r.channel_id == general)
        .unwrap();
    assert_eq!((state.unread, state.mentions), (1, 1));
    assert_eq!(member.app.badge(), 1);
    let (mentions, more) = member.app.mentions(Some(&server_id), None).await.unwrap();
    assert_eq!(mentions.len(), 1);
    assert!(!more);
    assert_eq!(mentions[0].message_id, reply_id);

    // A reaction: the instance refuses anything but one emoji.
    let reacted = member.seen();
    member.app.react(&general, &reply_id, "👍", false).unwrap();
    member
        .wait_for(reacted, Duration::from_secs(10), |_| {
            member
                .app
                .messages(&general, 50)
                .unwrap()
                .into_iter()
                .find(|m| m.message_id == reply_id && !m.reactions.is_empty())
        })
        .await
        .map(|m| {
            assert_eq!(m.reactions[0].reaction, "👍");
            assert!(m.reactions[0].me && m.reactions[0].count == 1);
        })
        .expect("the reaction is in the store");
    let refused = member.seen();
    member.app.react(&general, &reply_id, "ok", false).unwrap();
    member
        .wait_for(refused, Duration::from_secs(10), |e| {
            matches!(e, AppEvent::Error { .. }).then_some(())
        })
        .await
        .expect("a word is refused as a reaction");

    // Read: nothing is unread, and the owner's snapshot agrees for the member.
    let read = member.app.mark_read(&general, reply.seq).await.unwrap();
    assert_eq!((read.unread, read.mentions), (0, 0));
    assert_eq!(member.app.badge(), 0);
    let again = member.app.snapshot().await.unwrap();
    let state = again
        .read_states
        .iter()
        .find(|r| r.channel_id == general)
        .unwrap();
    assert!(state.read_seq >= reply.seq && state.unread == 0);
}

/// Editing and deleting (architecture 6.6): a member edits their own
/// message, the owner may not edit it but, holding `MANAGE_MESSAGES`, may
/// delete it, and a member's device follows both.
#[tokio::test(flavor = "multi_thread")]
async fn an_author_edits_and_a_moderator_deletes() {
    let Some(base) = instance() else { return };
    let (owner, member, server_id, _member_id) = owner_and_member(&base).await;
    let general = member
        .app
        .snapshot()
        .await
        .unwrap()
        .channels
        .iter()
        .find(|c| c.server_id == server_id && c.name == "general")
        .unwrap()
        .channel_id
        .clone();
    let from = member.seen();
    member.app.subscribe(&general).unwrap();
    member
        .wait_for(from, Duration::from_secs(10), first_read(&general))
        .await
        .expect("the channel is read");
    let acked = |e: &AppEvent| matches!(e, AppEvent::Acked { .. }).then_some(());
    let refused = |e: &AppEvent| matches!(e, AppEvent::Error { .. }).then_some(());
    let sent = member.seen();
    let id = member
        .app
        .send_message(&general, "Shipped Thursday.")
        .unwrap();
    member
        .wait_for(sent, Duration::from_secs(10), acked)
        .await
        .expect("sent");

    let started = owner.seen();
    owner.start(std::slice::from_ref(&general)).unwrap();
    owner
        .wait_for(started, Duration::from_secs(10), welcomed)
        .await
        .expect("the owner is welcomed");
    let tried = owner.seen();
    owner
        .app
        .edit_message(&general, &id, "Not yours.", &[])
        .unwrap();
    owner
        .wait_for(tried, Duration::from_secs(10), refused)
        .await
        .expect("an edit of another's message is refused");

    let edited = member.seen();
    member
        .app
        .edit_message(&general, &id, "Shipped Friday.", &[])
        .unwrap();
    let shown = member
        .wait_for(edited, Duration::from_secs(10), |_| {
            member
                .app
                .messages(&general, 50)
                .unwrap()
                .into_iter()
                .find(|m| m.message_id == id && m.edited)
        })
        .await
        .expect("the edit reaches the store");
    assert_eq!(shown.body, "Shipped Friday.");

    let gone = member.seen();
    owner.app.delete_message(&general, &id).unwrap();
    member
        .wait_for(gone, Duration::from_secs(10), |_| {
            member
                .app
                .messages(&general, 50)
                .unwrap()
                .into_iter()
                .find(|m| m.message_id == id && m.deleted)
        })
        .await
        .expect("the moderator's delete reaches the member");
}
