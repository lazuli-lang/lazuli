//! Cell CODEGEN-1 — IR Error-Vocab emitter. Walks the new
//! `Command.policy_when_denied`, `PolicyCategory.when_denied`, and
//! `Feature.errors: Option<FeatureErrors>` slots and emits the three
//! sites required by the four-layer message resolution chain
//! (proposal §2.E):
//!
//! - **(a) Per-operation `ErrorKeys` literal** — a `var <op>ErrorKeys =
//!   lazuli.ErrorKeys{ ... }` block. Emitted when the command/query has
//!   an effective `policy_denied` override (`policy_when_denied` or
//!   `PolicyCategory.when_denied`). Composes inline into the existing
//!   `command.gen.go` body so the command literal can reference
//!   `&<cmd>ErrorKeys` via a new `ErrorKeys` kv row. Codegen emits
//!   zero LOC when the command has no override (§4.4 LOC budget).
//!
//! - **(b) Per-feature `errors.gen.go`** — one file per feature with
//!   an `errors` block. Carries the lowered `FeatureErrorContract`
//!   shape: default exposure, 4xx/5xx exposure lists, and the per-code
//!   message overrides (`policy_denied → @translation.<key>`).
//!   Skipped when `Feature.errors.is_none()`.
//!
//! - **(c) App-level `app/error_resolution.gen.go`** — registers every
//!   feature's `FeatureErrors` with the runtime via
//!   `lazuli.RegisterFeatureErrors("<feature>", <feature>gen.FeatureErrors)`.
//!   Skipped when the module has zero features with `errors` blocks.
//!
//! The runtime types referenced here (`lazuli.ErrorKeys`,
//! `lazuli.FeatureErrorContract`, `lazuli.ErrorExposureDefault`,
//! `lazuli.RegisterFeatureErrors`) ship via Cell RUNTIME-1 — see
//! proposal §5. Their shapes are documented at the top of each
//! emit_* function so RUNTIME-1's owner has a contract reference.
//!
//! Proposal references:
//! - §4.1.1 — Per-command `ErrorKeys` literal shape.
//! - §4.1.2 — Per-feature `FeatureErrors` shape.
//! - §4.1.3 — App-level `ErrorResolverRegistry` shape.
//! - §5 — Runtime contract types (Cell RUNTIME-1).
//!
//! Determinism: maps are walked in sorted key order via `BTreeMap` to
//! keep generated output byte-equivalent across runs.

use std::collections::BTreeMap;

use lazuli_ir::{
    Command, ErrorExposureDefault, Feature, FeatureErrorMessage, FeatureErrors, Module,
    Policies, PolicyRef, TranslationKeyRef,
};

use super::casing::gen_package_name;
use super::command::{command_var_name, effect_resource_pascal};
use super::imports::ImportSet;
use super::patterns::{PATTERN_ERROR_RESOLVER, emit_pattern_header};
use super::printer::GoPrinter;
use crate::GeneratedFile;

/// File path for the app-level error resolver registry. Sits at the
/// generated module root under `app/` alongside other top-level files.
pub const APP_ERROR_RESOLUTION_PATH: &str = "app/error_resolution.gen.go";

/// Lowered identifier used for the per-command `ErrorKeys` literal. The
/// command-emitter consults this helper so the `lazuli.Command[...]`
/// value can reference the same name — single source of truth, no
/// drift.
pub fn command_error_keys_var(command: &Command) -> String {
    let resource_pascal = effect_resource_pascal(&command.effect);
    let base = command_var_name(&command.name, &resource_pascal);
    format!("{base}ErrorKeys")
}

/// `true` when the command should emit a per-command `ErrorKeys`
/// literal. Direct per-command `policy_when_denied` wins; otherwise a
/// named policy category's `when_denied` becomes the operation-level
/// runtime key.
pub fn command_has_error_keys(command: &Command, policies: Option<&Policies>) -> bool {
    command_policy_denied_key(command, policies).is_some()
}

/// Effective `policy_denied` message key for a command.
pub fn command_policy_denied_key<'a>(
    command: &'a Command,
    policies: Option<&'a Policies>,
) -> Option<&'a TranslationKeyRef> {
    policy_denied_key_for_policy(command.policy_when_denied.as_ref(), &command.policy, policies)
}

