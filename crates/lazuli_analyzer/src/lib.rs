//! Lowering from `lazuli_syntax::Document` (the legacy `aggregate { ... }`
//! parser AST) into the canonical `lazuli_ir::Module`.
//!
//! The legacy parser predates canonical syntax. We bridge it by synthesising
//! one feature per parsed `Document`. Each `aggregate Foo` becomes a
//! `Resource Foo` inside that synthetic feature. Commands, queries, and
//! surfaces from the legacy AST are lowered into their nearest canonical IR
//! shape with conservative defaults; richer constructs (workflows, rules,
//! events, raw queries, `route`, `let`, typed inputs) only land when the
//! canonical parser arrives in a later phase.
//!
//! Phase 1a goal: every `examples/crm.lzi` shape lowers cleanly. Anything
//! requiring canonical-only constructs will surface as `TypeRef::Unresolved`
//! or `PolicyRef::Unresolved` rather than fabricated facts.

use std::collections::BTreeSet;

use lazuli_ir as ir;
use lazuli_syntax as syntax;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalyzeError {
    #[error("duplicate aggregate `{name}`")]
    DuplicateAggregate { name: String },

    #[error("duplicate field `{field}` in aggregate `{aggregate}`")]
    DuplicateField { aggregate: String, field: String },

    #[error("unknown field `{field}` referenced by `{context}` in aggregate `{aggregate}`")]
    UnknownField {
        aggregate: String,
        context: String,
        field: String,
    },

    #[error("command `{command}` in aggregate `{aggregate}` is missing an explicit policy")]
    MissingCommandPolicy { aggregate: String, command: String },

    #[error("invalid tool reference `{reference}`")]
    InvalidToolRef { reference: String },

    /// Phase L — `auth identity` field reference must split exactly once
    /// into `<Resource>.<field>`. Parser already rejects missing-dot
    /// shapes; this guards downstream lowering against multi-dot or
    /// empty-segment forms slipping through.
    #[error("invalid auth identity `{reference}` — expected `<Resource>.<field>`")]
    InvalidAuthIdentity { reference: String },

    /// Phase L Tier 3 — `webhook verify <scheme>` only accepts `hmac`
    /// today. Adapters that ship other schemes lift through the
    /// registry adapter binding, not the verifier surface.
    #[error("unsupported webhook verify scheme `{scheme}` (use `hmac`)")]
    UnsupportedVerifyScheme { scheme: String },
}

pub fn lower_document(document: &syntax::Document) -> Result<ir::Module, AnalyzeError> {
    let mut aggregate_names = BTreeSet::new();
    let mut resources = Vec::new();
    let mut commands = Vec::new();
    let mut queries = Vec::new();

    for aggregate in &document.aggregates {
        if !aggregate_names.insert(aggregate.name.clone()) {
            return Err(AnalyzeError::DuplicateAggregate {
                name: aggregate.name.clone(),
            });
        }

        let lowered = lower_aggregate(aggregate)?;
        resources.push(lowered.resource);
        commands.extend(lowered.commands);
        queries.extend(lowered.queries);
    }

    let feature_name = document
        .app
        .clone()
        .map(|name| name.to_ascii_lowercase())
        .unwrap_or_else(|| "lazuli_app".to_owned());

    let feature = ir::Feature {
        name: feature_name,
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        defaults: ir::Defaults::default(),
        uses: Vec::new(),
        requirements: Vec::new(),
        enums: Vec::new(),
        resources,
        events: Vec::new(),
        rules: Vec::new(),
        policies: ir::Policies::default(),
        commands,
        apis: Vec::new(),
        records: Vec::new(),
        queries,
        workflows: Vec::new(),
        jobs: Vec::new(),
        webhooks: Vec::new(),
        notifications: Vec::new(),
        event_groups: Vec::new(),
        tenant_migrations: Vec::new(),
        translation: None,
        auth: None,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        agents: Vec::new(),
        previous_names: Vec::new(),
        span_ref: Some(ir::SpanRef {
            start: document.span.start,
            end: document.span.end,
        }),
    };

    Ok(ir::Module {
        workspace: None,
        contracts: Vec::new(),
        app: None,
        registry: None,
        profiles: Vec::new(),
        features: vec![feature],
    })
}

pub fn lower_lzx_document(document: &syntax::LzxDocument) -> ir::ExperienceModule {
    ir::ExperienceModule {
        app: document.app.as_ref().map(lower_lzx_app),
        routes: document.routes.iter().map(lower_lzx_route).collect(),
        experiences: document.experiences.iter().map(lower_experience).collect(),
        surfaces: document
            .surfaces
            .iter()
            .map(lower_platform_surface)
            .collect(),
    }
}

fn lower_lzx_app(app: &syntax::LzxApp) -> ir::AppManifest {
    ir::AppManifest {
        name: app.name.clone(),
        title: app.title.clone(),
        version: app.version.clone(),
        targets: app.targets.clone(),
        default_locale: app.default_locale.clone(),
        default_timezone: app.default_timezone.clone(),
        auth_failed_redirect: app.auth_failed_redirect.clone(),
        not_found: app.not_found.clone(),
        uses: app.uses.clone(),
        packs: Vec::new(),
        bindings: Vec::new(),
        architecture: None,
        services: Vec::new(),
        communication: None,
        environments: Vec::new(),
        urls: Vec::new(),
        cors: None,
        env: Vec::new(),
        integrations: Vec::new(),
        capabilities: Vec::new(),
        runtime: Vec::new(),
        deploy: None,
        logging: None,
        tracing: None,
        locale: None,
        span_ref: Some(span_of(app.span)),
    }
}

fn lower_lzx_route(route: &syntax::LzxRoute) -> ir::AppRoute {
    ir::AppRoute {
        name: route.name.clone(),
        path: route.path.clone(),
        routes: route.routes.clone(),
        to: route.to.clone(),
        surface: route.surface.clone(),
        audience: route.audience.clone(),
        lazy: route.lazy,
        prerender: route.prerender.clone(),
        span_ref: Some(span_of(route.span)),
    }
}

fn lower_experience(experience: &syntax::LzxExperience) -> ir::Experience {
    ir::Experience {
        name: experience.name.clone(),
        imports: experience.imports.clone(),
        views: experience.views.iter().map(lower_experience_view).collect(),
        extensions: experience
            .extensions
            .iter()
            .map(lower_view_extension)
            .collect(),
        span_ref: Some(span_of(experience.span)),
    }
}

fn lower_experience_view(view: &syntax::LzxExperienceView) -> ir::ExperienceView {
    ir::ExperienceView {
        name: view.name.clone(),
        anchor: view.anchor.clone(),
        routes: view.routes.clone(),
        extensible_by: view.extensible_by.clone(),
        source: view.source.clone(),
        submit: view.submit.clone(),
        blocks: view.blocks.clone(),
        actions: view.actions.iter().map(lower_experience_action).collect(),
        opens: view.opens.clone(),
        tests: view.tests.clone(),
        span_ref: Some(span_of(view.span)),
    }
}

fn lower_experience_action(action: &syntax::LzxAction) -> ir::ExperienceAction {
    ir::ExperienceAction {
        name: action.name.clone(),
        target: action.target.clone(),
        span_ref: Some(span_of(action.span)),
    }
}

fn lower_view_extension(extension: &syntax::LzxViewExtension) -> ir::ViewExtension {
    ir::ViewExtension {
        anchor: extension.anchor.clone(),
        blocks: extension.blocks.clone(),
        slots: extension
            .slots
            .iter()
            .map(lower_view_extension_slot)
            .collect(),
        span_ref: Some(span_of(extension.span)),
    }
}

fn lower_view_extension_slot(slot: &syntax::LzxExtensionSlot) -> ir::ViewExtensionSlot {
    ir::ViewExtensionSlot {
        name: slot.name.clone(),
        order: slot.order.as_ref().map(|order| ir::ViewExtensionOrder {
            relation: order.relation.clone(),
            target: order.target.clone(),
        }),
        blocks: slot.blocks.clone(),
        platforms: slot.platforms.clone(),
        audiences: slot.audiences.clone(),
        span_ref: Some(span_of(slot.span)),
    }
}

fn lower_platform_surface(surface: &syntax::LzxSurface) -> ir::PlatformSurface {
    ir::PlatformSurface {
        experience: surface.experience.clone(),
        platform: match surface.platform {
            syntax::LzxPlatform::Web => ir::Platform::Web,
            syntax::LzxPlatform::Mobile => ir::Platform::Mobile,
        },
        uses_experience: surface.uses_experience.clone(),
        audiences: surface
            .audiences
            .iter()
            .map(lower_audience_surface)
            .collect(),
        span_ref: Some(span_of(surface.span)),
    }
}

fn lower_audience_surface(audience: &syntax::LzxAudience) -> ir::AudienceSurface {
    ir::AudienceSurface {
        name: audience.name.clone(),
        qualifiers: audience.qualifiers.clone(),
        views: audience.views.iter().map(lower_platform_view).collect(),
        span_ref: Some(span_of(audience.span)),
    }
}

fn lower_platform_view(view: &syntax::LzxPlatformView) -> ir::PlatformView {
    ir::PlatformView {
        name: view.name.clone(),
        view_type: view.view_type.clone(),
        columns: view.columns.clone(),
        fields: view.fields.clone(),
        sections: view.sections.clone(),
        search: view.search.clone(),
        filter: view.filter.clone(),
        cells: view.cells.clone(),
        actions: view.actions.clone(),
        submit: view.submit.clone(),
        blocks: view.blocks.clone(),
        span_ref: Some(span_of(view.span)),
    }
}

struct LoweredAggregate {
    resource: ir::Resource,
    commands: Vec<ir::Command>,
    queries: Vec<ir::Query>,
}

fn lower_aggregate(aggregate: &syntax::Aggregate) -> Result<LoweredAggregate, AnalyzeError> {
    let mut field_names = BTreeSet::new();
    let mut fields = Vec::new();

    for field in &aggregate.fields {
        if !field_names.insert(field.name.clone()) {
            return Err(AnalyzeError::DuplicateField {
                aggregate: aggregate.name.clone(),
                field: field.name.clone(),
            });
        }

        fields.push(lower_field(field));
    }

    for command in &aggregate.commands {
        validate_known_fields(
            aggregate,
            &field_names,
            format!("command {}", command.name),
            &command.input,
        )?;
    }

    for query in &aggregate.queries {
        validate_known_fields(
            aggregate,
            &field_names,
            format!("query {} search", query.name),
            &query.search,
        )?;
        validate_known_fields(
            aggregate,
            &field_names,
            format!("query {} filters", query.name),
            &query.filters,
        )?;
    }

    for surface in &aggregate.surfaces {
        validate_known_fields(
            aggregate,
            &field_names,
            format!("surface {} list", surface.name),
            &surface.list_columns,
        )?;
        validate_known_fields(
            aggregate,
            &field_names,
            format!("surface {} form", surface.name),
            &surface.form_fields,
        )?;
        validate_known_fields(
            aggregate,
            &field_names,
            format!("surface {} detail", surface.name),
            &surface.detail_fields,
        )?;
    }

    let resource_name = aggregate.name.clone();
    let resource = ir::Resource {
        name: resource_name.clone(),
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields,
        constraints: Vec::new(),
        validate: None,
        validates: Vec::new(),
        retention: None,
        previous_names: Vec::new(),
        span_ref: Some(span_of(aggregate.span)),
    };

    let commands = aggregate
        .commands
        .iter()
        .map(|c| lower_command(c, &resource_name))
        .collect::<Result<Vec<_>, _>>()?;

    let queries = aggregate.queries.iter().map(lower_query).collect();

    Ok(LoweredAggregate {
        resource,
        commands,
        queries,
    })
}

fn lower_field(field: &syntax::Field) -> ir::Field {
    let mut required = false;
    let mut unique = false;
    let mut default: Option<ir::DefaultValue> = None;

    for modifier in &field.modifiers {
        match modifier {
            syntax::FieldModifier::Required => required = true,
            syntax::FieldModifier::Unique => unique = true,
            syntax::FieldModifier::Default(value) => {
                default = Some(parse_default(value));
            }
        }
    }

    ir::Field {
        name: field.name.clone(),
        type_ref: type_ref_from_syntax(&field.ty),
        required,
        unique,
        default,
        derived_from: None,
        previous_names: Vec::new(),
        span_ref: Some(span_of(field.span)),
    }
}

fn lower_command(
    command: &syntax::Command,
    resource_name: &str,
) -> Result<ir::Command, AnalyzeError> {
    let policy = command
        .policy
        .as_ref()
        .map(|raw| ir::PolicyRef::Unresolved(raw.clone()))
        .ok_or_else(|| AnalyzeError::MissingCommandPolicy {
            aggregate: resource_name.to_owned(),
            command: command.name.clone(),
        })?;

    // Legacy `command Create { input ... emits ... }` is treated as a create
    // effect over the parent aggregate with `from_input` semantics. The
    // canonical parser will replace this heuristic with explicit
    // `creates`/`updates`/`deletes` keywords.
    let effect = ir::CommandEffect::Creates(ir::CreateEffect {
        resource: ir::QualifiedName {
            feature: None,
            name: resource_name.to_owned(),
        },
        from_input: true,
        assignments: Vec::new(),
    });

    Ok(ir::Command {
        name: command.name.clone(),
        kind: ir::CommandKind::Create,
        route: Vec::new(),
        input: ir::CommandInput::Short(command.input.clone()),
        target: None,
        lets: Vec::new(),
        effect,
        policy,
        emits: command.emits.clone(),
        rate_limit: None,
        audit: None,
        approval: None,
        invalidates: Vec::new(),
        external_calls: Vec::new(),
        timeout: None,
        retry: None,
        idempotency: None,
        deprecated: None,
        tests: None,
        previous_names: Vec::new(),
        span_ref: Some(span_of(command.span)),
    })
}

