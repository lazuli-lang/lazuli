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
/// * **convention-derived feature context** (`VOCAB-CONTEXT-CTXMD-001`) —
///   fires on an absent / stub co-located `<feature>.ctx.md` sidecar. After
///   the `attach_ctx` keyword was retired in favour of that file convention,
///   the rule has no keyword owner (its `purpose` / `non_goals` siblings stay
///   attributed to their surviving keywords).
///
/// This is a documented home, NOT a dumping ground: the
/// `lazuli_diagnostics_registry` bridge asserts every code here resolves to a
/// live rule and is claimed *exactly once* (here OR by a capability's
/// `produces`). A future diagnostic added without a capability home must land
/// here explicitly or the `complete` test fails the build.
pub const GLOBAL_DIAGNOSTICS: &[DiagnosticFacet] = &[
    // SPEC-05 — bare `=` used as equality in a closed predicate. Genuinely
    // cross-cutting: fires across rule/test/filter/policy/invariant/route-guard/
    // webhook-emit predicate contexts, owned by no single keyword.
    DiagnosticFacet {
        code: "PREDICATE-EQ-OPERATOR-001",
        base_severity: "error",
        category: "correctness",
    },
    // SPEC-07 C — a `policies` category named after a CRUD/effect verb. Source
    // scan over the policy-category position; owned by no single keyword (the
    // `policies` block + its `@policy.<cat>` reference sites both carry it).
    DiagnosticFacet {
        code: "POLICY-CATEGORY-SHADOWS-EFFECT-001",
        base_severity: "error",
        category: "correctness",
    },
    // SECURITY — a command / query / api `policy <ref>` that resolves to no
    // declared `policies` category in any feature (cross-feature
    // `PolicyRef::External` to a missing category, or a feature-local
    // `@policy.<name>` with no match). Owned by no single keyword: the
    // reference sites (`command` / `query` / `api`) and the cross-feature
    // `policies` block all carry it. Codegen fails closed (deny atom); this
    // rule surfaces the broken reference at build time.
    DiagnosticFacet {
        code: "POLICY-REF-UNRESOLVED-001",
        base_severity: "error",
        category: "correctness",
    },
    // W2-1 — two DSL constructs (enum / lifecycle-generated-enum / query /
    // command / transition) that lower to the SAME emitted Go identifier in a
    // feature's `<feature>gen` package, producing a `go build` double
    // declaration. Genuinely cross-cutting: owned by no single keyword (every
    // one of the five colliding construct families carries it), so it lands in
    // GLOBAL rather than on any one capability's `produces[]`.
    DiagnosticFacet {
        code: "CODEGEN-GO-IDENT-COLLISION-008",
        base_severity: "error",
        category: "correctness",
    },
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
    // W2-4 — a framework-synthesized table the runtime WRITES (audit_log,
    // lazuli_audit, lazuli_outbox, …) with no `CREATE TABLE` migration
    // emitted into `dist/go/migrations/`. Cross-cutting over generated
    // SQL/Go artifacts (no single `.lzi` keyword owns it; the activating
    // construct is `audit`/`outbox guaranteed`, but the diagnostic fires
    // over the migration tree), so it lands in GLOBAL alongside the other
    // migration-codegen codes. `RUNTIME-` falls through `from_code_prefix`
    // to `Vocabulary` — matching its `RUNTIME-UPDATE-BUILDER-JSONB-001` peer.
    DiagnosticFacet {
        code: "RUNTIME-EMITTED-TABLE-MIGRATION-001",
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
    // spec 0008 — resource-graph cohesion sibling + its info companions.
    DiagnosticFacet {
        code: "LZI-FEATURE-COHESION-002",
        base_severity: "warning",
        category: "lzi_hygiene",
    },
    DiagnosticFacet {
        code: "LZI-FEATURE-COHESION-002-INFO",
        base_severity: "info",
        category: "lzi_hygiene",
    },
    // ── spec 0010 escape-hatch visibility (cross-cutting: Go handlers ∩
    // `.lzi` IR ∩ `.sql` files, so no single keyword owns them) ──
    DiagnosticFacet {
        code: "ESC-RAWSQL-IN-HANDLER-001",
        base_severity: "warning",
        category: "escape_hatch",
    },
    DiagnosticFacet {
        code: "ESC-SQL-TENANCY-CONTRACT-001",
        base_severity: "warning",
        category: "escape_hatch",
    },
    DiagnosticFacet {
        code: "ESC-SCOPE-OVERRIDE-UNGUARDED-001",
        base_severity: "warning",
        category: "escape_hatch",
    },
    // ── spec 0014 referential-guard suggestion (cross-cutting: a Go
    // handler hand-writing the COUNT/EXISTS-then-reject guard the
    // resource-level `restrict on_delete` primitive replaces; no single
    // keyword owns the handler-body scan) ──
    DiagnosticFacet {
        code: "SUGGEST-REFERENTIAL-GUARD-001",
        base_severity: "warning",
        category: "vocabulary",
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
    // ── convention-derived feature context (no keyword owner) ──
    // `VOCAB-CONTEXT-CTXMD-001` fires when a feature's co-located
    // `<feature>.ctx.md` sidecar is absent or a <100-char stub. After the
    // `attach_ctx` keyword was retired in favour of that convention, the
    // rule no longer has a keyword to attach to — it is derived from the
    // file convention, so it lives here (its sibling `purpose` /
    // `non_goals` codes stay attributed to their surviving keywords).
    DiagnosticFacet {
        code: "VOCAB-CONTEXT-CTXMD-001",
        base_severity: "warning",
        category: "vocabulary",
    },
    // `VOCAB-CONTEXT-PROSE-SHADOWS-IR-001` (CUT 1b, the drift-killer) fires
    // when a feature's co-located `<feature>.ctx.md` prose SHADOWS the IR —
    // a markdown table whose header columns duplicate >=3 of a resource's
    // fields. Same convention origin as CTXMD-001 (no keyword owner): it
    // enforces existing doctrine (canonical-semantics "Do not duplicate
    // schema ... there"), not net-new vocab, so it lives here too.
    DiagnosticFacet {
        code: "VOCAB-CONTEXT-PROSE-SHADOWS-IR-001",
        base_severity: "warning",
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
