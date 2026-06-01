//! `VOCAB-CRUD-SYNTH-AVAILABLE-001` — the `crud` synth run **backwards**.
//!
//! Today's conventions doctor family (`vocab/conventions.rs`) only
//! *validates existing opt-ins*. This rule does the opposite: it nudges a
//! resource that hand-rolls the `create_X`/`update_X`/`delete_X` (+
//! `lookup_X`/`list_Xs`) surface — whose command/query names match exactly
//! what `conventions [crud]` would synthesize — to drop the boilerplate and
//! opt in.
//!
//! Warns when a resource hand-rolls the full canonical `create_X`/`update_X`/
//! `delete_X` command set (signatures matching the synth) without declaring
//! `conventions [crud]`. Example: a `Note` resource with explicit
//! `create_note`/`update_note`/`delete_note` commands fires the advisory.
//!
//! Spec: `.specs/changes/0002-crud-inverse-linter/` (PRD/ADR/techspec).
//!
//! - **Advisory.** The facet row (`P_CONVENTIONS` in `lazuli_keywords`) is
//!   base severity `warning`, category `vocabulary` — kept out of the gating
//!   set, so `lazuli check`/`doctor` exit codes never change because of it.
//! - **Suppressible.** The finding is anchored on the resource's `.lzi`; a
//!   `# doctor:allow VOCAB-CRUD-SYNTH-AVAILABLE-001` in the file silences it
//!   through the shared [`crate::allow_comment`] path (mirrors
//!   `vocab_tests_missing_001`).
//! - **Soft-delete carve-out.** A matched `delete_<r>` is dropped from the
//!   suggested replacement set when the resource declares `soft_delete` or a
//!   `retention … then …` posture, because the canonical synth delete is a
//!   *hard* delete (see spec 0015). Delete stays explicit; create/update
//!   (+lookup/list when matched) are still suggested.
//!
//! The synth pass this inverts is
//! `crates/lazuli_analyzer/src/conventions/mod.rs` — it derives the 5 names
//! from `pascal_to_snake(resource.name)` as
//! `create_<r>` / `update_<r>` / `delete_<r>` (commands) and
//! `lookup_<r>` / `list_<r>s` (queries). This rule mirrors that exact
//! spelling so the inverse can never disagree with the forward pass.
//!
//! Like the rest of the `vocab::*` family, dispatch into
//! `DoctorPackage::diagnostics()` is a separate cell (the
//! `lazuli_doctor_run` bridge); this module owns the rule logic + canonical
//! message, exercised by the inline `#[cfg(test)] mod tests`.

use std::path::{Path, PathBuf};

use lazuli_ir::{ConventionRef, Feature, Resource};

// ── output ────────────────────────────────────────────────────────────────────

