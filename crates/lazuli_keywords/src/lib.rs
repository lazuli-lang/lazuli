//! `lazuli_keywords` — the single canonical source of truth for every
//! Lazuli language keyword/construct and its presentation + LSP metadata.
//!
//! This crate is a **pure-data leaf**. It depends on nothing internal
//! (optionally `serde` for serialization); `lazuli_syntax`, `lazuli_lsp`,
//! the tmLanguage generator, and the parity tests all depend *on* it,
//! never the reverse. Keeping it a leaf is what lets `lazuli_syntax`
//! (the parser) reference it without a dependency cycle.
//!
//! The optional `serde` feature derives **`Serialize` only** — the
//! registry is a `const` table of `&'static str` references, so the
//! Wave-H2 tmLanguage generator can project it to JSON, but there is
//! nothing to deserialize *into* (you can't borrow `'static` from a
//! decoder).
//!
//! ## What lives here
//!
//! [`ALL`] is the one iterable every downstream surface derives-from or is
//! asserted-against:
//!
//! * the **parser** is proven-complete against it — every keyword literal
//!   the hand-rolled recursive-descent parser recognizes must appear in
//!   [`ALL`] (enforced by `tests/proven_complete.rs`, Wave H1);
//! * the **tmLanguage** keyword-alternation rules are generated from the
//!   [`CapabilitySpec::scope`] field (Wave H2, not yet built);
//! * the **LSP** keyword catalog / `*_BODY_KINDS` / hover one-liners are
//!   generated/asserted from [`CapabilitySpec`] (Wave H3, not yet built);
//! * the **semantic-token** legend is the [`SemanticToken`] projection
//!   (Wave H4, not yet built);
//! * the **diagnostic codes** a capability produces are the [`produces`]
//!   facet — backfilled in Wave C1 and asserted coherent + complete by the
//!   `lazuli_diagnostics_registry` bridge crate.
//!
//! [`produces`]: CapabilitySpec::produces
//!
//! ## Context-as-data
//!
//! A literal valid in N contexts with different scopes is N separate
//! [`CapabilitySpec`] rows — e.g. `default` inside a `cookie` block vs an
//! `errors` block vs an `env` block. This mirrors exactly how the
//! tmLanguage already disambiguates (`entity.name.function.statement.cookie`
//! vs `.errors`) and how the LSP `*_BODY_KINDS` already partition the
//! vocabulary. Context-sensitive scoping therefore falls out of the data
//! rather than needing per-call-site logic.

#![forbid(unsafe_code)]

mod registry;

pub use registry::ALL;

/// One language capability: a keyword/construct the parser recognizes,
/// declared once per `(literal, context)` pair with everything every
/// downstream surface needs.
///
/// Field types are deliberately `&'static str` for the textual facets
/// (`literal`, `scope`, `hover`) so the registry is a `const` table with
/// zero runtime cost and no allocation; the categorical facets
/// (`context`, `surface`, `sigil`, `token`) are `Copy` enums so consumers
/// can `match` on them exhaustively at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CapabilitySpec {
    /// The literal the parser recognizes: `"command"`, `"query.list"`,
    /// `"many_through"`, `"@policy"`. Dotted kinds (`query.list`) and
    /// `@`-sigil decorators (`@slug`) keep their full form here; the
    /// proven-complete test reduces a parser literal to its head token
    /// before checking membership, so a multi-word parser phrase like
    /// `uses experience` matches the `uses` row.
    pub literal: &'static str,

    /// The block/scope the literal is valid in. The same literal in two
    /// contexts is two rows.
    pub context: Context,

    /// The TextMate scope leaf this literal gets in the given context,
    /// e.g. `keyword.control.statement.lazuli` or
    /// `entity.name.function.statement.cookie.lazuli`. Mirrors the scope
    /// the hand-written tmLanguage currently assigns (SCOPES.md taxonomy).
    pub scope: &'static str,

    /// The LSP semantic-token type this literal classifies as (Wave H4).
    pub token: SemanticToken,

    /// Which surface family the literal belongs to.
    pub surface: Surface,

    /// The decorator/namespace sigil, if any. `None` for a bare keyword;
    /// `Some(Sigil::At)` for `@`-decorators (`@policy`, `@slug`);
    /// `Some(Sigil::DottedKind)` for dotted kind keywords (`query.list`,
    /// `event.trace`).
    pub sigil: Option<Sigil>,

    /// One-line hover documentation. Feeds the LSP `keyword_description`
    /// table. Empty string when no curated description exists.
    pub hover: &'static str,

    /// The diagnostic codes this capability can produce. Backfilled in
    /// Wave C1: each facet mirrors a live `lazuli_doctor` rule `CODE` const.
    /// Empty for purely structural keywords that never gate a diagnostic.
    /// Carried as string-mirror refs ([`DiagnosticFacet`]) so this leaf
    /// stays free of `lazuli_doctor`; the `lazuli_diagnostics_registry`
    /// bridge crate (над `lazuli_doctor`) asserts coherence + completeness.
    pub produces: &'static [DiagnosticFacet],
}

