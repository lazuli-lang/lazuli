//! `lazuli generate go --check` closed error catalog.
//!
//! The pass is intentionally codegen-local: it catches refs that the
//! Go emitter cannot resolve without reaching into doctor or the
//! filesystem. Discovery-backed checks (`@adapter.*` register sites
//! and `@fn.*` stubs) are recognized here but remain no-op stubs until
//! extension discovery lands.

use std::collections::BTreeSet;

use lazuli_ir::{
    AppIntegration, BuiltinType, Command, CommandEffect, CommandInput, EvalContainsRhs,
    EvalPredicate, Expr, ExtensionContract, Feature, JobBody, LegacyView, Module, PolicyRef,
    Predicate, Query, TestAssertion, TestBlock, TypeRef,
};

pub const CODE_PLUGIN: &str = "CODEGEN-GO-PLUGIN-001";
pub const CODE_UNRESOLVED: &str = "CODEGEN-GO-UNRESOLVED-002";
pub const CODE_ADAPTER: &str = "CODEGEN-GO-ADAPTER-003";
pub const CODE_SEMANTIC: &str = "CODEGEN-GO-SEMANTIC-004";
pub const CODE_CAP: &str = "CODEGEN-GO-CAP-005";
pub const CODE_FN: &str = "CODEGEN-GO-FN-006";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckIssue {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub feature: Option<String>,
    pub site: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RefUse {
    literal: String,
    feature: Option<String>,
    site: Option<String>,
}

pub fn run_checks(module: &Module) -> Vec<CheckIssue> {
    let declared_plugins = declared_plugin_names(module);
    let mut refs = Vec::new();

    for feature in &module.features {
        collect_feature_refs(feature, &mut refs);
    }

    let mut issues = Vec::new();
    let mut seen = BTreeSet::new();
    for reference in refs {
        if let Some(name) = reference.literal.strip_prefix("@plugin/") {
            if !plugin_declared(&declared_plugins, name) {
                push_issue(
                    &mut issues,
                    &mut seen,
                    CODE_PLUGIN,
                    Severity::Error,
                    format!(
                        "plugin reference {} not declared in app.lzi registry",
                        reference.literal
                    ),
                    &reference,
                );
            }
        } else if let Some(name) = reference.literal.strip_prefix("@runtime/") {
            if !known_runtime_ref(name) {
                push_issue(
                    &mut issues,
                    &mut seen,
                    CODE_UNRESOLVED,
                    Severity::Error,
                    format!(
                        "runtime reference {} is not in the closed Go runtime catalog",
                        reference.literal
                    ),
                    &reference,
                );
            }
        } else if let Some(name) = reference.literal.strip_prefix("@semantic.") {
            if !known_semantic_ref(name) {
                push_issue(
                    &mut issues,
                    &mut seen,
                    CODE_SEMANTIC,
                    Severity::Error,
                    format!(
                        "semantic reference {} is outside the closed Go semantic table",
                        reference.literal
                    ),
                    &reference,
                );
            }
        } else if let Some(name) = reference.literal.strip_prefix("@cap.") {
            if !known_cap_ref(name) {
                push_issue(
                    &mut issues,
                    &mut seen,
                    CODE_CAP,
                    Severity::Error,
                    format!(
                        "capability reference {} is outside Hashed/Encrypted/Token/File",
                        reference.literal
                    ),
                    &reference,
                );
            }
        } else if reference.literal.starts_with("@adapter.") {
            // Stub for CODEGEN-GO-ADAPTER-003. RegisterAdapter discovery
            // needs filesystem/runtime integration context that this pure
            // Module pass does not receive yet.
        } else if reference.literal.starts_with("@fn.") {
            // Stub for CODEGEN-GO-FN-006. Extension stub discovery lands
            // with the follow-up §10.5 resolver.
        }
    }

    issues
}

fn push_issue(
    issues: &mut Vec<CheckIssue>,
    seen: &mut BTreeSet<(&'static str, String, Option<String>, Option<String>)>,
    code: &'static str,
    severity: Severity,
    message: String,
    reference: &RefUse,
) {
    let key = (
        code,
        reference.literal.clone(),
        reference.feature.clone(),
        reference.site.clone(),
    );
    if seen.insert(key) {
        issues.push(CheckIssue {
            code,
            severity,
            message,
            feature: reference.feature.clone(),
            site: reference.site.clone(),
        });
    }
}

fn declared_plugin_names(module: &Module) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(app) = &module.app {
        collect_declared_plugins_from_integrations(&app.integrations, &mut names);
    }
    if let Some(registry) = &module.registry {
        collect_declared_plugins_from_integrations(&registry.integrations, &mut names);
    }
    names
}

fn collect_declared_plugins_from_integrations(
    integrations: &[AppIntegration],
    names: &mut BTreeSet<String>,
) {
    for integration in integrations {
        insert_plugin_name_variants(names, &integration.name);
        if let Some(tail) = integration
            .adapter
            .as_deref()
            .and_then(|adapter| adapter.strip_prefix("@plugin/"))
        {
            insert_plugin_name_variants(names, tail);
        }
    }
}

fn plugin_declared(declared_plugins: &BTreeSet<String>, tail: &str) -> bool {
    plugin_name_variants(tail)
        .into_iter()
        .any(|name| declared_plugins.contains(&name))
}

fn insert_plugin_name_variants(names: &mut BTreeSet<String>, name: &str) {
    for variant in plugin_name_variants(name) {
        names.insert(variant);
    }
}

fn plugin_name_variants(name: &str) -> Vec<String> {
    let trimmed = name.trim_matches('/');
    let last = trimmed.rsplit('/').next().unwrap_or(trimmed);
    [trimmed, last]
        .into_iter()
        .flat_map(|value| {
            [
                value.to_owned(),
                value.replace('-', "_"),
                value.to_ascii_lowercase(),
                value.replace('-', "_").to_ascii_lowercase(),
            ]
        })
        .collect()
}

fn known_runtime_ref(name: &str) -> bool {
    matches!(
        name,
        "postgres"
            | "s3"
            | "google_oauth"
            | "mercadopago"
            | "payments"
            | "customer-import"
            | "anthropic"
            | "serper"
            | "google_calendar"
    )
}

fn known_semantic_ref(name: &str) -> bool {
    matches!(
        name,
        "Email" | "Phone" | "Url" | "Uuid" | "Money" | "GeoPoint" | "Currency"
    )
}

fn known_cap_ref(name: &str) -> bool {
    matches!(name, "Hashed" | "Encrypted" | "Token" | "File")
}

fn collect_feature_refs(feature: &Feature, refs: &mut Vec<RefUse>) {
    let feature_name = feature.name.as_str();

    for value in &feature.uses {
        collect_text_refs(value, feature_name, "feature uses", refs);
    }

    collect_policy_ref(
        &feature.defaults.policy,
        feature_name,
        "defaults.policy",
        refs,
    );

    for resource in &feature.resources {
        for field in &resource.fields {
            let site = format!("resource {}.{}", resource.name, field.name);
            collect_type_ref(&field.type_ref, feature_name, &site, refs);
            collect_optional_text_ref(&field.derived_from, feature_name, &site, refs);
        }
    }

    for record in &feature.records {
        for field in &record.fields {
            let site = format!("record {}.{}", record.name, field.name);
            collect_type_ref(&field.type_ref, feature_name, &site, refs);
            collect_optional_text_ref(&field.derived_from, feature_name, &site, refs);
        }
    }

    for event in &feature.events {
        for field in &event.payload {
            let site = format!("event {}.{}", event.name, field.name);
            collect_type_ref(&field.type_ref, feature_name, &site, refs);
        }
    }

    for command in &feature.commands {
        collect_command_refs(command, feature_name, refs);
    }

    for api in &feature.apis {
        let site = format!("api {} output", api.name);
        collect_type_ref(&api.output, feature_name, &site, refs);
    }

    for query in &feature.queries {
        collect_query_refs(query, feature_name, refs);
    }

    for rule in &feature.rules {
        let site = format!("rule {}", rule.title);
        collect_predicate_refs(&rule.when, feature_name, &site, refs);
        collect_optional_text_ref(&rule.message_ref, feature_name, &site, refs);
        collect_test_block_refs(&rule.tests, feature_name, &site, refs);
    }

    for workflow in &feature.workflows {
        let site = format!("workflow {}", workflow.name);
        collect_policy_ref(&workflow.default_policy, feature_name, &site, refs);
        for transition in &workflow.transitions {
            let transition_site = format!("workflow {}.{}", workflow.name, transition.name);
            collect_optional_text_ref(&transition.requires, feature_name, &transition_site, refs);
            collect_test_block_refs(&transition.tests, feature_name, &transition_site, refs);
        }
    }

    for job in &feature.jobs {
        let site = format!("job {}", job.name);
        collect_policy_ref(&job.policy, feature_name, &site, refs);
        match &job.body {
            JobBody::Handler(handler) => {
                if let Some(returns) = &handler.returns {
                    collect_type_ref(returns, feature_name, &site, refs);
                }
            }
            JobBody::Declarative(body) => {
                if let Some(target) = &body.target {
                    for arg in &target.args {
                        collect_expr_refs(&arg.value, feature_name, &site, refs);
                    }
                }
                for binding in &body.lets {
                    collect_expr_refs(&binding.value, feature_name, &site, refs);
                }
                collect_command_effect_refs(&body.effect, feature_name, &site, refs);
            }
        }
    }

    for webhook in &feature.webhooks {
        let site = format!("webhook {}", webhook.name);
        collect_policy_ref(&webhook.policy, feature_name, &site, refs);
        if let Some(returns) = &webhook.returns {
            collect_type_ref(returns, feature_name, &site, refs);
        }
    }

    for notification in &feature.notifications {
        let site = format!("notification {}", notification.name);
        collect_policy_ref(&notification.policy, feature_name, &site, refs);
    }

    for event_group in &feature.event_groups {
        let site = format!("event_group {}", event_group.pattern);
        for payload in &event_group.raw_payload {
            collect_text_refs(payload, feature_name, &site, refs);
        }
        collect_optional_text_ref(&event_group.raw_audit, feature_name, &site, refs);
    }

    if let Some(auth) = &feature.auth {
        if let Some(password) = &auth.password {
            collect_text_refs(&password.hash, feature_name, "auth.password.hash", refs);
            collect_text_refs(&password.verify, feature_name, "auth.password.verify", refs);
        }
        if let Some(mfa) = &auth.mfa {
            collect_text_refs(&mfa.enroll, feature_name, "auth.mfa.enroll", refs);
            collect_text_refs(&mfa.verify, feature_name, "auth.mfa.verify", refs);
            collect_optional_text_ref(&mfa.adapter, feature_name, "auth.mfa.adapter", refs);
        }
        for oauth in &auth.oauth {
            let site = format!("auth.oauth.{}", oauth.provider);
            collect_text_refs(&oauth.adapter, feature_name, &site, refs);
        }
    }

    // Legacy `.lzi`-level surface views are no longer carried on
    // `Feature.surfaces`; the new lzx ViewModel pipeline (L0 #3) emits
    // typed view spec consts directly via the codegen_ts crate. This
    // helper still services the legacy fixture path via
    // `collect_view_refs`; live IR consumers reach views through the
    // lzx codegen.
    let _ = collect_view_refs;

    for extension in &feature.extensions {
        let site = format!("extension {}", extension.name);
        collect_extension_contract_refs(&extension.contract, feature_name, &site, refs);
    }

    for agent in &feature.agents {
        let site = format!("agent {}", agent.name);
        for slot in &agent.input {
            let slot_site = format!("agent {} input {}", agent.name, slot.name);
            collect_type_ref(&slot.type_ref, feature_name, &slot_site, refs);
        }
        if let Some(output_type) = &agent.output_type {
            collect_type_ref(output_type, feature_name, &site, refs);
        }
        for eval in &agent.evals {
            let eval_site = format!("agent {} eval {}", agent.name, eval.name);
            for assertion in &eval.assertions {
                match &assertion.predicate {
                    EvalPredicate::Closed(predicate) => {
                        collect_predicate_refs(predicate, feature_name, &eval_site, refs);
                    }
                    EvalPredicate::Contains { rhs, .. } => {
                        if let EvalContainsRhs::SemanticType(qname) = rhs {
                            collect_text_refs(&qname.name, feature_name, &eval_site, refs);
                        }
                    }
                    EvalPredicate::ToolsCalls { .. } => {}
                    EvalPredicate::Unparsed(text) => {
                        collect_text_refs(text, feature_name, &eval_site, refs);
                    }
                }
            }
        }
    }
}

fn collect_command_refs(command: &Command, feature: &str, refs: &mut Vec<RefUse>) {
    let site = format!("command {}", command.name);
    for slot in &command.route {
        let slot_site = format!("command {} route {}", command.name, slot.name);
        collect_type_ref(&slot.type_ref, feature, &slot_site, refs);
        collect_optional_text_ref(&slot.from, feature, &slot_site, refs);
    }
    if let CommandInput::Typed(slots) = &command.input {
        for slot in slots {
            let slot_site = format!("command {} input {}", command.name, slot.name);
            collect_type_ref(&slot.type_ref, feature, &slot_site, refs);
        }
    }
    if let Some(target) = &command.target {
        for arg in &target.args {
            collect_expr_refs(&arg.value, feature, &site, refs);
        }
    }
    for binding in &command.lets {
        collect_expr_refs(&binding.value, feature, &site, refs);
    }
    collect_command_effect_refs(&command.effect, feature, &site, refs);
    collect_policy_ref(&Some(command.policy.clone()), feature, &site, refs);
    if let Some(audit) = &command.audit {
        for subject in &audit.subjects {
            collect_text_refs(subject, feature, &site, refs);
        }
    }
    collect_test_block_refs(&command.tests, feature, &site, refs);
}

fn collect_command_effect_refs(
    effect: &CommandEffect,
    feature: &str,
    site: &str,
    refs: &mut Vec<RefUse>,
) {
    match effect {
        CommandEffect::Creates(effect) => {
            for assignment in &effect.assignments {
                collect_expr_refs(&assignment.value, feature, site, refs);
            }
        }
        CommandEffect::Updates(effect) => {
            for assignment in &effect.assignments {
                collect_expr_refs(&assignment.value, feature, site, refs);
            }
        }
        CommandEffect::Deletes(_) | CommandEffect::None => {}
        CommandEffect::Returns(effect) => {
            collect_type_ref(&effect.return_type, feature, site, refs);
        }
    }
}

fn collect_query_refs(query: &Query, feature: &str, refs: &mut Vec<RefUse>) {
    match query {
        Query::List(query) => {
            let site = format!("query.list {}", query.name);
            for param in &query.params {
                collect_type_ref(&param.type_ref, feature, &site, refs);
            }
            for predicate in &query.scope {
                collect_predicate_refs(predicate, feature, &site, refs);
            }
            for filter in &query.filters {
                collect_predicate_refs(&filter.predicate, feature, &site, refs);
            }
        }
        Query::Lookup(query) => {
            let site = format!("query.lookup {}", query.name);
            for param in &query.params {
                collect_type_ref(&param.type_ref, feature, &site, refs);
            }
            for predicate in &query.scope {
                collect_predicate_refs(predicate, feature, &site, refs);
            }
            for filter in &query.filters {
                collect_predicate_refs(&filter.predicate, feature, &site, refs);
            }
            for key in &query.keys {
                collect_expr_refs(&key.equals, feature, &site, refs);
            }
        }
        Query::Sql(query) => {
            let site = format!("query.sql {}", query.name);
            for param in &query.params {
                collect_type_ref(&param.type_ref, feature, &site, refs);
            }
            for predicate in &query.scope {
                collect_predicate_refs(predicate, feature, &site, refs);
            }
            collect_type_ref(&query.returns, feature, &site, refs);
        }
    }
}

fn collect_view_refs(view: &LegacyView, feature: &str, refs: &mut Vec<RefUse>) {
    match view {
        LegacyView::Table(view) => {
            for cell in &view.cells {
                let site = format!("view {}.{}", view.name, cell.field);
                collect_text_refs(&cell.renderer, feature, &site, refs);
            }
            collect_test_block_refs(&view.tests, feature, &format!("view {}", view.name), refs);
        }
        LegacyView::SidePanel(view) => {
            for block in &view.blocks {
                let site = format!("view {}", view.name);
                collect_text_refs(&block.renderer, feature, &site, refs);
            }
            collect_test_block_refs(&view.tests, feature, &format!("view {}", view.name), refs);
        }
        LegacyView::Form(view) => {
            collect_test_block_refs(&view.tests, feature, &format!("view {}", view.name), refs);
        }
        LegacyView::Custom(view) => {
            collect_test_block_refs(&view.tests, feature, &format!("view {}", view.name), refs);
        }
    }
}

fn collect_extension_contract_refs(
    contract: &ExtensionContract,
    feature: &str,
    site: &str,
    refs: &mut Vec<RefUse>,
) {
    match contract {
        ExtensionContract::CellRenderer { type_arg }
        | ExtensionContract::ViewBlock { type_arg }
        | ExtensionContract::FormField { type_arg }
        | ExtensionContract::Hook { type_arg }
        | ExtensionContract::Validator { type_arg }
        | ExtensionContract::QueryModifier { type_arg }
        | ExtensionContract::IntegrationAdapter { type_arg } => {
            collect_type_ref(type_arg, feature, site, refs);
        }
        ExtensionContract::Function { input, output } => {
            collect_type_ref(input, feature, site, refs);
            collect_type_ref(output, feature, site, refs);
        }
    }
}

fn collect_type_ref(type_ref: &TypeRef, feature: &str, site: &str, refs: &mut Vec<RefUse>) {
    match type_ref {
        TypeRef::Builtin(BuiltinType::CapSecret) => {
            push_ref("@cap.Secret", feature, site, refs);
        }
        TypeRef::Builtin(_) | TypeRef::Capability(_) => {}
        TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) => {
            collect_text_refs(&qname.name, feature, site, refs);
        }
        TypeRef::Many(inner) => {
            collect_type_ref(inner, feature, site, refs);
        }
        TypeRef::Unresolved(raw) => {
            collect_text_refs(raw, feature, site, refs);
        }
    }
}

