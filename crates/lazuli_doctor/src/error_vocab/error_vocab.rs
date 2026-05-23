//! IR Error-Vocab — 7 typed analyzer cross-checks (Cell ANALYZE-1).
//!
//! Each rule is a self-contained `check_*` function that consumes the
//! lowered IR slice it needs and returns a `Finding`-typed vec. The doctor
//! pipeline (`lazuli_cli::doctor`) adapts findings to `DoctorDiagnostic`
//! and routes the per-rule severity. The LSP can mount the same checks
//! for live diagnostics in a later cell.
//!
//! Reference: `docs/proposals/ir-error-messages-vocab.md` §6 §11
//! Cell ANALYZE-1.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{
    Command, Feature, FeatureErrorMessage, FeatureErrors, PolicyCategory, PolicyExpr, PolicyRef,
    SpanRef, TranslationKeyRef,
};

// =============================================================================
// Closed catalogs — kept in sync with the runtime (`runtime/go/lazuli/error.go`)
// and the IR comment on `FeatureErrors.exposure_4xx` / `exposure_5xx`.
// =============================================================================

/// Closed catalog of 12 framework error codes that may be overridden via
/// `errors <code> message @translation.<key>` (proposal §2.B).
///
/// DB-INTEGRITY-CATALOG-EXT (2026-05-19): extended with the 4 db-integrity
/// codes emitted by the runtime's `classifyDBError` helper. These map
/// Postgres class-23 SQLSTATEs (`23505`, `23503`, `23502`, `23514`) to
/// stable wire codes so authors can author `errors unique_violation
/// message @translation.<key>` overrides without leaking SQL internals.
pub const FRAMEWORK_ERROR_CODES: &[&str] = &[
    "policy_denied",
    "validation_failed",
    "tenant_mismatch",
    "not_found",
    "rate_limited",
    "bad_request",
    "method_not_allowed",
    "integration_error",
    "unique_violation",
    "foreign_key_violation",
    "not_null_violation",
    "check_violation",
];

/// Closed catalog of `expose client 4xx <field>` tokens (proposal §2.C).
pub const EXPOSE_4XX_FIELDS: &[&str] = &["message", "code", "data", "message_key"];

/// Closed catalog of `expose client 5xx <field>` tokens (proposal §2.C).
/// `message` is deliberately excluded — see ERR-VOCAB-EXPOSE-5XX-MESSAGE.
pub const EXPOSE_5XX_FIELDS: &[&str] = &["code", "data"];

// =============================================================================
// ERR-VOCAB-001 — feature has `policies` block but no `when_denied`
//                 anywhere and no `errors policy_denied` catch-all.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoliciesNoWhenDeniedFinding {
    pub path: PathBuf,
    pub feature: String,
    /// 1-based source line of the `policies` header; `None` when the
    /// feature carries no span (programmatic construction).
    pub policies_span: Option<SpanRef>,
}

impl PoliciesNoWhenDeniedFinding {
    pub const CODE: &'static str = "ERR-VOCAB-001";

    pub fn message(&self) -> String {
        format!(
            "feature `{}` declares policies but no `when_denied` overrides. Commands gated by \
             these policies will fall back to the framework's built-in PT-BR/en-US text. Add \
             `when_denied @translation.<key>` to each policy, or declare `errors policy_denied \
             message @translation.<key>` once for the feature, to customize the human-readable \
             phrasing.",
            self.feature
        )
    }
}

/// Run ERR-VOCAB-001 over one lowered feature.
///
/// Fires when:
/// 1. The feature authored a `policies` block (`policies.span_ref.is_some()`),
/// 2. No named policy in that block has `when_denied`,
/// 3. The feature has no `errors policy_denied message @translation.<key>`
///    catch-all (`feature.errors.messages` contains no `code == "policy_denied"`).
pub fn check_policies_no_when_denied(
    feature: &Feature,
    path: &Path,
) -> Vec<PoliciesNoWhenDeniedFinding> {
    // Step 1: the `policies` block must have been authored explicitly.
    // `Policies::default()` carries `span_ref: None`, so this distinguishes
    // "absent" from "declared but empty".
    if feature.policies.span_ref.is_none() {
        return Vec::new();
    }
    // Step 2: at least one named policy must exist — empty `policies {}`
    // can't trigger because there's nothing to gate against.
    if feature.policies.categories.is_empty() {
        return Vec::new();
    }
    // Step 3: any `when_denied` anywhere silences the warning.
    let any_when_denied = feature
        .policies
        .categories
        .iter()
        .any(|c| c.when_denied.is_some());
    if any_when_denied {
        return Vec::new();
    }
    // Step 4: a feature-level `errors policy_denied` catch-all also silences.
    if has_policy_denied_catchall(feature.errors.as_ref()) {
        return Vec::new();
    }
    vec![PoliciesNoWhenDeniedFinding {
        path: path.to_path_buf(),
        feature: feature.name.clone(),
        policies_span: feature.policies.span_ref,
    }]
}

