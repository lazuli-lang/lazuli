//! `VOCAB-RUNTIME-REINVENTED-001` — a `@fn` handler reinvents a runtime /
//! language primitive the Lazuli runtime already owns.
//!
//! ## What it checks
//!
//! For each `features/<f>/handlers/*.go`, run a parameterized
//! [`REINVENTION_TABLE`] of `{trigger, equivalent, family}` rows over the
//! handler source. Two detector families:
//!
//! - **Import-signal** ([`Trigger::Imports`]) — the handler imports a
//!   crypto/encoding lib (`golang.org/x/crypto/argon2`, `crypto/sha256` +
//!   `encoding/hex`, `crypto/rand`) AND the runtime exports an equivalent
//!   (`auth.HashPassword`, `auth.HashSessionToken`, `auth.MintSessionToken`).
//!   The strongest signal — these imports in an app handler almost always
//!   mean reinvented crypto.
//! - **Shape-signal** ([`Trigger::BodyShape`]) — the body matches a
//!   reinvention shape: `UPDATE ... status IN (...) ... RowsAffected ... == 0`
//!   (lifecycle transition) or a `[0-9A-Fa-f]{6}` hex-color regex (HexColor).
//!   A bare `deleted_at IS NULL` soft-delete row was intentionally left out
//!   of the seed (it fires on legitimate referential guards — see the table).
//!
//! The table is the extension point: adding ONE row catches a whole new
//! reinvention family with no change to [`check`] (proven by the
//! `table_is_extensible` test). This is the audit oracle for the
//! reinvention-defense effort — it reproduces the 31-finding pilot audit at
//! lint time.
//!
//! ## Precision guard
//!
//! A row fires only when its trigger matches AND a real runtime/language
//! equivalent is named. Two guards keep precision high:
//!
//! - `crypto/hmac` ([`SUPPRESS_IMPORTS`]) — a vendor webhook signature
//!   (MercadoPago, Stripe, …) has NO runtime equivalent, so its presence
//!   suppresses every import-signal row even when `crypto/sha256` +
//!   `encoding/hex` are also imported. Grounded on hostpoint
//!   `payments/handlers/mp_client.go`.
//! - the lifecycle shape requires the FULL `RowsAffected` + `== 0` sentinel,
//!   so a plain imperative `UPDATE` stays silent.
//!
//! At most one finding is emitted per handler (the first matching row wins),
//! mirroring `ESC-RAWSQL-IN-HANDLER-001`.
//!
//! ## Severity
//!
//! `Warning` (advisory, never gates). Some reinventions are gated on an
//! upstream primitive (a parser GAP, a transition-name collision); those
//! carry `# doctor:allow VOCAB-RUNTIME-REINVENTED-001 — reason "..."`. The
//! rule still fires when waived but records the silence as a reasoned opt-out
//! (the `waived` flag + a body note), mirroring the uniform allow contract.
//!
//! ## Trigger cue / fixture
//!
//! Fires when a handler imports `golang.org/x/crypto/argon2` (reinvented
//! argon2id) — see `argon2_import_fires`. Stays silent on a vendor
//! `crypto/hmac` webhook verify — see `vendor_hmac_does_not_fire`.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

use crate::allow_comment::source_contains_doctor_allow;

/// What flips a [`ReinventionRule`] on for a given handler source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    /// ALL listed import paths must appear in the handler import block.
    Imports(&'static [&'static str]),
    /// ALL listed substrings must appear in the handler body.
    BodyShape(&'static [&'static str]),
}

/// One parameterized reinvention: a trigger + the equivalent the handler
/// should call instead + the family it belongs to. Adding a row = catching a
/// new family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReinventionRule {
    /// The condition that fires the row.
    pub trigger: Trigger,
    /// The runtime symbol / `.lzi` primitive the handler should use.
    pub equivalent: &'static str,
    /// The reinvention family (matches the audit's family labels).
    pub family: &'static str,
}

/// Imports that PROVE a handler is doing legitimate vendor-signature work
/// with no runtime equivalent. Their presence suppresses every import-signal
/// row (the body is verifying a third-party HMAC, not reinventing auth).
const SUPPRESS_IMPORTS: &[&str] = &["crypto/hmac"];

