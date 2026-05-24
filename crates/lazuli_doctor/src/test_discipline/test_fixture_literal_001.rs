//! TEST-FIXTURE-LITERAL-001 — predicate value is a fixture-shaped literal.
//!
//! Per `docs/proposals/test-completeness-lints.md` §TEST-FIXTURE-LITERAL-001.
//! Catches Jest-creep at the boundary: string / integer / decimal literals
//! that resemble real-world data (CPF, email, phone, UUID v4) inside a
//! `when <expr>` predicate. Boundaries should be `!= nil` / enum-eq / catalog
//! predicate, not a literal fixture.
//!
//! v0.1 shape detectors:
//!   * CPF: `^\d{11}$`
//!   * Email: `^[\w.+-]+@[\w-]+\.[\w.-]+$`
//!   * Phone: `^\+\d{10,15}$`
//!   * UUID v4: standard 8-4-4-4-12 hex with version-4 nibble
//!
//! Severity: `error` (strict + production). Hardens the closed predicate
//! language invariant on day one — every fixture literal that ships becomes
//! precedent.

use std::path::{Path, PathBuf};

use lazuli_ir::{Expr, Feature, Predicate, SpanRef, TestAssertion, TestBlock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub construct_kind: String,
    pub construct: String,
    pub literal: String,
    /// One of `cpf`, `email`, `phone`, `uuid`.
    pub shape: &'static str,
    pub span: Option<SpanRef>,
}

impl Finding {
    pub const CODE: &'static str = "TEST-FIXTURE-LITERAL-001";

    pub fn message(&self) -> String {
        format!(
            "{} `{}` predicate carries a fixture-shaped literal (`{}`, shape `{}`) — \
             the closed predicate language is for inference, not data fixtures. \
             Replace with `!= nil` / `= nil` / enum-eq / catalog predicate.",
            self.construct_kind, self.construct, self.literal, self.shape
        )
    }
}

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    for cmd in &feature.commands {
        if let Some(tests) = &cmd.tests {
            visit(
                tests,
                feature,
                path,
                "command",
                &cmd.name,
                &mut findings,
            );
        }
    }

    for rule in &feature.rules {
        if let Some(tests) = &rule.tests {
            visit(
                tests,
                feature,
                path,
                "rule",
                &rule.title,
                &mut findings,
            );
        }
    }

    for workflow in &feature.workflows {
        for transition in &workflow.transitions {
            if let Some(tests) = &transition.tests {
                visit(
                    tests,
                    feature,
                    path,
                    "workflow_transition",
                    &format!("{}.{}", workflow.name, transition.name),
                    &mut findings,
                );
            }
        }
    }

    for resource in &feature.resources {
        if let Some(lifecycle) = &resource.lifecycle {
            for transition in &lifecycle.transitions {
                if let Some(tests) = &transition.tests {
                    visit(
                        tests,
                        feature,
                        path,
                        "lifecycle_transition",
                        &format!("{}.{}", resource.name, transition.name),
                        &mut findings,
                    );
                }
            }
        }
    }

    findings
}

fn visit(
    tests: &TestBlock,
    feature: &Feature,
    path: &Path,
    construct_kind: &str,
    construct: &str,
    out: &mut Vec<Finding>,
) {
    for assertion in &tests.assertions {
        let predicate = match assertion {
            TestAssertion::AllowsWhen { predicate } | TestAssertion::DeniesWhen { predicate } => {
                predicate
            }
            _ => continue,
        };
        for (literal, shape) in collect_fixture_literals(predicate) {
            out.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                construct_kind: construct_kind.to_owned(),
                construct: construct.to_owned(),
                literal,
                shape,
                span: tests.span_ref.clone(),
            });
        }
    }
}

fn collect_fixture_literals(predicate: &Predicate) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    walk(predicate, &mut out);
    out
}

