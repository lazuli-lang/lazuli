//! Auto-photo command + record synthesis (FR-3a).
//!
//! For each resource that carries the canonical `user: User required
//! unique` identity field PLUS an optional `@cap.File(...)` typed field,
//! the synth pass appends:
//!
//!   * 4 commands — `request_<field>_upload`, `confirm_<field>`,
//!     `clear_<field>`, `get_<field>_url` — covering the standard
//!     S3 / GCS / Cloudflare-R2 signed-URL flow.
//!   * 2 records — `<Field>UploadIntent` (request side) and
//!     `<Field>UploadDisplay` (read side) — typed input / output
//!     for the four commands.
//!
//! Author overrides win (same-name skip). The synth is fired from
//! `lower_feature_skeleton` after the rest of the feature lowers.
//!
//! The trailing `simple_*` / `builtin_*` helpers are small typed
//! field constructors reused throughout this module's record/command
//! shape definitions.

use crate::helpers::{pascal_to_snake, snake_to_pascal};
use lazuli_ir as ir;
use lazuli_ir::SynthesizedFromCapFile;

// =============================================================================
// Cut A — agent lowering (canonical-indent slice).
//
// `lower_feature_skeleton(&syntax::FeatureSkeleton)` projects the new
// canonical-indent AST into an `ir::Feature` carrying `agents: Vec<Agent>`.
// Other feature children stay in the legacy pipeline; this function
// returns a `Feature` with zeroed siblings so callers (CLI / LSP / tests)
// can merge it against the legacy lowering result if both pipelines are
// running.
//
// Resolved tool fields (`ToolBinding.resolved_effect`,
// `resolved_policy`, `resolved_pii_classes`) stay `None` here — the
// expand pass in `lazuli_cli` populates them when the full workspace IR
// is loaded (plan §4.3).
//
// See docs/proposals/ai-primitives-v0-implementation.md §4.
// =============================================================================