/// One `VOCAB-CRUD-SYNTH-AVAILABLE-001` finding: a resource that hand-rolls
/// (by name) the crud surface `conventions [crud]` would synthesize.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the resource was authored in.
    pub path: PathBuf,
    /// Resource name (PascalCase, e.g. `Customer`).
    pub resource: String,
    /// The synth member names the author hand-rolled, in canonical synth
    /// order, **after** the soft-delete carve-out (so `delete_<r>` is
    /// absent here when `delete_excluded` is true).
    pub matched: Vec<String>,
    /// True when a matched `delete_<r>` was carved out of `matched` because
    /// the resource is soft-delete / retention-bound.
    pub delete_excluded: bool,
    /// Snake form of the resource name (`customer`) — names `delete_<r>` in
    /// the carve-out sentence.
    pub resource_snake: String,
    /// Spec 0018 — true when the matched hand-rolled commands carry
    /// per-command specifics the BARE synth can't reproduce (a non-default
    /// `policy`, `emits`, or extra effect assignments beyond
    /// `<f> = input.<f>`). In that case the message names the `crud`
    /// overlay (policy / assign / emits / `input excludes`) as the shape to
    /// adopt — not just bare `conventions [crud]`. This is what makes the
    /// rule fire usefully on real production CRUD (Pauta `create_customer`
    /// et al.) rather than only on policy-trivial resources.
    pub overlay_needed: bool,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-CRUD-SYNTH-AVAILABLE-001";

    /// Render the advisory message (mirrors the `vocab/conventions.rs`
    /// phrasing style). Names the exact members that would be replaced and,
    /// when the carve-out fired, why `delete_<r>` stays explicit + the
    /// canonical `# doctor:allow` escape hatch.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::crud_synth_available::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("customer.lzi"),
    ///     resource: "Customer".into(),
    ///     matched: vec!["create_customer".into(), "update_customer".into()],
    ///     delete_excluded: false,
    ///     resource_snake: "customer".into(),
    ///     overlay_needed: true,
    /// };
    /// assert!(f.message().contains("conventions [crud]"));
    /// ```
    pub fn message(&self) -> String {
        let count = self.matched.len();
        let plural = if count == 1 { "" } else { "s" };
        let mut msg = format!(
            "Resource `{}` hand-rolls {} command{} the `crud` convention would \
             synthesize ({}). Add `conventions [crud]` to the resource and delete them. \
             If the explicit handlers are intentional, add \
             `# doctor:allow VOCAB-CRUD-SYNTH-AVAILABLE-001 — reason \"...\"`.",
            self.resource,
            count,
            plural,
            self.matched.join(", "),
        );
        if self.overlay_needed {
            msg.push_str(
                " These commands carry per-command policy / emits / default-literal \
                 assignments, so pair `conventions [crud]` with a `crud` overlay \
                 (`crud` block: `create`/`update`/`delete` sub-blocks carrying \
                 `policy` / `validate` / `input excludes` / `assign` / `emits`) — \
                 see spec 0018.",
            );
        }
        if self.delete_excluded {
            msg.push_str(&format!(
                " Keep `delete_{}` explicit — it is a soft-delete; the synthesized \
                 delete is hard (see spec 0015).",
                self.resource_snake
            ));
        }
        msg
    }
}

// ── synth-name set (the inverse key) ───────────────────────────────────────────

/// The canonical `crud` synth member names for a resource, in synth order.
/// Mirrors `crates/lazuli_analyzer/src/conventions/mod.rs` §5.1 exactly:
/// `create_<r>` / `update_<r>` / `delete_<r>` (commands) and
/// `lookup_<r>` / `list_<r>s` (queries).
struct SynthNames {
    create: String,
    update: String,
    delete: String,
    lookup: String,
    list: String,
}