fn walk(predicate: &Predicate, out: &mut Vec<(String, &'static str)>) {
    match predicate {
        Predicate::Comparison { left, right, .. } => {
            check_expr(left, out);
            check_expr(right, out);
        }
        Predicate::And(parts) | Predicate::Or(parts) => {
            for p in parts {
                walk(p, out);
            }
        }
        Predicate::Has { collection, element } => {
            check_expr(collection, out);
            check_expr(element, out);
        }
    }
}

fn check_expr(expr: &Expr, out: &mut Vec<(String, &'static str)>) {
    if let Expr::String(s) = expr {
        if let Some(shape) = classify_string(s) {
            out.push((s.clone(), shape));
        }
    }
}

/// Returns the fixture shape name when `s` matches a known data shape, `None`
/// otherwise. The detectors are deliberately narrow (regex-free) to avoid
/// false-positives on prose strings.
fn classify_string(s: &str) -> Option<&'static str> {
    if is_cpf_shape(s) {
        return Some("cpf");
    }
    if is_uuid_v4(s) {
        return Some("uuid");
    }
    if is_phone_e164(s) {
        return Some("phone");
    }
    if is_email_shape(s) {
        return Some("email");
    }
    None
}

fn is_cpf_shape(s: &str) -> bool {
    s.len() == 11 && s.chars().all(|c| c.is_ascii_digit())
}

fn is_phone_e164(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 11 || bytes.len() > 16 {
        return false;
    }
    if bytes[0] != b'+' {
        return false;
    }
    bytes[1..].iter().all(|b| b.is_ascii_digit())
}

fn is_email_shape(s: &str) -> bool {
    // Conservative: exactly one `@`, dot in domain, no whitespace.
    if s.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    let mut parts = s.split('@');
    let local = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return false;
    }
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    domain.contains('.')
}

fn is_uuid_v4(s: &str) -> bool {
    // 8-4-4-4-12 hex, third group starts with `4`, fourth group starts with 8/9/a/b
    let bytes = s.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (i, b) in bytes.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if *b != b'-' {
                    return false;
                }
            }
            14 => {
                if *b != b'4' {
                    return false;
                }
            }
            19 => {
                if !matches!(*b, b'8' | b'9' | b'a' | b'b' | b'A' | b'B') {
                    return false;
                }
            }
            _ => {
                if !b.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        CompareOp, Defaults, OperationKind, OperationRef, Policies, QualifiedName, Rule,
    };

    fn mk_rule_with_literal(literal: &str) -> Rule {
        Rule {
            title: "r".to_owned(),
            denies: OperationRef {
                resource: QualifiedName {
                    feature: None,
                    name: "X".to_owned(),
                },
                op_name: "noop".to_owned(),
                kind: OperationKind::Unresolved,
            },
            when: Predicate::And(vec![]),
            message: String::new(),
            message_ref: None,
            tests: Some(TestBlock {
                assertions: vec![TestAssertion::AllowsWhen {
                    predicate: Predicate::Comparison {
                        left: Expr::Path(lazuli_ir::Path::from_segments(["self", "cpf"])),
                        op: CompareOp::Eq,
                        right: Expr::String(literal.to_owned()),
                    },
                }],
                span_ref: None,
            }),
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(rule: Rule) -> Feature {
        Feature {
            name: "test_feat".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![rule],
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
    fn cpf_shape_fires() {
        let feature = mk_feature(mk_rule_with_literal("12345678901"));
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].shape, "cpf");
    }

    #[test]
    fn email_shape_fires() {
        let feature = mk_feature(mk_rule_with_literal("ada@example.com"));
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].shape, "email");
    }

    #[test]
    fn phone_shape_fires() {
        let feature = mk_feature(mk_rule_with_literal("+5511987654321"));
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].shape, "phone");
    }

    #[test]
    fn uuid_shape_fires() {
        let feature = mk_feature(mk_rule_with_literal("a1b2c3d4-e5f6-4789-abcd-1234567890ab"));
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].shape, "uuid");
    }

    #[test]
    fn plain_short_string_does_not_fire() {
        let feature = mk_feature(mk_rule_with_literal("active"));
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn cpf_too_short_does_not_fire() {
        let feature = mk_feature(mk_rule_with_literal("123"));
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
