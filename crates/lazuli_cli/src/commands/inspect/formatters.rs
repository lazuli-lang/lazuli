//! IR-to-text formatters shared across the inspect projectors.
//!
//! Each function in this module turns a typed `lazuli_ir::*` shape
//! into the canonical string the inspect projection emits inside
//! `Inspect<X>` carriers (predicate text on aggregates, target
//! expressions on commands, capability descriptors on resource
//! fields, etc.). The formatters are pure — they take a borrowed IR
//! reference and return a `String` or `&'static str` — so they're
//! safe to call from any projector.
//!
//! The formatters split into three rough families:
//!
//! 1. **Predicate + comparison ops** (`predicate_to_string`,
//!    `compare_op_to_string`) — render aggregate invariant
//!    expressions back into the surface syntax authors typed.
//! 2. **Tool / qname / path / policy refs** (`tool_ref_to_string`,
//!    `tool_kind_segment`, `format_qname`, `path_to_string`,
//!    `policy_ref_to_string`, `type_ref_to_string`, `op_as_str`) —
//!    render the qualified identifier shapes that show up across
//!    every projection (resource fields, agent tools, commands).
//! 3. **Expression + target + let-binding + command-effect +
//!    assignment renderers** (`inspect_expr_to_string`,
//!    `inspect_target_expr_to_string`,
//!    `inspect_let_binding_to_string`,
//!    `inspect_command_effect_to_string`,
//!    `inspect_assignments_to_string`) — render the typed
//!    command/query spine into the inspect-side text projection.
//!
//! Every entry is `pub(super)` — only the parent `inspect/` module
//! (and its sibling projectors) call into here. Promoting any of
//! them to `pub(crate)` would mean a non-inspect consumer reaches
//! into IR-text rendering, which is an antipattern; that consumer
//! should call into IR directly or use the inspect JSON instead.

pub(super) fn predicate_to_string(pred: &lazuli_ir::Predicate) -> String {
    match pred {
        lazuli_ir::Predicate::Comparison { left, op, right } => format!(
            "{} {} {}",
            inspect_expr_to_string(left),
            compare_op_to_string(*op),
            inspect_expr_to_string(right),
        ),
        lazuli_ir::Predicate::Has {
            collection,
            element,
        } => format!(
            "{} has {}",
            inspect_expr_to_string(collection),
            inspect_expr_to_string(element),
        ),
        lazuli_ir::Predicate::And(parts) => parts
            .iter()
            .map(predicate_to_string)
            .collect::<Vec<_>>()
            .join(" and "),
        lazuli_ir::Predicate::Or(parts) => parts
            .iter()
            .map(predicate_to_string)
            .collect::<Vec<_>>()
            .join(" or "),
    }
}

pub(super) fn compare_op_to_string(op: lazuli_ir::CompareOp) -> &'static str {
    match op {
        lazuli_ir::CompareOp::Eq => "=",
        lazuli_ir::CompareOp::Ne => "!=",
        lazuli_ir::CompareOp::Lt => "<",
        lazuli_ir::CompareOp::Le => "<=",
        lazuli_ir::CompareOp::Gt => ">",
        lazuli_ir::CompareOp::Ge => ">=",
    }
}

pub(super) fn op_as_str(op: &lazuli_ir::ToolsCallsOp) -> &'static str {
    match op {
        lazuli_ir::ToolsCallsOp::Includes => "includes",
        lazuli_ir::ToolsCallsOp::Excludes => "excludes",
    }
}

pub(super) fn tool_ref_to_string(t: &lazuli_ir::QualifiedToolRef) -> String {
    match t {
        lazuli_ir::QualifiedToolRef::Local { kind, name } => {
            format!("{}.{}", tool_kind_segment(*kind), name)
        }
        lazuli_ir::QualifiedToolRef::CrossFeature {
            feature,
            kind,
            name,
        } => format!("{feature}.{}.{name}", tool_kind_segment(*kind)),
        lazuli_ir::QualifiedToolRef::Adapter { dotted } => {
            format!("@tool.{}", dotted.join("."))
        }
    }
}

