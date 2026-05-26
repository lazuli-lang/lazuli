//! `view_e2e_pair` coverage layer.
//!
//! Walks every `view` declaration surfaced from `.lzx` and
//! filesystem-checks for a paired Playwright spec file. A view is
//! covered when ANY of these paths resolves to a real file:
//!
//!   1. `<discovery_root>/<experience>/<view>.spec.ts` — nested
//!      layout (Wave 3.5 canonical convention).
//!   2. `<discovery_root>/<view>.spec.ts` — flat layout, view name
//!      verbatim (snake_case as the IR carries it).
//!   3. `<discovery_root>/<view-kebab>.spec.ts` — flat layout, view
//!      name kebab-cased (the JS-ecosystem-conventional file name).
//!
//! `<discovery_root>` defaults to `<project>/e2e/` when no override
//! is supplied. Pilots that wire `[testing.playwright].discovery_root`
//! in `Lazurite.toml` (e.g. `app/clients/<name>/e2e` for multi-client
//! layouts) have that root honored as the scan base.
//!
//! Doctor does NOT run Playwright. This calculator only checks file
//! presence — the same `TEST-VIEW-E2E-MISSING-001` shape, surfaced as
//! a coverage metric rather than a diagnostic. Acceptance bar item:
//! "Lazuli wires to Playwright, doesn't reimplement it."
//!
//! When no project root is provided, the layer reports `total = 0`
//! (vacuous pass) instead of failing the caller.

use std::path::Path;

use super::{LayerCoverage, LzxViewRef};

/// Compute the `view_e2e_pair` layer. For every view, probes nested
/// and flat Playwright spec paths under the resolved discovery root and
/// counts the view as covered when any candidate resolves to an existing
/// file. Without a project root, returns a vacuous-pass layer with
/// `source = "filesystem"`.
pub fn compute(
    views: &[LzxViewRef],
    project_root: Option<&Path>,
    discovery_root: Option<&Path>,
) -> LayerCoverage {
    let Some(root) = project_root else {
        return LayerCoverage::new(0, 0).with_source("filesystem");
    };
    let scan_root = match discovery_root {
        Some(rel) if rel.is_absolute() => rel.to_path_buf(),
        Some(rel) => root.join(rel),
        None => root.join("e2e"),
    };
    let mut total = 0usize;
    let mut covered = 0usize;
    for v in views {
        total += 1;
        if view_has_spec(v, &scan_root) {
            covered += 1;
        }
    }
    LayerCoverage::new(covered, total).with_source("filesystem")
}

fn view_has_spec(v: &LzxViewRef, scan_root: &Path) -> bool {
    let view_snake = &v.view;
    let view_kebab = v.view.replace('_', "-");

    let nested = scan_root
        .join(&v.experience)
        .join(format!("{view_snake}.spec.ts"));
    if nested.exists() {
        return true;
    }

    let flat_snake = scan_root.join(format!("{view_snake}.spec.ts"));
    if flat_snake.exists() {
        return true;
    }

    if view_kebab != *view_snake {
        let flat_kebab = scan_root.join(format!("{view_kebab}.spec.ts"));
        if flat_kebab.exists() {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_spec(dir: &Path, name: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(name), b"// stub").unwrap();
    }

    #[test]
    fn no_project_root_yields_zero_total() {
        let l = compute(&[], None, None);
        assert_eq!(l.total, 0);
        assert_eq!(l.covered, 0);
    }

    #[test]
    fn missing_spec_file_is_uncovered() {
        let tmp = tempfile::tempdir().unwrap();
        let views = vec![LzxViewRef {
            experience: "account".to_string(),
            view: "profile".to_string(),
        }];
        let l = compute(&views, Some(tmp.path()), None);
        assert_eq!(l.total, 1);
        assert_eq!(l.covered, 0);
    }

    #[test]
    fn nested_canonical_spec_is_covered() {
        let tmp = tempfile::tempdir().unwrap();
        write_spec(&tmp.path().join("e2e").join("account"), "profile.spec.ts");
        let views = vec![LzxViewRef {
            experience: "account".to_string(),
            view: "profile".to_string(),
        }];
        let l = compute(&views, Some(tmp.path()), None);
        assert_eq!(l.covered, 1);
        assert_eq!(l.total, 1);
    }

    #[test]
    fn flat_snake_layout_is_covered() {
        let tmp = tempfile::tempdir().unwrap();
        write_spec(&tmp.path().join("e2e"), "host_property_edit.spec.ts");
        let views = vec![LzxViewRef {
            experience: "catalog".to_string(),
            view: "host_property_edit".to_string(),
        }];
        let l = compute(&views, Some(tmp.path()), None);
        assert_eq!(l.covered, 1);
    }

    #[test]
    fn flat_kebab_layout_is_covered() {
        let tmp = tempfile::tempdir().unwrap();
        write_spec(&tmp.path().join("e2e"), "host-property-edit.spec.ts");
        let views = vec![LzxViewRef {
            experience: "catalog".to_string(),
            view: "host_property_edit".to_string(),
        }];
        let l = compute(&views, Some(tmp.path()), None);
        assert_eq!(l.covered, 1);
    }

    #[test]
    fn discovery_root_override_is_honored() {
        let tmp = tempfile::tempdir().unwrap();
        let custom = Path::new("app/clients/hostpoint-app/e2e");
        write_spec(&tmp.path().join(custom), "host-property-edit.spec.ts");
        let views = vec![LzxViewRef {
            experience: "catalog".to_string(),
            view: "host_property_edit".to_string(),
        }];
        let l_default = compute(&views, Some(tmp.path()), None);
        assert_eq!(l_default.covered, 0);
        let l_override = compute(&views, Some(tmp.path()), Some(custom));
        assert_eq!(l_override.covered, 1);
    }

    #[test]
    fn mixed_layouts_aggregate_correctly() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("e2e");
        write_spec(&root.join("account"), "profile.spec.ts");
        write_spec(&root, "host-property-edit.spec.ts");
        let views = vec![
            LzxViewRef {
                experience: "account".to_string(),
                view: "profile".to_string(),
            },
            LzxViewRef {
                experience: "catalog".to_string(),
                view: "host_property_edit".to_string(),
            },
            LzxViewRef {
                experience: "trust".to_string(),
                view: "review_inbox".to_string(),
            },
        ];
        let l = compute(&views, Some(tmp.path()), None);
        assert_eq!(l.total, 3);
        assert_eq!(l.covered, 2);
    }

    #[test]
    fn legacy_present_spec_file_is_covered() {
        let tmp = tempfile::tempdir().unwrap();
        write_spec(&tmp.path().join("e2e").join("account"), "profile.spec.ts");
        let views = vec![LzxViewRef {
            experience: "account".to_string(),
            view: "profile".to_string(),
        }];
        let l = compute(&views, Some(tmp.path()), None);
        assert_eq!(l.total, 1);
        assert_eq!(l.covered, 1);
    }
}
