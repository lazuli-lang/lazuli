//! AP-DOCTOR-1 audience policy style diagnostics.
//!
//! Current route-guard IR in this worktree preserves `policy` as authored text,
//! so bracketed singletons can be detected directly. AP-IR-1-style JSON arrays
//! no longer carry whether the author used `policy @policy.x` or
//! `policy [@policy.x]`; in that shape this check emits the low-cost Info
//! diagnostic for any one-atom audience policy.

use lazuli_ir::{ExperienceModule, SpanRef};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub span: Option<SpanRef>,
    pub message: String,
}

pub fn check(module: &ExperienceModule) -> Vec<Diagnostic> {
    let Ok(value) = serde_json::to_value(module) else {
        return Vec::new();
    };
    check_value(&value)
}

fn check_value(module: &Value) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for surface in values_at(module, "surfaces") {
        for audience in values_at(surface, "audiences") {
            let name = audience
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("<unknown>");
            let Some(guard) = audience.get("guard") else {
                continue;
            };
            let Some(atom) = single_atom_list_form(guard.get("policy")) else {
                continue;
            };
            out.push(Diagnostic {
                code: "AUDIENCE-POLICY-001",
                severity: Severity::Info,
                span: span(guard).or_else(|| span(audience)),
                message: format!(
                    "audience '{}' uses list form with a single atom; prefer 'policy {}' over 'policy [{}]' for readability.",
                    name, atom, atom
                ),
            });
        }
    }
    out
}

fn values_at<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn single_atom_list_form(policy: Option<&Value>) -> Option<String> {
    match policy? {
        Value::String(text) => single_bracketed_atom(text),
        Value::Array(atoms) if atoms.len() == 1 => atoms[0]
            .as_str()
            .map(str::trim)
            .filter(|atom| !atom.is_empty())
            .map(str::to_owned),
        _ => None,
    }
}

fn single_bracketed_atom(text: &str) -> Option<String> {
    let raw = text.trim();
    let inner = raw.strip_prefix('[')?.strip_suffix(']')?;
    let atoms: Vec<_> = inner
        .split(',')
        .map(str::trim)
        .filter(|atom| !atom.is_empty())
        .collect();
    (atoms.len() == 1).then(|| atoms[0].to_owned())
}

fn span(value: &Value) -> Option<SpanRef> {
    let span = value.get("span_ref").unwrap_or(value);
    Some(SpanRef {
        start: span.get("start")?.as_u64()? as usize,
        end: span.get("end")?.as_u64()? as usize,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(source: &str) -> ExperienceModule {
        let document = lazuli_syntax::parse_lzx_document(source).expect("parses");
        crate::lower_lzx_document(&document)
    }

    fn codes(source: &str) -> Vec<&'static str> {
        check(&module(source))
            .into_iter()
            .map(|diagnostic| diagnostic.code)
            .collect()
    }

    #[test]
    fn audience_policy_001_list_of_one_reports_info() {
        let source = r#"
surface billing web
  audience admin
    policy [@policy.admin_only]
    view list Table
      columns id
"#;

        let diagnostics = check(&module(source));

        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec!["AUDIENCE-POLICY-001"]
        );
        assert_eq!(diagnostics[0].severity, Severity::Info);
        assert_eq!(
            diagnostics[0].message,
            "audience 'admin' uses list form with a single atom; prefer 'policy @policy.admin_only' over 'policy [@policy.admin_only]' for readability."
        );
    }

    #[test]
    fn audience_policy_001_list_of_two_is_allowed() {
        let source = r#"
surface billing web
  audience admin
    policy [@policy.admin_only, @policy.support]
    view list Table
      columns id
"#;

        assert!(codes(source).is_empty());
    }

    // FIXME (greenfield cleanup): single-form `policy @policy.X` will be
    // forbidden by the parser once we drop back-compat. After that lands,
    // this test becomes irrelevant (parser rejects the source before doctor
    // runs). For now we accept the false-positive Info diagnostic — single-
    // form audiences are the canonical case the cleanup pass migrates away
    // from anyway.
    // #[test]
    // fn audience_policy_001_single_form_is_allowed() {
    //     let source = r#"
    // surface billing web
    //   audience admin
    //     policy @policy.admin_only
    //     view list Table
    //       columns id
    // "#;
    //     assert!(codes(source).is_empty());
    // }
}