pub fn policy_denied_key_for_policy<'a>(
    operation_override: Option<&'a TranslationKeyRef>,
    policy: &'a PolicyRef,
    policies: Option<&'a Policies>,
) -> Option<&'a TranslationKeyRef> {
    if operation_override.is_some() {
        return operation_override;
    }
    let policy_name = local_policy_name(policy)?;
    policies?
        .categories
        .iter()
        .find(|category| category.name == policy_name)
        .and_then(|category| category.when_denied.as_ref())
}

fn local_policy_name(policy: &PolicyRef) -> Option<&str> {
    match policy {
        PolicyRef::Local(name) => Some(name.as_str()),
        PolicyRef::Atom(atom) => {
            let stripped = atom.strip_prefix('@').unwrap_or(atom);
            stripped.strip_prefix("policy.")
        }
        PolicyRef::Unresolved(raw) => {
            let stripped = raw.strip_prefix('@').unwrap_or(raw);
            stripped.strip_prefix("policy.")
        }
        PolicyRef::External { .. } | PolicyRef::None => None,
    }
}

/// Emit the per-command `var <cmd>ErrorKeys = lazuli.ErrorKeys{ ... }`
/// literal into the current printer. Called by `command.rs` immediately
/// after the `lazuli.Command[I, O]{...}` block lands, so the two values
/// sit adjacent in the generated file.
///
/// Runtime contract (Cell RUNTIME-1):
///   type MessageRef struct { Feature, Key string }
///   type ErrorKeys struct {
///       PolicyDenied      MessageRef
///       ValidationFailed  MessageRef
///       TenantMismatch    MessageRef
///       NotFound          MessageRef
///       RateLimited       MessageRef
///       BadRequest        MessageRef
///       MethodNotAllowed  MessageRef
///       IntegrationError  MessageRef
///   }
///
/// Each field is a typed `@translation.<key>` reference (carried in IR
/// as a `TranslationKeyRef`). Zero-valued (empty) `MessageRef` means
/// "fall through to the per-feature / built-in catalog" — the runtime
/// resolver handles the chain (proposal §2.E).
pub fn emit_command_error_keys(
    p: &mut GoPrinter,
    command: &Command,
    feature_name: &str,
    policies: Option<&Policies>,
) {
    let Some(key_ref) = command_policy_denied_key(command, policies) else {
        return;
    };
    let var_name = command_error_keys_var(command);

    p.line(&format!(
        "// Per-command error-key overrides for `{}`. Resolution-chain step 1",
        command.name
    ));
    p.line("// (proposal §2.E step 1): the runtime resolver consults this struct");
    p.line("// before falling through to PolicyCategory.when_denied or");
    p.line("// FeatureErrors.messages.");
    emit_pattern_header(p, PATTERN_ERROR_RESOLVER);
    p.line(&format!("var {var_name} = lazuli.ErrorKeys{{"));
    p.indent();
    p.line(&format!(
        "PolicyDenied: {},",
        message_ref_literal(feature_name, key_ref)
    ));
    p.dedent();
    p.line("}");
}

pub fn emit_operation_error_keys(
    p: &mut GoPrinter,
    var_name: &str,
    operation_label: &str,
    feature_name: &str,
    key_ref: &TranslationKeyRef,
) {
    p.line(&format!(
        "// Error-key overrides for `{operation_label}`. Resolution-chain step 1."
    ));
    emit_pattern_header(p, PATTERN_ERROR_RESOLVER);
    p.line(&format!("var {var_name} = lazuli.ErrorKeys{{"));
    p.indent();
    p.line(&format!(
        "PolicyDenied: {},",
        message_ref_literal(feature_name, key_ref)
    ));
    p.dedent();
    p.line("}");
}