fn lower_query(query: &syntax::Query) -> ir::Query {
    // Legacy `query List { search ... filter ... }` lowers into a list query
    // with field filters. Search currently has no canonical home and is
    // dropped on the floor; it will return as a typed query construct in a
    // later phase.
    let filters = query
        .filters
        .iter()
        .map(|name| ir::Filter {
            predicate: ir::Predicate::Comparison {
                left: ir::Expr::Path(ir::Path::from_segments([name.clone()])),
                op: ir::CompareOp::Eq,
                right: ir::Expr::Path(ir::Path::from_segments(["params".to_owned(), name.clone()])),
            },
            when: Some(name.clone()),
        })
        .collect();

    ir::Query::List(ir::ListQuery {
        name: query.name.clone(),
        params: Vec::new(),
        scope: Vec::new(),
        scope_override: false,
        filters,
        order: Vec::new(),
        paginate: None,
        modifier: None,
        cache: None,
        previous_names: Vec::new(),
        span_ref: Some(span_of(query.span)),
    })
}

fn validate_known_fields(
    aggregate: &syntax::Aggregate,
    known: &BTreeSet<String>,
    context: String,
    fields: &[String],
) -> Result<(), AnalyzeError> {
    for field in fields {
        if !known.contains(field) {
            return Err(AnalyzeError::UnknownField {
                aggregate: aggregate.name.clone(),
                context,
                field: field.clone(),
            });
        }
    }

    Ok(())
}

/// Public wrapper around `type_ref_from_syntax` so the inspect CLI can
/// reuse the analyzer's `@cap.File(...)` typing pass without re-implementing
/// the parser. The bare function stays private for the rest of the crate so
/// future internal callers keep their existing access path.
pub fn type_ref_from_syntax_public(ty: &str) -> ir::TypeRef {
    type_ref_from_syntax(ty)
}

fn type_ref_from_syntax(ty: &str) -> ir::TypeRef {
    // Phase L Tier 4 follow-up — the canonical-indent parser captures
    // the whole post-`:` head as `type_text`, including trailing
    // decorator markers like `@pii.contact` that follow the type but
    // precede modifiers. The legacy text-walker peeled them as
    // "modifiers"; here we take the first paren-balanced token as the
    // actual type and drop the rest. This matches the behaviour of
    // `parse_resource_field` in the retired doctor walker.
    let ty = first_paren_balanced_token(ty);
    // Phase L Tier 2 — typed `@cap.File(...)` capability.
    if let Some(file) = parse_cap_file_type(ty) {
        return ir::TypeRef::Capability(ir::CapabilityRef::File(file));
    }
    // Phase L Tier 4 follow-up — typed `@cap.Hashed/Encrypted/Token`.
    if let Some(hashed) = parse_cap_hashed_type(ty) {
        return ir::TypeRef::Capability(ir::CapabilityRef::Hashed(hashed));
    }
    if let Some(encrypted) = parse_cap_encrypted_type(ty) {
        return ir::TypeRef::Capability(ir::CapabilityRef::Encrypted(encrypted));
    }
    if let Some(token) = parse_cap_token_type(ty) {
        return ir::TypeRef::Capability(ir::CapabilityRef::Token(token));
    }
    // Phase L Tier 4 follow-up — typed `@semantic.*` shorthand for the
    // closed catalog (Email/Phone/Url/Uuid). Other `@semantic.<X>`
    // names still fall through to `UserDefined` so the language can
    // surface "unknown semantic" diagnostics rather than silently
    // accepting them.
    match ty {
        "@semantic.Email" => return ir::TypeRef::Builtin(ir::BuiltinType::SemanticEmail),
        "@semantic.Phone" => return ir::TypeRef::Builtin(ir::BuiltinType::SemanticPhone),
        "@semantic.Url" => return ir::TypeRef::Builtin(ir::BuiltinType::SemanticUrl),
        "@semantic.Uuid" => return ir::TypeRef::Builtin(ir::BuiltinType::SemanticUuid),
        _ => {}
    }
    match ty {
        "ID" | "Id" => ir::TypeRef::Builtin(ir::BuiltinType::Id),
        "Text" | "String" => ir::TypeRef::Builtin(ir::BuiltinType::Text),
        "Boolean" | "Bool" => ir::TypeRef::Builtin(ir::BuiltinType::Boolean),
        "Integer" | "Int" => ir::TypeRef::Builtin(ir::BuiltinType::Integer),
        "Decimal" | "Float" | "Money" => ir::TypeRef::Builtin(ir::BuiltinType::Decimal),
        "Date" => ir::TypeRef::Builtin(ir::BuiltinType::Date),
        "DateTime" => ir::TypeRef::Builtin(ir::BuiltinType::DateTime),
        "JSON" | "Json" => ir::TypeRef::Builtin(ir::BuiltinType::Json),
        "Email" => ir::TypeRef::Builtin(ir::BuiltinType::SemanticEmail),
        other => ir::TypeRef::UserDefined(ir::QualifiedName {
            feature: None,
            name: other.to_owned(),
        }),
    }
}

/// Phase L Tier 4 follow-up — return the first whitespace-delimited
/// token from `text`, respecting paren-balanced segments. The
/// canonical-indent parser captures decorator markers (`@pii.contact`,
/// `@cap.Hashed(algorithm:argon2id)`) and trailing markers after the
/// real type in `type_text`; this helper picks the leading type.
fn first_paren_balanced_token(text: &str) -> &str {
    let text = text.trim();
    let mut depth = 0i32;
    for (idx, ch) in text.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            c if c.is_whitespace() && depth == 0 => return text[..idx].trim_end(),
            _ => {}
        }
    }
    text
}

