//! `lazuli-dev` — the framework-development binary (contributors only).
//!
//! A one-line shell over [`lazuli_cli::run_dev`]. Exposes the commands that
//! mean nothing to an app developer and were therefore split off the published
//! `lazuli` surface (SPEC-20 2/n): `parse`, `spike-generate`, `examples`, and
//! `self-doctor` (the old `lazuli doctor --self`). Run via
//! `cargo run -p lazuli_cli --bin lazuli-dev -- <cmd>`.

fn main() -> anyhow::Result<()> {
    lazuli_cli::run_dev()
}
