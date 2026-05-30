//! `report` declaration lowering.
//!
//! Lifts a `report <name>` AST onto `ir::Report`:
//!
//!   * source projection (`from query.<name>` or `from sql "..."`)
//!   * columns (with the column-source vocabulary)
//!   * visibility default (`signed`)
//!   * filename pattern parser — closed catalog of tokens
//!     (`{format}`, `{ctx.now:<strftime>}`, `{ctx.user.id}`,
//!     `{ctx.tenant.id}`)
//!
//! Per-format doctor diagnostics (`REPORT-FORMAT-UNKNOWN-001`,
//! `REPORT-FILENAME-TOKEN-UNKNOWN-001`) are emitted upstream from
//! the AST; this module only does the structural lift.

use crate::AnalyzeError;
use crate::command_decl::lower_typed_input_slots;
use crate::expr::{lower_policy_atom, lower_policy_expr, lower_qualified_name};
use crate::helpers::span_of;
use crate::resource::lower_rate_limit_spec;
use lazuli_ir as ir;
use lazuli_syntax as syntax;

// -----------------------------------------------------------------------------
// Report vocab — lower `report <name>` AST onto IR.
// -----------------------------------------------------------------------------

/// Lower a `ReportDecl` AST into `ir::Report`. Visibility defaults to
/// `signed` (per proposal §Slot inventory); formats outside the closed
/// `{csv, xlsx}` catalog drop silently — doctor reports
/// `REPORT-FORMAT-UNKNOWN-001` against the AST. Filename tokens are
/// parsed via the closed catalog (`{format}`, `{ctx.now:<strftime>}`,
/// `{ctx.user.id}`, `{ctx.tenant.id}`); unknown tokens land as
/// `FilenameToken::CtxNowStrftime("")` placeholders only if a parsing
/// helper rejects them — but we instead keep the literal verbatim and
/// surface unknown tokens via doctor.
pub(crate) fn lower_report_decl(
    _feature: &str,
    r: &syntax::ReportDecl,
) -> Result<ir::Report, AnalyzeError> {
    let source = lower_report_source(&r.source);

    let columns: Vec<ir::ReportColumn> = r
        .columns
        .iter()
        .map(|col| ir::ReportColumn {
            name: col.name.clone(),
            source: lower_report_column_source(&col.source),
            label: col.label.clone(),
            format: col.format.clone(),
            span_ref: Some(span_of(col.span)),
        })
        .collect();

    let formats: Vec<ir::ReportFormat> = r
        .formats
        .iter()
        .filter_map(|token| ir::ReportFormat::from_token(token.as_str()))
        .collect();

    let storage = r.storage.as_deref().map(lower_qualified_name);

    let visibility = match r.visibility.as_deref() {
        Some("public") => ir::FileVisibility::Public,
        Some("private") => ir::FileVisibility::Private,
        // Default per proposal §Slot inventory; doctor enforces signed
        // pairing with `signed_ttl`.
        _ => ir::FileVisibility::Signed,
    };

    let filename = r.filename.as_deref().map(lower_report_filename);

    let policy = r
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .unwrap_or(ir::PolicyRef::None);

    let audit = r.audit.as_ref().map(|a| ir::AuditSpec {
        subjects: a.subjects.clone(),
        // Proposal v0.2 forbids `emit_to` on reports; doctor surfaces
        // any author-supplied value. The lowering preserves what was
        // written so the doctor lint sees the offending edge.
        emit_to: a.emit_to.clone(),
        data_subject: a.data_subject.clone(),
        record_before: a.record_before,
        record_after: a.record_after,
        retain_for: a.retain_for.clone(),
        // GAP-AUDIT-01 — `materialize` is command-only; reports never
        // materialize an OperationLog row.
        materialize: None,
    });

    let policy_expr = r.policy_expr.as_ref().map(lower_policy_expr);
    // W5 GAP-REPORT-01 — lift the `input { … }` params through the
    // shared command-input slot lifter so report params receive the same
    // four constraint gates and the same downstream schema treatment.
    let input = lower_typed_input_slots(&r.input)?;
    Ok(ir::Report {
        name: r.name.clone(),
        input,
        source,
        columns,
        formats,
        storage,
        visibility,
        signed_ttl: r.signed_ttl.clone(),
        filename,
        policy,
        policy_expr,
        rate_limit: r.rate_limit.as_ref().map(lower_rate_limit_spec),
        audit,
        span_ref: Some(span_of(r.span)),
    })
}

pub(crate) fn lower_report_source(text: &str) -> ir::ReportSource {
    // Source forms:
    //   - `query.<name>`         (local short)
    //   - `<feature>.query.<name>` (cross-feature)
    //   - `<feature>.query.list.<name>` / `.lookup.<name>` / `.sql.<name>`
    //     (kind-qualified). The analyzer collapses the kind segment;
    //     doctor enforces the kind from the resolved target.
    let trimmed = text.trim();
    let parts: Vec<&str> = trimmed.split('.').collect();
    let qn = match parts.as_slice() {
        ["query", name] => ir::QualifiedName {
            feature: None,
            name: (*name).to_owned(),
        },
        [feature, "query", name] => ir::QualifiedName {
            feature: Some((*feature).to_owned()),
            name: (*name).to_owned(),
        },
        [feature, "query", _kind, name] => ir::QualifiedName {
            feature: Some((*feature).to_owned()),
            name: (*name).to_owned(),
        },
        _ => lower_qualified_name(trimmed),
    };
    ir::ReportSource::Query(qn)
}

pub(crate) fn lower_report_column_source(
    src: &syntax::ReportColumnSourceAst,
) -> ir::ReportColumnSource {
    match src {
        syntax::ReportColumnSourceAst::RowField(field) => {
            ir::ReportColumnSource::RowField(field.clone())
        }
        syntax::ReportColumnSourceAst::FnCall { name, args } => {
            ir::ReportColumnSource::Fn(ir::FnInvocation {
                name: name.clone(),
                args: args.clone(),
            })
        }
    }
}

/// Parse a filename template string into the closed `FilenameToken`
/// catalog. Unknown `{...}` tokens are silently dropped from the typed
/// token list; the literal is preserved so doctor's
/// `REPORT-FILENAME-TOKEN-UNKNOWN-001` rule can scan the literal and
/// report user-facing diagnostics.
pub(crate) fn lower_report_filename(literal: &str) -> ir::ReportFilenamePattern {
    let mut tokens = Vec::new();
    let bytes = literal.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{'
            && let Some(close) = literal[i + 1..].find('}')
        {
            let raw = &literal[i + 1..i + 1 + close];
            if let Some(token) = parse_filename_token(raw) {
                tokens.push(token);
            }
            i = i + 1 + close + 1;
            continue;
        }
        i += 1;
    }
    ir::ReportFilenamePattern {
        literal: literal.to_owned(),
        tokens,
    }
}

fn parse_filename_token(raw: &str) -> Option<ir::FilenameToken> {
    match raw {
        "format" => Some(ir::FilenameToken::Format),
        "ctx.user.id" => Some(ir::FilenameToken::CtxUserId),
        "ctx.tenant.id" => Some(ir::FilenameToken::CtxTenantId),
        _ => {
            if let Some(strftime) = raw.strip_prefix("ctx.now:") {
                return Some(ir::FilenameToken::CtxNowStrftime(strftime.to_owned()));
            }
            None
        }
    }
}
