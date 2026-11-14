//! The Linux AppImage's updater (architecture section 13, "How the desktop shell updates
//! itself"): the same feed format and key as the Mac's Sparkle, read by Lumia's own code
//! because nothing like Sparkle exists here. The window's side (what the person sees and
//! asks for) is `app::updates`; this is where it meets the network and the disk.
//!
//! - **Who has one.** A process the AppImage runtime started from a Lumia AppImage
//!   (`APPIMAGE` names the file, and this executable is inside the runtime's `APPDIR`), in
//!   a build that names a feed and a key (`LUMIA_UPDATE_FEED`, `LUMIA_UPDATE_KEY`, compiled
//!   in by `scripts/dist-linux.sh`). A deb or rpm has neither the variables nor the right
//!   to replace itself: the package manager updates it, from the repository the release
//!   publishes. A `cargo build` names no feed and has no updater.
//! - **Checking.** The feed is fetched and must carry its own signature with the key
//!   (`lumia_appcast::read`); only a release above this one's version is offered, so an old
//!   feed served again offers nothing.
//! - **Installing.** The download is held in memory until its length and signature check
//!   against the feed, then written beside the AppImage and renamed over it, so the file is
//!   either the old release or the new one, never half of either. The running copy keeps
//!   the old one open and goes on; the new one starts at the next launch, or now with
//!   "Restart", which starts it and quits ([`wait_for_previous`] holds the new process back
//!   until this one has let go of the data directory).
//! - **Where it cannot.** An AppImage in a directory this user cannot write to (put in
//!   `/opt` by root, say) cannot be replaced; the person is told to download the release.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use lumia_appcast::{PublicKey, Release};
use rune::{AppHandle, Tx};

use crate::app::State;
use crate::app::updates::{Ask, Report, Updater};

/// The appcast this build reads, and the public key its feed and downloads are signed with.
const FEED: Option<&str> = option_env!("LUMIA_UPDATE_FEED");
const KEY: Option<&str> = option_env!("LUMIA_UPDATE_KEY");
/// This build's version, which a release must be above.
const CURRENT: &str = env!("CARGO_PKG_VERSION");
/// The variable a restarted AppImage is given: the pid it waits to see exit.
const AFTER: &str = "LUMIA_RELAUNCH_AFTER";
/// How long a restarted AppImage waits for the copy that started it.
const WAIT: Duration = Duration::from_secs(10);
/// How often the daily check looks at whether it is due.
const LOOK: Duration = Duration::from_secs(60 * 60);
/// How long after launch the first look comes, so a check never competes with the launch.
const SETTLE: Duration = Duration::from_secs(30);
/// The largest download taken. A release is about 30 MB.
const MAX_DOWNLOAD: u64 = 512 << 20;

/// What this launch updates from, once it is known to have an updater.
struct Source {
    feed: String,
    key: PublicKey,
    /// The AppImage file, symbolic links resolved: what is replaced.
    image: PathBuf,
}

static SOURCE: OnceLock<Source> = OnceLock::new();
/// The release the last check found, which "Update" downloads.
static FOUND: Mutex<Option<Release>> = Mutex::new(None);

/// Starts the updater when this launch has one: the window gets its Updates card, and a
/// thread looks once an hour at whether the daily check is due.
pub fn start(handle: &AppHandle<State>) {
    let Some(source) = source() else {
        super::trace(|| "appimage updater absent".to_string());
        return;
    };
    super::trace(|| format!("appimage updater for {}", source.image.display()));
    if SOURCE.set(source).is_err() {
        return;
    }
    handle.update(|state, _| state.updater = Some(Updater::default()));
    let daily = handle.clone();
    let spawned = std::thread::Builder::new()
        .name("lumia-update-schedule".into())
        .spawn(move || {
            std::thread::sleep(SETTLE);
            loop {
                daily.update_if(|state, tx| {
                    let before = state.updater.clone();
                    state.check_if_due(now());
                    pay(state, tx);
                    state.updater != before
                });
                std::thread::sleep(LOOK);
            }
        });
    if let Err(error) = spawned {
        eprintln!("lumia: no daily update check: {error}");
    }
}

