//! One running copy per data directory, so two processes never open one MLS store
//! (architecture 15.5 says why a ratchet has one writer), and a second launch reaches the
//! first: its `lumia:` links are handed over and the first one's window comes forward.
//!
//! Two things make that true, because neither does alone:
//!
//! - **A lock on the directory.** The first process takes an exclusive lock on
//!   `instance.lock` in the data directory and holds it until it exits. This is the rule:
//!   whoever cannot take it does not open the stores, however it was started (a second
//!   `open -n`, the binary run by hand, a headless `--snap` while the app runs).
//! - **A way to reach the holder.** macOS keeps one process per bundle and hands it links
//!   itself, and Rune does the same for Windows over a named pipe
//!   ([`rune::single_instance`]). Neither covers a copy started around them, and Linux
//!   starts a new process for every link. So on Unix the holder also listens on a socket
//!   whose path it writes into the lock file, and a later launch sends its links there.
//!
//! A headless run takes the lock too and never forwards: it is a tool drawing a picture,
//! and it says so when the directory is in use.

use std::fs::File;
use std::path::Path;

/// A later launch, as the running copy hears it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Launch {
    /// The activation token the launcher gave the later process (`XDG_ACTIVATION_TOKEN`,
    /// `DESKTOP_STARTUP_ID`), which lets a Wayland compositor give the window focus. Empty
    /// where there is none.
    pub token: String,
    /// The `lumia:` links among its arguments.
    pub links: Vec<String>,
}

/// How a launch stands to the copies already running on its data directory.
#[derive(Debug)]
pub enum Claim {
    /// This is the one copy. Keep the hold until the process exits.
    First(Hold),
    /// Another copy runs and has taken this launch's links: exit quietly.
    Forwarded,
    /// Another copy holds the directory and this launch cannot go on: exit, saying why.
    Refused(String),
}

/// The lock on the data directory, and the socket later launches reach this process on.
#[derive(Debug)]
pub struct Hold {
    /// Held open, and so locked, for as long as the hold lives.
    _lock: File,
    #[cfg(unix)]
    listener: Option<(std::os::unix::net::UnixListener, std::path::PathBuf)>,
    #[cfg(unix)]
    socket: Option<std::path::PathBuf>,
}

/// The `lumia:` links among this process's arguments. Linux and Windows pass a link to the
/// process they start for it as an argument; macOS sends it as an Apple event instead.
pub fn launch_links() -> Vec<String> {
    std::env::args().skip(1).filter(|a| is_link(a)).collect()
}

fn is_link(argument: &str) -> bool {
    argument
        .get(..8)
        .is_some_and(|start| start.eq_ignore_ascii_case("lumia://"))
}

/// The name this app's one process goes by, for Rune's pipe on Windows: the bundle
/// identifier for the usual data directory, and a name of its own for any other, so a
/// second profile (`LUMIA_DATA_DIR`) runs beside the first.
pub fn app_id(data_dir: &Path, usual: bool) -> String {
    if usual {
        return super::APP_ID.to_string();
    }
    // FNV-1a: the same name for the same directory in every build.
    let hash = data_dir
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    format!("{}.{hash:016x}", super::APP_ID)
}

/// Claims `data_dir` for this process. `launch` is what to hand a copy that already runs;
/// a `headless` run (Rune's tooling flags) never hands anything over.
pub fn claim(data_dir: &Path, launch: &Launch, headless: bool) -> Claim {
    let usual = std::env::var_os("LUMIA_DATA_DIR").is_none_or(|dir| dir.is_empty());
    // Windows: the first process owns a named pipe, and a later one hands it its arguments.
    // Rune claims on Linux too, with a socket of its own per process; Unix has the socket
    // below, and two would hand every link over twice.
    if cfg!(windows) && !headless && !rune::single_instance(&app_id(data_dir, usual)) {
        return Claim::Forwarded;
    }
    let path = data_dir.join("instance.lock");
    let opened = std::fs::create_dir_all(data_dir).and_then(|()| {
        File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
    });
    let lock = match opened {
        Ok(lock) => lock,
        Err(error) => return Claim::Refused(format!("{}: {error}", path.display())),
    };
    match lock.try_lock() {
        Ok(()) => Claim::First(Hold::new(lock, data_dir, headless)),
        Err(std::fs::TryLockError::WouldBlock) if headless => Claim::Refused(format!(
            "another copy of Lumia is using {}; set LUMIA_DATA_DIR to draw from another directory",
            data_dir.display()
        )),
        Err(std::fs::TryLockError::WouldBlock) => match forward(lock, launch) {
            Ok(()) => Claim::Forwarded,
            Err(error) => Claim::Refused(format!(
                "Lumia is already running on {} and did not answer ({error})",
                data_dir.display()
            )),
        },
        // A filesystem that cannot lock: run, as before there was a lock.
        Err(std::fs::TryLockError::Error(error)) => {
            eprintln!("lumia: {} cannot be locked: {error}", path.display());
            Claim::First(Hold::new(lock, data_dir, true))
        }
    }
}

impl Hold {
    #[cfg(unix)]
    fn new(lock: File, data_dir: &Path, quiet: bool) -> Self {
        let listener = if quiet {
            None
        } else {
            unix::listen(&lock, data_dir)
        };
        let socket = listener.as_ref().map(|(_, path)| path.clone());
        Self {
            _lock: lock,
            listener,
            socket,
        }
    }

    #[cfg(not(unix))]
    fn new(lock: File, _data_dir: &Path, _quiet: bool) -> Self {
        Self { _lock: lock }
    }

