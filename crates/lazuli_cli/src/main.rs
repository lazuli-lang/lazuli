//! The published `lazuli` binary — a one-line shell over [`lazuli_cli::run`].
//!
//! All command logic lives in the library crate (`src/lib.rs` and the
//! `cli_run` / `commands` subtree) so the framework-dev `lazuli-dev` binary
//! (`src/bin/lazuli-dev.rs`) can reuse the same handlers + the
//! `build_module_from_path` compile pipeline. See SPEC-20 2/n.

fn main() -> anyhow::Result<()> {
    lazuli_cli::run()
}
