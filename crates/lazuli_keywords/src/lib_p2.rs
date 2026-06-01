/// The surface family a capability belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Surface {
    /// Canonical `.lzi` source.
    Lzi,
    /// Surface `.lzx` source.
    Lzx,
    /// App-manifest / top-level orchestration (`app`, `registry`,
    /// `workspace`, `profile`).
    App,
    /// Registry source.
    Registry,
    /// `Lazurite.toml` manifest overlay.
    Manifest,
    /// `design.lzi` token catalog.
    Design,
}

/// The decorator/namespace sigil a literal carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Sigil {
    /// An `@`-prefixed decorator namespace (`@policy`, `@scope`, `@fn`,
    /// `@slug`, `@semantic`, ...).
    At,
    /// A dotted kind keyword (`query.list`, `event.trace`).
    DottedKind,
}

/// The LSP semantic-token type a literal classifies as. Projected to the
/// LSP standard token-type set for the `SemanticTokensLegend` in Wave H4;
/// the legend order is `SemanticToken as usize` so it never reshuffles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum SemanticToken {
    /// Control/declaration/statement keyword.
    Keyword,
    /// Built-in or user-declared type name.
    Type,
    /// Named-block name / function-like construct.
    Function,
    /// Package/import namespace reference.
    Namespace,
    /// `@`-decorator.
    Decorator,
    /// Modifier word.
    Modifier,
    /// Closed-catalog enum-member value.
    EnumMember,
    /// String literal.
    String,
    /// Comment.
    Comment,
    /// Operator / predicate.
    Operator,
    /// Bound variable / identifier.
    Variable,
    /// Field / property name.
    Property,
}

impl CapabilitySpec {
    /// Whether this capability is a bare keyword (no sigil).
    pub const fn is_bare_keyword(&self) -> bool {
        self.sigil.is_none()
    }
}

/// Every distinct keyword literal in the registry, for membership checks
/// (used by the proven-complete parser test and downstream surfaces).
pub fn all_literals() -> impl Iterator<Item = &'static str> {
    ALL.iter().map(|c| c.literal)
}

/// Look up the first [`CapabilitySpec`] whose literal matches `literal`
/// in any context. Convenience for hover/completion lookups that don't
/// care about context.
pub fn find(literal: &str) -> Option<&'static CapabilitySpec> {
    ALL.iter().find(|c| c.literal == literal)
}

/// The closed catalog of `@`-reference namespaces — the **single source** for
/// "is `@<ns>.<target>` a valid reference?". `lazuli_lsp` (`vocab.rs`) and
/// `lazuli_doctor` (`refs.rs`) derive their allow-checks from this instead of
/// each keeping a divergent hand-maintained copy: before this, the LSP allowed
/// 23 namespaces and the doctor only 18 (silently rejecting `@feature` /
/// `@translation` / `@file` / `@audience` that the LSP accepted).
///
/// Gated against the registry's `@`-decorator rows by the
/// `reference_namespaces_cover_registry_decorators` test (below): every decorator
/// that is not a bare field-marker (`@slug` / `@full_text` / `@owner_axis`) or
/// the `@resume` flow-sigil must appear here, so a new `@`-namespace cannot
/// enter the registry without surfacing in this catalog.
pub const REFERENCE_NAMESPACES: &[&str] = &[
    "role", "scope", "actor", "policy", // identity / authorization
    "semantic", "cap", "pii", "key", // data classification / typed
    "fn", "hook", "validator", "adapter", // extension surface
    "client", "query_modifier", "anchor", //
    "llm", "tool", // AI
    "trace", "translation", "feature", // observability / i18n / cross-feature
    // SPEC-02 — `@command` retired (commands are referenced bare everywhere;
    // the only sigil use, `view.inline_table on_change @command.<name>`, now
    // takes a bare command name).
    "file", "audience", // named references
];

/// Whether `ns` (the segment after `@`, e.g. `"policy"` in `@policy.update`) is
/// a valid reference namespace. The closed-catalog contract — unknown
/// namespaces are diagnostics. See [`REFERENCE_NAMESPACES`].
pub fn is_reference_namespace(ns: &str) -> bool {
    REFERENCE_NAMESPACES.contains(&ns)
}

/// The closed scalar type catalog (bare PascalCase). Single source for the
/// generated catalog docs and the analyzer's membership gate. Authored as the
/// canonical spelling only — aliases live in [`SCALAR_ALIASES`].
pub const SCALAR_TYPES: &[&str] =
    &["ID", "Text", "Boolean", "Integer", "Decimal", "Date", "DateTime", "JSON"];

