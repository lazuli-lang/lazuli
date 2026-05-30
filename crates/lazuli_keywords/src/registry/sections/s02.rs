//! Registry `ALL` section 2/11 (SPEC-19 split; concatenated in `registry::ALL`).
#![allow(clippy::all, unused_imports)]

use super::super::builders::*;
use super::super::facets::*;
use crate::{CapabilitySpec, Context, DiagnosticFacet, SemanticToken, Sigil, Surface};

pub(crate) const ROWS: &[CapabilitySpec] = &[
    kw(
        "proxy",
        Context::App,
        SECTION,
        "Trusted-proxy / forwarded-header block.",
    ),
    kw(
        "runtime",
        Context::App,
        SECTION,
        "Runtime unit / process topology block.",
    ),
    kw(
        "services",
        Context::App,
        SECTION,
        "Service decomposition block.",
    ),
    kw(
        "targets",
        Context::App,
        STMT,
        "Generation targets (go/ts/...).",
    ),
    kw("title", Context::App, STMT, "App display title."),
    kw(
        "tracing",
        Context::App,
        SECTION,
        "Distributed-tracing configuration block.",
    ),
    kw(
        "urls",
        Context::App,
        SECTION,
        "Named URL declarations block.",
    ),
    kw(
        "uses",
        Context::App,
        STMT,
        "Declares a registry/experience the app uses.",
    ),
    kw("version", Context::App, STMT, "App version string."),
    kw(
        "env",
        Context::App,
        SECTION,
        "Environment-variable declarations block.",
    ),
    kw("route_guard", Context::App, STMT, "App-level route guard."),
    // app-meta scalar lines (statements-app-meta scope leaf)
    stmt(
        "mode",
        Context::App,
        "entity.name.function.statement.app-meta.lazuli",
        "Service mode (monolith/service).",
    ),
    stmt(
        "service_ready",
        Context::App,
        "entity.name.function.statement.app-meta.lazuli",
        "Marks the app service-ready.",
    ),
    stmt(
        "enforce_service_boundaries",
        Context::App,
        "entity.name.function.statement.app-meta.lazuli",
        "Enforce declared service boundaries.",
    ),
    stmt(
        "environment",
        Context::App,
        "entity.name.function.statement.app-meta.lazuli",
        "Environment selector.",
    ),
    // ── app: cookie block ──
    stmt(
        "default",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "Default cookie profile.",
    ),
    stmt(
        "session",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "Session cookie settings.",
    ),
    stmt(
        "csrf",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "CSRF cookie settings.",
    ),
    stmt(
        "signed",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "Signed-cookie flag.",
    ),
    stmt(
        "secure",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "Secure (HTTPS-only) flag.",
    ),
    stmt(
        "http_only",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "HttpOnly flag.",
    ),
    stmt(
        "same_site",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "SameSite policy.",
    ),
    stmt(
        "max_age",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "Cookie max-age.",
    ),
    stmt(
        "domain",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "Cookie domain.",
    ),
    stmt(
        "path",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
        "Cookie path.",
    ),
    // ── app: headers block ──
    stmt(
        "csp",
        Context::Headers,
        "entity.name.function.statement.headers.lazuli",
        "Content-Security-Policy header.",
    ),
    stmt(
        "hsts",
        Context::Headers,
        "entity.name.function.statement.headers.lazuli",
        "Strict-Transport-Security header.",
    ),
    stmt(
        "x_frame_options",
        Context::Headers,
        "entity.name.function.statement.headers.lazuli",
        "X-Frame-Options header.",
    ),
    stmt(
        "x_content_type_options",
        Context::Headers,
        "entity.name.function.statement.headers.lazuli",
        "X-Content-Type-Options header.",
    ),
    stmt(
        "referrer_policy",
        Context::Headers,
        "entity.name.function.statement.headers.lazuli",
        "Referrer-Policy header.",
    ),
    stmt(
        "permissions_policy",
        Context::Headers,
        "entity.name.function.statement.headers.lazuli",
        "Permissions-Policy header.",
    ),
    stmt(
        "include_subdomains",
        Context::Headers,
        "entity.name.function.statement.headers.lazuli",
        "HSTS includeSubDomains flag.",
    ),
    stmt(
        "preload",
        Context::Headers,
        "entity.name.function.statement.headers.lazuli",
        "HSTS preload flag.",
    ),
    // ── app: limits block ──
    stmt(
        "body_size",
        Context::Limits,
        "entity.name.function.statement.limits.lazuli",
        "Max request body size.",
    ),
    stmt(
        "header_size",
        Context::Limits,
        "entity.name.function.statement.limits.lazuli",
        "Max header size.",
    ),
    stmt(
        "upload_size",
        Context::Limits,
        "entity.name.function.statement.limits.lazuli",
        "Max upload size.",
    ),
    // ── app: proxy block ──
    stmt(
        "trusted",
        Context::Proxy,
        "entity.name.function.statement.proxy.lazuli",
        "Trusted proxy CIDRs.",
    ),
    stmt(
        "real_ip_header",
        Context::Proxy,
        "entity.name.function.statement.proxy.lazuli",
        "Real-IP header name.",
    ),
    stmt(
        "forwarded_proto_header",
        Context::Proxy,
        "entity.name.function.statement.proxy.lazuli",
        "Forwarded-Proto header name.",
    ),
    stmt(
        "forwarded_host_header",
        Context::Proxy,
        "entity.name.function.statement.proxy.lazuli",
        "Forwarded-Host header name.",
    ),
    // ── app: encryption block ──
    // Indent-6 children of an `encryption / key @key.<scope>` binding
    // (`crates/lazuli_manifest/src/app_manifest/manifest_indent.rs`
    // `Some("encryption")` arm): `source` / `algorithm` / `rotation` /
    // `rotation_profile`. `source` / `rotation` / `key` already have
    // `Context::Encryption` rows in the H2-backfill block below, so only
    // `algorithm` / `rotation_profile` are declared here.
    stmt(
        "algorithm",
        Context::Encryption,
        "entity.name.function.statement.encryption.lazuli",
        "Encryption algorithm.",
    ),
    stmt(
        "rotation_profile",
        Context::Encryption,
        "entity.name.function.statement.encryption.lazuli",
        "Key-rotation profile.",
    ),
    // ── app: locale block ──
    // Surface keywords the `app.locale` block parser accepts
    // (`crates/lazuli_manifest/src/app_manifest/manifest_indent4.rs`
    // `Some("locale")` arm): `default`, `supported`, `fallback`.
    stmt(
        "default",
        Context::Locale,
        "entity.name.function.statement.locale.lazuli",
        "Primary BCP-47 locale tag.",
    ),
    stmt(
        "supported",
        Context::Locale,
        "entity.name.function.statement.locale.lazuli",
        "Supported locales.",
    ),
    stmt(
        "fallback",
        Context::Locale,
        "entity.name.function.statement.locale.lazuli",
        "Fallback locale.",
    ),
    // ── app: cors block ──
    // Child keys the `app.cors` block parser accepts
    // (`crates/lazuli_manifest/src/app_manifest/manifest_indent4.rs`
    // `Some("cors")` arm): `allow_origins`, `allow_credentials`, `max_age`.
    stmt(
        "allow_origins",
        Context::Cors,
        "entity.name.function.statement.cors.lazuli",
        "Allowed CORS origins.",
    ),
];
