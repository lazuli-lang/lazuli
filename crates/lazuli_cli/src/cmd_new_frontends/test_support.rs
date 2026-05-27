//! Inline tempdir helper shared by the web/mobile/cross scaffold
//! tests in this module tree.
//!
//! Mirrors the pattern used by `cmd_generate_feature::tests` — avoids
//! adding a `tempfile` dev-dep for one module.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub(super) fn new() -> io::Result<Self> {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let bump = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "lazuli-new-frontends-test-{}-{suffix}-{bump}",
            std::process::id()
        ));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub(super) fn tempdir() -> TempDir {
    TempDir::new().unwrap()
}
