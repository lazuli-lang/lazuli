//! Compile-time version pin for Lazuli CLI/runtime compatibility checks.

use anyhow::{Result, bail};

use crate::lazurite_manifest::Manifest;

/// Compile-time Lazuli version baked in via `CARGO_PKG_VERSION`.
pub const LAZULI_VERSION: &str = env!("CARGO_PKG_VERSION");

/// True when `declared` (trimmed) is compatible with the compiled-in
/// Lazuli version. Used by [`enforce_manifest_pin`] and the LSP version
/// surface.
///
/// The reserved sentinel `self` always matches: it is the value the
/// framework's *own* workspace `Lazurite.toml` carries (`[lazuli] runtime
/// = "self"`), declaring "do not pin — I am built from this very tree".
/// Pilot projects pin a concrete version (`"0.1.0"`); only the framework
/// repo uses `self`, so honoring it here keeps `lazuli-dev self-doctor`
/// (and every other in-repo command) from failing its own version gate.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::version::{manifest_pin_matches, LAZULI_VERSION};
/// assert!(manifest_pin_matches(LAZULI_VERSION));
/// assert!(manifest_pin_matches("self")); // reserved no-pin sentinel
/// ```
pub fn manifest_pin_matches(declared: &str) -> bool {
    let declared = declared.trim();
    declared == "self" || declared == LAZULI_VERSION
}

/// Fail with a friendly remediation message when `manifest` carries a
/// `[lazuli] runtime = …` that does not match this binary. Pilots
/// pass `--allow-version-mismatch` to opt out.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::version::enforce_manifest_pin;
/// // enforce_manifest_pin(manifest.as_ref())?;
/// ```
pub fn enforce_manifest_pin(manifest: Option<&Manifest>) -> Result<()> {
    let Some(manifest) = manifest else {
        return Ok(());
    };
    let Some(declared) = manifest.lazuli_runtime_version() else {
        return Ok(());
    };

    if manifest_pin_matches(declared) {
        return Ok(());
    }

    bail!(
        "Lazurite.toml `[lazuli] runtime = \"{declared}\"` does not match installed Lazuli version `{}`. \
         Pin mismatch can produce incompatible output. \
         Pass `--allow-version-mismatch` to override (and update Lazurite.toml in a follow-up commit).",
        LAZULI_VERSION
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_version_matches() {
        assert!(manifest_pin_matches(LAZULI_VERSION));
    }

    #[test]
    fn different_version_does_not_match() {
        assert!(!manifest_pin_matches("0.0.0"));
    }

    #[test]
    fn whitespace_is_trimmed() {
        assert!(manifest_pin_matches(&format!(" {LAZULI_VERSION} ")));
    }

    #[test]
    fn self_sentinel_always_matches() {
        // `[lazuli] runtime = "self"` is the framework workspace's reserved
        // no-pin value — it must satisfy the gate regardless of the binary's
        // CARGO_PKG_VERSION (otherwise `lazuli-dev self-doctor` fails on its
        // own repo).
        assert!(manifest_pin_matches("self"));
        assert!(manifest_pin_matches("  self  "));
    }
}
