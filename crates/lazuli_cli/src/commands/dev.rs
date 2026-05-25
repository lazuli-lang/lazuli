//! `lazuli dev` — watch the source tree, regenerate Go output on
//! change, and (unless `--no-run` is set) run the resulting Lazuli Go
//! server.
//!
//! The handler is a thin wire between the clap arm and
//! `crate::dev::run_dev`. The actual file-watching, debounce, and
//! regenerate-and-restart loop lives in `crate::dev` so unit tests
//! and integration tests can drive `DevOptions` directly without
//! going through `Cli::parse()`.
//!
//! Typical authoring loop: `lazuli dev path=app/ out=dist/go` — every
//! save to a `.lzi`/`.lzx` triggers regen and a Go restart. Authors
//! hit Ctrl-C to stop the loop.
//!
//! Cross-refs:
//! - `crate::dev::run_dev` / `crate::dev::DevOptions` — the actual
//!   loop and its typed options.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;

use crate::dev;

/// Handler for the `Commands::Dev` clap arm.
pub fn dev_command(path: PathBuf, out: PathBuf, no_run: bool, debounce_ms: u64) -> Result<()> {
    dev::run_dev(dev::DevOptions {
        source_root: path,
        out,
        no_run,
        debounce: Duration::from_millis(debounce_ms),
    })
}
