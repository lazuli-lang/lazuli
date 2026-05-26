//! `Lazurite.toml` `[frontends.*]` snippets appended by `lazuli new`.
//!
//! Two small constants — one per frontend stack — that the
//! scaffold writes into the project's `Lazurite.toml` when the
//! corresponding `--frontends <stack>` flag is set.
//!
//! Wave R7-3 extract: lifted out of `templates.rs`.

/// `Lazurite.toml [frontends.<x>]` snippet appended when --frontends flag set.
/// Paths follow the canonical layout in `docs/project-structure.md`:
/// default web client at `app/web/`, mobile (different runtime) at
/// `app/clients/mobile/`.
pub const FRONTEND_MANIFEST_WEB_SNIPPET: &str = r#"
[frontends.web]
target = "tanstack-vite"
source = "app/web"
out = "dist/ts-web"
audiences = ["admin", "public"]
"#;

/// `[frontends.mobile]` snippet appended to a project's
/// `Lazurite.toml` when `scaffold_frontend_mobile` runs.
pub const FRONTEND_MANIFEST_MOBILE_SNIPPET: &str = r#"
[frontends.mobile]
target = "expo"
source = "app/clients/mobile"
out = "dist/ts-mobile"
audiences = ["mobile"]
"#;