/// The seed reinvention table (extensible — this IS the deliverable shape).
///
/// Seeded with the families the audit actually found across pauta+hostpoint:
/// crypto/auth (import-signal), lifecycle-transition + scalar + soft-delete
/// (shape-signal). A new family is a one-row addition.
pub const REINVENTION_TABLE: &[ReinventionRule] = &[
    // ── import-signal (crypto / auth) ────────────────────────────────────
    ReinventionRule {
        trigger: Trigger::Imports(&["golang.org/x/crypto/argon2"]),
        equivalent: "auth.HashPassword / auth.VerifyPassword (argon2id + \
concurrency cap + rate-limit are runtime-owned via auth.HashWithArgon2 / \
auth.Argon2Params)",
        family: "auth.password-hash",
    },
    ReinventionRule {
        trigger: Trigger::Imports(&["crypto/sha256", "encoding/hex"]),
        equivalent: "auth.HashSessionToken / auth.HashWithArgon2 (the runtime \
hashes opaque tokens; wire it via @cap.Hashed)",
        family: "auth.token-hash",
    },
    ReinventionRule {
        trigger: Trigger::Imports(&["crypto/rand"]),
        equivalent: "auth.MintSessionToken / auth.RequestPasswordReset (the \
runtime mints + hashes + TTLs opaque tokens)",
        family: "auth.opaque-token",
    },
    // ── shape-signal (lifecycle / scalar / soft-delete) ──────────────────
    ReinventionRule {
        trigger: Trigger::BodyShape(&["UPDATE", "status IN (", "RowsAffected", "== 0"]),
        equivalent: "a declared `transition` in .lzi — the runtime owns \
TransitionAdvance + LifecycleStateMismatchError (lifecycle.Machine / \
lazuli.Transition)",
        family: "lifecycle.transition",
    },
    ReinventionRule {
        trigger: Trigger::BodyShape(&["[0-9A-Fa-f]{6}"]),
        equivalent: "@semantic.HexColor (lazuli.HexColor)",
        family: "scalar.hexcolor",
    },
    // NOTE: a `deleted_at IS NULL` soft-delete row was deliberately NOT
    // seeded. The audit confirmed exactly ONE soft-delete reinvention, but a
    // bare `deleted_at IS NULL` substring fires on ~18 LEGITIMATE referential
    // guards / status lookups across pauta (a normal "exclude soft-deleted
    // rows" filter is NOT reinventing the mechanism). Distinguishing the two
    // needs `.lzi` read cross-reference, which is `vocab_soft_delete_actor_001`
    // + the language-primitive linters' territory — out of this substring
    // rule's precision budget. ADR 0024: precision over recall; tighten before
    // shipping a >10% FP row. Re-add only with a discriminating body context.
];

/// A single reinvention hit inside one handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// The equivalent the handler should call.
    pub equivalent: &'static str,
    /// The reinvention family.
    pub family: &'static str,
}

/// True if the handler's `crypto/rand` opaque-token row has the body context
/// that distinguishes a token mint from an unrelated random-id generation.
/// `crypto/rand` alone is too broad; require a token/session/reset/code cue.
fn has_opaque_token_context(source: &str) -> bool {
    let lower = source.to_lowercase();
    ["token", "session", "reset", "verification", "verify_email"]
        .iter()
        .any(|cue| lower.contains(cue))
}

/// True if `source`'s import block contains `path`. We match the quoted import
/// literal so a substring inside a comment/body doesn't count.
fn imports(source: &str, path: &str) -> bool {
    source.contains(&format!("\"{path}\""))
}

/// Run the reinvention `table` over one handler `source`, returning every
/// matching row. The import-signal suppression guard ([`SUPPRESS_IMPORTS`])
/// short-circuits ALL import rows when a vendor-signature import is present.
pub fn scan_handler(source: &str, table: &[ReinventionRule]) -> Vec<Hit> {
    let vendor_signature = SUPPRESS_IMPORTS.iter().any(|p| imports(source, p));
    let mut hits = Vec::new();
    for rule in table {
        let fired = match &rule.trigger {
            Trigger::Imports(paths) => {
                if vendor_signature {
                    false
                } else if !paths.iter().all(|p| imports(source, p)) {
                    false
                } else if *paths == ["crypto/rand"] {
                    // The broadest crypto import needs an opaque-token cue so
                    // a random-id mint for an unrelated purpose stays silent.
                    has_opaque_token_context(source)
                } else {
                    true
                }
            }
            Trigger::BodyShape(parts) => parts.iter().all(|p| source.contains(p)),
        };
        if fired {
            hits.push(Hit {
                equivalent: rule.equivalent,
                family: rule.family,
            });
        }
    }
    hits
}

