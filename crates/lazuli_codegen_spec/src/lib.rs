//! Runtime spec — the input shape the Go and TS codegens consume.
//!
//! The full IR (`lazuli_ir::Module`) is rich and supports many features the
//! runtime spike does not yet target (workflows, surfaces, contracts,
//! mobile profiles, etc.). The runtime spec is a deliberately minimal
//! projection: just enough to emit the runtime-shaped Go file
//! (`dist/go/customer/customer.gen.go`) and TS file
//! (`dist/web/customer/src/customer.gen.ts`).
//!
//! Phase K decouples the spec from the hardcoded `customer_spike()`
//! fixture: every type derives serde traits, and producers can ship a
//! spec as JSON. A future cut adds `from_module(&Module) -> RuntimeFeature`
//! once the modern `.lzi` syntax has parser coverage. Until then, JSON
//! manifests are the portable interchange format between authoring tools
//! (LLMs, IDEs, hand-written) and the codegen pipeline.

// Internal-tooling workspace: rustdoc cross-refs routinely point to
// `#[cfg(test)]` proof-tests and `pub(crate)` helpers (valid navigation under
// `--document-private-items`, but unresolvable to a public-API resolver). CI
// keeps `-D broken_intra_doc_links` on; this is the deliberate posture for these
// internal crates (genuine wrong refs are still fixed inline).
#![allow(rustdoc::broken_intra_doc_links, rustdoc::private_intra_doc_links)]
use serde::{Deserialize, Serialize};

/// Top-level codegen input — one feature with its resources, commands
/// and queries.
///
/// Both backend (Go) and frontend (TS) emitters consume this same value;
/// any divergence in their outputs is intentional and lives in the
/// emitter, not in the spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeFeature {
    /// Canonical feature name. Drives package names, import paths, route
    /// prefixes, and (for the Go emitter) the package directive.
    pub name: String,
    /// Canonical source path that emitted files reference in their header
    /// banner. Example: `examples/full-capsule/full-capsule.lzi`.
    pub source_path: String,
    /// Resources (DB-backed entities) the feature owns.
    pub resources: Vec<RuntimeResource>,
    /// Side-effecting operations (writes / mutations).
    pub commands: Vec<RuntimeCommand>,
    /// Read-only operations (lists, lookups).
    pub queries: Vec<RuntimeQuery>,
}

/// A persisted entity inside a [`RuntimeFeature`].
///
/// Resources carry only author-defined fields — runtime-managed columns
/// (`id`, `org_id`, `created_at`, `updated_at`, `deleted_at`) are emitted
/// automatically by the codegen and are NOT modeled here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeResource {
    /// Lower-case canonical name, e.g. `customer`. Used for table name and
    /// file scope. The Go struct is the PascalCase variant.
    pub name: String,
    /// Tenancy axis. `Org` is the default; `None` means resource-level
    /// opt-out from tenant scoping.
    pub tenancy: Tenancy,
    /// `true` ⇒ `deleted_at` column + `DELETE` translates to soft-delete.
    pub soft_delete: bool,
    /// Optional retention spec — none today, but encoded so the emitter can
    /// fill `lazuli.RetentionSpec` deterministically.
    pub retention: Option<RetentionSpec>,
    /// Resource-defined fields (excludes the runtime-managed id, org_id,
    /// created_at, updated_at, deleted_at — those are emitted automatically).
    pub fields: Vec<RuntimeField>,
}

/// Tenancy axis a [`RuntimeResource`] participates in.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Tenancy {
    /// Scoped per organization — `org_id` column emitted, queries filter
    /// by `org_id = ctx.OrgID`.
    Org,
    /// Resource-level opt-out from tenant scoping (e.g. lookup tables).
    None,
}

/// Data retention policy attached to a [`RuntimeResource`].
///
/// Mapped 1:1 to the runtime's `lazuli.RetentionSpec` value at emit time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionSpec {
    /// Window string, e.g. "7y" or "30d".
    pub window: String,
    /// Action keyword: `anonymize`, `purge`, etc. Mirrors the DSL invariant.
    pub action: RetentionAction,
}

/// What happens to rows when their [`RetentionSpec`] window elapses.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RetentionAction {
    /// Replace PII-bearing columns with deterministic placeholders.
    Anonymize,
    /// Hard-delete the row.
    Purge,
}

/// One field on a [`RuntimeResource`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeField {
    /// Canonical lowercase name as written in the DSL.
    pub name: String,
    /// Logical kind — drives both the Go type and the TS type.
    pub kind: FieldKind,
}

