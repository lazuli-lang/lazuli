//! Query declaration lowering — the read-path slot of a feature.
//!
//! ## Role in the pipeline
//!
//! Lifts `syntax::QueryDecl` (three variants: `List`, `Lookup`,
//! `Sql`) onto the equivalent `ir::Query` shapes. A query is the
//! canonical typed read against a resource:
//!
//! * `query.list` — paginated multi-row read with filters, order,
//!   modifier, cache, policy.
//! * `query.lookup` — single-row by key, with optional filters,
//!   policy, and (after the convention synth pass) owner-scope SQL.
//! * `query.sql` / `query.view` — escape hatch: a verbatim SQL file
//!   reference plus typed params and a typed `returns` row shape.
//!
//! Inline `filters` lines parse out of free-form text per
//! `WAR-VOCAB-QUERY-ENUM-01`: closed comparison operators, then RHS
//! literal / dotted path / bare enum-variant inference. The bare
//! single-segment identifier case lifts to `Expr::Enum` so codegen
//! can serialize it as a TEXT bind parameter matching an enum column.
//!
//! Cache resolution (CL.C.3) lives here too: an inline `cache` body
//! wins; otherwise a `cache <profile_name>` reference is resolved
//! against the feature's `caches`, producing a `QueryCache` with the
//! profile body copied in plus a `profile_ref` breadcrumb. Unknown
//! profiles still emit a stub so doctor can fire
//! `cache-profile-unknown` without crashing codegen.
//!
//! ## Cross-references
//!
//! * Input: `lazuli_syntax::ast::QueryDecl`, `CacheProfileDecl`,
//!   `CommandInputSlot`.
//! * Output: `lazuli_ir::Query` (List|Lookup|Sql), `QueryCache`,
//!   `CacheProfile`, `Filter`, `TypedSlot`.
//! * Sibling: `command.rs` (effects), `resource.rs` (constraints
//!   threaded through query input slots).
//!
//! ## ABI guarantee
//!
//! Every fn here is `pub(crate)` — internal to the analyzer.
//! `strip_validate_skip` is also called from `lower_feature_skeleton`
//! for typed command input slots; it stays here because the cleanest
//! caller (the typed-slot helper) lives here.

use crate::expr::{lower_policy_atom, lower_policy_expr};
use crate::helpers::{find_top_level_operator, span_of};
use crate::resource::lift_field_constraints;
use crate::{AnalyzeError, lower_public_contract, type_ref_from_text};
use lazuli_ir as ir;
use lazuli_syntax as syntax;

