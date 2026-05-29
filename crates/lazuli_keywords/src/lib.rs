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
//!   facet — left empty in H1, backfilled in Wave C1.
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

    /// The diagnostic codes this capability can produce. **Empty in Wave
    /// H1** — Wave C1 backfills the code↔capability links. Carried as
    /// string-mirror refs ([`DiagnosticFacet`]) so this leaf stays free of
    /// `lazuli_doctor`.
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
