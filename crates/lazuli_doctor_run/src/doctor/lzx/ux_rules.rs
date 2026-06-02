//! Wave-W6 surface UX doctor rules (GAP-UX-01..04).
//!
//! Four sibling checks share this module because they walk the same
//! `Surface -> Audience -> View` tree and lean on the same enum-field /
//! view / command resolution helpers:
//!
//! - `LZX-WIZARD-STEPS-EXPR-001` — `wizard_steps <N> current <field>`: the
//!   `current` field must be a declared enum field on the bound resource; the
//!   total must be positive (parser guarantees) and should match the enum's
//!   variant count (warn on mismatch — surfaced as a `kind = Warn` finding).
//! - `LZX-TAB-GROUP-CASE-001` — `tab_group derived_from <field>`: the field
//!   must be a declared enum field; every `case` value must be a variant of
//!   that enum; non-exhaustive coverage is a `kind = Warn` finding.
//! - `LZX-TAB-VIEW-REF-001` — each `tab -> view X` and each wizard `step`
//!   must reference a view declared in the same audience.
//! - `LZX-VIEW-MODE-001` — each `view_mode` keyword must parse to a known
//!   render mode (`RenderMode::parse`); `inline_table on_change` must
//!   reference a declared command whose target resource matches the view's.
//! - `LZX-BOARD-LANES-001` (GAP-UX-05) — `view.board lanes derived_from
//!   <field>`: the lane source must be a declared enum field (one lane per
//!   variant) or a has_many relation on the view's bound resource.
//! - `LZX-REPEATABLE-SUM-001` (GAP-UX-05) — `repeatable input … validates
//!   sum(<f>) = <n>`: the summed field must be a numeric field declared in the
//!   group (the parser guarantees `<n>` is a number literal).
//!
//! All operate on the real `lazuli_ir::Module`. Unknown source queries /
//! resources are suppressed (those belong to the source-resource rules).

use std::collections::BTreeSet;

use lazuli_ir::{
    Audience, EnumDecl, Feature, Field, Module, Query, QueryRef, Resource, TypeRef, View, ViewUx,
};

use super::sort_findings;

/// Severity of a UX-rule finding. `Error` blocks; `Warn` is advisory
/// (non-exhaustive coverage, variant-count mismatch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warn,
}

/// One finding from any of the four W6 UX rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub code: &'static str,
    pub severity: Severity,
    pub feature: String,
    pub view: String,
    pub line: usize,
    pub message: String,
}

/// GAP-UX-02 — `view … wizard_steps current <field>`: the stepped field
/// must reference a declared enum field on the bound resource, with one
/// step per variant.
pub const WIZARD_STEPS_CODE: &str = "LZX-WIZARD-STEPS-EXPR-001";
/// GAP-UX-03 — `view … tabs group <field>`: each tab `case` must match a
/// variant of the grouped enum field (exhaustive, no stray cases).
pub const TAB_GROUP_CASE_CODE: &str = "LZX-TAB-GROUP-CASE-001";
/// GAP-UX-03 — `view … tabs … <view>`: each tab must reference a view that
/// exists on the same audience.
pub const TAB_VIEW_REF_CODE: &str = "LZX-TAB-VIEW-REF-001";
/// GAP-UX-04 — `view.<mode>`: the render mode keyword must be one of the
/// closed view-mode catalog (`table`, `kanban`, `calendar`, …).
pub const VIEW_MODE_CODE: &str = "LZX-VIEW-MODE-001";
/// GAP-UX-05 — `view.board lanes derived_from <field>`: the lane source must
/// be a declared enum field (one lane per variant) or a has_many relation on
/// the view's bound resource.
pub const BOARD_LANES_CODE: &str = "LZX-BOARD-LANES-001";
/// GAP-UX-05 — `repeatable input … validates sum(<f>) = <n>`: the summed
/// field must be a numeric field declared in the group (the parser already
/// guarantees `<n>` is a number literal).
pub const REPEATABLE_SUM_CODE: &str = "LZX-REPEATABLE-SUM-001";