/// Emit `<feature>/errors.gen.go` for a feature that declares an
/// `errors` block. Returns `None` when the feature has no
/// customization so the orchestrator can skip the file entirely
/// (mirrors the resource / enum skip rule — output listing stays
/// signal-rich).
///
/// Runtime contract (Cell RUNTIME-1):
///   type FeatureErrorContract struct {
///       Default          ErrorExposureDefault
///       ExposeClient4xx  []string
///       ExposeClient5xx  []string
///       Messages         map[string]MessageRef   // code -> @translation.<key>
///       FieldMessages    map[string]MessageRef   // "<resource>.<field>.<code>" -> key
///   }
///   type ErrorExposureDefault uint8
///   const ( ExposureHide ErrorExposureDefault = iota; ExposureExpose )
pub fn emit_feature_errors_file(source_label: &str, feature: &Feature) -> Option<String> {
    let errors = feature.errors.as_ref()?;

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    imports.add("lazuli.dev/runtime/lazuli");
    if !errors.messages.is_empty() || !errors.field_messages.is_empty() {
        imports.add("lazuli.dev/runtime/lazuli/i18n");
    }

    p.banner(source_label, &gen_package_name(&feature.name));
    imports.emit(&mut p);
    p.blank();

    p.line("// FeatureErrors is the lowered `errors` block for this feature.");
    p.line("// Carries both the exposure rules (`default hide/expose`, `expose");
    p.line("// client 4xx/5xx <fields>`) and the per-code typed message overrides");
    p.line("// (`<code> message @translation.<key>`). Resolution-chain step 3");
    p.line("// (proposal §2.E step 3): consulted when no per-command or per-policy");
    p.line("// override fires for the active error code.");
    emit_pattern_header(&mut p, PATTERN_ERROR_RESOLVER);
    p.line("var FeatureErrors = lazuli.FeatureErrorContract{");
    p.indent();
    emit_feature_errors_body(&mut p, errors, &feature.name);
    p.dedent();
    p.line("}");

    Some(p.finish())
}

/// Emit the body of the `lazuli.FeatureErrorContract{ ... }` literal.
/// Kept separate so a future inspect projection can reuse the same
/// field-order discipline. Field order matches §4.1.2.
fn emit_feature_errors_body(p: &mut GoPrinter, errors: &FeatureErrors, feature_name: &str) {
    if let Some(default) = errors.default {
        p.line(&format!(
            "Default:         {},",
            exposure_default_const(default)
        ));
    }
    if !errors.exposure_4xx.is_empty() {
        p.line(&format!(
            "ExposeClient4xx: []string{{{}}},",
            format_string_slice(&errors.exposure_4xx)
        ));
    }
    if !errors.exposure_5xx.is_empty() {
        p.line(&format!(
            "ExposeClient5xx: []string{{{}}},",
            format_string_slice(&errors.exposure_5xx)
        ));
    }
    if !errors.messages.is_empty() {
        // Walk messages in sorted code order so the output is
        // deterministic regardless of authoring order.
        let mut by_code: BTreeMap<&str, &FeatureErrorMessage> = BTreeMap::new();
        for msg in &errors.messages {
            by_code.insert(msg.code.as_str(), msg);
        }
        p.line("Messages: map[string]i18n.MessageRef{");
        p.indent();
        for (code, msg) in &by_code {
            p.line(&format!(
                "{:?}: {},",
                code,
                message_ref_literal(feature_name, &msg.message)
            ));
        }
        p.dedent();
        p.line("},");
    }
}

/// Emit the app-level `app/error_resolution.gen.go` when at least one
/// feature in the module declares an `errors` block. The file ships an
/// `init()` that walks each feature's `FeatureErrors` and registers it
/// with the runtime registry so the resolver chain has a populated
/// per-feature map at boot. Returns `None` when no feature has an
/// `errors` block — keeps the output listing signal-rich.
///
/// Runtime contract (Cell RUNTIME-1):
///   func RegisterFeatureErrors(feature string, contract FeatureErrorContract)
///
/// The registry merges with the built-in PT-BR/en-US catalog
/// (`runtime/go/lazuli/i18n/builtin.<locale>.json`, Cell RUNTIME-2)
/// before serving requests.
pub fn emit_app_error_resolution(
    source_label: &str,
    module: &Module,
    module_name: &str,
) -> Option<String> {
    // Walk features in sorted name order so the generated import block
    // and `init()` body are byte-equivalent across runs regardless of
    // IR `Vec` insertion order.
    let mut features_with_errors: BTreeMap<&str, &Feature> = BTreeMap::new();
    for feature in &module.features {
        if feature.errors.is_some() {
            features_with_errors.insert(feature.name.as_str(), feature);
        }
    }
    if features_with_errors.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    imports.add("lazuli.dev/runtime/lazuli");
    for feature_name in features_with_errors.keys() {
        imports.add(&format!("{module_name}/{feature_name}"));
    }

    p.banner(source_label, "app");
    imports.emit(&mut p);
    p.blank();

    p.line("// ErrorResolverRegistry binds every feature's lowered FeatureErrors");
    p.line("// contract into the runtime resolver chain (proposal §4.1.3). The");
    p.line("// init() below registers each contract at boot; the resolver merges");
    p.line("// the registry with the built-in PT-BR/en-US catalog shipped by the");
    p.line("// runtime (Cell RUNTIME-2) before serving requests.");
    emit_pattern_header(&mut p, PATTERN_ERROR_RESOLVER);
    p.line("func init() {");
    p.indent();
    for feature_name in features_with_errors.keys() {
        let pkg = gen_package_name(feature_name);
        p.line(&format!(
            "lazuli.RegisterFeatureErrors({:?}, {pkg}.FeatureErrors)",
            feature_name
        ));
    }
    p.dedent();
    p.line("}");

    Some(p.finish())
}

