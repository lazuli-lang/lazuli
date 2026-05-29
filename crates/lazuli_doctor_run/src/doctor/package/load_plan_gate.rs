//! Phase L PG.B — package-wide plan + gate aggregation.
//!
//! Walks every `.lzi` source one extra time at the tail of
//! `DoctorPackage::load` to collect top-level `plan` blocks and
//! per-feature `gate ...` directives, then reads the subscription
//! anchor from `app.lzi`. The output (`Option<PlanGateFacts>`) is
//! consumed by the diagnostics pass and by codegen.
//!
//! Returns `None` when the package authors no `plan` blocks, no
//! `gate` directives, and has no subscription anchor — that's the
//! happy path for a freshly scaffolded project.

use super::super::parsers::is_lzi_path;
use super::super::scanners::derive_feature_name;
use super::super::{DoctorAppManifest, DoctorFile};

pub(super) fn build_plan_gate_facts(
    files: &[DoctorFile],
    app: Option<&DoctorAppManifest>,
) -> Option<lazuli_analyzer::PlanGateFacts> {
    let mut plan_blocks_raw: Vec<lazuli_syntax::PlanBlockAst> = Vec::new();
    let mut feature_gates_raw: Vec<(String, lazuli_syntax::FeatureGatesAst)> = Vec::new();
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        if let Ok(blocks) = lazuli_syntax::parse_plan_blocks(&file.source) {
            plan_blocks_raw.extend(blocks);
        }
        if let Ok(fg) = lazuli_syntax::parse_feature_gates(&file.source) {
            if !fg.callables.is_empty() {
                // Derive feature name from the file's first
                // `feature <name>` header (mirrors the existing
                // doctor convention).
                let feature_name = derive_feature_name(&file.source).unwrap_or_else(|| {
                    file.path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_owned()
                });
                feature_gates_raw.push((feature_name, fg));
            }
        }
    }
    let anchor = app.and_then(|a| lazuli_analyzer::parse_subscription_anchor(&a.source));
    if plan_blocks_raw.is_empty() && feature_gates_raw.is_empty() && anchor.is_none() {
        None
    } else {
        Some(lazuli_analyzer::aggregate_plan_gate_facts(
            &plan_blocks_raw,
            &feature_gates_raw,
            anchor,
        ))
    }
}