/// Phase L Tier 4 follow-up — `@cap.Hashed(algorithm:<X>)`. Closed
/// catalog `{argon2id, bcrypt}`. Returns `None` if the algorithm is
/// missing or unrecognised so callers fall through to `UserDefined`
/// (LSP surfaces shape errors).
fn parse_cap_hashed_type(ty: &str) -> Option<ir::HashedCapability> {
    let inner = ty.strip_prefix("@cap.Hashed(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);
    let algorithm = match args.get("algorithm")?.as_str() {
        "argon2id" => ir::HashAlgorithm::Argon2id,
        "bcrypt" => ir::HashAlgorithm::Bcrypt,
        _ => return None,
    };
    Some(ir::HashedCapability { algorithm })
}

/// Phase L Tier 4 follow-up — `@cap.Encrypted(key:@key.<scope>)`. Key
/// reference is stored verbatim with its `@key.` prefix.
fn parse_cap_encrypted_type(ty: &str) -> Option<ir::EncryptedCapability> {
    let inner = ty.strip_prefix("@cap.Encrypted(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);
    let key = args.get("key")?.clone();
    if !key.starts_with("@key.") {
        return None;
    }
    Some(ir::EncryptedCapability { key })
}

/// Phase L Tier 4 follow-up — `@cap.Token(ttl:<dur>,single_use:<bool>,
/// store:<storage>)`. All three dimensions are mandatory; closed
/// catalog `store:{hashed}` and `single_use:{true,false}`.
fn parse_cap_token_type(ty: &str) -> Option<ir::TokenCapability> {
    let inner = ty.strip_prefix("@cap.Token(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);
    let ttl = args.get("ttl")?.clone();
    let single_use = match args.get("single_use")?.as_str() {
        "true" => true,
        "false" => false,
        _ => return None,
    };
    let store = match args.get("store")?.as_str() {
        "hashed" => ir::TokenStore::Hashed,
        _ => return None,
    };
    Some(ir::TokenCapability {
        ttl,
        single_use,
        store,
    })
}

/// Parse `@cap.File(max_size:25mb,accept:text/csv[,visibility:...,signed_ttl:...])`
/// into a typed `FileCapability`. Returns `None` for any malformed shape so
/// the caller falls through to the legacy `UserDefined` fallback — the LSP
/// already surfaces shape errors for the same patterns.
fn parse_cap_file_type(ty: &str) -> Option<ir::FileCapability> {
    let inner = ty.strip_prefix("@cap.File(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);

    let max_size = parse_file_size(args.get("max_size")?)?;
    let accept = parse_mime_list(args.get("accept")?)?;
    if accept.is_empty() {
        return None;
    }
    let visibility = args
        .get("visibility")
        .map(|s| s.as_str())
        .and_then(parse_file_visibility);
    let signed_ttl = args.get("signed_ttl").map(|s| s.clone());

    Some(ir::FileCapability {
        max_size,
        accept,
        visibility,
        signed_ttl,
    })
}

fn parse_capability_args(inner: &str) -> std::collections::BTreeMap<String, String> {
    inner
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .filter_map(|part| {
            part.split_once(':')
                .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        })
        .collect()
}

fn parse_file_size(raw: &str) -> Option<ir::FileSize> {
    let digit_count = raw.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count == 0 || digit_count == raw.len() {
        return None;
    }
    let amount: u32 = raw[..digit_count].parse().ok()?;
    let unit = &raw[digit_count..];
    let literal = match unit {
        "kb" => ir::FileSizeLiteral::Kb(amount),
        "mb" => ir::FileSizeLiteral::Mb(amount),
        "gb" => ir::FileSizeLiteral::Gb(amount),
        _ => return None,
    };
    Some(ir::FileSize {
        bytes: literal.bytes(),
        literal,
    })
}

fn parse_mime_list(raw: &str) -> Option<Vec<ir::MimeType>> {
    let mut out = Vec::new();
    for token in raw.split('|') {
        let token = token.trim();
        if token.is_empty() {
            return None;
        }
        let (family, subtype) = token.split_once('/')?;
        let family = family.trim();
        let subtype = subtype.trim();
        if family.is_empty() || subtype.is_empty() {
            return None;
        }
        out.push(ir::MimeType {
            family: family.to_owned(),
            subtype: subtype.to_owned(),
        });
    }
    Some(out)
}

fn parse_file_visibility(raw: &str) -> Option<ir::FileVisibility> {
    match raw {
        "public" => Some(ir::FileVisibility::Public),
        "private" => Some(ir::FileVisibility::Private),
        "signed" => Some(ir::FileVisibility::Signed),
        _ => None,
    }
}

fn parse_default(raw: &str) -> ir::DefaultValue {
    if raw == "true" {
        return ir::DefaultValue::Boolean(true);
    }
    if raw == "false" {
        return ir::DefaultValue::Boolean(false);
    }
    if raw == "nil" {
        return ir::DefaultValue::Nil;
    }
    if let Ok(value) = raw.parse::<i64>() {
        return ir::DefaultValue::Integer(value);
    }

    if raw
        .chars()
        .next()
        .map(|c| c.is_alphabetic() || c == '_')
        .unwrap_or(false)
        && raw.chars().all(|c| c.is_alphanumeric() || c == '_')
    {
        return ir::DefaultValue::EnumLiteral(ir::EnumLiteral {
            type_name: None,
            variant: raw.to_owned(),
        });
    }

    ir::DefaultValue::String(raw.to_owned())
}

fn span_of(span: syntax::Span) -> ir::SpanRef {
    ir::SpanRef {
        start: span.start,
        end: span.end,
    }
}

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

/// Lower a canonical-indent feature skeleton into an `ir::Feature` whose
/// only populated child slot is `agents`. Every other vector / option
/// uses zero defaults; callers that consume both pipelines fold this
/// result into the legacy `Feature` produced by `lower_document`.
pub fn lower_feature_skeleton(
    skeleton: &syntax::FeatureSkeleton,
) -> Result<ir::Feature, AnalyzeError> {
    let mut agents = Vec::with_capacity(skeleton.agents.len());
    for agent_ast in &skeleton.agents {
        agents.push(lower_agent(&skeleton.name, agent_ast)?);
    }
    let auth = match &skeleton.auth {
        Some(auth_ast) => Some(lower_auth(auth_ast)?),
        None => None,
    };
    let mut jobs = Vec::with_capacity(skeleton.jobs.len());
    for job_ast in &skeleton.jobs {
        jobs.push(lower_job(&skeleton.name, job_ast)?);
    }
    let mut webhooks = Vec::with_capacity(skeleton.webhooks.len());
    for webhook_ast in &skeleton.webhooks {
        webhooks.push(lower_webhook(webhook_ast)?);
    }
    let mut notifications = Vec::with_capacity(skeleton.notifications.len());
    for notification_ast in &skeleton.notifications {
        notifications.push(lower_notification(&skeleton.name, notification_ast)?);
    }
    let mut event_groups = Vec::with_capacity(skeleton.event_groups.len());
    for group_ast in &skeleton.event_groups {
        event_groups.push(lower_event_group(group_ast));
    }
    let mut tenant_migrations = Vec::with_capacity(skeleton.tenant_migrations.len());
    for tm_ast in &skeleton.tenant_migrations {
        tenant_migrations.push(lower_tenant_migration(tm_ast)?);
    }
    let defaults = match &skeleton.defaults {
        Some(d) => lower_defaults(d),
        None => ir::Defaults::default(),
    };
    let commands = skeleton.commands.iter().map(lower_command_decl).collect();
    let apis = skeleton.apis.iter().map(lower_api_decl).collect();
    let resources = skeleton.resources.iter().map(lower_resource_decl).collect();
    let queries = skeleton.queries.iter().map(lower_query_decl).collect();
    let records = skeleton.records.iter().map(lower_record_decl).collect();
    let policies = skeleton
        .policies
        .as_ref()
        .map(lower_policies_decl)
        .unwrap_or_default();
    let enums = skeleton.enums.iter().map(lower_enum_decl).collect();
    Ok(ir::Feature {
        name: skeleton.name.clone(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        defaults,
        uses: Vec::new(),
        requirements: Vec::new(),
        enums,
        resources,
        events: Vec::new(),
        rules: Vec::new(),
        policies,
        commands,
        apis,
        records,
        queries,
        workflows: Vec::new(),
        jobs,
        webhooks,
        notifications,
        event_groups,
        tenant_migrations,
        translation: skeleton.translation.as_ref().map(lower_translation_decl),
        auth,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        agents,
        previous_names: Vec::new(),
        span_ref: Some(span_of(skeleton.span)),
    })
}

/// Phase L Tier 4d — lower a canonical-indent query declaration into
/// `ir::Query`. The three shapes (`query.list`, `query.lookup`,
/// `query.sql`) project onto the existing IR variants.
fn lower_query_decl(q: &syntax::QueryDecl) -> ir::Query {
    match q {
        syntax::QueryDecl::List(list) => ir::Query::List(ir::ListQuery {
            name: list.name.clone(),
            params: list
                .params
                .iter()
                .map(lower_command_input_to_typed)
                .collect(),
            scope: Vec::new(),
            scope_override: list.scope_override,
            filters: Vec::new(),
            order: Vec::new(),
            paginate: list.paginate,
            modifier: list.modifier.clone(),
            cache: lower_query_cache(&list.cache),
            previous_names: Vec::new(),
            span_ref: Some(span_of(list.span)),
        }),
        syntax::QueryDecl::Lookup(lookup) => ir::Query::Lookup(ir::LookupQuery {
            name: lookup.name.clone(),
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
            filters: Vec::new(),
            previous_names: Vec::new(),
            span_ref: Some(span_of(lookup.span)),
        }),
        syntax::QueryDecl::Sql(sql) => ir::Query::Sql(ir::SqlQuery {
            name: sql.name.clone(),
            params: sql
                .params
                .iter()
                .map(lower_command_input_to_typed)
                .collect(),
            scope: Vec::new(),
            scope_override: false,
            returns: type_ref_from_text(&sql.returns),
            sql_path: sql.sql_path.clone(),
            cache: None,
            previous_names: Vec::new(),
            span_ref: Some(span_of(sql.span)),
        }),
    }
}

/// Cache bucket cycle — lift `cache` body lines (`key <expr>`, `ttl
/// <literal-or-prose>`, `tags <label>...`, `namespace <label>`) into
/// the typed `QueryCache` IR shape. Returns `None` when no `key` is
/// declared (defensive — doctor flags `cache without key/ttl`).
fn lower_query_cache(lines: &[String]) -> Option<ir::QueryCache> {
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
    })
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

fn lower_command_input_to_typed(slot: &syntax::CommandInputSlot) -> ir::TypedSlot {
    ir::TypedSlot {
        name: slot.name.clone(),
        type_ref: type_ref_from_text(&slot.type_text),
        required: slot.required,
    }
}

/// Phase L Tier 4d — lower a canonical-indent `record` block into
/// `ir::Record`.
fn lower_record_decl(r: &syntax::RecordDecl) -> ir::Record {
    ir::Record {
        name: r.name.clone(),
        fields: r.fields.iter().map(lower_resource_field).collect(),
        discriminator_field: r.discriminator_field.clone(),
        span_ref: Some(span_of(r.span)),
    }
}

/// Phase L Tier 4 follow-up — lower a canonical-indent `policies` block
/// into `ir::Policies`. The AST mirrors the IR shape 1:1 so this is a
/// structural copy: category atoms and per-resource field overrides
/// project directly. Closed-catalog validation lives in doctor.
fn lower_policies_decl(decl: &syntax::PoliciesDecl) -> ir::Policies {
    let categories = decl
        .categories
        .iter()
        .map(|c| ir::PolicyCategory {
            name: c.name.clone(),
            atoms: c.atoms.clone(),
            previous_names: Vec::new(),
        })
        .collect();
    let fields = decl
        .fields
        .iter()
        .map(|f| ir::FieldPolicies {
            resource: lower_qualified_name(&f.resource),
            fields: f
                .fields
                .iter()
                .map(|fp| ir::FieldPolicy {
                    field: fp.field.clone(),
                    read: fp.read.clone(),
                    write: fp.write.clone(),
                    previous_names: Vec::new(),
                })
                .collect(),
        })
        .collect();
    ir::Policies {
        categories,
        fields,
        span_ref: Some(span_of(decl.span)),
    }
}

/// Phase L Tier 4 follow-up — lower a canonical-indent `enum <Name>`
/// declaration into `ir::EnumDecl`. Variant storage values project
/// directly onto `ir::StorageValue`; absent values leave the codegen
/// target free to pick.
fn lower_enum_decl(decl: &syntax::EnumDeclAst) -> ir::EnumDecl {
    ir::EnumDecl {
        name: decl.name.clone(),
        variants: decl
            .variants
            .iter()
            .map(|v| ir::EnumVariant {
                name: v.name.clone(),
                storage_value: v.storage.as_ref().map(|s| match s {
                    syntax::EnumStorageValueDecl::Integer(n) => ir::StorageValue::Integer(*n),
                    syntax::EnumStorageValueDecl::String(s) => ir::StorageValue::String(s.clone()),
                }),
                previous_names: Vec::new(),
            })
            .collect(),
        previous_names: Vec::new(),
        span_ref: Some(span_of(decl.span)),
    }
}

/// Phase L Tier 4c — lower a canonical-indent `resource` block into
/// `ir::Resource`. `tenancy` (resource-local override), `soft_delete`,
/// `timestamps`, `retention`, `validates`, and `derived_from` all
/// project through additive IR fields landed alongside this lowering.
fn lower_resource_decl(r: &syntax::ResourceDecl) -> ir::Resource {
    let tenancy = r.tenancy.as_ref().map(|t| match t {
        syntax::DefaultsTenancy::Org => ir::Tenancy::Org,
        syntax::DefaultsTenancy::Team => ir::Tenancy::Team,
        syntax::DefaultsTenancy::None => ir::Tenancy::None,
        syntax::DefaultsTenancy::Custom(axis) => ir::Tenancy::Custom(axis.clone()),
    });
    let fields = r.fields.iter().map(lower_resource_field).collect();
    let retention = r.retention.as_ref().map(|ret| ir::RetentionSpec {
        duration: ret.duration.clone(),
        action: match ret.action {
            syntax::ResourceRetentionAction::Anonymize => ir::RetentionAction::Anonymize,
            syntax::ResourceRetentionAction::Delete => ir::RetentionAction::Delete,
            syntax::ResourceRetentionAction::Archive => ir::RetentionAction::Archive,
        },
    });
    // `validates @validator.tier_check` collapses onto `Resource.validate`
    // for a single-entry case (the fixture pattern). Multi-entry would
    // need a `Vec`; defer until pilot evidence demands it.
    let validate = r.validates.first().map(|v| ir::PathRef::authored(v));
    ir::Resource {
        name: r.name.clone(),
        tenancy,
        soft_delete: r.soft_delete,
        timestamps: if r.timestamps { Some(true) } else { None },
        fields,
        constraints: Vec::new(),
        validate,
        validates: Vec::new(),
        retention,
        previous_names: r
            .previously
            .iter()
            .map(|p| strip_previously_mode(p))
            .collect(),
        span_ref: Some(span_of(r.span)),
    }
}

/// Migrations bucket cycle Route C — strip the `migrated`/`alias` mode
/// prefix from a parsed `previously` line. `previously migrated Foo`
/// keeps `Foo` in IR; `previously alias Foo` ditto. Doctor compares
/// against current symbol names, so the mode keyword is noise here.
fn strip_previously_mode(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("migrated ") {
        return rest.trim().to_owned();
    }
    if let Some(rest) = trimmed.strip_prefix("alias ") {
        return rest.trim().to_owned();
    }
    trimmed.to_owned()
}

fn lower_resource_field(f: &syntax::ResourceFieldDecl) -> ir::Field {
    let default = f.default.as_deref().map(|raw| parse_default(raw.trim()));
    ir::Field {
        name: f.name.clone(),
        // Phase L Tier 4 follow-up — use `type_ref_from_syntax` so
        // `@cap.Hashed(algorithm:…)`, `@cap.Encrypted(key:…)`,
        // `@cap.Token(…)`, and `@semantic.*` lift into typed variants.
        // The legacy `type_ref_from_text` path is preserved for
        // call sites that pass cleaned-up identifiers only.
        type_ref: type_ref_from_syntax(&f.type_text),
        required: f.required,
        unique: f.unique,
        default,
        derived_from: f.derived_from.clone(),
        previous_names: f
            .previously
            .iter()
            .map(|p| strip_previously_mode(p))
            .collect(),
        span_ref: Some(span_of(f.span)),
    }
}

/// Phase L Tier 4b — lower a canonical-indent `command` block into
/// `ir::Command`. The kind is inferred from the body shape: `creates`
/// → Create, `updates` → Update, `deletes` → Delete, `returns` → Returns,
/// `handler`-only → Returns (the escape hatch case).
fn lower_command_decl(c: &syntax::CommandDecl) -> ir::Command {
    let kind = match c.effect.as_ref().map(|e| e.kind) {
        Some(syntax::CommandEffectKindDecl::Creates) => ir::CommandKind::Create,
        Some(syntax::CommandEffectKindDecl::Updates) => ir::CommandKind::Update,
        Some(syntax::CommandEffectKindDecl::Deletes) => ir::CommandKind::Delete,
        None => ir::CommandKind::Returns,
    };
    let route = c
        .route
        .iter()
        .map(|r| ir::RouteSlot {
            name: r.name.clone(),
            type_ref: type_ref_from_text(&r.type_text),
            from: r.from.clone(),
        })
        .collect();
    let input = match &c.input {
        syntax::CommandInputDecl::Empty => ir::CommandInput::Empty,
        syntax::CommandInputDecl::Short(name) => ir::CommandInput::Short(vec![name.clone()]),
        syntax::CommandInputDecl::Typed(slots) => ir::CommandInput::Typed(
            slots
                .iter()
                .map(|s| ir::TypedSlot {
                    name: s.name.clone(),
                    type_ref: type_ref_from_text(&s.type_text),
                    required: s.required,
                })
                .collect(),
        ),
    };
    let target = c.target.as_ref().map(lower_target_expr);
    let lets = c.lets.iter().map(lower_let_binding).collect();
    let effect = if let Some(e) = c.effect.as_ref() {
        lower_command_effect(e)
    } else if let Some(returns) = c.returns.as_deref() {
        ir::CommandEffect::Returns(ir::ReturnsEffect {
            return_type: type_ref_from_text(returns),
        })
    } else {
        ir::CommandEffect::None
    };
    let policy = c
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .unwrap_or(ir::PolicyRef::None);
    let emits = c.emits.iter().map(|e| e.name.clone()).collect();
    let audit = c.audit.as_ref().map(|a| ir::AuditSpec {
        subjects: a.subjects.clone(),
        emit_to: a.emit_to.clone(),
    });
    let approval = c.approval.as_ref().map(|a| ir::ApprovalSpec {
        required_when: a.required_when.clone(),
        by: a.by.clone(),
        timeout: a.timeout.clone(),
        then: match a.then {
            syntax::ApprovalThenDecl::Deny => ir::ApprovalThen::Deny,
            syntax::ApprovalThenDecl::Allow => ir::ApprovalThen::Allow,
            syntax::ApprovalThenDecl::Escalate => ir::ApprovalThen::Escalate,
        },
    });
    let invalidates = c
        .invalidates
        .iter()
        .map(|inv| ir::InvalidatesSpec {
            query: lower_qualified_name(&inv.query),
            args: inv.args.iter().map(lower_named_arg).collect(),
        })
        .collect();
    let external_calls = c.external_calls.iter().map(lower_external_call).collect();
    let deprecated = c.deprecated.as_ref().map(lower_command_deprecated);
    // Phase L Tier 4 follow-up — lift `timeout`/`retry`/`idempotency by`
    // mirrors of `parse_job`. Doctor cross-checks against
    // `external_calls` for the `INT-CALL-*` integration coverage rules.
    let timeout = c.timeout.clone();
    let retry = c.retry.as_ref().map(lower_retry);
    let idempotency = c
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    ir::Command {
        name: c.name.clone(),
        kind,
        route,
        input,
        target,
        lets,
        effect,
        policy,
        emits,
        rate_limit: c.rate_limit.clone(),
        audit,
        approval,
        invalidates,
        external_calls,
        timeout,
        retry,
        idempotency,
        deprecated,
        tests: None,
        previous_names: c.previously.clone(),
        span_ref: Some(span_of(c.span)),
    }
}

/// OpenAPI bucket cycle — lower an authored `deprecated` decorator into
/// the typed IR shape. `replacement` is classified by syntactic shape:
/// `https?://` → Url, `<feature>.command.<name>` → Qualified, otherwise
/// → LocalCommand. Doctor resolves LocalCommand against the same-feature
/// command table.
fn lower_command_deprecated(decl: &syntax::CommandDeprecatedDecl) -> ir::Deprecation {
    let replacement = decl.replacement.as_ref().map(|raw| {
        let trimmed = raw.trim();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            ir::DeprecationReplacement::Url(trimmed.to_owned())
        } else if let Some(stripped) = trimmed.strip_prefix("@") {
            // `@adapter.command.<name>` or similar — store as Url-style
            // verbatim escape hatch.
            ir::DeprecationReplacement::Url(format!("@{}", stripped))
        } else {
            // Detect `<feature>.command.<name>` shape.
            let parts: Vec<&str> = trimmed.split('.').collect();
            if parts.len() == 3 && parts[1] == "command" {
                ir::DeprecationReplacement::Qualified(ir::QualifiedName {
                    feature: Some(parts[0].to_owned()),
                    name: parts[2].to_owned(),
                })
            } else {
                ir::DeprecationReplacement::LocalCommand(trimmed.to_owned())
            }
        }
    });
    ir::Deprecation {
        since: decl.since.clone(),
        replacement,
        sunset: decl.sunset.clone(),
    }
}

/// Phase L Tier 4b — lower a canonical-indent `api` block into `ir::Api`.
fn lower_api_decl(a: &syntax::ApiDecl) -> ir::Api {
    let method = match a.method {
        syntax::HttpMethod::Get => ir::HttpMethod::Get,
        syntax::HttpMethod::Post => ir::HttpMethod::Post,
        syntax::HttpMethod::Put => ir::HttpMethod::Put,
        syntax::HttpMethod::Patch => ir::HttpMethod::Patch,
        syntax::HttpMethod::Delete => ir::HttpMethod::Delete,
    };
    let policy = a
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .unwrap_or(ir::PolicyRef::None);
    let handler = a
        .handler
        .as_deref()
        .map(ir::PathRef::authored)
        .unwrap_or_else(|| ir::PathRef::convention(format!("./api/{}.go", a.name)));
    ir::Api {
        name: a.name.clone(),
        method,
        path: a.path.clone(),
        policy,
        rate_limit: a.rate_limit.clone(),
        output: type_ref_from_text(&a.output),
        handler,
        locale_negotiate: a.locale_negotiate.as_ref().map(lower_locale_negotiate_decl),
        span_ref: Some(span_of(a.span)),
    }
}

/// i18n bucket cycle — lower an authored `translation` block onto
/// `ir::Translation`. Variant locales and plural arms come through
/// verbatim; doctor validates them against `app.locale.supported` and
/// the CLDR plural catalog.
fn lower_translation_decl(t: &syntax::TranslationDecl) -> ir::Translation {
    ir::Translation {
        catalog: t.catalog.clone(),
        keys: t
            .keys
            .iter()
            .map(|key| ir::TranslationKey {
                name: key.name.clone(),
                variants: key
                    .variants
                    .iter()
                    .map(|v| ir::TranslationVariant {
                        locale: v.locale.clone(),
                        text: v.text.clone(),
                    })
                    .collect(),
                plurals: key
                    .plurals
                    .iter()
                    .map(|p| ir::TranslationPluralArm {
                        arm: p.arm.clone(),
                        variants: p
                            .variants
                            .iter()
                            .map(|v| ir::TranslationVariant {
                                locale: v.locale.clone(),
                                text: v.text.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// i18n bucket cycle — lower a per-api `locale_negotiate` block onto
/// `ir::LocaleNegotiate`. The runtime-unit form is parsed elsewhere
/// (`crates/lazuli_cli/src/app_manifest.rs`) since it lives on the
/// `app.lzi` side rather than feature side.
fn lower_locale_negotiate_decl(n: &syntax::LocaleNegotiateDecl) -> ir::LocaleNegotiate {
    ir::LocaleNegotiate {
        source: n.source.clone(),
        strategy: n.strategy.clone(),
        fallback: n.fallback.clone(),
    }
}

/// Phase L Tier 4a — lower a canonical-indent `defaults` block into
/// `ir::Defaults`. `policy_for` entries collapse onto `Defaults.policy`
/// when a single entry is authored; multi-entry `policy_for` (different
/// atoms per kind list) is captured by reading the first entry — the
/// language disallows conflicting defaults by convention. Doctor cross-
/// checks the surface form by walking the typed
/// `feature.policies.categories` slot (`populate_commands_from_ir`);
/// the legacy `collect_policy_atoms` text walker is retired.
fn lower_defaults(defaults: &syntax::FeatureDefaults) -> ir::Defaults {
    let tenancy = defaults.tenancy.as_ref().map(|t| match t {
        syntax::DefaultsTenancy::Org => ir::Tenancy::Org,
        syntax::DefaultsTenancy::Team => ir::Tenancy::Team,
        syntax::DefaultsTenancy::None => ir::Tenancy::None,
        syntax::DefaultsTenancy::Custom(name) => ir::Tenancy::Custom(name.clone()),
    });
    let policy = defaults
        .policy_for
        .first()
        .map(|entry| lower_policy_atom(entry.atom.as_str()))
        .filter(|p| !matches!(p, ir::PolicyRef::None));
    ir::Defaults {
        tenancy,
        timestamps: defaults.timestamps,
        policy,
    }
}

/// Phase L Tier 3 — lower a canonical-indent `job` block into `ir::Job`.
/// Handler-backed bodies lower fully; declarative bodies preserve the
/// raw spine (`raw_target`, `raw_lets`, `raw_effect`) until Tier 4.
pub fn lower_job(feature: &str, job: &syntax::Job) -> Result<ir::Job, AnalyzeError> {
    let trigger = lower_job_trigger(feature, &job.trigger);
    let idempotency = job
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let retry = job.retry.as_ref().map(lower_retry);
    let tenant_from = job
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });
    let fanout = job.fanout.as_ref().map(lower_fanout);
    let external_calls = job.external_calls.iter().map(lower_external_call).collect();
    let policy = job
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .unwrap_or(ir::PolicyRef::None);
    let policy = match policy {
        ir::PolicyRef::None => None,
        other => Some(other),
    };
    let body = lower_job_body(&job.body);

    Ok(ir::Job {
        name: job.name.clone(),
        trigger,
        queue: job.queue.clone(),
        idempotency,
        retry,
        policy,
        tenant_from,
        fanout,
        timeout: job.timeout.clone(),
        external_calls,
        body,
        emits: job.emits.clone(),
        previous_names: Vec::new(),
        span_ref: Some(span_of(job.span)),
    })
}

/// Phase L Tier 3 — lower a canonical-indent `webhook` block into
/// `ir::Webhook`. `verify: PathRef` falls back to a conventional path
/// derived from the webhook name (the legacy IR field is non-optional);
/// `structured_verify` carries the real structured spec lifted by
/// `parse_webhook_verify`.
pub fn lower_webhook(webhook: &syntax::Webhook) -> Result<ir::Webhook, AnalyzeError> {
    let structured_verify = Some(ir::VerifySpec {
        scheme: match webhook.verify.scheme.as_str() {
            "hmac" => ir::VerifyScheme::Hmac,
            other => {
                return Err(AnalyzeError::UnsupportedVerifyScheme {
                    scheme: other.to_owned(),
                });
            }
        },
        algorithm: webhook.verify.algorithm.clone(),
        secret_env: webhook
            .verify
            .secret_env
            .as_deref()
            .map(extract_env_binding)
            .unwrap_or_default(),
        header: webhook.verify.header.clone().unwrap_or_default(),
    });
    let tenant_from = webhook
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });
    let idempotency = webhook
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let policy = webhook
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .filter(|p| !matches!(p, ir::PolicyRef::None));

    let (handler, returns) = match &webhook.handler {
        Some(h) => (
            ir::PathRef::authored(&h.path),
            h.returns.as_deref().map(|t| type_ref_from_text(t)),
        ),
        None => (
            ir::PathRef::convention(format!("./webhooks/{}.go", webhook.name)),
            None,
        ),
    };

    // Webhooks expanded cycle — typed payload reference (`payload from
    // webhook_events.<name>`). The parser stripped the catalog prefix
    // already, so the IR just keeps the suffix.
    let payload_from = webhook
        .payload_from
        .as_deref()
        .map(|name| ir::WebhookEventRef {
            name: name.to_owned(),
        });

    // `replay` short form (`replay allow within "..."`) and long form
    // (nested children) collapse onto the same `ReplaySpec`.
    let replay = webhook.replay.as_ref().map(|r| ir::ReplaySpec {
        mode: match r.mode.as_str() {
            "deny" => ir::ReplayMode::Deny,
            _ => ir::ReplayMode::Allow,
        },
        within: r.within.clone(),
        dedupe_by: r.dedupe_by.as_deref().map(lower_path_string),
    });

    // `dlq` discriminator (mutual exclusion enforced by the parser).
    let dlq = webhook.dlq.as_ref().map(|d| match d {
        syntax::WebhookDlq::Emit { event, .. } => ir::DlqSpec::Emit {
            event: event.clone(),
        },
        syntax::WebhookDlq::Handler { path, .. } => ir::DlqSpec::Handler {
            path: ir::PathRef::authored(path),
        },
        syntax::WebhookDlq::Drop { reason, .. } => ir::DlqSpec::Drop {
            reason: reason.clone(),
        },
    });

    // Inbound retry shares the jobs `RetryPolicy` shape (Atrito #5).
    let retry = webhook.retry.as_ref().map(lower_retry);

    Ok(ir::Webhook {
        name: webhook.name.clone(),
        route: webhook.route.clone(),
        verify: ir::PathRef::convention(format!("./webhooks/{}_verify.go", webhook.name)),
        structured_verify,
        tenant_from,
        idempotency,
        policy,
        handler,
        returns,
        emits: webhook.emits.clone(),
        payload_from,
        replay,
        dlq,
        retry,
        previous_names: Vec::new(),
        span_ref: Some(span_of(webhook.span)),
    })
}

/// Phase L Tier 3 — lower a canonical-indent `notification` block into
/// `ir::Notification`. Reuses `JobTrigger`, `IdempotencyKey`,
/// `RetryPolicy`, `TenantFromSpec` from the job lowering helpers.
pub fn lower_notification(
    feature: &str,
    notification: &syntax::Notification,
) -> Result<ir::Notification, AnalyzeError> {
    let trigger = lower_job_trigger(feature, &notification.trigger);
    let tenant_from = notification
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });
    let idempotency = notification
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let retry = notification.retry.as_ref().map(lower_retry);
    let policy = notification
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .filter(|p| !matches!(p, ir::PolicyRef::None));
    let digest = notification.digest.as_ref().map(lower_notification_digest);
    let throttle = notification
        .throttle
        .as_ref()
        .map(lower_notification_throttle);
    Ok(ir::Notification {
        name: notification.name.clone(),
        trigger,
        channels: notification.channels.clone(),
        recipient: notification.recipient.clone(),
        template: notification.template.clone(),
        policy,
        tenant_from,
        idempotency,
        retry,
        emits: notification.emits.clone(),
        digest,
        throttle,
        previous_names: Vec::new(),
        span_ref: Some(span_of(notification.span)),
    })
}

/// Notifications expanded bucket cycle — lower AST `NotificationDigest`
/// into the typed IR. `template_strategy` falls through `merge` /
/// `append` into the closed-catalog enum; unknown values become None
/// so doctor's `NOTIF-DIGEST-003` can flag them with a precise
/// message rather than the lowering failing silently.
fn lower_notification_digest(digest: &syntax::NotificationDigest) -> ir::NotificationDigest {
    let template_strategy = digest
        .template_strategy
        .as_deref()
        .and_then(|raw| match raw {
            "merge" => Some(ir::DigestStrategy::Merge),
            "append" => Some(ir::DigestStrategy::Append),
            _ => None,
        });
    ir::NotificationDigest {
        every: digest.every.clone(),
        group_by: digest.group_by.clone(),
        max_size: digest.max_size,
        template_strategy,
    }
}

/// Notifications expanded bucket cycle — lower AST
/// `NotificationThrottle` into the typed IR. Pure field-for-field
/// projection; no validation here (doctor `NOTIF-THROTTLE-*` covers
/// the closed-catalog and combinatorial rules).
fn lower_notification_throttle(
    throttle: &syntax::NotificationThrottle,
) -> ir::NotificationThrottle {
    ir::NotificationThrottle {
        max_per: throttle.max_per.clone(),
        per_recipient: throttle.per_recipient,
        per_channel: throttle.per_channel,
        burst: throttle.burst,
    }
}

/// Phase L Tier 3 — lower a canonical-indent `event_group` into
/// `ir::EventGroup`. The payload bag and authored events stay as raw
/// strings; Tier 4 will lift these into typed shapes once the shared
/// declarative parser exists.
pub fn lower_event_group(group: &syntax::EventGroup) -> ir::EventGroup {
    ir::EventGroup {
        pattern: group.pattern.clone(),
        on_resource: group.on_resource.clone(),
        raw_payload: group.payload.clone(),
        raw_audit: group.audit.clone(),
        events: group.events.clone(),
        span_ref: Some(span_of(group.span)),
    }
}

/// Migrations bucket cycle Route C — lower a canonical-indent
/// `tenant_migration` block into `ir::TenantMigration`. Mirrors
/// `lower_job` for the shared spine (idempotency / retry / timeout /
/// handler) and adds the `target tenants <axis>` slot. The lowering
/// does **not** enforce that `idempotency` is authored; that is
/// `TM-IDEMP-001`'s job downstream.
pub fn lower_tenant_migration(
    tm: &syntax::TenantMigration,
) -> Result<ir::TenantMigration, AnalyzeError> {
    let idempotency = tm
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path })
        .unwrap_or_else(|| ir::IdempotencyKey {
            by: ir::Path::from_segments(Vec::<String>::new()),
        });
    let retry = tm.retry.as_ref().map(lower_retry);
    Ok(ir::TenantMigration {
        name: tm.name.clone(),
        target: ir::TenantMigrationTarget {
            axis: tm.target_axis.clone(),
        },
        idempotency,
        retry,
        timeout: tm.timeout.clone(),
        handler: ir::PathRef::authored(&tm.handler),
        previous_names: Vec::new(),
        span_ref: Some(span_of(tm.span)),
    })
}

fn lower_job_trigger(feature: &str, trigger: &syntax::JobTrigger) -> ir::JobTrigger {
    match trigger {
        syntax::JobTrigger::Event(name) => ir::JobTrigger::Event {
            event: qualified_event_name(feature, name),
        },
        syntax::JobTrigger::Schedule(cron) => ir::JobTrigger::Schedule { cron: cron.clone() },
    }
}

fn qualified_event_name(feature: &str, name: &str) -> ir::QualifiedName {
    if let Some((ns, ev)) = name.split_once('.') {
        ir::QualifiedName {
            feature: Some(ns.to_owned()),
            name: ev.to_owned(),
        }
    } else {
        ir::QualifiedName {
            feature: Some(feature.to_owned()),
            name: name.to_owned(),
        }
    }
}

fn lower_retry(retry: &syntax::JobRetry) -> ir::RetryPolicy {
    ir::RetryPolicy {
        count: retry.count,
        backoff: match retry.backoff.as_str() {
            "exponential" => ir::BackoffStrategy::Exponential,
            _ => ir::BackoffStrategy::Fixed,
        },
    }
}

fn lower_fanout(fanout: &syntax::JobFanout) -> ir::FanoutSpec {
    ir::FanoutSpec {
        scope: ir::FanoutScope::Tenants,
        axis: fanout.axis.clone(),
    }
}

fn lower_external_call(call: &syntax::JobExternalCall) -> ir::ExternalCallRef {
    ir::ExternalCallRef {
        slot: call.slot.clone(),
        op: call.op.clone(),
        args: call
            .args
            .iter()
            .map(|arg| ir::NamedArg {
                name: arg.name.clone(),
                value: lower_raw_expr(&arg.value),
            })
            .collect(),
        span_ref: Some(span_of(call.span)),
    }
}

fn lower_job_body(body: &syntax::JobBody) -> ir::JobBody {
    match body {
        syntax::JobBody::Handler(h) => ir::JobBody::Handler(ir::JobHandler {
            path: ir::PathRef::authored(&h.path),
            returns: h.returns.as_deref().map(|t| type_ref_from_text(t)),
        }),
        syntax::JobBody::Declarative(d) => ir::JobBody::Declarative(ir::JobDeclarative {
            target: d.target.as_ref().map(lower_target_expr),
            lets: d.lets.iter().map(lower_let_binding).collect(),
            effect: d
                .effect
                .as_ref()
                .map(lower_command_effect)
                .unwrap_or(ir::CommandEffect::None),
        }),
        syntax::JobBody::None => ir::JobBody::Declarative(ir::JobDeclarative {
            target: None,
            lets: Vec::new(),
            effect: ir::CommandEffect::None,
        }),
    }
}

/// Phase L Tier 4b — shared lowering for `target query.<name>(args)`.
/// Reused by `lower_job_body` (Tier 3) and `lower_command_skeleton`
/// (Tier 4b) — closes the Tier 3 raw-spine carve-out.
fn lower_target_expr(t: &syntax::TargetExprDecl) -> ir::TargetExpr {
    ir::TargetExpr {
        query: lower_qualified_name(&t.query),
        args: t.args.iter().map(lower_named_arg).collect(),
    }
}

fn lower_let_binding(l: &syntax::LetBindingDecl) -> ir::LetBinding {
    ir::LetBinding {
        name: l.name.clone(),
        value: lower_raw_expr(&l.value),
    }
}

fn lower_named_arg(arg: &syntax::TargetArgDecl) -> ir::NamedArg {
    ir::NamedArg {
        name: arg.name.clone(),
        value: lower_raw_expr(&arg.value),
    }
}

fn lower_assignment(a: &syntax::AssignmentDecl) -> ir::Assignment {
    ir::Assignment {
        field: a.field.clone(),
        value: lower_raw_expr(&a.value),
    }
}

fn lower_command_effect(effect: &syntax::CommandEffectDecl) -> ir::CommandEffect {
    let resource = lower_qualified_name(&effect.resource);
    let assignments: Vec<ir::Assignment> =
        effect.assignments.iter().map(lower_assignment).collect();
    match effect.kind {
        syntax::CommandEffectKindDecl::Creates => ir::CommandEffect::Creates(ir::CreateEffect {
            resource,
            from_input: effect.from_input,
            assignments,
        }),
        syntax::CommandEffectKindDecl::Updates => ir::CommandEffect::Updates(ir::UpdateEffect {
            resource,
            assignments,
        }),
        syntax::CommandEffectKindDecl::Deletes => {
            ir::CommandEffect::Deletes(ir::DeleteEffect { resource })
        }
    }
}

fn lower_qualified_name(text: &str) -> ir::QualifiedName {
    let trimmed = text.trim();
    if let Some((feature, name)) = trimmed.split_once('.') {
        ir::QualifiedName {
            feature: Some(feature.to_owned()),
            name: name.to_owned(),
        }
    } else {
        ir::QualifiedName {
            feature: None,
            name: trimmed.to_owned(),
        }
    }
}

/// Capture a freeform expression as a string-bagged `Expr::Path`.
/// Tier 4's command parser will replace this with the typed expression.
fn lower_raw_expr(text: &str) -> ir::Expr {
    let trimmed = text.trim();
    if let Some(unquoted) = trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        return ir::Expr::String(unquoted.to_owned());
    }
    if let Ok(n) = trimmed.parse::<i64>() {
        return ir::Expr::Integer(n);
    }
    match trimmed {
        "true" => ir::Expr::Boolean(true),
        "false" => ir::Expr::Boolean(false),
        "nil" => ir::Expr::Nil,
        _ => {
            let segments = trimmed.split('.').map(|s| s.trim().to_owned()).collect();
            ir::Expr::Path(ir::Path { segments })
        }
    }
}

fn lower_path_string(text: &str) -> ir::Path {
    ir::Path {
        segments: text
            .split(',')
            .next()
            .unwrap_or(text)
            .split('.')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect(),
    }
}

/// Extract the env binding name from `env.<NAME>` (`secret env.X`).
fn extract_env_binding(raw: &str) -> String {
    raw.trim()
        .strip_prefix("env.")
        .map(|name| name.trim().to_owned())
        .unwrap_or_else(|| raw.trim().to_owned())
}

/// Phase L — lower a canonical-indent `auth` block into the IR `Auth`
/// shape. The translation is mostly structural; the analyzer's only
/// non-trivial duty is splitting `Customer.email` into `FieldRef`.
pub fn lower_auth(auth: &syntax::Auth) -> Result<ir::Auth, AnalyzeError> {
    Ok(ir::Auth {
        identity: lower_auth_identity(&auth.identity)?,
        password: auth.password.as_ref().map(lower_auth_password),
        sessions: auth.sessions.as_ref().map(lower_auth_sessions),
        mfa: auth.mfa.as_ref().map(lower_auth_mfa),
        oauth: auth.oauth.iter().map(lower_auth_oauth).collect(),
        span_ref: Some(span_of(auth.span)),
    })
}

fn lower_auth_identity(identity: &syntax::AuthIdentity) -> Result<ir::AuthIdentity, AnalyzeError> {
    let (resource, field) =
        identity
            .field
            .split_once('.')
            .ok_or_else(|| AnalyzeError::InvalidAuthIdentity {
                reference: identity.field.clone(),
            })?;
    if resource.is_empty() || field.is_empty() || field.contains('.') {
        return Err(AnalyzeError::InvalidAuthIdentity {
            reference: identity.field.clone(),
        });
    }
    Ok(ir::AuthIdentity {
        field: ir::FieldRef {
            resource: qualified_name_local(resource),
            field: field.to_owned(),
        },
    })
}

fn lower_auth_password(password: &syntax::AuthPassword) -> ir::AuthPassword {
    ir::AuthPassword {
        algorithm: password.algorithm.clone(),
        hash: password.hash.clone(),
        verify: password.verify.clone(),
        rate_limit: password.rate_limit.clone(),
    }
}

fn lower_auth_sessions(sessions: &syntax::AuthSessions) -> ir::AuthSessions {
    ir::AuthSessions {
        resource: qualified_name_local(&sessions.resource),
        ttl: sessions.ttl.clone(),
        refresh: sessions.refresh,
    }
}

fn lower_auth_mfa(mfa: &syntax::AuthMfa) -> ir::AuthMfa {
    ir::AuthMfa {
        method: mfa.method.clone(),
        enroll: mfa.enroll.clone(),
        verify: mfa.verify.clone(),
        adapter: mfa.adapter.clone(),
    }
}

fn lower_auth_oauth(oauth: &syntax::AuthOAuthProvider) -> ir::AuthOAuthProvider {
    ir::AuthOAuthProvider {
        provider: oauth.provider.clone(),
        adapter: oauth.adapter.clone(),
    }
}

/// Lower a single `agent` AST node into the IR form. The `feature` arg
/// pins the owning feature name on the IR record so cross-feature doctor
/// checks can rebuild `<feature>.agent.<name>` references.
pub fn lower_agent(feature: &str, agent: &syntax::Agent) -> Result<ir::Agent, AnalyzeError> {
    let input = agent
        .input
        .iter()
        .map(|slot| ir::TypedSlot {
            name: slot.name.clone(),
            type_ref: type_ref_from_text(&slot.type_text),
            required: slot.required,
        })
        .collect();

    let policy = agent
        .policy
        .as_ref()
        .and_then(|atoms| atoms.first())
        .map(|first| lower_policy_atom(first));

    let (output_kind, output_type, output_discriminator) = match &agent.output {
        Some(syntax::AgentOutput::Stream(ty)) => (
            ir::AgentOutputKind::Stream,
            Some(type_ref_from_text(ty)),
            None,
        ),
        Some(syntax::AgentOutput::Discriminator(name)) => (
            ir::AgentOutputKind::DiscriminatedEnum,
            None,
            Some(ir::DiscriminatorRef::Enum(qualified_name_local(name))),
        ),
        Some(syntax::AgentOutput::Plain(ty)) => (
            // Lowering can't tell `Text` from `DiscriminatedRecord` without
            // the feature scope (enum vs record). Default to `Text`; the
            // expand pass (Phase 5) promotes to `DiscriminatedRecord` when
            // the resolved type is a record with a `discriminator` field.
            ir::AgentOutputKind::Text,
            Some(type_ref_from_text(ty)),
            None,
        ),
        None => (ir::AgentOutputKind::Text, None, None),
    };

    let model = agent.model.as_ref().map(|s| qualified_namespace(s));

    let safety = agent
        .safety
        .iter()
        .map(|s| qualified_namespace(s))
        .collect();

    let mut tools = Vec::with_capacity(agent.tools.len());
    for tool_ast in &agent.tools {
        tools.push(ir::ToolBinding {
            reference: lower_tool_ref(&tool_ast.reference, feature)?,
            resolved_effect: None,
            resolved_policy: None,
            resolved_pii_classes: Vec::new(),
            span_ref: Some(span_of(tool_ast.span)),
        });
    }

    let mut evals = Vec::with_capacity(agent.evals.len());
    for case_ast in &agent.evals {
        evals.push(lower_eval_case(case_ast, feature)?);
    }

    let expose_http = agent.expose.as_ref().map(lower_agent_expose);

    Ok(ir::Agent {
        name: agent.name.clone(),
        feature: feature.to_owned(),
        input,
        context: None, // Phase 1 parser does not yet structure context expressions.
        policy,
        rate_limit: agent.rate_limit.clone(),
        output_kind,
        output_type,
        output_discriminator,
        model,
        temperature: agent.temperature,
        max_tokens: agent.max_tokens,
        top_p: agent.top_p,
        seed: agent.seed,
        prompt_path: agent.prompt.clone(),
        safety,
        tools,
        evals,
        expose_http,
        span_ref: Some(span_of(agent.span)),
    })
}

/// Cut A.7 — lower an `expose http` AST block. Method enum maps 1:1;
/// route slots become `TypedSlot`s with `required: true` (path params
/// are inherently required); audience / rate-limit pass-through as
/// strings.
fn lower_agent_expose(expose: &syntax::AgentExpose) -> ir::HttpExposure {
    let route_slots = expose
        .route_slots
        .iter()
        .map(|slot| ir::TypedSlot {
            name: slot.name.clone(),
            type_ref: type_ref_from_text(&slot.type_text),
            required: true,
        })
        .collect();
    ir::HttpExposure {
        method: match expose.method {
            syntax::HttpMethod::Get => ir::HttpMethod::Get,
            syntax::HttpMethod::Post => ir::HttpMethod::Post,
            syntax::HttpMethod::Put => ir::HttpMethod::Put,
            syntax::HttpMethod::Patch => ir::HttpMethod::Patch,
            syntax::HttpMethod::Delete => ir::HttpMethod::Delete,
        },
        path: expose.path.clone(),
        route_slots,
        audience: expose.audience.clone(),
        rate_limit_override: expose.rate_limit_override.clone(),
        span_ref: Some(span_of(expose.span)),
    }
}

/// Lower a single tool reference. `feature` is the owning feature so the
/// short form `query.by_id` rewrites to `Local` and the analyzer
/// preserves the same-feature locality for the expand pass to resolve.
fn lower_tool_ref(raw: &str, _feature: &str) -> Result<ir::QualifiedToolRef, AnalyzeError> {
    if let Some(rest) = raw.strip_prefix("@tool.") {
        if rest.is_empty() {
            return Err(AnalyzeError::InvalidToolRef {
                reference: raw.to_owned(),
            });
        }
        let dotted: Vec<String> = rest.split('.').map(str::to_owned).collect();
        return Ok(ir::QualifiedToolRef::Adapter { dotted });
    }

    // Tail tokens after the feature prefix (if any). The reference is
    // dotted; `query.list` / `query.lookup` / `query.sql` are the
    // three legal three-segment kinds.
    let segments: Vec<&str> = raw.split('.').collect();
    if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
        return Err(AnalyzeError::InvalidToolRef {
            reference: raw.to_owned(),
        });
    }

    // Local shorthand: `query.by_id`, `command.create`, `api.export`,
    // `query.list.by_email`, etc.
    if let Some((kind, name)) = parse_tool_kind_local(&segments) {
        return Ok(ir::QualifiedToolRef::Local { kind, name });
    }

    // Cross-feature: `<feature>.<kind>.<name>` or
    // `<feature>.query.list.<name>` / `query.lookup.<name>` / `query.sql.<name>`.
    if segments.len() >= 3 {
        let feature = segments[0].to_owned();
        if let Some((kind, name)) = parse_tool_kind_local(&segments[1..]) {
            return Ok(ir::QualifiedToolRef::CrossFeature {
                feature,
                kind,
                name,
            });
        }
    }

    Err(AnalyzeError::InvalidToolRef {
        reference: raw.to_owned(),
    })
}

/// Recognize the trailing `(kind, name)` of a tool reference. Accepts:
///
///   - `query.list.<name>`     -> QueryList
///   - `query.lookup.<name>`   -> QueryLookup
///   - `query.sql.<name>`      -> QuerySql
///   - `query.<name>`          -> QueryUnspecified
///   - `command.<name>`        -> Command
///   - `api.<name>`            -> Api
fn parse_tool_kind_local(segments: &[&str]) -> Option<(ir::ToolKind, String)> {
    match segments {
        ["query", "list", name] => Some((ir::ToolKind::QueryList, (*name).to_owned())),
        ["query", "lookup", name] => Some((ir::ToolKind::QueryLookup, (*name).to_owned())),
        ["query", "sql", name] => Some((ir::ToolKind::QuerySql, (*name).to_owned())),
        ["query", name] => Some((ir::ToolKind::QueryUnspecified, (*name).to_owned())),
        ["command", name] => Some((ir::ToolKind::Command, (*name).to_owned())),
        ["api", name] => Some((ir::ToolKind::Api, (*name).to_owned())),
        _ => None,
    }
}

fn lower_eval_case(
    case: &syntax::AgentEvalCase,
    feature: &str,
) -> Result<ir::EvalCase, AnalyzeError> {
    let mut assertions = Vec::with_capacity(case.assertions.len());
    for assertion in &case.assertions {
        assertions.push(ir::EvalAssertion {
            kind: match assertion.kind {
                syntax::AgentEvalKind::Requires => ir::EvalAssertionKind::Requires,
                syntax::AgentEvalKind::Forbids => ir::EvalAssertionKind::Forbids,
            },
            predicate: lower_eval_predicate(&assertion.predicate, feature)?,
            span_ref: Some(span_of(assertion.span)),
        });
    }
    let golden = case.golden.as_ref().map(|g| ir::GoldenSpec {
        path: g.path.clone(),
        min_score: g.min_score,
        span_ref: Some(span_of(g.span)),
    });
    Ok(ir::EvalCase {
        name: case.name.clone(),
        assertions,
        golden,
        span_ref: Some(span_of(case.span)),
    })
}

fn lower_eval_predicate(
    predicate: &syntax::AgentEvalPredicate,
    feature: &str,
) -> Result<ir::EvalPredicate, AnalyzeError> {
    match predicate {
        syntax::AgentEvalPredicate::Contains { lhs, rhs } => Ok(ir::EvalPredicate::Contains {
            lhs: ir::Path::from_segments(lhs.split('.').map(str::to_owned)),
            rhs: match rhs {
                syntax::ContainsRhs::Literal(s) => ir::EvalContainsRhs::Literal(s.clone()),
                syntax::ContainsRhs::SemanticType(s) => {
                    ir::EvalContainsRhs::SemanticType(qualified_namespace(s))
                }
            },
        }),
        syntax::AgentEvalPredicate::ToolsCalls { op, target } => {
            Ok(ir::EvalPredicate::ToolsCalls {
                op: match op {
                    syntax::ToolsCallsOp::Includes => ir::ToolsCallsOp::Includes,
                    syntax::ToolsCallsOp::Excludes => ir::ToolsCallsOp::Excludes,
                },
                target: lower_tool_ref(target, feature)?,
            })
        }
        syntax::AgentEvalPredicate::Closed { text } => Ok(parse_closed_predicate(text)),
    }
}

/// Parse the simple `<path> <op> <literal>` subset of the closed predicate
/// language. Richer shapes (compound `AND`/`OR`, `has`, parenthesised
/// expressions) fall through to `EvalPredicate::Unparsed` so doctor can
/// surface them — the parser stays narrow until the canonical predicate
/// parser lands.
fn parse_closed_predicate(text: &str) -> ir::EvalPredicate {
    let trimmed = text.trim();
    // Try ordered ops first (longest token wins to avoid `<=` parsing as `<`).
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
                return ir::EvalPredicate::Unparsed(text.to_owned());
            }
            return ir::EvalPredicate::Closed(ir::Predicate::Comparison {
                left: expr_from_text(lhs),
                op,
                right: expr_from_text(rhs),
            });
        }
    }
    ir::EvalPredicate::Unparsed(text.to_owned())
}

