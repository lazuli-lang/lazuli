//! `lzx-route-param-orphan` — `route X: Type from path` declared but no `:X`
//! placeholder in the path string.
//!
//! Fires on `view detail` views whose `route_params` list contains a name
//! that doesn't appear as `:<name>` in the `at` path string. Symmetric
//! counterpart of [`route_param_missing_binding`](super::route_param_missing_binding) — together
//! they ensure the placeholder set equals the typed-binding set.
//!
//! Severity: `warning` per `docs/proposals/lzx-integration-codegen.md` §11
//! (a declared-but-unused binding is dead code, not a runtime error).

use super::ir_stub::{Module, View, ViewDetail};
use super::{extract_path_placeholders, sort_findings};

/// One `lzx-route-param-orphan` finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub feature: String,
    pub view: String,
    pub line: usize,
    pub message: String,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "lzx-route-param-orphan";

    /// Render the canonical diagnostic message.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let msg = Finding::message(/* ... */);
    /// ```
    pub fn message(feature: &str, view: &str, param: &str, path: &str) -> String {
        format!(
            "view `{view}` in feature `{feature}`: typed binding `route \
             {param}: <Type> from path` is declared but `:{param}` does not \
             appear in path `{path}`. Remove the binding or add the \
             placeholder. See \
             docs/proposals/lzx-integration-codegen.md §5.2 + §11."
        )
    }
}

/// Run `lzx-route-param-orphan` across all `.lzx` surfaces.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::doctor::lzx::route_param_orphan::check;
/// use lazuli_cli::doctor::lzx::ir_stub::Module;
///
/// // let findings = check(&module);
/// ```
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
    // If `at` is missing, every binding is dead code per the rule contract.
    // We anchor to the binding line in that case so the user sees the offending
    // declaration directly. Use empty string for the path argument.
    let (path, placeholders): (String, Vec<String>) = match detail.at.as_deref() {
        Some(p) => (p.to_owned(), extract_path_placeholders(p)),
        None => (String::new(), Vec::new()),
    };

    for param in &detail.route_params {
        if !placeholders.iter().any(|ph| ph == &param.name) {
            out.push(Finding {
                feature: feature_name.to_owned(),
                view: detail.name.clone(),
                line: param.line,
                message: Finding::message(feature_name, &detail.name, &param.name, &path),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ir_stub::*;
    use super::*;

    fn detail(
        name: &str,
        line: usize,
        at: Option<&str>,
        route_params: Vec<(&str, &str, usize)>,
    ) -> View {
        View::Detail(ViewDetail {
            name: name.to_owned(),
            at: at.map(str::to_owned),
            source: QueryRef {
                feature: None,
                name: "by_key".into(),
            },
            route_params: route_params
                .into_iter()
                .map(|(n, t, l)| RouteParam {
                    name: n.to_owned(),
                    type_label: t.to_owned(),
                    line: l,
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

    /// Negative — every binding has a matching placeholder.
    #[test]
    fn negative_all_bindings_have_placeholders_no_finding() {
        let v = detail(
            "slug_detail",
            10,
            Some("/slugs/:key"),
            vec![("key", "Text", 11)],
        );
        let module = mk_module(vec![v]);
        assert!(check(&module).is_empty());
    }

    /// Positive — `:ghost` declared but not in path.
    #[test]
    fn positive_orphan_binding_fires() {
        let v = detail(
            "slug_detail",
            14,
            Some("/slugs/:key"),
            vec![("key", "Text", 15), ("ghost", "Text", 16)],
        );
        let module = mk_module(vec![v]);
        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].view, "slug_detail");
        // Anchored to the orphan binding line, not the view line.
        assert_eq!(findings[0].line, 16);
        assert!(findings[0].message.contains("ghost"));
    }

    /// Edge — view with no `at` and bindings → every binding is orphaned.
    #[test]
    fn edge_no_at_makes_every_binding_orphan() {
        let v = detail(
            "slug_detail",
            20,
            None,
            vec![("a", "Text", 21), ("b", "Text", 22)],
        );
        let module = mk_module(vec![v]);
        let findings = check(&module);
        assert_eq!(findings.len(), 2);
    }
}