/// Convenience for `emit_module` to fold both file emissions through a
/// single call site. Returns a `Vec` so the orchestrator can extend its
/// `files` vector verbatim. Includes per-feature `errors.gen.go` files
/// AND the app-level `error_resolution.gen.go` when applicable.
pub fn emit_error_vocab_files(
    source_label: &str,
    module: &Module,
    module_name: &str,
) -> Vec<GeneratedFile> {
    let mut out = Vec::new();
    // Per-feature `errors.gen.go` — sorted by feature name so the file
    // listing is deterministic.
    let mut features: BTreeMap<&str, &Feature> = BTreeMap::new();
    for feature in &module.features {
        features.insert(feature.name.as_str(), feature);
    }
    for (feature_name, feature) in &features {
        if let Some(contents) = emit_feature_errors_file(source_label, feature) {
            out.push(GeneratedFile {
                path: format!("{feature_name}/errors.gen.go"),
                contents,
            });
        }
    }
    // App-level registry — single file at the module root.
    if let Some(contents) = emit_app_error_resolution(source_label, module, module_name) {
        out.push(GeneratedFile {
            path: APP_ERROR_RESOLUTION_PATH.to_owned(),
            contents,
        });
    }
    out
}

/// Format a `TranslationKeyRef` as the literal string the runtime
/// resolver looks up. v1 IR carries only the bare key name (no
/// feature-qualified form yet — see `TranslationKeyRef` in
/// `lazuli_ir/src/lib.rs` §3.1 of the proposal); the runtime
/// fully-qualifies at lookup time using the source-tag feature.
fn translation_key_literal(key_ref: &TranslationKeyRef) -> &str {
    key_ref.key.as_str()
}

/// Format a `TranslationKeyRef` as the typed `i18n.MessageRef{Feature,
/// Key}` Go literal the runtime expects (proposal §5.1). The owning
/// feature is supplied by the caller since v1 IR carries only the bare
/// key name on `TranslationKeyRef`; cross-feature consumption (proposal
/// §2.B) is resolved at codegen time against the policy's owning
/// feature, not the consumer's.
fn message_ref_literal(feature_name: &str, key_ref: &TranslationKeyRef) -> String {
    format!(
        "i18n.MessageRef{{Feature: {:?}, Key: {:?}}}",
        feature_name,
        translation_key_literal(key_ref)
    )
}

fn exposure_default_const(default: ErrorExposureDefault) -> &'static str {
    match default {
        ErrorExposureDefault::Hide => "lazuli.ExposureHide",
        ErrorExposureDefault::Expose => "lazuli.ExposureExpose",
    }
}