/// Phase L Tier 4d — lower a canonical-indent `query` block into
/// `ir::Query`. The three shapes (`query.list` / `query.lookup` /
/// `query.sql` / `query.view`) project onto the existing IR variants.
///
/// Cache (CL.C.3): if the query authors `cache <profile_name>`, the
/// inline `cache` field is populated by resolving the profile against
/// `caches`. When the profile is unknown, lowering preserves the
/// reference (so doctor can fire `cache-profile-unknown`) without
/// inventing a body.
pub(crate) fn lower_query_decl(
    feature_name: &str,
    q: &syntax::QueryDecl,
    caches: &[syntax::CacheProfileDecl],
) -> Result<ir::Query, AnalyzeError> {
    match q {
        syntax::QueryDecl::List(list) => Ok(ir::Query::List(ir::ListQuery {
            name: list.name.clone(),
            public_contract: lower_public_contract(&list.public_contract),
            params: list
                .params
                .iter()
                .map(lower_command_input_to_typed)
                .collect::<Result<Vec<_>, _>>()?,
            scope: Vec::new(),
            scope_override: list.scope_override,
            filters: lower_query_filter_lines(&list.filters),
            order: Vec::new(),
            paginate: list.paginate,
            modifier: list.modifier.clone(),
            cache: lower_query_cache_with_profile(
                &list.cache,
                list.cache_profile_ref.as_deref(),
                caches,
            ),
            // QUERY-POLICY-001 — route the parsed `policy @policy.<X>`
            // atom into IR so codegen can emit a non-empty
            // `lazuli.Policy{...}` literal. Mirrors `lower_command_decl`.
            policy: list
                .policy
                .as_deref()
                .map(lower_policy_atom)
                .unwrap_or(ir::PolicyRef::None),
            policy_expr: list.policy_expr.as_ref().map(lower_policy_expr),
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: Some(span_of(list.span)),
            // owner-scope §7.3 — author-written queries default to
            // tenant-only. Synth pass mutates the slot for
            // convention-emitted queries.
            owner_scope_sql: None,
        })),
        syntax::QueryDecl::Lookup(lookup) => Ok(ir::Query::Lookup(ir::LookupQuery {
            name: lookup.name.clone(),
            public_contract: lower_public_contract(&lookup.public_contract),
            params: Vec::new(),
            keys: lookup
                .keys
                .iter()
                .map(|k| ir::KeyClause {
                    path: ir::Path::from_segments([k.name.clone()]),
                    equals: ir::Expr::Path(ir::Path::from_segments([k.name.clone()])),
                })
                .collect(),
            scope: Vec::new(),
            scope_override: false,
            filters: lower_query_filter_lines(&lookup.filters),
            // QUERY-POLICY-001 — same lowering as `query.list`.
            policy: lookup
                .policy
                .as_deref()
                .map(lower_policy_atom)
                .unwrap_or(ir::PolicyRef::None),
            policy_expr: lookup.policy_expr.as_ref().map(lower_policy_expr),
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: Some(span_of(lookup.span)),
            // owner-scope §7.3 — author-written `query.lookup` defaults
            // to tenant-only. Synth pass mutates for `lookup_<r>` /
            // `lookup_my_<r>` when the resource carries `@owner_axis`.
            owner_scope_sql: None,
        })),
        syntax::QueryDecl::Sql(sql) => Ok(ir::Query::Sql(ir::SqlQuery {
            name: sql.name.clone(),
            sql_kind: match sql.kind {
                syntax::SqlQueryKind::Sql => ir::SqlQueryKind::Sql,
                syntax::SqlQueryKind::View => ir::SqlQueryKind::View,
            },
            public_contract: lower_public_contract(&sql.public_contract),
            params: sql
                .params
                .iter()
                .map(lower_command_input_to_typed)
                .collect::<Result<Vec<_>, _>>()?,
            scope: Vec::new(),
            scope_override: false,
            returns: type_ref_from_text(&sql.returns),
            sql_path: lower_sql_file_ref(feature_name, &sql.sql_path),
            cache: None,
            // QUERY-POLICY-001 — same lowering as `query.list`.
            policy: sql
                .policy
                .as_deref()
                .map(lower_policy_atom)
                .unwrap_or(ir::PolicyRef::None),
            policy_expr: sql.policy_expr.as_ref().map(lower_policy_expr),
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: Some(span_of(sql.span)),
        })),
    }
}

pub(crate) fn lower_sql_file_ref(feature_name: &str, source: &str) -> String {
    let trimmed = source.trim();
    let Some(rest) = trimmed.strip_prefix("@file.") else {
        return trimmed.to_owned();
    };
    if rest.ends_with(".sql")
        && !rest.contains('/')
        && !rest.contains('\\')
        && rest
            .trim_end_matches(".sql")
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return format!("app/features/{feature_name}/queries/{rest}");
    }
    trimmed.to_owned()
}

/// WAR-VOCAB-QUERY-ENUM-01 closure — lower the verbatim
/// `query.list filters` lines (parsed by the syntax as `Vec<String>`)
/// into typed `ir::Filter` predicates. Each line is shaped
/// `<field> <op> <expr>` where `<op>` is the closed comparison set
/// (`=`, `!=`, `<`, `<=`, `>`, `>=`) and `<expr>` is one of:
///   - `<dotted.path>` (e.g. `ctx.actor.org_id`, `params.kind`) →
///     `Expr::Path`
///   - quoted string (`"foo"`) → `Expr::String`
///   - integer literal → `Expr::Integer`
///   - `true` / `false` → `Expr::Boolean`
///   - `nil` → `Expr::Nil`
///   - bare single-segment identifier (e.g. `approved`, `pending`) →
///     `Expr::Enum(EnumLiteral { type_name: None, variant })` — the
///     codegen serialises this as `lazuli.FromConst("<variant>")` so
///     the value lands as a Postgres TEXT bind parameter matching
///     the enum-typed column.
///
/// Lines that fail to parse are dropped silently; doctor's
/// vocab-filter lint catches malformed forms (TODO: add the lint).
pub(crate) fn lower_query_filter_lines(lines: &[String]) -> Vec<ir::Filter> {
    lines
        .iter()
        .filter_map(|line| parse_query_filter_line(line))
        .collect()
}

