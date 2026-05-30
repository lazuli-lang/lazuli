//! Per-rule bridge helpers — adapt `lazuli_doctor` / hand-rolled
//! per-rule checkers into the canonical `DoctorDiagnostic` envelope.
//!
//! Three families live here:
//!
//! 1. **Vocab + Money bridges** (`vocab_grammar_form_diagnostics`,
//!    `vocab_tests_missing_001_diagnostics`, `money_compare_001_diagnostics`,
//!    `money_arithmetic_001_diagnostics`) — wrap the per-rule finders
//!    exported by `lazuli_doctor::vocab` so the dispatcher in
//!    `doctor::dispatch` just calls one function and gets a
//!    `Vec<DoctorDiagnostic>`.
//! 2. **Framework-source self-checks** (`check_codegen_wrap_001`,
//!    `check_pattern_draft_stale_001`, `check_auth_session_callsite_001`)
//!    — walk the lazuli framework / generated runtime source for the
//!    one-wrap, draft-pattern, and auth-callsite contracts. Each is a
//!    hand-rolled checker because the rule speaks Go / Rust source,
//!    not lifted IR.
//! 3. **Shared helpers** (`is_pattern_draft_line`, `git_blame_author_time`,
//!    `collect_codegen_wrap_001`, `is_bucket_go_source`,
//!    `collect_issue_session_callsites`) — small helpers consumed only
//!    by the family-2 checkers above.
//!
//! `read_design_ir` + `doctor_rule_path` sit alongside as small utilities
//! used by aggregators when they need the lowered design IR or a
//! project-rooted display path. Both are `pub(crate)` because they are
//! called from `aggregators::*`.
//!
//! Wave R7-3 extract: lifted out of `doctor/mod.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use lazuli_doctor::{RuleCategory, vocab};
use lazuli_doctor_config::DoctorProfile as SecurityProfile;

use super::parsers::is_lzi_path;
use super::{
    AuthFacts, DoctorDiagnostic, DoctorFile, DoctorSeverity, doctor_rule_severity,
    doctor_severity_for,
};
use crate::doctor::diagnostic::DoctorSeverityOverride;

include!("rule_bridges_p1.rs");
include!("rule_bridges_p2.rs");
include!("rule_bridges_p3.rs");
