//! `.lzx` ViewModel surface Doctor rules (`lzx-*`).
//!
//! Per `docs/proposals/lzx-integration-codegen.md` §11, this sub-namespace
//! hosts the seven `lzx-*` rules that validate the per-feature `.lzx`
//! ViewModel surface (audiences, views, sources, columns, fields, route
//! params, cell slot bindings, actions):
//!
//! E.1 — source / action consistency:
//! - [`source_resource_mismatch`] — `columns`/`search`/`filter` field not
//!   on the resource backing `source`.
//! - [`command_input_mismatch`] — `view create.fields` references a field
//!   not in the command's input.
//! - [`action_not_in_audience`] — `actions` references a command whose
//!   effective policy isn't reachable from the audience's `requires`.
//!
//! E.2 — route / slot / audience consistency:
//! - [`route_param_missing_binding`] — `at "/path/:slug"` placeholder has
//!   no `route slug: <Type> from path` declaration.
//! - [`route_param_orphan`] — `route X: Type from path` declared but no
//!   `:X` in the path string.
//! - [`cell_slot_orphan`] — `cells <field> @client.<slot>` references a
//!   field NOT in `columns`/`fields`/`sections`.
//! - [`cells_mixed_form`] — `view list` declares both grid-row
//!   `cells @client.<slot>` and per-column `cells <field> @client.<slot>`.
//! - [`audience_empty_sdk`] — audience's `requires` set produces an
//!   empty command/query intersection.
//!
//! L0 #6 Terminal grammar:
//! - [`cells_or_columns`] — `view list` declares exactly one row rendering
//!   form: `columns <field>, ...` or grid-form `cells @client.<slot>`.
//!
//! ## IR stub
//!
//! The richer `.lzx` IR shape (Audience, ViewList, ViewDetail, ViewCreate,
//! QueryRef, CommandRef, CellBinding, RouteParam, PolicyAtom) called for
//! by L0 #3 §6 is **not yet on main** — L2 cells A.1/A.2/A.3 will land it
//! in `lazuli_ir`. In the meantime, this module defines those types under
//! [`ir_stub`] so the rules and their tests can compile and exercise the
//! full check logic. When the L2 cells land, the orchestrator can either:
//!
//! 1. Re-export from `lazuli_ir` and delete `ir_stub`, or
//! 2. Map `lazuli_ir` shapes into `ir_stub` types via a thin adapter.
//!
//! The stub mirrors the shape from `docs/proposals/lzx-integration-codegen.md`
//! §5 (16-keyword catalog) and §6 (per-view emission shapes).

pub mod ir_stub;

pub mod action_not_in_audience;
pub mod audience_empty_sdk;
pub mod cells_or_columns;
pub mod cell_slot_orphan;
pub mod cells_mixed_form;
pub mod command_input_mismatch;
pub mod drawer_source;
pub mod filter_resolves;
pub mod route_param_missing_binding;
pub mod route_param_orphan;
pub mod source_resource_mismatch;

use ir_stub::{Audience, Command, Feature, QueryRef, Resource};

// =============================================================================
// Shared helpers
// =============================================================================

/// Resolve a `source <feature>.query.<name>` reference back to the resource it
/// backs. Walks the feature's queries to find one whose `backs_resource`
/// matches, then looks up that resource. Returns `None` if the query is
/// unknown OR the resource it backs is not declared on the feature.
///
/// Rationale: `lzx-source-resource-mismatch` (and the source-aware path of
/// `lzx-cell-slot-orphan`) need the column-bearing resource shape; the
/// connection is `View.source.query -> Query.backs_resource -> Resource.name`.
pub fn find_resource_for_query<'a>(
    feature: &'a Feature,
    query_ref: &QueryRef,
) -> Option<&'a Resource> {
    let query = feature.queries.iter().find(|q| q.name == query_ref.name)?;
    let resource_name = query.backs_resource.as_deref()?;
    feature.resources.iter().find(|r| r.name == resource_name)
}

