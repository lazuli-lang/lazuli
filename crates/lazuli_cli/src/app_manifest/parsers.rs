//! Shared line-level parsers and predicates for the `app_manifest` sub-tree.
//!
//! Every entry point in this module (`parse_app_manifest`, `parse_app_registry`,
//! `parse_app_workspace`, `parse_app_contracts`, `parse_app_profiles`) walks
//! source line by line, dispatches on indent depth, and then matches headers
//! and field shapes. The leaf-level recognizers — `is_identifier`,
//! `is_type_name`, `unquote`, `leading_spaces`, `split_items`,
//! `parse_quoted_prefix`, the child-block dispatchers (`app_child`,
//! `registry_child`, `workspace_child`, `profile_child`), and the per-shape
//! field parsers (`parse_contract_field`, `parse_app_env_var`,
//! `parse_app_pack_use`, etc.) — are pulled here so each entry-point file can
//! stay focused on its own block-level state machine.
//!
//! Everything in this file is intentionally `pub(super)` (not `pub`): the
//! helpers are internal to the `app_manifest` sub-tree. The crate boundary
//! only sees the five `parse_app_*` entry points re-exported from `mod.rs`.
//!
//! See: `lazuli_ir::nodes::app_manifest`,
//!      `lazuli_syntax::ast::feature::PackageSkeleton`.

use lazuli_ir::{
    AppBinding, AppCorsOriginRule, AppEnvVar, AppHsts, AppPackProvide, AppPackUse, ContractField,
    ContractImport, ContractOperationError, FeatureRequirement, SpanRef, WebhookEventField,
    WorkspaceApp, WorkspaceBoundary, WorkspaceGatewayRoute,
};

// All helpers below are `pub(super)`: shared internally across the
// `app_manifest` sub-modules but never exposed to other crates.

pub(super) fn workspace_child(trimmed: &str) -> Option<&'static str> {
    match trimmed.split_whitespace().next()? {
        "apps" => Some("apps"),
        "boundaries" => Some("boundaries"),
        "communication" => Some("communication"),
        _ => None,
    }
}

pub(super) fn parse_workspace_app(trimmed: &str) -> Option<WorkspaceApp> {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    match parts.as_slice() {
        [name, "at", path] if is_identifier(name) => Some(WorkspaceApp {
            name: (*name).to_owned(),
            kind: "local".to_owned(),
            path: Some(unquote(path).to_owned()),
            contract: None,
        }),
        [name, "external", "contract", contract] if is_identifier(name) => Some(WorkspaceApp {
            name: (*name).to_owned(),
            kind: "external".to_owned(),
            path: None,
            contract: Some(unquote(contract).to_owned()),
        }),
        _ => None,
    }
}

pub(super) fn parse_workspace_boundary(trimmed: &str) -> Option<WorkspaceBoundary> {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    match parts.as_slice() {
        [app, direction, pattern]
            if is_identifier(app) && matches!(*direction, "publishes" | "consumes") =>
        {
            Some(WorkspaceBoundary {
                app: (*app).to_owned(),
                direction: (*direction).to_owned(),
                pattern: (*pattern).to_owned(),
            })
        }
        _ => None,
    }
}

pub(super) fn parse_workspace_gateway_route(trimmed: &str) -> Option<WorkspaceGatewayRoute> {
    let rest = trimmed.strip_prefix("route ")?;
    let (path, tail) = parse_quoted_prefix(rest.trim())?;
    let parts: Vec<_> = tail.split_whitespace().collect();
    match parts.as_slice() {
        ["to", target_kind, target] if is_identifier(target) => Some(WorkspaceGatewayRoute {
            path,
            target_kind: (*target_kind).to_owned(),
            target: (*target).to_owned(),
            auth: None,
            tenant: None,
            timeout: None,
            rate_limit: None,
        }),
        _ => None,
    }
}

pub(super) fn parse_quoted_prefix(value: &str) -> Option<(String, &str)> {
    let rest = value.strip_prefix('"')?;
    let end = rest.find('"')?;
    let quoted = &rest[..end];
    let tail = rest[end + 1..].trim();
    Some((quoted.to_owned(), tail))
}

pub(super) fn parse_contract_import(trimmed: &str) -> Option<ContractImport> {
    let rest = trimmed.strip_prefix("import ")?;
    let parts: Vec<_> = rest.split_whitespace().collect();
    if parts.len() == 2 && is_contract_import_format(parts[0]) {
        Some(ContractImport {
            format: parts[0].to_owned(),
            source: unquote(parts[1]).to_owned(),
        })
    } else {
        None
    }
}

