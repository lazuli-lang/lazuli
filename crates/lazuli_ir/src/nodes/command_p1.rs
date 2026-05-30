/// Root IR node for a `command <name> { … }` block. Carries the typed
/// effect, input shape, route slots, and every cross-cutting decorator
/// (`policy`, `audit`, `approval`, `invalidates`, `external_calls`,
/// `timeout`, `retry`, `idempotency`, `write_window`, `deprecated`,
/// `handler`, `tests`, `triggers`) the analyzer resolved from source.
/// One [`Command`] per authored block; codegen targets fan out from here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContract>,
    pub kind: CommandKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route: Vec<RouteSlot>,
    pub input: CommandInput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<TargetExpr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lets: Vec<LetBinding>,
    pub effect: CommandEffect,
    pub policy: PolicyRef,
    /// RB.S6 — structured `policy <expr>` form when the authored
    /// policy contained predicates (`has_role` / `has_permission` /
    /// `authenticated`) or boolean combinators. Coexists with `policy`
    /// (legacy atom ref) for back-compat. `None` when the policy is
    /// a bare atom or absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExpr>,
    /// IR Error-Vocab — per-command override for the `policy_denied`
    /// error message. Highest-precedence step in the resolution chain
    /// (proposal §2.E step 1). When `Some`, the codegen emits a
    /// per-command `ErrorKeys.PolicyDenied` entry and the runtime
    /// resolver picks it before consulting `PolicyCategory.when_denied`
    /// or `FeatureErrors.messages`. See
    /// `docs/proposals/ir-error-messages-vocab.md` §3.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_when_denied: Option<TranslationKeyRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    /// `rate_limit "<N per period per scope>"` declaration, optionally
    /// env-qualified per `ir-rate-limit-env-aware` (cell 1). The single-
    /// line `rate_limit "X"` source shape lowers to `RateLimitSpec {
    /// default: "X", by_env: [] }`; multi-line shapes populate `by_env`.
    /// Captured verbatim and parsed by adapters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<RateLimitSpec>,
    /// Phase L Tier 4b — `audit <subject>, <subject>, ...` with optional
    /// `emit_to <event_group>` child.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<AuditSpec>,
    /// Phase L Tier 4b — Cut A.9 `approval` block. Replaces the
    /// `CommandApprovalFact` text-walker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval: Option<ApprovalSpec>,
    /// Phase L Tier 4b — `invalidates query.<name>(...)` references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidates: Vec<InvalidatesSpec>,
    /// Phase L Tier 4b — `calls <slot>.<op>` inside a command body.
    /// Mirrors `Job.external_calls`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_calls: Vec<ExternalCallRef>,
    /// Phase L Tier 4 follow-up — `timeout "<duration>"` literal.
    /// Mirrors `Job.timeout`. Adapter parses; language keeps verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    /// Phase L Tier 4 follow-up — `retry <count> [backoff <strategy>]`.
    /// Mirrors `Job.retry`. Doctor cross-checks against `external_calls`
    /// to enforce `INT-CALL-RETRY-001`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    /// Phase L Tier 4 follow-up — `idempotency by <field>[, ...]`.
    /// Mirrors `Job.idempotency`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<IdempotencyKey>,
    /// `write_window by <path> within <duration_or_ref>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_window: Option<CommandWriteWindow>,
    /// OpenAPI bucket cycle — `deprecated [since "..." replacement <ref>
    /// sunset "..."]`. `None` for live commands; `Some` for those flagged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<Deprecation>,
    /// `handler @fn.<name>` or `handler "./path.go"` escape hatch.
    /// `None` for commands fully described by their declarative body
    /// (`creates`/`updates`/`deletes`/`returns` with no user code).
    ///
    /// When set with `effect == None`, the runtime treats the command as
    /// a pure-read invocation routed to the user's Go handler — codegen
    /// emits `Effect: lazuli.Returns(<PascalCase(name)>)` instead of
    /// `Effect: nil` (closes WAR-RUNTIME-COMMAND-01 Effect half).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler: Option<HandlerRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<TestBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
    /// Lifecycle transitions this command fires, in order. Empty = no
    /// lifecycle binding. Multi-element = chain that runs in one tx
    /// (pre-guard = `transitions[0].from`, post-update = `last.to`).
    /// See docs/proposals/ir-command-transition-binding.md.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<String>,
    /// FR-3a — marker set on commands that the analyzer synthesized
    /// from a `@cap.File(...)` field on a per-user resource. `None`
    /// for author-written commands. Codegen reads this to wire the
    /// auto-photo runtime helper instead of expecting a hand-rolled
    /// handler. See docs/proposals/fileref-jsonb-fr3-design.md.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synthesized_from_cap_file: Option<SynthesizedFromCapFile>,
    /// `ir-resource-conventions-owner-scope.md` §7.3 + §8.5.A —
    /// owner-scope SQL fragment composed by the analyzer at synth
    /// time. `Some` only when this command was emitted by the crud /
    /// me synth pass for a resource carrying a `@owner_axis(through:
    /// <col>)` field. Codegen passes the fragments through verbatim
    /// — there is no runtime branching. See Cell O2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_scope_sql: Option<OwnerScopeSql>,
    /// `docs/proposals/lifecycle-vocab.md` §4.2 + audit Cell G —
    /// provenance marker the analyzer stamps when the command was
    /// synthesized from a higher-level vocabulary (today: lifecycle
    /// transitions). `None` for author-written commands. Cold-readers
    /// tracing a lifecycle-emitted command see the source
    /// `resource.lifecycle.<transition>` here via `lazuli inspect
    /// --expand=commands`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<DerivedFrom>,
}

