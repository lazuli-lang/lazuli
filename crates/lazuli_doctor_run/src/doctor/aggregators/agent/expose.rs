//! Cut A.7 — `agent expose http` cross-feature diagnostics.
//!
//! Walk every agent with `expose_http` plus every `api` path
//! declared in source. Two diagnostics:
//!
//! - `agent_expose_path_conflict_cross_feature_diagnostics` (error)
//!   — two facts with the same (normalised path, method) but
//!   different feature/api ids. Same-feature collisions are
//!   file-local and surface in LSP instead.
//! - `agent_expose_audience_unknown_diagnostics` (error) — an agent's
//!   `expose http audience <x>` references an audience no `.lzx`
//!   surface or `app.lzi` audience declaration knows.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::doctor::parsers::{http_method_word, normalise_path};
use crate::doctor::{AgentFacts, DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

/// Walk every agent with `expose_http` plus every `api` path
/// declared in source. Reject cross-feature collisions on (normalised
/// path, method) and `audience` references that don't resolve to any
/// known `.lzx` surface or `app.lzi` audience declaration.
pub(crate) fn agent_expose_diagnostics(
    agents: &[AgentFacts],
    tier3_facts: &[Tier3FeatureFacts],
    known_audiences: &BTreeSet<String>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // Collect every (method, normalized_path) pair from agent expose
    // blocks + every api block, anchored to their source location.
    let mut pairs: Vec<ExposePathFact> = Vec::new();
    for fact in agents {
        let Some(expose) = fact.agent.expose_http.as_ref() else {
            continue;
        };
        pairs.push(ExposePathFact {
            path_normalised: normalise_path(&expose.path),
            path_raw: expose.path.clone(),
            method: http_method_word(expose.method).to_owned(),
            origin: format!("agent {}.{}", fact.feature, fact.agent.name),
            owner_path: fact.path.clone(),
            line: fact.line,
        });
    }
    // Phase L Tier 4b — read `Api` declarations from `Tier3FeatureFacts`
    // (IR), retiring the `ApiPathFact` text-walker.
    for feature in tier3_facts {
        for api in &feature.apis {
            let line = feature
                .api_lines
                .get(&api.name)
                .copied()
                .unwrap_or(feature.feature_line);
            pairs.push(ExposePathFact {
                path_normalised: normalise_path(&api.path),
                path_raw: api.path.clone(),
                method: http_method_word(api.method).to_owned(),
                origin: format!("api {}.{}", feature.feature, api.name),
                owner_path: feature.path.clone(),
                line,
            });
        }
    }

    // Cross-feature path collision detection. Two facts collide when
    // they share (normalized_path, method) but originate from
    // different feature/api ids — same feature/agent collisions are
    // file-local and surface in LSP instead.
    for (i, a) in pairs.iter().enumerate() {
        for b in pairs.iter().skip(i + 1) {
            if a.path_normalised == b.path_normalised
                && a.method == b.method
                && a.origin != b.origin
            {
                diagnostics.push(DoctorDiagnostic {
                    path: a.owner_path.clone(),
                    line: a.line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "agent_expose_path_conflict_cross_feature_diagnostics".to_owned(),
                    message: format!(
                        "{origin_a} declares HTTP path `{path}` ({method}) that conflicts with {origin_b}; same method+path must originate from a single feature.",
                        origin_a = a.origin,
                        origin_b = b.origin,
                        path = a.path_raw,
                        method = a.method,
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    // Audience reachability check.
    for fact in agents {
        let Some(expose) = fact.agent.expose_http.as_ref() else {
            continue;
        };
        let Some(audience) = expose.audience.as_ref() else {
            continue;
        };
        if !known_audiences.contains(audience) {
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "agent_expose_audience_unknown_diagnostics".to_owned(),
                message: format!(
                    "agent `{}` declares `expose http audience {audience}`, but no `.lzx` surface or `app.lzi` audience declares it.",
                    fact.agent.name,
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    diagnostics
}

#[derive(Debug, Clone)]
struct ExposePathFact {
    path_normalised: String,
    path_raw: String,
    method: String,
    origin: String,
    owner_path: PathBuf,
    line: usize,
}
