//! B5 framework gap 2 — per-branch webhook dispatch.
//!
//! Renders the `EmitBindings` slice on a `webhooks.WebhookContract`. One
//! entry per `emits` line; entries without a `when` predicate are emitted
//! with `Kind: webhooks.EmitPredicateNone` so a single receiver walk-loop
//! handles both branches without special-casing the absent-predicate slot.

use lazuli_ir::{EmitPredicate, EmitPredicateKind};

use super::format::escape_string;

pub(super) fn format_emit_bindings(
    emits: &[String],
    predicates: &[Option<EmitPredicate>],
) -> String {
    let mut out = String::from("[]webhooks.EmitBinding{\n");
    for (idx, event) in emits.iter().enumerate() {
        let predicate = predicates.get(idx).and_then(|p| p.as_ref());
        let (kind_const, path, literal, literals_lit, raw) = match predicate {
            None => (
                "webhooks.EmitPredicateNone",
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ),
            Some(p) => match &p.kind {
                EmitPredicateKind::Equals { path, literal } => (
                    "webhooks.EmitPredicateEquals",
                    path.clone(),
                    literal.clone(),
                    String::new(),
                    p.raw.clone(),
                ),
                EmitPredicateKind::In { path, literals } => {
                    let entries: Vec<String> = literals
                        .iter()
                        .map(|v| format!("\"{}\"", escape_string(v)))
                        .collect();
                    (
                        "webhooks.EmitPredicateIn",
                        path.clone(),
                        String::new(),
                        format!("[]string{{{}}}", entries.join(", ")),
                        p.raw.clone(),
                    )
                }
                EmitPredicateKind::Other { raw } => (
                    "webhooks.EmitPredicateOther",
                    String::new(),
                    String::new(),
                    String::new(),
                    raw.clone(),
                ),
            },
        };
        let mut fields = vec![format!("Event: \"{}\"", escape_string(event))];
        fields.push(format!("Kind: {}", kind_const));
        if !path.is_empty() {
            fields.push(format!("Path: \"{}\"", escape_string(&path)));
        }
        if !literal.is_empty() {
            fields.push(format!("Literal: \"{}\"", escape_string(&literal)));
        }
        if !literals_lit.is_empty() {
            fields.push(format!("Literals: {}", literals_lit));
        }
        if !raw.is_empty() {
            fields.push(format!("Raw: \"{}\"", escape_string(&raw)));
        }
        out.push_str(&format!("\t\t{{{}}},\n", fields.join(", ")));
    }
    out.push_str("\t},");
    out
}
