//! `lzx-cell-missing-impl` — `cells … @client.<slot>` references a slot
//! whose implementation file does not exist on disk for the enclosing
//! surface's target.
//!
//! For each cell binding `cells <field> @client.<slot>` inside a view
//! owned by feature `<feat>` on surface `<target>` (`"web"` or
//! `"mobile"`), the rule asserts that
//! `features/<feat>/<target>/cells/<slot>.tsx` exists. Authors who
//! delete the binding OR materialize the impl clear the finding.
//!
//! Severity: `error` per `docs/proposals/mobile-target.md` §9.1. The
//! rule was announced in L0 #1 §5.2 but never shipped — this is the
//! first implementation. Distinct from the existing
//! [`lzx-cell-slot-orphan`] (which fires on `field-not-in-columns`,
//! a purely-DSL violation that doesn't read the disk).
//!
//! Reference: docs/proposals/mobile-target.md §9.1.

use std::path::{Path, PathBuf};

use super::ir_stub::{Audience, CellBinding, Module, Surface, View};
use super::sort_findings;

/// One `lzx-cell-missing-impl` finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub feature: String,
    pub target: String,
    pub view: String,
    pub slot: String,
    pub expected_path: String,
    pub line: usize,
    pub sibling_target_path: Option<String>,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "lzx-cell-missing-impl";

    /// Render the canonical diagnostic message.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let msg = Finding::message(/* ... */);
    /// ```
    pub fn message(&self) -> String {
        let mut msg = format!(
            "slot `@client.{}` referenced from {} surface, but {} does not exist.",
            self.slot, self.target, self.expected_path,
        );
        if let Some(sibling) = self.sibling_target_path.as_deref() {
            msg.push_str(&format!(
                " (Hint: {sibling} exists, but {target} surfaces need their own implementation. \
                 Author {expected}, OR remove the cell binding from this view.)",
                target = self.target,
                expected = self.expected_path,
                sibling = sibling,
            ));
        } else {
            msg.push_str(&format!(
                " Author {} on disk, OR remove the cell binding from this view.",
                self.expected_path
            ));
        }
        msg
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run `lzx-cell-missing-impl` across all surfaces in the module. When
/// `project_root` is `None` (e.g., unit tests that don't have a real
/// filesystem), the rule is a no-op — the filesystem check is the
/// trigger, and there's nothing to assert without it.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::doctor::lzx::cell_missing_impl::check;
/// use lazuli_cli::doctor::lzx::ir_stub::Module;
///
/// // let findings = check(&module);
/// ```
pub fn check(module: &Module, project_root: Option<&Path>) -> Vec<Finding> {
    let Some(project_root) = project_root else {
        return Vec::new();
    };
    let mut out = Vec::new();

    for feature in &module.features {
        for surface in &feature.surfaces {
            for audience in &surface.audiences {
                for view in &audience.views {
                    check_view(
                        project_root,
                        &feature.name,
                        surface,
                        audience,
                        view,
                        &mut out,
                    );
                }
            }
        }
    }

    sort_findings(&mut out, |f| {
        (f.feature.clone(), f.view.clone(), Finding::CODE, f.line)
    });
    out
}

fn check_view(
    project_root: &Path,
    feature_name: &str,
    surface: &Surface,
    _audience: &Audience,
    view: &View,
    out: &mut Vec<Finding>,
) {
    let (view_name, cells) = match view {
        View::List(v) => (v.name.as_str(), v.cells.as_slice()),
        View::Detail(v) => (v.name.as_str(), v.cells.as_slice()),
        View::Create(v) => (v.name.as_str(), v.cells.as_slice()),
    };

    for cell in cells {
        let slot = strip_client_prefix(&cell.slot);
        if slot.is_empty() {
            continue;
        }
        let expected = cell_impl_path(feature_name, &surface.target, slot);
        if project_root.join(&expected).exists() {
            continue;
        }

        // Look for the sibling-target impl so the diagnostic can point
        // the author at the platform mirror.
        let sibling_target = match surface.target.as_str() {
            "web" => Some("mobile"),
            "mobile" => Some("web"),
            _ => None,
        };
        let sibling_target_path = sibling_target.and_then(|other| {
            let path = cell_impl_path(feature_name, other, slot);
            project_root.join(&path).exists().then_some(path)
        });

        out.push(Finding {
            feature: feature_name.to_owned(),
            target: surface.target.clone(),
            view: view_name.to_owned(),
            slot: slot.to_owned(),
            expected_path: expected,
            line: cell.line,
            sibling_target_path,
        });
    }
}

fn strip_client_prefix(slot: &str) -> &str {
    slot.strip_prefix("@client.").unwrap_or(slot)
}

fn cell_impl_path(feature: &str, target: &str, slot: &str) -> String {
    let mut p = PathBuf::from("features");
    p.push(feature);
    p.push(target);
    p.push("cells");
    p.push(format!("{slot}.tsx"));
    p.to_string_lossy().replace('\\', "/")
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::super::ir_stub::*;
    use super::*;

    fn cell(field: &str, slot: &str, line: usize) -> CellBinding {
        CellBinding {
            field: field.to_owned(),
            slot: slot.to_owned(),
            line,
        }
    }

    fn list_view_with_cells(name: &str, line: usize, cells: Vec<CellBinding>) -> View {
        View::List(ViewList {
            name: name.to_owned(),
            at: None,
            source: QueryRef {
                feature: None,
                name: "mine".into(),
            },
            render: ListRender::Table {
                columns: vec!["title".into(), "badge".into()],
            },
            columns: vec!["title".into(), "badge".into()],
            search: vec![],
            filter: vec![],
            search_decl: None,
            filter_decls: vec![],
            selection: None,
            sort: None,
            cells,
            actions: vec![],
            drawer: None,
            line,
        })
    }

    fn feature_with_surface(target: &str, view: View) -> Feature {
        Feature {
            name: "catalog".into(),
            resources: vec![],
            queries: vec![],
            commands: vec![],
            surfaces: vec![Surface {
                feature: "catalog".into(),
                target: target.into(),
                audiences: vec![Audience {
                    name: "buyer".into(),
                    requires: vec![],
                    views: vec![view],
                    line: 1,
                }],
            }],
        }
    }

    fn module_with(feature: Feature) -> Module {
        Module {
            features: vec![feature],
        }
    }

    #[test]
    fn missing_mobile_cell_fires_when_only_web_impl_exists() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // Only the web impl exists on disk.
        fs::create_dir_all(root.join("features/catalog/web/cells")).unwrap();
        fs::write(
            root.join("features/catalog/web/cells/price_badge.tsx"),
            "export const PriceBadge = () => null;\n",
        )
        .unwrap();

        let view = list_view_with_cells(
            "listings",
            8,
            vec![cell("badge", "@client.price_badge", 10)],
        );
        let module = module_with(feature_with_surface("mobile", view));

        let findings = check(&module, Some(root));
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.feature, "catalog");
        assert_eq!(f.target, "mobile");
        assert_eq!(f.slot, "price_badge");
        assert_eq!(
            f.expected_path,
            "features/catalog/mobile/cells/price_badge.tsx"
        );
        assert_eq!(
            f.sibling_target_path.as_deref(),
            Some("features/catalog/web/cells/price_badge.tsx")
        );
        let msg = f.message();
        assert!(msg.contains("mobile surface"));
        assert!(msg.contains("features/catalog/mobile/cells/price_badge.tsx"));
        assert!(msg.contains("Hint: features/catalog/web/cells/price_badge.tsx exists"));
    }

    #[test]
    fn present_impl_does_not_fire() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("features/catalog/mobile/cells")).unwrap();
        fs::write(
            root.join("features/catalog/mobile/cells/price_badge.tsx"),
            "export const PriceBadge = () => null;\n",
        )
        .unwrap();

        let view = list_view_with_cells(
            "listings",
            8,
            vec![cell("badge", "@client.price_badge", 10)],
        );
        let module = module_with(feature_with_surface("mobile", view));

        assert!(check(&module, Some(root)).is_empty());
    }

    #[test]
    fn no_sibling_path_when_neither_impl_exists() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let view = list_view_with_cells(
            "listings",
            8,
            vec![cell("badge", "@client.price_badge", 10)],
        );
        let module = module_with(feature_with_surface("mobile", view));

        let findings = check(&module, Some(root));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].sibling_target_path.is_none());
        assert!(findings[0].message().contains("Author"));
    }

    #[test]
    fn no_project_root_is_noop() {
        let view = list_view_with_cells(
            "listings",
            8,
            vec![cell("badge", "@client.price_badge", 10)],
        );
        let module = module_with(feature_with_surface("mobile", view));

        assert!(check(&module, None).is_empty());
    }

    #[test]
    fn empty_slot_after_strip_is_skipped() {
        let tmp = TempDir::new().unwrap();
        let view = list_view_with_cells("listings", 8, vec![cell("badge", "@client.", 10)]);
        let module = module_with(feature_with_surface("mobile", view));

        assert!(check(&module, Some(tmp.path())).is_empty());
    }

    #[test]
    fn web_surface_checks_web_directory_not_mobile() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // Mobile impl exists; web does not.
        fs::create_dir_all(root.join("features/catalog/mobile/cells")).unwrap();
        fs::write(
            root.join("features/catalog/mobile/cells/price_badge.tsx"),
            "export const PriceBadge = () => null;\n",
        )
        .unwrap();

        let view = list_view_with_cells(
            "listings",
            8,
            vec![cell("badge", "@client.price_badge", 10)],
        );
        let module = module_with(feature_with_surface("web", view));

        let findings = check(&module, Some(root));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].target, "web");
        assert_eq!(
            findings[0].expected_path,
            "features/catalog/web/cells/price_badge.tsx"
        );
        assert_eq!(
            findings[0].sibling_target_path.as_deref(),
            Some("features/catalog/mobile/cells/price_badge.tsx")
        );
    }
}