fn collect_policy_ref(
    policy: &Option<PolicyRef>,
    feature: &str,
    site: &str,
    refs: &mut Vec<RefUse>,
) {
    if let Some(PolicyRef::Atom(value) | PolicyRef::Unresolved(value)) = policy {
        collect_text_refs(value, feature, site, refs);
    }
}

fn collect_predicate_refs(
    predicate: &Predicate,
    feature: &str,
    site: &str,
    refs: &mut Vec<RefUse>,
) {
    match predicate {
        Predicate::Comparison { left, right, .. } => {
            collect_expr_refs(left, feature, site, refs);
            collect_expr_refs(right, feature, site, refs);
        }
        Predicate::Has {
            collection,
            element,
        } => {
            collect_expr_refs(collection, feature, site, refs);
            collect_expr_refs(element, feature, site, refs);
        }
        Predicate::And(predicates) | Predicate::Or(predicates) => {
            for predicate in predicates {
                collect_predicate_refs(predicate, feature, site, refs);
            }
        }
    }
}

fn collect_expr_refs(expr: &Expr, feature: &str, site: &str, refs: &mut Vec<RefUse>) {
    match expr {
        Expr::Path(path) => {
            for segment in &path.segments {
                collect_text_refs(segment, feature, site, refs);
            }
        }
        Expr::String(value) => collect_text_refs(value, feature, site, refs),
        Expr::Enum(value) => {
            if let Some(qname) = &value.type_name {
                collect_text_refs(&qname.name, feature, site, refs);
            }
        }
        Expr::Integer(_) | Expr::Boolean(_) | Expr::Nil => {}
    }
}

