//! `.lzi` resource cluster — every closed-grammar block authored under
//! a `resource <Name>` header lives in this sub-tree:
//!
//! - `field` — `<name>: <Type> [modifiers]` line + `@slug` / `@full_text`
//!   / `@owner_axis(...)` decorator peelers + nested `previously` lines.
//! - `index` — `index on`, `unique (...)`, `fts on (...)` shared
//!   identifier-list/method parsers (consumed by handlers below).
//! - `conventions` — `conventions [crud, me]` closed-catalog list with
//!   nearest-match suggestion for unknown identifiers.
//! - `aggregate_invariant` — `aggregate <Name>` block + the shared
//!   `invariant <name>` parser used by both aggregates and resources.
//! - `composite_key_lock` — `lock <strategy>` single-line decorator +
//!   `composite_key` block (fields + primary).
//! - `retention` — `retention <duration> then <action>` single line.
//! - `has_many` — `has_many <name>: <Type> [inverse <field>]` line.
//! - `lifecycle_routes` — router-w4 redirect table per lifecycle state.
//!
//! The entry point `parse_resource_decl` lives here in `mod.rs` and
//! dispatches body lines either inline (lifecycle / lifecycle_routes /
//! invariant / lock / composite_key / conventions / field) or through
//! the `resource_body_handlers()` prefix table (previously / tenancy /
//! retention / validates / has_many / index / unique / fts).

mod aggregate_invariant;
mod body_handlers;
mod composite_key_lock;
mod conventions;
mod field;
mod has_many;
mod index;
mod lifecycle_routes;
mod retention;

#[cfg(test)]
mod lifecycle_tests;

// `parse_aggregate_decl` + `parse_resource_field_decl` re-export to
// `lzi`'s namespace so the parent `lzi/mod.rs` (which calls
// `parse_aggregate_decl` from the feature-skeleton walker) and the
// sibling `lzi/record.rs` (which calls
// `super::parse_resource_field_decl`) reach them without diving into
// the resource sub-tree. `parse_invariant_decl` stays internal — it's
// only called from inside this sub-tree.
pub(super) use aggregate_invariant::parse_aggregate_decl;
pub(super) use field::parse_resource_field_decl;

use aggregate_invariant::parse_invariant_decl;
use body_handlers::{ResourceBodyState, resource_body_handlers};

use composite_key_lock::{parse_resource_composite_key, parse_resource_lock};
use conventions::parse_resource_conventions_list;
use lifecycle_routes::parse_resource_lifecycle_routes;

use super::super::common::{SourceLine, is_trivia, line_error};
use super::super::error::ParseError;
use super::lifecycle;

use crate::ast::{ManyThroughAst, ResourceDecl, Span};