pub(super) fn is_contract_import_format(value: &str) -> bool {
    matches!(
        value,
        "openapi" | "asyncapi" | "proto" | "json_schema" | "avro"
    )
}

pub(super) fn named_block_name<'a>(trimmed: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = trimmed.strip_prefix(keyword)?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let name = rest.split_whitespace().next()?;
    is_identifier(name).then_some(name)
}

pub(super) fn parse_contract_operation_error(rest: &str) -> Option<ContractOperationError> {
    // Shape: `<Name> [status <code>] [expose <field>, <field>...]`
    let mut tokens = rest.split_whitespace();
    let name = tokens.next()?.to_owned();
    let mut status = None;
    let mut expose: Vec<String> = Vec::new();

    let mut state = "start";
    for token in tokens {
        match (state, token) {
            (_, "status") => state = "status",
            (_, "expose") => state = "expose",
            ("status", value) => {
                status = Some(value.to_owned());
                state = "after";
            }
            ("expose", value) => {
                expose.extend(
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|f| !f.is_empty())
                        .map(str::to_owned),
                );
            }
            _ => {}
        }
    }

    Some(ContractOperationError {
        name,
        status,
        expose,
    })
}

pub(super) fn parse_contract_field(trimmed: &str) -> Option<ContractField> {
    let (name, rest) = trimmed.split_once(':')?;
    let name = name.trim();
    if !is_identifier(name) {
        return None;
    }

    let mut parts: Vec<_> = rest.split_whitespace().collect();
    let requiredness = parts
        .last()
        .copied()
        .filter(|value| matches!(*value, "required" | "optional"))
        .map(str::to_owned);
    if requiredness.is_some() {
        parts.pop();
    }

    let type_name = parts.first()?.to_string();
    let markers = parts
        .iter()
        .skip(1)
        .filter(|part| part.starts_with('@'))
        .map(|part| (*part).to_owned())
        .collect();

    Some(ContractField {
        name: name.to_owned(),
        type_name,
        markers,
        requiredness,
    })
}

pub(super) fn app_child(trimmed: &str) -> Option<&'static str> {
    match trimmed.split_whitespace().next()? {
        "uses" => Some("uses"),
        "packs" => Some("packs"),
        "bindings" => Some("bindings"),
        "targets" => Some("targets"),
        "environments" => Some("environments"),
        "urls" => Some("urls"),
        "cors" => Some("cors"),
        "env" => Some("env"),
        "integrations" => Some("integrations"),
        "capabilities" => Some("capabilities"),
        "architecture" => Some("architecture"),
        "services" => Some("services"),
        "communication" => Some("communication"),
        "runtime" => Some("runtime"),
        "deploy" => Some("deploy"),
        // Observability bucket cycle row 36.
        "logging" => Some("logging"),
        "tracing" => Some("tracing"),
        "observability" => Some("observability"),
        // i18n bucket cycle — `locale` block at app indent-2.
        "locale" => Some("locale"),
        // Encryption bucket cycle — `encryption` block at app indent-2.
        // See `docs/proposals/encryption-vocab.md`.
        "encryption" => Some("encryption"),
        // Roadmap §1.10 — `headers` block at app indent-2 carries
        // CSP / HSTS / X-Frame-Options / X-Content-Type-Options /
        // Referrer-Policy / Permissions-Policy. Child grammar lives
        // at indent 4 (top-level fields) and indent 6 (HSTS body).
        "headers" => Some("headers"),
        // Roadmap §1.2 — HTTP hygiene at app indent-2. `cookie` groups
        // named profiles (default / session / csrf / ...); `proxy`
        // declares trusted upstreams + real-IP header overrides;
        // `limits` declares request-shape ceilings.
        "cookie" => Some("cookie"),
        "proxy" => Some("proxy"),
        "limits" => Some("limits"),
        "route_guard" => Some("route_guard"),
        _ => None,
    }
}

/// Webhooks expanded cycle — parse one indent-6 line of a
/// `webhook_events.<name>` envelope.
///
/// Grammar (positional, mirrors the per-record field shape):
///
/// ```text
/// <field_name>: <Type> [@semantic.X | @pii.Y ...] (required | optional)
/// ```
///
/// The type token is captured verbatim because the envelope is
/// provider-side. `@semantic.*` / `@pii.*` decorators are collected
/// into `capabilities` in author order. The trailing `required` or
/// `optional` keyword toggles `required`.
pub(super) fn parse_webhook_event_field(trimmed: &str) -> Option<WebhookEventField> {
    let (name_raw, rest) = trimmed.split_once(':')?;
    let name = name_raw.trim();
    if name.is_empty() {
        return None;
    }
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }
    let type_text = tokens[0].to_owned();
    let mut required = true;
    let mut capabilities: Vec<String> = Vec::new();
    for token in &tokens[1..] {
        match *token {
            "required" => required = true,
            "optional" => required = false,
            other if other.starts_with('@') => capabilities.push(other.to_owned()),
            _ => {}
        }
    }
    Some(WebhookEventField {
        name: name.to_owned(),
        type_text,
        required,
        capabilities,
    })
}

