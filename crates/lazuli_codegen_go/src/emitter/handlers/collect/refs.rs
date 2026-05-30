//! Leaf collectors — walk every "low-level" IR shape and push handler
//! references onto the stub map.
//!
//! The dispatcher in `super::collect_feature_handler_refs` (and the
//! command/query walkers in `super::commands`) call into these to
//! cover the long tail of sites where `@fn.X` / `@hook.X` references
//! can hide: type-refs, policy atoms, job triggers, predicates,
//! expressions, paths, test-blocks, raw strings.
//!
//! `push_handler_ref` is the final write — it looks the name up in the
//! signature index, falls back to `(any, any)` when undeclared, and
//! inserts under a `(feature, path_name)` key so two references with
//! the same disk path don't double-stub.
//!
//! `extract_handler_refs` is the textual splitter — given a string,
//! pull out every `@fn.<name>` and `@hook.<name>` token. Used wherever
//! the IR holds raw user text (paths in `lets`, audit subjects, etc.)
//! and a structured walk isn't available.

use std::collections::BTreeMap;

use lazuli_ir::{Expr, Predicate, TestAssertion, TestBlock, TypeRef};

use super::super::paths::path_name_for;
use super::super::{HandlerNamespace, HandlerSignature, HandlerStub, SignatureMap, StubKey};

pub(super) fn collect_type_ref(
    type_ref: &TypeRef,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    match type_ref {
        TypeRef::Builtin(_) | TypeRef::Capability(_) => {}
        TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) => {
            collect_text_handler_refs(&qname.name, feature, site, signatures, stubs);
        }
        TypeRef::Many(inner) => {
            collect_type_ref(inner, feature, site, signatures, stubs);
        }
        TypeRef::Unresolved(raw) => {
            collect_text_handler_refs(raw, feature, site, signatures, stubs);
        }
    }
}

pub(super) fn collect_policy_ref(
    policy: &Option<lazuli_ir::PolicyRef>,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    if let Some(lazuli_ir::PolicyRef::Atom(value) | lazuli_ir::PolicyRef::Unresolved(value)) =
        policy
    {
        collect_text_handler_refs(value, feature, site, signatures, stubs);
    }
}

pub(super) fn collect_job_trigger_refs(
    trigger: &lazuli_ir::JobTrigger,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    match trigger {
        lazuli_ir::JobTrigger::Event { event } => {
            collect_text_handler_refs(&event.name, feature, site, signatures, stubs);
        }
        lazuli_ir::JobTrigger::Schedule { cron } => {
            collect_text_handler_refs(cron, feature, site, signatures, stubs);
        }
    }
}

pub(super) fn collect_predicate_refs(
    predicate: &Predicate,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    match predicate {
        Predicate::Comparison { left, right, .. } => {
            collect_expr_refs(left, feature, site, signatures, stubs);
            collect_expr_refs(right, feature, site, signatures, stubs);
        }
        Predicate::Has {
            collection,
            element,
        } => {
            collect_expr_refs(collection, feature, site, signatures, stubs);
            collect_expr_refs(element, feature, site, signatures, stubs);
        }
        Predicate::And(predicates) | Predicate::Or(predicates) => {
            for predicate in predicates {
                collect_predicate_refs(predicate, feature, site, signatures, stubs);
            }
        }
    }
}

pub(super) fn collect_expr_refs(
    expr: &Expr,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    match expr {
        Expr::Path(path) => {
            collect_path_refs(path, feature, site, signatures, stubs);
        }
        Expr::String(value) => collect_text_handler_refs(value, feature, site, signatures, stubs),
        Expr::Enum(value) => {
            if let Some(qname) = &value.type_name {
                collect_text_handler_refs(&qname.name, feature, site, signatures, stubs);
            }
            collect_text_handler_refs(&value.variant, feature, site, signatures, stubs);
        }
        Expr::Integer(_) | Expr::Boolean(_) | Expr::Nil => {}
        Expr::FnCall(call) => {
            collect_text_handler_refs(&call.name.name, feature, site, signatures, stubs);
            for arg in &call.args {
                collect_expr_refs(arg, feature, site, signatures, stubs);
            }
        }
    }
}