fn collect_test_block_refs(
    tests: &Option<TestBlock>,
    feature: &str,
    site: &str,
    refs: &mut Vec<RefUse>,
) {
    let Some(tests) = tests else {
        return;
    };
    for assertion in &tests.assertions {
        match assertion {
            TestAssertion::PolicyAllow { actors } | TestAssertion::PolicyDeny { actors } => {
                for actor in actors {
                    collect_text_refs(actor, feature, site, refs);
                }
            }
            TestAssertion::AllowsWhen { predicate } | TestAssertion::DeniesWhen { predicate } => {
                collect_predicate_refs(predicate, feature, site, refs);
            }
            TestAssertion::AllowsAs { actor }
            | TestAssertion::DeniesAs { actor }
            | TestAssertion::AllowsFromAs { actor, .. }
            | TestAssertion::DeniesFromAs { actor, .. } => {
                collect_text_refs(actor, feature, site, refs);
            }
            TestAssertion::AllowsFrom { .. }
            | TestAssertion::DeniesFrom { .. }
            | TestAssertion::AcceptedBy { .. }
            | TestAssertion::RejectedBy { .. } => {}
        }
    }
}

fn collect_optional_text_ref(
    value: &Option<String>,
    feature: &str,
    site: &str,
    refs: &mut Vec<RefUse>,
) {
    if let Some(value) = value {
        collect_text_refs(value, feature, site, refs);
    }
}