impl SynthNames {
    fn for_resource(name: &str) -> Self {
        let r = pascal_to_snake(name);
        Self {
            create: format!("create_{r}"),
            update: format!("update_{r}"),
            delete: format!("delete_{r}"),
            lookup: format!("lookup_{r}"),
            list: format!("list_{r}s"),
        }
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run the rule over one feature, checking every resource that does NOT
/// already carry `ConventionRef::Crud`.
///
/// `path` is the source `.lzi` — used to anchor findings AND to honor the
/// `# doctor:allow` opt-out (the only I/O performed; mirrors
/// `vocab_tests_missing_001::check`).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::crud_synth_available::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a hand-rolled CRUD resource");
/// let _ = check(&feature, Path::new("customer_management.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    // Opt-out: a `# doctor:allow VOCAB-CRUD-SYNTH-AVAILABLE-001` anywhere in
    // the feature's `.lzi` silences the rule for that file. The finding is
    // anchored on the resource declaration; the allow_comment scan is
    // file-scoped, so the comment on the resource suppresses it.
    if crate::allow_comment::file_contains_doctor_allow(path, Finding::CODE) {
        return Vec::new();
    }

    let commands_by_name: std::collections::HashMap<&str, &lazuli_ir::Command> = feature
        .commands
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();
    let command_names: std::collections::HashSet<&str> = commands_by_name.keys().copied().collect();
    let query_names: std::collections::HashSet<&str> =
        feature.queries.iter().map(|q| q.name()).collect();

    let mut findings = Vec::new();
    for resource in &feature.resources {
        if let Some(finding) = check_resource(
            resource,
            &command_names,
            &commands_by_name,
            &query_names,
            path,
        ) {
            findings.push(finding);
        }
    }
    findings
}

/// Inverse-synth detection for a single resource.
fn check_resource(
    resource: &Resource,
    command_names: &std::collections::HashSet<&str>,
    commands_by_name: &std::collections::HashMap<&str, &lazuli_ir::Command>,
    query_names: &std::collections::HashSet<&str>,
    path: &Path,
) -> Option<Finding> {
    // Trigger #1 — already opted in: nothing to nudge.
    if resource.conventions.contains(&ConventionRef::Crud) {
        return None;
    }

    let names = SynthNames::for_resource(&resource.name);

    // Trigger #2 — the create + update core must be hand-rolled by name.
    let has_create = command_names.contains(names.create.as_str());
    let has_update = command_names.contains(names.update.as_str());
    if !(has_create && has_update) {
        return None;
    }

    // Soft-delete carve-out: a matched `delete_<r>` is dropped from the
    // suggestion when the resource is soft-delete / retention-bound, because
    // the synth delete is hard.
    let delete_matched = command_names.contains(names.delete.as_str());
    let soft = resource.soft_delete || resource.retention.is_some();
    let delete_excluded = delete_matched && soft;
    let suggest_delete = delete_matched && !soft;

    // Build the matched set in canonical synth order:
    // create, update, delete, lookup, list.
    let mut matched = vec![names.create.clone(), names.update.clone()];
    if suggest_delete {
        matched.push(names.delete.clone());
    }
    if query_names.contains(names.lookup.as_str()) {
        matched.push(names.lookup.clone());
    }
    if query_names.contains(names.list.as_str()) {
        matched.push(names.list.clone());
    }

    // Trigger #3 — the matched set after the carve-out is non-empty. The
    // create+update core guarantees this, but keep the guard explicit so the
    // contract reads off the code.
    if matched.is_empty() {
        return None;
    }

    // Spec 0018 — does any matched write command carry per-command
    // specifics the BARE synth can't reproduce (non-default policy, emits,
    // or extra effect assignments)? If so the migration target is
    // `conventions [crud]` + a `crud` overlay, and the message says so.
    let overlay_needed = [
        names.create.as_str(),
        names.update.as_str(),
        names.delete.as_str(),
    ]
    .iter()
    .filter_map(|n| commands_by_name.get(n))
    .any(|c| command_needs_overlay(c));

    Some(Finding {
        path: path.to_path_buf(),
        resource: resource.name.clone(),
        matched,
        delete_excluded,
        resource_snake: pascal_to_snake(&resource.name),
        overlay_needed,
    })
}

/// A hand-rolled write command carries overlay-only specifics when it has
/// a non-default `policy` (anything other than the synth's
/// `authenticated`), emits any event, or carries effect assignments that
/// are NOT the plain `<field> = input.<field>` the synth auto-generates
/// (default literals, field renames, `ctx.*`). Any of these means bare
/// `conventions [crud]` would silently change behavior — the author needs
/// the overlay to reproduce them.
fn command_needs_overlay(c: &lazuli_ir::Command) -> bool {
    use lazuli_ir::{CommandEffect, Expr, PolicyRef};
    let policy_overlay = match &c.policy {
        PolicyRef::Local(p) => p != "authenticated",
        PolicyRef::None => false,
        // Atom / structured policy is always a per-command override.
        _ => true,
    };
    if policy_overlay || !c.emits.is_empty() {
        return true;
    }
    let assignments = match &c.effect {
        CommandEffect::Creates(e) => &e.assignments,
        CommandEffect::Updates(e) => &e.assignments,
        _ => return false,
    };
    assignments.iter().any(|a| {
        // The auto-generated shape is `<field> = input.<field>`. Anything
        // else (literal, rename, ctx.*) is an overlay assign.
        match &a.value {
            Expr::Path(p) => {
                !(p.segments.len() == 2 && p.segments[0] == "input" && p.segments[1] == a.field)
            }
            _ => true,
        }
    })
}

// ── internals ─────────────────────────────────────────────────────────────────

/// Convert `PascalCase` to `snake_case`. Replicated **verbatim** from
/// `crates/lazuli_analyzer/src/helpers.rs::pascal_to_snake` (which is
/// `pub(crate)` to the analyzer and so unreachable from this crate —
/// `lazuli_doctor` depends on `lazuli_ir`/`lazuli_syntax`, not
/// `lazuli_analyzer`). Every sibling doctor rule that needs snake-casing
/// does the same. The `derives_canonical_synth_names` test pins the output
/// against the exact 5 synth names so the two cannot silently drift.
fn pascal_to_snake(s: &str) -> String {
    let mut out = String::new();
    let mut prev: Option<char> = None;
    let mut iter = s.chars().peekable();

    while let Some(ch) = iter.next() {
        if ch.is_ascii_uppercase() {
            let next_is_lower = iter
                .peek()
                .copied()
                .is_some_and(|next| next.is_ascii_lowercase());
            let prev_needs_sep = prev.is_some_and(|p| {
                p.is_ascii_lowercase()
                    || p.is_ascii_digit()
                    || (p.is_ascii_uppercase() && next_is_lower)
            });
            if !out.is_empty() && prev_needs_sep {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
        prev = Some(ch);
    }

    out
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use lazuli_ir::{
        Command, CommandEffect, CommandInput, CommandKind, Defaults, ListQuery, LookupQuery,
        Policies, PolicyRef, Query, RetentionAction, RetentionSpec,
    };

    fn mk_cmd(name: &str) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::None,
            policy: PolicyRef::None,
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
            owner_scope_sql: None,
            previous_names: vec![],
            span_ref: None,
            derived_from: None,
        }
    }

    /// Spec 0018 — a hand-rolled write command carrying per-command
    /// specifics the BARE synth can't reproduce: a `@policy.*` override,
    /// an `emits`, and an effect with a default-literal assignment. This
    /// is the shape the overlay exists to absorb.
    fn mk_overlayable_create(name: &str) -> Command {
        Command {
            policy: PolicyRef::Atom("policy.edit".to_owned()),
            emits: vec!["customer_created".to_owned()],
            effect: CommandEffect::Creates(lazuli_ir::CreateEffect {
                resource: lazuli_ir::QualifiedName {
                    feature: None,
                    name: "Customer".to_owned(),
                },
                from_input: true,
                assignments: vec![lazuli_ir::Assignment {
                    field: "situation".to_owned(),
                    value: lazuli_ir::Expr::Path(lazuli_ir::Path::from_segments([
                        "prospect".to_owned()
                    ])),
                }],
            }),
            kind: CommandKind::Create,
            ..mk_cmd(name)
        }
    }

    fn mk_lookup(name: &str) -> Query {
        Query::Lookup(LookupQuery {
            name: name.to_owned(),
            public_contract: None,
            params: vec![],
            keys: vec![],
            scope: vec![],
            scope_override: false,
            filters: vec![],
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
        })
    }

    fn mk_list(name: &str) -> Query {
        Query::List(ListQuery {
            name: name.to_owned(),
            public_contract: None,
            params: vec![],
            scope: vec![],
            scope_override: false,
            filters: vec![],
            order: vec![],
            paginate: None,
            modifier: None,
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
        })
    }

    fn mk_resource(name: &str) -> Resource {
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

    fn mk_feature(
        resources: Vec<Resource>,
        commands: Vec<Command>,
        queries: Vec<Query>,
    ) -> Feature {
        Feature {
            name: "customer_management".into(),
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
            commands,
            apis: vec![],
            records: vec![],
            queries,
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

    /// `Customer` resource hand-rolling the full crud surface by name, no
    /// `conventions [crud]`.
    fn full_handrolled() -> Feature {
        mk_feature(
            vec![mk_resource("Customer")],
            vec![
                mk_cmd("create_customer"),
                mk_cmd("update_customer"),
                mk_cmd("delete_customer"),
            ],
            vec![mk_lookup("lookup_customer"), mk_list("list_customers")],
        )
    }

    #[test]
    fn derives_canonical_synth_names() {
        // Pin the inverse name set against the forward synth's spelling so
        // the two can never drift (techspec "Contracts").
        let names = SynthNames::for_resource("Customer");
        assert_eq!(names.create, "create_customer");
        assert_eq!(names.update, "update_customer");
        assert_eq!(names.delete, "delete_customer");
        assert_eq!(names.lookup, "lookup_customer");
        assert_eq!(names.list, "list_customers");
        // Multi-word resource snake-cases identically to the synth helper.
        let m = SynthNames::for_resource("OrgInvitation");
        assert_eq!(m.create, "create_org_invitation");
        assert_eq!(m.list, "list_org_invitations");
    }

    #[test]
    fn flags_full_handrolled_crud() {
        let f = full_handrolled();
        let findings = check(&f, Path::new("customer_management.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "VOCAB-CRUD-SYNTH-AVAILABLE-001");
        assert_eq!(findings[0].resource, "Customer");
        // matched = all 5 synth members, delete kept (hard delete).
        assert_eq!(
            findings[0].matched,
            vec![
                "create_customer",
                "update_customer",
                "delete_customer",
                "lookup_customer",
                "list_customers",
            ]
        );
        assert!(!findings[0].delete_excluded);
        let msg = findings[0].message();
        for name in [
            "create_customer",
            "update_customer",
            "delete_customer",
            "lookup_customer",
            "list_customers",
            "conventions [crud]",
        ] {
            assert!(msg.contains(name), "message missing `{name}`: {msg}");
        }
    }

    #[test]
    fn silent_when_opted_in() {
        let mut f = full_handrolled();
        f.resources[0].conventions.push(ConventionRef::Crud);
        assert!(check(&f, Path::new("customer_management.lzi")).is_empty());
    }

    #[test]
    fn excludes_soft_delete_from_suggestion() {
        // `soft_delete` resource: delete carved out of `matched`,
        // `delete_excluded == true`, message keeps delete explicit.
        let mut f = full_handrolled();
        f.resources[0].soft_delete = true;
        let findings = check(&f, Path::new("customer_management.lzi"));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].delete_excluded);
        assert!(!findings[0].matched.contains(&"delete_customer".to_string()));
        assert!(findings[0].matched.contains(&"create_customer".to_string()));
        assert!(findings[0].matched.contains(&"update_customer".to_string()));
        let msg = findings[0].message();
        assert!(msg.contains("Keep `delete_customer` explicit"), "{msg}");

        // The carve-out also fires for a `retention` posture (no soft_delete).
        let mut g = full_handrolled();
        g.resources[0].retention = Some(RetentionSpec {
            duration: "90d".to_owned(),
            action: RetentionAction::Delete,
        });
        let gf = check(&g, Path::new("customer_management.lzi"));
        assert_eq!(gf.len(), 1);
        assert!(gf[0].delete_excluded);
        assert!(!gf[0].matched.contains(&"delete_customer".to_string()));
    }

    #[test]
    fn requires_create_and_update_core() {
        // Only lookup + list hand-rolled — no create/update core ⇒ no finding.
        let only_reads = mk_feature(
            vec![mk_resource("Customer")],
            vec![],
            vec![mk_lookup("lookup_customer"), mk_list("list_customers")],
        );
        assert!(check(&only_reads, Path::new("customer_management.lzi")).is_empty());

        // create alone (no update) ⇒ still no finding.
        let create_only = mk_feature(
            vec![mk_resource("Customer")],
            vec![mk_cmd("create_customer")],
            vec![],
        );
        assert!(check(&create_only, Path::new("customer_management.lzi")).is_empty());
    }

    #[test]
    fn partial_handroll_create_update_only_scopes_message() {
        // create + update only ⇒ finding scoped to exactly those two.
        let f = mk_feature(
            vec![mk_resource("Customer")],
            vec![mk_cmd("create_customer"), mk_cmd("update_customer")],
            vec![],
        );
        let findings = check(&f, Path::new("customer_management.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].matched,
            vec!["create_customer", "update_customer"]
        );
        let msg = findings[0].message();
        assert!(msg.contains("2 commands"));
        assert!(!msg.contains("delete_customer"));
        assert!(!msg.contains("lookup_customer"));
        assert!(!msg.contains("list_customers"));
    }

    #[test]
    fn respects_doctor_allow() {
        use std::io::Write;
        // The allow_comment path reads the file on disk, so write a temp
        // `.lzi` carrying the opt-out comment (uses `std::env::temp_dir`,
        // mirroring the in-tree opt-out tests; no `tempfile` dev-dep needed
        // for the lib-test target).
        let dir = std::env::temp_dir().join("lazuli_crud_synth_available_optout");
        std::fs::create_dir_all(&dir).unwrap();
        let lzi = dir.join("customer_management.lzi");
        let mut fh = std::fs::File::create(&lzi).unwrap();
        writeln!(
            fh,
            "# doctor:allow VOCAB-CRUD-SYNTH-AVAILABLE-001 — reason \"explicit handlers\""
        )
        .unwrap();
        writeln!(fh, "feature customer_management").unwrap();
        let f = full_handrolled();
        assert!(check(&f, &lzi).is_empty());
    }

    #[test]
    fn severity_is_advisory_and_message_is_grammatical() {
        // The rule fires (advisory) and the count phrasing agrees in number.
        // The facet's `warning`/non-gating posture is pinned in
        // `lazuli_keywords` `P_CONVENTIONS`; this rule never emits an error.
        let f = full_handrolled();
        let findings = check(&f, Path::new("customer_management.lzi"));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message().contains("5 commands"));
    }

    #[test]
    fn crud_synth_available_fires_on_overlayable() {
        // Spec 0018 — a `Customer` resource hand-rolling create/update with
        // per-command policy + emits + default-literal assigns (the real
        // production-CRUD shape, e.g. Pauta `create_customer`). The rule
        // fires AND its message names the `crud` overlay as the migration
        // target — not just bare `conventions [crud]`.
        let f = mk_feature(
            vec![mk_resource("Customer")],
            vec![
                mk_overlayable_create("create_customer"),
                mk_overlayable_create("update_customer"),
            ],
            vec![],
        );
        let findings = check(&f, Path::new("customer_management.lzi"));
        assert_eq!(findings.len(), 1, "rule must fire on overlayable CRUD");
        assert!(findings[0].overlay_needed, "overlay_needed must be set");
        let msg = findings[0].message();
        assert!(msg.contains("conventions [crud]"), "{msg}");
        assert!(
            msg.contains("crud` overlay"),
            "message must name the overlay: {msg}"
        );
        assert!(
            msg.contains("spec 0018"),
            "message must cite spec 0018: {msg}"
        );
    }

    #[test]
    fn bare_handroll_does_not_recommend_overlay() {
        // A resource hand-rolling the trivial (policy-default, no-emits,
        // input-only-assign) CRUD shape gets the BARE `conventions [crud]`
        // nudge — `overlay_needed` stays false, no overlay sentence.
        let f = full_handrolled(); // mk_cmd: PolicyRef::None, no emits, no effect.
        let findings = check(&f, Path::new("customer_management.lzi"));
        assert_eq!(findings.len(), 1);
        assert!(!findings[0].overlay_needed);
        assert!(!findings[0].message().contains("crud` overlay"));
    }
}