/// Logical column / argument type. The Go and TS emitters each pick a
/// language-native representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldKind {
    /// Free-form text — Go `string`, TS `string`.
    Text,
    /// Validated email — same wire type as `Text` but flagged for
    /// validator generation.
    Email,
    /// 64-bit signed integer.
    Integer,
    /// Boolean.
    Boolean,
}

/// A write-side operation (mutation) on a [`RuntimeFeature`].
///
/// Commands carry their full policy + rate-limit + validators + effect
/// + inputs + emits + cache-invalidation surface. The Go emitter walks
/// this shape directly to produce the handler + policy registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeCommand {
    /// Short name inside the feature (e.g. `create`, `update_email`).
    /// The fully qualified name is `<feature>.<name>`.
    pub short_name: String,
    /// Free-form policy reference used for the canonical Policy.Name field.
    pub policy_name: String,
    /// Resolved atoms — pairs of (namespace, name) like ("role", "admin").
    pub policy_atoms: Vec<(String, String)>,
    /// Free-form rate-limit clause, e.g. `30 per hour per ip`.
    pub rate_limit: String,
    /// Named validators run before the effect.
    pub validators: Vec<String>,
    /// What the command does to the resource (insert / update / delete).
    pub effect: RuntimeEffect,
    /// Inputs the author wrote in the DSL. Includes route keys (e.g. `id`)
    /// and `input` block fields. Order matches the DSL.
    pub inputs: Vec<RuntimeInput>,
    /// Events published after the effect commits.
    pub emits: Vec<RuntimeEmit>,
    /// Cache keys to bust after a successful run.
    pub invalidates: Vec<String>,
    /// Deprecation metadata — surfaced in headers + OpenAPI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<RuntimeDeprecation>,
}

/// Deprecation metadata attached to a [`RuntimeCommand`].
///
/// All fields optional — at least one should be set; otherwise the
/// emitter omits the deprecation banner entirely.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeDeprecation {
    /// Version / date the deprecation began.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// Suggested replacement command (e.g. `customer.update`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
    /// Planned removal date / version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sunset: Option<String>,
}

/// One input parameter on a [`RuntimeCommand`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInput {
    /// Field name as it appears on the Go input struct (PascalCase).
    pub field_name: String,
    /// Source field on the resource for typing — `id` -> `lazuli.ID`,
    /// otherwise looked up in the resource's RuntimeField list.
    pub kind: FieldKind,
}

/// How a [`RuntimeCommand`] mutates its resource.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RuntimeEffect {
    /// `creates Customer` — bind every input field as the same column.
    CreatesFromInput,
    /// `updates Customer where id = input.id` — Where is `id`,
    /// SET is every other input field.
    UpdatesByID,
    /// `deletes Customer where id = input.id`.
    DeletesByID,
}

/// One event emitted by a [`RuntimeCommand`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEmit {
    /// Event name (e.g. `customer_created`). Fully qualified at the wire
    /// level as `<feature>.<name>`.
    pub name: String,
    /// How the payload is constructed.
    pub kind: RuntimeEmitKind,
}

/// Strategy for building a [`RuntimeEmit`]'s payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeEmitKind {
    /// Derived emit: payload is the producing row from creates/updates/deletes.
    FromCreates,
    /// Explicit bind block — pairs of (column, source).
    Bind(Vec<(String, EmitSource)>),
}

/// Source of a single column in a [`RuntimeEmitKind::Bind`] payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EmitSource {
    /// Pulled from a named command input.
    Input(String),
    /// Pulled from the request context (e.g. `user.id`).
    Ctx(String),
}

/// A read-side operation on a [`RuntimeFeature`].
///
/// Queries split into two shapes — list (paginated, filterable,
/// searchable) and lookup (single-row by key). The discriminator is
/// [`QueryKind`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeQuery {
    /// Short name inside the feature (e.g. `list`, `by_id`). Fully qualified
    /// wire-registry key is `<feature>.<name>` (cell B1 dropped the historical
    /// `.query.` infix; the `/q/` HTTP prefix already disambiguates kind).
    pub short_name: String,
    /// `List` (many rows) or `Lookup` (single row).
    pub kind: QueryKind,
    /// Free-form policy reference used for the canonical Policy.Name field.
    pub policy_name: String,
    /// Resolved atoms — pairs of (namespace, name) like ("scope", "same_org").
    pub policy_atoms: Vec<(String, String)>,
    /// Args fields the query accepts (besides the canonical lookup keys).
    /// For lists: optional filter inputs + the search term, etc.
    /// For lookups: the lookup keys (e.g. `id`).
    pub args: Vec<RuntimeArg>,
    /// `cache key/ttl` block. None disables caching.
    pub cache: Option<RuntimeCache>,
    /// Page size for `List` queries; `0` means no pagination.
    pub paginate: u32,
    /// `where` filters bound to optional inputs.
    pub filters: Vec<RuntimeFilter>,
    /// Optional free-text search projection.
    pub search: Option<RuntimeSearch>,
    /// Columns the lookup matches against (e.g. `id`, `email`).
    pub lookup_by: Vec<RuntimeLookupKey>,
}

