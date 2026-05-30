//! Walk a `Feature` (and every shape it transitively carries —
//! commands, queries, jobs, workflows, extensions, agents, etc.) and
//! emit every `@`-prefixed reference the Go codegen would need to
//! resolve.
//!
//! Output is a flat `Vec<RefUse>` keyed by feature + site so the
//! caller in `mod.rs` can dedupe diagnostics and surface a single
//! issue per (code, literal, feature, site) tuple.
//!
//! Refs are extracted by `extract_codegen_refs`: a scan over the
//! authored text that recognizes the closed set of namespace
//! prefixes (`@lazuli/plugin-`, `@runtime/`, `@adapter.`, `@fn.`,
//! `@semantic.`, `@cap.`) and consumes the longest token of
//! reference-legal chars.

use lazuli_ir::{
    BuiltinType, Command, CommandEffect, CommandInput, EvalContainsRhs, EvalPredicate, Expr,
    ExtensionContract, Feature, JobBody, PolicyRef, Predicate, Query, TestAssertion, TestBlock,
    TypeRef,
};

use super::{RefUse, UNRESOLVED_TYPE_PREFIX};

pub(super) fn collect_feature_refs(feature: &Feature, refs: &mut Vec<RefUse>) {
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
        // W4 GAP-REORDER-01 — reorder carries no expression operands.
        CommandEffect::Deletes(_) | CommandEffect::Reorders(_) | CommandEffect::None => {}
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
            let site = match query.sql_kind {
                lazuli_ir::SqlQueryKind::Sql => format!("query.sql {}", query.name),
                lazuli_ir::SqlQueryKind::View => format!("query.view {}", query.name),
            };
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
            // First, let the legacy text-extractor pick up any nested
            // `@lazuli/plugin-`, `@semantic.`, `@cap.` references inside the
            // raw string — that preserves existing diagnostics for
            // fixtures that author e.g. `TypeRef::Unresolved("@semantic.Currency")`.
            collect_text_refs(raw, feature, site, refs);
            // Then, if the raw string is NOT an @-prefixed reference
            // (i.e. it's a bare identifier like `"Customer"` that the
            // analyzer failed to resolve to a UserDefined qname), push
            // a synthetic literal so `run_checks` emits
            // `CODE_TYPE_UNRESOLVED` on it. Silent fallthrough at this
            // point produced non-compiling Go output without any
            // codegen-time warning (review bug #6, 2026-05-15).
            if !raw.trim_start().starts_with('@') {
                push_ref(
                    &format!("{}{}", UNRESOLVED_TYPE_PREFIX, raw),
                    feature,
                    site,
                    refs,
                );
            }
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
        Expr::FnCall(call) => {
            collect_text_refs(&call.name.name, feature, site, refs);
            for arg in &call.args {
                collect_expr_refs(arg, feature, site, refs);
            }
        }
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
            | TestAssertion::AllowsExtension { .. }
            | TestAssertion::DeniesExtension { .. } => {}
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
        "@lazuli/plugin-",
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