pub(super) fn collect_path_refs(
    path: &lazuli_ir::Path,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    if path.segments.is_empty() {
        return;
    }
    collect_text_handler_refs(&path.segments.join("."), feature, site, signatures, stubs);
}

pub(super) fn collect_test_block_refs(
    tests: &Option<TestBlock>,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    let Some(tests) = tests else {
        return;
    };
    for assertion in &tests.assertions {
        match assertion {
            TestAssertion::PolicyAllow { actors } | TestAssertion::PolicyDeny { actors } => {
                for actor in actors {
                    collect_text_handler_refs(actor, feature, site, signatures, stubs);
                }
            }
            TestAssertion::AllowsWhen { predicate } | TestAssertion::DeniesWhen { predicate } => {
                collect_predicate_refs(predicate, feature, site, signatures, stubs);
            }
            TestAssertion::AllowsAs { actor }
            | TestAssertion::DeniesAs { actor }
            | TestAssertion::AllowsFromAs { actor, .. }
            | TestAssertion::DeniesFromAs { actor, .. } => {
                collect_text_handler_refs(actor, feature, site, signatures, stubs);
            }
            TestAssertion::AllowsFrom { .. }
            | TestAssertion::DeniesFrom { .. }
            | TestAssertion::AllowsExtension { .. }
            | TestAssertion::DeniesExtension { .. } => {}
        }
    }
}

pub(super) fn collect_optional_text_handler_ref(
    value: &Option<String>,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    if let Some(value) = value {
        collect_text_handler_refs(value, feature, site, signatures, stubs);
    }
}

pub(super) fn collect_text_handler_refs(
    text: &str,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    for reference in extract_handler_refs(text) {
        push_handler_ref(
            reference.namespace,
            &reference.name,
            feature,
            site,
            signatures,
            stubs,
        );
    }
}

pub(super) fn push_handler_ref(
    namespace: HandlerNamespace,
    name: &str,
    feature: &str,
    site: &str,
    signatures: &SignatureMap,
    stubs: &mut BTreeMap<StubKey, HandlerStub>,
) {
    if name.is_empty() {
        return;
    }

    let path_name = path_name_for(name);
    if path_name.is_empty() {
        return;
    }

    let key = StubKey {
        feature: feature.to_owned(),
        path_name,
    };
    if stubs.contains_key(&key) {
        return;
    }

    let signature = signatures
        .get(&(feature.to_owned(), namespace, name.to_owned()))
        .cloned()
        .unwrap_or_else(HandlerSignature::any);

    stubs.insert(
        key,
        HandlerStub {
            feature: feature.to_owned(),
            namespace,
            name: name.to_owned(),
            site: format!("{feature}.{site}"),
            input_type: signature.input_type,
            output_type: signature.output_type,
        },
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::emitter::handlers) struct HandlerRef {
    pub(in crate::emitter::handlers) namespace: HandlerNamespace,
    pub(in crate::emitter::handlers) name: String,
}

pub(in crate::emitter::handlers) fn extract_handler_refs(text: &str) -> Vec<HandlerRef> {
    let mut refs = Vec::new();
    let mut offset = 0;

    while let Some(relative_at) = text[offset..].find('@') {
        let start = offset + relative_at;
        let rest = &text[start..];
        let namespace = if rest.starts_with(HandlerNamespace::Fn.prefix()) {
            Some(HandlerNamespace::Fn)
        } else if rest.starts_with(HandlerNamespace::Hook.prefix()) {
            Some(HandlerNamespace::Hook)
        } else {
            None
        };

        let Some(namespace) = namespace else {
            offset = start + 1;
            continue;
        };

        let prefix_len = namespace.prefix().len();
        let after_prefix = &rest[prefix_len..];
        let name_len = after_prefix
            .char_indices()
            .find_map(
                |(index, ch)| {
                    if is_ref_char(ch) { None } else { Some(index) }
                },
            )
            .unwrap_or(after_prefix.len());
        let name = after_prefix[..name_len].trim_matches('.').to_owned();
        if !name.is_empty() {
            refs.push(HandlerRef { namespace, name });
        }
        offset = start + prefix_len + name_len;
    }

    refs
}

fn is_ref_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/')
}
