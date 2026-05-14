//! `VECTOR-*` doctor codes for the vectorstore bucket cycle.
//!
//! V4 of `docs/proposals/bucket-vectorstore-cycle.md`. Four rules:
//!
//! - `VECTOR-REGISTRY-001` (error)   — VectorStore bound without `dimension` or Embedder.
//! - `VECTOR-REGISTRY-002` (error)   — feature requires VectorStore but slot unresolved.
//! - `VECTOR-DIM-001`      (warning) — dimension mismatch across ≥2 declaration sources.
//! - `VECTOR-RAW-001`      (warn/error) — `VectorFilter.Raw` escape hatch in handler files.
//!
//! Each rule is a free `check_*` function returning `Vec<Finding>`.
//! The orchestrator wires these into `DoctorPackage::diagnostics()` post-merge.

use std::path::{Path, PathBuf};

use lazuli_ir::{AppRegistry, FeatureRequirement};

// ── shared result types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub line: usize,
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

// ── VECTOR-REGISTRY-001 ───────────────────────────────────────────────────────

/// Checks that every `vector_store: VectorStore` integration in the registry
/// has either an explicit `dimension <int>` field in the registry source or a
/// bound `Embedder` integration (from which dimension can be inferred at runtime).
///
/// Without one of these anchors the framework cannot validate dimension
/// consistency at design time; a production write to the wrong collection
/// dimension will fail silently at the provider level.
pub fn check_registry_001(
    registry: &AppRegistry,
    registry_source: &str,
    registry_path: &Path,
) -> Vec<Finding> {
    let has_vector_store = registry
        .integrations
        .iter()
        .any(|i| i.kind == "VectorStore");

    if !has_vector_store {
        return vec![];
    }

    let has_embedder = registry
        .integrations
        .iter()
        .any(|i| i.kind == "Embedder");

    let has_dimension = source_declares_dimension(registry_source);

    if has_embedder || has_dimension {
        return vec![];
    }

    let anchor = find_line(registry_source, |l| {
        let t = l.trim();
        t.contains("VectorStore") && t.contains(':')
    })
    .unwrap_or(1);

    vec![Finding {
        path: registry_path.to_path_buf(),
        line: anchor,
        code: "VECTOR-REGISTRY-001",
        severity: Severity::Error,
        message: "vector_store binding requires either `dimension <int>` or a bound \
                  Embedder integration — without one, collection dimension cannot be \
                  validated at design time. Add `dimension 1536` (or the appropriate \
                  value) to the integration block, or bind an `embedder: Embedder`."
            .to_owned(),
    }]
}

// ── VECTOR-REGISTRY-002 ───────────────────────────────────────────────────────

/// Checks that every `requires integration <slot>: VectorStore` declared in a
/// feature is resolved by a matching `<slot>: VectorStore` entry in the registry.
///
/// `feature_reqs` pairs each feature's source path with its requirement slice.
pub fn check_registry_002(
    registry: Option<&AppRegistry>,
    feature_reqs: &[(PathBuf, Vec<FeatureRequirement>)],
) -> Vec<Finding> {
    let mut findings = Vec::new();

    for (path, reqs) in feature_reqs {
        for req in reqs {
            if req.kind != "integration" || req.contract != "VectorStore" {
                continue;
            }
            let resolved = registry
                .map(|reg| {
                    reg.integrations
                        .iter()
                        .any(|i| i.name == req.name && i.kind == "VectorStore")
                })
                .unwrap_or(false);

            if !resolved {
                findings.push(Finding {
                    path: path.clone(),
                    line: 1,
                    code: "VECTOR-REGISTRY-002",
                    severity: Severity::Error,
                    message: format!(
                        "feature requires `integration {slot}: VectorStore` but no \
                         matching binding exists in the app registry — add \
                         `{slot}: VectorStore` under `registry.integrations`.",
                        slot = req.name,
                    ),
                });
            }
        }
    }

    findings
}

// ── VECTOR-DIM-001 ────────────────────────────────────────────────────────────