/// Read-side shape of a [`RuntimeQuery`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryKind {
    /// Many rows, paginated.
    List,
    /// Single row by key.
    Lookup,
}

/// One argument on a [`RuntimeQuery`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeArg {
    /// Field name as it appears on the args struct. PascalCase for Go,
    /// lowercase or original for TS (the emitter handles casing).
    pub field_name: String,
    /// Logical type — drives Go and TS argument types.
    pub kind: FieldKind,
    /// Optional args become `*T` in Go and `T?` in TS.
    pub optional: bool,
}

/// Cache configuration on a [`RuntimeQuery`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeCache {
    /// Cache key template (e.g. `customer.list`).
    pub key: String,
    /// TTL expression (Go duration literal, e.g. `5 * time.Minute`).
    pub ttl: String,
}

/// One `where` filter on a [`RuntimeQuery`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeFilter {
    /// Resource column the filter targets.
    pub column: String,
    /// Args input that supplies the filter value when present.
    pub when_input: String,
}

/// Free-text search projection on a [`RuntimeQuery`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSearch {
    /// Args input that supplies the search term.
    pub source_input: String,
    /// Columns the search ranges over.
    pub over: Vec<String>,
}

/// One lookup key on a [`QueryKind::Lookup`] [`RuntimeQuery`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeLookupKey {
    /// Resource column the key matches.
    pub column: String,
    /// Args input that supplies the key value.
    pub source_input: String,
}