pub(super) fn registry_child(trimmed: &str) -> Option<&'static str> {
    match trimmed.split_whitespace().next()? {
        "env" => Some("env"),
        "integrations" => Some("integrations"),
        // B1 (W3-blockers) — `bindings` is registry-level sugar over
        // `integrations`. Same IR target (`AppIntegration`), same
        // codegen, but indent-6 children may use the simplified
        // `endpoint env.X` / `auth keys env.A env.B` surface instead
        // of nesting under `credentials platform`. The parser lowers
        // the sugar to canonical credential bindings on the fly.
        "bindings" => Some("integrations"),
        "capabilities" => Some("capabilities"),
        "packs" => Some("packs"),
        "tools" => Some("tools"),
        // Webhooks expanded cycle — `webhook_events` is the registry-side
        // catalog of expected inbound envelope shapes.
        "webhook_events" => Some("webhook_events"),
        // Roadmap §1.10 — `secret_rotation <name>` is a NAMED block
        // at indent-2 (not a container with indent-4 children like
        // `env`). The parser detects the header inline and switches
        // current_child to `"secret_rotation"`; indent-4 lines feed
        // the currently open `SecretRotation` entry.
        "secret_rotation" => Some("secret_rotation"),
        _ => None,
    }
}

pub(super) fn webhook_event_name(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix("webhook_event ")?;
    let name = rest.split_whitespace().next()?;
    (!name.is_empty()).then_some(name)
}

pub(super) fn profile_child(trimmed: &str) -> Option<&'static str> {
    match trimmed.split_whitespace().next()? {
        "urls" => Some("urls"),
        "bindings" => Some("bindings"),
        "integrations" => Some("integrations"),
        "deploy" => Some("deploy"),
        _ => None,
    }
}

