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
        .create_channel(&server.server_id, "general")
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