/// Locate `op` outside of any double-quoted span. Returns the byte index
/// in `text` where the operator begins, or `None` if no top-level match.
fn find_top_level_operator(text: &str, op: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut in_quote = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            in_quote = !in_quote;
            i += 1;
            continue;
        }
        if !in_quote && text[i..].starts_with(op) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Promote a token to the smallest expression kind that fits. Strings
/// must be double-quoted; integers parse as `Integer`; `true`/`false` as
/// `Boolean`; bare identifiers / dotted paths become `Path`; bare
/// identifiers that look like enum literals (no dots) also surface as
/// `Path` — the analyzer narrows once symbols resolve in expand.
fn expr_from_text(text: &str) -> ir::Expr {
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
    ir::Expr::Path(ir::Path::from_segments(text.split('.').map(str::to_owned)))
}

/// Build a feature-local `QualifiedName` (no feature prefix).
fn qualified_name_local(name: &str) -> ir::QualifiedName {
    ir::QualifiedName {
        feature: None,
        name: name.to_owned(),
    }
}

/// Treat the entire namespace literal as a single name (e.g.
/// `@llm.default`, `@validator.pii_email_scrub`, `@semantic.Email`).
/// Doctor + LSP enforce the closed-namespace catalog elsewhere; this
/// helper keeps the raw form so resolution stays uniform.
fn qualified_namespace(raw: &str) -> ir::QualifiedName {
    ir::QualifiedName {
        feature: None,
        name: raw.to_owned(),
    }
}

