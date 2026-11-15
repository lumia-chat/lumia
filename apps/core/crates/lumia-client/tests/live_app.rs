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

/// A number no other call in this run shares: the clock here counts in microseconds, so two
/// tests starting together read the same "nanoseconds" and made the same handle.
fn stamp() -> u128 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros()
        * 1000
        + u128::from(n % 1000)
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
    let refused = |e: &AppEvent| match e {
        AppEvent::Error { command_id, .. } => Some(command_id.clone()),
        _ => None,
    };
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
    let command = owner
        .app
        .edit_message(&general, &id, "Not yours.", &[])
        .unwrap();
    let refusal = owner
        .wait_for(tried, Duration::from_secs(10), refused)
        .await
        .expect("an edit of another's message is refused");
    assert_eq!(refusal, command, "the refusal names the edit");

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
    // A server shows no deleted messages unless it says so (architecture 6.7).
    member
        .wait_for(gone, Duration::from_secs(10), |_| {
            let listed = member.app.messages(&general, 50).unwrap();
            (!listed.is_empty() && !listed.iter().any(|m| m.message_id == id)).then_some(())
        })
        .await
        .expect("the moderator's delete takes the message away for the member");

    // Said so, the member reads it in its place as deleted, once its snapshot says it too.
    owner.app.set_show_deleted(&server_id, true).await.unwrap();
    let shown = member
        .app
        .snapshot()
        .await
        .unwrap()
        .servers
        .into_iter()
        .find(|s| s.server_id == server_id)
        .unwrap();
    assert!(shown.show_deleted);
    assert!(
        member
            .app
            .messages(&general, 50)
            .unwrap()
            .iter()
            .any(|m| m.message_id == id && m.deleted),
        "the deleted message keeps its place where the server shows them"
    );
}

/// The rail as the account arranges it (architecture 6.9): kept by the instance, tidied, in
/// the snapshot with its servers in order, and told through the account log.
#[tokio::test(flavor = "multi_thread")]
async fn the_rail_is_arranged_kept_and_told() {
    use lumia_client::rail::{self, RailItem};

    let Some(base) = instance() else { return };
    let device = Device::new(&base);
    device
        .app
        .create_account(&format!("rail{}", stamp()), PASSWORD, None)
        .await
        .unwrap();
    let a = device.app.create_server("A").await.unwrap().server_id;
    let b = device.app.create_server("B").await.unwrap().server_id;
    let c = device.app.create_server("C").await.unwrap().server_id;

    // Nothing arranged: the order they were joined in.
    let first = device.app.snapshot().await.unwrap();
    assert_eq!(
        rail::server_order(&first.rail),
        vec![a.clone(), b.clone(), c.clone()]
    );

    let started = device.seen();
    device.start(&[]).unwrap();
    device
        .wait_for(started, Duration::from_secs(10), welcomed)
        .await
        .expect("welcomed");

    // C onto A makes a folder; B goes first.
    let folder = lumia_core::wire::display(&lumia_core::wire::new_id());
    let merged = rail::merge(&first.rail, &c, &a, &folder);
    let arranged = rail::move_to(&merged, &b, None, Some(&folder));
    let named = rail::set_folder(&arranged, &folder, "Work", "sky");
    let told = device.seen();
    let kept = device.app.arrange_rail(&named).await.unwrap();
    assert_eq!(
        kept.rail,
        vec![
            RailItem::Server {
                server_id: b.clone()
            },
            RailItem::Folder {
                folder_id: folder.clone(),
                name: "Work".into(),
                tone: "sky".into(),
                server_ids: vec![a.clone(), c.clone()],
            },
        ]
    );
    let order: Vec<String> = kept.servers.iter().map(|s| s.server_id.clone()).collect();
    assert_eq!(
        order,
        vec![b, a.clone(), c],
        "the snapshot's servers follow the rail"
    );
    device
        .wait_for(told, Duration::from_secs(10), account_applied)
        .await
        .expect("the arrangement is told through the account log");

    // What the instance refuses comes back as its field.
    let bad = rail::set_folder(&kept.rail, &folder, "Work", "sky")
        .into_iter()
        .map(|item| match item {
            RailItem::Folder {
                folder_id,
                name,
                server_ids,
                ..
            } => RailItem::Folder {
                folder_id,
                name,
                tone: "red".into(),
                server_ids,
            },
            other => other,
        })
        .collect::<Vec<_>>();
    assert!(device.app.arrange_rail(&bad).await.is_err());

    // Two made quickly, with a snapshot read meanwhile: they are kept in the order they were
    // made, and the snapshot shows the later one while it is on its way.
    let up = rail::step(&kept.rail, &folder, -1);
    let swapped = rail::step(&up, &a, 1);
    let (one, two, between) = tokio::join!(
        device.app.arrange_rail(&up),
        device.app.arrange_rail(&swapped),
        device.app.snapshot(),
    );
    one.unwrap();
    assert_eq!(two.unwrap().rail, swapped);
    assert_eq!(between.unwrap().rail, swapped);
    assert_eq!(
        device.app.snapshot().await.unwrap().rail,
        swapped,
        "the instance keeps the last one made"
    );
}

