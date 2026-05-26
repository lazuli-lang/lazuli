//! App-manifest line-level parsers — block dispatcher (`app_child`),
//! environment vars, pack uses, bindings, CORS, HSTS, integration
//! adapter provenance, and the registry `bindings` sugar lowering.

use lazuli_ir::{
    AppBinding, AppCorsOriginRule, AppEnvVar, AppHsts, AppPackProvide, AppPackUse,
    FeatureRequirement,
};

use super::parsers_common::{is_identifier, is_type_name, split_items, unquote};

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