fn has_policy_denied_catchall(errors: Option<&FeatureErrors>) -> bool {
    let Some(errors) = errors else { return false };
    errors.messages.iter().any(|m| m.code == "policy_denied")
}

// =============================================================================
// ERR-VOCAB-002 — `@translation.<key>` does not resolve to a declared key.
//
// Sites checked:
// * `Command.policy_when_denied`            (per-command override)
// * `PolicyCategory.when_denied`            (per-policy override)
// * `FeatureErrors.messages[].message`      (feature-level catch-all)
//
// Resolution scope: same feature first; falls back to features named in
// `Feature.uses` (cross-feature lookup). The legacy `rule_message_ref_unresolved`
// rule covers `Rule.message_ref` strings; this rule extends the cross-check
// to the new typed surfaces.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyUnknownFinding {
    pub path: PathBuf,
    pub feature: String,
    pub key: String,
    /// Where the reference was authored — e.g. `command capture_lead.when_denied`,
    /// `policies update.when_denied`, `errors policy_denied`. Improves the
    /// rendered message.
    pub site: String,
    pub span: Option<SpanRef>,
}

impl KeyUnknownFinding {
    pub const CODE: &'static str = "ERR-VOCAB-002";

    pub fn message(&self, declared: &[&str]) -> String {
        let declared_text = if declared.is_empty() {
            "<none declared>".to_owned()
        } else {
            declared.join(", ")
        };
        format!(
            "`@translation.{}` referenced from {} does not resolve in feature `{}`. Declared \
             keys: {}.",
            self.key, self.site, self.feature, declared_text
        )
    }
}

/// Run ERR-VOCAB-002 over one feature. `keys_by_feature` maps every
/// feature name to its declared translation key catalog so cross-feature
/// `uses` lookup works without re-walking the package.
pub fn check_translation_key_unknown(
    feature: &Feature,
    path: &Path,
    keys_by_feature: &std::collections::BTreeMap<String, BTreeSet<String>>,
) -> Vec<KeyUnknownFinding> {
    let mut findings = Vec::new();
    let visible = visible_translation_keys(feature, keys_by_feature);

    // Site 1 — per-command `when_denied`.
    for cmd in &feature.commands {
        if let Some(reference) = &cmd.policy_when_denied {
            ensure_resolves(
                reference,
                &visible,
                feature,
                path,
                format!("command `{}.{}.when_denied`", feature.name, cmd.name),
                &mut findings,
            );
        }
    }
    // Site 2 — per-policy `when_denied`.
    for category in &feature.policies.categories {
        if let Some(reference) = &category.when_denied {
            ensure_resolves(
                reference,
                &visible,
                feature,
                path,
                format!(
                    "policies `{}.{}.when_denied`",
                    feature.name, category.name
                ),
                &mut findings,
            );
        }
    }
    // Site 3 — feature-level `errors <code> message @translation.<key>`.
    if let Some(errors) = &feature.errors {
        for entry in &errors.messages {
            ensure_resolves(
                &entry.message,
                &visible,
                feature,
                path,
                format!("errors `{}.{}`", feature.name, entry.code),
                &mut findings,
            );
        }
    }
    findings
}

fn ensure_resolves(
    reference: &TranslationKeyRef,
    visible: &BTreeSet<String>,
    feature: &Feature,
    path: &Path,
    site: String,
    out: &mut Vec<KeyUnknownFinding>,
) {
    if visible.contains(&reference.key) {
        return;
    }
    out.push(KeyUnknownFinding {
        path: path.to_path_buf(),
        feature: feature.name.clone(),
        key: reference.key.clone(),
        site,
        span: reference.span_ref,
    });
}

fn visible_translation_keys(
    feature: &Feature,
    keys_by_feature: &std::collections::BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut visible = BTreeSet::new();
    // Same-feature keys take precedence.
    if let Some(translation) = &feature.translation {
        for key in &translation.keys {
            visible.insert(key.name.clone());
        }
    }
    // Cross-feature lookup through `uses` — the resolution chain (proposal
    // §2.E step 3) allows a feature to reference translation keys from any
    // feature it explicitly consumes via `uses`. The mapping is package-
    // wide because `Feature.uses` lists feature names only (no per-key
    // visibility surface in v1).
    for used in &feature.uses {
        if let Some(declared) = keys_by_feature.get(used) {
            for key in declared {
                visible.insert(key.clone());
            }
        }
    }
    visible
}