pub(super) fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Roadmap §1.10 — parse the tail of `hsts` (either inline as
/// `hsts max_age 31536000 include_subdomains preload` or as a
/// six-space body where each child gets its own line). Tokens are
/// whitespace-separated; only the named slots write to `hsts`.
/// Unknown tokens are silently ignored — doctor diagnostics flag
/// shape errors.
pub(super) fn parse_hsts_inline(rest: &str, hsts: &mut AppHsts) {
    let trimmed = rest.trim();
    if trimmed.is_empty() {
        return;
    }
    let mut tokens = trimmed.split_whitespace().peekable();
    while let Some(token) = tokens.next() {
        match token {
            "max_age" => {
                if let Some(value) = tokens.next() {
                    if let Ok(n) = value.parse::<u64>() {
                        hsts.max_age = n;
                    }
                }
            }
            "include_subdomains" => {
                hsts.include_subdomains = true;
            }
            "preload" => {
                hsts.preload = true;
            }
            _ => {}
        }
    }
}

/// Cut A.11 — parse the tail of `allow_origins <env> "<origin>"[, "<origin>"]+`.
/// `rest` is the substring after `allow_origins `. The function pulls
/// the first whitespace-separated token as the environment, then
/// splits the remainder on commas, unquoting each origin.
pub(super) fn parse_cors_allow_origins(rest: &str) -> Option<AppCorsOriginRule> {
    let trimmed = rest.trim();
    let (env, body) = trimmed.split_once(char::is_whitespace)?;
    let environment = env.trim().to_owned();
    if environment.is_empty() {
        return None;
    }
    let origins: Vec<String> = body
        .split(',')
        .map(|raw| unquote(raw.trim()).to_owned())
        .filter(|s| !s.is_empty())
        .collect();
    if origins.is_empty() {
        return None;
    }
    Some(AppCorsOriginRule {
        environment,
        origins,
    })
}

pub(super) fn used_feature_name(trimmed: &str) -> Option<&str> {
    if trimmed.starts_with("feature ") {
        return trimmed.split_whitespace().nth(1);
    }
    trimmed
        .split(',')
        .next()
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

pub(super) fn split_items(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(super) fn parse_env_group_name(trimmed: &str) -> Option<&str> {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if parts.len() == 2 && parts[0] == "group" && is_identifier(parts[1]) {
        Some(parts[1])
    } else {
        None
    }
}

pub(super) fn parse_app_env_var(trimmed: &str, group: Option<&str>) -> Option<AppEnvVar> {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    let has_environment_scope = parts.len() >= 6 && parts.get(4) == Some(&"in");
    if !(parts.len() == 4 || has_environment_scope) {
        return None;
    }

    if !matches!(parts[0], "server" | "client" | "mobile")
        || !parts[1].ends_with(':')
        || !matches!(parts[2], "Secret" | "Text" | "Url" | "Boolean" | "Integer")
        || !matches!(parts[3], "required" | "optional")
    {
        return None;
    }

    let environments = if has_environment_scope {
        let environments = split_items(&parts[5..].join(" "));
        if environments
            .iter()
            .any(|environment| !is_identifier(environment))
        {
            return None;
        }
        environments
    } else {
        Vec::new()
    };

    Some(AppEnvVar {
        group: group.map(str::to_owned),
        scope: parts[0].to_owned(),
        name: parts[1].trim_end_matches(':').to_owned(),
        type_name: parts[2].to_owned(),
        requiredness: parts[3].to_owned(),
        environments,
    })
}

pub(super) fn parse_integration_header(trimmed: &str) -> Option<(String, String)> {
    let (name, kind) = trimmed.split_once(':')?;
    let name = name.trim();
    let kind = kind.trim();
    if is_identifier(name) && is_type_name(kind) {
        Some((name.to_owned(), kind.to_owned()))
    } else {
        None
    }
}

pub(super) fn adapter_source_provenance(source: &str) -> Option<&'static str> {
    if source
        .strip_prefix("@runtime/")
        .is_some_and(valid_pathish_tail)
    {
        Some("runtime")
    } else if source
        .strip_prefix("@lazuli/plugin-")
        .is_some_and(valid_plugin_tail)
    {
        Some("plugin")
    } else if source.strip_prefix("@adapter.").is_some_and(is_identifier)
        || source.starts_with("./")
        || source.starts_with("../")
        || (source.starts_with('"') && source.ends_with('"'))
    {
        Some("local")
    } else {
        None
    }
}

pub(super) fn valid_plugin_tail(value: &str) -> bool {
    // Single-segment (`@lazuli/plugin-<name>`) and multi-segment
    // (`@lazuli/plugin-<publisher>/<name>`) refs are both valid — the convention
    // shipped by the existing plugin repos (chromadb, expo-push, google-maps,
    // mercadopago, openai-embeddings, scalars-br, object-store, smtp, sms-twilio,
    // social-google, social-apple) uses single-segment `@lazuli/plugin-<name>` per their
    // manifest.toml + Lazurite.toml [plugins] keys.
    let segments: Vec<&str> = value.split('/').filter(|p| !p.is_empty()).collect();
    !segments.is_empty() && segments.iter().all(|s| valid_path_segment(s))
}

pub(super) fn valid_pathish_tail(value: &str) -> bool {
    !value.is_empty() && value.split('/').all(valid_path_segment)
}

pub(super) fn valid_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

pub(super) fn parse_app_pack_use(trimmed: &str) -> Option<AppPackUse> {
    let (name, source) = trimmed.split_once(" from ")?;
    let name = name.trim();
    let source = source.trim();
    if is_identifier(name) && is_pack_source(source) {
        Some(AppPackUse {
            name: name.to_owned(),
            source: source.to_owned(),
        })
    } else {
        None
    }
}

pub(super) fn parse_pack_header(trimmed: &str) -> Option<(String, String)> {
    let (name, source) = trimmed.split_once(" from ")?;
    let name = name.trim();
    let source = source.trim();
    if is_identifier(name) && is_pack_package_source(source) {
        Some((name.to_owned(), source.to_owned()))
    } else {
        None
    }
}

pub(super) fn parse_pack_provide(trimmed: &str) -> Option<AppPackProvide> {
    let rest = trimmed.strip_prefix("provides ")?;
    let parts: Vec<_> = rest.split_whitespace().collect();
    if parts.len() == 2 && is_identifier(parts[0]) && is_identifier(parts[1]) {
        Some(AppPackProvide {
            kind: parts[0].to_owned(),
            name: parts[1].to_owned(),
        })
    } else {
        None
    }
}

pub(super) fn parse_pack_requirement(trimmed: &str) -> Option<FeatureRequirement> {
    let rest = trimmed.strip_prefix("requires ")?;
    let requirement = rest.strip_prefix("integration ")?;
    let (name, contract) = requirement.split_once(':')?;
    let name = name.trim();
    let contract = contract.trim();
    if is_identifier(name) && is_type_name(contract) {
        Some(FeatureRequirement {
            kind: "integration".to_owned(),
            name: name.to_owned(),
            contract: contract.to_owned(),
        })
    } else {
        None
    }
}

pub(super) fn is_pack_source(source: &str) -> bool {
    pack_source_name(source).is_some_and(is_identifier)
}

pub(super) fn pack_source_name(source: &str) -> Option<&str> {
    source
        .strip_prefix("packs.")
        .or_else(|| source.strip_prefix("registry.packs."))
}

pub(super) fn is_pack_package_source(source: &str) -> bool {
    source.starts_with('@')
        || source.starts_with("./")
        || source.starts_with("../")
        || source.starts_with("http://")
        || source.starts_with("https://")
        || (source.starts_with('"') && source.ends_with('"'))
}

pub(super) fn parse_credential_binding(trimmed: &str) -> Option<(String, String)> {
    let mut parts = trimmed.split_whitespace();
    let name = parts.next()?;
    let source = parts.collect::<Vec<_>>().join(" ");
    if is_identifier(name) && !source.is_empty() {
        Some((name.to_owned(), source))
    } else {
        None
    }
}

/// B1 (W3-blockers) — recognize the `bindings` registry sugar at
/// indent-6 directly under an integration header. Two shapes:
///
/// ```text
/// endpoint env.S3_ENDPOINT
/// auth keys env.S3_ACCESS_KEY_ID env.S3_SECRET_ACCESS_KEY
/// ```
///
/// `endpoint <env-or-secret>` desugars to a single credential
/// binding (`endpoint -> env.S3_ENDPOINT`). `auth keys A B`
/// desugars to two bindings (`access_key_id -> A`,
/// `secret_access_key -> B`). Both lines synthesize an
/// implicit `credentials platform` scope when none is declared.
///
/// Returns `Some(bindings)` if the line matches one of the
/// sugared shapes, `None` otherwise (so the caller can fall
/// through to the canonical integration grammar or surface a
/// shape error).
pub(super) fn parse_bindings_sugar_line(trimmed: &str) -> Option<Vec<(String, String)>> {
    if let Some(rest) = trimmed.strip_prefix("endpoint ") {
        let source = rest.trim();
        if source.is_empty() {
            return None;
        }
        return Some(vec![("endpoint".to_owned(), source.to_owned())]);
    }
    if let Some(rest) = trimmed.strip_prefix("auth ") {
        // `auth keys env.A env.B` — positional S3-style credentials.
        // Two positional sources map to `access_key_id` +
        // `secret_access_key` in that order. Anything else (zero,
        // one, three+ sources) falls through so doctor can flag it.
        let mut parts = rest.split_whitespace();
        if parts.next() != Some("keys") {
            return None;
        }
        let sources: Vec<&str> = parts.collect();
        if sources.len() != 2 {
            return None;
        }
        return Some(vec![
            ("access_key_id".to_owned(), sources[0].to_owned()),
            ("secret_access_key".to_owned(), sources[1].to_owned()),
        ]);
    }
    None
}

pub(super) fn parse_app_binding(trimmed: &str) -> Option<AppBinding> {
    let (target, source) = trimmed.split_once('=')?;
    let target = target.trim();
    let source = source.trim();
    let (target_feature, target_slot) = target.split_once('.')?;

    if !is_identifier(target_feature)
        || !is_identifier(target_slot)
        || !is_integration_source(source)
    {
        return None;
    }

    Some(AppBinding {
        target_feature: target_feature.to_owned(),
        target_slot: target_slot.to_owned(),
        source: source.to_owned(),
    })
}

pub(super) fn is_integration_source(source: &str) -> bool {
    let Some(name) = integration_source_name(source) else {
        return false;
    };
    is_identifier(name)
}

pub(super) fn integration_source_name(source: &str) -> Option<&str> {
    source
        .strip_prefix("integrations.")
        .or_else(|| source.strip_prefix("registry.integrations."))
}

pub(super) fn parse_route_guard_redirect(value: &str) -> Option<String> {
    let target = value.strip_prefix("redirect ")?.trim();
    Some(unquote(target).to_owned())
}

pub(super) fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

pub(super) fn line_start_offsets(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

pub(super) fn line_span_ref(line_starts: &[usize], line_index: usize, line: &str) -> SpanRef {
    let start = line_starts.get(line_index).copied().unwrap_or_default();
    SpanRef {
        start,
        end: start + line.len(),
    }
}

pub(super) fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

pub(super) fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub(super) fn is_type_name(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_uppercase())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}