/// Provenance marker for commands the analyzer synthesized from a
/// higher-level vocabulary. Closed catalog — today only the lifecycle
/// vocab uses it (one variant per origin shape); future synth passes
/// add new variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum DerivedFrom {
    /// Command was emitted by `lazuli_analyzer::lifecycle::lower_transition_command`
    /// for a `transition` declared inside `resource.<resource>.lifecycle`.
    Lifecycle {
        /// Resource owning the lifecycle.
        resource: String,
        /// Transition name (e.g. `publish` for `transition publish`).
        transition: String,
    },
}

/// `write_window by <path> within <duration>` — restricts repeated writes
/// keyed by a target field within a sliding time window. Runtime adapters
/// read this to throttle re-runs of the same command on the same target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandWriteWindow {
    /// Field path that keys the window (e.g. `target.user_id`).
    pub by: Path,
    /// Authored duration literal (`"24h"`, `"5m"`).
    pub within: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// OpenAPI bucket cycle — typed deprecation marker for commands (and
/// post-Tier-4 apis). All sub-fields are optional; bare `deprecated`
/// surfaces as `Deprecation::default()`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deprecation {
    /// Authored `since "<version>"`. Free-form (semver, calendar,
    /// git-sha) — emitted verbatim under `x-lazuli-deprecated-since`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// Authored `replacement <ref>` resolved at lowering. `None` when
    /// omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<DeprecationReplacement>,
    /// Authored `sunset "<YYYY-MM-DD>"` — ISO-8601 date. Format-checked
    /// at lowering; doctor warns if in the past.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sunset: Option<String>,
}

/// Closed catalog of replacement reference shapes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum DeprecationReplacement {
    /// `replacement <command_name>` — same-feature short form.
    LocalCommand(String),
    /// `replacement api.<name>` on an api — same-feature short form.
    LocalApi(String),
    /// `replacement <feature>.command.<name>` — cross-feature.
    Qualified(QualifiedName),
    /// `replacement <feature>.api.<name>` — cross-feature api.
    QualifiedApi(QualifiedName),
    /// `replacement "https://..."` — explicit URL escape hatch.
    Url(String),
}