/// Warns when the `dimension` declared in the registry integration block
/// disagrees with the dimension declared in `lazurite.toml` (under any
/// `[plugins.*]` section, key `dimension = <int>`).
///
/// Fires only when ≥2 sources are present and they disagree. The registry
/// block takes precedence (source-of-truth order from the proposal).
pub fn check_dim_001(
    registry_source: &str,
    manifest_toml: Option<&str>,
    registry_path: &Path,
) -> Vec<Finding> {
    let registry_dim = extract_registry_dimension(registry_source);
    let toml_dim = manifest_toml.and_then(extract_toml_vectorstore_dimension);

    let (Some(rd), Some(td)) = (registry_dim, toml_dim) else {
        return vec![];
    };

    if rd == td {
        return vec![];
    }

    let anchor = find_line(registry_source, |l| {
        l.trim()
            .strip_prefix("dimension ")
            .is_some_and(|r| r.trim().parse::<u32>().is_ok())
    })
    .unwrap_or(1);

    vec![Finding {
        path: registry_path.to_path_buf(),
        line: anchor,
        code: "VECTOR-DIM-001",
        severity: Severity::Warning,
        message: format!(
            "vector_store dimension mismatch: registry declares {rd}, \
             lazurite.toml declares {td}. \
             The registry `dimension <int>` takes precedence at doctor time; \
             align lazurite.toml or remove the duplicate to suppress this warning."
        ),
    }]
}

// ── VECTOR-RAW-001 ────────────────────────────────────────────────────────────

/// Walks hand-written `.go` handler files under `features/` for `VectorFilter`
/// usages that populate the `Raw` escape-hatch field.
///
/// `is_production` upgrades severity from warning to error — production
/// codebases should prefer the typed filter surface (`Tags`, `Equals`) over
/// provider-specific raw maps.
pub fn check_raw_001(project_root: &Path, is_production: bool) -> Vec<Finding> {
    let mut findings = Vec::new();
    let features_root = project_root.join("features");
    if !features_root.exists() {
        return findings;
    }
    collect_raw_findings(&features_root, is_production, &mut findings);
    findings
}

// ── shared helpers ────────────────────────────────────────────────────────────

/// Returns `true` if the source text has a line matching `dimension <uint>`.
fn source_declares_dimension(source: &str) -> bool {
    source.lines().any(|l| {
        if let Some(rest) = l.trim().strip_prefix("dimension ") {
            rest.trim().parse::<u32>().is_ok()
        } else {
            false
        }
    })
}

/// Extracts the numeric value of the first `dimension <uint>` line found.
fn extract_registry_dimension(source: &str) -> Option<u32> {
    source.lines().find_map(|l| {
        l.trim()
            .strip_prefix("dimension ")
            .and_then(|r| r.trim().parse::<u32>().ok())
    })
}

