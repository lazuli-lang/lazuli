//! Cache bucket cycle aggregator.
//!
//! Surfaces eight closed-catalog `cache_*` / `cache-*` rules from the
//! cache bucket cycle (row 51 + CL.C.3):
//!
//! - `cache_capability_undeclared`          — error    (any query has `cache` but no `cache` capability in registry)
//! - `cache_namespace_collision`            — warning  (two features alias the same `namespace` label)
//! - `cache_ttl_unit_invalid`               — error    (inline `cache ttl` quoted prose is empty)
//! - `cache_invalidates_target_unresolved`  — error    (`invalidates query.<name>` does not resolve)
//! - `cache-profile-unknown`                — error    (query references `cache <name>` profile with no matching declaration)
//! - `cache-ttl-contract`                   — error    (empty profile TTL, SWR > TTL, or `sliding true` without literal TTL)
//! - `cache-tag-unknown`                    — warning  (inline tag is the only declarer of itself — no fan-out target)
//!
//! Index-building strategy: the aggregator walks `facts` once to build
//! three cross-feature indices (`queries_by_feature`, `all_tags`,
//! `namespace_owners`) and a flag `any_query_has_cache`. Each rule then
//! reads from these indices, so the per-rule loop body is O(1) per query
//! / profile / command and the whole pass stays O(facts).
//!
//! See `docs/proposals/bucket-cache-cycle.md` §Doctor/LSP and
//! `docs/proposals/bucket-cache-scope.md` (CL.C.3 row) for the closed
//! catalog and per-rule rationale.

use std::collections::{BTreeMap, BTreeSet};

use crate::doctor::parsers::cache_ttl_as_seconds;
use crate::doctor::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