/// Phase L Tier 4b — declarative audit spec captured from a command's
/// `audit <subject>, <subject>, ...` line and optional `emit_to <group>`
/// child. The subject strings stay verbatim (`actor`, `target.id`,
/// `input.owner_id`) because the analyzer resolves them against the
/// command's input slots; doctor cross-checks `emit_to` against the
/// feature's `event_group` declarations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditSpec {
    /// `actor`, `target.id`, `input.<field>`, etc. Each entry is a single
    /// subject reference.
    pub subjects: Vec<String>,
    /// `emit_to <event_group>` — optional event-group emission target.
    /// `None` when the command writes audit without emitting to a group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emit_to: Option<String>,
    /// LGPD/GDPR shape — names the field on the affected resource
    /// that identifies the data subject for right-of-access /
    /// right-to-erasure queries. `None` for non-personal-data
    /// commands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_subject: Option<String>,
    /// `audit before` — capture pre-mutation field values.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub record_before: bool,
    /// `audit after` — capture post-mutation field values.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub record_after: bool,
    /// `audit retain <duration>` — retention horizon for audit rows.
    /// `None` = use feature/registry default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retain_for: Option<String>,
    /// GAP-AUDIT-01 — `materialize @feature.<feature>.<Resource>` clause.
    /// When present, the runtime writes the assembled audit record into
    /// the named (append_only) OperationLog resource *in addition to* the
    /// `emit_to` event — same record, two sinks. The target resolves like
    /// a GAP-12 cross-feature reference (`uses`-as-Dependencies) and must
    /// be an `append_only` resource (doctor `AUDIT-MATERIALIZE-TARGET-001`).
    /// `None` = audit emits the event only (today's behaviour). Additive:
    /// pre-GAP-AUDIT-01 snapshots deserialize with `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materialize: Option<AuditMaterialize>,
}

/// GAP-AUDIT-01 — typed payload for the `materialize
/// @feature.<feature>.<Resource>` clause on an `audit` block. Names the
/// feature that owns the OperationLog resource + the resource itself. The
/// reference resolves like a GAP-12 cross-feature target (same feature or
/// a `uses`-listed dependency); doctor `AUDIT-MATERIALIZE-TARGET-001`
/// additionally enforces the resolved resource is `append_only`. Mirrors
/// `lazuli_syntax::AuditMaterializeAst`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AuditMaterialize {
    /// Owning feature name (segment after `@feature.`).
    pub feature: String,
    /// Target OperationLog resource name (PascalCase).
    pub resource: String,
}

/// Phase L Tier 4b — Cut A.9 `approval` block lifted into IR.
///
/// W4 GAP-06 widened the single-approver form to an ordered approval
/// *chain* (`approval chain [@role.manager, @role.admin] sequential timeout
/// 24h then deny`). Back-compat shim: the single-approver `by` slot is
/// retained and always reflects the *first* approver in the chain, so
/// pre-W4 snapshots (which serialized only `by`) deserialize with a
/// 1-element chain (see `chain`'s `#[serde(default)]`); consumers that only
/// read `by` keep working unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalSpec {
    /// `required_when <predicate>` — verbatim predicate text. The
    /// predicate language tracks the command target's resource fields;
    /// the analyzer keeps the raw form so future Cut A.9 evolutions can
    /// land without an IR churn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_when: Option<String>,
    /// `by @role.<name>` or `by @actor.<name>` — the *first* approver atom.
    /// Always equals `chain[0]`. Kept as a dedicated field for back-compat
    /// with pre-W4 single-approver snapshots + consumers that read one
    /// approver.
    pub by: String,
    /// W4 GAP-06 — the ordered approval chain. Single-approver forms lift
    /// to a 1-element chain (`[by]`). `#[serde(default)]` so a pre-W4
    /// snapshot with only `by` deserializes with an empty `chain`;
    /// `approvers()` falls back to `by` in that case.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chain: Vec<String>,
    /// W4 GAP-06 — `sequential` requires the approvers to approve in chain
    /// order (each tier gates the next). Absent = parallel (any-order).
    /// `false` for the legacy single-approver form.
    #[serde(default, skip_serializing_if = "is_false")]
    pub sequential: bool,
    /// `timeout "24h"` — duration literal parsed by the adapter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    /// `then deny | allow | escalate` — closed catalog of resolutions.
    pub then: ApprovalThen,
}

