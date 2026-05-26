//! `lzx-route-collision` — two views in the same `(audience, target)`
//! tuple emit the same router-translated path.
//!
//! Single-segment dynamic params under Expo Router collapse to
//! `[name].tsx`, so authored routes `at "/users/:id"` and
//! `at "/users/:user_id"` both translate to the same file path
//! `/users/[id]` (or `/users/[user_id]`, depending on declaration
//! order). On TanStack the same authored pair stays distinct
//! (`$id` vs `$user_id`), so the rule is target-aware.
//!
//! Severity: `error` per `docs/proposals/mobile-target.md` §9.2.
//!
//! Reference: docs/proposals/mobile-target.md §9.2.

use std::collections::HashMap;

use super::ir_stub::{Audience, Module, Surface, View};
use super::sort_findings;

/// One `lzx-route-collision` finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub feature: String,
    pub audience: String,
    pub target: String,
    pub view: String,
    pub authored_route: String,
    pub translated_route: String,
    pub collides_with_view: String,
    pub collides_with_route: String,
    pub line: usize,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "lzx-route-collision";

    /// Render the canonical diagnostic message.
    pub fn message(&self) -> String {
        format!(
            "routes `{}` and `{}` both translate to `{}` under the {} router, \
             producing a file-system collision in audience `{}`. \
             Hint: disambiguate by making one route deeper, e.g. `/{}/by-id/:id`. \
             See docs/proposals/mobile-target.md §9.2.",
            self.collides_with_route,
            self.authored_route,
            self.translated_route,
            self.target,
            self.audience,
            self.translated_route
                .split('/')
                .find(|s| !s.is_empty() && !s.starts_with('[') && !s.starts_with('$'))
                .unwrap_or("path"),
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run `lzx-route-collision` across the module. Walks every
/// `(surface.target, audience.name)` tuple and groups views by their
/// translated route. Two views with the same translated route in the
/// same tuple emit findings (one per duplicate view, anchored at the
/// later declaration).
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::doctor::lzx::route_collision::check;
/// use lazuli_cli::doctor::lzx::ir_stub::Module;
///
/// // let findings = check(&module);
/// ```
pub fn check(module: &Module) -> Vec<Finding> {
    let mut out = Vec::new();
    for feature in &module.features {
        for surface in &feature.surfaces {
            for audience in &surface.audiences {
                walk_audience(&feature.name, surface, audience, &mut out);
            }
        }
    }
    sort_findings(&mut out, |f| {
        (f.feature.clone(), f.view.clone(), Finding::CODE, f.line)
    });
    out
}

fn walk_audience(
    feature_name: &str,
    surface: &Surface,
    audience: &Audience,
    out: &mut Vec<Finding>,
) {
    // First-pass: collect (translated_route, view_name, authored_route, line)
    // for each view that carries an `at` clause.
    struct RouteEntry<'a> {
        view_name: &'a str,
        authored: &'a str,
        translated: String,
        line: usize,
    }

    let mut entries: Vec<RouteEntry> = Vec::new();
    for view in &audience.views {
        let (name, route, line) = match view {
            View::List(v) => match v.at.as_deref() {
                Some(r) => (v.name.as_str(), r, v.line),
                None => continue,
            },
            View::Detail(v) => match v.at.as_deref() {
                Some(r) => (v.name.as_str(), r, v.line),
                None => continue,
            },
            View::Create(v) => match v.at.as_deref() {
                Some(r) => (v.name.as_str(), r, v.line),
                None => continue,
            },
        };
        entries.push(RouteEntry {
            view_name: name,
            authored: route,
            translated: translate_for_target(&surface.target, route),
            line,
        });
    }

    // Second-pass: bucket by translated route; each bucket with size >1
    // emits one finding per entry past the first (anchored at the later
    // declaration, pointing back at the prior entry).
    let mut buckets: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, entry) in entries.iter().enumerate() {
        buckets
            .entry(entry.translated.clone())
            .or_default()
            .push(idx);
    }

    for (translated, indices) in buckets {
        if indices.len() < 2 {
            continue;
        }
        let first_idx = indices[0];
        for &later_idx in &indices[1..] {
            let later = &entries[later_idx];
            let earlier = &entries[first_idx];
            out.push(Finding {
                feature: feature_name.to_owned(),
                audience: audience.name.clone(),
                target: surface.target.clone(),
                view: later.view_name.to_owned(),
                authored_route: later.authored.to_owned(),
                translated_route: translated.clone(),
                collides_with_view: earlier.view_name.to_owned(),
                collides_with_route: earlier.authored.to_owned(),
                line: later.line,
            });
        }
    }
}