fn format_string_slice(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("{value:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        CommandEffect, CommandInput, CommandKind, Defaults, Feature, FeatureErrorMessage,
        FeatureErrors, Module, Policies, PolicyCategory, PolicyRef, TranslationKeyRef,
    };

    fn empty_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies {
                categories: Vec::new(),
                fields: Vec::new(),
                span_ref: None,
            },
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: Vec::new(),
            span_ref: None,
            synth_origins: std::collections::BTreeMap::new(),
        }
    }

    fn empty_command(name: &str) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind: CommandKind::Create,
            route: Vec::new(),
            input: CommandInput::Empty,
            target: None,
            lets: Vec::new(),
            effect: CommandEffect::None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: Vec::new(),
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: Vec::new(),
            external_calls: Vec::new(),
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn module_with(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features,
        }
    }

    #[test]
    fn command_without_override_emits_nothing() {
        let command = empty_command("create");
        assert!(!command_has_error_keys(&command, None));
        let mut p = GoPrinter::new();
        emit_command_error_keys(&mut p, &command, "account", None);
        assert_eq!(p.finish(), String::new());
    }

    #[test]
    fn command_with_override_emits_error_keys_var() {
        let mut command = empty_command("choose_role");
        command.policy_when_denied = Some(TranslationKeyRef {
            key: "choose_role_signin_required".to_owned(),
            span_ref: None,
        });
        assert!(command_has_error_keys(&command, None));
        let mut p = GoPrinter::new();
        emit_command_error_keys(&mut p, &command, "account", None);
        let out = p.finish();
        assert!(out.contains("//lazuli:pattern error_resolver v1"));
        // `command_var_name` interleaves the effect-pinned resource pascal
        // (`Result` when `CommandEffect::None`) between the verb and the
        // modifier words — `choose_role` + `Result` → `chooseResultRole`.
        // Reusing that helper keeps the ErrorKeys var name in lock-step
        // with the `lazuli.Command[I, O]` var name in command.gen.go
        // (single source of truth, no drift).
        let expected_var = command_error_keys_var(&command);
        assert_eq!(expected_var, "chooseResultRoleErrorKeys");
        assert!(out.contains(&format!("var {expected_var} = lazuli.ErrorKeys{{")));
        assert!(out.contains(
            "PolicyDenied: i18n.MessageRef{Feature: \"account\", Key: \"choose_role_signin_required\"},"
        ));
    }

    #[test]
    fn command_with_policy_category_when_denied_emits_error_keys_var() {
        let mut command = empty_command("account_me");
        command.policy = PolicyRef::Local("authenticated".to_owned());
        let policies = Policies {
            categories: vec![PolicyCategory {
                name: "authenticated".to_owned(),
                atoms: vec!["@scope.authenticated".to_owned()],
                previous_names: Vec::new(),
                when_denied: Some(TranslationKeyRef {
                    key: "account_signin".to_owned(),
                    span_ref: None,
                }),
            }],
            fields: Vec::new(),
            span_ref: None,
        };

        assert!(command_has_error_keys(&command, Some(&policies)));
        let mut p = GoPrinter::new();
        emit_command_error_keys(&mut p, &command, "account", Some(&policies));
        let out = p.finish();
        assert!(out.contains(
            "PolicyDenied: i18n.MessageRef{Feature: \"account\", Key: \"account_signin\"},"
        ));
    }

    #[test]
    fn feature_without_errors_block_skips_file() {
        let feature = empty_feature("account");
        assert!(emit_feature_errors_file("lazuli/test-app", &feature).is_none());
    }

    #[test]
    fn feature_with_errors_block_emits_contract_value() {
        let mut feature = empty_feature("account");
        feature.errors = Some(FeatureErrors {
            default: Some(ErrorExposureDefault::Hide),
            exposure_4xx: vec!["message".to_owned(), "code".to_owned()],
            exposure_5xx: vec!["code".to_owned()],
            messages: vec![
                FeatureErrorMessage {
                    code: "policy_denied".to_owned(),
                    message: TranslationKeyRef {
                        key: "account_signin_required".to_owned(),
                        span_ref: None,
                    },
                    span_ref: None,
                },
                FeatureErrorMessage {
                    code: "tenant_mismatch".to_owned(),
                    message: TranslationKeyRef {
                        key: "account_wrong_workspace".to_owned(),
                        span_ref: None,
                    },
                    span_ref: None,
                },
            ],
            field_messages: Vec::new(),
            audience_exposure: Vec::new(),
            redact_patterns: Vec::new(),
            span_ref: None,
        });
        let out = emit_feature_errors_file("lazuli/test-app", &feature)
            .expect("feature with errors block must emit");

        assert!(out.contains("\npackage accountgen\n"));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli\""));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/i18n\""));
        assert!(out.contains("//lazuli:pattern error_resolver v1"));
        assert!(out.contains("var FeatureErrors = lazuli.FeatureErrorContract{"));
        assert!(out.contains("Default:         lazuli.ExposureHide,"));
        assert!(out.contains("ExposeClient4xx: []string{\"message\", \"code\"},"));
        assert!(out.contains("ExposeClient5xx: []string{\"code\"},"));
        // BTreeMap sorts codes: `policy_denied` < `tenant_mismatch`.
        let policy = out
            .find("\"policy_denied\":")
            .expect("policy_denied present");
        let tenant = out
            .find("\"tenant_mismatch\":")
            .expect("tenant_mismatch present");
        assert!(
            policy < tenant,
            "messages must be sorted by code:\n{out}"
        );
        assert!(out.contains(
            "\"policy_denied\": i18n.MessageRef{Feature: \"account\", Key: \"account_signin_required\"},"
        ));
        assert!(out.contains(
            "\"tenant_mismatch\": i18n.MessageRef{Feature: \"account\", Key: \"account_wrong_workspace\"},"
        ));
    }

    #[test]
    fn app_resolver_skipped_when_no_feature_has_errors() {
        let module = module_with(vec![empty_feature("account")]);
        assert!(
            emit_app_error_resolution("lazuli/test-app", &module, "lazuli/test-app").is_none()
        );
    }

    #[test]
    fn app_resolver_registers_each_feature_with_errors() {
        let mut account = empty_feature("account");
        account.errors = Some(FeatureErrors {
            default: Some(ErrorExposureDefault::Hide),
            exposure_4xx: Vec::new(),
            exposure_5xx: Vec::new(),
            messages: Vec::new(),
            field_messages: Vec::new(),
            audience_exposure: Vec::new(),
            redact_patterns: Vec::new(),
            span_ref: None,
        });
        let mut billing = empty_feature("billing");
        billing.errors = Some(FeatureErrors::default());
        // Insert in reverse alphabetical order so we can prove sorting.
        let module = module_with(vec![billing, account]);
        let out = emit_app_error_resolution("lazuli/test-app", &module, "lazuli/test-app")
            .expect("must emit when at least one feature has errors");

        assert!(out.contains("\npackage app\n"));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli\""));
        assert!(out.contains("\"lazuli/test-app/account\""));
        assert!(out.contains("\"lazuli/test-app/billing\""));
        assert!(out.contains("//lazuli:pattern error_resolver v1"));
        assert!(out.contains("func init() {"));
        assert!(
            out.contains("lazuli.RegisterFeatureErrors(\"account\", accountgen.FeatureErrors)")
        );
        assert!(
            out.contains("lazuli.RegisterFeatureErrors(\"billing\", billinggen.FeatureErrors)")
        );
        // Alphabetical: account before billing.
        let account_idx = out
            .find("\"account\", accountgen")
            .expect("account register present");
        let billing_idx = out
            .find("\"billing\", billinggen")
            .expect("billing register present");
        assert!(
            account_idx < billing_idx,
            "features must register in sorted order:\n{out}"
        );
    }

    #[test]
    fn emit_error_vocab_files_collects_features_and_app_registry() {
        let mut account = empty_feature("account");
        account.errors = Some(FeatureErrors::default());
        let module = module_with(vec![account]);
        let files =
            emit_error_vocab_files("lazuli/test-app", &module, "lazuli/test-app");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "account/errors.gen.go");
        assert_eq!(files[1].path, "app/error_resolution.gen.go");
    }

    #[test]
    fn deterministic_across_runs() {
        let mut feature = empty_feature("account");
        feature.errors = Some(FeatureErrors {
            default: Some(ErrorExposureDefault::Hide),
            exposure_4xx: vec!["message".to_owned(), "code".to_owned()],
            exposure_5xx: Vec::new(),
            messages: vec![
                FeatureErrorMessage {
                    code: "tenant_mismatch".to_owned(),
                    message: TranslationKeyRef {
                        key: "k1".to_owned(),
                        span_ref: None,
                    },
                    span_ref: None,
                },
                FeatureErrorMessage {
                    code: "policy_denied".to_owned(),
                    message: TranslationKeyRef {
                        key: "k2".to_owned(),
                        span_ref: None,
                    },
                    span_ref: None,
                },
            ],
            field_messages: Vec::new(),
            audience_exposure: Vec::new(),
            redact_patterns: Vec::new(),
            span_ref: None,
        });
        let a = emit_feature_errors_file("lazuli/test-app", &feature).unwrap();
        let b = emit_feature_errors_file("lazuli/test-app", &feature).unwrap();
        assert_eq!(a, b);
    }
}
