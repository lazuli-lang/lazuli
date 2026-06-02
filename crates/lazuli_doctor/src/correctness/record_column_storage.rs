//! `@info.record_column_jsonb` / `@correctness.record_column_cross_feature`
//! — surface the storage kind chosen for resource fields typed as a
//! user-declared `record`, and HARD-ERROR the cross-feature variant.
//!
//! Wave A cell A8 settled the canonical mapping: a resource field whose
//! type resolves to a `record X { ... }` (and the `many <Record>`
//! collection form) lowers to a `JSONB` Postgres column with Go-side
//! Scanner/Valuer round-trip — see
//! `crates/lazuli_codegen_go/src/emitter/migration_ddl/sql_column.rs:229`.
//! That choice is invisible from the `.lzi` source alone: an author who
//! writes `address: Address required` cannot tell whether the column
//! becomes a foreign key, a flattened struct, or a JSONB document
//! without reading codegen output.
//!
//! This diagnostic closes that gap by emitting one line per record-typed
//! field, naming the resource, field, record type and the `JSONB`
//! storage kind.
//!
//! ## Two faces, two severities
//!
//! The codegen emitter resolves a record type by **bare name across the
//! whole module** (`migration_ddl::sql_column::name_is_record`): a field
//! whose type name matches a `record` declared in ANY feature lowers to
//! JSONB, regardless of whether the name is qualified (`other.Address`)
//! or bare. This rule mirrors that exact resolution so doctor and
//! codegen never disagree about which columns are JSONB (the "bug #11"
//! face-disagreement class).
//!
//! Severity splits on WHERE the record lives:
//!
//! * **Same-feature** record → JSONB is the canonical value-object
//!   pattern (`record Address` + `resource Host { address: Address }`
//!   in one feature). It is correct, intended storage — a fact, not a
//!   bug. Emitted at **`info`** under the Strict profile (code
//!   [`Finding::CODE_INFO`]). Authors who knowingly chose JSONB can opt
//!   out with `# doctor:allow @info.record_column_jsonb`.
//!
//! * **Cross-feature** record reference — a `resource` in feature A
//!   whose field is typed as a `record` OWNED BY feature B (either the
//!   qualified form `b.Address`, or a bare `Address` that resolves to a
//!   record only declared in another feature). This was the rule's old
//!   blind spot: the pre-W2 check resolved records *within the current
//!   feature only* and silently skipped anything cross-feature, so the
//!   storage drift never surfaced. It is an **`error`** (code
//!   [`Finding::CODE_CROSS_FEATURE`]), unconditional across profiles,
//!   because it is almost always a concrete data-shape bug: the author
//!   reached for `b.Address` expecting a foreign-key *relation* to a
//!   resource in feature B, but the name actually resolves to a `record`
//!   value object, so codegen embeds a JSONB *copy* of B's struct into
//!   A's table instead of a `BIGINT` FK. The two features' copies then
//!   drift independently. Opt out with
//!   `# doctor:allow @correctness.record_column_cross_feature` when the
//!   embedded-document shape is genuinely intended.
//!
//! ## Scope
//!
//! Resolution is by bare record name across the full feature set (it
//! must match codegen). The cross-feature error fires when a resource
//! field's record type resolves to a record declared ONLY in another
//! feature. A field whose type resolves to a `resource` (foreign-key
//! relation, BIGINT) or an `enum` is NOT flagged — only `record` types
//! reach the JSONB lowering. See the `tests` module for the
//! same-feature, cross-feature-qualified, cross-feature-bare, and
//! shared-name fixtures.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, TypeRef};

// ── global record index ───────────────────────────────────────────────────────

/// A record declared somewhere in the module: its bare name plus the
/// feature that owns the declaration. Built once over the full feature
/// set so [`check`] can resolve a field's record type the same way the
/// codegen emitter does (`migration_ddl::sql_column::name_is_record`) —
/// by bare name, across features.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordOwner {
    /// Bare record name (e.g. `Address`).
    pub name: String,
    /// Feature that declares this record.
    pub feature: String,
}