/// The closed semantic-scalar catalog. Today these are spelled `@semantic.<X>`;
/// SPEC-04 retires the sigil and they become bare reserved type names (which is
/// why they live in the type catalog, not the decorator rows).
pub const SEMANTIC_TYPES: &[&str] = &[
    "Email",
    "Phone",
    "Url",
    "Uuid",
    "Currency",
    "GeoPoint",
    "HexColor",
    "Percentage",
    // Batch E — strictly-positive decimal (`> 0`) + non-negative integer
    // (`>= 0`) range scalars; pilots hand-wrote these as `> 0` / `>= 0`
    // validators on prices/amounts/counts.
    "PositiveDecimal",
    "NonNegativeInt",
    "Money",
];

/// Non-canonical scalar aliases the analyzer historically resolved **silently**
/// (the bug SPEC-01 ends): `Int`->`Integer`, `Bool`->`Boolean`, etc. They map
/// alias -> canonical so `VOCAB-SCALAR-ALIAS-001` can name the fix and
/// `lazuli fmt` can normalize. Reject+autocorrect, never silently tolerate.
pub const SCALAR_ALIASES: &[(&str, &str)] = &[
    ("Id", "ID"),
    ("String", "Text"),
    ("Bool", "Boolean"),
    ("Int", "Integer"),
    ("Float", "Decimal"),
    ("Json", "JSON"),
];

/// The canonical scalar spelling for `name` if it is a known alias, else `None`.
pub fn canonical_scalar(name: &str) -> Option<&'static str> {
    SCALAR_ALIASES
        .iter()
        .find(|(alias, _)| *alias == name)
        .map(|(_, canonical)| *canonical)
}

/// Non-canonical semantic-scalar spelling aliases that downstream surfaces
/// (codegen `emitter::handlers::types`) historically accepted, mapping
/// alias -> canonical [`SEMANTIC_TYPES`] spelling. Kept so the doctor's
/// membership gate agrees with codegen's tolerance instead of false-rejecting
/// an upper-cased acronym. `Url`/`Uuid` are the canonical spellings; the
/// all-caps acronym forms are the tolerated aliases.
pub const SEMANTIC_TYPE_ALIASES: &[(&str, &str)] = &[("URL", "Url"), ("UUID", "Uuid")];

/// Whether `name` is a recognized semantic-scalar type name (bare, e.g.
/// `"PositiveDecimal"`, or its tolerated alias). Single source of truth for
/// every membership gate (analyzer lowering, doctor's `@semantic.<X>` closed
/// catalog check) so the closed catalog can NEVER drift between surfaces:
/// add a scalar to [`SEMANTIC_TYPES`] and every consumer learns it at once.
pub fn is_semantic_type(name: &str) -> bool {
    SEMANTIC_TYPES.contains(&name)
        || SEMANTIC_TYPE_ALIASES.iter().any(|(alias, _)| *alias == name)
}

/// The app-manifest block-header name a [`Context`] models, if the context
/// IS an app-manifest indent-2 block whose indent-4 children are registry
/// rows. Returns `None` for every context that is not an app-manifest
/// child-key block (feature bodies, surfaces, value catalogs, …).
///
/// This is the **single source of truth** mapping a `Context` variant to
/// the `app <Name>` block header string the parser dispatches on (the
/// `state.current_child` literal in
/// `crates/lazuli_manifest/src/app_manifest/manifest_indent4.rs`). The LSP
/// operational walker and the anti-drift gate both derive their
/// validated-block set from this function via [`manifest_child_keys`], so
/// the walker, the registry, and the parser can never silently diverge.
///
/// Only the contexts whose indent-4 children are declared as registry rows
/// appear here; a block whose children are not yet registry-backed (so it
/// has no closed catalog to validate against) is deliberately absent.
pub const fn manifest_block_name(context: Context) -> Option<&'static str> {
    match context {
        Context::Cookie => Some("cookie"),
        Context::Headers => Some("headers"),
        Context::Limits => Some("limits"),
        Context::Proxy => Some("proxy"),
        Context::Cors => Some("cors"),
        Context::RouteGuard => Some("route_guard"),
        Context::Encryption => Some("encryption"),
        Context::Locale => Some("locale"),
        Context::Logging => Some("logging"),
        Context::Tracing => Some("tracing"),
        Context::ErrorPage => Some("error_page"),
        _ => None,
    }
}

