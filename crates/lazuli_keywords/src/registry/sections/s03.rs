//! Registry `ALL` section 3/11 (SPEC-19 split; concatenated in `registry::ALL`).
#![allow(clippy::all, unused_imports)]

use super::super::builders::*;
use super::super::facets::*;
use crate::{CapabilitySpec, Context, DiagnosticFacet, SemanticToken, Sigil, Surface};

pub(crate) const ROWS: &[CapabilitySpec] = &[
    stmt(
        "allow_credentials",
        Context::Cors,
        "entity.name.function.statement.cors.lazuli",
        "Allow credentialed CORS requests.",
    ),
    stmt(
        "max_age",
        Context::Cors,
        "entity.name.function.statement.cors.lazuli",
        "CORS preflight max-age.",
    ),
    // ── app: route_guard block ──
    // Child keys the `app.route_guard` defaults block parser accepts
    // (`crates/lazuli_manifest/src/app_manifest/manifest_indent4.rs`
    // `Some("route_guard")` arm): `default_policy`, `on_unauthenticated`,
    // `on_unauthorized`, `skeleton`.
    stmt(
        "default_policy",
        Context::RouteGuard,
        "entity.name.function.statement.route-guard.lazuli",
        "App-level default route policy.",
    ),
    stmt(
        "on_unauthenticated",
        Context::RouteGuard,
        "entity.name.function.statement.route-guard.lazuli",
        "Default redirect when unauthenticated.",
    ),
    stmt(
        "on_unauthorized",
        Context::RouteGuard,
        "entity.name.function.statement.route-guard.lazuli",
        "Default redirect when unauthorized.",
    ),
    stmt(
        "skeleton",
        Context::RouteGuard,
        "entity.name.function.statement.route-guard.lazuli",
        "Default loading skeleton.",
    ),
    // ── app: error_page block ──
    // Child keys the `app.error_page <NNN>` block parser accepts
    // (`crates/lazuli_manifest/src/app_manifest/manifest_indent4.rs`
    // `Some("error_page")` arm): `template`, `audience`. Distinct from the
    // top-level `error_page` DECL (Context::TopLevel) above.
    stmt(
        "template",
        Context::ErrorPage,
        "entity.name.function.statement.error-page.lazuli",
        "Error-page template path.",
    ),
    stmt(
        "audience",
        Context::ErrorPage,
        "entity.name.function.statement.error-page.lazuli",
        "Error-page audience selector.",
    ),
    // ── app: logging block ──
    stmt(
        "level",
        Context::Logging,
        "entity.name.function.statement.logging.lazuli",
        "Log level.",
    ),
    stmt(
        "format",
        Context::Logging,
        "entity.name.function.statement.logging.lazuli",
        "Log format (json/text).",
    ),
    stmt(
        "redact",
        Context::Logging,
        "entity.name.function.statement.logging.lazuli",
        "Redacted fields.",
    ),
    stmt(
        "sample_rate",
        Context::Logging,
        "entity.name.function.statement.logging.lazuli",
        "Log sample rate.",
    ),
    // ── app: tracing block ──
    stmt(
        "propagate",
        Context::Tracing,
        "entity.name.function.statement.tracing.lazuli",
        "Trace-context propagation.",
    ),
    stmt(
        "exporter",
        Context::Tracing,
        "entity.name.function.statement.tracing.lazuli",
        "Trace exporter.",
    ),
    // ── app: runtime block ──
    stmt(
        "unit",
        Context::Runtime,
        "entity.name.function.statement.runtime.lazuli",
        "Runtime unit (process).",
    ),
    stmt(
        "serves",
        Context::Runtime,
        "entity.name.function.statement.runtime.lazuli",
        "What the unit serves.",
    ),
    stmt(
        "runs",
        Context::Runtime,
        "entity.name.function.statement.runtime.lazuli",
        "What the unit runs.",
    ),
    stmt(
        "healthcheck",
        Context::Runtime,
        "entity.name.function.statement.runtime.lazuli",
        "Healthcheck endpoint.",
    ),
    stmt(
        "readiness",
        Context::Runtime,
        "entity.name.function.statement.runtime.lazuli",
        "Readiness probe.",
    ),
    // ── app: deploy block ──
    stmt(
        "migrations",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Migration policy.",
    ),
    stmt(
        "migration_lock",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Migration advisory lock.",
    ),
    stmt(
        "destructive_migrations",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Destructive-migration policy.",
    ),
    stmt(
        "rollback",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Rollback policy.",
    ),
    stmt(
        "topology",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Deployment topology.",
    ),
    stmt(
        "strategy",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Deploy strategy (rolling/blue_green/canary).",
    ),
    stmt(
        "lock_timeout",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Migration lock timeout.",
    ),
    stmt(
        "pre_migration_hook",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Pre-migration hook.",
    ),
    stmt(
        "post_migration_hook",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Post-migration hook.",
    ),
    stmt(
        "checkpoint",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
        "Migration checkpoint.",
    ),
    // ── app: services + communication ──
    stmt(
        "service",
        Context::Services,
        "entity.name.function.statement.services.lazuli",
        "Declares a service.",
    ),
    stmt(
        "owns",
        Context::Services,
        "entity.name.function.statement.services.lazuli",
        "Resources a service owns.",
    ),
    stmt(
        "exposes",
        Context::Services,
        "entity.name.function.statement.services.lazuli",
        "Operations a service exposes.",
    ),
    stmt(
        "publishes",
        Context::Services,
        "entity.name.function.statement.services.lazuli",
        "Events a service publishes.",
    ),
    stmt(
        "consumes",
        Context::Services,
        "entity.name.function.statement.services.lazuli",
        "Events a service consumes.",
    ),
    stmt(
        "internal",
        Context::Communication,
        "entity.name.function.statement.communication.lazuli",
        "Internal communication mode.",
    ),
    stmt(
        "external",
        Context::Communication,
        "entity.name.function.statement.communication.lazuli",
        "External communication mode.",
    ),
    stmt(
        "async",
        Context::Communication,
        "entity.name.function.statement.communication.lazuli",
        "Async communication.",
    ),
    // ── app/registry: env block ──
    stmt(
        "group",
        Context::Env,
        "entity.name.function.statement.env.lazuli",
        "Env-var group.",
    ),
    stmt(
        "client",
        Context::Env,
        "entity.name.function.statement.env.lazuli",
        "Client-exposed env var.",
    ),
    stmt(
        "server",
        Context::Env,
        "entity.name.function.statement.env.lazuli",
        "Server-only env var.",
    ),
    // ── integrations block ──
    stmt(
        "adapter",
        Context::Integrations,
        "entity.name.function.statement.integration.lazuli",
        "Integration adapter.",
    ),
    stmt(
        "credentials",
        Context::Integrations,
        "entity.name.function.statement.integration.lazuli",
        "Integration credentials.",
    ),
    stmt(
        "data_classification",
        Context::Integrations,
        "entity.name.function.statement.integration.lazuli",
        "Data-classification tag.",
    ),
    stmt(
        "integration",
        Context::Integrations,
        "entity.name.label.integration.lazuli",
        "Named integration.",
    ),
];