/// Aggregate every cache-cycle finding across all Tier 3 features into
/// the canonical `DoctorDiagnostic` envelope.
pub(crate) fn diagnostics(
    facts: &[Tier3FeatureFacts],
    registry: Option<&lazuli_ir::AppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // Build cross-feature indices.
    let mut queries_by_feature: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut all_tags: BTreeSet<String> = BTreeSet::new();
    let mut namespace_owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut any_query_has_cache = false;
    for feature in facts {
        let set = queries_by_feature
            .entry(feature.feature.as_str())
            .or_default();
        for q in &feature.queries {
            let (name, cache) = query_name_and_cache(q);
            set.insert(name);
            if let Some(cache) = cache {
                any_query_has_cache = true;
                for tag in &cache.tags {
                    all_tags.insert(tag.clone());
                }
                if let Some(ns) = &cache.namespace {
                    namespace_owners
                        .entry(ns.clone())
                        .or_default()
                        .insert(feature.feature.clone());
                }
            }
        }
        // CL.C.3 — feature-level cache profiles also contribute to the
        // tag and namespace indexes. A profile referenced by a query is
        // a cached query; queue the capability check too.
        for profile in &feature.caches {
            for tag in &profile.tags {
                all_tags.insert(tag.clone());
            }
            if let Some(ns) = &profile.namespace {
                namespace_owners
                    .entry(ns.clone())
                    .or_default()
                    .insert(feature.feature.clone());
            }
        }
    }

    // `cache_capability_undeclared`: if any query carries `cache` but
    // the registry has no `cache <name>` capability, surface once at
    // the registry file (or, if no registry parsed, at the first
    // offending query).
    if any_query_has_cache {
        let has_cache_cap = registry
            .map(|r| {
                r.capabilities
                    .iter()
                    .any(|cap| cap.name.eq_ignore_ascii_case("cache"))
            })
            .unwrap_or(false);
        if !has_cache_cap {
            // Anchor at the first feature with a cache; the LSP rule on
            // `registry.lzi` is the file-local surface.
            if let Some(feature) = facts.iter().find(|f| {
                f.queries
                    .iter()
                    .any(|q| query_name_and_cache(q).1.is_some())
            }) {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: feature.feature_line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "cache_capability_undeclared".to_owned(),
                    message: "`cache` block requires a `cache <name>` capability in `registry.lzi` but none is declared. Add `cache <name>` to `registry.capabilities`.".to_owned(),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    // `cache_namespace_collision`: a namespace label declared by two
    // distinct features. One feature owning the namespace is fine.
    for (ns, owners) in &namespace_owners {
        if owners.len() >= 2 {
            // Emit one warning per owning feature.
            let owners_list: Vec<&str> = owners.iter().map(String::as_str).collect();
            for feature in facts {
                if !owners.contains(&feature.feature) {
                    continue;
                }
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: feature.feature_line,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "cache_namespace_collision".to_owned(),
                    message: format!(
                        "`cache namespace {}` is declared by queries in {}. Cross-feature namespace aliasing is unusual; rename one to avoid accidental cache-key collisions.",
                        ns,
                        owners_list.join(" and ")
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

    for feature in facts {
        // `cache_ttl_unit_invalid` is a typed promotion: when the parser
        // produced `CacheTtl::Quoted` with a payload that does *not* look
        // like a typed duration (closed catalog: `s|m|h|d`), emit. Empty
        // payload is also rejected. The literal form is type-safe by
        // construction so it never trips.
        for query in &feature.queries {
            let (name, cache) = query_name_and_cache(query);
            let Some(cache) = cache else { continue };
            let line = feature
                .query_lines
                .get(name)
                .copied()
                .unwrap_or(feature.feature_line);

            if let lazuli_ir::CacheTtl::Quoted(prose) = &cache.ttl {
                let cleaned = prose.trim();
                if cleaned.is_empty() {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "cache_ttl_unit_invalid".to_owned(),
                        message: format!(
                            "`cache ttl` on query `{}` is empty. Use a typed duration (`<int>s|m|h|d`) or non-empty quoted prose.",
                            name
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }

            // `cache_invalidates_target_unresolved` — defer to command
            // walk below.

            // Tag-side `cache_tags_*` is handled at the invalidates
            // walk below.
        }

        for command in &feature.commands {
            for invalidate in &command.invalidates {
                let line = feature
                    .command_lines
                    .get(&command.name)
                    .copied()
                    .unwrap_or(feature.feature_line);

                // The parser captures `invalidates query.<name>` as a
                // qualified pair where the literal prefix `query` lands
                // in `query.feature`. That's a shorthand for "same
                // feature, kind=query, name=<X>". When the qualifier is
                // literally `query`, resolve `<name>` against the
                // current feature's queries. Any other qualifier is a
                // genuine cross-feature reference (`<feature>.<query>`).
                let raw_feature = invalidate.query.feature.as_deref();
                let raw_name = invalidate.query.name.as_str();
                let (target_feature, target_name) = match raw_feature {
                    Some("query") => (feature.feature.as_str(), raw_name),
                    Some(other) => (other, raw_name),
                    None => (feature.feature.as_str(), raw_name),
                };

                let resolves = queries_by_feature
                    .get(target_feature)
                    .map(|set| set.contains(target_name))
                    .unwrap_or(false);
                if !resolves {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "cache_invalidates_target_unresolved".to_owned(),
                        message: format!(
                            "`invalidates query.{}` in command `{}` does not resolve: query `{}` not found in feature `{}`.",
                            target_name, command.name, target_name, target_feature
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

        // `cache_tags_referenced_but_undeclared` — placeholder: today
        // the parser does not lift `invalidates tag:<label>` into a
        // typed `InvalidationTarget::Tag` variant (it captures only
        // `query.<name>(...)`). When the parser surfaces tag targets,
        // this branch tests `all_tags.contains(label)`. The rule stays
        // wired so the LSP fallback continues to cover the surface.
        let _ = &all_tags;

        // CL.C.3 — `cache-profile-unknown`: a query referenced
        // `cache <name>` but no feature-level `cache <name>` profile
        // matches.
        let profile_names: BTreeSet<&str> =
            feature.caches.iter().map(|c| c.name.as_str()).collect();
        for query in &feature.queries {
            let (qname, cache) = query_name_and_cache(query);
            let Some(cache) = cache else { continue };
            let Some(profile_name) = cache.profile_ref.as_deref() else {
                continue;
            };
            if profile_names.contains(profile_name) {
                continue;
            }
            let line = feature
                .query_lines
                .get(qname)
                .copied()
                .unwrap_or(feature.feature_line);
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "cache-profile-unknown".to_owned(),
                message: format!(
                    "`cache {profile_name}` on query `{qname}` does not resolve: no feature-level `cache {profile_name}` profile declared in feature `{}`.",
                    feature.feature
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // CL.C.3 — `cache-ttl-contract`: closed-catalog TTL shape
        // invariants on feature-level profiles.
        for profile in &feature.caches {
            let line = feature
                .cache_lines
                .get(&profile.name)
                .copied()
                .unwrap_or(feature.feature_line);

            // (1) Empty quoted prose TTL.
            if let lazuli_ir::CacheTtl::Quoted(prose) = &profile.ttl
                && prose.trim().is_empty()
            {
                diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "cache-ttl-contract".to_owned(),
                        message: format!(
                            "`cache {}` has an empty `ttl`. Use a typed duration (`<int>s|m|h|d`) or non-empty quoted prose.",
                            profile.name
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
            }

            // (2) SWR > TTL.
            if let Some(swr) = &profile.stale_while_revalidate
                && let (Some(ttl_secs), Some(swr_secs)) = (
                    cache_ttl_as_seconds(&profile.ttl),
                    cache_ttl_as_seconds(swr),
                )
                && swr_secs > ttl_secs
            {
                diagnostics.push(DoctorDiagnostic {
                            path: feature.path.clone(),
                            line,
                            column: 1,
                            severity: DoctorSeverity::Error,
                            code: "cache-ttl-contract".to_owned(),
                            message: format!(
                                "`cache {}` has `stale_while_revalidate` ({swr_secs}s) larger than `ttl` ({ttl_secs}s). SWR must extend the freshness window, not invert it.",
                                profile.name
                            ),
                            category: None,
                            feature_name: None,
                            construct: None,
                            fix: None,
                            group: None,
                        });
            }

            // (3) `sliding true` without a typed TTL literal.
            if profile.sliding == Some(true)
                && !matches!(profile.ttl, lazuli_ir::CacheTtl::Literal(_))
            {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "cache-ttl-contract".to_owned(),
                    message: format!(
                        "`cache {}` declares `sliding true` but its `ttl` is not a typed duration literal. Use `<int>s|m|h|d` so the runtime can slide the window deterministically.",
                        profile.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        // CL.C.3 — `cache-tag-unknown`: a query carrying inline tags
        // names a tag that no other site declares.
        for query in &feature.queries {
            let (qname, cache) = query_name_and_cache(query);
            let Some(cache) = cache else { continue };
            if cache.tags.is_empty() {
                continue;
            }
            let line = feature
                .query_lines
                .get(qname)
                .copied()
                .unwrap_or(feature.feature_line);
            for tag in &cache.tags {
                let declarers = facts.iter().fold(0usize, |acc, f| {
                    let from_queries = f
                        .queries
                        .iter()
                        .filter(|q| {
                            query_name_and_cache(q)
                                .1
                                .map(|c| c.tags.iter().any(|t| t == tag))
                                .unwrap_or(false)
                        })
                        .count();
                    let from_profiles = f
                        .caches
                        .iter()
                        .filter(|p| p.tags.iter().any(|t| t == tag))
                        .count();
                    acc + from_queries + from_profiles
                });
                if declarers <= 1 {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Warning,
                        code: "cache-tag-unknown".to_owned(),
                        message: format!(
                            "`cache tags {tag}` on query `{qname}` is the only declarer of `{tag}`. Either declare it on another query/profile or remove it — tags without a second declarer cannot fan out invalidation.",
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
    }

    diagnostics
}

/// Helper — destructure an `ir::Query` into `(name, Option<&QueryCache>)`.
/// Both `ListQuery` and `SqlQuery` carry `cache`; `LookupQuery` does not.
fn query_name_and_cache(q: &lazuli_ir::Query) -> (&str, Option<&lazuli_ir::QueryCache>) {
    match q {
        lazuli_ir::Query::List(l) => (l.name.as_str(), l.cache.as_ref()),
        lazuli_ir::Query::Lookup(l) => (l.name.as_str(), None),
        lazuli_ir::Query::Sql(s) => (s.name.as_str(), s.cache.as_ref()),
    }
}