/// Run all four W6 surface UX rules across the module.
pub(crate) fn check(module: &Module) -> Vec<Finding> {
    let mut out = Vec::new();
    for feature in &module.features {
        for surface in &feature.surfaces {
            for audience in &surface.audiences {
                check_audience_tabs(module, feature, audience, &mut out);
                for view in &audience.views {
                    let (view_name, source, ux) = match view {
                        View::List(v) => (v.name.as_str(), Some(&v.source), Some(&v.ux)),
                        View::Detail(v) => (v.name.as_str(), Some(&v.source), Some(&v.ux)),
                        View::Create(_) => continue,
                    };
                    let Some(ux) = ux else { continue };
                    let resource = source.and_then(|s| resolve_resource(module, &feature.name, s));
                    check_view_ux(feature, view_name, ux, resource, &mut out);
                }
            }
        }
    }
    sort_findings(&mut out, |f| {
        (f.feature.clone(), f.view.clone(), f.code, f.line)
    });
    out
}

fn check_view_ux(
    feature: &Feature,
    view_name: &str,
    ux: &ViewUx,
    resource: Option<(&Feature, &Resource)>,
    out: &mut Vec<Finding>,
) {
    // ── LZX-WIZARD-STEPS-EXPR-001 ──────────────────────────────────────────
    if let Some(steps) = &ux.wizard_steps {
        let line = steps.span_ref.map(|s| s.start).unwrap_or(0);
        match resource.and_then(|(f, r)| enum_for_field(f, r, &steps.current_field)) {
            None => out.push(Finding {
                code: WIZARD_STEPS_CODE,
                severity: Severity::Error,
                feature: feature.name.clone(),
                view: view_name.to_owned(),
                line,
                message: format!(
                    "wizard_steps `current {}` must reference a declared enum field on the bound resource",
                    steps.current_field
                ),
            }),
            Some(decl) if decl.variants.len() as u32 != steps.total => out.push(Finding {
                code: WIZARD_STEPS_CODE,
                severity: Severity::Warn,
                feature: feature.name.clone(),
                view: view_name.to_owned(),
                line,
                message: format!(
                    "wizard_steps total {} does not match enum `{}` variant count {}",
                    steps.total,
                    decl.name,
                    decl.variants.len()
                ),
            }),
            Some(_) => {}
        }
    }

    // ── LZX-TAB-GROUP-CASE-001 ─────────────────────────────────────────────
    if let Some(group) = &ux.tab_group {
        let line = group.span_ref.map(|s| s.start).unwrap_or(0);
        match resource.and_then(|(f, r)| enum_for_field(f, r, &group.derived_from)) {
            None => out.push(Finding {
                code: TAB_GROUP_CASE_CODE,
                severity: Severity::Error,
                feature: feature.name.clone(),
                view: view_name.to_owned(),
                line,
                message: format!(
                    "tab_group `derived_from {}` must reference a declared enum field on the bound resource",
                    group.derived_from
                ),
            }),
            Some(decl) => {
                // The grammar declares enum variants as `IDENT_LOWER`
                // (`reassign`) but `tab_group` `case` references them as
                // `IDENT_UPPER` (`REASSIGN`) — see `docs/grammar.lzx.md` §7a.3
                // (`enum_variant_list = IDENT_UPPER`). Compare on the canonical
                // screaming-snake projection so the dialects line up; the
                // diagnostic message still echoes the authored token. Matching
                // on the raw casing (as this rule originally did) over-fired on
                // every conformant `tab_group` whose enum is lowercase-declared.
                let variants: BTreeSet<String> = decl
                    .variants
                    .iter()
                    .map(|v| screaming_snake(&v.name))
                    .collect();
                let mut covered: BTreeSet<String> = BTreeSet::new();
                for case in &group.cases {
                    let case_line = case.span_ref.map(|s| s.start).unwrap_or(line);
                    for v in &case.variants {
                        let key = screaming_snake(v);
                        if !variants.contains(&key) {
                            out.push(Finding {
                                code: TAB_GROUP_CASE_CODE,
                                severity: Severity::Error,
                                feature: feature.name.clone(),
                                view: view_name.to_owned(),
                                line: case_line,
                                message: format!(
                                    "tab_group case `{}` is not a variant of enum `{}`",
                                    v, decl.name
                                ),
                            });
                        } else {
                            covered.insert(key);
                        }
                    }
                }
                // Report missing variants in the authored (declared) casing so
                // the message matches what the user wrote in the `.lzi`.
                let missing: Vec<&str> = decl
                    .variants
                    .iter()
                    .filter(|v| !covered.contains(&screaming_snake(&v.name)))
                    .map(|v| v.name.as_str())
                    .collect();
                if !missing.is_empty() {
                    out.push(Finding {
                        code: TAB_GROUP_CASE_CODE,
                        severity: Severity::Warn,
                        feature: feature.name.clone(),
                        view: view_name.to_owned(),
                        line,
                        message: format!(
                            "tab_group over enum `{}` is non-exhaustive: missing {}",
                            decl.name,
                            missing.join(", ")
                        ),
                    });
                }
            }
        }
    }

    // ── LZX-VIEW-MODE-001 (inline_table command target) ────────────────────
    // The `view_mode` keyword closed-catalog check happens at analyzer
    // lowering (unknown keywords raise `AnalyzeError::LzxUnknownRenderMode`);
    // by the time the IR carries `Vec<RenderMode>`, every mode is valid. The
    // doctor half of `LZX-VIEW-MODE-001` enforces the `inline_table` command
    // target.
    if let Some(inline) = &ux.inline_table {
        let line = inline.span_ref.map(|s| s.start).unwrap_or(0);
        let cmd_feature = inline.on_change.feature.as_str();
        let cmd_name = inline.on_change.name.as_str();
        // The command must be declared (on its own feature). When the inline
        // table's view binds a resource, the command should target it — but
        // command→resource targeting is not carried on `CommandRef`, so we
        // verify existence here and leave deeper target matching to the
        // command-routing rules.
        // `(A && X) || !A` ≡ `!A || X`: cross-feature refs are trusted (resolution
        // happens elsewhere); same-feature refs must name a real command.
        let cmd_known =
            cmd_feature != feature.name || feature.commands.iter().any(|c| c.name == cmd_name);
        if !cmd_known {
            out.push(Finding {
                code: VIEW_MODE_CODE,
                severity: Severity::Error,
                feature: feature.name.clone(),
                view: view_name.to_owned(),
                line,
                message: format!(
                    "view.inline_table on_change references unknown command `{}`",
                    cmd_name
                ),
            });
        }
    }

    // ── LZX-BOARD-LANES-001 ────────────────────────────────────────────────
    // `view.board lanes derived_from <field>` — the lane source must be a
    // declared enum field (one lane per variant) OR a has_many relation
    // (`TypeRef::Many`) on the view's bound resource.
    // Only assert when we positively resolved the bound resource. A view
    // whose source query cannot be resolved to a concrete resource (notably
    // the experience→web projection, which synthesizes a SOURCELESS
    // `query.list` ref — see
    // `lazuli_analyzer::lzx_p1::lower_feature_view_from_experience`) gives the
    // rule no resource to check `derived_from` against. Firing there is a
    // false positive: "can't validate" is NOT "invalid". This closes the
    // `LZX-BOARD-LANES-001` over-block on pauta `activity_board` (sources a
    // sourceless ref into a 2-resource feature). When the source DOES name a
    // query, `resolve_resource` scores it to the right resource (codegen
    // parity) and the lane source is checked against THAT resource — a
    // genuinely-bad lane source still fires.
    if let (Some(board), Some((res_feature, res))) = (&ux.board, resource) {
        let line = board.span_ref.map(|s| s.start).unwrap_or(0);
        let field = field_on(res, &board.lanes_source);
        // A lane source is valid when the field resolves to a declared enum
        // (one lane per variant) OR is a has_many relation (`TypeRef::Many`).
        // Enum fields lower to `TypeRef::UserDefined` (the bare `enum X`
        // domain decl) or `TypeRef::EnumRef` (the lifecycle-synthesized
        // discriminator); `enum_for_field` resolves both. Matching only on
        // `EnumRef` (as this rule originally did) over-fired on every plain
        // `enum`-typed lane field.
        let resolves_enum = enum_for_field(res_feature, res, &board.lanes_source).is_some();
        let valid = resolves_enum || matches!(field.map(|f| &f.type_ref), Some(TypeRef::Many(_)));
        if !valid {
            out.push(Finding {
                code: BOARD_LANES_CODE,
                severity: Severity::Error,
                feature: feature.name.clone(),
                view: view_name.to_owned(),
                line,
                message: format!(
                    "view.board `lanes derived_from {}` must reference a declared enum field or has_many relation on the bound resource",
                    board.lanes_source
                ),
            });
        }
    }

    // ── LZX-REPEATABLE-SUM-001 ─────────────────────────────────────────────
    // `repeatable input … validates sum(<f>) = <n>` — the summed field must
    // be a numeric field declared inside the group. The parser already
    // guarantees the `<n>` target is a number literal.
    for group in &ux.repeatable_groups {
        let line = group.span_ref.map(|s| s.start).unwrap_or(0);
        match group.fields.iter().find(|f| f.name == group.sum_field) {
            None => out.push(Finding {
                code: REPEATABLE_SUM_CODE,
                severity: Severity::Error,
                feature: feature.name.clone(),
                view: view_name.to_owned(),
                line,
                message: format!(
                    "repeatable input `{}` sums `{}`, which is not a field declared in the group",
                    group.name, group.sum_field
                ),
            }),
            Some(field) if !is_numeric_type_name(&field.type_name) => out.push(Finding {
                code: REPEATABLE_SUM_CODE,
                severity: Severity::Error,
                feature: feature.name.clone(),
                view: view_name.to_owned(),
                line,
                message: format!(
                    "repeatable input `{}` sums non-numeric field `{}` (type `{}`); sum requires a numeric field",
                    group.name, group.sum_field, field.type_name
                ),
            }),
            Some(_) => {}
        }
    }
}

