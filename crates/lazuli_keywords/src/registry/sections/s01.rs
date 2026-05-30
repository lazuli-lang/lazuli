//! Registry `ALL` section 1/11 (SPEC-19 split; concatenated in `registry::ALL`).
#![allow(clippy::all, unused_imports)]

use super::super::builders::*;
use super::super::facets::*;
use crate::{CapabilitySpec, Context, DiagnosticFacet, SemanticToken, Sigil, Surface};

pub(crate) const ROWS: &[CapabilitySpec] = &[
    // ════════════════════════════════════════════════════════════════
    // Top-level declarations (indent 0) — Surface::App / Lzi
    // ════════════════════════════════════════════════════════════════
    CapabilitySpec {
        literal: "workspace",
        context: Context::TopLevel,
        scope: DECL,
        token: SemanticToken::Keyword,
        surface: Surface::App,
        sigil: None,
        hover: "Declares a multi-app workspace root.",
        produces: &[],
    },
    CapabilitySpec {
        literal: "app",
        context: Context::TopLevel,
        scope: DECL,
        token: SemanticToken::Keyword,
        surface: Surface::App,
        sigil: None,
        hover: "Declares an application: targets, urls, runtime, deploy topology.",
        produces: &[],
    },
    CapabilitySpec {
        literal: "registry",
        context: Context::TopLevel,
        scope: DECL,
        token: SemanticToken::Keyword,
        surface: Surface::Registry,
        sigil: None,
        hover: "Declares the shared integration/capability registry.",
        produces: &[],
    },
    CapabilitySpec {
        literal: "profile",
        context: Context::TopLevel,
        scope: DECL,
        token: SemanticToken::Keyword,
        surface: Surface::App,
        sigil: None,
        hover: "Declares a deployment/runtime profile.",
        produces: &[],
    },
    kw(
        "feature",
        Context::TopLevel,
        DECL,
        "Declares a feature: the unit of business capability.",
    ),
    produces(
        kw(
            "design",
            Context::TopLevel,
            DECL,
            "Declares the project-root design token catalog.",
        ),
        P_DESIGN,
    ),
    kw(
        "plan",
        Context::TopLevel,
        DECL,
        "Declares a billing/entitlement plan.",
    ),
    kw(
        "gate",
        Context::TopLevel,
        DECL,
        "Top-level feature/limit gating directive.",
    ),
    produces(
        kw(
            "route",
            Context::TopLevel,
            DECL,
            "Top-level route declaration.",
        ),
        P_ROUTE,
    ),
    kw(
        "permission",
        Context::TopLevel,
        DECL,
        "RBAC permission catalog entry.",
    ),
    kw("role", Context::TopLevel, DECL, "RBAC role catalog entry."),
    kw(
        "experience",
        Context::TopLevel,
        DECL,
        "Declares a shared surface experience.",
    ),
    kw(
        "contract",
        Context::TopLevel,
        DECL,
        "Declares a service contract.",
    ),
    kw(
        "error_page",
        Context::TopLevel,
        DECL,
        "Declares an app-level error page.",
    ),
    kw(
        "escape_route",
        Context::TopLevel,
        DECL,
        "Documents a deliberate framework escape hatch.",
    ),
    kw(
        "shared_registry",
        Context::TopLevel,
        DECL,
        "Workspace-level shared registry reference.",
    ),
    kw(
        "apps",
        Context::TopLevel,
        SECTION,
        "Workspace `apps` listing block.",
    ),
    kw(
        "boundaries",
        Context::TopLevel,
        SECTION,
        "Workspace service-boundary declarations.",
    ),
    kw(
        "gateway",
        Context::TopLevel,
        SECTION,
        "Workspace API gateway block.",
    ),
    kw("grants", Context::TopLevel, STMT, "RBAC grant statement."),
    kw(
        "grants_all",
        Context::TopLevel,
        STMT,
        "RBAC grant-all statement.",
    ),
    kw(
        "revoke_user",
        Context::TopLevel,
        STMT,
        "RBAC revoke-user action.",
    ),
    kw(
        "revoke_session_family",
        Context::TopLevel,
        STMT,
        "RBAC revoke-session-family action.",
    ),
    kw(
        "skeleton",
        Context::TopLevel,
        SECTION,
        "Package skeleton block.",
    ),
    // ════════════════════════════════════════════════════════════════
    // App body (indent-2 kinds + app-meta) — APP_BODY_KINDS
    // ════════════════════════════════════════════════════════════════
    kw(
        "architecture",
        Context::App,
        SECTION,
        "App architecture / service-boundary block.",
    ),
    kw(
        "actor_query",
        Context::App,
        STMT,
        "App-level actor resolution query.",
    ),
    kw(
        "auth_failed_redirect",
        Context::App,
        STMT,
        "Redirect target on auth failure.",
    ),
    kw(
        "bindings",
        Context::App,
        SECTION,
        "Registry binding overrides for this app.",
    ),
    kw(
        "capabilities",
        Context::App,
        SECTION,
        "App capability declarations.",
    ),
    kw(
        "communication",
        Context::App,
        SECTION,
        "Inter-service communication block.",
    ),
    kw(
        "cookie",
        Context::App,
        SECTION,
        "App cookie defaults block.",
    ),
    kw("cors", Context::App, SECTION, "CORS configuration block."),
    kw(
        "default_locale",
        Context::App,
        STMT,
        "Default locale for the app.",
    ),
    kw(
        "default_timezone",
        Context::App,
        STMT,
        "Default timezone for the app.",
    ),
    kw(
        "deploy",
        Context::App,
        SECTION,
        "Deployment topology + migration policy block.",
    ),
    produces(
        kw(
            "encryption",
            Context::App,
            SECTION,
            "Field-encryption configuration block.",
        ),
        P_ENCRYPTION,
    ),
    kw(
        "environments",
        Context::App,
        SECTION,
        "Named environment declarations.",
    ),
    kw(
        "headers",
        Context::App,
        SECTION,
        "Security/response header defaults block.",
    ),
    kw(
        "integrations",
        Context::App,
        SECTION,
        "Third-party integration declarations.",
    ),
    kw(
        "lazuli_version",
        Context::App,
        STMT,
        "Pinned Lazuli framework version.",
    ),
    kw(
        "limits",
        Context::App,
        SECTION,
        "Request/body size + timeout limits block.",
    ),
    kw("locale", Context::App, SECTION, "Locale negotiation block."),
    kw(
        "logging",
        Context::App,
        SECTION,
        "Structured logging configuration block.",
    ),
    kw(
        "not_found",
        Context::App,
        STMT,
        "404 / not-found handler reference.",
    ),
    kw(
        "packs",
        Context::App,
        SECTION,
        "Feature-pack inclusion block.",
    ),
];