/// A diagnostic-code facet of a capability — a string-mirror reference to
/// a doctor rule, NOT a dependency on `lazuli_doctor`. Backfilled in Wave
/// C1; `lazuli_diagnostics_registry` (над `lazuli_doctor`) asserts each
/// facet resolves to a live rule `CODE`, that `category` matches
/// `from_code_prefix(code)`, and that `base_severity` matches the rule's
/// authored base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DiagnosticFacet {
    /// The diagnostic code, e.g. `"COMPUTED-DATE-EXPR-001"`.
    pub code: &'static str,
    /// The rule's authored base severity — string mirror of
    /// `DoctorSeverity` (`"error"` | `"warning"` | `"info"` | `"hint"`).
    pub base_severity: &'static str,
    /// The rule's category — string mirror of `RuleCategory`, asserted
    /// `== from_code_prefix(code)` by `lazuli_diagnostics_registry`.
    pub category: &'static str,
}

/// Cross-cutting diagnostic codes that are NOT produced by a single
/// language keyword/capability — they guard framework-wide or
/// generated-artifact concerns rather than one DSL construct:
///
/// * **migration codegen** (`MIGRATION-*`, `@correctness.migration_out_of_sync`,
///   `@info.record_column_jsonb`, `RUNTIME-UPDATE-BUILDER-JSONB-001`) — emitted
///   over the generated SQL/Go, not a `.lzi` keyword;
/// * **framework-internal hygiene** (`INTERNAL-*`) — fires under
///   `lazuli doctor --self` against `crates/lazuli_*/src/`, auditing the
///   framework's own Rust, not user DSL;
/// * **`.lzi` source-shape hygiene** (`LZI-*`) — file size / name alignment /
///   cohesion, a property of the file, not any one construct;
/// * **doctor-meta** (`DOCTOR-OVERRIDE-NEEDS-REASON-001`) — guards the
///   `Lazurite.toml` severity-override table itself;
/// * **CRUD / actor synthesis** (`crud_synth_*`, `me_synth_*`) — fire during
///   command synthesis spanning resource + policy + handler, not bound to a
///   single keyword.
///
/// This is a documented home, NOT a dumping ground: the
/// `lazuli_diagnostics_registry` bridge asserts every code here resolves to a
/// live rule and is claimed *exactly once* (here OR by a capability's
/// `produces`). A future diagnostic added without a capability home must land
/// here explicitly or the `complete` test fails the build.
pub const GLOBAL_DIAGNOSTICS: &[DiagnosticFacet] = &[
    // ── migration codegen / runtime update-builder (over generated artifacts) ──
    DiagnosticFacet {
        code: "MIGRATION-ALTER-MISSING-001",
        base_severity: "error",
        category: "vocabulary",
    },
    DiagnosticFacet {
        code: "MIGRATION-DSL-UNIQUE-001",
        base_severity: "error",
        category: "vocabulary",
    },
    DiagnosticFacet {
        code: "MIGRATION-IDEMPOTENT-CREATE-001",
        base_severity: "warning",
        category: "vocabulary",
    },
    DiagnosticFacet {
        code: "RUNTIME-UPDATE-BUILDER-JSONB-001",
        base_severity: "error",
        category: "vocabulary",
    },
    DiagnosticFacet {
        code: "@correctness.migration_out_of_sync",
        base_severity: "error",
        category: "vocabulary",
    },
    DiagnosticFacet {
        code: "@info.record_column_jsonb",
        base_severity: "info",
        category: "vocabulary",
    },
    // ── framework-internal hygiene (`--self`, audits framework Rust) ──
    DiagnosticFacet {
        code: "INTERNAL-FILE-SIZE-001",
        base_severity: "warning",
        category: "internal_hygiene",
    },
    DiagnosticFacet {
        code: "INTERNAL-NO-EXAMPLE-001",
        base_severity: "warning",
        category: "internal_hygiene",
    },
    DiagnosticFacet {
        code: "INTERNAL-TEST-PAIRING-001",
        base_severity: "warning",
        category: "internal_hygiene",
    },
    DiagnosticFacet {
        code: "INTERNAL-UNDOC-PUB-001",
        base_severity: "warning",
        category: "internal_hygiene",
    },
    DiagnosticFacet {
        code: "INTERNAL-ERROR-NAMING-001",
        base_severity: "warning",
        category: "error_handling",
    },
    DiagnosticFacet {
        code: "INTERNAL-ERROR-NON-EXHAUSTIVE-001",
        base_severity: "warning",
        category: "error_handling",
    },
    DiagnosticFacet {
        code: "INTERNAL-ERROR-VARIANT-DOC-001",
        base_severity: "warning",
        category: "error_handling",
    },
    DiagnosticFacet {
        code: "INTERNAL-PANIC-UNWRAP-001",
        base_severity: "warning",
        category: "error_handling",
    },
    // ── `.lzi` source-shape hygiene (file property, not a construct) ──
    DiagnosticFacet {
        code: "LZI-FILE-SIZE-001",
        base_severity: "warning",
        category: "lzi_hygiene",
    },
    DiagnosticFacet {
        code: "LZI-FEATURE-NAMING-MATCHES-FILE-001",
        base_severity: "warning",
        category: "lzi_hygiene",
    },
    DiagnosticFacet {
        code: "LZI-FEATURE-COHESION-001",
        base_severity: "warning",
        category: "lzi_hygiene",
    },
    // ── doctor-meta (guards the override table itself) ──
    DiagnosticFacet {
        code: "DOCTOR-OVERRIDE-NEEDS-REASON-001",
        base_severity: "error",
        category: "test_discipline",
    },
    // ── CRUD / actor synthesis (spans resource + policy + handler) ──
    DiagnosticFacet {
        code: "crud_synth_no_required_fields",
        base_severity: "error",
        category: "vocabulary",
    },
    DiagnosticFacet {
        code: "crud_synth_policy_not_found",
        base_severity: "error",
        category: "vocabulary",
    },
    DiagnosticFacet {
        code: "crud_synth_signature_mismatch",
        base_severity: "error",
        category: "vocabulary",
    },
    DiagnosticFacet {
        code: "me_synth_no_actor_resolution",
        base_severity: "error",
        category: "vocabulary",
    },
    DiagnosticFacet {
        code: "me_synth_signature_mismatch",
        base_severity: "error",
        category: "vocabulary",
    },
];