/// Hand-built spec mirroring the runtime spike's hand-written
/// `dist/go/customer/customer.gen.go` and
/// `dist/web/customer/src/customer.gen.ts`.
///
/// Used as the seed input for Phase K codegen tests. Replaced in a
/// future cut with `from_module(&lazuli_ir::Module)` once the modern
/// `.lzi` parser stack covers the full shape.
///
/// ## Examples
///
/// ```rust
/// use lazuli_codegen_spec::{customer_spike, QueryKind, RuntimeEffect};
///
/// let spec = customer_spike();
/// assert_eq!(spec.name, "customer");
/// assert_eq!(spec.commands.len(), 3);
/// assert_eq!(spec.queries.len(), 2);
/// assert!(matches!(spec.queries[0].kind, QueryKind::List));
/// assert!(matches!(spec.commands[0].effect, RuntimeEffect::CreatesFromInput));
/// ```
pub fn customer_spike() -> RuntimeFeature {
    RuntimeFeature {
        name: "customer".to_owned(),
        source_path: "examples/full-capsule/full-capsule.lzi".to_owned(),
        resources: vec![RuntimeResource {
            name: "customer".to_owned(),
            tenancy: Tenancy::Org,
            soft_delete: true,
            retention: Some(RetentionSpec {
                window: "7y".to_owned(),
                action: RetentionAction::Anonymize,
            }),
            fields: vec![
                RuntimeField {
                    name: "name".to_owned(),
                    kind: FieldKind::Text,
                },
                RuntimeField {
                    name: "email".to_owned(),
                    kind: FieldKind::Email,
                },
            ],
        }],
        commands: vec![
            RuntimeCommand {
                short_name: "create".to_owned(),
                policy_name: "@policy.create".to_owned(),
                policy_atoms: vec![("role".to_owned(), "admin".to_owned())],
                rate_limit: "30 per hour per ip".to_owned(),
                validators: vec!["email_check".to_owned()],
                effect: RuntimeEffect::CreatesFromInput,
                inputs: vec![
                    RuntimeInput {
                        field_name: "Name".to_owned(),
                        kind: FieldKind::Text,
                    },
                    RuntimeInput {
                        field_name: "Email".to_owned(),
                        kind: FieldKind::Email,
                    },
                ],
                emits: vec![RuntimeEmit {
                    name: "customer_created".to_owned(),
                    kind: RuntimeEmitKind::FromCreates,
                }],
                invalidates: vec![
                    "customer.list".to_owned(),
                    "customer.global_search".to_owned(),
                ],
                deprecated: None,
            },
            RuntimeCommand {
                short_name: "update_email".to_owned(),
                policy_name: "@policy.update".to_owned(),
                policy_atoms: vec![("role".to_owned(), "admin".to_owned())],
                rate_limit: "10 per hour per user".to_owned(),
                validators: vec!["email_check".to_owned()],
                effect: RuntimeEffect::UpdatesByID,
                inputs: vec![
                    RuntimeInput {
                        field_name: "ID".to_owned(),
                        kind: FieldKind::Integer,
                    },
                    RuntimeInput {
                        field_name: "Email".to_owned(),
                        kind: FieldKind::Email,
                    },
                ],
                emits: vec![],
                invalidates: vec!["customer.list".to_owned(), "customer.by_id".to_owned()],
                deprecated: None,
            },
            RuntimeCommand {
                short_name: "archive".to_owned(),
                policy_name: "@policy.delete".to_owned(),
                policy_atoms: vec![("role".to_owned(), "admin".to_owned())],
                rate_limit: "10 per hour per user".to_owned(),
                validators: vec![],
                effect: RuntimeEffect::DeletesByID,
                inputs: vec![RuntimeInput {
                    field_name: "ID".to_owned(),
                    kind: FieldKind::Integer,
                }],
                emits: vec![RuntimeEmit {
                    name: "customer_archived".to_owned(),
                    kind: RuntimeEmitKind::Bind(vec![
                        ("customer_id".to_owned(), EmitSource::Input("ID".to_owned())),
                        ("actor_id".to_owned(), EmitSource::Ctx("user.id".to_owned())),
                    ]),
                }],
                invalidates: vec!["customer.list".to_owned(), "customer.by_id".to_owned()],
                deprecated: None,
            },
        ],
        queries: vec![
            RuntimeQuery {
                short_name: "list".to_owned(),
                kind: QueryKind::List,
                policy_name: "@policy.read".to_owned(),
                policy_atoms: vec![("scope".to_owned(), "same_org".to_owned())],
                args: vec![
                    RuntimeArg {
                        field_name: "Email".to_owned(),
                        kind: FieldKind::Email,
                        optional: true,
                    },
                    RuntimeArg {
                        field_name: "Search".to_owned(),
                        kind: FieldKind::Text,
                        optional: true,
                    },
                ],
                cache: Some(RuntimeCache {
                    key: "customer.list".to_owned(),
                    ttl: "5 * time.Minute".to_owned(),
                }),
                paginate: 50,
                filters: vec![RuntimeFilter {
                    column: "email".to_owned(),
                    when_input: "Email".to_owned(),
                }],
                search: Some(RuntimeSearch {
                    source_input: "Search".to_owned(),
                    over: vec!["name".to_owned(), "email".to_owned()],
                }),
                lookup_by: vec![],
            },
            RuntimeQuery {
                short_name: "by_id".to_owned(),
                kind: QueryKind::Lookup,
                policy_name: "@policy.read".to_owned(),
                policy_atoms: vec![("scope".to_owned(), "same_org".to_owned())],
                args: vec![RuntimeArg {
                    field_name: "ID".to_owned(),
                    kind: FieldKind::Integer,
                    optional: false,
                }],
                cache: None,
                paginate: 0,
                filters: vec![],
                search: None,
                lookup_by: vec![RuntimeLookupKey {
                    column: "id".to_owned(),
                    source_input: "ID".to_owned(),
                }],
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn customer_spike_has_expected_shape() {
        let spec = customer_spike();
        assert_eq!(spec.name, "customer");
        assert_eq!(spec.resources.len(), 1);
        assert_eq!(spec.commands.len(), 3);
        assert_eq!(spec.queries.len(), 2);
    }

    #[test]
    fn customer_spike_round_trips_through_json() {
        let spec = customer_spike();
        let json = serde_json::to_string(&spec).expect("spec serializes to JSON");
        let back: RuntimeFeature =
            serde_json::from_str(&json).expect("spec round-trips through JSON");
        assert_eq!(back.name, spec.name);
        assert_eq!(back.commands.len(), spec.commands.len());
        assert_eq!(back.queries.len(), spec.queries.len());
    }
}
