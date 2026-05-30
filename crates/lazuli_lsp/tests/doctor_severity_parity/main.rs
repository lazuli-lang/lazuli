//! Anti-drift severity-parity proof (Wave D4).
//!
//! Proves the SINGLE-SOURCE property mechanically: the LSP and the CLI
//! `lazuli doctor` resolve **identical** severities because both reduce to
//! the one shared resolver in `lazuli_doctor_config`
//! (`effective_severity` / `effective_severity_over_base`). If a future
//! change hardcodes a severity on either side instead of routing through
//! the shared resolver, the concrete per-cell assertions below fail the
//! build.
//!
//! ## The ownership partition decides WHICH resolver a code uses
//!
//! D3 split every doctor finding into two buckets (`is_lsp_owned` /
//! `is_doctor_owned`, total + disjoint). In-editor, the bucket decides the
//! severity path — and crucially BOTH paths reduce to the same
//! `lazuli_doctor_config` function as the CLI:
//!
//! - **Doctor-owned** (SCREAMING-KEBAB rule-catalog codes — `HOOK-*`,
//!   `VOCAB-*`, `TEST-*`, …): published in-editor by the package ENGINE
//!   (`doctor_engine::doctor_owned_for_document` → `run_package`). The
//!   engine resolves their severity with `effective_severity` — the *exact*
//!   call the CLI's `doctor_severity_for` /
//!   `package_methods::context_vocab_diagnostics` make
//!   (`crates/lazuli_doctor_run/src/doctor/diagnostic.rs:225`,
//!   `.../package_methods.rs:87-96`). Same function ⇒ identical severity;
//!   only the final `DoctorSeverity → DiagnosticSeverity` map is applied,
//!   and Property 0 pins that map equal to the CLI's inverse.
//!
//! - **LSP-owned** (file-local kebab "contract/shape" codes —
//!   `env-schema-contract`, …): published in-editor by the synchronous
//!   file-local pass via the REAL
//!   `doctor_local::doctor_class_lsp_severity` =
//!   `effective_severity_over_base(code, base, from_code_prefix(code),
//!   cfg).map(lsp_severity)`
//!   (`crates/lazuli_lsp/src/diagnostics/doctor_local/mod.rs:63-77`). The
//!   CLI emits these same file-local codes at the same intrinsic base, so
//!   `effective_severity_over_base` is the matching CLI computation.
//!
//! Either way the answer comes from `lazuli_doctor_config`; this test calls
//! the **real** LSP-side function (`lazuli_lsp::test_surface`) and the
//! **real** shared resolver, then asserts concrete per-cell severities so a
//! one-sided hardcode is caught.

use lazuli_doctor_config::{
    DoctorProfile, DoctorSeverity, ResolvedDoctorConfig, RuleCategory, SeverityOverride,
    effective_severity, effective_severity_over_base,
};
use lazuli_lsp::test_surface::{
    doctor_class_lsp_severity, is_doctor_owned, is_lsp_owned, lsp_severity,
};
use tower_lsp::lsp_types::DiagnosticSeverity;

include!("main_p1.rs");
include!("main_p2.rs");
