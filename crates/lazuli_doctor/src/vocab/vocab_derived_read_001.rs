//! VOCAB-DERIVED-READ-001 — handler-computed read-only field drift.
//!
//! Fires when a resource field is:
//!   - optional (not `required`)
//!   - has no explicit `default` value
//!   - has no `@cap.*` capability tier
//!   - has no existing `derived from <expr>` annotation
//!   - is NEVER assigned a value in any `command.creates.<field> =`,
//!     `command.updates.<field> =`, `job.creates.<field> =`, or
//!     `job.updates.<field> =` write site
//!
//! These fields are likely computed every read — the vocabulary already
//! names `derived from <expression>` for exactly this intent
//! (docs/invariants.md:89-92).
//!
//! Detection is IR-walk only (no handler Go source needed).
//!
//! Severity: `warning` (strict-profile), `warning` (production-profile).
//! Lower severity than VOCAB-UNION-001 because materialised computed values
//! are a legitimate optimisation for indexability.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lazuli_ir::{self as ir, CommandEffect, Feature, TypeRef};

// ── output ───────────────────────────────────────────────────────────────────

/// One VOCAB-DERIVED-READ-001 finding: a resource field that is never
/// written by any command or declarative job and should likely be
/// `derived from <expr>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Resource name.
    pub resource: String,
    /// Field name flagged as never-written.
    pub field: String,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-DERIVED-READ-001";

    pub fn message(&self) -> String {
        format!(
            "field `{}.{}` is never written by any command or job — \
             if it is computed at read time, consider `derived from <expr>` \
             (docs/invariants.md:89-92)",
            self.resource, self.field,
        )
    }
}

// ── detection ────────────────────────────────────────────────────────────────

/// Run VOCAB-DERIVED-READ-001 over one feature's resources.
///
/// `path` is the source `.lzi` file; no I/O is performed here.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let written = collect_write_sites(&feature.commands, &feature.jobs);
    feature
        .resources
        .iter()
        .flat_map(|r| check_resource(r, &written, path))
        .collect()
}

// ── internals ────────────────────────────────────────────────────────────────

/// Sentinel inserted into the write-set when a command uses `creates X from
/// input`, signalling that field-level write enumeration is unavailable and
/// the entire resource should be treated as fully written.
const FROM_INPUT_SENTINEL: &str = "\0from_input";

/// Check a single resource for never-written optional fields.
fn check_resource(
    resource: &ir::Resource,
    written: &BTreeMap<String, BTreeSet<String>>,
    path: &Path,
) -> Vec<Finding> {
    let mut out = Vec::new();

    // If any command/job does `creates <Resource> from input`, all fields of
    // that resource may be written via the input mapping; skip to avoid FPs.
    if written
        .get(&resource.name)
        .map_or(false, |s| s.contains(FROM_INPUT_SENTINEL))
    {
        return out;
    }

    let written_here = written.get(&resource.name);
    for field in &resource.fields {
        if should_skip(field, written_here) {
            continue;
        }
        out.push(Finding {
            path: path.to_path_buf(),
            resource: resource.name.clone(),
            field: field.name.clone(),
        });
    }
    out
}

/// Returns `true` if the field should NOT trigger the diagnostic.
fn should_skip(
    field: &ir::Field,
    written_fields: Option<&BTreeSet<String>>,
) -> bool {
    // Primary key — runtime-managed, never a derived candidate.
    if field.name == "id" {
        return true;
    }
    // Already annotated `derived from <expr>` — nothing to suggest.
    if field.derived_from.is_some() {
        return true;
    }
    // Required fields must be set on creates — a "never written" signal here
    // is more likely a different bug than a derived-field opportunity.
    if field.required {
        return true;
    }
    // Explicit `default` implies intentional storage semantics.
    if field.default.is_some() {
        return true;
    }
    // `@cap.*` capability tiers imply storage semantics incompatible with
    // `derived from` (e.g. @cap.Hashed stores a hash, @cap.Token issues tokens).
    if matches!(&field.type_ref, TypeRef::Capability(_)) {
        return true;
    }
    // Field is explicitly assigned in at least one write site.
    if written_fields.map_or(false, |s| s.contains(&field.name)) {
        return true;
    }
    false
}