pub(super) fn tool_kind_segment(kind: lazuli_ir::ToolKind) -> &'static str {
    match kind {
        lazuli_ir::ToolKind::QueryList => "query.list",
        lazuli_ir::ToolKind::QueryLookup => "query.lookup",
        lazuli_ir::ToolKind::QuerySql => "query.sql",
        lazuli_ir::ToolKind::QueryView => "query.view",
        lazuli_ir::ToolKind::Command => "command",
        lazuli_ir::ToolKind::Api => "api",
        lazuli_ir::ToolKind::QueryUnspecified => "query",
    }
}

pub(super) fn format_qname(q: &lazuli_ir::QualifiedName) -> String {
    match q.feature.as_deref() {
        Some(f) => format!("{f}.{}", q.name),
        None => q.name.clone(),
    }
}

pub(super) fn path_to_string(p: &lazuli_ir::Path) -> String {
    p.segments.join(".")
}

pub(super) fn policy_ref_to_string(p: &lazuli_ir::PolicyRef) -> String {
    match p {
        lazuli_ir::PolicyRef::Local(name) => format!("@policy.{name}"),
        lazuli_ir::PolicyRef::Atom(atom) => atom.clone(),
        lazuli_ir::PolicyRef::External { feature, name } => format!("{feature}.{name}"),
        lazuli_ir::PolicyRef::Unresolved(text) => text.clone(),
        lazuli_ir::PolicyRef::None => String::new(),
    }
}

pub(super) fn type_ref_to_string(t: &lazuli_ir::TypeRef) -> String {
    match t {
        lazuli_ir::TypeRef::Builtin(b) => format!("{b:?}"),
        lazuli_ir::TypeRef::UserDefined(q) => format_qname(q),
        lazuli_ir::TypeRef::EnumRef(q) => format_qname(q),
        lazuli_ir::TypeRef::Many(inner) => format!("Many({})", type_ref_to_string(inner)),
        lazuli_ir::TypeRef::Unresolved(s) => s.clone(),
        lazuli_ir::TypeRef::Capability(_) => "@cap.File(...)".to_owned(),
    }
}

/// Phase L Tier 4b — pretty-print a typed `Expr` back into source-like
/// text for inspect projections. Used by both job declarative bodies
/// and command projections so the inspect output is stable across
/// Tier 3 and Tier 4 lifts.
pub(super) fn inspect_expr_to_string(e: &lazuli_ir::Expr) -> String {
    match e {
        lazuli_ir::Expr::Path(p) => p.segments.join("."),
        lazuli_ir::Expr::String(s) => format!("\"{s}\""),
        lazuli_ir::Expr::Integer(n) => n.to_string(),
        lazuli_ir::Expr::Boolean(b) => b.to_string(),
        lazuli_ir::Expr::Enum(l) => match &l.type_name {
            Some(q) => format!("{}.{}", format_qname(q), l.variant),
            None => l.variant.clone(),
        },
        lazuli_ir::Expr::Nil => "nil".to_owned(),
        lazuli_ir::Expr::FnCall(call) => {
            let args: Vec<String> = call.args.iter().map(inspect_expr_to_string).collect();
            format!("@fn.{}({})", call.name.name, args.join(", "))
        }
    }
}

pub(super) fn inspect_target_expr_to_string(t: &lazuli_ir::TargetExpr) -> String {
    let args: Vec<String> = t
        .args
        .iter()
        .map(|a| format!("{}: {}", a.name, inspect_expr_to_string(&a.value)))
        .collect();
    format!("{}({})", format_qname(&t.query), args.join(", "))
}

pub(super) fn inspect_let_binding_to_string(l: &lazuli_ir::LetBinding) -> String {
    format!("{} = {}", l.name, inspect_expr_to_string(&l.value))
}

