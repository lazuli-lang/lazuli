//! `policies > fields <Resource>` sub-block parser — per-field `read:`
//! and `write:` clauses, plus the `access:` symmetric shorthand (0005).
//!
//! `access: P` desugars in-parser to `read: P` + `write: P`, so the emitted
//! [`FieldPolicyDecl`] is byte-identical to the explicit two-line form and all
//! downstream consumers (IR lowering, codegen, doctor) see ONE representation.
//! One form per field: `access:` and `read:`/`write:` on the same field is a
//! parse error (no merging, no algebra).
//!
//! Extracted from the original monolithic `policy.rs`.

use super::super::super::common::{SourceLine, is_trivia, line_error};
use super::super::super::error::ParseError;
use super::super::is_policy_identifier;
use crate::ast::{FieldPoliciesDecl, FieldPolicyDecl, Span};

pub(super) fn parse_field_policies_block(
    lines: &[SourceLine<'_>],
    start: usize,
    resource: String,
    field_indent: usize,
    clause_indent: usize,
) -> Result<(FieldPoliciesDecl, usize), ParseError> {
    let header = &lines[start];
    let header_indent = header.indent;
    let mut fields: Vec<FieldPolicyDecl> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != field_indent {
            return Err(line_error(
                line,
                "`fields` children use one indentation level deeper than the header",
            ));
        }

        // Bare field name at field_indent (`email`); read/write at
        // clause_indent below.
        let field_name = trimmed.to_owned();
        if field_name.is_empty() || !is_policy_identifier(&field_name) {
            return Err(line_error(
                line,
                "field policy entry must be a bare identifier",
            ));
        }
        let field_header_end = line.end;
        let mut read: Option<Vec<String>> = None;
        let mut write: Option<Vec<String>> = None;
        // 0005 — `access:` symmetric shorthand. Tracked separately from
        // `read`/`write` so the one-form-per-field rule can reject mixing
        // (`access:` desugars into BOTH `read` and `write`, so the explicit
        // form on the same field would silently double-set one axis).
        let mut saw_access = false;
        let mut saw_explicit = false;
        let mut last_field_end = field_header_end;
        let mut j = i + 1;
        while j < lines.len() {
            let inner = &lines[j];
            let inner_trim = inner.text.trim_start();
            if is_trivia(inner_trim) {
                j += 1;
                continue;
            }
            if inner.indent <= field_indent {
                break;
            }
            if inner.indent != clause_indent {
                return Err(line_error(
                    inner,
                    "field policy clauses use one indentation level deeper than the field name",
                ));
            }
            let parsed_atoms = |rest: &str| -> Vec<String> {
                rest.split(',')
                    .map(str::trim)
                    .filter(|atom| atom.starts_with('@'))
                    .map(str::to_owned)
                    .collect()
            };
            // 0005 — `access: P` symmetric shorthand: desugars to
            // `read: P` + `write: P` so the resulting `FieldPolicyDecl`
            // (and its IR) is byte-identical to the explicit two-line form.
            if let Some(rest) = inner_trim.strip_prefix("access:") {
                if saw_explicit {
                    return Err(line_error(
                        inner,
                        "field policy uses one form per field — `access:` (symmetric) \
                         OR `read:`/`write:` (asymmetric), never both",
                    ));
                }
                let atoms = parsed_atoms(rest);
                read = Some(atoms.clone());
                write = Some(atoms);
                saw_access = true;
                last_field_end = inner.end;
                j += 1;
                continue;
            }
            if let Some(rest) = inner_trim.strip_prefix("read:") {
                if saw_access {
                    return Err(line_error(
                        inner,
                        "field policy uses one form per field — `access:` (symmetric) \
                         OR `read:`/`write:` (asymmetric), never both",
                    ));
                }
                read = Some(parsed_atoms(rest));
                saw_explicit = true;
                last_field_end = inner.end;
                j += 1;
                continue;
            }
            if let Some(rest) = inner_trim.strip_prefix("write:") {
                if saw_access {
                    return Err(line_error(
                        inner,
                        "field policy uses one form per field — `access:` (symmetric) \
                         OR `read:`/`write:` (asymmetric), never both",
                    ));
                }
                write = Some(parsed_atoms(rest));
                saw_explicit = true;
                last_field_end = inner.end;
                j += 1;
                continue;
            }
            return Err(line_error(
                inner,
                "field policy clauses are `access:` (symmetric) or `read:`/`write:` \
                 followed by atoms",
            ));
        }
        fields.push(FieldPolicyDecl {
            field: field_name,
            read,
            write,
            span: Span::new(line.start, last_field_end),
        });
        last_end = last_field_end;
        i = j;
    }

    Ok((
        FieldPoliciesDecl {
            resource,
            fields,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
