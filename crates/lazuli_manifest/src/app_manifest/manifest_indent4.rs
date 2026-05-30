//! Indent-4 line handler extracted from `manifest.rs`.
//!
//! Routes by `state.current_child` (set on the indent-2 dispatch) to
//! one of two-dozen leaf sections: `uses`, `packs`, `bindings`,
//! `error_page`, `route_guard`, `targets`, `environments`, `urls`,
//! `cors`, `cookie`, `proxy`, `limits`, `headers`, `env`,
//! `capabilities`, `integrations`, `architecture`, `services`,
//! `communication`, `runtime`, `deploy`, `logging`, `tracing`,
//! `observability`, `locale`, `encryption`. Each arm reads + writes
//! the slot it owns and updates the cursors `state` exposes for the
//! indent-6 / indent-8 follow-up handlers.

use lazuli_ir::{
    AppArchitecture, AppCapability, AppCommunication, AppCookie, AppCors, AppDeploy, AppHeaders,
    AppHsts, AppIntegration, AppLimits, AppLocale, AppLogging, AppManifest, AppObservability,
    AppProxy, AppRuntimeUnit, AppService, AppTracing, AppUrl, CookieProfile, DeployCheckpoint,
    EncryptionAlgorithm, EncryptionBinding, EncryptionRotation, EncryptionSource,
    EncryptionTemplate, LocaleFallback, RouteGuardDefaults,
};

use super::manifest_indent::ManifestParseState;
use super::parsers::{
    is_identifier, line_span_ref, parse_app_binding, parse_app_env_var, parse_app_pack_use,
    parse_bool, parse_cors_allow_origins, parse_env_group_name, parse_hsts_inline,
    parse_integration_header, parse_route_guard_redirect, split_items, unquote, used_feature_name,
};

