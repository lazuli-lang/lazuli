//! VOCAB-SOFT-DELETE-ACTOR-001 — soft-delete actor pair hand-rolled
//! instead of the `soft_delete by` trait.
//!
//! The `soft_delete` trait projects `deleted_at`; spec 0015 added
//! `soft_delete by`, which ALSO projects a nullable `deleted_by` actor
//! column populated from `ctx.actor` on the soft-delete write. Before the
//! trait carried the actor column, every pilot that needed *who deleted a
//! row* hand-rolled the pair — Pauta did this 54× across 10 features, each
//! tagged with a recurring `# Soft-delete` comment (e.g.
//! `media_price_tables.lzi:35-37`). This rule fires on that hand-rolled
//! shape and points at the trait.
//!
//! ## Severity
//!
//! `Warning` — the hand-rolled pair still compiles; the rule names the
//! first-class replacement (`soft_delete by`), mirroring
//! `VOCAB-CRUD-SYNTH-AVAILABLE-001` / `VOCAB-MONEY-SHAPE-001`. Non-gating
//! (`vocabulary` category). Suppressible per-file with
//! `# doctor:allow VOCAB-SOFT-DELETE-ACTOR-001`.
//!
//! ## Trigger cue / fixture
//!
//! Fires when a resource declares BOTH a `deleted_at` and a `deleted_by`
//! field by hand and does NOT carry the `soft_delete by` actor projection
//! (`Resource.soft_delete_actor == false`). Silent once the resource is
//! migrated to `soft_delete by` (the trait then owns both columns, so the
//! author no longer declares the fields). See the inline `example`
//! fixtures `fires_on_handrolled_pair` and `silent_on_soft_delete_by`.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, Resource};

/// One `VOCAB-SOFT-DELETE-ACTOR-001` finding: a resource that hand-rolls
/// the `deleted_at` + `deleted_by` pair instead of `soft_delete by`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the resource was authored in.
    pub path: PathBuf,
    /// Owning feature name (for the diagnostic envelope).
    pub feature: String,
    /// Resource name (PascalCase, e.g. `MediaPriceTable`).
    pub resource: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-SOFT-DELETE-ACTOR-001";

    /// Render the advisory message. Names the trait + the canonical
    /// `# doctor:allow` escape hatch.
    pub fn message(&self) -> String {
        format!(
            "Resource `{}` hand-rolls a `deleted_at` + `deleted_by` soft-delete pair. \
             Reach for the `soft_delete by` trait — it projects both columns \
             (`deleted_at` + a `deleted_by` actor column populated from `ctx.actor`) \
             and makes the `conventions [crud]` delete soft-delete-aware. \
             If the hand-rolled pair is intentional, add \
             `# doctor:allow VOCAB-SOFT-DELETE-ACTOR-001 — reason \"...\"`.",
            self.resource,
        )
    }
}

/// Run the rule over one feature. `path` anchors findings AND honors the
/// `# doctor:allow` opt-out (the only I/O; mirrors `crud_synth_available`).
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    if crate::allow_comment::file_contains_doctor_allow(path, Finding::CODE) {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for resource in &feature.resources {
        if let Some(finding) = check_resource(feature, resource, path) {
            findings.push(finding);
        }
    }
    findings
}

/// Detection for a single resource.
fn check_resource(feature: &Feature, resource: &Resource, path: &Path) -> Option<Finding> {
    // Already on the trait's actor form: the trait owns both columns, so
    // a hand-rolled field is not expected — nothing to nudge.
    if resource.soft_delete_actor {
        return None;
    }

    let has_deleted_at = resource.fields.iter().any(|f| f.name == "deleted_at");
    let has_deleted_by = resource.fields.iter().any(|f| f.name == "deleted_by");
    if !(has_deleted_at && has_deleted_by) {
        return None;
    }

    Some(Finding {
        path: path.to_path_buf(),
        feature: feature.name.clone(),
        resource: resource.name.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use lazuli_ir::{BuiltinType, Defaults, Field, Policies, TypeRef};

    fn field(name: &str) -> Field {
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::DateTime),
            required: false,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    fn resource_named(name: &str) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            soft_delete_actor: false,
            timestamps: None,
            fields: vec![],
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
            append_only: false,
            many_through: Vec::new(),
            restrict_on_delete: Vec::new(),
        }
    }

    fn feature_with(resource: Resource) -> Feature {
        Feature {
            name: "media_price_tables".into(),
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
            resources: vec![resource],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
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

    #[test]
    fn fires_on_handrolled_pair() {
        // example: a resource with hand-rolled `deleted_at` + `deleted_by`
        // and no `soft_delete by` trait fires the advisory.
        let mut r = resource_named("MediaPriceTable");
        r.fields.push(field("deleted_at"));
        r.fields.push(field("deleted_by"));
        let f = feature_with(r);
        let findings = check(&f, Path::new("media_price_tables.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "VOCAB-SOFT-DELETE-ACTOR-001");
        assert_eq!(findings[0].resource, "MediaPriceTable");
        let msg = findings[0].message();
        assert!(msg.contains("soft_delete by"), "{msg}");
        assert!(msg.contains("deleted_by"), "{msg}");
    }

    #[test]
    fn silent_on_soft_delete_by() {
        // example: once migrated to `soft_delete by`, the trait owns both
        // columns — the rule is silent (no hand-rolled fields present, and
        // the actor flag set short-circuits regardless).
        let mut r = resource_named("MediaPriceTable");
        r.soft_delete = true;
        r.soft_delete_actor = true;
        let f = feature_with(r);
        assert!(check(&f, Path::new("media_price_tables.lzi")).is_empty());
    }

    #[test]
    fn silent_on_deleted_at_only() {
        // A `deleted_at`-only resource (no `deleted_by`) is plain
        // `soft_delete`; nothing to nudge toward the actor form.
        let mut r = resource_named("Export");
        r.fields.push(field("deleted_at"));
        let f = feature_with(r);
        assert!(check(&f, Path::new("reports_exports.lzi")).is_empty());
    }

    #[test]
    fn respects_doctor_allow() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("lazuli_soft_delete_actor_optout");
        std::fs::create_dir_all(&dir).unwrap();
        let lzi = dir.join("media_price_tables.lzi");
        let mut fh = std::fs::File::create(&lzi).unwrap();
        writeln!(
            fh,
            "# doctor:allow VOCAB-SOFT-DELETE-ACTOR-001 — reason \"intentional\""
        )
        .unwrap();
        writeln!(fh, "feature media_price_tables").unwrap();
        let mut r = resource_named("MediaPriceTable");
        r.fields.push(field("deleted_at"));
        r.fields.push(field("deleted_by"));
        let f = feature_with(r);
        assert!(check(&f, &lzi).is_empty());
    }
}