// =============================================================================
// ERR-VOCAB-003 — command policy resolves to a `PolicyCategory` without
//                 `when_denied`, AND the feature has no `errors
//                 policy_denied` catch-all. Net result: built-in floor text.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinFallbackFinding {
    pub path: PathBuf,
    pub feature: String,
    pub command: String,
    pub policy: String,
    pub span: Option<SpanRef>,
}

impl BuiltinFallbackFinding {
    pub const CODE: &'static str = "ERR-VOCAB-003";

    pub fn message(&self) -> String {
        format!(
            "command `{}.{}` is gated by policy `{}` which has no `when_denied`, and feature `{}` \
             declares no `errors policy_denied message @translation.<key>` catch-all. On \
             `policy_denied`, the client will see the framework's built-in localized message \
             (good floor, but not domain-specific). Consider adding `when_denied \
             @translation.<key>` to the policy or a feature-level catch-all.",
            self.feature, self.command, self.policy, self.feature
        )
    }
}

/// Run ERR-VOCAB-003 over one feature. Per-command bypass: if the command
/// already authors its own `policy_when_denied`, the chain resolves at
/// step 1 and the warning is silent.
pub fn check_builtin_fallback(feature: &Feature, path: &Path) -> Vec<BuiltinFallbackFinding> {
    if has_policy_denied_catchall(feature.errors.as_ref()) {
        return Vec::new();
    }
    let mut findings = Vec::new();
    for cmd in &feature.commands {
        // Step 1: command supplies its own override -> chain resolves at
        // step 1 of proposal §2.E.
        if cmd.policy_when_denied.is_some() {
            continue;
        }
        // The command must actually be gated by a policy that could deny
        // — otherwise `policy_denied` never fires.
        if !command_has_denyable_policy(cmd) {
            continue;
        }
        let Some((policy_label, category)) = resolve_local_policy_category(cmd, &feature.policies)
        else {
            // Unresolved or external/atom-only policies leave the
            // chain at step 2 to the runtime built-in floor without
            // warning. `ERR-VOCAB-002` covers unresolved key refs; the
            // built-in fallback for atom policies is by-design.
            continue;
        };
        // Step 2: the resolved category authors its own `when_denied`.
        if category.when_denied.is_some() {
            continue;
        }
        findings.push(BuiltinFallbackFinding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            command: cmd.name.clone(),
            policy: policy_label,
            span: cmd.span_ref,
        });
    }
    findings
}

fn command_has_denyable_policy(cmd: &Command) -> bool {
    if !matches!(cmd.policy, PolicyRef::None) {
        return true;
    }
    cmd.policy_expr.as_ref().is_some_and(policy_expr_can_deny)
}

/// `policy_denied` can only fire when the expression has at least one
/// denyable branch — atom refs, has_role, has_permission, authenticated
/// (anonymous actor fails). Boolean combinators recurse.
fn policy_expr_can_deny(expr: &PolicyExpr) -> bool {
    match expr {
        PolicyExpr::Authenticated
        | PolicyExpr::HasRole(_)
        | PolicyExpr::HasPermission(_)
        | PolicyExpr::Atom(_) => true,
        PolicyExpr::And(parts) | PolicyExpr::Or(parts) => parts.iter().any(policy_expr_can_deny),
        PolicyExpr::Not(inner) => policy_expr_can_deny(inner),
    }
}

/// Resolves `cmd.policy` to a `PolicyCategory` within this feature.
///
/// Two surface forms map onto a local category:
///  * `PolicyRef::Local(name)`            — bare `policy update` form.
///  * `PolicyRef::Atom("policy.<name>")`  — `policy @policy.update` form;
///    the `@policy.<name>` namespace is canonical for naming local
///    policy categories from outside the `policies` block.
///
/// External atoms (`@role.*`, `@scope.*`, `@actor.*`) and unresolved /
/// external refs return `None` — the rule can't fire for them because
/// the resolution chain bypasses local categories entirely.
fn resolve_local_policy_category<'a>(
    cmd: &'a Command,
    policies: &'a lazuli_ir::Policies,
) -> Option<(String, &'a PolicyCategory)> {
    let candidate = match &cmd.policy {
        PolicyRef::Local(name) => Some(name.clone()),
        PolicyRef::Atom(atom) => atom
            .strip_prefix("policy.")
            .map(|name| name.to_owned()),
        _ => None,
    }?;
    policies
        .categories
        .iter()
        .find(|c| c.name == candidate)
        .map(|c| (format!("@policy.{}", candidate), c))
}

// =============================================================================
// ERR-VOCAB-CODE-UNKNOWN — `errors <code>` outside the closed catalog of 8.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeUnknownFinding {
    pub path: PathBuf,
    pub feature: String,
    pub code: String,
    pub span: Option<SpanRef>,
}

impl CodeUnknownFinding {
    pub const CODE: &'static str = "ERR-VOCAB-CODE-UNKNOWN";