/// The feed, the key and the AppImage, when all three are there.
fn source() -> Option<Source> {
    let feed = FEED
        .filter(|feed| feed.starts_with("https://"))?
        .to_string();
    let key = PublicKey::from_base64(KEY?).ok()?;
    let image = std::fs::canonicalize(std::env::var_os("APPIMAGE")?).ok()?;
    // APPIMAGE alone could be inherited from another AppImage that started this process
    // (a terminal, say); this executable being inside the runtime's mount is what says the
    // file is this one.
    let appdir = std::fs::canonicalize(std::env::var_os("APPDIR")?).ok()?;
    let exe = std::env::current_exe().ok()?;
    (exe.starts_with(&appdir) && image.is_file()).then_some(Source { feed, key, image })
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Carries out what the window asked for (`app::updates::Ask`), from the platform's poll.
pub fn pay(state: &mut State, tx: &mut Tx) {
    let Some(ask) = state.updater.as_mut().and_then(|u| u.owed.take()) else {
        return;
    };
    let Some(source) = SOURCE.get() else { return };
    match ask {
        Ask::Check => work("lumia-update-check", move || check(source)),
        Ask::Install => work("lumia-update-install", move || install(source)),
        Ask::Restart => match restart(&source.image) {
            Ok(()) => tx.quit(),
            Err(why) => state.react_update(Report::Failed(why)),
        },
    }
}

/// Runs `job` on a thread of its own and hands what it finds to the window.
fn work(name: &str, job: impl FnOnce() -> Report + Send + 'static) {
    let Some(handle) = super::HANDLE.get().cloned() else {
        return;
    };
    let spawned = std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let report = job();
            super::trace(|| format!("update {report:?}"));
            handle.update(move |state, _| state.react_update(report));
        });
    if let Err(error) = spawned {
        eprintln!("lumia: {name}: {error}");
    }
}

fn check(source: &Source) -> Report {
    let releases = match fetch(&source.feed, lumia_appcast::MAX_FEED as u64)
        .and_then(|feed| lumia_appcast::read(&feed, source.key).map_err(|e| e.to_string()))
    {
        Ok(releases) => releases,
        Err(why) => return Report::Failed(why),
    };
    let newest = lumia_appcast::newest_above(&releases, CURRENT).cloned();
    let version = newest.as_ref().map(|release| release.display.clone());
    *FOUND.lock().unwrap_or_else(PoisonError::into_inner) = newest;
    Report::Checked(version)
}

fn install(source: &Source) -> Report {
    let Some(release) = FOUND.lock().unwrap_or_else(PoisonError::into_inner).clone() else {
        return Report::Failed("there is no release to download; check again".into());
    };
    if release.length > MAX_DOWNLOAD {
        return Report::Failed(format!("the release is {} bytes", release.length));
    }
    let download = match fetch(&release.url, release.length) {
        Ok(download) => download,
        Err(why) => return Report::Failed(why),
    };
    if let Err(why) = release.check(&download, source.key) {
        return Report::Failed(why.to_string());
    }
    match replace(&source.image, &download) {
        Ok(()) => Report::Installed(release.display),
        Err(why) => Report::Failed(why),
    }
}

/// `url`'s body, refused past `limit` bytes.
fn fetch(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(15 * 60))
        .user_agent(format!("Lumia/{CURRENT}"))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|e| format!("could not reach the release: {e}"))?;
    if response
        .content_length()
        .is_some_and(|length| length > limit)
    {
        return Err("the release's server sent more than was asked for".into());
    }
    let mut body = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut body)
        .map_err(|e| format!("the download stopped: {e}"))?;
    if body.len() as u64 > limit {
        return Err("the release's server sent more than was asked for".into());
    }
    Ok(body)
}

/// Puts `bytes` in place of the AppImage at `image`: written beside it, made executable,
/// flushed, then renamed over it in one step.
fn replace(image: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let dir = image.parent().ok_or("the AppImage has no directory")?;
    let name = image.file_name().ok_or("the AppImage has no name")?;
    let part = dir.join(format!(".{}.update", name.to_string_lossy()));
    let cannot = |error: std::io::Error| {
        format!(
            "Lumia cannot write to {} ({error}); download the new release from GitHub instead",
            dir.display()
        )
    };
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o755)
        .open(&part)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        });
    if let Err(error) = written {
        let _ = std::fs::remove_file(&part);
        return Err(cannot(error));
    }
    std::fs::rename(&part, image).map_err(|error| {
        let _ = std::fs::remove_file(&part);
        cannot(error)
    })
}

/// Starts the AppImage now in place, which waits for this process to exit.
fn restart(image: &Path) -> Result<(), String> {
    std::process::Command::new(image)
        .env(AFTER, std::process::id().to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("could not start the new release: {e}"))
}

