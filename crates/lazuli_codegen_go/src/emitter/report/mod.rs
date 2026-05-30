//! Report vocab — `<feature>/reports.gen.go` emission.
//!
//! Walks every `report <name>` declaration on a feature and emits one
//! `report.Contract` value per declaration. Per proposal §Codegen
//! worked example, the emitter is wire-thin: it imports the report +
//! storage runtime, declares the typed Contract, and emits a
//! `Run<Name>` entry point that delegates to `report.Run`.
//!
//! Determinism: reports are sorted by name; column / format ordering
//! preserves source order (the IR captures author intent).
//!
//! See `docs/proposals/report-vocab.md` v0.2.

use lazuli_ir::{
    BuiltinType, Feature, FileVisibility, PolicyAtom, PolicyExpr, PolicyRef, Report,
    ReportColumnSource, ReportFormat, ReportSource, TypeRef,
};

use super::casing::pascal_case;
use super::imports::ImportSet;
use super::patterns::{PATTERN_REPORT_AUTOMOUNT, PATTERN_REPORT_RUN, emit_pattern_header};
use super::printer::GoPrinter;

include!("mod_p1.rs");
include!("mod_p2.rs");