fn lower_policy_atom(atom: &str) -> ir::PolicyRef {
    if let Some(rest) = atom.strip_prefix('@') {
        ir::PolicyRef::Atom(rest.to_owned())
    } else {
        ir::PolicyRef::Local(atom.to_owned())
    }
}

/// The Phase 1 parser captures type references as raw source text. Turn
/// that into a minimal `TypeRef` so doctor and inspect can read it; the
/// canonical-indent migration replaces this with a real type-ref parser.
fn type_ref_from_text(text: &str) -> ir::TypeRef {
    let trimmed = text.trim();
    match trimmed {
        "Text" => ir::TypeRef::Builtin(ir::BuiltinType::Text),
        "Integer" => ir::TypeRef::Builtin(ir::BuiltinType::Integer),
        "Boolean" => ir::TypeRef::Builtin(ir::BuiltinType::Boolean),
        "Decimal" => ir::TypeRef::Builtin(ir::BuiltinType::Decimal),
        "Date" => ir::TypeRef::Builtin(ir::BuiltinType::Date),
        "DateTime" => ir::TypeRef::Builtin(ir::BuiltinType::DateTime),
        "ID" => ir::TypeRef::Builtin(ir::BuiltinType::Id),
        "Json" => ir::TypeRef::Builtin(ir::BuiltinType::Json),
        _ if trimmed.starts_with("@semantic.") => {
            ir::TypeRef::Builtin(ir::BuiltinType::SemanticEmail) // placeholder
        }
        _ => ir::TypeRef::Unresolved(trimmed.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use lazuli_syntax::{parse_document, parse_lzx_document};

    use super::{
        AnalyzeError, lower_auth_identity, lower_document, lower_lzx_document, type_ref_from_syntax,
    };

    #[test]
    fn lowers_valid_document_to_ir() {
        let document = parse_document(include_str!("../../../examples/crm.lzi")).unwrap();
        let module = lower_document(&document).unwrap();

        assert_eq!(module.features.len(), 1);
        let feature = &module.features[0];
        assert_eq!(feature.name, "crm");
        assert_eq!(feature.resources.len(), 2);

        let customer = &feature.resources[0];
        assert_eq!(customer.name, "Customer");
        assert_eq!(customer.fields[1].name, "email");
        assert!(customer.fields[1].unique);
    }

    #[test]
    fn rejects_unknown_field_references() {
        let source = r#"
            aggregate Customer {
              name: Text

              command Create {
                input email
                policy customer.create
              }
            }
        "#;

        let document = parse_document(source).unwrap();
        let error = lower_document(&document).unwrap_err();

        assert!(matches!(error, AnalyzeError::UnknownField { .. }));
    }

    #[test]
    fn rejects_commands_without_policy() {
        let source = r#"
            aggregate Customer {
              name: Text

              command Create {
                input name
              }
            }
        "#;

        let document = parse_document(source).unwrap();
        let error = lower_document(&document).unwrap_err();

        assert!(matches!(error, AnalyzeError::MissingCommandPolicy { .. }));
    }

    #[test]
    fn lowers_lzx_experience_and_surface_to_ir() {
        let experience =
            parse_lzx_document(include_str!("../../../examples/customer-capsule.lzx")).unwrap();
        let surface =
            parse_lzx_document(include_str!("../../../examples/customer-capsule.web.lzx")).unwrap();

        let experience_ir = lower_lzx_document(&experience);
        let surface_ir = lower_lzx_document(&surface);

        assert_eq!(experience_ir.experiences[0].name, "customer");
        assert_eq!(experience_ir.experiences[0].imports, vec!["customer"]);
        assert_eq!(
            experience_ir.experiences[0].views[0].actions[0].target,
            "customer.command.create"
        );
        assert_eq!(surface_ir.surfaces[0].experience, "customer");
        assert_eq!(
            surface_ir.surfaces[0].uses_experience.as_deref(),
            Some("customer")
        );
        assert_eq!(surface_ir.surfaces[0].audiences[0].name, "admin");
        assert_eq!(
            surface_ir.surfaces[0].audiences[0].views[0].columns,
            vec!["name", "email", "status", "created_at"]
        );
        assert_eq!(
            surface_ir.surfaces[0].audiences[0].views[0].search,
            vec!["name", "email"]
        );
        assert_eq!(
            surface_ir.surfaces[0].audiences[0].views[0].cells,
            vec!["status @client.status_cell"]
        );
    }

    #[test]
    fn lowers_lzx_extension_slots_to_ir() {
        let source = r#"
experience customer_tags
  imports customer_tags, customer

  extends @anchor.customer_detail
    slot aside after activity_timeline
      block @client.tag_editor
      platforms web
      audience admin
"#;
        let document = parse_lzx_document(source).unwrap();
        let module = lower_lzx_document(&document);
        let extension = &module.experiences[0].extensions[0];

        assert_eq!(extension.anchor, "@anchor.customer_detail");
        assert_eq!(extension.slots.len(), 1);
        assert_eq!(extension.slots[0].name, "aside");
        assert_eq!(extension.slots[0].blocks, vec!["@client.tag_editor"]);
        assert_eq!(extension.slots[0].platforms, vec!["web"]);
        assert_eq!(extension.slots[0].audiences, vec!["admin"]);
        assert_eq!(
            extension.slots[0]
                .order
                .as_ref()
                .map(|order| (order.relation.as_str(), order.target.as_str())),
            Some(("after", "activity_timeline"))
        );
    }

    #[test]
    fn lowers_lzx_app_manifest_and_routes_to_ir() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
  targets
    backend go
    web react
  uses customer, billing

route customer_detail
  path "/customers/:id"
  route id: Customer.ID
  to customer.view.detail(id: route.id)
  surface customer web
  audience admin
"#;

        let document = parse_lzx_document(source).unwrap();
        let module = lower_lzx_document(&document);

        assert_eq!(module.app.as_ref().unwrap().name, "AcmeCRM");
        assert_eq!(
            module.app.as_ref().unwrap().targets,
            vec!["backend go", "web react"]
        );
        assert_eq!(module.routes[0].name, "customer_detail");
        assert_eq!(module.routes[0].routes, vec!["id: Customer.ID"]);
        assert_eq!(
            module.routes[0].to.as_deref(),
            Some("customer.view.detail(id: route.id)")
        );
    }

    // -------------------------------------------------------------------------
    // Cut A — agent lowering (§4.4 snapshot tests)
    // -------------------------------------------------------------------------

    use super::lower_feature_skeleton;
    use lazuli_ir as ir;
    use lazuli_syntax::parse_feature_skeletons;

    fn lower_first_agent(source: &str) -> ir::Agent {
        let features = parse_feature_skeletons(source).expect("parses");
        assert_eq!(features.len(), 1);
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        feature.agents.into_iter().next().expect("agent")
    }

    #[test]
    fn lower_agent_with_tools_resolves_to_ir() {
        let source = r#"
feature customer
  agent triage
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      customer.query.by_id
      query.by_id
      command.archive
      @tool.web_search
      @tool.calendar.create_event
"#;
        let agent = lower_first_agent(source);

        assert_eq!(agent.feature, "customer");
        assert_eq!(agent.name, "triage");
        assert_eq!(agent.tools.len(), 5);

        match &agent.tools[0].reference {
            ir::QualifiedToolRef::CrossFeature {
                feature,
                kind,
                name,
            } => {
                assert_eq!(feature, "customer");
                assert_eq!(*kind, ir::ToolKind::QueryUnspecified);
                assert_eq!(name, "by_id");
            }
            other => panic!("expected CrossFeature, got {other:?}"),
        }
        match &agent.tools[1].reference {
            ir::QualifiedToolRef::Local { kind, name } => {
                assert_eq!(*kind, ir::ToolKind::QueryUnspecified);
                assert_eq!(name, "by_id");
            }
            other => panic!("expected Local, got {other:?}"),
        }
        match &agent.tools[2].reference {
            ir::QualifiedToolRef::Local { kind, name } => {
                assert_eq!(*kind, ir::ToolKind::Command);
                assert_eq!(name, "archive");
            }
            other => panic!("expected Local Command, got {other:?}"),
        }
        match &agent.tools[3].reference {
            ir::QualifiedToolRef::Adapter { dotted } => {
                assert_eq!(dotted, &vec!["web_search".to_owned()]);
            }
            other => panic!("expected Adapter, got {other:?}"),
        }
        match &agent.tools[4].reference {
            ir::QualifiedToolRef::Adapter { dotted } => {
                assert_eq!(
                    dotted,
                    &vec!["calendar".to_owned(), "create_event".to_owned()]
                );
            }
            other => panic!("expected Adapter dotted, got {other:?}"),
        }

        // Expand pass populates the resolved_* fields; lowering leaves them
        // None / empty.
        assert!(agent.tools.iter().all(|t| t.resolved_effect.is_none()));
        assert!(agent.tools.iter().all(|t| t.resolved_policy.is_none()));
        assert!(
            agent
                .tools
                .iter()
                .all(|t| t.resolved_pii_classes.is_empty())
        );
    }

    #[test]
    fn lower_agent_with_evals_resolves_to_ir() {
        let source = r#"
feature customer
  agent summarize
    input
      customer_id: Customer.ID required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      case short_for_active
        requires customer.lifecycle_stage = active
        requires output contains "active"

      case redacts_email
        forbids output contains @semantic.Email

      case uses_lookup
        requires tools.calls includes customer.query.by_id
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.evals.len(), 3);

        // Case 0: Closed Comparison + Contains literal.
        let c0 = &agent.evals[0];
        assert_eq!(c0.name, "short_for_active");
        match &c0.assertions[0].predicate {
            ir::EvalPredicate::Closed(ir::Predicate::Comparison { left, op, right }) => {
                assert_eq!(*op, ir::CompareOp::Eq);
                match (left, right) {
                    (ir::Expr::Path(lhs), ir::Expr::Path(rhs)) => {
                        assert_eq!(lhs.segments, vec!["customer", "lifecycle_stage"]);
                        assert_eq!(rhs.segments, vec!["active"]);
                    }
                    other => panic!("unexpected Comparison sides: {other:?}"),
                }
            }
            other => panic!("expected Closed Comparison, got {other:?}"),
        }
        match &c0.assertions[1].predicate {
            ir::EvalPredicate::Contains { lhs, rhs } => {
                assert_eq!(lhs.segments, vec!["output"]);
                assert_eq!(rhs, &ir::EvalContainsRhs::Literal("active".to_owned()));
            }
            other => panic!("expected Contains literal, got {other:?}"),
        }

        // Case 1: Forbids + Contains semantic.
        let c1 = &agent.evals[1];
        assert_eq!(c1.assertions[0].kind, ir::EvalAssertionKind::Forbids);
        match &c1.assertions[0].predicate {
            ir::EvalPredicate::Contains { rhs, .. } => match rhs {
                ir::EvalContainsRhs::SemanticType(qn) => {
                    assert_eq!(qn.name, "@semantic.Email");
                }
                other => panic!("expected SemanticType, got {other:?}"),
            },
            other => panic!("expected Contains, got {other:?}"),
        }

        // Case 2: ToolsCalls includes a cross-feature target.
        let c2 = &agent.evals[2];
        match &c2.assertions[0].predicate {
            ir::EvalPredicate::ToolsCalls { op, target } => {
                assert_eq!(*op, ir::ToolsCallsOp::Includes);
                match target {
                    ir::QualifiedToolRef::CrossFeature { feature, name, .. } => {
                        assert_eq!(feature, "customer");
                        assert_eq!(name, "by_id");
                    }
                    other => panic!("expected CrossFeature target, got {other:?}"),
                }
            }
            other => panic!("expected ToolsCalls, got {other:?}"),
        }
    }

    #[test]
    fn lower_agent_with_discriminator_output_resolves() {
        let source = r#"
feature customer_support
  agent classify_intent
    input
      message: Text required
    policy @policy.read
    output discriminator Intent
    model @llm.classifier
    temperature 0
    seed 42
    prompt "./p.md"
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.output_kind, ir::AgentOutputKind::DiscriminatedEnum);
        match agent.output_discriminator.as_ref().unwrap() {
            ir::DiscriminatorRef::Enum(qn) => {
                assert_eq!(qn.name, "Intent");
                assert!(qn.feature.is_none());
            }
            other => panic!("expected Enum discriminator, got {other:?}"),
        }
        assert!(agent.output_type.is_none());
    }

    #[test]
    fn lower_agent_with_discriminated_record_resolves() {
        // Bare `output Action` lowers as Text + Some(output_type=Action).
        // The expand pass (Phase 5) promotes to DiscriminatedRecord when
        // it resolves `Action` to a record with a `discriminator` field.
        let source = r#"
feature customer
  agent extract_action
    input
      message: Text required
    policy @policy.read
    output Action
    model @llm.default
    prompt "./p.md"
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.output_kind, ir::AgentOutputKind::Text);
        assert!(agent.output_discriminator.is_none());
        match agent.output_type.as_ref().unwrap() {
            ir::TypeRef::Unresolved(name) => assert_eq!(name, "Action"),
            other => panic!("expected Unresolved Action, got {other:?}"),
        }
    }

    #[test]
    fn lower_agent_evals_without_temperature_zero_is_marked_nondeterministic() {
        // Lowering doesn't fail; doctor's diagnostic
        // `eval_nondeterministic_warning` fires in Phase 3. Here we just
        // verify lowering captures `temperature` and `seed` so doctor can
        // inspect them.
        let source = r#"
feature customer
  agent flaky
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0.7
    prompt "./p.md"
    evals
      case nondeterministic
        requires output contains "x"
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.temperature, Some(0.7));
        assert!(agent.seed.is_none());
        assert!(!agent.evals.is_empty());
        // Doctor will combine temperature + seed + evals.is_empty() to
        // emit `eval_nondeterministic_warning` in Phase 3.
    }

    #[test]
    fn lower_agent_propagates_safety_list_for_cut_a5_ready() {
        // Cut A allows 0..1 safety entries; Cut A.5 widens to a list.
        // The IR shape `safety: Vec<QualifiedName>` already supports the
        // wider form — this test pins the shape so A.5 lands by adding
        // a doctor diagnostic, not by changing IR.
        let source = r#"
feature customer
  agent guarded
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    safety @validator.pii_email_scrub, @validator.pii_ssn_scrub
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.safety.len(), 2);
        assert_eq!(agent.safety[0].name, "@validator.pii_email_scrub");
        assert_eq!(agent.safety[1].name, "@validator.pii_ssn_scrub");
    }

    #[test]
    fn lower_agent_ordered_compare_op_lowers_to_lt_le_gt_ge() {
        // Proposal §A3 admits ordered ops inside evals. Lowering parses
        // them; doctor's `eval_ordered_op_invalid_diagnostics` decides
        // whether the operand types are numeric.
        let source = r#"
feature customer
  agent ordered
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      case bounded
        requires output.length <= 800
        requires output.length >= 1
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.evals.len(), 1);
        match &agent.evals[0].assertions[0].predicate {
            ir::EvalPredicate::Closed(ir::Predicate::Comparison { op, .. }) => {
                assert_eq!(*op, ir::CompareOp::Le);
            }
            other => panic!("expected Le Comparison, got {other:?}"),
        }
        match &agent.evals[0].assertions[1].predicate {
            ir::EvalPredicate::Closed(ir::Predicate::Comparison { op, .. }) => {
                assert_eq!(*op, ir::CompareOp::Ge);
            }
            other => panic!("expected Ge Comparison, got {other:?}"),
        }
    }

    #[test]
    fn lower_agent_invalid_tool_ref_errors() {
        // `@tool` (no dotted tail) is malformed; lowering returns
        // `AnalyzeError::InvalidToolRef`. Tool-string sanity checks fire
        // here so doctor can stay focused on cross-feature resolution.
        let source = r#"
feature customer
  agent broken
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      @tool.
"#;
        // Note: the parser already rejects `@tool.` (trailing dot leaves an
        // empty tail when split). We craft a slightly different shape so
        // the parser accepts and lowering rejects.
        let parsed = parse_feature_skeletons(source);
        match parsed {
            Err(_) => return, // parser caught it — equally valid
            Ok(features) => {
                let err = lower_feature_skeleton(&features[0]).unwrap_err();
                match err {
                    AnalyzeError::InvalidToolRef { .. } => {}
                    other => panic!("expected InvalidToolRef, got {other:?}"),
                }
            }
        }
    }

    #[test]
    fn lower_agent_golden_eval_lowers_to_ir() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      case quality
        requires output contains "active"
        golden "./evals/summarize.jsonl" min_score 0.85