    pub fn message(&self) -> String {
        format!(
            "error code `{}` is not in the framework catalog: `policy_denied`, \
             `validation_failed`, `tenant_mismatch`, `not_found`, `rate_limited`, `bad_request`, \
             `method_not_allowed`, `integration_error`, `unique_violation`, `foreign_key_violation`, \
             `not_null_violation`, `check_violation`. To register a new code, propose an \
             addition to `crates/lazuli/runtime/go/lazuli/error.go`.",
            self.code
        )
    }
}

pub fn check_code_unknown(feature: &Feature, path: &Path) -> Vec<CodeUnknownFinding> {
    let Some(errors) = &feature.errors else {
        return Vec::new();
    };
    let mut findings = Vec::new();
    for entry in &errors.messages {
        if !is_known_error_code(&entry.code) {
            findings.push(CodeUnknownFinding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                code: entry.code.clone(),
                span: span_of_message(entry),
            });
        }
    }
    findings
}

fn span_of_message(entry: &FeatureErrorMessage) -> Option<SpanRef> {
    entry.span_ref.or(entry.message.span_ref)
}

fn is_known_error_code(code: &str) -> bool {
    FRAMEWORK_ERROR_CODES.iter().any(|c| *c == code)
}

// =============================================================================
// ERR-VOCAB-EXPOSE-UNKNOWN — unknown field token in
//                            `expose client 4xx <fields>` or
//                            `expose client 5xx <fields>`.
//
// Promotes the pre-existing LSP-only shape check (`valid_error_exposure_line`)
// into a typed doctor diagnostic now that the IR carries
// `FeatureErrors.exposure_4xx` / `exposure_5xx` slots. The LSP keeps the
// inline editor shape check; doctor reports the same constraint with the
// IR-driven span.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExposeUnknownFinding {
    pub path: PathBuf,
    pub feature: String,
    /// `"4xx"` | `"5xx"` — which exposure axis carried the unknown field.
    pub axis: String,
    pub field: String,
    pub span: Option<SpanRef>,
}

impl ExposeUnknownFinding {
    pub const CODE: &'static str = "ERR-VOCAB-EXPOSE-UNKNOWN";

    pub fn message(&self) -> String {
        let allowed = match self.axis.as_str() {
            "4xx" => EXPOSE_4XX_FIELDS,
            "5xx" => EXPOSE_5XX_FIELDS,
            _ => &[] as &[&str],
        };
        let allowed_render = allowed
            .iter()
            .map(|f| format!("`{}`", f))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "`expose client {}` field `{}` must be one of: {}.",
            self.axis, self.field, allowed_render
        )
    }
}

pub fn check_expose_unknown(feature: &Feature, path: &Path) -> Vec<ExposeUnknownFinding> {
    let Some(errors) = &feature.errors else {
        return Vec::new();
    };
    let mut findings = Vec::new();
    for field in &errors.exposure_4xx {
        if !is_known_4xx_field(field) {
            findings.push(ExposeUnknownFinding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                axis: "4xx".to_owned(),
                field: field.clone(),
                span: errors.span_ref,
            });
        }
    }
    for field in &errors.exposure_5xx {
        // `message` on 5xx is rejected by ERR-VOCAB-EXPOSE-5XX-MESSAGE
        // (a separate, more specific rule). Suppress here so the same
        // line doesn't produce two diagnostics for the same authoring
        // mistake — the 5xx-message rule has the targeted prose.
        if field == "message" {
            continue;
        }
        if !is_known_5xx_field(field) {
            findings.push(ExposeUnknownFinding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                axis: "5xx".to_owned(),
                field: field.clone(),
                span: errors.span_ref,
            });
        }
    }
    findings
}

fn is_known_4xx_field(field: &str) -> bool {
    EXPOSE_4XX_FIELDS.iter().any(|f| *f == field)
}

fn is_known_5xx_field(field: &str) -> bool {
    EXPOSE_5XX_FIELDS.iter().any(|f| *f == field)
}

// =============================================================================
// ERR-VOCAB-WHEN-DENIED-NO-POLICY — `when_denied` authored where nothing
//                                   can deny. Override would never fire.
//
// Two sites:
//  1. Per-command `policy_when_denied: Some(_)` with `policy ==
//     PolicyRef::None` and `policy_expr == None` — proposal §6.6 strict
//     wording.
//  2. Per-policy `PolicyCategory.when_denied: Some(_)` with empty
//     `atoms` — the category declares no atoms so nothing can fail.
//     Mirrors the task's fixture intent (a `policies.X:` line that has
//     no policy expression on the right-hand side).
//
// Both forms are dead code; reporting them together keeps the rule's
// surface coherent.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhenDeniedNoPolicyFinding {
    pub path: PathBuf,
    pub feature: String,
    /// Authoring site for the dead `when_denied`. Two shapes are
    /// reachable:
    ///  * `Command(<name>)` — per-command `policy_when_denied`.
    ///  * `Policy(<name>)`  — per-policy `PolicyCategory.when_denied`.
    pub site: WhenDeniedSite,
    pub key: String,
    pub span: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhenDeniedSite {
    Command(String),
    Policy(String),
}

