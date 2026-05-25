//! Parser for `app.lzi` — the operational app manifest.
//!
//! This is the single largest entry point in the `app_manifest` sub-tree
//! because `app.lzi` is the widest contract in Lazuli: a single block
//! covers identity (`title`, `version`, `lazuli_version`), targets,
//! environment list, URL templates, CORS, security headers (HSTS / CSP),
//! cookies, proxy trust, request limits, route guards, env vars (with
//! environment scoping + grouping), integrations + credential bindings,
//! capability slots, architecture + services, communication defaults,
//! runtime units (with i18n locale negotiation), deploy topology + roll
//! strategy, logging / tracing / observability, locale defaults, and
//! encryption key bindings. Each section is dispatched by `app_child` at
//! indent-2; indent-4 / indent-6 / indent-8 lines fill the open block.
//!
//! The body is one large state machine on purpose — the cross-section
//! invariants (`current_integration` cleared by indent-2 breaks,
//! `in_headers_hsts` resets on outer transitions, etc.) cannot be
//! cleanly split without leaking state across functions. The cost is the
//! file length; the benefit is no surprise side-effects between
//! adjacent sections.
//!
//! See: `lazuli_ir::nodes::app_manifest::AppManifest`,
//!      `lazuli_syntax::ast::feature::PackageSkeleton`.

use lazuli_ir::{
    AppArchitecture, AppCapability, AppCommunication, AppCookie, AppCors, AppDeploy, AppHeaders,
    AppHsts, AppIntegration, AppIntegrationCredentialBinding, AppIntegrationCredentials, AppLimits,
    AppLocale, AppLogging, AppManifest, AppObservability, AppProxy, AppRuntimeUnit, AppService,
    AppServiceExposure, AppTracing, AppUrl, CookieProfile, DeployCheckpoint, EncryptionAlgorithm,
    EncryptionBinding, EncryptionRotation, EncryptionSource, EncryptionTemplate, ErrorPage,
    LocaleFallback, LocaleNegotiate, RouteGuardDefaults,
};

use super::parsers::{
    adapter_source_provenance, app_child, is_identifier, leading_spaces, line_span_ref,
    line_start_offsets, parse_app_binding, parse_app_env_var, parse_app_pack_use, parse_bool,
    parse_cors_allow_origins, parse_credential_binding, parse_env_group_name, parse_hsts_inline,
    parse_integration_header, parse_route_guard_redirect, split_items, unquote, used_feature_name,
};