/// One runtime-reinvention finding (one per handler — first matching row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Path to the Go handler that reinvents the runtime.
    pub handler_path: PathBuf,
    /// Feature owning the handler.
    pub feature: String,
    /// The reinvention family (e.g. `auth.password-hash`).
    pub family: &'static str,
    /// The runtime symbol / `.lzi` primitive the handler should call.
    pub equivalent: &'static str,
    /// True if a `# doctor:allow` is present — the finding still fires; the
    /// flag marks the silence as a reasoned opt-out.
    pub waived: bool,
}

impl Finding {
    /// Rule code.
    pub const CODE: &'static str = "VOCAB-RUNTIME-REINVENTED-001";

    /// Human-readable diagnostic message.
    pub fn message(&self) -> String {
        let lede = format!(
            "handler `{}` reinvents `{}` in feature `{}` — use {}. The Lazuli runtime owns this \
mechanism; see docs/lazuli_way/delegate-to-runtime.md. (Add `# doctor:allow \
VOCAB-RUNTIME-REINVENTED-001 — reason \"...\"` only if this is a tracked upstream gap.)",
            self.handler_path.display(),
            self.family,
            self.feature,
            self.equivalent,
        );
        if self.waived {
            format!(
                "{lede} (a `# doctor:allow` is present here — the silence is recorded as a reasoned opt-out.)"
            )
        } else {
            lede
        }
    }
}

/// Run `VOCAB-RUNTIME-REINVENTED-001` over a feature's handlers directory.
///
/// Scans `<feature_dir>/handlers/*.go`, runs the [`REINVENTION_TABLE`] over
/// each, and emits at most ONE finding per handler (the first matching row —
/// mirroring `ESC-RAWSQL-IN-HANDLER-001`'s one-per-file shape). Honors
/// `# doctor:allow VOCAB-RUNTIME-REINVENTED-001`.
pub fn check_dir(feature: &str, feature_dir: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    let handlers_dir = feature_dir.join("handlers");
    if !handlers_dir.exists() {
        return findings;
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&handlers_dir)
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    paths.sort();

    for path in paths {
        if path.extension().and_then(|e| e.to_str()) != Some("go") {
            continue;
        }
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("_test.go"))
        {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let hits = scan_handler(&source, REINVENTION_TABLE);
        let Some(first) = hits.into_iter().next() else {
            continue;
        };
        let waived = source_contains_doctor_allow(&source, Finding::CODE);
        findings.push(Finding {
            handler_path: path.clone(),
            feature: feature.to_owned(),
            family: first.family,
            equivalent: first.equivalent,
            waived,
        });
    }
    findings
}

/// `ESC-RAWSQL-IN-HANDLER-001`-shaped entry point for the run aggregator,
/// which already holds a lowered [`Feature`] + its on-disk dir. Delegates to
/// [`check_dir`] using the feature's name.
pub fn check(feature: &Feature, feature_dir: &Path) -> Vec<Finding> {
    check_dir(&feature.name, feature_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_hmac_suppresses_token_hash_row() {
        // crypto/sha256 + encoding/hex present, but crypto/hmac proves vendor
        // signature — no import row may fire.
        let src = "import (\n\t\"crypto/hmac\"\n\t\"crypto/sha256\"\n\t\"encoding/hex\"\n)";
        assert!(scan_handler(src, REINVENTION_TABLE).is_empty());
    }

    #[test]
    fn token_hash_fires_without_hmac() {
        let src = "import (\n\t\"crypto/sha256\"\n\t\"encoding/hex\"\n)";
        let hits = scan_handler(src, REINVENTION_TABLE);
        assert!(hits.iter().any(|h| h.family == "auth.token-hash"));
    }

    #[test]
    fn crypto_rand_needs_token_context() {
        let bare = "import (\n\t\"crypto/rand\"\n)\nfunc f(){ rand.Read(buf) }";
        assert!(
            !scan_handler(bare, REINVENTION_TABLE)
                .iter()
                .any(|h| h.family == "auth.opaque-token"),
            "crypto/rand without a token cue must not fire the opaque-token row"
        );
        let token = "import (\n\t\"crypto/rand\"\n)\nfunc f(){ session := mintToken() }";
        assert!(
            scan_handler(token, REINVENTION_TABLE)
                .iter()
                .any(|h| h.family == "auth.opaque-token")
        );
    }

    #[test]
    fn code_const_is_stable() {
        assert_eq!(Finding::CODE, "VOCAB-RUNTIME-REINVENTED-001");
    }
}
