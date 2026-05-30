//! Scope-name consts + the `kw`/`stmt`/`value`/... `CapabilitySpec` builders.
//!
//! Split out of the former monolithic `registry.rs` (SPEC-19, <=500 LOC/file).
#![allow(dead_code)]

use crate::{CapabilitySpec, Context, DiagnosticFacet, SemanticToken, Sigil, Surface};

pub(crate) const DECL: &str = "keyword.control.declaration.structural.lazuli";
pub(crate) const SECTION: &str = "keyword.control.section.lazuli";
pub(crate) const STMT: &str = "keyword.control.statement.lazuli";
pub(crate) const MODIFIER: &str = "storage.modifier.lazuli";
pub(crate) const DECORATOR: &str = "entity.name.tag.decorator.lazuli";
/// SPEC-07 (B): the identity-axis catalog atoms `@role`/`@scope`/`@actor` carry
/// a scope leaf distinct from generic reference decorators so the sigil-straddle
/// (named feature-local reference vs app-level catalog atom) is named in the
/// generated `docs/keyword-reference.md` scope column, not just in prose.
pub(crate) const CATALOG_ATOM: &str = "entity.name.tag.catalog-atom.lazuli";
pub(crate) const OP_LOGICAL: &str = "keyword.operator.logical.lazuli";
pub(crate) const OP_PREDICATE: &str = "keyword.operator.predicate.lazuli";
pub(crate) const TYPE_CTOR: &str = "support.function.type-constructor.lazuli";

// ── per-block statement-scope leaves (used by the H2 tmLanguage backfill) ──
pub(crate) const TESTS: &str = "entity.name.function.statement.tests.lazuli";
pub(crate) const TRANSLATION: &str = "entity.name.function.statement.translation.lazuli";
pub(crate) const HEADERS: &str = "entity.name.function.statement.headers.lazuli";
pub(crate) const LIMITS: &str = "entity.name.function.statement.limits.lazuli";
pub(crate) const ENCRYPTION: &str = "entity.name.function.statement.encryption.lazuli";
pub(crate) const TRACING: &str = "entity.name.function.statement.tracing.lazuli";
pub(crate) const DEPLOY: &str = "entity.name.function.statement.deploy.lazuli";
pub(crate) const COMMUNICATION: &str = "entity.name.function.statement.communication.lazuli";
pub(crate) const ENV: &str = "entity.name.function.statement.env.lazuli";
pub(crate) const INTEGRATION: &str = "entity.name.function.statement.integration.lazuli";
pub(crate) const PACKS: &str = "entity.name.function.statement.packs.lazuli";
pub(crate) const DEFAULTS: &str = "entity.name.function.statement.defaults.lazuli";
pub(crate) const AUDIT: &str = "entity.name.function.statement.audit.lazuli";
pub(crate) const APPROVAL: &str = "entity.name.function.statement.approval.lazuli";
pub(crate) const REPLAY: &str = "entity.name.function.statement.replay.lazuli";

/// Build a bare-keyword `.lzi` capability row with no `produces` facet.
pub(crate) const fn kw(
    literal: &'static str,
    context: Context,
    scope: &'static str,
    hover: &'static str,
) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context,
        scope,
        token: SemanticToken::Keyword,
        surface: Surface::Lzi,
        sigil: None,
        hover,
        produces: &[],
    }
}

/// Per-block statement-keyword scope leaf (`entity.name.function.statement.<block>.lazuli`).
pub(crate) const fn stmt(
    literal: &'static str,
    context: Context,
    scope: &'static str,
    hover: &'static str,
) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context,
        scope,
        token: SemanticToken::Keyword,
        surface: Surface::Lzi,
        sigil: None,
        hover,
        produces: &[],
    }
}

/// A closed-catalog VALUE row (`constant.language.<catalog>.lazuli`).
pub(crate) const fn value(literal: &'static str, scope: &'static str) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context: Context::Value,
        scope,
        token: SemanticToken::EnumMember,
        surface: Surface::Lzi,
        sigil: None,
        hover: "",
        produces: &[],
    }
}

/// A cross-cutting `storage.modifier` word.
pub(crate) const fn modifier(literal: &'static str, hover: &'static str) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context: Context::Modifier,
        scope: MODIFIER,
        token: SemanticToken::Modifier,
        surface: Surface::Lzi,
        sigil: None,
        hover,
        produces: &[],
    }
}

/// A filter/policy/test expression operator or predicate.
pub(crate) const fn op(literal: &'static str, scope: &'static str) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context: Context::Expression,
        scope,
        token: SemanticToken::Operator,
        surface: Surface::Lzi,
        sigil: None,
        hover: "",
        produces: &[],
    }
}

/// An `@`-decorator namespace row.
pub(crate) const fn decorator(literal: &'static str, hover: &'static str) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context: Context::ResourceBody,
        scope: DECORATOR,
        token: SemanticToken::Decorator,
        surface: Surface::Lzi,
        sigil: Some(Sigil::At),
        hover,
        produces: &[],
    }
}

/// An identity-axis catalog-atom namespace row (`@role`/`@scope`/`@actor`).
///
/// SPEC-07 (B): distinguished from [`decorator`] (the feature-local named
/// reference `@policy.<category>`) — these resolve against an APP-LEVEL closed
/// identity catalog declared in the registry, not a feature-local block. Same
/// `@` sigil and LSP [`SemanticToken::Decorator`] token (they read as decorators
/// to a highlighter); the [`CATALOG_ATOM`] scope leaf is what names the kind in
/// the generated keyword reference. The split is enforced by
/// `identity_catalog_atoms_are_a_distinct_kind` in `lib_p2.rs`.
pub(crate) const fn catalog_atom(literal: &'static str, hover: &'static str) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context: Context::ResourceBody,
        scope: CATALOG_ATOM,
        token: SemanticToken::Decorator,
        surface: Surface::Lzi,
        sigil: Some(Sigil::At),
        hover,
        produces: &[],
    }
}

/// A dotted-kind feature-body declaration (`query.list`, `event.trace`).
pub(crate) const fn dotted(literal: &'static str, context: Context, hover: &'static str) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context,
        scope: DECL,
        token: SemanticToken::Keyword,
        surface: Surface::Lzi,
        sigil: Some(Sigil::DottedKind),
        hover,
        produces: &[],
    }
}

/// A `*.design.lzi`-surface keyword row.
pub(crate) const fn design(literal: &'static str, hover: &'static str) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context: Context::Design,
        scope: SECTION,
        token: SemanticToken::Keyword,
        surface: Surface::Design,
        sigil: None,
        hover,
        produces: &[],
    }
}

/// An `.lzx` surface-view keyword row.
pub(crate) const fn lzx(
    literal: &'static str,
    context: Context,
    scope: &'static str,
    hover: &'static str,
) -> CapabilitySpec {
    CapabilitySpec {
        literal,
        context,
        scope,
        token: SemanticToken::Keyword,
        surface: Surface::Lzx,
        sigil: None,
        hover,
        produces: &[],
    }
}