/// Scans a raw TOML text for `dimension = <uint>` under any `[plugins.*]`
/// section header. Returns the first match found.
fn extract_toml_vectorstore_dimension(toml: &str) -> Option<u32> {
    let mut in_plugin_section = false;
    for line in toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[plugins") {
            in_plugin_section = true;
            continue;
        }
        if in_plugin_section && trimmed.starts_with('[') {
            in_plugin_section = false;
        }
        if in_plugin_section {
            if let Some(rest) = trimmed.strip_prefix("dimension = ") {
                if let Ok(n) = rest.trim().parse::<u32>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// Returns the 1-based line number of the first line matching `predicate`.
fn find_line(source: &str, predicate: impl Fn(&str) -> bool) -> Option<usize> {
    source
        .lines()
        .enumerate()
        .find_map(|(i, l)| if predicate(l) { Some(i + 1) } else { None })
}

/// Recursive directory walk for `VectorFilter.Raw` call sites.
fn collect_raw_findings(dir: &Path, is_production: bool, out: &mut Vec<Finding>) {
    use std::fs;
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_raw_findings(&path, is_production, out);
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if !name.ends_with(".go") || name.ends_with(".gen.go") || name.ends_with("_test.go") {
            continue;
        }
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        scan_source_for_raw_filter(&source, &path, is_production, out);
    }
}

/// Scans a Go source string for `VectorFilter` struct literals that set `Raw:`.
///
/// Detection window: within 6 lines of a `VectorFilter` occurrence. This
/// covers both single-line (`VectorFilter{Raw: ...}`) and multi-line literals.
///
/// Exposed as `pub(crate)` so inline tests can call it without filesystem I/O.
pub(crate) fn scan_source_for_raw_filter(
    source: &str,
    path: &Path,
    is_production: bool,
    out: &mut Vec<Finding>,
) {
    const WINDOW: usize = 6;
    let lines: Vec<&str> = source.lines().collect();
    let severity = if is_production {
        Severity::Error
    } else {
        Severity::Warning
    };

    let mut i = 0;
    while i < lines.len() {
        if lines[i].contains("VectorFilter") {
            let end = (i + WINDOW).min(lines.len());
            for j in i..end {
                if lines[j].contains("Raw:") {
                    out.push(Finding {
                        path: path.to_path_buf(),
                        line: j + 1,
                        code: "VECTOR-RAW-001",
                        severity,
                        message: format!(
                            "`VectorFilter.Raw` escape hatch used at line {} — \
                             Raw maps are provider-specific and bypass the closed \
                             Lazuli filter surface. Replace with typed alternatives \
                             (`Tags`, `Equals`) to stay portable across adapters. \
                             If Raw is intentional, confirm the adapter supports \
                             the key shape via its adapter manifest.",
                            j + 1,
                        ),
                    });
                    break; // one finding per VectorFilter site
                }
            }
        }
        i += 1;
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{AppIntegration, AppRegistry};

    fn mk_integration(name: &str, kind: &str) -> AppIntegration {
        AppIntegration {
            name: name.into(),
            kind: kind.into(),
            adapter: None,
            adapter_provenance: None,
            environments: vec![],
            credentials: None,
            data_classification: None,
        }
    }

    fn mk_registry(integrations: Vec<AppIntegration>) -> AppRegistry {
        AppRegistry {
            env: vec![],
            integrations,
            capabilities: vec![],
            packs: vec![],
            tools: vec![],
            webhook_events: vec![],
        }
    }

    fn mk_req(kind: &str, name: &str, contract: &str) -> FeatureRequirement {
        FeatureRequirement {
            kind: kind.into(),
            name: name.into(),
            contract: contract.into(),
        }
    }

    // ── VECTOR-REGISTRY-001 ──────────────────────────────────────────────────

    #[test]
    fn registry_001_fires_when_no_dimension_and_no_embedder() {
        let registry = mk_registry(vec![mk_integration("vector_store", "VectorStore")]);
        let source = "registry\n  integrations\n    vector_store: VectorStore\n";
        let findings = check_registry_001(&registry, source, Path::new("registry.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "VECTOR-REGISTRY-001");
        assert_eq!(findings[0].severity, Severity::Error);
    }

    #[test]
    fn registry_001_silent_when_dimension_declared() {
        let registry = mk_registry(vec![mk_integration("vector_store", "VectorStore")]);
        let source =
            "registry\n  integrations\n    vector_store: VectorStore\n      dimension 1536\n";
        let findings = check_registry_001(&registry, source, Path::new("registry.lzi"));
        assert!(
            findings.is_empty(),
            "should not fire when dimension is declared; got {findings:?}"
        );
    }

    #[test]
    fn registry_001_silent_when_embedder_bound() {
        let registry = mk_registry(vec![
            mk_integration("vector_store", "VectorStore"),
            mk_integration("embedder", "Embedder"),
        ]);
        let source = "registry\n  integrations\n    vector_store: VectorStore\n    embedder: Embedder\n";
        let findings = check_registry_001(&registry, source, Path::new("registry.lzi"));
        assert!(
            findings.is_empty(),
            "should not fire when Embedder is bound; got {findings:?}"
        );
    }

    #[test]
    fn registry_001_silent_when_no_vector_store_bound() {
        let registry = mk_registry(vec![mk_integration("payment_gateway", "PaymentGateway")]);
        let source = "registry\n  integrations\n    payment_gateway: PaymentGateway\n";
        let findings = check_registry_001(&registry, source, Path::new("registry.lzi"));
        assert!(
            findings.is_empty(),
            "should not fire when no VectorStore integration; got {findings:?}"
        );
    }

    // ── VECTOR-REGISTRY-002 ──────────────────────────────────────────────────

    #[test]
    fn registry_002_fires_when_slot_unresolved() {
        let registry = mk_registry(vec![]);
        let reqs = vec![(
            PathBuf::from("features/search/search.lzi"),
            vec![mk_req("integration", "vector_store", "VectorStore")],
        )];
        let findings = check_registry_002(Some(&registry), &reqs);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "VECTOR-REGISTRY-002");
        assert!(findings[0].message.contains("vector_store"));
    }

    #[test]
    fn registry_002_silent_when_slot_resolved() {
        let registry = mk_registry(vec![mk_integration("vector_store", "VectorStore")]);
        let reqs = vec![(
            PathBuf::from("features/search/search.lzi"),
            vec![mk_req("integration", "vector_store", "VectorStore")],
        )];
        let findings = check_registry_002(Some(&registry), &reqs);
        assert!(
            findings.is_empty(),
            "should not fire when slot is resolved; got {findings:?}"
        );
    }

    #[test]
    fn registry_002_silent_when_no_vector_store_requirement() {
        let registry = mk_registry(vec![]);
        let reqs = vec![(
            PathBuf::from("features/search/search.lzi"),
            vec![mk_req("integration", "payment_gw", "PaymentGateway")],
        )];
        let findings = check_registry_002(Some(&registry), &reqs);
        assert!(
            findings.is_empty(),
            "non-VectorStore requirements should be ignored; got {findings:?}"
        );
    }

    // ── VECTOR-DIM-001 ───────────────────────────────────────────────────────

    #[test]
    fn dim_001_fires_on_mismatch() {
        let registry_source =
            "registry\n  integrations\n    vector_store: VectorStore\n      dimension 1536\n";
        let manifest_toml = "[plugins.\"@plugin/chromadb\"]\ndimension = 768\n";
        let findings = check_dim_001(
            registry_source,
            Some(manifest_toml),
            Path::new("registry.lzi"),
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "VECTOR-DIM-001");
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].message.contains("1536"));
        assert!(findings[0].message.contains("768"));
    }

    #[test]
    fn dim_001_silent_when_dimensions_agree() {
        let registry_source =
            "registry\n  integrations\n    vector_store: VectorStore\n      dimension 1536\n";
        let manifest_toml = "[plugins.\"@plugin/chromadb\"]\ndimension = 1536\n";
        let findings = check_dim_001(
            registry_source,
            Some(manifest_toml),
            Path::new("registry.lzi"),
        );
        assert!(
            findings.is_empty(),
            "should not fire when dimensions agree; got {findings:?}"
        );
    }

    #[test]
    fn dim_001_silent_when_only_one_source() {
        // Only registry declares dimension; toml has none → no mismatch possible.
        let registry_source =
            "registry\n  integrations\n    vector_store: VectorStore\n      dimension 1536\n";
        let findings = check_dim_001(registry_source, None, Path::new("registry.lzi"));
        assert!(
            findings.is_empty(),
            "should not fire with only one source; got {findings:?}"
        );
    }

    // ── VECTOR-RAW-001 ───────────────────────────────────────────────────────

    #[test]
    fn raw_001_fires_on_raw_filter_usage() {
        let source = r#"package handlers

func Search(ctx context.Context, q string) ([]Match, error) {
    results, err := lazuli.Vectorstore.Collection("items").QueryByVector(ctx, emb,
        lazuli.VectorQuery{
            Limit:  20,
            Filter: lazuli.VectorFilter{Raw: map[string]any{"where": "$contains"}},
        })
    return results, err
}
"#;
        let mut out = Vec::new();
        scan_source_for_raw_filter(source, Path::new("handlers/search.go"), false, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].code, "VECTOR-RAW-001");
        assert_eq!(out[0].severity, Severity::Warning);
    }

    #[test]
    fn raw_001_fires_as_error_in_production() {
        let source = "func F() { lazuli.VectorFilter{Raw: map[string]any{}} }\n";
        let mut out = Vec::new();
        scan_source_for_raw_filter(source, Path::new("handlers/f.go"), true, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].severity, Severity::Error);
    }

    #[test]
    fn raw_001_silent_on_typed_filter() {
        let source = r#"package handlers

func Search(ctx context.Context, tags []string) ([]Match, error) {
    results, err := lazuli.Vectorstore.Collection("items").QueryByVector(ctx, emb,
        lazuli.VectorQuery{
            Limit:  20,
            Filter: lazuli.VectorFilter{Tags: tags},
        })
    return results, err
}
"#;
        let mut out = Vec::new();
        scan_source_for_raw_filter(source, Path::new("handlers/search.go"), false, &mut out);
        assert!(
            out.is_empty(),
            "typed filter should not fire VECTOR-RAW-001; got {out:?}"
        );
    }

    #[test]
    fn raw_001_silent_on_empty_vector_filter() {
        let source = "func F() { lazuli.VectorFilter{} }\n";
        let mut out = Vec::new();
        scan_source_for_raw_filter(source, Path::new("handlers/f.go"), false, &mut out);
        assert!(
            out.is_empty(),
            "empty VectorFilter should not fire; got {out:?}"
        );
    }
}