/// Translate an authored `at "<route>"` to the COLLISION-EQUIVALENCE
/// key for the surface's target. Two views collide when their keys are
/// equal.
///
/// Under Expo Router (`mobile`), a directory may contain at most one
/// `[<name>].tsx` file — multiple dynamic siblings produce ambiguous
/// routing regardless of the placeholder name. The translator replaces
/// every single-segment dynamic param with a constant marker `<dyn>`
/// so `/users/:id` and `/users/:user_id` bucket together.
///
/// Under TanStack (`web`), file-based routing distinguishes `$id.tsx`
/// from `$user_id.tsx` — the placeholder name IS the file. The
/// translator preserves the name so the collision check only fires
/// when both authored routes happen to use identical placeholder names
/// (a much rarer mistake).
///
/// Unknown targets fall back to the authored form unchanged.
fn translate_for_target(target: &str, route: &str) -> String {
    let mut out = String::with_capacity(route.len());
    let bytes = route.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > start {
                let name = &route[start..j];
                match target {
                    "mobile" => {
                        // Name-erasure: all dynamic siblings collide.
                        out.push_str("[*]");
                    }
                    "web" => {
                        out.push('$');
                        out.push_str(name);
                    }
                    _ => {
                        out.push(':');
                        out.push_str(name);
                    }
                }
                i = j;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::ir_stub::*;
    use super::*;

    fn detail_view(name: &str, at: &str, line: usize) -> View {
        View::Detail(ViewDetail {
            name: name.to_owned(),
            at: Some(at.to_owned()),
            source: QueryRef {
                feature: None,
                name: "by_id".into(),
            },
            route_params: vec![],
            sections: vec![],
            cells: vec![],
            actions: vec![],
            line,
        })
    }

    fn list_view(name: &str, at: &str, line: usize) -> View {
        View::List(ViewList {
            name: name.to_owned(),
            at: Some(at.to_owned()),
            source: QueryRef {
                feature: None,
                name: "mine".into(),
            },
            render: ListRender::Table { columns: vec![] },
            columns: vec![],
            search: vec![],
            filter: vec![],
            search_decl: None,
            filter_decls: vec![],
            selection: None,
            sort: None,
            cells: vec![],
            actions: vec![],
            drawer: None,
            line,
        })
    }

    fn surface(target: &str, views: Vec<View>) -> Surface {
        Surface {
            feature: "catalog".into(),
            target: target.into(),
            audiences: vec![Audience {
                name: "buyer".into(),
                requires: vec![],
                views,
                line: 1,
            }],
        }
    }

    fn module_with(surface: Surface) -> Module {
        Module {
            features: vec![Feature {
                name: "catalog".into(),
                resources: vec![],
                queries: vec![],
                commands: vec![],
                surfaces: vec![surface],
            }],
        }
    }

    #[test]
    fn distinct_dynamic_names_collide_under_expo() {
        let m = module_with(surface(
            "mobile",
            vec![
                detail_view("by_id", "/listings/:id", 10),
                detail_view("by_slug", "/listings/:slug", 14),
            ],
        ));
        let findings = check(&m);
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.view, "by_slug");
        assert_eq!(f.collides_with_view, "by_id");
        assert_eq!(f.authored_route, "/listings/:slug");
        assert_eq!(f.collides_with_route, "/listings/:id");
        // Name-erased equivalence key.
        assert_eq!(f.translated_route, "/listings/[*]");
        let msg = f.message();
        assert!(msg.contains("translate to `/listings/[*]`"));
        assert!(msg.contains("mobile router"));
    }

    #[test]
    fn same_authored_route_collides_under_any_target() {
        let m = module_with(surface(
            "mobile",
            vec![
                list_view("admin_list", "/users", 4),
                list_view("buyer_list", "/users", 9),
            ],
        ));
        let findings = check(&m);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].view, "buyer_list");
        assert_eq!(findings[0].translated_route, "/users");
    }

    #[test]
    fn distinct_dynamic_names_do_not_collide_under_web() {
        // TanStack uses `$name` which keeps distinct placeholders distinct.
        let m = module_with(surface(
            "web",
            vec![
                detail_view("by_id", "/listings/:id", 10),
                detail_view("by_slug", "/listings/:slug", 14),
            ],
        ));
        assert!(check(&m).is_empty());
    }

    #[test]
    fn views_without_at_clause_are_skipped() {
        let m = module_with(surface(
            "mobile",
            vec![
                detail_view("by_id", "/listings/:id", 10),
                // Same translated path but no `at` declaration — not a
                // route view; ignored.
                View::Detail(ViewDetail {
                    name: "no_route".into(),
                    at: None,
                    source: QueryRef {
                        feature: None,
                        name: "by_id".into(),
                    },
                    route_params: vec![],
                    sections: vec![],
                    cells: vec![],
                    actions: vec![],
                    line: 14,
                }),
            ],
        ));
        assert!(check(&m).is_empty());
    }

    #[test]
    fn three_way_collision_emits_two_findings() {
        let m = module_with(surface(
            "mobile",
            vec![
                detail_view("by_id", "/listings/:id", 10),
                detail_view("by_slug", "/listings/:slug", 14),
                detail_view("by_key", "/listings/:key", 18),
            ],
        ));
        let findings = check(&m);
        assert_eq!(findings.len(), 2);
        // Both later views are pinned against the first.
        for f in &findings {
            assert_eq!(f.collides_with_view, "by_id");
        }
    }
}