/// Extract `:<name>` placeholders from a route path string. Strips the leading
/// `:` and returns names in declaration order, deduplicated to first occurrence
/// (later duplicates preserved for symmetric checks).
///
/// `extract_path_placeholders("/slugs/:org/:slug/edit")` → `["org", "slug"]`.
///
/// Rationale: `lzx-route-param-missing-binding` and `lzx-route-param-orphan`
/// both pivot on the placeholder set vs the `route ... from path` set.
pub fn extract_path_placeholders(route: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for segment in route.split('/') {
        if let Some(rest) = segment.strip_prefix(':') {
            // Stop at the first non-identifier char (covers e.g. `:slug?`
            // optional markers, `:slug.json` extensions; we keep only the
            // identifier portion).
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() && !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// Intersect an audience's `requires` set with each command's effective policy
/// atoms. A command is "reachable" from the audience iff at least one of its
/// effective policy atoms matches one of the audience's required scopes
/// (per `docs/proposals/lzx-integration-codegen.md` §7.2: multiple `requires`
/// use OR).
///
/// Commands whose effective policy is empty (`policy_atoms.is_empty()`) are
/// treated as **public** — reachable from any audience. This matches the
/// runtime behavior where an unrestricted command is callable by any caller.
///
/// Rationale: `lzx-action-not-in-audience` calls this to verify each entry in
/// `view.actions` is reachable; `lzx-audience-empty-sdk` calls it to detect
/// dead audiences.
pub fn audience_reachable_commands<'a>(
    audience: &Audience,
    feature: &'a Feature,
) -> Vec<&'a Command> {
    feature
        .commands
        .iter()
        .filter(|cmd| {
            if cmd.policy_atoms.is_empty() {
                // Public command — reachable from any audience.
                return true;
            }
            cmd.policy_atoms
                .iter()
                .any(|atom| audience.requires.iter().any(|req| req == atom))
        })
        .collect()
}

/// Sort findings deterministically. The CLI emits findings in this order so
/// snapshot tests stay stable across HashMap/HashSet iteration orders. Tuple
/// shape: `(feature_name, view_name, rule_code, line)`.
///
/// Each `Finding` type in this namespace exposes `feature`, `view`, `line`,
/// plus a `CODE` const — the helper takes a `code` accessor that callers
/// supply (typically `|_| T::CODE`).
pub fn sort_findings<F, K>(findings: &mut [F], key: K)
where
    K: Fn(&F) -> (String, String, &'static str, usize),
{
    findings.sort_by(|a, b| {
        let ka = key(a);
        let kb = key(b);
        ka.cmp(&kb)
    });
}

// =============================================================================
// Shared-helper tests
// =============================================================================

#[cfg(test)]
mod helper_tests {
    use super::ir_stub::*;
    use super::*;

    fn feat() -> Feature {
        Feature {
            name: "slug".into(),
            resources: vec![
                Resource {
                    name: "Slug".into(),
                    id_type: "ID".into(),
                    fields: vec!["key".into(), "title".into(), "tags".into()],
                },
                Resource {
                    name: "Other".into(),
                    id_type: "ID".into(),
                    fields: vec!["id".into()],
                },
            ],
            queries: vec![
                ListQuery {
                    name: "mine".into(),
                    input_slots: vec![],
                    backs_resource: Some("Slug".into()),
                },
                ListQuery {
                    name: "orphaned".into(),
                    input_slots: vec![],
                    backs_resource: None,
                },
            ],
            commands: vec![
                Command {
                    name: "create".into(),
                    input_fields: vec!["key".into(), "title".into()],
                    policy_atoms: vec!["@scope.admin".into()],
                },
                Command {
                    name: "public_ping".into(),
                    input_fields: vec![],
                    policy_atoms: vec![],
                },
            ],
            surfaces: vec![],
        }
    }

    // ── find_resource_for_query ──────────────────────────────────────────────

    #[test]
    fn helper_find_resource_resolves_via_backs() {
        let f = feat();
        let qr = QueryRef {
            feature: None,
            name: "mine".into(),
        };
        let r = find_resource_for_query(&f, &qr).expect("must resolve");
        assert_eq!(r.name, "Slug");
    }

    #[test]
    fn helper_find_resource_returns_none_for_unknown_query() {
        let f = feat();
        let qr = QueryRef {
            feature: None,
            name: "ghost".into(),
        };
        assert!(find_resource_for_query(&f, &qr).is_none());
    }

    #[test]
    fn helper_find_resource_returns_none_when_query_has_no_backing() {
        let f = feat();
        let qr = QueryRef {
            feature: None,
            name: "orphaned".into(),
        };
        assert!(find_resource_for_query(&f, &qr).is_none());
    }

    // ── extract_path_placeholders ────────────────────────────────────────────

    #[test]
    fn helper_extract_placeholders_basic() {
        let p = extract_path_placeholders("/slugs/:key");
        assert_eq!(p, vec!["key".to_string()]);
    }

    #[test]
    fn helper_extract_placeholders_multiple() {
        let p = extract_path_placeholders("/orgs/:org/slugs/:slug");
        assert_eq!(p, vec!["org".to_string(), "slug".to_string()]);
    }

    #[test]
    fn helper_extract_placeholders_dedupes() {
        let p = extract_path_placeholders("/:x/sub/:x");
        assert_eq!(p, vec!["x".to_string()]);
    }

    #[test]
    fn helper_extract_placeholders_empty_when_static() {
        let p = extract_path_placeholders("/about/us");
        assert!(p.is_empty());
    }

    #[test]
    fn helper_extract_placeholders_strips_non_identifier_suffix() {
        // `:slug.json` should yield "slug" — only identifier chars are kept.
        let p = extract_path_placeholders("/items/:slug.json");
        assert_eq!(p, vec!["slug".to_string()]);
    }

    // ── audience_reachable_commands ──────────────────────────────────────────

    #[test]
    fn helper_reachable_includes_match() {
        let f = feat();
        let aud = Audience {
            name: "admin".into(),
            requires: vec!["@scope.admin".into()],
            views: vec![],
            line: 1,
        };
        let r = audience_reachable_commands(&aud, &f);
        let names: Vec<&str> = r.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"create"));
        assert!(names.contains(&"public_ping")); // public always reachable
    }

    #[test]
    fn helper_reachable_excludes_unmatched_scoped() {
        let f = feat();
        let aud = Audience {
            name: "public".into(),
            requires: vec!["@scope.member".into()],
            views: vec![],
            line: 1,
        };
        let r = audience_reachable_commands(&aud, &f);
        let names: Vec<&str> = r.iter().map(|c| c.name.as_str()).collect();
        assert!(
            !names.contains(&"create"),
            "scoped command must be excluded"
        );
        assert!(
            names.contains(&"public_ping"),
            "public command always included"
        );
    }

    #[test]
    fn helper_reachable_or_semantics() {
        // Multiple `requires` use OR — one match suffices.
        let f = feat();
        let aud = Audience {
            name: "mixed".into(),
            requires: vec!["@scope.other".into(), "@scope.admin".into()],
            views: vec![],
            line: 1,
        };
        let r = audience_reachable_commands(&aud, &f);
        let names: Vec<&str> = r.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"create"));
    }
}
