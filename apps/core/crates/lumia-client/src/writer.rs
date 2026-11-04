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
//! locking as unsupported (Android's does), the lock is kept in this
//! process instead, keyed by the file's path: no second process shares the
//! stores there, since push is handled inside the app, but the push
//! receiver's client and the screens' client are two clients in one
//! process on the same stores.
use std::collections::HashMap;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
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
    _local: Option<LocalGuard>,
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
            Ok(()) => Ok(WriterGuard {
                _file: Some(file),
                _local: None,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::Unsupported => Ok(WriterGuard {
                _file: None,
                _local: Some(LocalLock::at(&self.path).shared()),
            }),
            Err(e) => Err(ClientError::State(format!("writer lock: {e}"))),
        }
    }

    /// Takes the lock exclusively, trying for up to `timeout`; `None` when
    /// another holder kept it.
    ///
    /// # Errors
    /// When the file cannot be opened or locked.
    pub fn exclusive(&self, timeout: Duration) -> Result<Option<WriterGuard>, ClientError> {
        let file = self.open()?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            match file.try_lock() {
                Ok(()) => {
                    return Ok(Some(WriterGuard {
                        _file: Some(file),
                        _local: None,
                    }))
                }
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Error(e)) if e.kind() == std::io::ErrorKind::Unsupported => {
                    return Ok(LocalLock::at(&self.path).exclusive(timeout).map(|local| {
                        WriterGuard {
                            _file: None,
                            _local: Some(local),
                        }
                    }));
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

/// The in-process lock for one path: readers and at most one writer, the
/// same shape as the file lock.
#[derive(Debug, Default)]
struct LocalState {
    readers: usize,
    writer: bool,
}

#[derive(Debug, Clone)]
struct LocalLock(Arc<Mutex<LocalState>>);

/// Held while the in-process lock is; dropping it releases.
#[derive(Debug)]
struct LocalGuard {
    lock: LocalLock,
    exclusive: bool,
}

impl LocalLock {
    /// The process's lock for `path`, one per path for the process's life.
    fn at(path: &Path) -> Self {
        static LOCKS: OnceLock<Mutex<HashMap<PathBuf, LocalLock>>> = OnceLock::new();
        let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
        let mut locks = locks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        locks
            .entry(path.to_path_buf())
            .or_insert_with(|| Self(Arc::new(Mutex::new(LocalState::default()))))
            .clone()
    }

    fn state(&self) -> std::sync::MutexGuard<'_, LocalState> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Shared: waits for a writer to finish, then counts itself in.
    fn shared(&self) -> LocalGuard {
        loop {
            {
                let mut state = self.state();
                if !state.writer {
                    state.readers += 1;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        LocalGuard {
            lock: self.clone(),
            exclusive: false,
        }
    }

    /// Exclusive: only when nobody holds it, tried until `timeout`.
    fn exclusive(&self, timeout: Duration) -> Option<LocalGuard> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            {
                let mut state = self.state();
                if !state.writer && state.readers == 0 {
                    state.writer = true;
                    return Some(LocalGuard {
                        lock: self.clone(),
                        exclusive: true,
                    });
                }
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for LocalGuard {
    fn drop(&mut self) {
        let mut state = self.lock.state();
        if self.exclusive {
            state.writer = false;
        } else {
            state.readers = state.readers.saturating_sub(1);
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

    #[test]
    fn in_process_lock_has_the_same_shape() {
        let path = PathBuf::from(format!("in-process-{}", std::process::id()));
        let lock = LocalLock::at(&path);
        let shared = lock.shared();
        assert!(lock.exclusive(Duration::from_millis(120)).is_none());
        // The same path from elsewhere in the process is the same lock.
        assert!(LocalLock::at(&path)
            .exclusive(Duration::from_millis(120))
            .is_none());
        drop(shared);
        let exclusive = lock.exclusive(Duration::from_millis(120)).unwrap();
        assert!(lock.exclusive(Duration::from_millis(120)).is_none());
        drop(exclusive);
        drop(lock.shared());
        assert!(lock.exclusive(Duration::from_millis(120)).is_some());
    }
}