/// Indent-4 dispatch — branches by `state.current_child` and fills
/// the open block. `line` / `line_index` / `line_starts` are threaded
/// through for the span-tracking arms (`route_guard`).
pub(super) fn handle_indent4(
    trimmed: &str,
    line: &str,
    line_index: usize,
    line_starts: &[usize],
    app: &mut AppManifest,
    state: &mut ManifestParseState,
) {
    match state.current_child {
        Some("uses") => {
            if let Some(name) = used_feature_name(trimmed) {
                app.uses.push(name.to_owned());
            }
        }
        Some("packs") => {
            if let Some(pack_use) = parse_app_pack_use(trimmed) {
                app.packs.push(pack_use);
            }
        }
        Some("bindings") => {
            if let Some(binding) = parse_app_binding(trimmed) {
                app.bindings.push(binding);
            }
        }
        Some("error_page") => {
            if let Some(index) = state.current_error_page {
                let page = &mut app.error_pages[index];
                if let Some(rest) = trimmed.strip_prefix("template ") {
                    page.template = unquote(rest.trim()).to_owned();
                } else if let Some(rest) = trimmed.strip_prefix("audience ") {
                    page.audience = Some(rest.trim().to_owned());
                }
            }
        }
        Some("route_guard") => {
            let route_guard = app.route_guard.get_or_insert(RouteGuardDefaults {
                default_policy: None,
                on_unauthenticated: None,
                on_unauthorized: None,
                skeleton: None,
                span_ref: Some(line_span_ref(line_starts, line_index, line)),
            });
            if let Some(rest) = trimmed.strip_prefix("default_policy ") {
                route_guard.default_policy = Some(rest.trim().to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("on_unauthenticated ") {
                if let Some(target) = parse_route_guard_redirect(rest.trim()) {
                    route_guard.on_unauthenticated = Some(target);
                }
            } else if let Some(rest) = trimmed.strip_prefix("on_unauthorized ") {
                if let Some(target) = parse_route_guard_redirect(rest.trim()) {
                    route_guard.on_unauthorized = Some(target);
                }
            } else if let Some(rest) = trimmed.strip_prefix("skeleton ") {
                route_guard.skeleton = Some(rest.trim().to_owned());
            }
        }
        Some("targets") => app.targets.push(trimmed.to_owned()),
        Some("environments") => app.environments.push(trimmed.to_owned()),
        Some("urls") => {
            let parts: Vec<_> = trimmed.split_whitespace().collect();
            if parts.len() == 3 {
                app.urls.push(AppUrl {
                    target: parts[0].to_owned(),
                    environment: parts[1].to_owned(),
                    url: unquote(parts[2]).to_owned(),
                });
            }
        }
        Some("cors") => {
            let cors = app.cors.get_or_insert_with(AppCors::default);
            if let Some(rest) = trimmed.strip_prefix("allow_origins ") {
                if let Some(rule) = parse_cors_allow_origins(rest) {
                    cors.allow_origins.push(rule);
                }
            } else if let Some(rest) = trimmed.strip_prefix("allow_credentials ") {
                if let Some(value) = parse_bool(rest.trim()) {
                    cors.allow_credentials = value;
                }
            } else if let Some(rest) = trimmed.strip_prefix("max_age ") {
                cors.max_age = Some(unquote(rest.trim()).to_owned());
            }
        }
        // Roadmap §1.2 — `cookie.<profile>` header at indent 4
        // opens a profile (`default`, `session`, `csrf`, ...).
        // Bare ident only; doctor flags anything else.
        // Children (`signed`, `secure`, `http_only`,
        // `same_site`, `max_age`) land at indent 6.
        Some("cookie") => {
            let name = trimmed.trim();
            if is_identifier(name) {
                let cookie = app.cookie.get_or_insert_with(AppCookie::default);
                cookie.profiles.push(CookieProfile {
                    name: name.to_owned(),
                    signed: None,
                    secure: None,
                    http_only: None,
                    same_site: None,
                    max_age: None,
                    span_ref: None,
                });
                state.current_cookie_profile = cookie.profiles.len().checked_sub(1);
            } else {
                state.current_cookie_profile = None;
            }
        }
        // Roadmap §1.2 — `proxy` block: flat indent-4 children.
        // `trusted` accepts a comma-separated CIDR list (may
        // appear multiple times — entries merge). Header slots
        // are stored as raw strings; doctor enforces the
        // catalog.
        Some("proxy") => {
            let proxy = app.proxy.get_or_insert_with(AppProxy::default);
            if let Some(rest) = trimmed.strip_prefix("trusted ") {
                for cidr in split_items(rest) {
                    let trimmed_cidr = unquote(cidr.trim()).to_owned();
                    if !trimmed_cidr.is_empty() && !proxy.trusted.contains(&trimmed_cidr) {
                        proxy.trusted.push(trimmed_cidr);
                    }
                }
            } else if let Some(rest) = trimmed.strip_prefix("real_ip_header ") {
                proxy.real_ip_header = Some(unquote(rest.trim()).to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("forwarded_proto_header ") {
                proxy.forwarded_proto_header = Some(unquote(rest.trim()).to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("forwarded_host_header ") {
                proxy.forwarded_host_header = Some(unquote(rest.trim()).to_owned());
            }
        }
        // Roadmap §1.2 — `limits` block: flat indent-4 children.
        // Sizes/durations stored verbatim (quoted or bare);
        // doctor validates the parseability.
        Some("limits") => {
            let limits = app.limits.get_or_insert_with(AppLimits::default);
            if let Some(rest) = trimmed.strip_prefix("body_size ") {
                limits.body_size = Some(unquote(rest.trim()).to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("header_size ") {
                limits.header_size = Some(unquote(rest.trim()).to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("upload_size ") {
                limits.upload_size = Some(unquote(rest.trim()).to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("timeout ") {
                limits.timeout = Some(unquote(rest.trim()).to_owned());
            }
        }
        // Roadmap §1.10 — `headers` block scalar children.
        // Each slot maps 1:1 to a production-grade HTTP
        // security header. The HSTS sub-block opens its own
        // indent-6 scope via `state.in_headers_hsts`.
        Some("headers") => {
            let headers = app.headers.get_or_insert_with(AppHeaders::default);
            if let Some(rest) = trimmed.strip_prefix("csp ") {
                headers.csp = Some(unquote(rest.trim()).to_owned());
                state.in_headers_hsts = false;
            } else if let Some(rest) = trimmed.strip_prefix("hsts") {
                // `hsts` may carry inline children
                // (`hsts max_age 31536000 include_subdomains
                // preload`) or open a six-space body. The
                // inline form is the canonical sugar.
                let hsts = headers.hsts.get_or_insert_with(AppHsts::default);
                parse_hsts_inline(rest.trim(), hsts);
                state.in_headers_hsts = true;
            } else if let Some(rest) = trimmed.strip_prefix("x_frame_options ") {
                headers.x_frame_options = Some(rest.trim().to_owned());
                state.in_headers_hsts = false;
            } else if let Some(rest) = trimmed.strip_prefix("x_content_type_options ") {
                headers.x_content_type_options = Some(rest.trim().to_owned());
                state.in_headers_hsts = false;
            } else if let Some(rest) = trimmed.strip_prefix("referrer_policy ") {
                headers.referrer_policy = Some(rest.trim().to_owned());
                state.in_headers_hsts = false;
            } else if let Some(rest) = trimmed.strip_prefix("permissions_policy ") {
                headers.permissions_policy = Some(unquote(rest.trim()).to_owned());
                state.in_headers_hsts = false;
            } else {
                state.in_headers_hsts = false;
            }
        }
        Some("env") => {
            if let Some(group) = parse_env_group_name(trimmed) {
                state.current_env_group = Some(group.to_owned());
            } else {
                state.current_env_group = None;
                if let Some(env_var) = parse_app_env_var(trimmed, None) {
                    app.env.push(env_var);
                }
            }
        }
        Some("capabilities") => {
            let parts: Vec<_> = trimmed.split_whitespace().collect();
            if parts.len() == 2 {
                app.capabilities.push(AppCapability {
                    name: parts[0].to_owned(),
                    value: parts[1].to_owned(),
                });
            }
        }
        Some("integrations") => {
            if let Some((name, kind)) = parse_integration_header(trimmed) {
                app.integrations.push(AppIntegration {
                    name,
                    kind,
                    adapter: None,
                    adapter_provenance: None,
                    environments: Vec::new(),
                    credentials: None,
                    data_classification: None,
                });
                state.current_integration = app.integrations.len().checked_sub(1);
                state.current_integration_child = None;
            } else {
                state.current_integration = None;
                state.current_integration_child = None;
            }
        }
        Some("architecture") => {
            let architecture = app
                .architecture
                .get_or_insert_with(AppArchitecture::default);
            let parts: Vec<_> = trimmed.split_whitespace().collect();
            match parts.as_slice() {
                ["mode", value] => architecture.mode = Some((*value).to_owned()),
                ["service_ready", value] => {
                    architecture.service_ready = parse_bool(value);
                }
                ["enforce_service_boundaries", value] => {
                    architecture.enforce_service_boundaries = parse_bool(value);
                }
                _ => {}
            }
        }
        Some("services") => {
            let parts: Vec<_> = trimmed.split_whitespace().collect();
            if parts.len() == 2 && parts[0] == "service" {
                app.services.push(AppService {
                    name: parts[1].to_owned(),
                    owns: Vec::new(),
                    exposes: Vec::new(),
                    publishes: Vec::new(),
                    consumes: Vec::new(),
                });
                state.current_service = app.services.len().checked_sub(1);
                state.current_service_child = None;
            } else {
                state.current_service = None;
                state.current_service_child = None;
            }
        }
        Some("communication") => {
            let communication = app
                .communication
                .get_or_insert_with(AppCommunication::default);
            let parts: Vec<_> = trimmed.split_whitespace().collect();
            match parts.as_slice() {
                ["internal", "sync", value] => {
                    communication.internal = Some(format!("sync {value}"));
                }
                ["external", value] => communication.external = Some((*value).to_owned()),
                ["async", value] => communication.asynchronous = Some((*value).to_owned()),
                ["propagate", rest @ ..] => {
                    communication.propagate.extend(split_items(&rest.join(" ")));
                }
                ["timeout", "default", value] => {
                    communication.timeout_default = Some(unquote(value).to_owned());
                }
                ["retry", "default", count, "backoff", strategy] => {
                    communication.retry_default = Some(format!("{count} backoff {strategy}"));
                }
                _ => {}
            }
        }
        Some("runtime") => {
            let parts: Vec<_> = trimmed.split_whitespace().collect();
            if parts.len() == 2 && parts[0] == "unit" {
                app.runtime.push(AppRuntimeUnit {
                    name: parts[1].to_owned(),
                    serves: Vec::new(),
                    runs: Vec::new(),
                    healthcheck: None,
                    readiness: None,
                    locale_negotiate: None,
                });
                state.current_runtime_unit = app.runtime.len().checked_sub(1);
            } else {
                state.current_runtime_unit = None;
            }
        }
        Some("deploy") => {
            let deploy = app.deploy.get_or_insert_with(AppDeploy::default);
            let parts: Vec<_> = trimmed.split_whitespace().collect();
            match parts.as_slice() {
                ["migrations", value] => deploy.migrations = Some((*value).to_owned()),
                ["migration_lock", value] => {
                    deploy.migration_lock = Some((*value).to_owned());
                }
                ["destructive_migrations", value] => {
                    deploy.destructive_migrations = Some((*value).to_owned());
                }
                ["rollback", value] => deploy.rollback = Some((*value).to_owned()),
                // Migrations bucket cycle Route C — closed catalog
                // enforced downstream by `DEPLOY-STRATEGY-001`.
                ["strategy", value] => deploy.strategy = Some((*value).to_owned()),
                // Adapter-parsed duration literal; quotes stripped.
                ["lock_timeout", value] => {
                    deploy.lock_timeout = Some(unquote(value).to_owned());
                }
                ["pre_migration_hook", value] => {
                    deploy.pre_migration_hook = Some(unquote(value).to_owned());
                }
                ["post_migration_hook", value] => {
                    deploy.post_migration_hook = Some(unquote(value).to_owned());
                }
                // `checkpoint <name> "<path>"` — three tokens.
                ["checkpoint", cp_name, cp_path] => {
                    deploy.checkpoint = Some(DeployCheckpoint {
                        name: (*cp_name).to_owned(),
                        path: unquote(cp_path).to_owned(),
                        span_ref: None,
                    });
                }
                _ => {}
            }
        }
        // Observability bucket cycle row 36 — `app.logging`
        // block. All slots are optional; doctor closes the
        // catalogs (`level`, `format`, `redact`,
        // `sample_rate`).
        Some("logging") => {
            let logging = app.logging.get_or_insert_with(AppLogging::default);
            if let Some(rest) = trimmed.strip_prefix("level ") {
                logging.level = Some(rest.trim().to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("format ") {
                logging.format = Some(rest.trim().to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("redact ") {
                logging.redact = Some(rest.trim().to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("sample_rate ")
                && let Ok(value) = rest.trim().parse::<f64>()
            {
                logging.sample_rate = Some(value);
            }
        }
        // Observability bucket cycle row 36 — `app.tracing`
        // block. `propagate` accepts `true | false`;
        // `sample_rate` ∈ `[0.0, 1.0]` (doctor checks the
        // range); `exporter` names a `registry.capabilities`
        // slot.
        Some("tracing") => {
            let tracing = app.tracing.get_or_insert_with(AppTracing::default);
            if let Some(rest) = trimmed.strip_prefix("propagate ") {
                if let Some(value) = parse_bool(rest.trim()) {
                    tracing.propagate = Some(value);
                }
            } else if let Some(rest) = trimmed.strip_prefix("sample_rate ") {
                if let Ok(value) = rest.trim().parse::<f64>() {
                    tracing.sample_rate = Some(value);
                }
            } else if let Some(rest) = trimmed.strip_prefix("exporter ") {
                tracing.exporter = Some(rest.trim().to_owned());
            }
        }
        Some("observability") => {
            let observability = app
                .observability
                .get_or_insert_with(AppObservability::default);
            if let Some(rest) = trimmed.strip_prefix("error_source ") {
                observability.error_source = split_items(rest)
                    .into_iter()
                    .map(|item| item.trim().to_owned())
                    .filter(|item| !item.is_empty())
                    .collect();
            } else if let Some(rest) = trimmed.strip_prefix("panic_recover ")
                && let Some(value) = parse_bool(rest.trim())
            {
                observability.panic_recover = value;
            }
        }
        // i18n bucket cycle — `app.locale` block. `default`
        // declares the primary BCP-47 tag; `supported` is a
        // comma-separated list of tags; `fallback <src> -> <dst>`
        // declares one fallback edge (repeatable). The bare
        // scalar `default_locale` still parses for back-compat
        // when this block is absent; doctor `app_locale_block_
        // overrides_default_locale` warns when both are present.
        Some("locale") => {
            let locale = app.locale.get_or_insert_with(AppLocale::default);
            if let Some(rest) = trimmed.strip_prefix("default ") {
                locale.default = unquote(rest.trim()).to_owned();
            } else if let Some(rest) = trimmed.strip_prefix("supported ") {
                for tag in split_items(rest) {
                    let unquoted = unquote(tag.trim()).to_owned();
                    if !locale.supported.contains(&unquoted) {
                        locale.supported.push(unquoted);
                    }
                }
            } else if let Some(rest) = trimmed.strip_prefix("fallback ")
                && let Some((from_part, to_part)) = rest.split_once("->")
            {
                let from = unquote(from_part.trim()).to_owned();
                let to = unquote(to_part.trim()).to_owned();
                if !from.is_empty() && !to.is_empty() {
                    locale.fallbacks.push(LocaleFallback { from, to });
                }
            }
        }
        // Encryption bucket cycle — `encryption.key @key.<scope>`
        // header at indent 4 opens a binding. The verbatim
        // `@key.<scope>` reference is stored on
        // `EncryptionBinding.scope`. Source/algorithm/rotation
        // children land below at indent 6.
        Some("encryption") => {
            if let Some(rest) = trimmed.strip_prefix("key ") {
                let scope = rest.trim().to_owned();
                if scope.starts_with("@key.") {
                    app.encryption_bindings.push(EncryptionBinding {
                        scope,
                        source: EncryptionSource::Env(EncryptionTemplate {
                            literal: String::new(),
                            axes: Vec::new(),
                        }),
                        algorithm: EncryptionAlgorithm::Aes256Gcm,
                        rotation: EncryptionRotation::Manual,
                        rotation_profile: None,
                        span_ref: None,
                    });
                    state.current_encryption_binding = app.encryption_bindings.len().checked_sub(1);
                } else {
                    state.current_encryption_binding = None;
                }
            } else {
                state.current_encryption_binding = None;
            }
        }
        _ => {}
    }
}
