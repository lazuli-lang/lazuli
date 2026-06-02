//! ENUM-VARIANT-UNDECLARED-001 — an enum-variant reference names a variant
//! the target enum never declared.
//!
//! **Severity:** error. This is a correctness gap, not style — a bare or
//! predicate-RHS enum-variant *typo* is silently accepted as a `FromConst`
//! literal instead of being validated against the enum's declared variants.
//! Example: `status == publishedd` (a typo for `published`) compiles fine
//! and `crates/lazuli_codegen_go/src/emitter/query/filters.rs`
//! (`format_source_expr`) lowers the unqualified `Expr::Enum` to
//! `lazuli.FromConst("publishedd")` — a Postgres bind parameter that never
//! matches any row. The bug is latent: the query simply returns empty
//! forever. This rule upgrades that silent acceptance into a hard,
//! build-failing diagnostic that names the bad variant + the declared set.
//!
//! **Fires when** an enum literal (`Expr::Enum`) appears in a query filter
//! predicate RHS (`<enum_field> == <variant>`) or a resource field default
//! (`status: Status = <variant>`) whose `variant` is none of the target
//! enum's declared variant names (nor a recorded `previous_names` rename
//! alias). Example fixtures that fire: `status == publishedd` against an
//! `enum Status { draft published archived }`, or a default
//! `status: Status = activ` (typo for `active`).
//!
//! **Does not fire** for a correctly-spelled variant (`status == published`),
//! a comparison against a genuinely free-text `Text` field (`title ==
//! "anything"` — the RHS is `Expr::String`, never `Expr::Enum`, and the
//! field's `TypeRef` is a builtin, not an enum), a dynamic/bound RHS
//! (`status == params.kind` is a `Path`, resolved at runtime — not a
//! literal), a recorded `previous_names` rename alias, or an enum reference
//! the rule cannot bind to a declared enum (no target resource / unresolvable
//! field / unknown enum type → conservative skip, no false positive).
//!
//! The enum-typed field's `TypeRef` lowers as either `EnumRef(<Enum>)` or
//! `UserDefined(<Name>)`; `enum_for_type_ref` accepts both and confirms the
//! name binds to a declared enum on the feature.

use std::path::{Path, PathBuf};

use lazuli_ir::{
    DefaultValue, EnumDecl, EnumLiteral, Expr, Feature, Predicate, Query, Resource, TypeRef,
};

// output

/// One ENUM-VARIANT-UNDECLARED-001 finding: an enum literal naming a variant
/// the resolved enum does not declare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    /// The enum type the variant was checked against (e.g. `Status`).
    pub enum_name: String,
    /// The undeclared variant the author wrote (e.g. `publishedd`).
    pub variant: String,
    /// The declared variant names, in declaration order, for the hint.
    pub declared: Vec<String>,
    /// Where the bad reference sits, for the message:
    /// `query filter \`list_posts\`` or `field default \`Post.status\``.
    pub site: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "ENUM-VARIANT-UNDECLARED-001";

    /// Render the "undeclared enum variant" message — name the enum, the bad
    /// variant, the site, and the declared set so the author can spot the typo.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::enum_variant_undeclared_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("posts.lzi"),
    ///     enum_name: "Status".into(),
    ///     variant: "publishedd".into(),
    ///     declared: vec!["draft".into(), "published".into()],
    ///     site: "query filter `list_posts`".into(),
    /// };
    /// assert!(f.message().contains("publishedd"));
    /// assert!(f.message().contains("published"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{site}: `{variant}` is not a declared variant of enum `{enum_name}`. \
             It would silently lower to a `FromConst(\"{variant}\")` literal that \
             never matches. Declared variants: {declared}.",
            site = self.site,
            variant = self.variant,
            enum_name = self.enum_name,
            declared = if self.declared.is_empty() {
                "(none)".to_owned()
            } else {
                self.declared
                    .iter()
                    .map(|v| format!("`{v}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            },
        )
    }
}

// detection

/// Run ENUM-VARIANT-UNDECLARED-001 over one feature: every query filter
/// predicate RHS that is an enum literal, plus every resource field default
/// that is an enum literal.
///
/// `path` anchors findings; no I/O is performed here.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::enum_variant_undeclared_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with enums + queries");
/// let _ = check(&feature, Path::new("posts.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut out = Vec::new();
    check_query_filters(feature, path, &mut out);
    check_field_defaults(feature, path, &mut out);
    out
}

// internals — query filters (predicate-RHS variant; audit 05-DX F4 + 03)