impl WhenDeniedNoPolicyFinding {
    pub const CODE: &'static str = "ERR-VOCAB-WHEN-DENIED-NO-POLICY";

    pub fn message(&self) -> String {
        match &self.site {
            WhenDeniedSite::Command(command) => format!(
                "command `{}.{}` declares `when_denied @translation.{}` but has no `policy`. \
                 Remove the `when_denied` line, or add a `policy` to gate the command.",
                self.feature, command, self.key
            ),
            WhenDeniedSite::Policy(policy) => format!(
                "policy `{}.{}` declares `when_denied @translation.{}` but its atom list is \
                 empty. The override is dead code — declare at least one atom \
                 (e.g. `{}: @role.admin`) or remove the `when_denied` line.",
                self.feature, policy, self.key, policy
            ),
        }
    }
}

pub fn check_when_denied_no_policy(
    feature: &Feature,
    path: &Path,
) -> Vec<WhenDeniedNoPolicyFinding> {
    let mut findings = Vec::new();
    // Per-command site.
    for cmd in &feature.commands {
        let Some(reference) = &cmd.policy_when_denied else {
            continue;
        };
        if matches!(cmd.policy, PolicyRef::None) && cmd.policy_expr.is_none() {
            findings.push(WhenDeniedNoPolicyFinding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                site: WhenDeniedSite::Command(cmd.name.clone()),
                key: reference.key.clone(),
                span: reference.span_ref.or(cmd.span_ref),
            });
        }
    }
    // Per-policy site.
    for category in &feature.policies.categories {
        let Some(reference) = &category.when_denied else {
            continue;
        };
        if category.atoms.is_empty() {
            findings.push(WhenDeniedNoPolicyFinding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                site: WhenDeniedSite::Policy(category.name.clone()),
                key: reference.key.clone(),
                span: reference.span_ref,
            });
        }
    }
    findings
}

// =============================================================================
// ERR-VOCAB-EXPOSE-5XX-MESSAGE — `expose client 5xx message`. Runtime
//                                force-hides regardless; analyzer flags
//                                the authoring mistake.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expose5xxMessageFinding {
    pub path: PathBuf,
    pub feature: String,
    pub span: Option<SpanRef>,
}

impl Expose5xxMessageFinding {
    pub const CODE: &'static str = "ERR-VOCAB-EXPOSE-5XX-MESSAGE";

    pub fn message(&self) -> String {
        format!(
            "`expose client 5xx message` (in feature `{}`) is rejected — 5xx errors are \
             framework-internal and their messages contain stack traces or implementation \
             details. Use `expose client 5xx code` (and optionally `data`) so the wire payload \
             stays safe; the server-side log still captures the full message for operators.",
            self.feature
        )
    }
}

pub fn check_expose_5xx_message(feature: &Feature, path: &Path) -> Vec<Expose5xxMessageFinding> {
    let Some(errors) = &feature.errors else {
        return Vec::new();
    };
    if errors.exposure_5xx.iter().any(|f| f == "message") {
        return vec![Expose5xxMessageFinding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            span: errors.span_ref,
        }];
    }
    Vec::new()
}

