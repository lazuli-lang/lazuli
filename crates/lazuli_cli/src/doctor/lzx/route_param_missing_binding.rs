//! `lzx-route-param-missing-binding` — `at "/path/:slug"` placeholder has no
//! `route slug: <Type> from path` declaration.
//!
//! Fires on `view detail` views whose `at` path string contains a `:<name>`
//! placeholder for which there's no matching `route <name>: <Type> from path`
//! binding. `view list` and `view create` may carry routes (via the `at`
//! keyword) but their typed bindings live in different shapes — only
//! `view detail` is in scope for this rule per §5.1.
//!
//! Severity: `error` per `docs/proposals/lzx-integration-codegen.md` §11.

use super::ir_stub::{Module, View, ViewDetail};
use super::{extract_path_placeholders, sort_findings};

/// One `lzx-route-param-missing-binding` finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub feature: String,
    pub view: String,
    pub line: usize,
    pub message: String,
}

impl Finding {
    pub const CODE: &'static str = "lzx-route-param-missing-binding";

    pub fn message(feature: &str, view: &str, placeholder: &str, path: &str) -> String {
        format!(
            "view `{view}` in feature `{feature}`: path `{path}` contains \
             placeholder `:{placeholder}` but no `route {placeholder}: <Type> \
             from path` binding is declared. Add the typed binding or remove \
             the placeholder. See \
             docs/proposals/lzx-integration-codegen.md §5.2 + §11."
        )
    }
}

/// Run `lzx-route-param-missing-binding` across all `.lzx` surfaces.
pub fn check(module: &Module) -> Vec<Finding> {
    let mut out = Vec::new();
    for feature in &module.features {
        for surface in &feature.surfaces {
            for audience in &surface.audiences {
                for view in &audience.views {
                    if let View::Detail(detail) = view {
                        check_detail(&feature.name, detail, &mut out);
                    }
                }
            }
        }
    }
    sort_findings(&mut out, |f| {
        (f.feature.clone(), f.view.clone(), Finding::CODE, f.line)
    });
    out
}

fn check_detail(feature_name: &str, detail: &ViewDetail, out: &mut Vec<Finding>) {
    let path = match detail.at.as_deref() {
        Some(p) => p,
        None => return,
    };
    let placeholders = extract_path_placeholders(path);
    for ph in &placeholders {
        let bound = detail.route_params.iter().any(|r| r.name == *ph);
        if !bound {
            out.push(Finding {
                feature: feature_name.to_owned(),
                view: detail.name.clone(),
                line: detail.line,
                message: Finding::message(feature_name, &detail.name, ph, path),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ir_stub::*;
    use super::*;

    fn detail(name: &str, line: usize, at: &str, route_params: Vec<(&str, &str)>) -> View {
        View::Detail(ViewDetail {
            name: name.to_owned(),
            at: Some(at.to_owned()),
            source: QueryRef {
                feature: None,
                name: "by_key".into(),
            },
            route_params: route_params
                .into_iter()
                .enumerate()
                .map(|(i, (n, t))| RouteParam {
                    name: n.to_owned(),
                    type_label: t.to_owned(),
                    line: line + 1 + i,
                })
                .collect(),
            sections: vec![],
            cells: vec![],
            actions: vec![],
            line,
        })
    }

    fn mk_module(views: Vec<View>) -> Module {
        Module {
            features: vec![Feature {
                name: "slug".into(),
                resources: vec![],
                queries: vec![],
                commands: vec![],
                surfaces: vec![Surface {
                    feature: "slug".into(),
                    target: "web".into(),
                    audiences: vec![Audience {
                        name: "admin".into(),
                        requires: vec![],
                        views,
                        line: 3,
                    }],
                }],
            }],
        }
    }

    /// Negative — every placeholder has a matching `route` binding.
    #[test]
    fn negative_all_placeholders_bound_no_finding() {
        let v = detail("slug_detail", 10, "/slugs/:key", vec![("key", "Text")]);
        let module = mk_module(vec![v]);
        assert!(check(&module).is_empty());
    }

    /// Positive — a `:slug` placeholder with no binding fires.
    #[test]
    fn positive_missing_binding_fires() {
        let v = detail("slug_detail", 12, "/slugs/:slug", vec![]);
        let module = mk_module(vec![v]);
        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].view, "slug_detail");
        assert_eq!(findings[0].line, 12);
        assert!(findings[0].message.contains(":slug"));
        assert!(findings[0].message.contains("/slugs/:slug"));
    }

    /// Edge — multiple placeholders missing produce one finding each.
    #[test]
    fn edge_multiple_missing_fire_multiple_times() {
        let v = detail(
            "slug_detail",
            14,
            "/orgs/:org/slugs/:slug",
            vec![("org", "Text")], // slug missing
        );
        let module = mk_module(vec![v]);
        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains(":slug"));

        // Now both missing.
        let v2 = detail("slug_detail2", 15, "/orgs/:org/slugs/:slug", vec![]);
        let module2 = mk_module(vec![v2]);
        let findings2 = check(&module2);
        assert_eq!(findings2.len(), 2);
    }

    /// Edge — view with no `at` declaration produces no finding (nothing to
    /// check against).
    #[test]
    fn no_at_produces_no_finding() {
        let v = View::Detail(ViewDetail {
            name: "slug_detail".into(),
            at: None,
            source: QueryRef {
                feature: None,
                name: "by_key".into(),
            },
            route_params: vec![],
            sections: vec![],
            cells: vec![],
            actions: vec![],
            line: 18,
        });
        let module = mk_module(vec![v]);
        assert!(check(&module).is_empty());
    }
}