    /// Starts hearing later launches, each handed to `heard` on a thread of its own. On
    /// Windows Rune hears them (its pipe feeds `App::on_open_url` and `on_reopen`), and a
    /// headless run hears none.
    pub fn listen(&mut self, heard: impl Fn(Launch) + Send + 'static) {
        #[cfg(unix)]
        if let Some((listener, _)) = self.listener.take() {
            let spawned = std::thread::Builder::new()
                .name("lumia-instance".into())
                .spawn(move || unix::serve(&listener, &heard));
            if let Err(error) = spawned {
                eprintln!("lumia: later launches will not reach this one: {error}");
            }
        }
        #[cfg(not(unix))]
        let _ = heard;
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(socket) = &self.socket {
            let _ = std::fs::remove_file(socket);
        }
    }
}

/// Hands `launch` to the copy that holds the lock.
#[cfg(unix)]
fn forward(lock: File, launch: &Launch) -> std::io::Result<()> {
    unix::forward(lock, launch)
}

/// Windows reaches here only when Rune's pipe did not answer (the first process hangs or
/// is going away) and the lock is still held.
#[cfg(not(unix))]
fn forward(_lock: File, _launch: &Launch) -> std::io::Result<()> {
    Err(std::io::Error::other("it holds its data directory"))
}

#[cfg(unix)]
mod unix {
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use super::Launch;

    /// The longest socket path every Unix takes (`sun_path` is 104 bytes on macOS).
    const LONGEST_PATH: usize = 100;
    /// The most a later launch may send: a few links.
    const LONGEST_MESSAGE: u64 = 64 * 1024;
    const PATIENCE: Duration = Duration::from_secs(2);

    /// Binds the socket and writes its path into the lock file for later launches to read.
    /// `None`, with the reason on stderr, when no socket could be made: the lock still
    /// holds, later launches just cannot hand anything over.
    pub(super) fn listen(mut lock: &File, data_dir: &Path) -> Option<(UnixListener, PathBuf)> {
        let bound = places(data_dir).into_iter().find_map(|path| {
            // Whatever is there is a dead copy's: this process holds the lock.
            let _ = std::fs::remove_file(&path);
            UnixListener::bind(&path)
                .ok()
                .map(|listener| (listener, path))
        });
        let Some((listener, path)) = bound else {
            eprintln!("lumia: no socket for later launches to reach this one on");
            return None;
        };
        let written = lock
            .set_len(0)
            .and_then(|()| lock.seek(SeekFrom::Start(0)))
            .and_then(|_| lock.write_all(path.as_os_str().as_encoded_bytes()))
            .and_then(|()| lock.flush());
        if let Err(error) = written {
            eprintln!("lumia: writing the socket's path: {error}");
            let _ = std::fs::remove_file(&path);
            return None;
        }
        Some((listener, path))
    }

    /// Where the socket may go: beside the lock when the path is short enough for a
    /// socket, else in the user's runtime or temporary directory.
    fn places(data_dir: &Path) -> Vec<PathBuf> {
        let beside = data_dir.join("instance.sock");
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|dir| !dir.is_empty())
            .map_or_else(std::env::temp_dir, PathBuf::from);
        let elsewhere = runtime.join(format!("lumia-{}.sock", std::process::id()));
        [beside, elsewhere]
            .into_iter()
            .filter(|path| path.as_os_str().len() <= LONGEST_PATH)
            .collect()
    }

    /// Hears later launches until the process exits.
    pub(super) fn serve(listener: &UnixListener, heard: &impl Fn(Launch)) {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            if let Some(launch) = receive(stream) {
                heard(launch);
            }
        }
    }

    /// One later launch: its activation token on the first line, a link on each line after,
    /// answered with a byte so it knows it was taken. A peer that says nothing in time, or
    /// too much, is dropped.
    fn receive(mut stream: UnixStream) -> Option<Launch> {
        stream.set_read_timeout(Some(PATIENCE)).ok()?;
        stream.set_write_timeout(Some(PATIENCE)).ok()?;
        let mut text = String::new();
        (&stream)
            .take(LONGEST_MESSAGE)
            .read_to_string(&mut text)
            .ok()?;
        let mut lines = text.lines();
        let token = lines.next().unwrap_or_default().trim().to_string();
        let links = lines
            .map(str::trim)
            .filter(|line| super::is_link(line))
            .map(str::to_string)
            .collect();
        stream.write_all(b"k").ok()?;
        Some(Launch { token, links })
    }

    /// Sends `launch` to the socket named in the lock file and waits for the holder to
    /// take it. The holder writes the path just after it takes the lock, so a launch that
    /// loses the race by a moment reads again.
    pub(super) fn forward(mut lock: File, launch: &Launch) -> std::io::Result<()> {
        let mut last = std::io::Error::other("it names no socket");
        for attempt in 0..10 {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(100));
            }
            let mut path = String::new();
            lock.seek(SeekFrom::Start(0))?;
            lock.read_to_string(&mut path)?;
            if path.trim().is_empty() {
                continue;
            }
            match send(Path::new(path.trim()), launch) {
                Ok(()) => return Ok(()),
                Err(error) => last = error,
            }
        }
        Err(last)
    }

    fn send(socket: &Path, launch: &Launch) -> std::io::Result<()> {
        let mut stream = UnixStream::connect(socket)?;
        stream.set_read_timeout(Some(PATIENCE))?;
        stream.set_write_timeout(Some(PATIENCE))?;
        let mut message = format!("{}\n", launch.token.trim());
        for link in &launch.links {
            message.push_str(link.trim());
            message.push('\n');
        }
        stream.write_all(message.as_bytes())?;
        stream.shutdown(std::net::Shutdown::Write)?;
        let mut answer = [0u8; 1];
        stream.read_exact(&mut answer)?;
        Ok(())
    }
}