pub(super) fn inspect_command_effect_to_string(e: &lazuli_ir::CommandEffect) -> String {
    match e {
        lazuli_ir::CommandEffect::Creates(c) => {
            let head = if c.from_input {
                format!("creates {} from input", format_qname(&c.resource))
            } else {
                format!("creates {}", format_qname(&c.resource))
            };
            inspect_assignments_to_string(&head, &c.assignments)
        }
        lazuli_ir::CommandEffect::Updates(u) => inspect_assignments_to_string(
            &format!("updates {}", format_qname(&u.resource)),
            &u.assignments,
        ),
        lazuli_ir::CommandEffect::Deletes(d) => format!("deletes {}", format_qname(&d.resource)),
        lazuli_ir::CommandEffect::Returns(r) => {
            format!("returns {}", type_ref_to_string(&r.return_type))
        }
        lazuli_ir::CommandEffect::None => String::new(),
    }
}

pub(super) fn inspect_assignments_to_string(
    head: &str,
    assignments: &[lazuli_ir::Assignment],
) -> String {
    if assignments.is_empty() {
        head.to_owned()
    } else {
        let mut out = head.to_owned();
        for a in assignments {
            out.push_str("\n  ");
            out.push_str(&a.field);
            out.push_str(" = ");
            out.push_str(&inspect_expr_to_string(&a.value));
        }
        out
    }
}

// === Capability formatters ===
//
// Render every `lazuli_ir::*Capability` variant into the surface
// syntax authors typed: `@cap.Pii(...)`, `@cap.E2ee(...)`,
// `@cap.Hashed(...)`, `@cap.Encrypted(...)`, `@cap.Token(...)`,
// `@cap.File(...)`. The file-cap helper also reaches for the
// scalar size literal + visibility renderers below it. Used by the
// IR-side projectors when they need to mirror the lifted typed
// shape back into the inspect string surface (resource fields,
// api outputs, etc.).
pub(super) fn format_type_ref(t: &lazuli_ir::TypeRef) -> String {
    use lazuli_ir::{BuiltinType, CapabilityRef, TypeRef};
    match t {
        TypeRef::Builtin(BuiltinType::SemanticMoney { currency }) => {
            format!("@semantic.Money(currency:{})", currency.as_iso())
        }
        // B3 — surface plugin-contributed `@semantic.<Name>` back as
        // the authored alias so inspect-text renderings stay stable.
        TypeRef::Builtin(BuiltinType::SemanticPluginType { name, .. }) => {
            format!("@semantic.{}", name)
        }
        TypeRef::Builtin(b) => match b {
            BuiltinType::Text => "Text",
            BuiltinType::Integer => "Integer",
            BuiltinType::Boolean => "Boolean",
            BuiltinType::Decimal => "Decimal",
            BuiltinType::Date => "Date",
            BuiltinType::DateTime => "DateTime",
            BuiltinType::Id => "ID",
            BuiltinType::Json => "Json",
            BuiltinType::SemanticEmail => "@semantic.Email",
            BuiltinType::SemanticPhone => "@semantic.Phone",
            BuiltinType::SemanticUrl => "@semantic.Url",
            BuiltinType::SemanticUuid => "@semantic.Uuid",
            // SemanticMoney + SemanticPluginType handled above.
            BuiltinType::SemanticMoney { .. } => unreachable!(),
            BuiltinType::SemanticPluginType { .. } => unreachable!(),
            BuiltinType::SemanticCurrency => "@semantic.Currency",
            BuiltinType::SemanticGeoPoint => "@semantic.GeoPoint",
            BuiltinType::CapSecret => "@cap.Secret",
            BuiltinType::CapFile => "@cap.File",
        }
        .to_owned(),
        TypeRef::UserDefined(qn) | TypeRef::EnumRef(qn) => qn.name.clone(),
        TypeRef::Many(inner) => format!("{}*", format_type_ref(inner)),
        TypeRef::Unresolved(text) => text.clone(),
        // Phase L Tier 2 — render the typed capability back into the
        // canonical source form so inspect summary lines stay readable.
        TypeRef::Capability(CapabilityRef::File(file)) => format_file_capability(file),
        TypeRef::Capability(CapabilityRef::Hashed(h)) => format_hashed_capability(h),
        TypeRef::Capability(CapabilityRef::Encrypted(e)) => format_encrypted_capability(e),
        TypeRef::Capability(CapabilityRef::E2ee(e)) => format_e2ee_capability(e),
        TypeRef::Capability(CapabilityRef::Token(t)) => format_token_capability(t),
        TypeRef::Capability(CapabilityRef::PII(pii)) => format_pii_capability(pii),
    }
}

