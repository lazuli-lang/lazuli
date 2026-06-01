//! `@info.record_column_jsonb` — surface the storage kind chosen for
//! resource fields typed as a user-declared `record`.
//!
//! Wave A cell A8 settled the canonical mapping: a resource field whose
//! type resolves to a `record X { ... }` (and the `many <Record>`
//! collection form) lowers to a `JSONB` Postgres column with Go-side
//! Scanner/Valuer round-trip — see
//! `crates/lazuli_codegen_go/src/emitter/migration_ddl.rs:643`. That
//! choice is invisible from the `.lzi` source alone: an author who
//! writes `address: Address required` cannot tell whether the column
//! becomes a foreign key, a flattened struct, or a JSONB document
//! without reading codegen output.
//!
//! This diagnostic closes that gap by emitting one info-level line per
//! record-typed field, naming the resource, field, record type and the
//! `JSONB` storage kind. The intent is purely informational — authors
//! who reach for `record` as a typed JSON document should see that the
//! tooling agrees with them; authors who expected a foreign-key
//! relation see the mismatch immediately and can switch to a
//! `resource` declaration.
//!
//! Severity: `info` — this is a fact, not a bug. Info-level
//! diagnostics surface in `lazuli doctor` output and `lazuli_cli`'s
//! `DoctorSeverity::Info` rendering (`crates/lazuli_cli/src/doctor.rs:2231`)
//! but do not block CI gates that fail on Error/Warning.
//!
//! Scope: same-feature resolution only. A field typed as
//! `other_feature.Record` is intentionally skipped — cross-feature
//! record propagation is a future cycle (mirrors
//! `channel_payload_unresolved_001`'s MVP scope).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, TypeRef};

// ── output ───────────────────────────────────────────────────────────────────

/// One finding: a resource field that lowers to a JSONB column because
/// its type resolves to a same-feature `record`.
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
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "@info.record_column_jsonb";

    /// Render the "stored as JSONB" informational message naming the
    /// resource, field, and record type the column resolves to.
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
    /// };
    /// assert!(f.message().contains("JSONB"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "resource '{}' field '{}' is typed as record '{}'; stored as JSONB in the database.",
            self.resource, self.field, self.record_type,
        )
    }
}

// ── detection ────────────────────────────────────────────────────────────────

/// Run the diagnostic against every resource field in `feature`. Emits
/// one `Finding` for each field whose type resolves to a same-feature
/// `record` — both the scalar (`field: Record`) and collection
/// (`field: many Record`) forms.
///
/// `path` anchors findings to the source `.lzi`; no I/O is performed.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::record_column_storage::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with record-typed columns");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    if feature.records.is_empty() || feature.resources.is_empty() {
        return Vec::new();
    }

    // 2026-05-27 — honor `# doctor:allow @info.record_column_jsonb`
    // on the feature .lzi. The rule is purely informational ("this
    // record-typed field stores as JSONB") and authors who knowingly
    // chose JSONB storage can opt out of the per-field FYI.
    if crate::allow_comment::file_contains_doctor_allow(path, Finding::CODE) {
        return Vec::new();
    }

    let record_names: HashSet<&str> = feature
        .records
        .iter()
        .map(|record| record.name.as_str())
        .collect();

    let mut out = Vec::new();
    for resource in &feature.resources {
        for field in &resource.fields {
            if let Some((record_type, many)) =
                resolves_to_local_record(&field.type_ref, &record_names)
            {
                out.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    resource: resource.name.clone(),
                    field: field.name.clone(),
                    record_type: record_type.to_owned(),
                    many,
                });
            }
        }
    }
    out
}

// ── internals ────────────────────────────────────────────────────────────────