fn check_query_filters(feature: &Feature, path: &Path, out: &mut Vec<Finding>) {
    for query in &feature.queries {
        let (name, filters) = match query {
            Query::List(q) => (q.name.as_str(), &q.filters),
            Query::Lookup(q) => (q.name.as_str(), &q.filters),
            // `query.sql` carries no typed filter predicates (hand-rolled SQL).
            Query::Sql(_) => continue,
        };
        if filters.is_empty() {
            continue;
        }
        let resource = resource_for_query(feature, name);
        for filter in filters {
            collect_predicate(&filter.predicate, feature, resource, name, path, out);
        }
    }
}

/// Walk a predicate tree, validating any enum literal that compares against
/// an enum-typed column. `And`/`Or` recurse; `Has` is value-membership
/// (no column→enum binding the rule can trust), so it is skipped.
fn collect_predicate(
    predicate: &Predicate,
    feature: &Feature,
    resource: Option<&Resource>,
    query_name: &str,
    path: &Path,
    out: &mut Vec<Finding>,
) {
    match predicate {
        Predicate::Comparison { left, op: _, right } => {
            // One side is the column (a non-source `Path`), the other the
            // enum literal. Determine the column field, then validate the
            // literal against that field's enum (or against the literal's own
            // qualifier when it is fully qualified, e.g. `Status.published`).
            // RHS-literal (`status == published`) is the canonical shape;
            // the LHS-literal (`published == status`) shape is handled
            // symmetrically.
            let (literal, column_path) = match (literal_of(left), literal_of(right)) {
                (_, Some(lit)) => (lit, left.column_segments()),
                (Some(lit), _) => (lit, right.column_segments()),
                _ => return,
            };
            // Resolve which enum to check against.
            if let Some(enum_decl) =
                resolve_enum(literal, column_path.as_deref(), feature, resource)
                && !variant_declared(enum_decl, &literal.variant)
            {
                out.push(Finding {
                    path: path.to_path_buf(),
                    enum_name: enum_decl.name.clone(),
                    variant: literal.variant.clone(),
                    declared: enum_decl.variants.iter().map(|v| v.name.clone()).collect(),
                    site: format!("query filter `{query_name}`"),
                });
            }
        }
        Predicate::And(inner) | Predicate::Or(inner) => {
            for p in inner {
                collect_predicate(p, feature, resource, query_name, path, out);
            }
        }
        Predicate::Has { .. } => {}
    }
}

/// Pull the enum literal out of an `Expr` if it is one.
fn literal_of(expr: &Expr) -> Option<&EnumLiteral> {
    match expr {
        Expr::Enum(lit) => Some(lit),
        _ => None,
    }
}

trait ColumnSegments {
    /// Segments of a non-source column path (`status` / `self.status`),
    /// or `None` when the expr is a runtime source path / literal.
    fn column_segments(&self) -> Option<Vec<String>>;
}

impl ColumnSegments for Expr {
    fn column_segments(&self) -> Option<Vec<String>> {
        match self {
            Expr::Path(p) if !is_source_path(&p.segments) => {
                let segs = if p.segments.first().map(String::as_str) == Some("self") {
                    &p.segments[1..]
                } else {
                    &p.segments[..]
                };
                Some(segs.iter().map(|s| s.to_ascii_lowercase()).collect())
            }
            _ => None,
        }
    }
}

/// Runtime source paths (`params.x`, `ctx.user.id`, …) are not columns.
fn is_source_path(segments: &[String]) -> bool {
    matches!(
        segments.first().map(String::as_str),
        Some("params" | "input" | "ctx" | "target" | "route")
    )
}

// internals — field defaults (bare variant; audit 05-DX F4)

fn check_field_defaults(feature: &Feature, path: &Path, out: &mut Vec<Finding>) {
    for resource in &feature.resources {
        for field in &resource.fields {
            let Some(DefaultValue::EnumLiteral(literal)) = &field.default else {
                continue;
            };
            // The field type is the authoritative enum binding for a default.
            let enum_decl = enum_for_type_ref(&field.type_ref, feature).or_else(|| {
                // Fall back to the literal's own qualifier if the field type
                // didn't resolve to an enum (defensive — covers a default on a
                // field whose enum type stayed Unresolved).
                literal
                    .type_name
                    .as_ref()
                    .and_then(|q| find_enum(feature, &q.name))
            });
            if let Some(enum_decl) = enum_decl
                && !variant_declared(enum_decl, &literal.variant)
            {
                out.push(Finding {
                    path: path.to_path_buf(),
                    enum_name: enum_decl.name.clone(),
                    variant: literal.variant.clone(),
                    declared: enum_decl.variants.iter().map(|v| v.name.clone()).collect(),
                    site: format!("field default `{}.{}`", resource.name, field.name),
                });
            }
        }
    }
}