/// FR-3a — for each resource with a `user: User required unique`
/// field carrying an optional `@cap.File(...)` typed field, append
/// the 4 auto-photo commands + 2 records to the feature.
///
/// Trigger conditions (all must hold):
///   1. Resource has a field named `user` of type `User` required unique.
///   2. The `@cap.File(...)` field is declared `optional`.
///   3. No author-written command in `feature.commands` shares the
///      synthesized name for that field's command role (request,
///      confirm, clear, get_url) — name collision skips THAT one
///      role and emits the other 3.
pub(crate) fn synthesize_auto_photo(feature: &mut ir::Feature) {
    let mut to_add_commands: Vec<ir::Command> = Vec::new();
    let mut to_add_records: Vec<ir::Record> = Vec::new();

    let existing_command_names: std::collections::HashSet<String> =
        feature.commands.iter().map(|c| c.name.clone()).collect();
    let existing_record_names: std::collections::HashSet<String> =
        feature.records.iter().map(|r| r.name.clone()).collect();

    // Resolve the policy to attach. Heuristic per D5: pick the
    // feature-level policy whose name matches `<resource_singular>_only`
    // for the *current* resource; fall back to `authenticated`.
    let policy_name_for = |resource: &str| -> Option<String> {
        let snake = pascal_to_snake(resource);
        let target = format!("{}_only", snake);
        if feature.policies.categories.iter().any(|p| p.name == target) {
            return Some(target);
        }
        let compact_target = format!("{}_only", resource.to_ascii_lowercase());
        if feature
            .policies
            .categories
            .iter()
            .any(|p| p.name == compact_target)
        {
            return Some(compact_target);
        }
        if feature
            .policies
            .categories
            .iter()
            .any(|p| p.name == "authenticated")
        {
            return Some("authenticated".to_owned());
        }
        None
    };

    for resource in &feature.resources {
        let has_user_unique = resource.fields.iter().any(|f| {
            f.name == "user"
                && f.required
                && f.unique
                && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "User")
        });
        if !has_user_unique {
            continue;
        }

        for field in &resource.fields {
            if field.required {
                continue;
            }
            let cap_file = match &field.type_ref {
                ir::TypeRef::Capability(ir::CapabilityRef::File(spec)) => spec.clone(),
                _ => continue,
            };

            let pascal_field = snake_to_pascal(&field.name);
            let intent_name = format!("{}UploadIntent", pascal_field);
            let display_name = format!("{}DisplayUrl", pascal_field);

            // Wave §6 (2026-05-23) — prefer the author's explicit
            // `auto_photo_policy: @policy.<name>` over the
            // resource-singular heuristic. The heuristic produces
            // surprises when the convention name happens to match a
            // policy with a different audience (e.g. a feature with
            // both `host_only` and `host_and_operator` policies); the
            // explicit declaration is the only ground truth.
            //
            // Doctor `CAP-FILE-POLICY-IMPLICIT` flags any `@cap.File`
            // site that didn't declare an explicit policy.
            let policy_name = if let Some(explicit) = cap_file
                .auto_photo_policy
                .as_deref()
                .and_then(|raw| raw.strip_prefix("@policy.").or(Some(raw)))
            {
                explicit.to_owned()
            } else {
                match policy_name_for(&resource.name) {
                    Some(n) => n,
                    None => continue, // no policy => skip whole resource silently
                }
            };

            // 2 records first (idempotent: skip if author already declared).
            if !existing_record_names.contains(&intent_name) {
                to_add_records.push(auto_photo_intent_record(&intent_name));
            }
            if !existing_record_names.contains(&display_name) {
                to_add_records.push(auto_photo_display_record(&display_name));
            }

            // 4 commands. Each role checks for name collision.
            for role in [
                ir::AutoPhotoCommandRole::Request,
                ir::AutoPhotoCommandRole::Confirm,
                ir::AutoPhotoCommandRole::Clear,
                ir::AutoPhotoCommandRole::GetUrl,
            ] {
                let cmd_name = auto_photo_command_name(&field.name, role);
                if existing_command_names.contains(&cmd_name) {
                    continue;
                }
                to_add_commands.push(build_auto_photo_command(
                    cmd_name,
                    &resource.name,
                    &field.name,
                    role,
                    &intent_name,
                    &display_name,
                    &policy_name,
                ));
            }

            // Suppress unused warning on cap_file — adapters read it
            // via the field's TypeRef anyway. Reserved here for
            // future per-site validations (max_size, accept).
            let _ = cap_file;
        }
    }

    feature.commands.extend(to_add_commands);
    feature.records.extend(to_add_records);
}

fn auto_photo_command_name(field: &str, role: ir::AutoPhotoCommandRole) -> String {
    match role {
        ir::AutoPhotoCommandRole::Request => format!("request_{}_upload", field),
        ir::AutoPhotoCommandRole::Confirm => format!("confirm_{}_upload", field),
        ir::AutoPhotoCommandRole::Clear => format!("clear_{}", field),
        ir::AutoPhotoCommandRole::GetUrl => format!("get_{}_url", field),
    }
}

fn auto_photo_intent_record(name: &str) -> ir::Record {
    ir::Record {
        name: name.to_owned(),
        public_contract: None,
        fields: vec![
            simple_required_field("url", builtin_text()),
            simple_required_field("method", builtin_text()),
            simple_required_field("headers_content_type", builtin_text()),
            simple_required_field("key", builtin_text()),
            simple_required_field("expires_at", builtin_datetime()),
        ],
        discriminator_field: None,
        span_ref: None,
    }
}

fn auto_photo_display_record(name: &str) -> ir::Record {
    ir::Record {
        name: name.to_owned(),
        public_contract: None,
        fields: vec![
            simple_optional_field("url", builtin_text()),
            simple_optional_field("expires_at", builtin_datetime()),
        ],
        discriminator_field: None,
        span_ref: None,
    }
}