pub(crate) fn parse_query_filter_line(text: &str) -> Option<ir::Filter> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    for (token, op) in [
        ("<=", ir::CompareOp::Le),
        (">=", ir::CompareOp::Ge),
        ("!=", ir::CompareOp::Ne),
        ("<", ir::CompareOp::Lt),
        (">", ir::CompareOp::Gt),
        ("=", ir::CompareOp::Eq),
    ] {
        if let Some(idx) = find_top_level_operator(trimmed, token) {
            let (lhs_text, rhs_text) = trimmed.split_at(idx);
            let rhs_text = &rhs_text[token.len()..];
            let lhs = lhs_text.trim();
            let rhs = rhs_text.trim();
            if lhs.is_empty() || rhs.is_empty() {
                return None;
            }
            return Some(ir::Filter {
                predicate: ir::Predicate::Comparison {
                    left: filter_lhs_expr(lhs),
                    op,
                    right: filter_rhs_expr(rhs),
                },
                when: None,
            });
        }
    }
    None
}

/// LHS of a filter line is always a column reference. Single-segment
/// identifiers resolve to a column path; dotted forms (rare on LHS)
/// preserve their structure for the downstream codegen.
fn filter_lhs_expr(text: &str) -> ir::Expr {
    ir::Expr::Path(ir::Path::from_segments(text.split('.').map(str::to_owned)))
}

/// RHS may be a literal, a dotted runtime path, OR a bare enum variant.
/// The bare-identifier case is the WAR-VOCAB-QUERY-ENUM-01 closure —
/// `expr_from_text` treats bare identifiers as `Expr::Path`, which the
/// query codegen would render as `lazuli.FromInput(...)` (a runtime
/// input lookup); the correct semantic is "const string equal to the
/// enum variant name," so we lift to `Expr::Enum`.
fn filter_rhs_expr(text: &str) -> ir::Expr {
    let text = text.trim();
    if let Some(stripped) = text.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return ir::Expr::String(stripped.to_owned());
    }
    if let Ok(n) = text.parse::<i64>() {
        return ir::Expr::Integer(n);
    }
    match text {
        "true" => return ir::Expr::Boolean(true),
        "false" => return ir::Expr::Boolean(false),
        "nil" => return ir::Expr::Nil,
        _ => {}
    }
    if text.contains('.') {
        return ir::Expr::Path(ir::Path::from_segments(text.split('.').map(str::to_owned)));
    }
    // Bare single-segment identifier → enum variant. Type resolution
    // is deferred (no enum type carried in the syntax); codegen emits
    // a TEXT const via the unqualified `Expr::Enum` branch.
    ir::Expr::Enum(ir::EnumLiteral {
        type_name: None,
        variant: text.to_owned(),
    })
}

