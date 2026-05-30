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
# List the audiences this frontend serves. Each MUST be declared by a `.lzx`
# surface (`surface <f> web` / `audience <name>`), or doctor fires
# FRONTEND-AUDIENCE-UNKNOWN-001. Empty until you add your first surface — then
# add the audience name here. e.g. audiences = ["admin", "public"]
audiences = []
"#;

/// `[frontends.mobile]` snippet appended to a project's
/// `Lazurite.toml` when `scaffold_frontend_mobile` runs.
pub const FRONTEND_MANIFEST_MOBILE_SNIPPET: &str = r#"
[frontends.mobile]
target = "expo"
source = "app/clients/mobile"
out = "dist/ts-mobile"
# Each audience MUST be backed by a `.lzx` surface (`surface <f> mobile` /
# `audience <name>`) or doctor fires FRONTEND-AUDIENCE-UNKNOWN-001. Empty until
# you add a mobile surface — then add its audience here. e.g. audiences = ["mobile"]
audiences = []
"#;