pub(super) fn parse_resource_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(ResourceDecl, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("resource ")
        .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned())
        .ok_or_else(|| line_error(header, "resource header must be `resource <Name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "resource header requires a name"));
    }
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;

    let mut state = ResourceBodyState::default();
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
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "resource body children use one indentation level deeper than the `resource` header",
            ));
        }

        if trimmed == "soft_delete" {
            state.soft_delete = true;
            last_end = line.end;
            i += 1;
            continue;
        }
        if trimmed == "timestamps" {
            state.timestamps = true;
            last_end = line.end;
            i += 1;
            continue;
        }
        // GAP-AUDIT-02 — `append_only` resource modifier. Bare line like
        // `soft_delete` / `timestamps`. Makes the resource insert-only;
        // doctor `RESOURCE-APPEND-ONLY-001` rejects update/delete commands.
        if trimmed == "append_only" {
            state.append_only = true;
            last_end = line.end;
            i += 1;
            continue;
        }
        if trimmed == "lifecycle" {
            return Err(line_error(
                line,
                "`lifecycle` requires a discriminator field name: `lifecycle <field>`",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("lifecycle ") {
            if state.lifecycle.is_some() {
                return Err(line_error(
                    line,
                    "a resource may declare at most one `lifecycle` block",
                ));
            }
            if rest.trim().is_empty() {
                return Err(line_error(
                    line,
                    "`lifecycle` requires a discriminator field name: `lifecycle <field>`",
                ));
            }
            let (block, next) = lifecycle::parse_lifecycle_block(lines, i)?;
            state.lifecycle = Some(block);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }
        // GAP-07 — `many_through <Junction> to <Partner>` block. Payload
        // fields live at grandchild indent and reuse the resource field
        // parser. The block desugars (in the analyzer) into a synthesized
        // junction resource carrying the two endpoint FKs + payload columns.
        if trimmed == "many_through" {
            return Err(line_error(
                line,
                "`many_through` requires `<JunctionName> to <PartnerResource>` (e.g. `many_through JobMember to User`)",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("many_through ") {
            let (mt, next) = parse_resource_many_through(lines, i, rest, grandchild_indent)?;
            state.many_through.push(mt);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        // router-w4 — `lifecycle_routes` block.
        if trimmed == "lifecycle_routes" {
            if state.lifecycle_routes.is_some() {
                return Err(line_error(
                    line,
                    "a resource may declare at most one `lifecycle_routes` block",
                ));
            }
            let (block, next) = parse_resource_lifecycle_routes(lines, i)?;
            state.lifecycle_routes = Some(block);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        // CL.C.4 — resource-scoped `invariant <name>` block. Shares
        // parser with the aggregate-scoped form; closed body is
        // `when <predicate>` plus optional `message "<text>"`.
        if let Some(rest) = trimmed.strip_prefix("invariant ") {
            let (inv, next) = parse_invariant_decl(lines, i, rest)?;
            state.invariants.push(inv);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        // Roadmap §1.5 (CL.C.2) — `lock optimistic version_field: <name>`,
        // `lock pessimistic`, `lock row_level`. Single-line decorator;
        // at most one per resource.
        if trimmed == "lock" {
            return Err(line_error(
                line,
                "`lock` requires a strategy: `lock optimistic version_field: <field>`, `lock pessimistic`, or `lock row_level`",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("lock ") {
            if state.lock.is_some() {
                return Err(line_error(
                    line,
                    "a resource may declare at most one `lock` decorator",
                ));
            }
            state.lock = Some(parse_resource_lock(line, rest)?);
            last_end = line.end;
            i += 1;
            continue;
        }

        // Roadmap §1.5 (CL.C.2) — `composite_key` block. Children at
        // grandchild indent: `fields <a>, <b>, ...` and `primary true|false`.
        if trimmed == "composite_key" {
            if state.composite_key.is_some() {
                return Err(line_error(
                    line,
                    "a resource may declare at most one `composite_key` block",
                ));
            }
            let (ck, next) = parse_resource_composite_key(lines, i, grandchild_indent)?;
            state.composite_key = Some(ck);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("composite_key ") {
            // Reject inline arguments — composite_key uses a block form
            // for child fields/primary lines.
            let _ = rest;
            return Err(line_error(
                line,
                "`composite_key` does not accept inline arguments — list fields under the block",
            ));
        }

        // `conventions [<name>, ...]` resource-level slot. Closed catalog
        // (today: `crud`). Empty list is a parse error — author writes no
        // slot at all rather than an empty one. See
        // `docs/proposals/ir-resource-conventions-crud.md` §4.1.
        if trimmed == "conventions" {
            return Err(line_error(
                line,
                "`conventions` requires a bracketed identifier list: `conventions [crud]`",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("conventions ") {
            if !state.conventions.is_empty() {
                return Err(line_error(
                    line,
                    "a resource may declare at most one `conventions` slot",
                ));
            }
            let entries = parse_resource_conventions_list(line, rest)?;
            state.conventions = entries;
            last_end = line.end;
            i += 1;
            continue;
        }

        if trimmed.contains(':')
            && !resource_body_handlers()
                .iter()
                .any(|(prefix, _)| trimmed.starts_with(prefix))
        {
            // `<name>: <Type> [modifiers...]` field declaration. Consume
            // optional `previously` grandchild block.
            let (field, next) = parse_resource_field_decl(lines, i, grandchild_indent)?;
            state.fields.push(field);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        let mut matched = false;
        for (prefix, handler) in resource_body_handlers() {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                handler(line, rest, &mut state)?;
                last_end = line.end;
                i += 1;
                matched = true;
                break;
            }
        }
        if !matched {
            return Err(line_error(
                line,
                "`resource` children are `previously`, `tenancy`, `soft_delete`, `timestamps`, `append_only`, `retention`, `validates`, `has_many`, `lifecycle`, `conventions`, `index on`, `unique (...)`, `fts on (...)`, or `<field>: <Type>`",
            ));
        }
    }

    Ok((
        ResourceDecl {
            name,
            public_contract: None,
            previously: state.previously,
            tenancy: state.tenancy,
            fields: state.fields,
            has_many: state.has_many,
            soft_delete: state.soft_delete,
            timestamps: state.timestamps,
            retention: state.retention,
            validates: state.validates,
            lifecycle: state.lifecycle,
            invariants: state.invariants,
            lock: state.lock,
            composite_key: state.composite_key,
            conventions: state.conventions,
            constraints: state.constraints,
            polymorphic_refs: state.polymorphic_refs,
            append_only: state.append_only,
            many_through: state.many_through,
            lifecycle_routes: state.lifecycle_routes,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// GAP-07 — parse a `many_through <Junction> to <Partner>` block. The
/// header line carries the junction name and the explicit `to <Partner>`
/// endpoint; payload fields live one indentation level deeper and reuse
/// `parse_resource_field_decl` (so they support the full field surface).
/// At least one payload field is required — a junction with no metadata is
/// a plain `has_many`/`has_many inverse` pair, not a `many_through`.
fn parse_resource_many_through(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
    grandchild_indent: usize,
) -> Result<(ManyThroughAst, usize), ParseError> {
    let header = &lines[start];
    let rest = rest.trim();
    let Some(to_idx) = rest.find(" to ") else {
        return Err(line_error(
            header,
            "`many_through` requires `<JunctionName> to <PartnerResource>` (e.g. `many_through JobMember to User`)",
        ));
    };
    let name = rest[..to_idx].trim();
    let partner = rest[to_idx + " to ".len()..].trim();
    if name.is_empty() || name.split_whitespace().count() != 1 {
        return Err(line_error(
            header,
            "`many_through` junction name must be a single identifier before `to` (e.g. `many_through JobMember to User`)",
        ));
    }
    if partner.is_empty() || partner.split_whitespace().count() != 1 {
        return Err(line_error(
            header,
            "`many_through ... to` requires exactly one partner resource name (e.g. `to User`)",
        ));
    }

    let mut payload: Vec<crate::ast::ResourceFieldDecl> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header.indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "`many_through` payload fields use one indentation level deeper than the header",
            ));
        }
        if !trimmed.contains(':') {
            return Err(line_error(
                line,
                "`many_through` children are payload field declarations `<name>: <Type> [modifiers]`",
            ));
        }
        // Payload field grandchildren may carry their own `previously`
        // great-grandchild lines; the field parser consumes them and
        // returns the next unconsumed line index.
        let (field, next) = parse_resource_field_decl(lines, i, grandchild_indent + 2)?;
        payload.push(field);
        last_end = lines[next.saturating_sub(1).max(i)].end;
        i = next;
    }

    if payload.is_empty() {
        return Err(line_error(
            header,
            "`many_through` requires at least one payload field — a junction with no metadata is a plain `has_many`",
        ));
    }

    Ok((
        ManyThroughAst {
            name: name.to_owned(),
            partner: partner.to_owned(),
            payload,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}


// =============================================================================
// Phase L Tier 4c — `resource` + lifecycle + aggregate + invariant + slug +
// owner_axis parser tests.
// =============================================================================
#[cfg(test)]
mod resource_block_parser_tests {
    include!("resource_tests.rs");
}
