//! Operator-local CLI token cache (STORE-D). JSON-serialized flat file
//! at `directories::ProjectDirs::data_dir().join("cli-tokens.json")`,
//! mode 0600 on POSIX. The daemon plays no role; CLI HTTP calls attach
//! the cached access token as `Authorization: Bearer <jwt>` and the
//! daemon's existing JWT validator handles auth (matching the SPA
//! pattern used by `authedFetch`).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CliTokenStore {
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(skip)]
    path: PathBuf,
}

impl CliTokenStore {
    /// Load from the default path. Missing file returns an empty store.
    pub fn load() -> anyhow::Result<Self> {
        let path = cli_token_store_path()?;
        Self::load_from(&path)
    }

    /// Load from a specific path. Missing file returns an empty store.
    pub fn load_from(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self {
                path: path.to_path_buf(),
                ..Self::default()
            });
        }
        let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        let mut store: Self =
            serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
        store.path = path.to_path_buf();
        Ok(store)
    }

    /// Test seam: construct an empty store at `path` without touching disk.
    #[doc(hidden)]
    pub fn with_path(path: PathBuf) -> Self {
        Self {
            path,
            ..Self::default()
        }
    }

    /// Persist to the configured path. Creates parent directories. On
    /// POSIX, writes atomically with mode 0600 via tempfile-and-rename.
    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("mkdir -p {}", parent.display()))?;
        }
        let json = serde_json::to_vec_pretty(self)?;
        write_atomic_0600(&self.path, &json)
    }

    /// Remove the on-disk file. Missing file is not an error.
    pub fn clear(&self) -> anyhow::Result<()> {
        Self::clear_at(&self.path)
    }

    /// Test seam: clear by explicit path (used to verify idempotency
    /// without invoking `cli_token_store_path()`).
    #[doc(hidden)]
    pub fn clear_at(path: &Path) -> anyhow::Result<()> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).with_context(|| format!("rm {}", path.display())),
        }
    }
}

pub fn cli_token_store_path() -> anyhow::Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "Spacebot", "spacebot")
        .context("cannot resolve user data directory; set $XDG_DATA_HOME or $HOME")?;
    Ok(dirs.data_dir().join("cli-tokens.json"))
}

#[cfg(unix)]
fn write_atomic_0600(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;

    let parent = path.parent().context("path has no parent")?;
    let tmp = tempfile::NamedTempFile::new_in(parent)?;
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(tmp.path())?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    tmp.persist(path)
        .map_err(|e| anyhow::anyhow!("persist token file: {e}"))?;
    Ok(())
}

#[cfg(not(unix))]
fn write_atomic_0600(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    // Windows: %APPDATA% NTFS ACLs (per-user) provide confidentiality.
    fs::write(path, bytes).with_context(|| format!("write {}", path.display()))
}