/// Build the cross-feature record index from every feature's `records`.
///
/// A record name declared in more than one feature appears once per
/// declaring feature — the resolver in [`check`] treats a name as
/// same-feature whenever ANY declaration lives in the current feature
/// (mirrors the codegen `name_is_record` "declared in any feature"
/// posture, which never falls back to TEXT for a shared value object).
///
/// ## Examples
///
/// ```ignore
/// use lazuli_doctor::correctness::record_column_storage::build_record_index;
/// let features: Vec<lazuli_ir::Feature> = vec![];
/// let index = build_record_index(&features);
/// assert!(index.is_empty());
/// ```
pub fn build_record_index(features: &[Feature]) -> Vec<RecordOwner> {
    let mut out = Vec::new();
    for feature in features {
        for record in &feature.records {
            out.push(RecordOwner {
                name: record.name.clone(),
                feature: feature.name.clone(),
            });
        }
    }
    out
}

// ── output ───────────────────────────────────────────────────────────────────

/// One finding: a resource field that lowers to a JSONB column because
/// its type resolves to a `record` (same-feature or cross-feature).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub resource: String,
    pub field: String,
    /// Name of the record the field's type resolves to. For
    /// `Many<Record>` carries the inner record name; the storage
    /// shape stays JSONB (one JSONB array document, not `JSONB[]`).
    pub record_type: String,
    /// `true` when the field is `Many<Record>` (collection of records);
    /// `false` when it is a scalar `Record`. Both lower to JSONB —
    /// callers that want to distinguish array vs scalar can branch
    /// here without re-walking the field's type_ref.
    pub many: bool,
    /// `Some(owning_feature)` when the record is owned by a DIFFERENT
    /// feature than the resource (the cross-feature drift → `error`).
    /// `None` when the record is declared in the same feature as the
    /// resource (the benign value-object case → `info`).
    pub cross_feature_owner: Option<String>,
}

impl Finding {
    /// Stable code for the **same-feature** (info) case.
    pub const CODE_INFO: &'static str = "@info.record_column_jsonb";

    /// Stable code for the **cross-feature** (error) case — a resource
    /// field typed as a record owned by another feature.
    pub const CODE_CROSS_FEATURE: &'static str = "@correctness.record_column_cross_feature";

    /// Back-compat alias: the original single-code surface. Callers that
    /// only ever produced same-feature findings used `Finding::CODE`.
    pub const CODE: &'static str = Self::CODE_INFO;

    /// `true` when this finding is the cross-feature (error) variant.
    pub fn is_cross_feature(&self) -> bool {
        self.cross_feature_owner.is_some()
    }

    /// The diagnostic code this finding should be emitted under —
    /// `CODE_CROSS_FEATURE` for the cross-feature drift, `CODE_INFO`
    /// otherwise.
    pub fn code(&self) -> &'static str {
        if self.is_cross_feature() {
            Self::CODE_CROSS_FEATURE
        } else {
            Self::CODE_INFO
        }
    }

    /// Render the "stored as JSONB" message naming the resource, field,
    /// and record type the column resolves to. The cross-feature variant
    /// names the owning feature and warns about the FK-vs-record drift.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::record_column_storage::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     feature: "billing".into(),
    ///     resource: "Invoice".into(),
    ///     field: "metadata".into(),
    ///     record_type: "InvoiceMeta".into(),
    ///     many: false,
    ///     cross_feature_owner: None,
    /// };
    /// assert!(f.message().contains("JSONB"));
    /// ```
    pub fn message(&self) -> String {
        match &self.cross_feature_owner {
            None => format!(
                "resource '{}' field '{}' is typed as record '{}'; stored as JSONB in the database.",
                self.resource, self.field, self.record_type,
            ),
            Some(owner) => format!(
                "resource '{resource}' field '{field}' is typed as record '{record}' which is \
                 declared in feature '{owner}', not '{feature}'. Codegen lowers it to a JSONB copy \
                 of '{record}' embedded in '{resource}', not a foreign-key relation to feature \
                 '{owner}'. If you intended a relation, point the field at a `resource` in \
                 '{owner}'; if the embedded-document copy is intended, add \
                 `# doctor:allow {code}`.",
                resource = self.resource,
                field = self.field,
                record = self.record_type,
                owner = owner,
                feature = self.feature,
                code = Self::CODE_CROSS_FEATURE,
            ),
        }
    }
}