/// Find field `name` on `resource`.
fn field_on<'a>(resource: &'a Resource, name: &str) -> Option<&'a Field> {
    resource.fields.iter().find(|f| f.name == name)
}

/// Canonical screaming-snake projection used to compare an enum variant's
/// declared `IDENT_LOWER` name (`reassign`, `in_progress`) against a
/// `tab_group` `case` `IDENT_UPPER` reference (`REASSIGN`, `IN_PROGRESS`).
/// Inserts an `_` at lower→upper boundaries (so a stray camelCase token still
/// normalizes), collapses existing separators, then upper-cases — matching the
/// codegen `screaming_snake` (`to_snake_case().to_ascii_uppercase()`).
fn screaming_snake(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 4);
    let mut prev_lower_or_digit = false;
    for ch in value.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            if !out.ends_with('_') && !out.is_empty() {
                out.push('_');
            }
            prev_lower_or_digit = false;
            continue;
        }
        if ch.is_ascii_uppercase() && prev_lower_or_digit && !out.ends_with('_') {
            out.push('_');
        }
        out.push(ch.to_ascii_uppercase());
        prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    out.trim_matches('_').to_owned()
}

/// True when a repeatable-group field's type keyword names a numeric type the
/// `sum(...)` constraint can aggregate. Accepts the closed numeric builtins
/// (`Integer`/`Int`, `Decimal`, `Float`) plus the numeric semantics
/// (`Money`, `Percentage`). Matching is on the authored type keyword (the
/// repeatable group carries its fields verbatim, not as resolved `TypeRef`);
/// the leading `@semantic.` namespace, if present, is stripped first.
fn is_numeric_type_name(type_name: &str) -> bool {
    let bare = type_name
        .trim()
        .strip_prefix("@semantic.")
        .unwrap_or(type_name.trim());
    matches!(
        bare,
        "Int" | "Integer" | "Decimal" | "Float" | "Number" | "Money" | "Percentage"
    )
}