pub fn parse_app_manifest(source: &str) -> Option<AppManifest> {
    let lines: Vec<_> = source.lines().collect();
    let line_starts = line_start_offsets(source);
    let start = lines
        .iter()
        .position(|line| leading_spaces(line) == 0 && line.trim_start().starts_with("app "))?;
    let header = lines[start].trim_start();
    let name = header.split_whitespace().nth(1)?.to_owned();

    let mut app = AppManifest {
        name,
        title: None,
        version: None,
        lazuli_version: None,
        targets: Vec::new(),
        default_locale: None,
        default_timezone: None,
        auth_failed_redirect: None,
        not_found: None,
        error_pages: Vec::new(),
        uses: Vec::new(),
        packs: Vec::new(),
        bindings: Vec::new(),
        architecture: None,
        services: Vec::new(),
        communication: None,
        environments: Vec::new(),
        urls: Vec::new(),
        cors: None,
        headers: None,
        env: Vec::new(),
        integrations: Vec::new(),
        capabilities: Vec::new(),
        runtime: Vec::new(),
        deploy: None,
        logging: None,
        tracing: None,
        observability: None,
        locale: None,
        encryption_bindings: Vec::new(),
        cookie: None,
        proxy: None,
        limits: None,
        route_guard: None,
        actor_query: None,
        span_ref: None,
    };
    let mut current_child: Option<&str> = None;
    let mut current_runtime_unit: Option<usize> = None;
    let mut current_service: Option<usize> = None;
    let mut current_service_child: Option<&str> = None;
    // i18n bucket cycle — tracks which child of the current `runtime
    // unit` is open (e.g. `locale_negotiate`). Indent-8 lines branch off
    // this token rather than the top-level `current_child`.
    let mut current_runtime_child: Option<&str> = None;
    let mut current_env_group: Option<String> = None;
    let mut current_integration: Option<usize> = None;
    let mut current_integration_child: Option<&str> = None;
    let mut current_error_page: Option<usize> = None;
    // Encryption bucket cycle — tracks the open
    // `encryption.key @key.<scope>` binding. Indent-6 lines (`source`,
    // `algorithm`, `rotation`, `rotation_profile`) populate this
    // index. `None` when no binding is currently open.
    let mut current_encryption_binding: Option<usize> = None;
    // Roadmap §1.10 — tracks the open `headers.hsts` sub-block.
    // Indent-6 lines (`max_age`, `include_subdomains`, `preload`)
    // populate the HSTS struct. `None` when no HSTS body is open.
    let mut in_headers_hsts: bool = false;
    // Roadmap §1.2 — tracks the open cookie profile (`default`,
    // `session`, `csrf`, ...). Indent-4 lines headed by a bare ident
    // open a profile; indent-6 lines (`signed`, `secure`, etc.)
    // populate it.
    let mut current_cookie_profile: Option<usize> = None;

    for (line_index, line) in lines.iter().enumerate().skip(start + 1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) == 0 {
            break;
        }

        match leading_spaces(line) {
            2 => {
                current_runtime_unit = None;
                current_service = None;
                current_service_child = None;
                current_env_group = None;
                current_integration = None;
                current_integration_child = None;
                current_error_page = None;
                current_encryption_binding = None;
                in_headers_hsts = false;
                current_cookie_profile = None;
                if let Some(rest) = trimmed.strip_prefix("title ") {
                    app.title = Some(unquote(rest.trim()).to_owned());
                    current_child = None;
                } else if let Some(rest) = trimmed.strip_prefix("version ") {
                    app.version = Some(unquote(rest.trim()).to_owned());
                    current_child = None;
                } else if let Some(rest) = trimmed.strip_prefix("lazuli_version ") {
                    app.lazuli_version = Some(unquote(rest.trim()).to_owned());
                    current_child = None;
                } else if let Some(rest) = trimmed.strip_prefix("default_locale ") {
                    app.default_locale = Some(unquote(rest.trim()).to_owned());
                    current_child = None;
                } else if let Some(rest) = trimmed.strip_prefix("default_timezone ") {
                    app.default_timezone = Some(unquote(rest.trim()).to_owned());
                    current_child = None;
                } else if let Some(rest) = trimmed.strip_prefix("auth_failed_redirect ") {
                    app.auth_failed_redirect = Some(rest.trim().to_owned());
                    current_child = None;
                } else if trimmed == "route_guard" {
                    app.route_guard = Some(RouteGuardDefaults {
                        default_policy: None,
                        on_unauthenticated: None,
                        on_unauthorized: None,
                        skeleton: None,
                        span_ref: Some(line_span_ref(&line_starts, line_index, line)),
                    });
                    current_child = Some("route_guard");
                } else if let Some(rest) = trimmed.strip_prefix("actor_query ") {
                    app.actor_query = Some(unquote(rest.trim()).to_owned());
                    current_child = None;
                } else if let Some(rest) = trimmed.strip_prefix("not_found ") {
                    app.not_found = Some(rest.trim().to_owned());
                    current_child = None;
                } else if let Some(rest) = trimmed.strip_prefix("error_page ") {
                    if let Ok(status) = rest.trim().parse::<u16>() {
                        app.error_pages.push(ErrorPage {
                            status,
                            template: String::new(),
                            audience: None,
                        });
                        current_error_page = app.error_pages.len().checked_sub(1);
                        current_child = Some("error_page");
                    } else {
                        current_child = None;
                    }
                } else if let Some(rest) = trimmed.strip_prefix("uses ") {
                    app.uses.extend(split_items(rest));
                    current_child = None;
                } else if let Some(child) = app_child(trimmed) {
                    current_child = Some(child);
                } else {
                    current_child = None;
                }
            }
            4 => match current_child {
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
                    if let Some(index) = current_error_page {
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
                        span_ref: Some(line_span_ref(&line_starts, line_index, line)),
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
                        current_cookie_profile = cookie.profiles.len().checked_sub(1);
                    } else {
                        current_cookie_profile = None;
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
                // indent-6 scope via `in_headers_hsts`.
                Some("headers") => {
                    let headers = app.headers.get_or_insert_with(AppHeaders::default);
                    if let Some(rest) = trimmed.strip_prefix("csp ") {
                        headers.csp = Some(unquote(rest.trim()).to_owned());
                        in_headers_hsts = false;
                    } else if let Some(rest) = trimmed.strip_prefix("hsts") {
                        // `hsts` may carry inline children
                        // (`hsts max_age 31536000 include_subdomains
                        // preload`) or open a six-space body. The
                        // inline form is the canonical sugar.
                        let hsts = headers.hsts.get_or_insert_with(AppHsts::default);
                        parse_hsts_inline(rest.trim(), hsts);
                        in_headers_hsts = true;
                    } else if let Some(rest) = trimmed.strip_prefix("x_frame_options ") {
                        headers.x_frame_options = Some(rest.trim().to_owned());
                        in_headers_hsts = false;
                    } else if let Some(rest) = trimmed.strip_prefix("x_content_type_options ") {
                        headers.x_content_type_options = Some(rest.trim().to_owned());
                        in_headers_hsts = false;
                    } else if let Some(rest) = trimmed.strip_prefix("referrer_policy ") {
                        headers.referrer_policy = Some(rest.trim().to_owned());
                        in_headers_hsts = false;
                    } else if let Some(rest) = trimmed.strip_prefix("permissions_policy ") {
                        headers.permissions_policy = Some(unquote(rest.trim()).to_owned());
                        in_headers_hsts = false;
                    } else {
                        in_headers_hsts = false;
                    }
                }
                Some("env") => {
                    if let Some(group) = parse_env_group_name(trimmed) {
                        current_env_group = Some(group.to_owned());
                    } else {
                        current_env_group = None;
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
                        current_integration = app.integrations.len().checked_sub(1);
                        current_integration_child = None;
                    } else {
                        current_integration = None;
                        current_integration_child = None;
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
                        current_service = app.services.len().checked_sub(1);
                        current_service_child = None;
                    } else {
                        current_service = None;
                        current_service_child = None;
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
                            communication.retry_default =
                                Some(format!("{count} backoff {strategy}"));
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
                        current_runtime_unit = app.runtime.len().checked_sub(1);
                    } else {
                        current_runtime_unit = None;
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
                    } else if let Some(rest) = trimmed.strip_prefix("sample_rate ") {
                        if let Ok(value) = rest.trim().parse::<f64>() {
                            logging.sample_rate = Some(value);
                        }
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
                    } else if let Some(rest) = trimmed.strip_prefix("panic_recover ") {
                        if let Some(value) = parse_bool(rest.trim()) {
                            observability.panic_recover = value;
                        }
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
                    } else if let Some(rest) = trimmed.strip_prefix("fallback ") {
                        if let Some((from_part, to_part)) = rest.split_once("->") {
                            let from = unquote(from_part.trim()).to_owned();
                            let to = unquote(to_part.trim()).to_owned();
                            if !from.is_empty() && !to.is_empty() {
                                locale.fallbacks.push(LocaleFallback { from, to });
                            }
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
                            current_encryption_binding =
                                app.encryption_bindings.len().checked_sub(1);
                        } else {
                            current_encryption_binding = None;
                        }
                    } else {
                        current_encryption_binding = None;
                    }
                }
                _ => {}
            },
            6 => {
                if current_child == Some("env") {
                    if let Some(group) = current_env_group.as_deref() {
                        if let Some(env_var) = parse_app_env_var(trimmed, Some(group)) {
                            app.env.push(env_var);
                        }
                    }
                } else if current_child == Some("integrations") {
                    let Some(integration_index) = current_integration else {
                        continue;
                    };
                    let integration = &mut app.integrations[integration_index];
                    if let Some(rest) = trimmed.strip_prefix("adapter ") {
                        let adapter = rest.trim();
                        integration.adapter = Some(adapter.to_owned());
                        integration.adapter_provenance =
                            adapter_source_provenance(adapter).map(str::to_owned);
                        current_integration_child = None;
                    } else if let Some(rest) = trimmed.strip_prefix("environments ") {
                        integration.environments.extend(split_items(rest));
                        current_integration_child = None;
                    } else if let Some(rest) = trimmed.strip_prefix("credentials ") {
                        integration.credentials = Some(AppIntegrationCredentials {
                            scope: rest.trim().to_owned(),
                            bindings: Vec::new(),
                        });
                        current_integration_child = Some("credentials");
                    } else if let Some(rest) = trimmed.strip_prefix("data_classification ") {
                        integration.data_classification = Some(rest.trim().to_owned());
                        current_integration_child = None;
                    }
                } else if current_child == Some("runtime") {
                    let Some(unit_index) = current_runtime_unit else {
                        continue;
                    };
                    let unit = &mut app.runtime[unit_index];
                    if let Some(rest) = trimmed.strip_prefix("serves ") {
                        unit.serves.extend(split_items(rest));
                    } else if let Some(rest) = trimmed.strip_prefix("runs ") {
                        unit.runs.extend(split_items(rest));
                    } else if let Some(rest) = trimmed.strip_prefix("healthcheck ") {
                        unit.healthcheck = Some(unquote(rest.trim()).to_owned());
                    } else if let Some(rest) = trimmed.strip_prefix("readiness ") {
                        unit.readiness = Some(unquote(rest.trim()).to_owned());
                    } else if trimmed == "locale_negotiate" {
                        // i18n bucket cycle — open the block; its
                        // children at indent 8 land below via the
                        // `current_runtime_child` token.
                        unit.locale_negotiate
                            .get_or_insert_with(LocaleNegotiate::default);
                        current_runtime_child = Some("locale_negotiate");
                    }
                } else if current_child == Some("services") {
                    let Some(service_index) = current_service else {
                        continue;
                    };
                    let service = &mut app.services[service_index];
                    if let Some(rest) = trimmed.strip_prefix("owns ") {
                        service.owns.extend(split_items(rest));
                        current_service_child = None;
                    } else if trimmed == "exposes" {
                        current_service_child = Some("exposes");
                    } else if let Some(rest) = trimmed.strip_prefix("publishes ") {
                        service.publishes.extend(split_items(rest));
                        current_service_child = None;
                    } else if let Some(rest) = trimmed.strip_prefix("consumes ") {
                        service.consumes.extend(split_items(rest));
                        current_service_child = None;
                    }
                } else if current_child == Some("encryption") {
                    // Encryption bucket cycle — `source`, `algorithm`,
                    // `rotation`, `rotation_profile` children at indent
                    // 6 populate the currently open binding. Unknown
                    // algorithm/rotation tokens are silently kept at
                    // the default; doctor diagnostics surface shape
                    // errors.
                    let Some(binding_index) = current_encryption_binding else {
                        continue;
                    };
                    let binding = &mut app.encryption_bindings[binding_index];
                    if let Some(rest) = trimmed.strip_prefix("source ") {
                        let raw = rest.trim();
                        let template = if let Some(literal) = raw.strip_prefix("env.") {
                            Some(EncryptionSource::Env(EncryptionTemplate::parse(literal)))
                        } else if let Some(literal) = raw.strip_prefix("secrets.") {
                            Some(EncryptionSource::Secrets(EncryptionTemplate::parse(
                                literal,
                            )))
                        } else {
                            None
                        };
                        if let Some(source) = template {
                            binding.source = source;
                        }
                    } else if let Some(rest) = trimmed.strip_prefix("algorithm ") {
                        if let Some(alg) = EncryptionAlgorithm::parse(rest.trim()) {
                            binding.algorithm = alg;
                        }
                    } else if let Some(rest) = trimmed.strip_prefix("rotation_profile ") {
                        // Roadmap §1.10 — bind this `@key.<scope>` to
                        // a `registry.secret_rotation <name>` profile.
                        // The cross-check that the profile exists
                        // lives in doctor's
                        // `secret-rotation-binding-unknown`.
                        let name = rest.trim().to_owned();
                        if !name.is_empty() {
                            binding.rotation_profile = Some(name);
                        }
                    } else if let Some(rest) = trimmed.strip_prefix("rotation ") {
                        if let Some(rot) = EncryptionRotation::parse(rest.trim()) {
                            binding.rotation = rot;
                        }
                    }
                } else if current_child == Some("headers") && in_headers_hsts {
                    // Roadmap §1.10 — six-space HSTS body. Children
                    // are `max_age <n>`, `include_subdomains`,
                    // `preload`. The inline form `hsts max_age N
                    // include_subdomains preload` covers the same
                    // ground.
                    let Some(headers) = app.headers.as_mut() else {
                        continue;
                    };
                    let hsts = headers.hsts.get_or_insert_with(AppHsts::default);
                    parse_hsts_inline(trimmed, hsts);
                } else if current_child == Some("cookie") {
                    // Roadmap §1.2 — populate the currently open profile.
                    // Boolean slots accept `true | false`; unparseable
                    // values are silently kept at `None` and surface
                    // through doctor's app-cookie-contract diagnostic.
                    let Some(profile_index) = current_cookie_profile else {
                        continue;
                    };
                    let Some(cookie) = app.cookie.as_mut() else {
                        continue;
                    };
                    let profile = &mut cookie.profiles[profile_index];
                    if let Some(rest) = trimmed.strip_prefix("signed ") {
                        profile.signed = parse_bool(rest.trim());
                    } else if let Some(rest) = trimmed.strip_prefix("secure ") {
                        profile.secure = parse_bool(rest.trim());
                    } else if let Some(rest) = trimmed.strip_prefix("http_only ") {
                        profile.http_only = parse_bool(rest.trim());
                    } else if let Some(rest) = trimmed.strip_prefix("same_site ") {
                        profile.same_site = Some(rest.trim().to_owned());
                    } else if let Some(rest) = trimmed.strip_prefix("max_age ") {
                        profile.max_age = Some(unquote(rest.trim()).to_owned());
                    }
                }
            }
            8 => {
                if current_child == Some("integrations")
                    && current_integration_child == Some("credentials")
                {
                    let Some(integration_index) = current_integration else {
                        continue;
                    };
                    let Some(credentials) = &mut app.integrations[integration_index].credentials
                    else {
                        continue;
                    };
                    if let Some((name, source)) = parse_credential_binding(trimmed) {
                        credentials
                            .bindings
                            .push(AppIntegrationCredentialBinding { name, source });
                    }
                } else if current_child == Some("services")
                    && current_service_child == Some("exposes")
                {
                    let Some(service_index) = current_service else {
                        continue;
                    };
                    let parts: Vec<_> = trimmed.split_whitespace().collect();
                    if parts.len() == 2 {
                        app.services[service_index]
                            .exposes
                            .push(AppServiceExposure {
                                kind: parts[0].to_owned(),
                                target: parts[1].to_owned(),
                            });
                    }
                } else if current_child == Some("runtime")
                    && current_runtime_child == Some("locale_negotiate")
                {
                    let Some(unit_index) = current_runtime_unit else {
                        continue;
                    };
                    let Some(ln) = app.runtime[unit_index].locale_negotiate.as_mut() else {
                        continue;
                    };
                    if let Some(rest) = trimmed.strip_prefix("source ") {
                        ln.source = Some(rest.trim().to_owned());
                    } else if let Some(rest) = trimmed.strip_prefix("strategy ") {
                        ln.strategy = Some(rest.trim().to_owned());
                    } else if let Some(rest) = trimmed.strip_prefix("fallback ") {
                        ln.fallback = Some(unquote(rest.trim()).to_owned());
                    }
                }
            }
            _ => {}
        }
    }

    if app.route_guard.is_none() {
        if let Some(redirect) = app.auth_failed_redirect.clone() {
            app.route_guard = Some(RouteGuardDefaults {
                default_policy: None,
                on_unauthenticated: Some(redirect),
                on_unauthorized: None,
                skeleton: None,
                span_ref: Some(line_span_ref(&line_starts, start, lines[start])),
            });
        }
    }

    Some(app)
}
