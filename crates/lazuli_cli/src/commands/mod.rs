//! `lazuli` subcommand handler modules — rails-style W4.5 split.
//!
//! `main.rs` carries the `clap` `#[derive(Parser)]` definitions and the
//! top-level dispatcher; every per-subcommand handler body lives under
//! this directory, one file per `Commands::*` variant. The dispatch arm
//! in `main.rs` is reduced to a single call into the corresponding
//! `crate::commands::<subcmd>::*_command(...)` function.
//!
//! Why this split:
//!
//! - **Cold-readability**: a 16k-LOC `main.rs` forces every
//!   contributor to scroll through unrelated subcommand logic to find
//!   the one they care about. Per-subcommand files cap each file at
//!   the cognitive size of its concept ("what does `lazuli generate`
//!   do?") rather than the union of every command.
//!
//! - **Test affordance**: extracted handlers are `pub` functions with
//!   typed signatures, which means future integration tests can call
//!   them directly without going through `Cli::parse()` + a child
//!   process.
//!
//! - **ABI guarantee**: the clap definitions, value-enum mirrors, and
//!   `--help` output stay in `main.rs`. The split is purely the
//!   handler body, not the CLI surface. `lazuli --help` and
//!   `lazuli <subcommand> --help` remain byte-identical pre/post.
//!
//! Naming convention: file name matches the subcommand in snake_case
//! (`init.rs` for `lazuli init`, `spike_generate.rs` for
//! `lazuli spike-generate`). The handler function inside is named
//! `<subcommand>_command` for symmetry with the `Commands::*` variant
//! and with the pre-split function name. Cross-refs:
//! `docs/proposals/rails-style-refactor-2026-05-24.md` §W4.5.

pub mod init;
