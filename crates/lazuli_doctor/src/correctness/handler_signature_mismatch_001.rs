//! HANDLER-SIGNATURE-MISMATCH-001 — handler's Go function signature
//! disagrees with the codegen-emitted `Command[Input, Output]`
//! materialisation in `dist/go/<feature>/command.gen.go`.
//!
//! Sibling of [`super::handler_missing_001`]. That rule only stats the
//! file. THIS rule, once the file exists, opens both the handler Go
//! source and the codegen Go source, extracts the operative type idents
//! from each, and fires when they disagree.
//!
//! The runtime's [`ReturnsFromRegistry[I, O]`](
//! ../../../../runtime/go/lazuli/handler_registry.go) performs the same
//! comparison at dispatch time and returns a 500 `wrong signature`
//! error when they don't match. This rule promotes that runtime check
//! into a static check — the doctor's whole job per the runtime comment
//! at `handler_registry.go:67-72`.
//!
//! ## Detection
//!
//! For every `@fn.<name>` command-handler reference in the IR:
//!
//! 1. Locate the handler Go file via [`crate::handler_path::resolve`].
//!    Missing → skip (HANDLER-MISSING-001 owns that surface).
//! 2. Locate the codegen file at
//!    `dist/go/<feature>/command.gen.go`. Missing → skip silently
//!    (a sibling `@correctness.migration_out_of_sync`-shaped rule owns
//!    the "you didn't run lazuli generate go" path).
//! 3. Extract `func PascalCase(ctx *lazuli.Ctx, input <Type>) (<Out>, error)`
//!    from the handler. Unreadable → emit a
//!    [`Diff::HandlerSignatureUnreadable`] finding (conservative — the
//!    author opts out via `# doctor:allow` with a reason, leaving a
//!    paper trail).
//! 4. Extract `var <camel> = lazuli.Command[<I>, <O>]{ Name: "<f>.<c>", ... }`
//!    from the codegen, matching the `Name:` field. Not found → skip
//!    (cell A of `@correctness.migration_out_of_sync` covers IR↔codegen
//!    drift in the other direction).
//! 5. Normalise both sides by stripping a `<feature>gen.` package
//!    prefix and compare ident strings. Mismatch → finding.
//!
//! ## Severity
//!
//! Conservative `warning` here at the rule level; the doctor dispatcher
//! escalates to `error` in `strict`, `production`, and `tdd-iron-hand`
//! profiles via the existing severity-mapping machinery (mirrors
//! [`super::handler_missing_001`]). The dispatcher mapping is **not**
//! in this module's purview; it returns plain [`Finding`] values.
//!
//! ## Opt-out
//!
//! `# doctor:allow HANDLER-SIGNATURE-MISMATCH-001 — reason "..."` in
//! the `.lzi` file silences the finding for every site in that file.
//! Uses [`crate::allow_comment::file_contains_doctor_allow`].
//!
//! Reference: docs/proposals/doctor-handler-signature-mismatch.md.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

use crate::allow_comment::file_contains_doctor_allow;
use crate::handler_path;
use crate::handler_walker::{HandlerSite, HandlerSiteKind, iter_handler_sites};

include!("handler_signature_mismatch_001_p1.rs");
include!("handler_signature_mismatch_001_p2.rs");