/// The block/scope a capability is valid in. Mirrors the LSP
/// `*_BODY_KINDS` / `*_STATEMENT_KINDS` partitions and the tmLanguage
/// per-block scope leaves. New contexts are additive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum Context {
    // ── top-level declaration headers (indent 0) ──
    /// Top-level structural declaration keyword (`feature`, `app`,
    /// `workspace`, `registry`, `design`, `plan`, `gate`, `escape_route`,
    /// `permission`, `role`, `route`, `experience`, `profile`, `contract`).
    TopLevel,
    /// Inside an `app <name>` block (indent-2 children + app-meta lines).
    App,
    /// Inside the `registry` block.
    Registry,

    // ── feature body + its members ──
    /// Inside a `feature <name>` block (indent-2 kind keywords + meta).
    FeatureHeader,
    /// Inside a `resource`/`aggregate`/`entity`/`record` body — field
    /// modifiers, relations, invariants, conventions.
    ResourceBody,
    /// Inside an `enum <name>` body (member identifiers).
    EnumBody,
    /// Inside a `command <name>` body (indent-4 statements).
    CommandBody,
    /// Inside a `query.list`/`query.lookup`/`query.sql`/`query.view` body.
    Query,
    /// Inside a `job <name>` body.
    Job,
    /// Inside a `webhook <name>` body (+ `verify`/`replay`/`dlq` sub).
    Webhook,
    /// Inside an `agent <name>` body (+ `tools`/`expose`/`evals`/io sub).
    Agent,
    /// Inside a `notification <name>` body (+ `digest`/`throttle` sub).
    Notification,
    /// Inside a `poller <name>` body.
    Poller,
    /// Inside a `report <name>` body (+ `columns` sub).
    Report,
    /// Inside a `channel <name>` body.
    Channel,
    /// Inside a `tenant_migration <name>` body.
    TenantMigration,
    /// Inside an `api <name>` / `operation <name>` body.
    Api,
    /// Inside an `mcp_server <name>` body.
    McpServer,

    // ── shared feature sub-blocks ──
    /// Inside a `lifecycle <name>` block (states/transitions).
    Lifecycle,
    /// Inside an `audit ...` block (+ `emit_to`).
    Audit,
    /// Inside an `approval` block.
    Approval,
    /// Inside a `deprecated` sub-block (indent-6 children `since` /
    /// `replacement` / `sunset` of a `command`/`api` `deprecated` block —
    /// `parse_deprecated_block` / `parse_command_deprecated`).
    Deprecated,
    /// Inside a `policies`/`policy <expr>` block.
    Policy,
    /// Inside an `errors` block.
    Errors,
    /// Inside a `defaults` block.
    Defaults,
    /// Inside an `invariants` block.
    Invariants,
    /// Inside an `emits` block / event sub-statements.
    Emits,
    /// Inside an `event_group` / `event.trace` block.
    EventGroup,
    /// Inside a `cache <name>` profile block.
    Cache,
    /// Inside a `tests`/`evals` block.
    Tests,
    /// Inside a typed-extension declaration (`fn`/`hook`/`validator`/...).
    Extensions,
    /// Inside a `translation` block.
    Translation,
    /// Inside the `auth` block (+ identity/password/oauth/mfa/sessions).
    Auth,

    // ── app sub-blocks ──
    /// Inside an app `cookie` block.
    Cookie,
    /// Inside an app `headers` block.
    Headers,
    /// Inside an app `limits` block.
    Limits,
    /// Inside an app `proxy` block.
    Proxy,
    /// Inside an app `cors` block (child keys `allow_origins` /
    /// `allow_credentials` / `max_age`).
    Cors,
    /// Inside an app `route_guard` defaults block (child keys
    /// `default_policy` / `on_unauthenticated` / `on_unauthorized` /
    /// `skeleton`).
    RouteGuard,
    /// Inside an app `encryption` block.
    Encryption,
    /// Inside an app `locale` block.
    Locale,
    /// Inside an app `logging` block.
    Logging,
    /// Inside an app `tracing` block.
    Tracing,
    /// Inside an app `runtime` block.
    Runtime,
    /// Inside an app `deploy` block.
    Deploy,
    /// Inside an app `services` block.
    Services,
    /// Inside an app `communication` block.
    Communication,
    /// Inside an app `urls`/`environments` block.
    Urls,
    /// Inside an app/registry `env` block.
    Env,
    /// Inside an app/registry `integrations` block.
    Integrations,
    /// Inside an app/registry `capabilities` block.
    Capabilities,
    /// Inside a registry/app `bindings` block.
    Bindings,
    /// Inside a `packs` block.
    Packs,
    /// Inside an app `architecture` block.
    Architecture,
    /// Inside a `secret_rotation` block.
    SecretRotation,
    /// Inside an `error_page` block.
    ErrorPage,

    // ── surface (`.lzx`) ──
    /// Inside a `surface <name> <platform>` body (audience/uses/view).
    Surface,
    /// Inside an `audience <name>` block.
    SurfaceAudience,
    /// Inside a `view <name>` block (list/detail/create + UX primitives).
    SurfaceView,
    /// Inside an `extends @anchor` / `slot` extensibility block.
    Extends,

    // ── plan / RBAC ──
    /// Inside a `plan <name>` block.
    Plan,

    // ── design tokens (`design.lzi`) ──
    /// Inside a `design` token catalog (color/typography/space/...).
    Design,

    // ── value catalogs + cross-cutting modifiers ──
    /// A closed-catalog enum VALUE (`asc`/`desc`, `lax`/`strict`,
    /// `rolling`/`canary`, http methods, ...). These are
    /// `constant.language.*` in tmLanguage, not keywords; carried for
    /// completeness so the proven-complete scan can resolve match-arm
    /// value literals without an allowlist entry per value.
    Value,
    /// A cross-cutting `storage.modifier` word (`required`, `optional`,
    /// `from`, `to`, `by`, `at`, `on`, ...) valid in many declaration
    /// contexts.
    Modifier,
    /// A filter/policy/test expression operator or predicate
    /// (`and`/`or`/`not`/`in`/`has`/`when`/`between`/...).
    Expression,
}

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