/// `LZX-TAB-VIEW-REF-001` — tab/wizard refs resolve to a declared view in the
/// same audience.
fn check_audience_tabs(
    _module: &Module,
    feature: &Feature,
    audience: &Audience,
    out: &mut Vec<Finding>,
) {
    let view_names: BTreeSet<&str> = audience.views.iter().map(|v| v.name()).collect();
    for tabs in &audience.ux.tabs {
        for entry in &tabs.entries {
            if !view_names.contains(entry.view.as_str()) {
                out.push(Finding {
                    code: TAB_VIEW_REF_CODE,
                    severity: Severity::Error,
                    feature: feature.name.clone(),
                    view: entry.view.clone(),
                    line: entry.span_ref.map(|s| s.start).unwrap_or(0),
                    message: format!(
                        "tab \"{}\" references view `{}` not declared in audience `{}`",
                        entry.label, entry.view, audience.name
                    ),
                });
            }
        }
    }
    for wizard in &audience.ux.wizards {
        for step in &wizard.steps {
            if !view_names.contains(step.ref_name.as_str()) {
                out.push(Finding {
                    code: TAB_VIEW_REF_CODE,
                    severity: Severity::Error,
                    feature: feature.name.clone(),
                    view: step.ref_name.clone(),
                    line: step.span_ref.map(|s| s.start).unwrap_or(0),
                    message: format!(
                        "wizard `{}` step {} references view/form `{}` not declared in audience `{}`",
                        wizard.name, step.index, step.ref_name, audience.name
                    ),
                });
            }
        }
    }
}