fn collect_text_refs(text: &str, feature: &str, site: &str, refs: &mut Vec<RefUse>) {
    for literal in extract_codegen_refs(text) {
        push_ref(&literal, feature, site, refs);
    }
}

fn push_ref(literal: &str, feature: &str, site: &str, refs: &mut Vec<RefUse>) {
    refs.push(RefUse {
        literal: literal.to_owned(),
        feature: Some(feature.to_owned()),
        site: Some(site.to_owned()),
    });
}

fn extract_codegen_refs(text: &str) -> Vec<String> {
    let prefixes = [
        "@plugin/",
        "@runtime/",
        "@adapter.",
        "@fn.",
        "@semantic.",
        "@cap.",
    ];
    let mut refs = Vec::new();
    let mut offset = 0;
    while let Some(relative_at) = text[offset..].find('@') {
        let start = offset + relative_at;
        let rest = &text[start..];
        if prefixes.iter().any(|prefix| rest.starts_with(prefix)) {
            let end = rest
                .char_indices()
                .find_map(
                    |(index, ch)| {
                        if is_ref_char(ch) { None } else { Some(index) }
                    },
                )
                .unwrap_or(rest.len());
            refs.push(rest[..end].to_owned());
            offset = start + end;
        } else {
            offset = start + 1;
        }
    }
    refs
}