/// A launch that "Restart" started waits here, before it claims the data directory, for
/// the copy that started it to exit, for at most ten seconds. Any other launch goes on.
pub fn wait_for_previous() {
    let Some(pid) = std::env::var(AFTER)
        .ok()
        .and_then(|pid| pid.parse::<u32>().ok())
        .filter(|pid| *pid != std::process::id())
    else {
        return;
    };
    let alive = Path::new("/proc").join(pid.to_string());
    let started = std::time::Instant::now();
    while alive.exists() && started.elapsed() < WAIT {
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    //! The network and the disk with no window: a feed and a download served from a
    //! socket on this machine, and a file standing in for the AppImage.

    use std::io::Write;
    use std::net::TcpListener;

    use lumia_appcast::PrivateKey;

    use super::*;

    fn key() -> PrivateKey {
        PrivateKey::from_base64("NyS8HjwxfipXZ5AQLnqcLvSFNZGXrmIkCWLAWfrrx+w=").unwrap()
    }

    /// Serves `body` over plain HTTP to each request, on a port of its own.
    fn serve(body: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut request = [0u8; 4096];
                let _ = std::io::Read::read(&mut stream, &mut request);
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        format!("http://{address}/")
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lumia-appimage-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn source(feed: String, image: PathBuf) -> Source {
        Source {
            feed,
            key: key().public(),
            image,
        }
    }

    fn feed_for(version: &str, download: &[u8]) -> Vec<u8> {
        let key = key();
        let feed = lumia_appcast::write(
            "Lumia",
            version,
            "https://example.invalid/Lumia.AppImage",
            download.len() as u64,
            &key.sign(download),
        );
        key.sign_feed(&feed).into_bytes()
    }

    #[test]
    fn a_signed_feed_with_a_newer_release_is_offered_and_an_old_one_is_not() {
        let dir = scratch("check");
        let image = dir.join("Lumia.AppImage");
        let newer = source(serve(feed_for("999.0.0", b"new")), image.clone());
        assert_eq!(check(&newer), Report::Checked(Some("999.0.0".into())));
        let older = source(serve(feed_for("0.0.1", b"old")), image);
        assert_eq!(check(&older), Report::Checked(None));
    }

    #[test]
    fn a_feed_signed_with_another_key_fails_the_check() {
        let dir = scratch("forged");
        let other =
            PrivateKey::from_base64("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=").unwrap();
        let feed = other.sign_feed(&lumia_appcast::write(
            "Lumia",
            "999.0.0",
            "https://x/y",
            1,
            "s",
        ));
        let forged = source(serve(feed.into_bytes()), dir.join("Lumia.AppImage"));
        assert!(matches!(check(&forged), Report::Failed(_)));
    }

    #[test]
    fn a_checked_download_replaces_the_appimage_and_a_tampered_one_does_not() {
        let dir = scratch("install");
        let image = dir.join("Lumia.AppImage");
        std::fs::write(&image, b"the old release").unwrap();
        let new = b"the new release".to_vec();
        let url = serve(new.clone());
        let release = |bytes: &[u8]| Release {
            version: "999.0.0".into(),
            display: "999.0.0".into(),
            url: url.clone(),
            length: bytes.len() as u64,
            signature: key().sign(bytes),
        };
        let at = source(String::new(), image.clone());

        // Signed for other bytes: refused, and the AppImage is as it was.
        *FOUND.lock().unwrap() = Some(release(b"something else entirely"));
        assert!(matches!(install(&at), Report::Failed(_)));
        assert_eq!(std::fs::read(&image).unwrap(), b"the old release");

        *FOUND.lock().unwrap() = Some(release(&new));
        assert_eq!(install(&at), Report::Installed("999.0.0".into()));
        assert_eq!(std::fs::read(&image).unwrap(), new);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&image).unwrap().permissions().mode() & 0o777,
            0o755
        );
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert_eq!(left.len(), 1, "nothing is left beside the AppImage");
    }

    #[test]
    fn an_appimage_where_this_user_cannot_write_says_so() {
        let dir = scratch("readonly");
        let image = dir.join("Lumia.AppImage");
        std::fs::write(&image, b"old").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = replace(&image, b"new");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Root writes anywhere, so only a refusal is checked for its words.
        if let Err(why) = result {
            assert!(why.contains("download the new release"), "{why}");
            assert_eq!(std::fs::read(&image).unwrap(), b"old");
        }
    }
}