// ---- Setting up private messages (identity spec 4.1, 4.3, 4.7; issue #20) ----

fn web_device(base: &str) -> Device {
    let mut device = Device::new(base);
    let at = device.dir.clone();
    let opener: Opener = Arc::new(move |name| {
        SqliteStore::open(&at.join(match name {
            StoreName::Instance => "web-store.sqlite",
            StoreName::Keys => "web-keys.sqlite",
            StoreName::IdentityStore => "web-identity.sqlite",
            StoreName::IdentityKeys => "web-identity-keys.sqlite",
        }))
    });
    // The instance's own origin: a WEB device's proofs are taken there.
    let origin = base.trim_end_matches('/').to_string();
    device.app = App::open(base, Place::Web { origin }, opener).unwrap();
    device
}

impl Device {
    /// The same device with its app opened again over the same stores.
    fn reopen(&mut self, base: &str) {
        self.app.stop();
        let at = self.dir.clone();
        let opener: Opener = Arc::new(move |name| {
            SqliteStore::open(&at.join(match name {
                StoreName::Instance => "store.sqlite",
                StoreName::Keys => "keys.sqlite",
                StoreName::IdentityStore => "identity.sqlite",
                StoreName::IdentityKeys => "identity-keys.sqlite",
            }))
        });
        self.app = App::open(base, Place::Native, opener).unwrap();
    }
}