// internals — enum + variant resolution

/// Resolve the [`EnumDecl`] an enum literal should be checked against.
///
/// Precedence:
///  1. A fully-qualified literal (`Status.published`) names its enum directly.
///  2. Otherwise the comparison column (`status`) is resolved to a field whose
///     `type_ref` is `EnumRef(<Enum>)`.
///
/// Returns `None` (conservative skip — no false positive) when the literal is
/// unqualified AND the column cannot be bound to an enum-typed field. This is
/// the deliberate guard against flagging free-text fields or dynamic values.
fn resolve_enum<'a>(
    literal: &EnumLiteral,
    column_path: Option<&[String]>,
    feature: &'a Feature,
    resource: Option<&'a Resource>,
) -> Option<&'a EnumDecl> {
    if let Some(qname) = &literal.type_name {
        return find_enum(feature, &qname.name);
    }
    let column_path = column_path?;
    let resource = resource?;
    let column = column_to_field_name(column_path);
    let field = resource.fields.iter().find(|f| f.name == column)?;
    enum_for_type_ref(&field.type_ref, feature)
}

/// Resolve a field's `type_ref` to the [`EnumDecl`] it names, if any.
///
/// The analyzer lowers an enum-typed field as either `EnumRef(<Enum>)` (when
/// the type resolved to a declared enum at lowering) OR `UserDefined(<Name>)`
/// (the broader record/enum reference shape). We accept BOTH and confirm the
/// name binds to a declared enum on the feature — so the rule stays correct
/// regardless of which shape lowering chose. A `Text`/builtin/`Unresolved`
/// field returns `None` (the free-text-field guard).
fn enum_for_type_ref<'a>(type_ref: &TypeRef, feature: &'a Feature) -> Option<&'a EnumDecl> {
    match type_ref {
        TypeRef::EnumRef(qname) | TypeRef::UserDefined(qname) => find_enum(feature, &qname.name),
        _ => None,
    }
}

/// Lower a column path to the resource field name. Filters author single
/// segments (`status`) or rare dotted forms; we take the first segment as the
/// field name (FK `.id` traversal is never enum-typed, so first-segment is
/// the correct field to bind).
fn column_to_field_name(segments: &[String]) -> String {
    segments
        .first()
        .cloned()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Find a declared enum by its (unqualified) name on the feature.
fn find_enum<'a>(feature: &'a Feature, name: &str) -> Option<&'a EnumDecl> {
    feature.enums.iter().find(|e| e.name == name)
}

/// True when the variant is declared on the enum, OR is a recorded rename
/// alias (a `previous_names` entry) — the rename path must not false-positive.
fn variant_declared(enum_decl: &EnumDecl, variant: &str) -> bool {
    enum_decl
        .variants
        .iter()
        .any(|v| v.name == variant || v.previous_names.iter().any(|p| p == variant))
}

/// Score-based `query name -> resource` resolution. Mirrors the codegen
/// `resource_for_query` heuristic (the SAME binding codegen uses to attach a
/// query to its resource), so "the rule passes" ⟺ "codegen bound the same
/// enum field." A single-resource feature is the common case (no-op match);
/// multi-resource features score identifier-token overlap with plural
/// tolerance (`list_posts` -> `Post`).
fn resource_for_query<'a>(feature: &'a Feature, query_name: &str) -> Option<&'a Resource> {
    let mut resources: Vec<&Resource> = feature.resources.iter().collect();
    resources.sort_by(|a, b| a.name.cmp(&b.name));
    if resources.len() <= 1 {
        return resources.into_iter().next();
    }
    let query_tokens = split_ident_tokens(query_name);
    resources
        .into_iter()
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
        .max_by(|(score_a, a), (score_b, b)| {
            score_a.cmp(score_b).then_with(|| b.name.cmp(&a.name))
        })
        .map(|(_, resource)| resource)
}

/// Lowercase token split for the scorer (`UserSession` -> `["user","session"]`,
/// `list_posts` -> `["list","posts"]`). Mirrors codegen's `split_ident_tokens`.
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

/// Naive lowercase pluralizer for token matching only. Mirrors codegen.
fn plural(word: &str) -> String {
    if let Some(stem) = word.strip_suffix('y') {
        format!("{stem}ies")
    } else if word.ends_with('s') {
        format!("{word}es")
    } else {
        format!("{word}s")
    }
}

// tests

#[cfg(test)]
mod tests {
    include!("enum_variant_undeclared_001_tests.rs");
}