// ── detection ────────────────────────────────────────────────────────────────

/// Run the diagnostic against every resource field in `feature`,
/// resolving record types against the cross-feature `record_index`
/// (built once via [`build_record_index`] over the full feature set).
///
/// Emits one `Finding` per field whose type resolves to a `record` —
/// both the scalar (`field: Record`) and collection (`field: many
/// Record`) forms. The `cross_feature_owner` field distinguishes the
/// same-feature (info) case from the cross-feature (error) drift.
///
/// `path` anchors findings to the source `.lzi`; no I/O is performed
/// beyond reading the per-file `doctor:allow` comment.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::record_column_storage::{build_record_index, check};
/// use lazuli_ir::Feature;
///
/// let features: Vec<Feature> = vec![/* lowered features */];
/// let index = build_record_index(&features);
/// let _ = check(&features[0], &index, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, record_index: &[RecordOwner], path: &Path) -> Vec<Finding> {
    if record_index.is_empty() || feature.resources.is_empty() {
        return Vec::new();
    }

    // 2026-05-27 — honor `# doctor:allow @info.record_column_jsonb` and
    // `# doctor:allow @correctness.record_column_cross_feature` on the
    // feature .lzi. The same-feature line is a pure FYI ("this
    // record-typed field stores as JSONB"); the cross-feature line is a
    // real error escape hatch for an intentionally-embedded value object.
    let allow_info = crate::allow_comment::file_contains_doctor_allow(path, Finding::CODE_INFO);
    let allow_cross =
        crate::allow_comment::file_contains_doctor_allow(path, Finding::CODE_CROSS_FEATURE);

    // Records declared in THIS feature — a name present here is the
    // benign same-feature value object.
    let local_record_names: HashSet<&str> = feature
        .records
        .iter()
        .map(|record| record.name.as_str())
        .collect();

    let mut out = Vec::new();
    for resource in &feature.resources {
        for field in &resource.fields {
            let Some((record_type, many)) = resolves_to_record(&field.type_ref) else {
                continue;
            };

            // Resolve the record name across the whole module, exactly
            // like codegen's `name_is_record`. A name is same-feature
            // when this feature declares it; otherwise look for a
            // declaring feature elsewhere.
            let cross_feature_owner = if local_record_names.contains(record_type) {
                None
            } else {
                match owning_feature(record_index, record_type) {
                    Some(owner) => Some(owner.to_owned()),
                    // The name resolves to no record anywhere — it is a
                    // resource FK (BIGINT) or enum (BIGINT/TEXT), NOT a
                    // JSONB record column. Skip it.
                    None => continue,
                }
            };

            // Honor the matching opt-out.
            if cross_feature_owner.is_some() {
                if allow_cross {
                    continue;
                }
            } else if allow_info {
                continue;
            }

            out.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                resource: resource.name.clone(),
                field: field.name.clone(),
                record_type: record_type.to_owned(),
                many,
                cross_feature_owner,
            });
        }
    }
    out
}

// ── internals ────────────────────────────────────────────────────────────────

/// Returns `Some((record_name, many))` when `type_ref` is a
/// `UserDefined` (scalar) or `Many<UserDefined>` (collection) type — the
/// two shapes codegen resolves against the record index. The QUALIFIER
/// (`qname.feature`) is deliberately ignored here: codegen resolves by
/// bare name across the module, so both `Address` and `other.Address`
/// reach the record lookup. The caller decides same- vs cross-feature
/// from the record index, NOT from the qualifier (the analyzer often
/// leaves `qname.feature = None` even for a cross-feature reference).
fn resolves_to_record(type_ref: &TypeRef) -> Option<(&str, bool)> {
    match type_ref {
        TypeRef::UserDefined(qname) => Some((qname.name.as_str(), false)),
        TypeRef::Many(inner) => match inner.as_ref() {
            TypeRef::UserDefined(qname) => Some((qname.name.as_str(), true)),
            _ => None,
        },
        _ => None,
    }
}