/// Resolve the resource backing a view's source query. Returns the owning
/// feature + resource so the enum lookup can reach the feature's enum decls.
///
/// Multi-resource features resolve the SAME way codegen does in
/// `crates/lazuli_codegen_go/src/emitter/query/util.rs::resource_for_query`:
/// a `query.sql ... returns <T>` binds to its declared return type, otherwise
/// the query name is scored against each resource's name tokens (with `plural`
/// tolerance) and the best match wins. So `list_job_steps` resolves to
/// `JobStep` even when the feature also declares `Activity` — closing the
/// `LZX-BOARD-LANES-001` false-positive that previously bailed (returned
/// `None`) on every multi-resource feature.
fn resolve_resource<'a>(
    module: &'a Module,
    owning_feature: &str,
    source: &QueryRef,
) -> Option<(&'a Feature, &'a Resource)> {
    let source_feature_name = if source.feature.is_empty() {
        owning_feature
    } else {
        source.feature.as_str()
    };
    let feature = module
        .features
        .iter()
        .find(|f| f.name == source_feature_name)?;
    let resource = find_resource_for_query(feature, source)?;
    Some((feature, resource))
}

/// Resolve a view's source `QueryRef` to the resource it returns, mirroring
/// codegen's `query/util.rs::resource_for_query`:
///
/// 1. A single-resource feature resolves unambiguously.
/// 2. A `query.sql ... returns <T>` binds to its declared return type.
/// 3. Otherwise score the query name against each resource's identifier
///    tokens (`plural`-tolerant) and pick the best match — `list_job_steps`
///    → `JobStep`, `lookup_user_session` → `UserSession`.
fn find_resource_for_query<'a>(feature: &'a Feature, source: &QueryRef) -> Option<&'a Resource> {
    if feature.resources.len() <= 1 {
        return feature.resources.first();
    }

    // Multi-resource feature with no query name to resolve against — bail.
    // The experience→web projection lowers list/detail views with a SOURCELESS
    // synthetic `query.list` ref (empty `name`); scoring an empty name would
    // pick an arbitrary resource and turn a non-validatable view into a false
    // positive. Returning `None` makes the caller skip the assertion instead.
    if source.name.is_empty() {
        return None;
    }

    // `query.sql ... returns <T>` carries an explicit return type — honour it.
    if let Some(Query::Sql(sql)) = feature.queries.iter().find(|q| q.name() == source.name)
        && let Some(resource_name) = resource_name_from_type_ref(&sql.returns)
        && let Some(resource) = feature.resources.iter().find(|r| r.name == resource_name)
    {
        return Some(resource);
    }

    // Score-based name resolution (the codegen path for list/lookup queries).
    let query_tokens = split_ident_tokens(&source.name);
    feature
        .resources
        .iter()
        .map(|resource| {
            let tokens = split_ident_tokens(&resource.name);
            let last = tokens.last().cloned().unwrap_or_default();
            let mut score = 0usize;
            for token in &tokens {
                if query_tokens.iter().any(|q| q == token || q == &plural(token)) {
                    score += 10;
                }
            }
            if !last.is_empty()
                && query_tokens.iter().any(|q| q == &last || q == &plural(&last))
            {
                score += 50;
            }
            (score, resource)
        })
        // Highest score wins; tie-break on name descending to match codegen's
        // deterministic ordering. A zero-score winner still resolves to *a*
        // resource (codegen does the same) — the lane-source check below then
        // verifies the relation/field actually exists on it.
        .max_by(|(sa, a), (sb, b)| sa.cmp(sb).then_with(|| b.name.cmp(&a.name)))
        .map(|(_, resource)| resource)
}

