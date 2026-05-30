//! Registry `ALL` section 5/11 (SPEC-19 split; concatenated in `registry::ALL`).
#![allow(clippy::all, unused_imports)]

use crate::{CapabilitySpec, Context, DiagnosticFacet, SemanticToken, Sigil, Surface};
use super::super::builders::*;
use super::super::facets::*;

pub(crate) const ROWS: &[CapabilitySpec] = &[
    kw(
        "role",
        Context::FeatureHeader,
        DECL,
        "Feature-scoped RBAC role.",
    ),
    kw(
        "permission",
        Context::FeatureHeader,
        DECL,
        "Feature-scoped RBAC permission.",
    ),
    produces(
        kw(
            "invariants",
            Context::FeatureHeader,
            SECTION,
            "Declares the invariants block.",
        ),
        P_INVARIANTS,
    ),
    kw(
        "compatibility",
        Context::FeatureHeader,
        STMT,
        "Declares contract compatibility.",
    ),
    kw(
        "import",
        Context::FeatureHeader,
        STMT,
        "Imports a contract/operation.",
    ),
    kw(
        "imports",
        Context::FeatureHeader,
        STMT,
        "Declares feature imports.",
    ),
    kw(
        "operation",
        Context::FeatureHeader,
        DECL,
        "Declares a contract operation.",
    ),
    kw(
        "mcp_server",
        Context::FeatureHeader,
        DECL,
        "Declares an MCP server surface.",
    ),
    kw(
        "subscription",
        Context::FeatureHeader,
        DECL,
        "Declares an event subscription.",
    ),
    kw(
        "translation",
        Context::FeatureHeader,
        SECTION,
        "Declares a translation catalog.",
    ),
    produces(
        kw(
            "refs",
            Context::FeatureHeader,
            SECTION,
            "Declares cross-feature references.",
        ),
        P_CROSS_FEATURE,
    ),
    kw(
        "tenant_migration",
        Context::FeatureHeader,
        DECL,
        "Declares a tenant-data migration.",
    ),
    produces(
        dotted(
            "query.list",
            Context::FeatureHeader,
            "Declares a list query (collection projection).",
        ),
        P_QUERY,
    ),
    dotted(
        "query.lookup",
        Context::FeatureHeader,
        "Declares a lookup query (single-record fetch).",
    ),
    dotted(
        "query.sql",
        Context::FeatureHeader,
        "Declares a raw-SQL query.",
    ),
    dotted(
        "query.view",
        Context::FeatureHeader,
        "Declares a database-view-backed query.",
    ),
    dotted(
        "event.trace",
        Context::FeatureHeader,
        "Declares an event trace.",
    ),
    // feature-meta scalar lines (statements-feature-meta scope leaf)
    stmt(
        "requires",
        Context::FeatureHeader,
        "entity.name.function.statement.feature-meta.lazuli",
        "Feature dependency / requirement.",
    ),
    // ════════════════════════════════════════════════════════════════
    // Resource / aggregate / record body — field modifiers + relations
    // ════════════════════════════════════════════════════════════════
    produces(
        kw(
            "resource",
            Context::ResourceBody,
            DECL,
            "Declares a persisted resource.",
        ),
        P_RESOURCE,
    ),
    stmt(
        "tenancy",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Tenancy axis for the resource.",
    ),
    stmt(
        "timestamps",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Enable created/updated timestamps.",
    ),
    stmt(
        "soft_delete",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Enable soft-delete.",
    ),
    stmt(
        "retention",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Data-retention policy.",
    ),
    produces(
        stmt(
            "conventions",
            Context::ResourceBody,
            "entity.name.function.statement.resource.lazuli",
            "Opt into resource conventions.",
        ),
        P_CONVENTIONS,
    ),
    stmt(
        "paginate",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Default pagination.",
    ),
    stmt(
        "validates",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Field validation rule.",
    ),
    stmt(
        "validate",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Field validation rule.",
    ),
    produces(
        stmt(
            "unique",
            Context::ResourceBody,
            "entity.name.function.statement.resource.lazuli",
            "Unique constraint.",
        ),
        P_UNIQUE,
    ),
    stmt(
        "index",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Index declaration.",
    ),
    stmt(
        "on_delete",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Referential on-delete action.",
    ),
    produces(
        stmt(
            "derived",
            Context::ResourceBody,
            "entity.name.function.statement.resource.lazuli",
            "Derived/computed field.",
        ),
        P_DERIVED,
    ),
    stmt(
        "has_many",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "One-to-many relation.",
    ),
    stmt(
        "inverse",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Inverse relation side.",
    ),
    stmt(
        "previously",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Previous field name (rename).",
    ),
    stmt(
        "migrated",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Migration marker.",
    ),
    stmt(
        "alias",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Field alias.",
    ),
    produces(
        stmt(
            "composite_key",
            Context::ResourceBody,
            "entity.name.function.statement.resource.lazuli",
            "Composite primary key.",
        ),
        P_COMPOSITE_KEY,
    ),
    produces(
        stmt(
            "lock",
            Context::ResourceBody,
            "entity.name.function.statement.resource.lazuli",
            "Concurrency lock strategy.",
        ),
        P_LOCK,
    ),
    stmt(
        "invariant",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Resource invariant.",
    ),
    stmt(
        "fields",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Field group.",
    ),
    stmt(
        "primary",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Primary-key marker.",
    ),
    stmt(
        "field",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
        "Explicit field declaration.",
    ),
    stmt(
        "root",
        Context::ResourceBody,
        "keyword.control.statement.lazuli",
        "Aggregate root marker.",
    ),
];