/// First feature that declares a record named `name`, if any. Mirrors
/// codegen's "declared in any feature" resolution.
fn owning_feature<'a>(record_index: &'a [RecordOwner], name: &str) -> Option<&'a str> {
    record_index
        .iter()
        .find(|owner| owner.name == name)
        .map(|owner| owner.feature.as_str())
}

// ── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Defaults, Field, FieldConstraints, Policies, QualifiedName, Record, Resource,
        TypeRef,
    };

    fn mk_path() -> &'static Path {
        Path::new("features/hosts/host.lzi")
    }

    fn mk_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    fn mk_record(name: &str) -> Record {
        Record {
            name: name.to_owned(),
            public_contract: None,
            fields: vec![
                mk_field("street", TypeRef::Builtin(BuiltinType::Text)),
                mk_field("city", TypeRef::Builtin(BuiltinType::Text)),
            ],
            discriminator_field: None,
            span_ref: None,
        }
    }

    fn mk_resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            soft_delete_actor: false,
            timestamps: None,
            fields,
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            restrict_on_delete: Vec::new(),
            append_only: false,
        }
    }

    fn mk_feature(name: &str, records: Vec<Record>, resources: Vec<Resource>) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources,
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records,
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    fn local_user_defined(name: &str) -> TypeRef {
        TypeRef::UserDefined(QualifiedName {
            feature: None,
            name: name.to_owned(),
        })
    }

    fn qualified_user_defined(feature: &str, name: &str) -> TypeRef {
        TypeRef::UserDefined(QualifiedName {
            feature: Some(feature.to_owned()),
            name: name.to_owned(),
        })
    }

    /// Convenience: build the index over a single-feature set and run
    /// `check` against that feature.
    fn check_single(feature: &Feature) -> Vec<Finding> {
        let features = vec![feature.clone()];
        let index = build_record_index(&features);
        check(&features[0], &index, mk_path())
    }

    // ── canonical fixture (same-feature → info) ───────────────────────────────

    /// `record Address { ... }` + `resource Host { address: Address }`
    /// fires ONE same-feature finding (info code) with the right field,
    /// type, and message.
    #[test]
    fn positive_address_field_on_host_fires_same_feature_info() {
        let feature = mk_feature(
            "hosts",
            vec![mk_record("Address")],
            vec![mk_resource(
                "Host",
                vec![mk_field("address", local_user_defined("Address"))],
            )],
        );

        let findings = check_single(&feature);

        assert_eq!(findings.len(), 1, "expected exactly one finding");
        let finding = &findings[0];
        assert!(!finding.is_cross_feature(), "same-feature record is info");
        assert_eq!(finding.code(), "@info.record_column_jsonb");
        assert_eq!(finding.feature, "hosts");
        assert_eq!(finding.resource, "Host");
        assert_eq!(finding.field, "address");
        assert_eq!(finding.record_type, "Address");
        assert!(!finding.many, "scalar Record case must report many=false");

        assert_eq!(
            finding.message(),
            "resource 'Host' field 'address' is typed as record 'Address'; stored as JSONB in the database."
        );
    }

    // ── cross-feature (qualified) → error ─────────────────────────────────────

    /// `resource Host { address: billing.Address }` where `Address` is a
    /// `record` declared in the `billing` feature (NOT in `hosts`) fires
    /// the cross-feature ERROR code, names the owning feature, and warns
    /// about the FK-vs-record drift. This is the old blind spot
    /// (`negative_cross_feature_record_reference_does_not_fire`).
    #[test]
    fn positive_cross_feature_qualified_record_fires_error() {
        let billing = mk_feature("billing", vec![mk_record("Address")], vec![]);
        let hosts = mk_feature(
            "hosts",
            vec![],
            vec![mk_resource(
                "Host",
                vec![mk_field("address", qualified_user_defined("billing", "Address"))],
            )],
        );
        let features = vec![billing, hosts];
        let index = build_record_index(&features);

        let findings = check(&features[1], &index, mk_path());
        assert_eq!(findings.len(), 1, "expected one cross-feature finding");
        let finding = &findings[0];
        assert!(finding.is_cross_feature(), "cross-feature record is error");
        assert_eq!(finding.code(), "@correctness.record_column_cross_feature");
        assert_eq!(finding.cross_feature_owner.as_deref(), Some("billing"));
        assert_eq!(finding.record_type, "Address");
        let msg = finding.message();
        assert!(msg.contains("declared in feature 'billing'"), "msg: {msg}");
        assert!(msg.contains("not a foreign-key relation"), "msg: {msg}");
        assert!(
            msg.contains("@correctness.record_column_cross_feature"),
            "cross-feature message must name its allow-code: {msg}"
        );
    }

    /// The SAME blind spot via an UNQUALIFIED name: the analyzer often
    /// leaves `qname.feature = None` even for a cross-feature reference,
    /// so a bare `Address` that resolves to a record only declared in
    /// `billing` must still fire the cross-feature error (codegen lowers
    /// it to JSONB by bare-name resolution).
    #[test]
    fn positive_cross_feature_bare_name_fires_error() {
        let billing = mk_feature("billing", vec![mk_record("Address")], vec![]);
        let hosts = mk_feature(
            "hosts",
            vec![],
            vec![mk_resource(
                "Host",
                vec![mk_field("address", local_user_defined("Address"))],
            )],
        );
        let features = vec![billing, hosts];
        let index = build_record_index(&features);

        let findings = check(&features[1], &index, mk_path());
        assert_eq!(findings.len(), 1);
        assert!(findings[0].is_cross_feature());
        assert_eq!(findings[0].cross_feature_owner.as_deref(), Some("billing"));
    }

    /// A record name declared in BOTH the current feature and another
    /// feature (pauta's shared `Address` value object) is treated as
    /// SAME-feature (info), not cross-feature — the local declaration
    /// wins, matching codegen's "declared in any feature ⇒ JSONB"
    /// posture and keeping the benign value-object pattern quiet.
    #[test]
    fn shared_record_name_with_local_decl_stays_info() {
        let supplier = mk_feature("supplier", vec![mk_record("Address")], vec![]);
        let customer = mk_feature(
            "customer",
            vec![mk_record("Address")],
            vec![mk_resource(
                "Customer",
                vec![mk_field("address", local_user_defined("Address"))],
            )],
        );
        let features = vec![supplier, customer];
        let index = build_record_index(&features);

        let findings = check(&features[1], &index, mk_path());
        assert_eq!(findings.len(), 1);
        assert!(
            !findings[0].is_cross_feature(),
            "a locally-declared record stays same-feature info even if another feature shares the name"
        );
    }

    // ── edge cases: no record fields / empty index ────────────────────────────

    /// A resource with no record-typed fields emits zero diagnostics.
    #[test]
    fn negative_resource_without_record_fields_does_not_fire() {
        let feature = mk_feature(
            "hosts",
            vec![mk_record("Address")],
            vec![mk_resource(
                "Host",
                vec![
                    mk_field("id", TypeRef::Builtin(BuiltinType::Id)),
                    mk_field("name", TypeRef::Builtin(BuiltinType::Text)),
                    mk_field("active", TypeRef::Builtin(BuiltinType::Boolean)),
                ],
            )],
        );
        assert!(check_single(&feature).is_empty());
    }

    /// An empty record index short-circuits — no record anywhere in the
    /// module means no JSONB record columns.
    #[test]
    fn negative_empty_record_index_short_circuits() {
        let feature = mk_feature(
            "hosts",
            vec![],
            vec![mk_resource(
                "Host",
                vec![mk_field("name", TypeRef::Builtin(BuiltinType::Text))],
            )],
        );
        assert!(check_single(&feature).is_empty());
    }

    /// A feature with records but no resources is a no-op.
    #[test]
    fn negative_feature_without_resources_short_circuits() {
        let feature = mk_feature("hosts", vec![mk_record("Address")], vec![]);
        assert!(check_single(&feature).is_empty());
    }

    // ── collection form: many <Record> ───────────────────────────────────────

    /// `addresses: many Address` (same-feature) lowers to a single JSONB
    /// array column and fires the info code with `many = true`.
    #[test]
    fn positive_many_record_field_fires_with_many_flag() {
        let feature = mk_feature(
            "hosts",
            vec![mk_record("Address")],
            vec![mk_resource(
                "Host",
                vec![mk_field(
                    "addresses",
                    TypeRef::Many(Box::new(local_user_defined("Address"))),
                )],
            )],
        );

        let findings = check_single(&feature);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "addresses");
        assert_eq!(findings[0].record_type, "Address");
        assert!(findings[0].many, "many<Record> case must report many=true");
        assert!(!findings[0].is_cross_feature());
    }

    /// `many billing.Address` (cross-feature collection) fires the
    /// cross-feature error with `many = true`.
    #[test]
    fn positive_many_cross_feature_record_fires_error() {
        let billing = mk_feature("billing", vec![mk_record("Address")], vec![]);
        let hosts = mk_feature(
            "hosts",
            vec![],
            vec![mk_resource(
                "Host",
                vec![mk_field(
                    "addresses",
                    TypeRef::Many(Box::new(qualified_user_defined("billing", "Address"))),
                )],
            )],
        );
        let features = vec![billing, hosts];
        let index = build_record_index(&features);

        let findings = check(&features[1], &index, mk_path());
        assert_eq!(findings.len(), 1);
        assert!(findings[0].is_cross_feature());
        assert!(findings[0].many);
    }

    // ── exclusions: don't false-positive on resources / enums ─────────────────

    /// A field whose type resolves to a sibling `resource` (foreign-key
    /// relation, BIGINT) is NOT flagged — it is not a record, so it never
    /// reaches the JSONB lowering. A clean cross-feature reference to a
    /// RESOURCE in another feature must likewise stay quiet.
    #[test]
    fn negative_field_pointing_at_resource_does_not_fire() {
        // Same-feature resource ref + a cross-feature ref to a resource
        // that is NOT a record anywhere.
        let other = mk_feature(
            "other",
            vec![],
            vec![mk_resource(
                "Org",
                vec![mk_field("id", TypeRef::Builtin(BuiltinType::Id))],
            )],
        );
        let hosts = mk_feature(
            "hosts",
            vec![mk_record("Address")],
            vec![
                mk_resource(
                    "LocalOrg",
                    vec![mk_field("id", TypeRef::Builtin(BuiltinType::Id))],
                ),
                mk_resource(
                    "Host",
                    vec![
                        // FK to a same-feature resource — not a record.
                        mk_field("org", local_user_defined("LocalOrg")),
                        // "FK" to a cross-feature resource — not a record
                        // anywhere → must NOT false-positive.
                        mk_field("remote_org", qualified_user_defined("other", "Org")),
                        // The one real record field.
                        mk_field("address", local_user_defined("Address")),
                    ],
                ),
            ],
        );
        let features = vec![other, hosts];
        let index = build_record_index(&features);

        let findings = check(&features[1], &index, mk_path());
        assert_eq!(
            findings.len(),
            1,
            "only the record-typed field fires; resource FKs (local + cross-feature) stay quiet"
        );
        assert_eq!(findings[0].field, "address");
        assert!(!findings[0].is_cross_feature());
    }

    // ── multi-resource / multi-field coverage ────────────────────────────────

    /// Multiple same-feature record-typed fields each fire independently
    /// across resources.
    #[test]
    fn positive_multiple_record_fields_each_fire() {
        let feature = mk_feature(
            "hosts",
            vec![mk_record("Address"), mk_record("Geo")],
            vec![
                mk_resource(
                    "Host",
                    vec![
                        mk_field("billing", local_user_defined("Address")),
                        mk_field("shipping", local_user_defined("Address")),
                        mk_field("location", local_user_defined("Geo")),
                    ],
                ),
                mk_resource(
                    "Property",
                    vec![mk_field("coords", local_user_defined("Geo"))],
                ),
            ],
        );

        let findings = check_single(&feature);
        assert_eq!(findings.len(), 4);
        let fields: Vec<&str> = findings.iter().map(|f| f.field.as_str()).collect();
        assert!(fields.contains(&"billing"));
        assert!(fields.contains(&"shipping"));
        assert!(fields.contains(&"location"));
        assert!(fields.contains(&"coords"));
        assert!(findings.iter().all(|f| !f.is_cross_feature()));
    }
}