/// Collect all field names assigned in `creates`/`updates` effects across
/// commands and declarative jobs, keyed by the local resource name.
fn collect_write_sites(
    commands: &[ir::Command],
    jobs: &[ir::Job],
) -> BTreeMap<String, BTreeSet<String>> {
    let mut written: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for cmd in commands {
        add_effect_writes(&cmd.effect, &mut written);
    }
    for job in jobs {
        if let ir::JobBody::Declarative(decl) = &job.body {
            add_effect_writes(&decl.effect, &mut written);
        }
    }
    written
}

fn add_effect_writes(
    effect: &CommandEffect,
    written: &mut BTreeMap<String, BTreeSet<String>>,
) {
    match effect {
        CommandEffect::Creates(c) => {
            let entry = written.entry(c.resource.name.clone()).or_default();
            for a in &c.assignments {
                entry.insert(a.field.clone());
            }
            // `creates X from input` writes fields via the input type mapping;
            // we can't enumerate them, so insert a sentinel to skip the resource.
            if c.from_input {
                entry.insert(FROM_INPUT_SENTINEL.to_owned());
            }
        }
        CommandEffect::Updates(u) => {
            let entry = written.entry(u.resource.name.clone()).or_default();
            for a in &u.assignments {
                entry.insert(a.field.clone());
            }
        }
        CommandEffect::Deletes(_) | CommandEffect::Returns(_) | CommandEffect::None => {}
    }
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use lazuli_ir::{
        Assignment, BuiltinType, CapabilityRef, CommandInput, CommandKind, CreateEffect,
        DefaultValue, Defaults, Expr, Feature, Field, HashedCapability, HashAlgorithm,
        Job, JobBody, JobDeclarative, JobTrigger, Policies, QualifiedName, Resource, TypeRef,
    };
    use std::path::Path;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn mk_feature(resources: Vec<Resource>, commands: Vec<ir::Command>, jobs: Vec<Job>) -> Feature {
        Feature {
            name: "test_feature".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
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
            commands,
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs,
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
            span_ref: None,
        }
    }

    fn mk_resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
            name: name.into(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
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
        }
    }

    fn text_field_opt(name: &str) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: false,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn text_field_req(name: &str) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_create_cmd(resource: &str, field_names: &[&str]) -> ir::Command {
        let assignments = field_names
            .iter()
            .map(|f| Assignment {
                field: f.to_string(),
                value: Expr::String("value".into()),
            })
            .collect();
        ir::Command {
            name: "create_cmd".into(),
            public_contract: None,
            kind: CommandKind::Create,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Creates(CreateEffect {
                resource: QualifiedName {
                    feature: None,
                    name: resource.into(),
                },
                from_input: false,
                assignments,
            }),
            policy: ir::PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: vec![],
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_create_from_input_cmd(resource: &str) -> ir::Command {
        ir::Command {
            name: "create_from_input".into(),
            public_contract: None,
            kind: CommandKind::Create,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Creates(CreateEffect {
                resource: QualifiedName {
                    feature: None,
                    name: resource.into(),
                },
                from_input: true,
                assignments: vec![],
            }),
            policy: ir::PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: vec![],
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    // ── positive ─────────────────────────────────────────────────────────────

    /// Optional field never assigned in any command → fires.
    #[test]
    fn positive_never_written_optional_fires() {
        let resource = mk_resource("Post", vec![text_field_opt("canonical_url")]);
        let feature = mk_feature(vec![resource], vec![], vec![]);
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Post");
        assert_eq!(findings[0].field, "canonical_url");
        assert_eq!(Finding::CODE, "VOCAB-DERIVED-READ-001");
        assert!(findings[0].message().contains("derived from"));
    }

    // ── negative (i): field assigned in creates ───────────────────────────────

    /// Field explicitly assigned in a `creates` command → no finding.
    #[test]
    fn negative_assigned_in_creates() {
        let resource = mk_resource("Post", vec![text_field_opt("canonical_url")]);
        let cmd = mk_create_cmd("Post", &["canonical_url"]);
        let feature = mk_feature(vec![resource], vec![cmd], vec![]);
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert!(
            findings.is_empty(),
            "field with a creates assignment must not fire"
        );
    }

    // ── negative (ii): field has @cap.* tier ─────────────────────────────────

    /// Field typed `@cap.Hashed` → storage semantics imply it is persisted.
    #[test]
    fn negative_cap_field_skipped() {
        let cap_field = Field {
            name: "password_hash".into(),
            type_ref: TypeRef::Capability(CapabilityRef::Hashed(HashedCapability {
                algorithm: HashAlgorithm::Argon2id,
            })),
            required: false,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            span_ref: None,
        };
        let resource = mk_resource("User", vec![cap_field]);
        let feature = mk_feature(vec![resource], vec![], vec![]);
        let findings = check(&feature, Path::new("features/user/user.lzi"));
        assert!(
            findings.is_empty(),
            "@cap.* field must not trigger VOCAB-DERIVED-READ-001"
        );
    }

    // ── negative (iii): field has default ────────────────────────────────────

    /// Field with an explicit `default` value → storage intent, not derived.
    #[test]
    fn negative_default_value_skipped() {
        let default_field = Field {
            name: "status".into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: false,
            unique: false,
            slug: false,
            default: Some(DefaultValue::String("active".into())),
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            span_ref: None,
        };
        let resource = mk_resource("Account", vec![default_field]);
        let feature = mk_feature(vec![resource], vec![], vec![]);
        let findings = check(&feature, Path::new("features/account/account.lzi"));
        assert!(
            findings.is_empty(),
            "field with a default value must not fire"
        );
    }

    // ── negative (iv): `creates X from input` suppresses the resource ─────────

    /// When `creates Post from input` is present, all Post fields are
    /// treated as potentially written → no finding even for `canonical_url`.
    #[test]
    fn negative_from_input_suppresses_resource() {
        let resource = mk_resource("Post", vec![text_field_opt("canonical_url")]);
        let cmd = mk_create_from_input_cmd("Post");
        let feature = mk_feature(vec![resource], vec![cmd], vec![]);
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert!(
            findings.is_empty(),
            "`creates X from input` must suppress the resource from VOCAB-DERIVED-READ-001"
        );
    }

    // ── negative (v): already derived_from ───────────────────────────────────

    /// Field already annotated with `derived from <expr>` → skip.
    #[test]
    fn negative_already_derived() {
        let derived_field = Field {
            name: "canonical_url".into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: false,
            unique: false,
            slug: false,
            default: None,
            derived_from: Some("\"https://example.com/p/{{slug}}\"".into()),
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            span_ref: None,
        };
        let resource = mk_resource("Post", vec![derived_field]);
        let feature = mk_feature(vec![resource], vec![], vec![]);
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert!(
            findings.is_empty(),
            "already-derived field must not fire"
        );
    }

    // ── positive: declarative job write site suppresses ──────────────────────

    /// A declarative job that assigns the field counts as a write site.
    #[test]
    fn negative_job_write_site_suppresses() {
        let resource = mk_resource("Post", vec![text_field_opt("canonical_url")]);
        let job = Job {
            name: "rebuild_slug_job".into(),
            trigger: JobTrigger::Schedule {
                cron: "0 * * * *".into(),
            },
            queue: None,
            idempotency: None,
            retry: None,
            policy: None,
            policy_expr: None,
            policy_when_denied: None,
            tenant_from: None,
            fanout: None,
            timeout: None,
            external_calls: vec![],
            body: JobBody::Declarative(JobDeclarative {
                target: None,
                lets: vec![],
                effect: CommandEffect::Creates(CreateEffect {
                    resource: QualifiedName {
                        feature: None,
                        name: "Post".into(),
                    },
                    from_input: false,
                    assignments: vec![Assignment {
                        field: "canonical_url".into(),
                        value: Expr::String("computed".into()),
                    }],
                }),
            }),
            emits: vec![],
            previous_names: vec![],
            span_ref: None,
        };
        let feature = mk_feature(vec![resource], vec![], vec![job]);
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert!(
            findings.is_empty(),
            "declarative job write site must suppress VOCAB-DERIVED-READ-001"
        );
    }

    // ── negative: required field skipped ─────────────────────────────────────

    /// Required fields must be set on creates — not a derived-field candidate.
    #[test]
    fn negative_required_field_skipped() {
        let resource = mk_resource("Post", vec![text_field_req("slug")]);
        let feature = mk_feature(vec![resource], vec![], vec![]);
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert!(
            findings.is_empty(),
            "required fields must not trigger VOCAB-DERIVED-READ-001"
        );
    }
}