fn is_ref_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '@' | '_' | '-' | '/' | '.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{AppManifest, AppRegistry, Defaults, Field, Policies, QualifiedName, Resource};

    fn empty_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies {
                categories: Vec::new(),
                fields: Vec::new(),
                span_ref: None,
            },
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn empty_app() -> AppManifest {
        AppManifest {
            name: "test".to_owned(),
            title: None,
            version: None,
        lazuli_version: None,
            targets: Vec::new(),
            default_locale: None,
            default_timezone: None,
            auth_failed_redirect: None,
            not_found: None,
            uses: Vec::new(),
            packs: Vec::new(),
            bindings: Vec::new(),
            architecture: None,
            services: Vec::new(),
            communication: None,
            environments: Vec::new(),
            urls: Vec::new(),
            cors: None,
            env: Vec::new(),
            integrations: Vec::new(),
            capabilities: Vec::new(),
            runtime: Vec::new(),
            deploy: None,
            logging: None,
            tracing: None,
            observability: None,
            locale: None,
            encryption_bindings: Vec::new(),
            span_ref: None,
        }
    }

    fn empty_registry() -> AppRegistry {
        AppRegistry {
            env: Vec::new(),
            integrations: Vec::new(),
            capabilities: Vec::new(),
            packs: Vec::new(),
            tools: Vec::new(),
            webhook_events: Vec::new(),
        }
    }

    fn module_with_feature(feature: Feature) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(empty_app()),
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: vec![feature],
        }
    }

    fn field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn resource_with_field(type_ref: TypeRef) -> Resource {
        Resource {
            name: "Customer".to_owned(),
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![field("value", type_ref)],
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
        }
    }

    fn codes(issues: &[CheckIssue]) -> Vec<&'static str> {
        issues.iter().map(|issue| issue.code).collect()
    }

    #[test]
    fn missing_plugin_reference_reports_plugin_001() {
        let mut feature = empty_feature("billing");
        feature.uses.push("@plugin/mercadopago".to_owned());

        let issues = run_checks(&module_with_feature(feature));

        assert_eq!(codes(&issues), vec![CODE_PLUGIN]);
        assert_eq!(issues[0].severity, Severity::Error);
        assert_eq!(issues[0].feature.as_deref(), Some("billing"));
    }

    #[test]
    fn declared_plugin_registry_entry_suppresses_plugin_001() {
        let mut feature = empty_feature("billing");
        feature.uses.push("@plugin/mercadopago".to_owned());
        let mut registry = empty_registry();
        registry.integrations.push(AppIntegration {
            name: "mercadopago".to_owned(),
            kind: "PaymentGateway".to_owned(),
            adapter: Some("@plugin/mercadopago".to_owned()),
            adapter_provenance: Some("plugin".to_owned()),
            environments: Vec::new(),
            credentials: None,
            data_classification: None,
        });
        let mut module = module_with_feature(feature);
        module.registry = Some(registry);

        let issues = run_checks(&module);

        assert!(issues.is_empty());
    }

    #[test]
    fn unknown_semantic_reference_reports_semantic_004() {
        let mut feature = empty_feature("customer");
        feature
            .resources
            .push(resource_with_field(TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "@semantic.Locale".to_owned(),
            })));

        let issues = run_checks(&module_with_feature(feature));

        assert_eq!(codes(&issues), vec![CODE_SEMANTIC]);
        assert_eq!(issues[0].site.as_deref(), Some("resource Customer.value"));
    }

    #[test]
    fn unknown_capability_reference_reports_cap_005() {
        let mut feature = empty_feature("customer");
        feature
            .resources
            .push(resource_with_field(TypeRef::Unresolved(
                "@cap.E2ee".to_owned(),
            )));

        let issues = run_checks(&module_with_feature(feature));

        assert_eq!(codes(&issues), vec![CODE_CAP]);
        assert_eq!(issues[0].severity, Severity::Error);
    }

    #[test]
    fn legacy_cap_secret_reports_cap_005() {
        let mut feature = empty_feature("customer");
        feature.resources.push(resource_with_field(TypeRef::Builtin(
            BuiltinType::CapSecret,
        )));

        let issues = run_checks(&module_with_feature(feature));

        assert_eq!(codes(&issues), vec![CODE_CAP]);
    }

    #[test]
    fn valid_semantic_and_capability_catalog_entries_do_not_report() {
        let mut feature = empty_feature("customer");
        feature
            .resources
            .push(resource_with_field(TypeRef::Many(Box::new(
                TypeRef::Unresolved("@semantic.Currency".to_owned()),
            ))));
        feature
            .uses
            .push("@cap.File(max_size:25mb,accept:text/csv)".to_owned());

        let issues = run_checks(&module_with_feature(feature));

        assert!(issues.is_empty());
    }
}
