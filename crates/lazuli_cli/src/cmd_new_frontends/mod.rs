//! Frontend scaffolding helpers for `lazuli new --frontends <list>`.
//!
//! Implements L0 #1 §6.1 — when the user passes `--frontends web`
//! and/or `--frontends mobile` to `lazuli new`, this module materializes
//! the canonical frontend-adapter shell, theme, config, and manifest entries.
//!
//! ## Boundary
//!
//! These helpers know NOTHING about the backend scaffold (already wired
//! by `main::scaffold_bare` / `main::scaffold_from_template`). They
//! operate on an already-created `project_root` and only add/append.
//!
//! ## Idempotency
//!
//! Both `scaffold_frontend_web` and `scaffold_frontend_mobile` are
//! **idempotent**: a second invocation on the same `project_root` is a
//! no-op — already-existing files are left untouched and the manifest
//! is only appended once. This lets the orchestrator call them
//! defensively (e.g. when re-running `lazuli new --frontends web,mobile`
//! after a partial failure) without clobbering user edits.

mod internals;
mod mobile;
mod web;

#[cfg(test)]
mod test_support;

pub use mobile::scaffold_frontend_mobile;
pub use web::scaffold_frontend_web;

#[cfg(test)]
mod tests {
    use std::fs;

    use super::test_support::tempdir;
    use super::*;
    use crate::templates;

    #[test]
    fn web_and_mobile_compose_in_same_project() {
        let project = tempdir();
        let root = project.path();

        scaffold_frontend_web(root, "demo").unwrap();
        scaffold_frontend_mobile(root, "demo").unwrap();

        assert!(root.join("app/web/shell/root.tsx").exists());
        assert!(root.join("app/clients/mobile/app/_layout.tsx").exists());

        let manifest = fs::read_to_string(root.join("Lazurite.toml")).unwrap();
        assert!(manifest.contains("[frontends.web]"));
        assert!(manifest.contains("[frontends.mobile]"));
    }

    /// Smoke test that the FRONTEND_* template consts aren't empty —
    /// catches a future accidental truncation of `templates.rs`.
    #[test]
    fn frontend_template_consts_are_nonempty() {
        // web
        assert!(!templates::FRONTEND_WEB_INDEX_HTML.is_empty());
        assert!(!templates::FRONTEND_WEB_MAIN_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_ROOT_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_LAYOUT_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_ERROR_BOUNDARY_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_ROUTES_INDEX_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_STATE_APP_STORE_TS.is_empty());
        assert!(!templates::FRONTEND_THEME_GLOBALS_CSS.is_empty());
        assert!(!templates::FRONTEND_THEME_PROVIDER_TSX.is_empty());
        assert!(!templates::FRONTEND_TAILWIND_CONFIG_TS.is_empty());
        assert!(!templates::FRONTEND_TSCONFIG_JSON.is_empty());
        assert!(!templates::FRONTEND_VITE_CONFIG_TS.is_empty());
        assert!(!templates::FRONTEND_PACKAGE_JSON.is_empty());
        assert!(!templates::FRONTEND_GITIGNORE.is_empty());
        // Wave K — Shadcn-seed primitives + cn() helper.
        assert!(!templates::FRONTEND_WEB_THEME_CN_TS.is_empty());
        assert!(!templates::FRONTEND_WEB_UI_BUTTON_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_UI_INPUT_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_UI_TOAST_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_UI_CARD_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_UI_DIALOG_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_UI_STACK_TSX.is_empty());

        // mobile
        assert!(!templates::FRONTEND_MOBILE_APP_LAYOUT_TSX.is_empty());
        assert!(!templates::FRONTEND_MOBILE_APP_INDEX_TSX.is_empty());
        assert!(!templates::FRONTEND_MOBILE_APP_JSON.is_empty());
        assert!(!templates::FRONTEND_MOBILE_BABEL_CONFIG.is_empty());
        assert!(!templates::FRONTEND_MOBILE_METRO_CONFIG.is_empty());
        assert!(!templates::FRONTEND_MOBILE_TSCONFIG.is_empty());
        assert!(!templates::FRONTEND_MOBILE_SHELL_CLIENT_TS.is_empty());
        assert!(!templates::FRONTEND_MOBILE_GITIGNORE.is_empty());
        assert!(!templates::FRONTEND_MOBILE_PACKAGE_JSON.is_empty());

        // manifest snippets
        assert!(templates::FRONTEND_MANIFEST_WEB_SNIPPET.contains("[frontends.web]"));
        assert!(templates::FRONTEND_MANIFEST_MOBILE_SNIPPET.contains("[frontends.mobile]"));
    }

    /// Make sure no LF-newline got mangled into CRLF (templates must be
    /// cross-platform; Lazuli runs on Windows). r#"..."# preserves
    /// whatever is in source; this guards against future edits that
    /// might paste CRLF from a Windows clipboard.
    #[test]
    fn templates_use_lf_newlines_only() {
        let all = [
            templates::FRONTEND_WEB_INDEX_HTML,
            templates::FRONTEND_WEB_MAIN_TSX,
            templates::FRONTEND_WEB_ROOT_TSX,
            templates::FRONTEND_WEB_LAYOUT_TSX,
            templates::FRONTEND_WEB_ERROR_BOUNDARY_TSX,
            templates::FRONTEND_WEB_ROUTES_INDEX_TSX,
            templates::FRONTEND_WEB_STATE_APP_STORE_TS,
            templates::FRONTEND_WEB_THEME_CN_TS,
            templates::FRONTEND_WEB_UI_BUTTON_TSX,
            templates::FRONTEND_WEB_UI_INPUT_TSX,
            templates::FRONTEND_WEB_UI_TOAST_TSX,
            templates::FRONTEND_WEB_UI_CARD_TSX,
            templates::FRONTEND_WEB_UI_DIALOG_TSX,
            templates::FRONTEND_WEB_UI_STACK_TSX,
            templates::FRONTEND_THEME_GLOBALS_CSS,
            templates::FRONTEND_THEME_PROVIDER_TSX,
            templates::FRONTEND_TAILWIND_CONFIG_TS,
            templates::FRONTEND_TSCONFIG_JSON,
            templates::FRONTEND_VITE_CONFIG_TS,
            templates::FRONTEND_PACKAGE_JSON,
            templates::FRONTEND_GITIGNORE,
            templates::FRONTEND_MOBILE_APP_LAYOUT_TSX,
            templates::FRONTEND_MOBILE_APP_INDEX_TSX,
            templates::FRONTEND_MOBILE_APP_JSON,
            templates::FRONTEND_MOBILE_BABEL_CONFIG,
            templates::FRONTEND_MOBILE_METRO_CONFIG,
            templates::FRONTEND_MOBILE_TSCONFIG,
            templates::FRONTEND_MOBILE_SHELL_CLIENT_TS,
            templates::FRONTEND_MOBILE_GITIGNORE,
            templates::FRONTEND_MOBILE_PACKAGE_JSON,
            templates::FRONTEND_MANIFEST_WEB_SNIPPET,
            templates::FRONTEND_MANIFEST_MOBILE_SNIPPET,
        ];
        for (i, s) in all.iter().enumerate() {
            assert!(
                !s.contains('\r'),
                "template index {i} contains CR — templates must be LF-only for cross-platform output"
            );
        }
    }
}