// =============================================================================
// Tests — exercise each rule positive + negative against synthetic IR.
//
// Fixture-driven coverage lives in `crates/lazuli_cli/tests/doctor_error_vocab.rs`
// (the dispatcher feeds each fixture through `DoctorPackage::diagnostics()`
// and asserts the expected code fires exactly once). These unit tests
// keep the rule shape pinned without depending on the doctor scaffolding.
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Command, CommandEffect, CommandInput, CommandKind, Defaults, Feature, FeatureErrors,
        FeatureErrorMessage, Policies, PolicyCategory, PolicyExpr, PolicyRef, ReturnsEffect,
        SpanRef, Translation, TranslationKey, TranslationKeyRef, TranslationVariant, TypeRef,
    };
    use std::collections::BTreeMap;
    use std::path::Path;

    fn span() -> SpanRef {
        SpanRef { start: 0, end: 1 }
    }

    fn key_ref(name: &str) -> TranslationKeyRef {
        TranslationKeyRef {
            key: name.to_owned(),
            span_ref: Some(span()),
        }
    }

    fn mk_translation(keys: &[&str]) -> Translation {
        Translation {
            catalog: "./i18n/test.<locale>.json".to_owned(),
            keys: keys
                .iter()
                .map(|name| TranslationKey {
                    name: (*name).to_owned(),
                    variants: vec![TranslationVariant {
                        locale: "pt-BR".to_owned(),
                        text: "x".to_owned(),
                    }],
                    plurals: vec![],
                })
                .collect(),
        }
    }

    fn mk_command(name: &str, policy: PolicyRef) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Returns(ReturnsEffect {
                return_type: TypeRef::Builtin(lazuli_ir::BuiltinType::Boolean),
            }),
            policy,
            policy_expr: None,
            policy_when_denied: None,
            emits: vec![],
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            owner_scope_sql: None,
            previous_names: vec![],
            span_ref: Some(span()),
        }
    }

    fn mk_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: vec![],
            uses_versions: vec![],
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
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
            span_ref: Some(span()),
        }
    }

    fn declared_policies(categories: Vec<PolicyCategory>) -> Policies {
        Policies {
            categories,
            fields: vec![],
            span_ref: Some(span()),
        }
    }

    fn p_cat(name: &str, atoms: &[&str], when_denied: Option<&str>) -> PolicyCategory {
        PolicyCategory {
            name: name.to_owned(),
            atoms: atoms.iter().map(|a| (*a).to_owned()).collect(),
            previous_names: vec![],
            when_denied: when_denied.map(key_ref),
            when_denied_route: None,
        }
    }

    fn path() -> &'static Path {
        Path::new("features/test.lzi")
    }

    fn empty_keys_index() -> BTreeMap<String, BTreeSet<String>> {
        BTreeMap::new()
    }

    // ── ERR-VOCAB-001 ─────────────────────────────────────────────────────

    #[test]
    fn err_vocab_001_fires_when_policies_block_lacks_when_denied() {
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat("update", &["@role.admin"], None)]);
        let findings = check_policies_no_when_denied(&f, path());
        assert_eq!(findings.len(), 1);
        assert_eq!(PoliciesNoWhenDeniedFinding::CODE, "ERR-VOCAB-001");
    }

    #[test]
    fn err_vocab_001_silent_when_when_denied_present() {
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat(
            "update",
            &["@role.admin"],
            Some("admin_only"),
        )]);
        f.translation = Some(mk_translation(&["admin_only"]));
        assert!(check_policies_no_when_denied(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_001_silent_when_errors_catchall_present() {
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat("update", &["@role.admin"], None)]);
        f.errors = Some(FeatureErrors {
            messages: vec![FeatureErrorMessage {
                code: "policy_denied".to_owned(),
                message: key_ref("signin_required"),
                span_ref: Some(span()),
            }],
            ..Default::default()
        });
        f.translation = Some(mk_translation(&["signin_required"]));
        assert!(check_policies_no_when_denied(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_001_silent_when_policies_block_absent() {
        // `Policies::default()` carries `span_ref: None` — feature has no
        // `policies` block at all, so the rule cannot fire.
        let f = mk_feature("customer");
        assert!(check_policies_no_when_denied(&f, path()).is_empty());
    }

    // ── ERR-VOCAB-002 ─────────────────────────────────────────────────────

    #[test]
    fn err_vocab_002_fires_when_command_when_denied_unknown() {
        let mut f = mk_feature("customer");
        let mut cmd = mk_command("update", PolicyRef::Local("update".to_owned()));
        cmd.policy_when_denied = Some(key_ref("ghost_key"));
        f.commands = vec![cmd];
        f.translation = Some(mk_translation(&["other_key"]));
        let findings = check_translation_key_unknown(&f, path(), &empty_keys_index());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].key, "ghost_key");
        assert_eq!(KeyUnknownFinding::CODE, "ERR-VOCAB-002");
    }

    #[test]
    fn err_vocab_002_silent_when_key_resolves_locally() {
        let mut f = mk_feature("customer");
        let mut cmd = mk_command("update", PolicyRef::Local("update".to_owned()));
        cmd.policy_when_denied = Some(key_ref("admin_only"));
        f.commands = vec![cmd];
        f.translation = Some(mk_translation(&["admin_only"]));
        assert!(check_translation_key_unknown(&f, path(), &empty_keys_index()).is_empty());
    }

    #[test]
    fn err_vocab_002_silent_when_key_resolves_through_uses() {
        let mut f = mk_feature("sales");
        f.uses = vec!["crm".to_owned()];
        f.policies = declared_policies(vec![p_cat(
            "create",
            &["@role.sales"],
            Some("shared_key"),
        )]);
        let mut index: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut crm_keys = BTreeSet::new();
        crm_keys.insert("shared_key".to_owned());
        index.insert("crm".to_owned(), crm_keys);
        assert!(check_translation_key_unknown(&f, path(), &index).is_empty());
    }

    #[test]
    fn err_vocab_002_fires_for_unknown_errors_block_key() {
        let mut f = mk_feature("customer");
        f.errors = Some(FeatureErrors {
            messages: vec![FeatureErrorMessage {
                code: "policy_denied".to_owned(),
                message: key_ref("missing_key"),
                span_ref: Some(span()),
            }],
            ..Default::default()
        });
        let findings = check_translation_key_unknown(&f, path(), &empty_keys_index());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].key, "missing_key");
    }

    // ── ERR-VOCAB-003 ─────────────────────────────────────────────────────

    #[test]
    fn err_vocab_003_fires_when_policy_has_no_when_denied_and_no_catchall() {
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat("update", &["@role.admin"], None)]);
        f.commands = vec![mk_command("update", PolicyRef::Local("update".to_owned()))];
        let findings = check_builtin_fallback(&f, path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].command, "update");
        assert_eq!(BuiltinFallbackFinding::CODE, "ERR-VOCAB-003");
    }

    #[test]
    fn err_vocab_003_silent_when_command_overrides_when_denied() {
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat("update", &["@role.admin"], None)]);
        let mut cmd = mk_command("update", PolicyRef::Local("update".to_owned()));
        cmd.policy_when_denied = Some(key_ref("admin_only"));
        f.commands = vec![cmd];
        f.translation = Some(mk_translation(&["admin_only"]));
        assert!(check_builtin_fallback(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_003_silent_when_feature_catchall_present() {
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat("update", &["@role.admin"], None)]);
        f.commands = vec![mk_command("update", PolicyRef::Local("update".to_owned()))];
        f.errors = Some(FeatureErrors {
            messages: vec![FeatureErrorMessage {
                code: "policy_denied".to_owned(),
                message: key_ref("signin_required"),
                span_ref: Some(span()),
            }],
            ..Default::default()
        });
        f.translation = Some(mk_translation(&["signin_required"]));
        assert!(check_builtin_fallback(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_003_silent_when_command_has_no_policy() {
        // A command without `policy` cannot trigger `policy_denied`,
        // so the built-in fallback warning does not apply.
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat("update", &["@role.admin"], None)]);
        f.commands = vec![mk_command("update", PolicyRef::None)];
        assert!(check_builtin_fallback(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_003_silent_when_policy_authors_when_denied() {
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat(
            "update",
            &["@role.admin"],
            Some("admin_only"),
        )]);
        f.commands = vec![mk_command("update", PolicyRef::Local("update".to_owned()))];
        f.translation = Some(mk_translation(&["admin_only"]));
        assert!(check_builtin_fallback(&f, path()).is_empty());
    }

    // ── ERR-VOCAB-CODE-UNKNOWN ────────────────────────────────────────────

    #[test]
    fn err_vocab_code_unknown_fires() {
        let mut f = mk_feature("customer");
        f.errors = Some(FeatureErrors {
            messages: vec![FeatureErrorMessage {
                code: "blah_error".to_owned(),
                message: key_ref("k"),
                span_ref: Some(span()),
            }],
            ..Default::default()
        });
        let findings = check_code_unknown(&f, path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "blah_error");
        assert_eq!(CodeUnknownFinding::CODE, "ERR-VOCAB-CODE-UNKNOWN");
    }

    #[test]
    fn err_vocab_code_unknown_silent_for_all_known_codes() {
        let mut f = mk_feature("customer");
        f.errors = Some(FeatureErrors {
            messages: FRAMEWORK_ERROR_CODES
                .iter()
                .map(|code| FeatureErrorMessage {
                    code: (*code).to_owned(),
                    message: key_ref("k"),
                    span_ref: Some(span()),
                })
                .collect(),
            ..Default::default()
        });
        assert!(check_code_unknown(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_code_unknown_silent_for_db_integrity_codes() {
        // DB-INTEGRITY-CATALOG-EXT regression guard: the 4 new
        // db-integrity codes must parse cleanly in `errors` blocks so
        // hostpoint can author `errors unique_violation message
        // @translation.account_email_already_registered`.
        let mut f = mk_feature("account");
        f.errors = Some(FeatureErrors {
            messages: ["unique_violation", "foreign_key_violation", "not_null_violation", "check_violation"]
                .iter()
                .map(|code| FeatureErrorMessage {
                    code: (*code).to_owned(),
                    message: key_ref("k"),
                    span_ref: Some(span()),
                })
                .collect(),
            ..Default::default()
        });
        assert!(check_code_unknown(&f, path()).is_empty());
    }

    // ── ERR-VOCAB-EXPOSE-UNKNOWN ──────────────────────────────────────────

    #[test]
    fn err_vocab_expose_unknown_fires_for_4xx_unknown_field() {
        let mut f = mk_feature("customer");
        f.errors = Some(FeatureErrors {
            exposure_4xx: vec!["message".to_owned(), "schmessage".to_owned()],
            ..Default::default()
        });
        let findings = check_expose_unknown(&f, path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "schmessage");
        assert_eq!(findings[0].axis, "4xx");
        assert_eq!(ExposeUnknownFinding::CODE, "ERR-VOCAB-EXPOSE-UNKNOWN");
    }

    #[test]
    fn err_vocab_expose_unknown_does_not_double_count_5xx_message() {
        let mut f = mk_feature("customer");
        f.errors = Some(FeatureErrors {
            exposure_5xx: vec!["message".to_owned()],
            ..Default::default()
        });
        // `message` is delegated to ERR-VOCAB-EXPOSE-5XX-MESSAGE — this
        // rule must stay silent so the same authoring mistake produces
        // exactly one diagnostic.
        assert!(check_expose_unknown(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_expose_unknown_silent_for_all_known_fields() {
        let mut f = mk_feature("customer");
        f.errors = Some(FeatureErrors {
            exposure_4xx: EXPOSE_4XX_FIELDS.iter().map(|s| (*s).to_owned()).collect(),
            exposure_5xx: EXPOSE_5XX_FIELDS.iter().map(|s| (*s).to_owned()).collect(),
            ..Default::default()
        });
        assert!(check_expose_unknown(&f, path()).is_empty());
    }

    // ── ERR-VOCAB-WHEN-DENIED-NO-POLICY ───────────────────────────────────

    #[test]
    fn err_vocab_when_denied_no_policy_fires_for_command_site() {
        let mut f = mk_feature("customer");
        let mut cmd = mk_command("ping", PolicyRef::None);
        cmd.policy_when_denied = Some(key_ref("blocked"));
        f.commands = vec![cmd];
        let findings = check_when_denied_no_policy(&f, path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].site, WhenDeniedSite::Command("ping".to_owned()));
        assert_eq!(
            WhenDeniedNoPolicyFinding::CODE,
            "ERR-VOCAB-WHEN-DENIED-NO-POLICY"
        );
    }

    #[test]
    fn err_vocab_when_denied_no_policy_silent_when_policy_present() {
        let mut f = mk_feature("customer");
        let mut cmd = mk_command("ping", PolicyRef::Local("update".to_owned()));
        cmd.policy_when_denied = Some(key_ref("blocked"));
        f.commands = vec![cmd];
        assert!(check_when_denied_no_policy(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_when_denied_no_policy_silent_when_policy_expr_present() {
        let mut f = mk_feature("customer");
        let mut cmd = mk_command("ping", PolicyRef::None);
        cmd.policy_expr = Some(PolicyExpr::Authenticated);
        cmd.policy_when_denied = Some(key_ref("blocked"));
        f.commands = vec![cmd];
        assert!(check_when_denied_no_policy(&f, path()).is_empty());
    }

    #[test]
    fn err_vocab_when_denied_no_policy_fires_for_empty_policy_category() {
        // `policies.update:` with no atoms but a `when_denied` child —
        // the override is dead code because the category gates nothing.
        let mut f = mk_feature("customer");
        let mut cat = p_cat("update", &[], Some("blocked"));
        // Sanity: helper builds atoms from the slice; empty here.
        cat.atoms.clear();
        f.policies = declared_policies(vec![cat]);
        let findings = check_when_denied_no_policy(&f, path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].site, WhenDeniedSite::Policy("update".to_owned()));
    }

    #[test]
    fn err_vocab_when_denied_no_policy_silent_when_policy_category_has_atoms() {
        let mut f = mk_feature("customer");
        f.policies = declared_policies(vec![p_cat(
            "update",
            &["@role.admin"],
            Some("blocked"),
        )]);
        assert!(check_when_denied_no_policy(&f, path()).is_empty());
    }

    // ── ERR-VOCAB-EXPOSE-5XX-MESSAGE ──────────────────────────────────────

    #[test]
    fn err_vocab_expose_5xx_message_fires() {
        let mut f = mk_feature("customer");
        f.errors = Some(FeatureErrors {
            exposure_5xx: vec!["code".to_owned(), "message".to_owned()],
            ..Default::default()
        });
        let findings = check_expose_5xx_message(&f, path());
        assert_eq!(findings.len(), 1);
        assert_eq!(Expose5xxMessageFinding::CODE, "ERR-VOCAB-EXPOSE-5XX-MESSAGE");
    }

    #[test]
    fn err_vocab_expose_5xx_message_silent_when_message_absent() {
        let mut f = mk_feature("customer");
        f.errors = Some(FeatureErrors {
            exposure_5xx: vec!["code".to_owned(), "data".to_owned()],
            ..Default::default()
        });
        assert!(check_expose_5xx_message(&f, path()).is_empty());
    }
}
