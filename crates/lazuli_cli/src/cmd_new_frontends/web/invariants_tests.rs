//! Invariant tests on the materialized web scaffold output.
//!
//! Each test runs a fresh `scaffold_frontend_web` and then verifies a
//! framework-level invariant against the produced tree:
//!
//! - `scaffold_web_seeds_carry_banner_and_cn_import` — Wave K W2
//!   pick: every Shadcn-seed primitive carries the "User owns this
//!   file" banner and (when applicable) imports the canonical
//!   `@web/theme/cn` helper.
//! - `scaffold_web_satisfies_vocab_client_src_001` — Wave G: a fresh
//!   scaffold must produce zero `VOCAB-CLIENT-SRC-001` doctor
//!   diagnostics (canonical 6+6 closed catalog).

use std::fs;

use super::super::test_support::tempdir;
use super::{WEB_UI_SUBDIRS, scaffold_frontend_web};

/// Wave K invariant: each Shadcn-seed primitive carries the
/// scaffold-seed banner ("User owns this file") and references
/// the canonical `@web/theme/cn` helper. Catches accidental
/// drift between the templates and the W2 scaffold-seed pick.
#[test]
fn scaffold_web_seeds_carry_banner_and_cn_import() {
    let project = tempdir();
    let root = project.path();

    scaffold_frontend_web(root, "demo").unwrap();

    // `cn.ts` ships and is the standard tailwind-merge + clsx recipe.
    let cn_ts = fs::read_to_string(root.join("app/web/theme/cn.ts")).unwrap();
    assert!(cn_ts.contains("tailwind-merge"));
    assert!(cn_ts.contains("clsx"));
    assert!(cn_ts.contains("export function cn"));
    assert!(
        cn_ts.contains("User owns this file"),
        "cn.ts missing scaffold-seed banner"
    );

    // Every primitive carries the banner; each one that references
    // `cn()` imports it from `@web/theme/cn`.
    let primitives = [
        ("app/web/ui/forms/Button.tsx", true),
        ("app/web/ui/forms/Input.tsx", true),
        ("app/web/ui/feedback/Toast.tsx", false), // Sonner wrapper has no cn() use.
        ("app/web/ui/display/Card.tsx", true),
        ("app/web/ui/overlays/Dialog.tsx", true),
        ("app/web/ui/layout/Stack.tsx", true),
    ];
    for (rel, uses_cn) in primitives {
        let body = fs::read_to_string(root.join(rel)).unwrap();
        assert!(
            body.contains("User owns this file"),
            "{} missing scaffold-seed banner",
            rel,
        );
        if uses_cn {
            assert!(
                body.contains("@web/theme/cn"),
                "{} must import cn from @web/theme/cn",
                rel,
            );
        }
    }

    // Button uses CVA + Radix Slot for asChild (the W2-specified shape).
    let button = fs::read_to_string(root.join("app/web/ui/forms/Button.tsx")).unwrap();
    assert!(button.contains("class-variance-authority"));
    assert!(button.contains("@radix-ui/react-slot"));
    assert!(button.contains("asChild"));

    // Dialog uses @radix-ui/react-dialog (the W2-specified shape).
    let dialog = fs::read_to_string(root.join("app/web/ui/overlays/Dialog.tsx")).unwrap();
    assert!(dialog.contains("@radix-ui/react-dialog"));

    // Stack is pure Tailwind — no Radix import.
    let stack = fs::read_to_string(root.join("app/web/ui/layout/Stack.tsx")).unwrap();
    assert!(!stack.contains("@radix-ui"));
    assert!(stack.contains("class-variance-authority"));

    // Toast re-exports Sonner — no Radix import.
    let toast = fs::read_to_string(root.join("app/web/ui/feedback/Toast.tsx")).unwrap();
    assert!(toast.contains("sonner"));
}

/// Wave G invariant: a fresh `scaffold_frontend_web` output must
/// satisfy `VOCAB-CLIENT-SRC-001` — zero doctor diagnostics. The
/// scaffold emits the canonical 6+6 closed catalog per
/// `[[client_src_canonical_architecture_2026-05-17]]` §3, so the
/// doctor walker (which checks `app/web/` singular topology) must
/// see ONLY the six allowed top-level folders and ONLY the six
/// allowed `ui/` children.
#[test]
fn scaffold_web_satisfies_vocab_client_src_001() {
    use crate::doctor::folder::vocab_client_src_001;

    let project = tempdir();
    let root = project.path();

    scaffold_frontend_web(root, "demo").unwrap();

    let findings = vocab_client_src_001::check(root);
    assert!(
        findings.is_empty(),
        "fresh scaffold must produce zero VOCAB-CLIENT-SRC-001 \
         diagnostics; got {} finding(s): {:?}",
        findings.len(),
        findings
    );

    // Belt-and-braces: confirm each of the six allowed top-level
    // folders exists (this is what makes the doctor walker happy).
    for top in &["shell", "routes", "ui", "theme", "state", "assets"] {
        assert!(
            root.join("app/web").join(top).is_dir(),
            "canonical top-level folder `{}` missing",
            top
        );
    }
    // And each of the six allowed `ui/` children.
    for ui_sub in WEB_UI_SUBDIRS {
        assert!(
            root.join("app/web/ui").join(ui_sub).is_dir(),
            "canonical ui/{} missing",
            ui_sub
        );
    }
}