/// Project a `TypeRef` to its resource name (`User`, or the inner type of a
/// `Many(...)` collection return). Mirrors codegen's
/// `query/util.rs::resource_name_from_type_ref` shape.
fn resource_name_from_type_ref(type_ref: &TypeRef) -> Option<&str> {
    match type_ref {
        TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) => Some(qname.name.as_str()),
        TypeRef::Many(inner) => resource_name_from_type_ref(inner),
        _ => None,
    }
}

/// Naive lowercase pluralizer used only by the `find_resource_for_query`
/// scorer (never emitted). Mirrors codegen's `query/util.rs::plural`.
fn plural(word: &str) -> String {
    if let Some(stem) = word.strip_suffix('y') {
        format!("{stem}ies")
    } else if word.ends_with('s') {
        format!("{word}es")
    } else {
        format!("{word}s")
    }
}

/// Split an identifier into lowercase tokens for the resource scorer.
/// Mirrors codegen's `query/util.rs::split_ident_tokens`.
fn split_ident_tokens(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut prev_lower_or_digit = false;
    for ch in s.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            if !current.is_empty() {
                words.push(current.to_ascii_lowercase());
                current.clear();
            }
            prev_lower_or_digit = false;
            continue;
        }
        if ch.is_ascii_uppercase() && prev_lower_or_digit && !current.is_empty() {
            words.push(current.to_ascii_lowercase());
            current.clear();
        }
        current.push(ch);
        prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    if !current.is_empty() {
        words.push(current.to_ascii_lowercase());
    }
    words
}

/// Find the enum declaration backing field `field_name` on `resource`.
fn enum_for_field<'a>(
    feature: &'a Feature,
    resource: &Resource,
    field_name: &str,
) -> Option<&'a EnumDecl> {
    let field = resource.fields.iter().find(|f| f.name == field_name)?;
    let enum_name = enum_type_name(field)?;
    feature.enums.iter().find(|e| e.name == enum_name)
}

fn enum_type_name(field: &Field) -> Option<&str> {
    match &field.type_ref {
        TypeRef::EnumRef(qname) | TypeRef::UserDefined(qname) => Some(qname.name.as_str()),
        _ => None,
    }
}

#[cfg(test)]
#[path = "ux_rules_tests.rs"]
mod tests;
