//! Doctor fact collectors — the IR-to-fact projection layer.
//!
//! Each submodule pulls structured facts out of `DoctorFile` /
//! `LzxDocument` / `lazuli_ir::Feature` and writes them into the
//! `OperationalFacts` / `ExperienceFacts` / `commands` maps that the
//! diagnostic aggregators (under `doctor::aggregators::*`) consume.
//!
//! The split mirrors the input surface:
//!
//! * `canonical` — `.lzi` walks that build `OperationalFacts` (features,
//!   apis, webhooks, jobs, env references, integration requirements,
//!   external calls). Includes the IR-driven replacements for the
//!   retired `collect_external_calls_in_block` text-walker.
//! * `lzx` — `.lzx` walks that build `ExperienceFacts` (view routes,
//!   view actions) and the web/mobile route + surface side of
//!   `OperationalFacts`.
//!
//! Visibility rule: every collector is `pub(crate)` and consumed by
//! `doctor/package.rs` (the production load path) plus
//! `doctor/tests.rs` (the regression harness). They never touch
//! `DoctorPackage` directly — they receive an already-loaded
//! `DoctorFile` and mutate the destination facts map in place.
//!
//! Extracted from `doctor/mod.rs` in rails-style R5-retry-9.

pub(crate) mod canonical;
pub(crate) mod lzx;