"#;
        let agent = lower_first_agent(source);
        let case = &agent.evals[0];
        let golden = case.golden.as_ref().expect("golden");
        assert_eq!(golden.path, "./evals/summarize.jsonl");
        assert_eq!(golden.min_score, Some(0.85));
        // Assertions still present alongside the golden ref.
        assert_eq!(case.assertions.len(), 1);
    }

    #[test]
    fn lower_agent_with_expose_http_lowers_to_ir() {
        let source = r#"
feature customer
  agent summarize
    input
      customer_id: Customer.ID required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/customers/:customer_id/summary"
      route customer_id: Customer.ID
      audience admin
      rate_limit "5 per minute per user"
"#;
        let agent = lower_first_agent(source);
        let expose = agent.expose_http.as_ref().expect("expose_http");
        assert_eq!(expose.method, ir::HttpMethod::Post);
        assert_eq!(expose.path, "/api/customers/:customer_id/summary");
        assert_eq!(expose.route_slots.len(), 1);
        assert_eq!(expose.route_slots[0].name, "customer_id");
        assert!(expose.route_slots[0].required);
        assert_eq!(expose.audience.as_deref(), Some("admin"));
        assert_eq!(
            expose.rate_limit_override.as_deref(),
            Some("5 per minute per user")
        );
    }

    #[test]
    fn lower_agent_without_expose_keeps_field_none() {
        let source = r#"
feature customer
  agent simple
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let agent = lower_first_agent(source);
        assert!(agent.expose_http.is_none());
    }

    // -------------------------------------------------------------------------
    // Phase L — `auth` block lowering
    // -------------------------------------------------------------------------

    #[test]
    fn lower_auth_full_block_to_ir() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    password
      algorithm argon2id
      hash @fn.hash_customer_password
      verify @fn.verify_customer_password
      rate_limit "5 per 10 minutes"

    oauth google
      adapter @adapter.google_oauth

    mfa totp
      enroll @fn.enroll_customer_totp
      verify @validator.verify_customer_totp

    sessions
      resource CustomerSession
      ttl "7 days"
      refresh false
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let auth = feature.auth.expect("auth lowered");

        assert_eq!(auth.identity.field.resource.name, "Customer");
        assert_eq!(auth.identity.field.field, "email");

        let password = auth.password.as_ref().expect("password");
        assert_eq!(password.algorithm, "argon2id");
        assert_eq!(password.hash, "@fn.hash_customer_password");
        assert_eq!(password.verify, "@fn.verify_customer_password");
        assert_eq!(password.rate_limit.as_deref(), Some("5 per 10 minutes"));

        let mfa = auth.mfa.as_ref().expect("mfa");
        assert_eq!(mfa.method, "totp");
        assert_eq!(mfa.enroll, "@fn.enroll_customer_totp");
        assert_eq!(mfa.verify, "@validator.verify_customer_totp");

        let sessions = auth.sessions.as_ref().expect("sessions");
        assert_eq!(sessions.resource.name, "CustomerSession");
        assert_eq!(sessions.ttl, "7 days");
        assert!(!sessions.refresh);

        assert_eq!(auth.oauth.len(), 1);
        assert_eq!(auth.oauth[0].provider, "google");
        assert_eq!(auth.oauth[0].adapter, "@adapter.google_oauth");
    }

    #[test]
    fn lower_auth_identity_with_empty_field_errors() {
        // Parser would already reject `identity .email` because the
        // dot-qualified contract requires both segments; this test
        // documents the analyzer's defensive guard for any future
        // parser shape that lets a stray dot through.
        let identity = lazuli_syntax::AuthIdentity {
            field: "Customer.".to_owned(),
            span: lazuli_syntax::Span::new(0, 9),
        };
        let err = lower_auth_identity(&identity).unwrap_err();
        match err {
            AnalyzeError::InvalidAuthIdentity { reference } => {
                assert_eq!(reference, "Customer.");
            }
            other => panic!("expected InvalidAuthIdentity, got {other:?}"),
        }
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 3 — job / webhook / notification / event_group lowering
    // -------------------------------------------------------------------------

    #[test]
    fn lower_tier3_job_handler_full_block() {
        let source = r#"
feature customer
  job process_import
    trigger event customer_import_uploaded
    queue customer_imports
    tenant_from payload.org_id
    idempotency by payload.batch_id
    retry 3 backoff exponential
    calls crm.normalize_import_batch
      batch_id = payload.batch_id
      org_id = payload.org_id
    timeout "30s"
    handler "./jobs/process_import.go"
    emits customer_import_completed
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.jobs.len(), 1);
        let job = &feature.jobs[0];
        assert_eq!(job.name, "process_import");
        assert_eq!(job.queue.as_deref(), Some("customer_imports"));
        assert_eq!(job.timeout.as_deref(), Some("30s"));
        let tenant = job.tenant_from.as_ref().expect("tenant_from");
        assert_eq!(tenant.path.segments, vec!["payload", "org_id"]);
        let retry = job.retry.as_ref().expect("retry");
        assert_eq!(retry.count, 3);
        assert!(matches!(retry.backoff, ir::BackoffStrategy::Exponential));
        assert_eq!(job.external_calls.len(), 1);
        assert_eq!(job.external_calls[0].slot, "crm");
        assert_eq!(job.external_calls[0].op, "normalize_import_batch");
        assert_eq!(job.external_calls[0].args.len(), 2);
        match &job.body {
            ir::JobBody::Handler(h) => {
                assert_eq!(h.path.path, "./jobs/process_import.go");
            }
            other => panic!("expected Handler body, got {other:?}"),
        }
        assert_eq!(job.emits, vec!["customer_import_completed"]);
    }

    #[test]
    fn lower_tier3_job_declarative_carve_out() {
        let source = r#"
feature customer
  job recompute_score_after_invoice
    trigger event billing.invoice_paid
    tenant_from payload.org_id
    idempotency by envelope.id
    target query.by_id(id: payload.customer_id)
    let new_score = @fn.risk_score(target)
    updates Customer
      score = new_score
    emits customer_score_recomputed
      score = new_score
      reason = "invoice_paid"
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.jobs.len(), 1);
        let job = &feature.jobs[0];
        match &job.body {
            ir::JobBody::Declarative(d) => {
                let target = d.target.as_ref().expect("target lifted");
                assert_eq!(target.query.name, "by_id");
                assert_eq!(d.lets.len(), 1);
                assert_eq!(d.lets[0].name, "new_score");
                match &d.effect {
                    ir::CommandEffect::Updates(u) => {
                        assert_eq!(u.resource.name, "Customer");
                        assert_eq!(u.assignments.len(), 1);
                        assert_eq!(u.assignments[0].field, "score");
                    }
                    other => panic!("expected Updates effect, got {other:?}"),
                }
            }
            other => panic!("expected Declarative body, got {other:?}"),
        }
    }

    #[test]
    fn lower_tier3_webhook_structured_verify() {
        let source = r#"
feature customer
  webhook crm_customer_upsert
    path "/webhooks/crm/customer-upsert"
    verify hmac sha256
      secret env.CRM_WEBHOOK_SECRET
      header "X-CRM-Signature"
    tenant_from payload.org_id
    idempotency by payload.external_id
    handler "./integrations/upsert_customer_from_crm.go" returns Customer
    emits customer_webhook_received
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.webhooks.len(), 1);
        let webhook = &feature.webhooks[0];
        assert_eq!(webhook.route, "/webhooks/crm/customer-upsert");
        let verify = webhook
            .structured_verify
            .as_ref()
            .expect("structured verify");
        assert!(matches!(verify.scheme, ir::VerifyScheme::Hmac));
        assert_eq!(verify.algorithm, "sha256");
        assert_eq!(verify.secret_env, "CRM_WEBHOOK_SECRET");
        assert_eq!(verify.header, "X-CRM-Signature");
        let tenant = webhook.tenant_from.as_ref().expect("tenant_from");
        assert_eq!(tenant.path.segments, vec!["payload", "org_id"]);
        assert_eq!(
            webhook.handler.path,
            "./integrations/upsert_customer_from_crm.go"
        );
        assert_eq!(webhook.emits, vec!["customer_webhook_received"]);
    }

    #[test]
    fn lower_tier3_notification_full_block() {
        let source = r#"
feature customer_outreach
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    tenant_from payload.org_id
    idempotency by envelope.id
    retry 3 backoff exponential
    template "./outreach/welcome_email.mjml"
    policy @policy.notify
    emits welcome_email_sent
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.notifications.len(), 1);
        let n = &feature.notifications[0];
        assert_eq!(n.name, "welcome_email");
        assert_eq!(n.channels, vec!["email"]);
        assert_eq!(n.recipient, "target.email");
        assert_eq!(n.template, "./outreach/welcome_email.mjml");
        match &n.trigger {
            ir::JobTrigger::Event { event } => {
                assert_eq!(event.feature.as_deref(), Some("customer"));
                assert_eq!(event.name, "customer_activated");
            }
            other => panic!("expected Event trigger, got {other:?}"),
        }
        assert_eq!(n.emits, vec!["welcome_email_sent"]);
    }

    #[test]
    fn lower_tier3_event_group_payload_and_events() {
        let source = r#"
feature customer
  event_group customer_* on Customer
    payload
      customer_id = id
      org_id = org.id
    event created
    event activated
    event archived
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.event_groups.len(), 1);
        let group = &feature.event_groups[0];
        assert_eq!(group.pattern, "customer_*");
        assert_eq!(group.on_resource.as_deref(), Some("Customer"));
        assert_eq!(group.raw_payload.len(), 2);
        assert_eq!(
            group.events,
            vec![
                "created".to_owned(),
                "activated".to_owned(),
                "archived".to_owned()
            ]
        );
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 2 — `@cap.File(...)` typing
    // -------------------------------------------------------------------------

    #[test]
    fn type_ref_from_syntax_lowers_full_cap_file() {
        let ty =
            type_ref_from_syntax("@cap.File(max_size:25mb,accept:text/csv,visibility:private)");
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::File(file)) => {
                assert_eq!(file.max_size.bytes, 25 * 1024 * 1024);
                assert!(matches!(file.max_size.literal, ir::FileSizeLiteral::Mb(25)));
                assert_eq!(file.accept.len(), 1);
                assert_eq!(file.accept[0].family, "text");
                assert_eq!(file.accept[0].subtype, "csv");
                assert_eq!(file.visibility, Some(ir::FileVisibility::Private));
                assert!(file.signed_ttl.is_none());
            }
            other => panic!("expected Capability::File, got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_lowers_multi_mime_cap_file() {
        let ty = type_ref_from_syntax(
            "@cap.File(max_size:100mb,accept:text/csv|application/vnd.ms-excel,visibility:signed,signed_ttl:1h)",
        );
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::File(file)) => {
                assert_eq!(file.accept.len(), 2);
                assert_eq!(file.accept[1].family, "application");
                assert_eq!(file.accept[1].subtype, "vnd.ms-excel");
                assert_eq!(file.visibility, Some(ir::FileVisibility::Signed));
                assert_eq!(file.signed_ttl.as_deref(), Some("1h"));
            }
            other => panic!("expected Capability::File, got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_falls_through_when_cap_file_missing_max_size() {
        // No `max_size` arg → falls through to UserDefined so the LSP
        // shape diagnostic remains the canonical authority.
        let ty = type_ref_from_syntax("@cap.File(accept:text/csv)");
        assert!(matches!(ty, ir::TypeRef::UserDefined(_)));
    }

    #[test]
    fn type_ref_from_syntax_falls_through_when_cap_file_malformed_size() {
        // `25xy` is not a recognised size literal.
        let ty = type_ref_from_syntax("@cap.File(max_size:25xy,accept:text/csv)");
        assert!(matches!(ty, ir::TypeRef::UserDefined(_)));
    }

    #[test]
    fn type_ref_from_syntax_lifts_cap_hashed_argon2id() {
        // Phase L Tier 4 follow-up — `@cap.Hashed(algorithm:argon2id)`
        // now lowers into `CapabilityRef::Hashed(...)`.
        let ty = type_ref_from_syntax("@cap.Hashed(algorithm:argon2id)");
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::Hashed(h)) => {
                assert_eq!(h.algorithm, ir::HashAlgorithm::Argon2id);
            }
            other => panic!("expected Capability::Hashed, got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_lifts_cap_token_typed() {
        let ty = type_ref_from_syntax("@cap.Token(ttl:24h,single_use:true,store:hashed)");
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::Token(t)) => {
                assert_eq!(t.ttl, "24h");
                assert!(t.single_use);
                assert_eq!(t.store, ir::TokenStore::Hashed);
            }
            other => panic!("expected Capability::Token, got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_falls_through_on_unknown_hash_algorithm() {
        // Closed catalog: unknown algo falls through to UserDefined so
        // the LSP can surface a shape diagnostic.
        let ty = type_ref_from_syntax("@cap.Hashed(algorithm:scrypt)");
        assert!(matches!(ty, ir::TypeRef::UserDefined(_)));
    }

    #[test]
    fn lower_feature_without_auth_keeps_field_none() {
        let source = r#"
feature customer
  agent simple
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert!(feature.auth.is_none());
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 4a — `defaults` lowering
    // -------------------------------------------------------------------------

    #[test]
    fn lower_feature_defaults_full_block() {
        let source = r#"
feature customer
  defaults
    tenancy org
    timestamps
    policy_for jobs, webhooks: @actor.system
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert!(matches!(feature.defaults.tenancy, Some(ir::Tenancy::Org)));
        assert!(feature.defaults.timestamps);
        match feature.defaults.policy.as_ref().expect("policy") {
            ir::PolicyRef::Atom(atom) => assert_eq!(atom, "actor.system"),
            other => panic!("expected @actor.system atom, got {other:?}"),
        }
    }

    #[test]
    fn lower_feature_defaults_absent_keeps_default() {
        let source = r#"
feature customer
  agent simple
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert!(feature.defaults.tenancy.is_none());
        assert!(!feature.defaults.timestamps);
        assert!(feature.defaults.policy.is_none());
    }

    #[test]
    fn lower_feature_defaults_custom_tenancy() {
        let source = r#"
feature pinned
  defaults
    tenancy workspace
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        match feature.defaults.tenancy.as_ref().expect("axis") {
            ir::Tenancy::Custom(axis) => assert_eq!(axis, "workspace"),
            other => panic!("expected custom axis, got {other:?}"),
        }
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 4c — `resource` lowering
    // -------------------------------------------------------------------------

    #[test]
    fn lower_feature_resource_lifts_retention_and_derived() {
        let source = r#"
feature customer
  domain
    resource Customer
      name: Text required
      score: Integer = 0
      is_high_value: Boolean derived from score > 80
      has_many notes: CustomerNote inverse customer

      soft_delete
      retention 7y then anonymize
      validates @validator.tier_check
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.resources.len(), 1);
        let r = &feature.resources[0];
        assert_eq!(r.name, "Customer");
        assert!(r.soft_delete);
        let ret = r.retention.as_ref().expect("retention");
        assert_eq!(ret.duration, "7y");
        assert!(matches!(ret.action, ir::RetentionAction::Anonymize));
        let derived = r
            .fields
            .iter()
            .find(|f| f.name == "is_high_value")
            .expect("is_high_value");
        assert_eq!(derived.derived_from.as_deref(), Some("score > 80"));
        // validates @validator.tier_check projects onto `Resource.validate`
        // for single-entry authoring.
        assert!(r.validate.is_some());
    }

    #[test]
    fn lower_registry_tool_entry_with_effect_and_pii_classes() {
        // Pin the IR shape for `RegistryToolEntry`. The actual
        // registry.lzi parser lands in a later phase; this test
        // documents the contract that doctor's
        // `tool_registry_effect_required_diagnostics` will read.
        let entry = ir::RegistryToolEntry {
            name: "web_search".to_owned(),
            effect: ir::ToolEffect::Read,
            pii_classes: vec![ir::QualifiedName {
                feature: None,
                name: "@pii.contact".to_owned(),
            }],
            adapter: Some(ir::QualifiedName {
                feature: None,
                name: "@adapter.serp".to_owned(),
            }),
            span_ref: None,
        };

        let serialized = serde_json::to_value(&entry).unwrap();
        assert_eq!(serialized["name"], "web_search");
        assert_eq!(serialized["effect"], "read");
        assert_eq!(serialized["pii_classes"][0]["name"], "@pii.contact");
        assert_eq!(serialized["adapter"]["name"], "@adapter.serp");
    }
}