pub(super) fn format_pii_capability(pii: &lazuli_ir::PiiCapability) -> String {
    let mut args = vec![format!("class:{}", pii.class)];
    if let Some(retention) = pii.retention.as_ref() {
        args.push(format!("retention:{}", retention));
    }
    if let Some(log_redact) = pii.log_redact {
        args.push(format!("log_redact:{}", log_redact));
    }
    format!("@cap.PII({})", args.join(","))
}

/// Encryption bucket cycle — render `E2eeCapability` back to source form.
pub(super) fn format_e2ee_capability(e: &lazuli_ir::E2eeCapability) -> String {
    format!("@cap.E2ee(key:{})", e.key)
}

/// Phase L Tier 4 follow-up — render `HashedCapability` back to source form.
pub(super) fn format_hashed_capability(h: &lazuli_ir::HashedCapability) -> String {
    let alg = match h.algorithm {
        lazuli_ir::HashAlgorithm::Argon2id => "argon2id",
        lazuli_ir::HashAlgorithm::Bcrypt => "bcrypt",
    };
    format!("@cap.Hashed(algorithm:{alg})")
}

pub(super) fn format_encrypted_capability(e: &lazuli_ir::EncryptedCapability) -> String {
    format!("@cap.Encrypted(key:{})", e.key)
}

pub(super) fn format_token_capability(t: &lazuli_ir::TokenCapability) -> String {
    let store = match t.store {
        lazuli_ir::TokenStore::Hashed => "hashed",
    };
    format!(
        "@cap.Token(ttl:{},single_use:{},store:{})",
        t.ttl, t.single_use, store
    )
}

/// Render a `FileCapability` back into the `@cap.File(...)` source form.
/// Used by both `format_type_ref` and the `--expand=storage` projection.
pub(super) fn format_file_capability(file: &lazuli_ir::FileCapability) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!(
        "max_size:{}",
        format_file_size_literal(file.max_size.literal)
    ));
    let accept = file
        .accept
        .iter()
        .map(|m| format!("{}/{}", m.family, m.subtype))
        .collect::<Vec<_>>()
        .join("|");
    parts.push(format!("accept:{accept}"));
    if let Some(v) = file.visibility {
        parts.push(format!("visibility:{}", format_file_visibility(v)));
    }
    if let Some(ttl) = file.signed_ttl.as_deref() {
        parts.push(format!("signed_ttl:{ttl}"));
    }
    format!("@cap.File({})", parts.join(","))
}

pub(super) fn format_file_size_literal(literal: lazuli_ir::FileSizeLiteral) -> String {
    use lazuli_ir::FileSizeLiteral::*;
    match literal {
        Kb(n) => format!("{n}kb"),
        Mb(n) => format!("{n}mb"),
        Gb(n) => format!("{n}gb"),
    }
}

pub(super) fn format_file_visibility(visibility: lazuli_ir::FileVisibility) -> &'static str {
    use lazuli_ir::FileVisibility::*;
    match visibility {
        Public => "public",
        Private => "private",
        Signed => "signed",
    }
}