/// Returns `Some((record_name, many))` when `type_ref` resolves to a
/// same-feature record. `many == true` when the type is
/// `Many<UserDefined<Record>>`; the storage shape is JSONB in both
/// cases (`migration_ddl.rs:643`).
fn resolves_to_local_record<'a>(
    type_ref: &'a TypeRef,
    record_names: &HashSet<&str>,
) -> Option<(&'a str, bool)> {
    match type_ref {
        TypeRef::UserDefined(qname) if qname.feature.is_none() => {
            if record_names.contains(qname.name.as_str()) {
                Some((qname.name.as_str(), false))
            } else {
                None
            }
        }
        TypeRef::Many(inner) => match inner.as_ref() {
            TypeRef::UserDefined(qname) if qname.feature.is_none() => {
                if record_names.contains(qname.name.as_str()) {
                    Some((qname.name.as_str(), true))
                } else {
                    None
                }
            }
            _ => None,
        },
        _ => None,
    }
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

    fn mk_feature(records: Vec<Record>, resources: Vec<Resource>) -> Feature {
        Feature {
            name: "hosts".to_owned(),
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

    // ── canonical fixture ────────────────────────────────────────────────────

    /// The cell's canonical fixture: `record Address { ... }` +
    /// `resource Host { address: Address required }` fires one info
    /// diagnostic with the right field, type, and message.
    #[test]
    fn positive_address_field_on_host_fires() {
        let feature = mk_feature(
            vec![mk_record("Address")],
            vec![mk_resource(
                "Host",
                vec![mk_field("address", local_user_defined("Address"))],
            )],
        );

        let findings = check(&feature, mk_path());

        assert_eq!(findings.len(), 1, "expected exactly one info diagnostic");
        let finding = &findings[0];
        assert_eq!(Finding::CODE, "@info.record_column_jsonb");
        assert_eq!(finding.feature, "hosts");
        assert_eq!(finding.resource, "Host");
        assert_eq!(finding.field, "address");
        assert_eq!(finding.record_type, "Address");
        assert!(!finding.many, "scalar Record case must report many=false");

        let msg = finding.message();
        assert_eq!(
            msg,
            "resource 'Host' field 'address' is typed as record 'Address'; stored as JSONB in the database."
        );
    }

    // ── edge case: no record fields ──────────────────────────────────────────

    /// A resource with no record-typed fields emits zero diagnostics —
    /// the rule is purely informational and must not surface noise on
    /// resources that don't carry a JSONB column.
    #[test]
    fn negative_resource_without_record_fields_does_not_fire() {
        let feature = mk_feature(
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

        assert!(
            check(&feature, mk_path()).is_empty(),
            "no record-typed fields => no diagnostics"
        );
    }

    /// A feature with no records at all short-circuits — neither
    /// resources nor records carry the JSONB storage axis.
    #[test]
    fn negative_feature_without_records_short_circuits() {
        let feature = mk_feature(
            vec![],
            vec![mk_resource(
                "Host",
                vec![mk_field("name", TypeRef::Builtin(BuiltinType::Text))],
            )],
        );
        assert!(check(&feature, mk_path()).is_empty());
    }

    /// A feature with records but no resources is also a no-op (records
    /// outside a resource field never become columns).
    #[test]
    fn negative_feature_without_resources_short_circuits() {
        let feature = mk_feature(vec![mk_record("Address")], vec![]);
        assert!(check(&feature, mk_path()).is_empty());
    }

    // ── collection form: many <Record> ───────────────────────────────────────

    /// `addresses: many Address` lowers to a single JSONB array column
    /// (codegen rule at `migration_ddl.rs:661`). The diagnostic must
    /// fire so authors see that a `many <Record>` collection still
    /// lands as JSONB — not as a separate join table.
    #[test]
    fn positive_many_record_field_fires_with_many_flag() {
        let feature = mk_feature(
            vec![mk_record("Address")],
            vec![mk_resource(
                "Host",
                vec![mk_field(
                    "addresses",
                    TypeRef::Many(Box::new(local_user_defined("Address"))),
                )],
            )],
        );

        let findings = check(&feature, mk_path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "addresses");
        assert_eq!(findings[0].record_type, "Address");
        assert!(findings[0].many, "many<Record> case must report many=true");
    }

    // ── exclusions: don't false-positive on resources / cross-feature ────────

    /// A field whose `UserDefined` type resolves to a sibling
    /// `resource` (foreign-key relation) is intentionally NOT flagged.
    /// Those columns lower to `BIGINT` (`migration_ddl.rs:637`), not
    /// JSONB.
    #[test]
    fn negative_field_pointing_at_sibling_resource_does_not_fire() {
        let feature = mk_feature(
            vec![mk_record("Address")],
            vec![
                mk_resource(
                    "Org",
                    vec![mk_field("id", TypeRef::Builtin(BuiltinType::Id))],
                ),
                mk_resource(
                    "Host",
                    vec![
                        mk_field("org", local_user_defined("Org")),
                        mk_field("address", local_user_defined("Address")),
                    ],
                ),
            ],
        );

        let findings = check(&feature, mk_path());
        assert_eq!(findings.len(), 1, "only the record-typed field fires");
        assert_eq!(findings[0].field, "address");
    }

    /// Cross-feature record references (`other_feature.Address`) are
    /// out of scope for this MVP — same-feature resolution only.
    #[test]
    fn negative_cross_feature_record_reference_does_not_fire() {
        let feature = mk_feature(
            vec![],
            vec![mk_resource(
                "Host",
                vec![mk_field(
                    "address",
                    TypeRef::UserDefined(QualifiedName {
                        feature: Some("addresses".to_owned()),
                        name: "Address".to_owned(),
                    }),
                )],
            )],
        );
        assert!(check(&feature, mk_path()).is_empty());
    }

    // ── multi-resource / multi-field coverage ────────────────────────────────

    /// Multiple record-typed fields on the same resource each fire
    /// independently, and other resources in the same feature are
    /// walked too.
    #[test]
    fn positive_multiple_record_fields_each_fire() {
        let feature = mk_feature(
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

        let findings = check(&feature, mk_path());
        assert_eq!(findings.len(), 4);
        let fields: Vec<&str> = findings.iter().map(|f| f.field.as_str()).collect();
        assert!(fields.contains(&"billing"));
        assert!(fields.contains(&"shipping"));
        assert!(fields.contains(&"location"));
        assert!(fields.contains(&"coords"));
    }
}
