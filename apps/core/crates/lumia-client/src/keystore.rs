//! The CLI's keystore: one file per secret under a directory created with
//! mode 0600/0700. Plaintext at rest, for development and the headless
//! client only; the platform shells provide real secret stores.

use std::fs;
use std::path::{Path, PathBuf};

use lumia_core::keystore::{Keystore, KeystoreError};

/// A directory of secret files.
#[derive(Debug)]
pub struct FileKeystore {
    dir: PathBuf,
}

impl FileKeystore {
    /// Opens (creating) the keystore directory.
    ///
    /// # Errors
    /// When the directory cannot be created.
    pub fn open(dir: &Path) -> Result<Self, KeystoreError> {
        fs::create_dir_all(dir).map_err(|e| KeystoreError::Unavailable(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
                .map_err(|e| KeystoreError::Unavailable(e.to_string()))?;
        }
        Ok(Self {
            dir: dir.to_path_buf(),
        })
    }

    fn path(&self, name: &str) -> PathBuf {
        let safe: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.dir.join(safe)
    }
}

impl Keystore for FileKeystore {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, KeystoreError> {
        match fs::read(self.path(name)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(KeystoreError::Other(e.to_string())),
        }
    }

    fn put(&mut self, name: &str, value: &[u8]) -> Result<(), KeystoreError> {
        let path = self.path(name);
        fs::write(&path, value).map_err(|e| KeystoreError::Other(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                .map_err(|e| KeystoreError::Other(e.to_string()))?;
        }
        Ok(())
    }

    fn delete(&mut self, name: &str) -> Result<(), KeystoreError> {
        match fs::remove_file(self.path(name)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(KeystoreError::Other(e.to_string())),
        }
    }
}