/// The valid indent-4 child-key literals for an app-manifest `block`
/// header, derived from the [`ALL`] registry by filtering on
/// [`manifest_block_name`]. Returns an empty iterator for an unknown /
/// non-child-key block — callers treat "catalog non-empty" as the gate to
/// validate against it (so a block with no registry rows never false-fires
/// an unknown-child diagnostic).
///
/// This is the catalog the LSP `validate_app_block_child` walker looks up
/// per block, and the set the anti-drift gate
/// (`every_manifest_block_has_child_key_validation`) asserts non-empty for
/// every walker-validated block.
pub fn manifest_child_keys(block: &str) -> impl Iterator<Item = &'static str> {
    ALL.iter()
        .filter(move |c| manifest_block_name(c.context) == Some(block))
        .map(|c| c.literal)
}

#[cfg(test)]
mod catalog_tests {
    use super::*;

    #[test]
    fn is_semantic_type_covers_catalog_and_aliases_resolve_to_canonical() {
        // Every canonical entry is recognized — the SoT the doctor derives from.
        for s in SEMANTIC_TYPES {
            assert!(is_semantic_type(s), "{s} must be a recognized semantic type");
        }
        // Each alias resolves to a canonical spelling AND is itself recognized.
        for (alias, canonical) in SEMANTIC_TYPE_ALIASES {
            assert!(
                SEMANTIC_TYPES.contains(canonical),
                "semantic alias {alias} maps to {canonical}, not a canonical semantic type",
            );
            assert!(
                !SEMANTIC_TYPES.contains(alias),
                "alias {alias} must not also be a canonical semantic type name",
            );
            assert!(is_semantic_type(alias), "alias {alias} must be recognized");
        }
        // Negative: a scalar misused with `@semantic.` (e.g. JSON/Date) is NOT a
        // semantic type — those belong to SCALAR_TYPES.
        assert!(!is_semantic_type("JSON"));
        assert!(!is_semantic_type("Date"));
        assert!(!is_semantic_type("Nonsense"));
    }

    #[test]
    fn scalar_and_semantic_catalogs_are_disjoint_and_canonical() {
        for s in SEMANTIC_TYPES {
            assert!(!SCALAR_TYPES.contains(s), "{s} is in both scalar and semantic");
        }
        for (alias, canonical) in SCALAR_ALIASES {
            assert!(
                SCALAR_TYPES.contains(canonical),
                "alias {alias} maps to {canonical}, which is not a canonical scalar",
            );
            assert!(
                !SCALAR_TYPES.contains(alias) && !SEMANTIC_TYPES.contains(alias),
                "alias {alias} must not also be a canonical type name",
            );
        }
    }

    /// Registry `@`-decorators that are NOT `.target` reference namespaces:
    /// bare field flags + the `@resume` lifecycle flow-sigil.
    const NON_REFERENCE_DECORATORS: &[&str] = &["slug", "full_text", "owner_axis", "resume"];

    /// SPEC-07 (B): the identity-axis catalog atoms `@role`/`@scope`/`@actor`
    /// carry a scope leaf distinct from the feature-local named reference
    /// `@policy`, so the named-ref-vs-catalog-atom kind is visible in the
    /// generated `docs/keyword-reference.md` scope column (not just prose). The
    /// shared identity/authorization axis is unaffected — all four stay `@`
    /// reference namespaces; only the kind distinction is named here.
    #[test]
    fn identity_catalog_atoms_are_a_distinct_kind() {
        const CATALOG_ATOM: &str = "entity.name.tag.catalog-atom.lazuli";
        const DECORATOR: &str = "entity.name.tag.decorator.lazuli";
        let scope_of = |lit: &str| ALL.iter().find(|c| c.literal == lit).map(|c| c.scope);
        for atom in ["@role", "@scope", "@actor"] {
            assert_eq!(
                scope_of(atom),
                Some(CATALOG_ATOM),
                "{atom} must be a catalog-atom kind (SPEC-07 B) — use `catalog_atom(...)`",
            );
        }
        assert_eq!(
            scope_of("@policy"),
            Some(DECORATOR),
            "@policy is the feature-local named reference — it stays a `decorator(...)` kind",
        );
    }

    #[test]
    fn reference_namespaces_cover_registry_decorators() {
        for spec in ALL.iter() {
            let lit = spec.literal;
            if !lit.starts_with('@') || lit.contains('/') {
                continue; // skip non-decorators + `@runtime/…` pack refs
            }
            let ns = lit.trim_start_matches('@');
            if NON_REFERENCE_DECORATORS.contains(&ns) {
                continue;
            }
            assert!(
                REFERENCE_NAMESPACES.contains(&ns),
                "registry decorator `@{ns}` is missing from REFERENCE_NAMESPACES \
                 (catalog drift) — add it to the catalog or to NON_REFERENCE_DECORATORS",
            );
        }
    }
}