pub(crate) fn build_auto_photo_command(
    name: String,
    resource: &str,
    field: &str,
    role: ir::AutoPhotoCommandRole,
    intent_name: &str,
    display_name: &str,
    policy_name: &str,
) -> ir::Command {
    use ir::*;
    let (input, effect, rate_limit) = match role {
        AutoPhotoCommandRole::Request => (
            CommandInput::Typed(vec![
                TypedSlot {
                    name: "content_type".to_owned(),
                    type_ref: builtin_text(),
                    required: true,
                    constraints: FieldConstraints::default(),
                    validate_skip: false,
                },
                TypedSlot {
                    name: "size_bytes".to_owned(),
                    type_ref: builtin_integer(),
                    required: true,
                    constraints: FieldConstraints::default(),
                    validate_skip: false,
                },
            ]),
            CommandEffect::Returns(ReturnsEffect {
                return_type: TypeRef::UserDefined(QualifiedName {
                    feature: None,
                    name: intent_name.to_owned(),
                }),
            }),
            "30 per 10 minutes per ip",
        ),
        AutoPhotoCommandRole::Confirm => (
            CommandInput::Typed(vec![TypedSlot {
                name: "key".to_owned(),
                type_ref: builtin_text(),
                required: true,
                constraints: FieldConstraints::default(),
                validate_skip: false,
            }]),
            CommandEffect::None,
            "30 per 10 minutes per ip",
        ),
        AutoPhotoCommandRole::Clear => (
            CommandInput::Empty,
            CommandEffect::None,
            "10 per 10 minutes per ip",
        ),
        AutoPhotoCommandRole::GetUrl => (
            CommandInput::Empty,
            CommandEffect::Returns(ReturnsEffect {
                return_type: TypeRef::UserDefined(QualifiedName {
                    feature: None,
                    name: display_name.to_owned(),
                }),
            }),
            "60 per 10 minutes per ip",
        ),
    };

    let _ = (resource, field); // currently only used via marker
    Command {
        name,
        public_contract: None,
        kind: CommandKind::Returns,
        route: Vec::new(),
        input,
        target: None,
        lets: Vec::new(),
        effect,
        policy: PolicyRef::Local(policy_name.to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        emits: Vec::new(),
        rate_limit: Some(RateLimitSpec::from_default(rate_limit.to_owned())),
        audit: Some(AuditSpec {
            subjects: vec!["default".to_owned()],
            emit_to: None,
            data_subject: None,
            record_before: false,
            record_after: false,
            retain_for: None,
            materialize: None,
        }),
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
        previous_names: Vec::new(),
        span_ref: None,
        triggers: Vec::new(),
        synthesized_from_cap_file: Some(SynthesizedFromCapFile {
            resource: resource.to_owned(),
            field: field.to_owned(),
            role,
        }),
        owner_scope_sql: None,
        derived_from: None,
    }
}

fn simple_required_field(name: &str, type_ref: ir::TypeRef) -> ir::Field {
    simple_field(name, type_ref, true)
}

fn simple_optional_field(name: &str, type_ref: ir::TypeRef) -> ir::Field {
    simple_field(name, type_ref, false)
}

fn simple_field(name: &str, type_ref: ir::TypeRef, required: bool) -> ir::Field {
    ir::Field {
        name: name.to_owned(),
        type_ref,
        required,
        unique: false,
        slug: false,
        default: None,
        derived_from: None,
        computed_date: None,
        constraints: ir::FieldConstraints::default(),
        full_text: false,
        previous_names: Vec::new(),
        pii: None,
        owner_axis: None,
        cross_feature_target: None,
        span_ref: None,
    }
}

fn builtin_text() -> ir::TypeRef {
    ir::TypeRef::Builtin(ir::BuiltinType::Text)
}

fn builtin_integer() -> ir::TypeRef {
    ir::TypeRef::Builtin(ir::BuiltinType::Integer)
}

fn builtin_datetime() -> ir::TypeRef {
    ir::TypeRef::Builtin(ir::BuiltinType::DateTime)
}
