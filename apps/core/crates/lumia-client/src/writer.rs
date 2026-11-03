//! The single-writer lock between the app and its notification extension
//! (architecture 15.5): two processes share one instance's stores through
//! an app-group container, and advancing MLS state from both at once is a
//! known source of E2EE bugs. The app holds the lock shared while it runs
//! in the foreground and releases it in the background; the extension
//! takes it exclusively for its fetch-and-decrypt and gives up, showing
//! the generic notice, when the app holds it (the app is live and will
//! show the message itself). Whichever process wrote bumps a generation
//! in the store, and the other reloads before its next operation.
//!
//! An advisory lock on a file beside the stores (the standard library's,
//! `flock` underneath); nothing on the web, where one tab owns the store
//! through Web Locks. Where the platform's standard library reports file
//! locking as unsupported (Android's does), the lock is a no-op: no second
//! process shares the stores there, since push is handled inside the app.
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::ClientError;

/// The lock file beside an instance's stores.
#[derive(Debug, Clone)]
pub struct WriterLock {
    path: PathBuf,
}

/// Held while the lock is; dropping it releases.
#[derive(Debug)]
pub struct WriterGuard {
    _file: Option<File>,
}

impl WriterLock {
    /// The lock at `path`, created on first use.
    #[must_use]
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
        }
    }

    fn open(&self) -> Result<File, ClientError> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.path)
            .map_err(|e| ClientError::State(format!("writer lock {}: {e}", self.path.display())))
    }

    /// Takes the lock shared, waiting as long as an exclusive holder needs.
    ///
    /// # Errors
    /// When the file cannot be opened or locked.
    pub fn shared(&self) -> Result<WriterGuard, ClientError> {
        let file = self.open()?;
        match file.lock_shared() {
            Ok(()) => Ok(WriterGuard { _file: Some(file) }),
            Err(e) if e.kind() == std::io::ErrorKind::Unsupported => {
                Ok(WriterGuard { _file: None })
            }
            Err(e) => Err(ClientError::State(format!("writer lock: {e}"))),
        }
    }

    /// Takes the lock exclusively, trying for up to `timeout`; `None` when
    /// another process still held it.
    ///
    /// # Errors
    /// When the file cannot be opened or locked.
    pub fn exclusive(&self, timeout: Duration) -> Result<Option<WriterGuard>, ClientError> {
        let file = self.open()?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Some(WriterGuard { _file: Some(file) })),
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Error(e)) if e.kind() == std::io::ErrorKind::Unsupported => {
                    return Ok(Some(WriterGuard { _file: None }));
                }
                Err(TryLockError::Error(e)) => {
                    return Err(ClientError::State(format!("writer lock: {e}")));
                }
            }
            if std::time::Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn exclusive_waits_for_shared_holders() {
        let dir = std::env::temp_dir().join(format!("lumia-writer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lock = WriterLock::new(&dir.join("writer.lock"));
        let shared = lock.shared().unwrap();
        // The lock is per open file description, so a second open contends.
        assert!(lock
            .exclusive(Duration::from_millis(120))
            .unwrap()
            .is_none());
        drop(shared);
        let exclusive = lock.exclusive(Duration::from_millis(120)).unwrap();
        assert!(exclusive.is_some());
        assert!(lock
            .exclusive(Duration::from_millis(120))
            .unwrap()
            .is_none());
        drop(exclusive);
        assert!(lock.shared().is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