impl ApprovalSpec {
    /// The ordered approver atoms. Returns `chain` when populated (W4
    /// chain form); falls back to the single `by` approver for pre-W4
    /// snapshots that serialized only `by`.
    pub fn approvers(&self) -> Vec<String> {
        if self.chain.is_empty() {
            vec![self.by.clone()]
        } else {
            self.chain.clone()
        }
    }
}

/// Phase L Tier 4b — closed catalog of approval timeout resolutions
/// (Cut A.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalThen {
    /// Reject the command on approval timeout.
    Deny,
    /// Allow the command on approval timeout.
    Allow,
    /// Forward to the next approver tier on timeout.
    Escalate,
}

/// Phase L Tier 4b — `invalidates query.<name>(args)` reference.
/// `query` carries the qualified query name and `args` the explicit
/// named-argument bindings (e.g. `id: route.id`). Doctor uses this for
/// cache-invalidation cross-checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvalidatesSpec {
    pub query: QualifiedName,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<NamedArg>,
}

/// Closed catalog of command effect categories. Mirrors the four typed
/// effect shapes ([`CreateEffect`], [`UpdateEffect`], [`DeleteEffect`],
/// [`ReturnsEffect`]); kept as a separate flag so consumers can branch on
/// kind without pattern-matching the full `CommandEffect`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandKind {
    /// `creates <Resource>` — inserts a new row.
    Create,
    /// `updates <target>` — mutates an existing row.
    Update,
    /// `deletes <target>` — removes a row.
    Delete,
    /// `returns <Type>` — pure request/response, no side effect.
    Returns,
    /// W4 GAP-REORDER-01 — `reorder <Resource> by <position>` — batch
    /// position UPDATE over an ordered id list.
    Reorder,
}

/// One typed `route <name>: <Type>` slot on a command. The
/// `kind` axis ([`RouteSlotKind`]) discriminates plain ids,
/// opaque-token sentinels, and signed tokens; `from` carries the optional
/// `ctx.*` default binding so doctor can suppress missing-argument
/// diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteSlot {
    pub name: String,
    pub type_ref: TypeRef,
    /// Phase L Tier 4 follow-up — `route <name>: <Type> from ctx.<expr>`
    /// captures the optional default-binding expression. `Some(text)`
    /// means the slot has a context default; `None` means the caller
    /// must supply it. Doctor's command-route-binding check reads this
    /// to suppress missing-argument diagnostics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// `route opaque token: Text` -> `OpaqueToken`.
    /// `route signed_token`       -> `SignedToken`.
    /// Plain `route id: ID`       -> `Plain` (default).
    #[serde(default, skip_serializing_if = "is_plain_route_slot_kind")]
    pub kind: RouteSlotKind,
}

/// Closed catalog of route-slot shapes. `Plain` is the default
/// (typed identifier); `OpaqueToken` covers `route opaque token: Text` (no
/// id leakage); `SignedToken` covers `route signed_token` (HMAC-signed
/// stateless tokens decoded by the runtime).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteSlotKind {
    /// Default — plain typed parameter (`route id: ID`).
    #[default]
    Plain,
    /// `route opaque token: Text` — runtime hides the underlying id.
    OpaqueToken,
    /// `route signed_token` — HMAC-signed payload, no DB lookup.
    SignedToken,
}