/// The words at the quiz's positions.
fn answers(phrase: &str, quiz: &[u32]) -> Vec<String> {
    let words: Vec<&str> = phrase.split(' ').collect();
    quiz.iter()
        .map(|p| words[*p as usize - 1].to_string())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_native_sign_up_sets_up_silently_and_the_phrase_is_confirmed_later() {
    let Some(base) = instance() else { return };
    let mut phone = Device::new(&base);
    let handle = format!("setup{}", stamp());
    phone
        .app
        .create_account(&handle, PASSWORD, None)
        .await
        .unwrap();
    // The sign-up proved a password: its answer carried a reauth token.
    assert!(phone.app.has_reauth());
    assert_eq!(phone.app.account_identity().await.unwrap(), None);

    let identity = phone.app.identity_setup("Phone").await.unwrap();
    assert_eq!(
        phone.app.account_identity().await.unwrap(),
        Some(identity.clone())
    );
    assert!(!phone.app.has_reauth(), "binding spent the token");

    // Kept until confirmed, across the app opening again, and it is the
    // phrase of this identity's root.
    assert!(phone.app.identity_phrase_unconfirmed().await.unwrap());
    let phrase = phone.app.identity_phrase().await.unwrap().unwrap();
    let derived = lumia_core::identity::phrase::Phrase::parse(&phrase)
        .unwrap()
        .derive();
    assert_eq!(hex::encode(derived.identity_id()), identity);
    phone.reopen(&base);
    assert_eq!(
        phone.app.identity_phrase().await.unwrap(),
        Some(phrase.clone())
    );

    // A wrong word asks other positions.
    let quiz = phone.app.identity_phrase_quiz().await.unwrap();
    assert_eq!(quiz.len(), 3);
    assert_eq!(
        phone.app.identity_phrase_quiz().await.unwrap(),
        quiz,
        "the same until answered"
    );
    let mut wrong = answers(&phrase, &quiz);
    wrong[2] = "zoo".into();
    assert!(!phone.app.identity_phrase_confirm(wrong).await.unwrap());
    let quiz = phone.app.identity_phrase_quiz().await.unwrap();
    assert!(phone
        .app
        .identity_phrase_confirm(answers(&phrase, &quiz))
        .await
        .unwrap());
    assert!(!phone.app.identity_phrase_unconfirmed().await.unwrap());
    assert_eq!(phone.app.identity_phrase().await.unwrap(), None);

    // Setting up again is a no-op for the device that holds it.
    assert_eq!(phone.app.identity_setup("Phone").await.unwrap(), identity);

    // Without a reauth token in hand, setting up refuses before it makes
    // anything: the device stays without an identity.
    let other = Device::new(&base);
    other
        .app
        .create_account(&format!("{handle}b"), PASSWORD, None)
        .await
        .unwrap();
    other.app.reauth_take("");
    assert!(matches!(
        other.app.identity_setup("Other").await,
        Err(lumia_client::ClientError::ReauthNeeded)
    ));
    assert!(!other.app.has_identity());
    other.app.reauth_password(PASSWORD).await.unwrap();
    other.app.identity_setup("Other").await.unwrap();
    assert!(other.app.has_identity());

    // Another device of the same account is sent to link or restore,
    // never given a second identity.
    let laptop = Device::new(&base);
    laptop.app.sign_in(&handle, PASSWORD, None).await.unwrap();
    assert!(matches!(
        laptop.app.identity_setup("Laptop").await,
        Err(lumia_client::ClientError::AccountHasIdentity)
    ));
    assert!(!laptop.app.has_identity());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_browser_confirms_the_phrase_before_it_sets_up_and_keeps_none() {
    let Some(base) = instance() else { return };
    let browser = web_device(&base);
    let handle = format!("websetup{}", stamp());
    browser
        .app
        .create_account(&handle, PASSWORD, None)
        .await
        .unwrap();
    assert!(browser.app.identity_setup("Browser").await.is_err());

    let phrase = browser.app.identity_new_phrase().unwrap();
    assert!(
        browser.app.identity_setup("Browser").await.is_err(),
        "an unconfirmed phrase sets nothing up"
    );
    let quiz = browser.app.identity_phrase_quiz().await.unwrap();
    assert!(browser
        .app
        .identity_phrase_confirm(answers(&phrase, &quiz))
        .await
        .unwrap());
    let identity = browser.app.identity_setup("Browser").await.unwrap();
    assert_eq!(
        browser.app.account_identity().await.unwrap(),
        Some(identity)
    );
    assert!(!browser.app.identity_phrase_unconfirmed().await.unwrap());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_phrase_restores_on_a_new_device_and_starting_over_moves_the_account() {
    let Some(base) = instance() else { return };
    let phone = Device::new(&base);
    let handle = format!("restore{}", stamp());
    phone
        .app
        .create_account(&handle, PASSWORD, None)
        .await
        .unwrap();
    let identity = phone.app.identity_setup("Phone").await.unwrap();
    let phrase = phone.app.identity_phrase().await.unwrap().unwrap();

    // Every device gone but the phrase: a new one restores it, alone.
    let replacement = Device::new(&base);
    let roster = replacement
        .app
        .identity_recover(&phrase, "Replacement")
        .await
        .unwrap();
    assert_eq!(roster.identity_id, identity);
    assert!(replacement.app.signed_in());
    assert_eq!(
        replacement.app.account_identity().await.unwrap(),
        Some(identity.clone())
    );

    // Neither device nor phrase: start over, under the sign-in's reauth.
    let fresh = Device::new(&base);
    fresh.app.sign_in(&handle, PASSWORD, None).await.unwrap();
    let new = fresh.app.identity_start_over("Fresh").await.unwrap();
    assert_ne!(new, identity);
    assert_eq!(fresh.app.account_identity().await.unwrap(), Some(new));
    assert!(matches!(
        fresh.app.identity_start_over("Fresh").await,
        Err(lumia_client::ClientError::ReauthNeeded)
    ));
    fresh.app.reauth_password(PASSWORD).await.unwrap();
    assert!(fresh.app.has_reauth());
}

/// A device signed up, set up and running, and its account id.
async fn set_up(base: &str, name: &str) -> (Device, String, String) {
    let device = Device::new(base);
    let handle = format!("{name}{}", stamp());
    device
        .app
        .create_account(&handle, PASSWORD, None)
        .await
        .unwrap();
    device.app.identity_setup(name).await.unwrap();
    let from = device.seen();
    device.start(&[]).unwrap();
    let account = device
        .wait_for(from, Duration::from_secs(10), welcomed)
        .await
        .expect("welcomed");
    (device, handle, account)
}

/// Whether the device holds the conversation's group, within `within`.
async fn member_of(device: &Device, channel: &str, within: Duration) -> bool {
    let until = Instant::now() + within;
    while Instant::now() < until {
        let channels = device.app.dm_channels().await.unwrap_or_default();
        if channels.iter().any(|c| c.channel_id == channel && c.member) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

/// Whether the device has read `body` in the conversation, within `within`.
async fn reads(device: &Device, channel: &str, body: &str, within: Duration) -> bool {
    let until = Instant::now() + within;
    while Instant::now() < until {
        if let Ok(log) = device.app.dm_messages(channel).await {
            if log.messages.iter().any(|m| m.body == body) {
                return true;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

/// A contact starts over (identity spec 4.7; e2ee spec 4): the other side
/// is told, and the conversation goes on with the new identity's device.
#[tokio::test(flavor = "multi_thread")]
async fn a_contact_who_starts_over_is_told_and_the_conversation_goes_on() {
    let Some(base) = instance() else { return };
    let (ada, _, _) = set_up(&base, "ada").await;
    let (bo, bo_handle, bo_account) = set_up(&base, "bo").await;

    let request = ada.app.dm_request(&bo_account).await.unwrap();
    let Ok(opened) = bo
        .app
        .dm_respond(&request.request_id, "accept")
        .await
        .unwrap()
    else {
        panic!("accepting opens the conversation");
    };
    let channel = opened.channel_id;
    assert!(
        member_of(&ada, &channel, Duration::from_secs(20)).await,
        "ada is welcomed"
    );
    ada.app.dm_send(&channel, "before").await.unwrap();
    assert!(reads(&bo, &channel, "before", Duration::from_secs(20)).await);

    // Bo loses the device and the phrase, and starts over on a new one.
    let noted = ada.seen();
    let fresh = Device::new(&base);
    fresh.app.sign_in(&bo_handle, PASSWORD, None).await.unwrap();
    fresh.app.identity_start_over("Fresh").await.unwrap();
    let from = fresh.seen();
    fresh.start(&[]).unwrap();
    fresh
        .wait_for(from, Duration::from_secs(10), welcomed)
        .await
        .expect("the fresh device is welcomed");

    let told = ada
        .wait_for(noted, Duration::from_secs(20), |e| match e {
            AppEvent::DmNote { message } if message.contains("started over") => Some(()),
            _ => None,
        })
        .await;
    assert!(told.is_some(), "ada is told bo started over");

    // Ada removes the old identity's leaf; the new device rejoins from the
    // group info, and once Ada has seen it join, what she sends reaches it.
    let removed = ada
        .wait_for(noted, Duration::from_secs(20), |e| match e {
            AppEvent::DmNote { message } if message.starts_with("commit acked") => Some(()),
            _ => None,
        })
        .await;
    assert!(
        removed.is_some(),
        "ada's commit removing the old leaf is acked"
    );
    let joining = ada.seen();
    let until = Instant::now() + Duration::from_secs(10);
    // The new device learns of the conversation from its account log first.
    while let Err(e) = fresh.app.dm_rejoin(&channel).await {
        assert!(Instant::now() < until, "rejoin: {e}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(
        member_of(&fresh, &channel, Duration::from_secs(20)).await,
        "the new device is in the conversation"
    );
    let seen = ada
        .wait_for(joining, Duration::from_secs(20), |e| match e {
            AppEvent::DmNote { message } if message.contains("device recovered") => Some(()),
            _ => None,
        })
        .await;
    assert!(seen.is_some(), "ada sees the new device join");
    ada.app.dm_send(&channel, "after").await.unwrap();
    let read = reads(&fresh, &channel, "after", Duration::from_secs(20)).await;
    assert!(read, "the new device reads what is sent after");
    assert!(
        !reads(&fresh, &channel, "before", Duration::from_millis(200)).await,
        "and not what was sent before"
    );
}

/// The device that accepts a request creates the group and holds it,
/// alone, until the instance acks the commit adding the other: a send
/// before then is refused ("a commit is pending"). The conversation was
/// said to be a member's, so a shell offered its composer, from the moment
/// the group was created; it is a member's once the commit is acked.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_conversation_is_offered_for_sending_once_its_group_is_acked() {
    let Some(base) = instance() else { return };
    let (ada, _, _) = set_up(&base, "ada").await;
    let (bo, _, bo_account) = set_up(&base, "bo").await;
    let request = ada.app.dm_request(&bo_account).await.unwrap();
    let Ok(opened) = bo
        .app
        .dm_respond(&request.request_id, "accept")
        .await
        .unwrap()
    else {
        panic!("accepting opens the conversation");
    };
    let channel = opened.channel_id;
    assert!(
        member_of(&bo, &channel, Duration::from_secs(20)).await,
        "bo's group is ready"
    );
    bo.app.dm_send(&channel, "at once").await.unwrap();
    assert!(reads(&ada, &channel, "at once", Duration::from_secs(20)).await);
}

/// A personal message the store holds and the DM engine never read: a run
/// that ended between the two (the CLI stops once its group is ready, while
/// its gateway may have stored the next event) left it there, and the next
/// connection resumed from the store's cursor, so no event ever came to
/// make the engine catch up. The welcome does now.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_stored_but_never_read_is_read_on_the_next_connection() {
    use lumia_core::store::{LogKind, Store as _};

    let Some(base) = instance() else { return };
    let (ada, _, _) = set_up(&base, "ada").await;
    let (bo, _, bo_account) = set_up(&base, "bo").await;
    let request = ada.app.dm_request(&bo_account).await.unwrap();
    let Ok(opened) = bo
        .app
        .dm_respond(&request.request_id, "accept")
        .await
        .unwrap()
    else {
        panic!("accepting opens the conversation");
    };
    let channel = opened.channel_id;
    assert!(
        member_of(&ada, &channel, Duration::from_secs(20)).await,
        "ada is welcomed"
    );
    ada.app.dm_send(&channel, "first").await.unwrap();
    assert!(reads(&bo, &channel, "first", Duration::from_secs(20)).await);

    // Bo goes away. What Ada sends next reaches Bo's store as Bo's gateway
    // would have stored it, and Bo's engine never hears of it.
    stopped(&bo).await;
    ada.app.dm_send(&channel, "second").await.unwrap();
    let id = lumia_core::wire::uuid_bytes(uuid::Uuid::parse_str(&channel).unwrap());
    let mut bo_store = SqliteStore::open(&bo.dir.join("store.sqlite")).unwrap();
    let cursor = bo_store
        .cursors(LogKind::Channel)
        .unwrap()
        .into_iter()
        .find_map(|(log, seq)| (log == id).then_some(seq))
        .expect("bo's store follows the conversation");
    let ada_store = SqliteStore::open(&ada.dir.join("store.sqlite")).unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    let missed = loop {
        let missed: Vec<_> = ada_store
            .stored_events(LogKind::Channel, &id)
            .unwrap()
            .into_iter()
            .filter(|e| e.seq > cursor)
            .collect();
        if !missed.is_empty() {
            break missed;
        }
        assert!(Instant::now() < until, "ada's store never held her message");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    bo_store.apply(LogKind::Channel, &id, &missed).unwrap();
    assert!(
        !reads(&bo, &channel, "second", Duration::from_millis(200)).await,
        "bo has not read it before connecting"
    );

    let from = bo.seen();
    start_when_free(&bo, &[]).await;
    bo.wait_for(from, Duration::from_secs(10), welcomed)
        .await
        .expect("bo is welcomed again");
    assert!(
        reads(&bo, &channel, "second", Duration::from_secs(20)).await,
        "bo reads what the store held once connected"
    );
}

/// Linking by prompt (identity spec 4.2.1): the new device opens a session,
/// the device that holds the identity sees the prompt and answers, both
/// show the same six digits, the old device confirms, and the new device
/// finishes on its own, holding the identity. A second session is declined
/// by "Not me".
#[tokio::test(flavor = "multi_thread")]
async fn a_device_is_linked_by_a_prompt_and_matching_digits() {
    let Some(base) = instance() else { return };
    let laptop = Device::new(&base);
    let handle = format!("prompt{}", stamp());
    laptop
        .app
        .create_account(&handle, PASSWORD, None)
        .await
        .unwrap();
    let identity = laptop.app.identity_setup("Laptop").await.unwrap();

    let phone = Device::new(&base);
    phone.app.sign_in(&handle, PASSWORD, None).await.unwrap();
    assert_eq!(
        phone.app.link_start("Pixel 9").await.unwrap().stage,
        "waiting"
    );
    assert_eq!(phone.app.link_progress().await.unwrap().stage, "waiting");

    let prompts = laptop.app.link_prompts().await.unwrap();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].stage, "new");
    assert_eq!(prompts[0].request.label, "Pixel 9");
    let id = prompts[0].session_id.clone();
    // The new device, holding no identity, prompts nothing.
    assert!(phone.app.link_prompts().await.unwrap().is_empty());

    laptop.app.link_prompt_answer(&id).await.unwrap();
    assert_eq!(laptop.app.link_prompts().await.unwrap()[0].stage, "waiting");
    let on_phone = phone.app.link_progress().await.unwrap();
    assert_eq!(on_phone.stage, "compare");
    let on_laptop = laptop.app.link_prompts().await.unwrap();
    assert_eq!(on_laptop[0].stage, "compare");
    assert_eq!(
        on_laptop[0].digits, on_phone.digits,
        "both screens show the same digits"
    );
    assert_eq!(on_phone.digits.len(), 7);

    laptop.app.link_prompt_confirm(&id).await.unwrap();
    assert!(laptop.app.link_prompts().await.unwrap().is_empty());
    assert_eq!(phone.app.link_progress().await.unwrap().stage, "linked");
    assert!(phone.app.has_identity());
    assert_eq!(
        phone.app.identity_show().await.unwrap().identity_id,
        identity
    );
    assert!(
        phone.app.link_progress().await.is_err(),
        "nothing in progress after"
    );

    // Another device asks, and "Not me" ends it.
    let other = Device::new(&base);
    other.app.sign_in(&handle, PASSWORD, None).await.unwrap();
    other.app.link_start("Unknown").await.unwrap();
    let id = laptop.app.link_prompts().await.unwrap()[0]
        .session_id
        .clone();
    laptop.app.link_prompt_decline(&id).await.unwrap();
    assert_eq!(other.app.link_progress().await.unwrap().stage, "declined");
    assert!(laptop.app.link_prompts().await.unwrap().is_empty());
}
