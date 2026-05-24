//! `view_e2e_pair` coverage layer.
//!
//! Walks every `view` declaration surfaced from `.lzx` and
//! filesystem-checks `e2e/<feature>/<view>.spec.ts` per the Wave 3.5
//! path convention. A view is **covered** when the paired spec file
//! exists on disk.
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

pub fn compute(views: &[LzxViewRef], project_root: Option<&Path>) -> LayerCoverage {
    let Some(root) = project_root else {
        return LayerCoverage::new(0, 0).with_source("filesystem");
    };
    let mut total = 0usize;
    let mut covered = 0usize;
    for v in views {
        total += 1;
        let spec_path = root
            .join("e2e")
            .join(&v.experience)
            .join(format!("{}.spec.ts", v.view));
        if spec_path.exists() {
            covered += 1;
        }
    }
    LayerCoverage::new(covered, total).with_source("filesystem")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_project_root_yields_zero_total() {
        let l = compute(&[], None);
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
        let l = compute(&views, Some(tmp.path()));
        assert_eq!(l.total, 1);
        assert_eq!(l.covered, 0);
    }

    #[test]
    fn present_spec_file_is_covered() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("e2e").join("account");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("profile.spec.ts"), b"// stub").unwrap();
        let views = vec![LzxViewRef {
            experience: "account".to_string(),
            view: "profile".to_string(),
        }];
        let l = compute(&views, Some(tmp.path()));
        assert_eq!(l.total, 1);
        assert_eq!(l.covered, 1);
    }
}
