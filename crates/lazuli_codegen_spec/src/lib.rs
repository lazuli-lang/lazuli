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

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeFeature {
    /// Canonical feature name. Drives package names, import paths, route
    /// prefixes, and (for the Go emitter) the package directive.
    pub name: String,
    /// Canonical source path that emitted files reference in their header
    /// banner. Example: `examples/full-capsule/full-capsule.lzi`.
    pub source_path: String,
    pub resources: Vec<RuntimeResource>,
    pub commands: Vec<RuntimeCommand>,
    pub queries: Vec<RuntimeQuery>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeResource {
    /// Lower-case canonical name, e.g. `customer`. Used for table name and
    /// file scope. The Go struct is the PascalCase variant.
    pub name: String,
    /// Tenancy axis. `Org` is the default; `None` means resource-level
    /// opt-out from tenant scoping.
    pub tenancy: Tenancy,
    pub soft_delete: bool,
    /// Optional retention spec — none today, but encoded so the emitter can
    /// fill `lazuli.RetentionSpec` deterministically.
    pub retention: Option<RetentionSpec>,
    /// Resource-defined fields (excludes the runtime-managed id, org_id,
    /// created_at, updated_at, deleted_at — those are emitted automatically).
    pub fields: Vec<RuntimeField>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Tenancy {
    Org,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionSpec {
    /// Window string, e.g. "7y" or "30d".
    pub window: String,
    /// Action keyword: `anonymize`, `purge`, etc. Mirrors the DSL invariant.
    pub action: RetentionAction,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RetentionAction {
    Anonymize,
    Purge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeField {
    /// Canonical lowercase name as written in the DSL.
    pub name: String,
    pub kind: FieldKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldKind {
    Text,
    Email,
    Integer,
    Boolean,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeCommand {
    /// Short name inside the feature (e.g. `create`, `update_email`).
    /// The fully qualified name is `<feature>.<name>`.
    pub short_name: String,
    /// Free-form policy reference used for the canonical Policy.Name field.
    pub policy_name: String,
    /// Resolved atoms — pairs of (namespace, name) like ("role", "admin").
    pub policy_atoms: Vec<(String, String)>,
    pub rate_limit: String,
    pub validators: Vec<String>,
    pub effect: RuntimeEffect,
    /// Inputs the author wrote in the DSL. Includes route keys (e.g. `id`)
    /// and `input` block fields. Order matches the DSL.
    pub inputs: Vec<RuntimeInput>,
    pub emits: Vec<RuntimeEmit>,
    pub invalidates: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInput {
    /// Field name as it appears on the Go input struct (PascalCase).
    pub field_name: String,
    /// Source field on the resource for typing — `id` -> `lazuli.ID`,
    /// otherwise looked up in the resource's RuntimeField list.
    pub kind: FieldKind,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEmit {
    pub name: String,
    pub kind: RuntimeEmitKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeEmitKind {
    /// Derived emit: payload is the producing row from creates/updates/deletes.
    FromCreates,
    /// Explicit bind block — pairs of (column, source).
    Bind(Vec<(String, EmitSource)>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EmitSource {
    Input(String),
    Ctx(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeQuery {
    /// Short name inside the feature (e.g. `list`, `by_id`). Fully qualified
    /// is `<feature>.query.<name>`.
    pub short_name: String,
    pub kind: QueryKind,
    pub policy_name: String,
    pub policy_atoms: Vec<(String, String)>,
    /// Args fields the query accepts (besides the canonical lookup keys).
    /// For lists: optional filter inputs + the search term, etc.
    /// For lookups: the lookup keys (e.g. `id`).
    pub args: Vec<RuntimeArg>,
    /// `cache key/ttl` block. None disables caching.
    pub cache: Option<RuntimeCache>,
    pub paginate: u32,
    pub filters: Vec<RuntimeFilter>,
    pub search: Option<RuntimeSearch>,
    pub lookup_by: Vec<RuntimeLookupKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryKind {
    List,
    Lookup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeArg {
    /// Field name as it appears on the args struct. PascalCase for Go,
    /// lowercase or original for TS (the emitter handles casing).
    pub field_name: String,
    pub kind: FieldKind,
    /// Optional args become `*T` in Go and `T?` in TS.
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeCache {
    pub key: String,
    pub ttl: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeFilter {
    pub column: String,
    pub when_input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSearch {
    pub source_input: String,
    pub over: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeLookupKey {
    pub column: String,
    pub source_input: String,
}

/// Hand-built spec mirroring the runtime spike's hand-written
/// `dist/go/customer/customer.gen.go` and `dist/web/customer/src/customer.gen.ts`.
/// Replaced in a future cut with `from_module(&lazuli_ir::Module)`.
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
                    "customer.query.list".to_owned(),
                    "customer.query.global_search".to_owned(),
                ],
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
                invalidates: vec![
                    "customer.query.list".to_owned(),
                    "customer.query.by_id".to_owned(),
                ],
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
                        (
                            "customer_id".to_owned(),
                            EmitSource::Input("ID".to_owned()),
                        ),
                        (
                            "actor_id".to_owned(),
                            EmitSource::Ctx("user.id".to_owned()),
                        ),
                    ]),
                }],
                invalidates: vec![
                    "customer.query.list".to_owned(),
                    "customer.query.by_id".to_owned(),
                ],
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