/// Cache bucket cycle — lift `cache` body lines (`key <expr>`, `ttl
/// <literal-or-prose>`, `tags <label>...`, `namespace <label>`) into
/// the typed `QueryCache` IR shape. Returns `None` when no `key` is
/// declared (defensive — doctor flags `cache without key/ttl`).
pub(crate) fn lower_query_cache(lines: &[String]) -> Option<ir::QueryCache> {
    if lines.is_empty() {
        return None;
    }
    let mut key: Option<String> = None;
    let mut ttl: Option<ir::CacheTtl> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut namespace: Option<String> = None;
    for raw in lines {
        let trimmed = raw.trim();
        if let Some(rest) = trimmed.strip_prefix("key ") {
            key = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("ttl ") {
            let val = rest.trim();
            ttl = Some(parse_cache_ttl(val));
        } else if let Some(rest) = trimmed.strip_prefix("tags ") {
            for part in rest.split(',') {
                let label = part.trim();
                if !label.is_empty() {
                    tags.push(label.to_owned());
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("namespace ") {
            namespace = Some(rest.trim().to_owned());
        }
    }
    let key = key?;
    let ttl = ttl?;
    Some(ir::QueryCache {
        key,
        ttl,
        tags,
        namespace,
        profile_ref: None,
    })
}

/// Cache bucket cycle (CL.C.3) — resolve a query's cache reference,
/// preferring the inline body when present, otherwise looking up the
/// `cache_profile_ref` against the feature's `caches`. Returns `None`
/// when no cache is authored at all.
///
/// When the profile reference is unknown (no matching feature-level
/// `cache <name>`), this returns a stub `QueryCache` carrying the
/// `profile_ref` and no body so doctor can fire
/// `cache-profile-unknown`. The defensive shape (`key`/`ttl` left
/// empty) is OK because doctor blocks before codegen consumes it.
pub(crate) fn lower_query_cache_with_profile(
    inline_lines: &[String],
    profile_ref: Option<&str>,
    caches: &[syntax::CacheProfileDecl],
) -> Option<ir::QueryCache> {
    // Inline form wins when both are present (parser already rejects
    // the combination; this is defensive).
    if let Some(inline) = lower_query_cache(inline_lines) {
        return Some(inline);
    }
    let name = profile_ref?;
    if let Some(profile) = caches.iter().find(|c| c.name == name) {
        // Resolve: copy body fields from the profile and record the
        // reference name so inspect/codegen can preserve author intent.
        return Some(ir::QueryCache {
            key: profile.key.clone(),
            ttl: parse_cache_ttl(&profile.ttl),
            tags: profile.tags.clone(),
            namespace: profile.namespace.clone(),
            profile_ref: Some(name.to_owned()),
        });
    }
    // Unknown profile — emit a stub so the IR records author intent
    // and doctor can flag the dangling reference. `key`/`ttl` are
    // intentionally empty placeholders.
    Some(ir::QueryCache {
        key: String::new(),
        ttl: ir::CacheTtl::Quoted(String::new()),
        tags: Vec::new(),
        namespace: None,
        profile_ref: Some(name.to_owned()),
    })
}

/// Cache bucket cycle (CL.C.3) — lower a feature-level
/// `cache <name>` profile AST into `ir::CacheProfile`. Mirrors
/// the inline-shape lowering for `key`/`ttl`/`tags`/`namespace` and
/// adds the four CL.C.3 decorators (`stale_while_revalidate`,
/// `coalesce`, `sliding`). Closed-catalog enforcement (units, boolean
/// shape, SWR <= TTL) lives in doctor.
pub(crate) fn lower_cache_profile_decl(decl: &syntax::CacheProfileDecl) -> ir::CacheProfile {
    ir::CacheProfile {
        name: decl.name.clone(),
        key: decl.key.clone(),
        ttl: parse_cache_ttl(&decl.ttl),
        namespace: decl.namespace.clone(),
        tags: decl.tags.clone(),
        stale_while_revalidate: decl.stale_while_revalidate.as_deref().map(parse_cache_ttl),
        coalesce: decl.coalesce,
        sliding: decl.sliding,
        span_ref: Some(span_of(decl.span)),
    }
}

fn parse_cache_ttl(value: &str) -> ir::CacheTtl {
    // Quoted prose: `ttl "5 minutes"`.
    if value.starts_with('"') {
        let body = value.trim_matches('"').to_owned();
        return ir::CacheTtl::Quoted(body);
    }
    // Typed literal: `ttl 5m` (digits + s|m|h|d).
    let bytes = value.as_bytes();
    if let Some(idx) = bytes.iter().rposition(|c| c.is_ascii_alphabetic()) {
        // Find last alphabetic char; everything before is the digit body.
        let (num_part, unit_part) = value.split_at(idx);
        let unit = unit_part.trim();
        if let Ok(n) = num_part.trim().parse::<u32>() {
            return match unit {
                "s" => ir::CacheTtl::Literal(ir::CacheTtlLiteral::Seconds(n)),
                "m" => ir::CacheTtl::Literal(ir::CacheTtlLiteral::Minutes(n)),
                "h" => ir::CacheTtl::Literal(ir::CacheTtlLiteral::Hours(n)),
                "d" => ir::CacheTtl::Literal(ir::CacheTtlLiteral::Days(n)),
                _ => ir::CacheTtl::Quoted(value.to_owned()),
            };
        }
    }
    ir::CacheTtl::Quoted(value.to_owned())
}

pub(crate) fn lower_command_input_to_typed(
    slot: &syntax::CommandInputSlot,
) -> Result<ir::TypedSlot, AnalyzeError> {
    // LAZ-SEMANTIC-AUTO-VALIDATE W2 — `@validate.skip` is an authoring
    // annotation that opts the field out of semantic-scalar
    // auto-validation. Detected anywhere in the slot's type text and
    // stripped before type resolution.
    let (cleaned_type_text, validate_skip) = strip_validate_skip(&slot.type_text);
    Ok(ir::TypedSlot {
        name: slot.name.clone(),
        type_ref: type_ref_from_text(&cleaned_type_text),
        required: slot.required,
        constraints: lift_field_constraints(&slot.name, &slot.constraints)?,
        validate_skip,
    })
}

/// Strip `@validate.skip` (anywhere in the text) and return the
/// trimmed remainder + whether the marker was present.
pub(crate) fn strip_validate_skip(text: &str) -> (String, bool) {
    if !text.contains("@validate.skip") {
        return (text.to_owned(), false);
    }
    let cleaned = text
        .replace("@validate.skip", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (cleaned, true)
}
