use lazuli_ir::{
    AppArchitecture, AppBinding, AppCapability, AppCommunication, AppContract, AppCookie, AppCors,
    AppCorsOriginRule, AppDeploy, AppEnvVar, AppHeaders, AppHsts, AppIntegration,
    AppIntegrationCredentialBinding, AppIntegrationCredentials, AppLimits, AppLocale, AppLogging,
    AppManifest, AppObservability, AppPack, AppPackProvide, AppPackUse, AppProfile,
    AppProfileDeploy, AppProfileIntegration, AppProfileUrl, AppProxy, AppRegistry, AppRuntimeUnit,
    AppService, AppServiceExposure, AppTracing, AppUrl, AppWorkspace, ContractEvent, ContractField,
    ContractImport, ContractOperation, ContractOperationError, ContractRecord, CookieProfile,
    DeployCheckpoint, EncryptionAlgorithm, EncryptionBinding, EncryptionRotation, EncryptionSource,
    EncryptionTemplate, ErrorPage, FeatureRequirement, LocaleFallback, LocaleNegotiate,
    QualifiedName, RegistryToolEntry, SecretRotation, ToolEffect, WebhookEvent, WebhookEventField,
    WorkspaceApp, WorkspaceBoundary, WorkspaceCommunication, WorkspaceGateway,
    WorkspaceGatewayRoute,
};

/// Side-channel captured during registry parsing for entries that exist
/// syntactically but lack `effect`. The IR's `RegistryToolEntry` carries
/// `effect` as a required field, so we cannot encode a missing-effect
/// entry there. Doctor consumes this list to emit
/// `tool_registry_effect_required_diagnostics`.
#[derive(Debug, Clone)]
pub struct RegistryToolEntryDefect {
    /// 1-based line where the offending `tool <name>` header appears.
    pub line: usize,
    pub name: String,
    pub reason: RegistryToolDefectReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryToolDefectReason {
    EffectMissing,
    EffectInvalid,
}

/// Output of `parse_app_registry`. Splits the well-formed registry IR
/// from the defect list so doctor can surface both.
#[derive(Debug, Clone, Default)]
pub struct RegistryParseOutput {
    pub registry: Option<AppRegistry>,
    pub tool_defects: Vec<RegistryToolEntryDefect>,
}

pub fn parse_app_contracts(source: &str) -> Vec<AppContract> {
    let lines: Vec<_> = source.lines().collect();
    let mut contracts = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        if leading_spaces(lines[index]) != 0 || !trimmed.starts_with("contract ") {
            index += 1;
            continue;
        }

        let Some(name) = trimmed.split_whitespace().nth(1) else {
            index += 1;
            continue;
        };

        let start = index;
        index += 1;
        while index < lines.len() {
            let next_trimmed = lines[index].trim_start();
            if leading_spaces(lines[index]) == 0
                && !next_trimmed.is_empty()
                && !next_trimmed.starts_with('#')
            {
                break;
            }
            index += 1;
        }

        contracts.push(parse_app_contract_block(name, &lines[start + 1..index]));
    }

    contracts
}

fn parse_app_contract_block(name: &str, lines: &[&str]) -> AppContract {
    let mut contract = AppContract {
        name: name.to_owned(),
        purpose: None,
        compatibility: None,
        imports: Vec::new(),
        records: Vec::new(),
        operations: Vec::new(),
        events: Vec::new(),
        span_ref: None,
    };
    let mut current_record: Option<usize> = None;
    let mut current_operation: Option<usize> = None;
    let mut current_event: Option<usize> = None;
    let mut in_event_payload = false;

    for line in lines {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            2 => {
                current_record = None;
                current_operation = None;
                current_event = None;
                in_event_payload = false;
                if let Some(rest) = trimmed.strip_prefix("purpose ") {
                    contract.purpose = Some(unquote(rest.trim()).to_owned());
                } else if let Some(rest) = trimmed.strip_prefix("compatibility ") {
                    contract.compatibility = Some(rest.trim().to_owned());
                } else if let Some(import) = parse_contract_import(trimmed) {
                    contract.imports.push(import);
                } else if let Some(name) = named_block_name(trimmed, "record") {
                    contract.records.push(ContractRecord {
                        name: name.to_owned(),
                        fields: Vec::new(),
                    });
                    current_record = contract.records.len().checked_sub(1);
                } else if let Some(name) = named_block_name(trimmed, "operation") {
                    contract.operations.push(ContractOperation {
                        name: name.to_owned(),
                        transport: None,
                        method: None,
                        path: None,
                        input: None,
                        output: None,
                        auth: None,
                        timeout: None,
                        retry: None,
                        idempotency: None,
                        errors: Vec::new(),
                    });
                    current_operation = contract.operations.len().checked_sub(1);
                } else if let Some(name) = named_block_name(trimmed, "event") {
                    contract.events.push(ContractEvent {
                        name: name.to_owned(),
                        topic: None,
                        payload: Vec::new(),
                    });
                    current_event = contract.events.len().checked_sub(1);
                }
            }
            4 => {
                if let Some(record_index) = current_record {
                    if let Some(field) = parse_contract_field(trimmed) {
                        contract.records[record_index].fields.push(field);
                    }
                } else if let Some(operation_index) = current_operation {
                    let operation = &mut contract.operations[operation_index];
                    if let Some(rest) = trimmed.strip_prefix("error ") {
                        if let Some(err) = parse_contract_operation_error(rest) {
                            operation.errors.push(err);
                        }
                        continue;
                    }
                    let parts: Vec<_> = trimmed.split_whitespace().collect();
                    match parts.as_slice() {
                        ["transport", value] => operation.transport = Some((*value).to_owned()),
                        ["method", value] => operation.method = Some((*value).to_owned()),
                        ["path", value] => operation.path = Some(unquote(value).to_owned()),
                        ["input", value] => operation.input = Some((*value).to_owned()),
                        ["output", rest @ ..] if !rest.is_empty() => {
                            operation.output = Some(rest.join(" "));
                        }
                        ["auth", value] => operation.auth = Some((*value).to_owned()),
                        ["timeout", value] => operation.timeout = Some(unquote(value).to_owned()),
                        ["retry", rest @ ..] if !rest.is_empty() => {
                            operation.retry = Some(rest.join(" "));
                        }
                        ["idempotency", "by", rest @ ..] if !rest.is_empty() => {
                            operation.idempotency = Some(rest.join(" "));
                        }
                        _ => {}
                    }
                } else if let Some(event_index) = current_event {
                    if let Some(rest) = trimmed.strip_prefix("topic ") {
                        contract.events[event_index].topic = Some(unquote(rest.trim()).to_owned());
                        in_event_payload = false;
                    } else {
                        in_event_payload = trimmed == "payload";
                    }
                }
            }
            6 => {
                if in_event_payload
                    && let Some(event_index) = current_event
                    && let Some(field) = parse_contract_field(trimmed)
                {
                    contract.events[event_index].payload.push(field);
                }
            }
            _ => {}
        }
    }

    contract
}

pub fn parse_app_workspace(source: &str) -> Option<AppWorkspace> {
    let lines: Vec<_> = source.lines().collect();
    let start = lines.iter().position(|line| {
        leading_spaces(line) == 0 && line.trim_start().starts_with("workspace ")
    })?;
    let header = lines[start].trim_start();
    let name = header.split_whitespace().nth(1)?.to_owned();

    let mut workspace = AppWorkspace {
        name,
        apps: Vec::new(),
        shared_registry: None,
        boundaries: Vec::new(),
        communication: None,
        gateways: Vec::new(),
        span_ref: None,
    };
    let mut current_child: Option<&str> = None;
    let mut current_gateway: Option<usize> = None;
    let mut current_gateway_route: Option<usize> = None;

    for line in lines.iter().skip(start + 1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) == 0 {
            break;
        }

        match leading_spaces(line) {
            2 => {
                current_gateway = None;
                current_gateway_route = None;
                if let Some(rest) = trimmed.strip_prefix("shared_registry ") {
                    workspace.shared_registry = Some(unquote(rest.trim()).to_owned());
                    current_child = None;
                } else if let Some(name) = trimmed.strip_prefix("gateway ") {
                    if is_identifier(name.trim()) {
                        workspace.gateways.push(WorkspaceGateway {
                            name: name.trim().to_owned(),
                            routes: Vec::new(),
                        });
                        current_gateway = workspace.gateways.len().checked_sub(1);
                        current_child = Some("gateway");
                    } else {
                        current_child = None;
                    }
                } else {
                    current_child = workspace_child(trimmed);
                }
            }
            4 => match current_child {
                Some("apps") => {
                    if let Some(app) = parse_workspace_app(trimmed) {
                        workspace.apps.push(app);
                    }
                }
                Some("boundaries") => {
                    if let Some(boundary) = parse_workspace_boundary(trimmed) {
                        workspace.boundaries.push(boundary);
                    }
                }
                Some("communication") => {
                    let communication = workspace
                        .communication
                        .get_or_insert_with(WorkspaceCommunication::default);
                    let parts: Vec<_> = trimmed.split_whitespace().collect();
                    match parts.as_slice() {
                        ["propagate", rest @ ..] => {
                            communication.propagate.extend(split_items(&rest.join(" ")));
                        }
                        ["default", "sync", rest @ ..] if !rest.is_empty() => {
                            communication.sync_default = Some(rest.join(" "));
                        }
                        ["default", "async", rest @ ..] if !rest.is_empty() => {
                            communication.async_default = Some(rest.join(" "));
                        }
                        _ => {}
                    }
                }
                Some("gateway") => {
                    let Some(gateway_index) = current_gateway else {
                        continue;
                    };
                    if let Some(route) = parse_workspace_gateway_route(trimmed) {
                        workspace.gateways[gateway_index].routes.push(route);
                        current_gateway_route = workspace.gateways[gateway_index]
                            .routes
                            .len()
                            .checked_sub(1);
                    } else {
                        current_gateway_route = None;
                    }
                }
                _ => {}
            },
            6 => {
                if current_child != Some("gateway") {
                    continue;
                }
                let Some(gateway_index) = current_gateway else {
                    continue;
                };
                let Some(route_index) = current_gateway_route else {
                    continue;
                };
                let route = &mut workspace.gateways[gateway_index].routes[route_index];
                if let Some(rest) = trimmed.strip_prefix("auth ") {
                    route.auth = Some(rest.trim().to_owned());
                } else if let Some(rest) = trimmed.strip_prefix("tenant ") {
                    route.tenant = Some(rest.trim().to_owned());
                } else if let Some(rest) = trimmed.strip_prefix("timeout ") {
                    route.timeout = Some(unquote(rest.trim()).to_owned());
                } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
                    route.rate_limit = Some(unquote(rest.trim()).to_owned());
                }
            }
            _ => {}
        }
    }

    Some(workspace)
}

pub fn parse_app_manifest(source: &str) -> Option<AppManifest> {
    let lines: Vec<_> = source.lines().collect();
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

    for line in lines.iter().skip(start + 1) {
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

    Some(app)
}

/// Backwards-compatible entry: returns just the well-formed registry IR.
/// Doctor uses `parse_app_registry_with_defects` to also collect the
/// `tool <name>` entries that lack an `effect` child.
pub fn parse_app_registry(source: &str) -> Option<AppRegistry> {
    parse_app_registry_with_defects(source).registry
}

pub fn parse_app_registry_with_defects(source: &str) -> RegistryParseOutput {
    let lines: Vec<_> = source.lines().collect();
    let Some(start) = lines.iter().position(|line| {
        leading_spaces(line) == 0
            && line
                .trim_start()
                .split_whitespace()
                .next()
                .is_some_and(|keyword| keyword == "registry")
    })
    else {
        return RegistryParseOutput::default();
    };

    let mut registry = AppRegistry {
        env: Vec::new(),
        integrations: Vec::new(),
        capabilities: Vec::new(),
        packs: Vec::new(),
        tools: Vec::new(),
        webhook_events: Vec::new(),
        secret_rotations: Vec::new(),
    };
    let mut current_child: Option<&str> = None;
    let mut current_env_group: Option<String> = None;
    let mut current_integration: Option<usize> = None;
    let mut current_integration_child: Option<&str> = None;
    let mut current_pack: Option<usize> = None;

    // Pending tool: when the parser encounters `tool <name>` at indent 4
    // it stages a PendingTool; effect / pii_classes / adapter children
    // fill it. When the tool exits (next indent <= 4) the parser either
    // commits to `registry.tools` (effect present) or records a defect.
    let mut pending_tool: Option<PendingTool> = None;
    let mut tool_defects: Vec<RegistryToolEntryDefect> = Vec::new();
    // Webhook event registry — currently staged event entry. Legacy
    // plural `webhook_events` puts fields at indent 6; singular
    // `webhook_event <name>` requires a `payload` child before fields.
    let mut current_webhook_event_index: Option<usize> = None;
    let mut in_webhook_event_payload = false;
    // Roadmap §1.10 — the `SecretRotation` entry whose indent-4
    // body (`cadence` / `overlap` / `auto_rollback`) is currently
    // being populated. Each indent-2 `secret_rotation <name>` line
    // opens a fresh entry.
    let mut current_secret_rotation: Option<usize> = None;

    for (offset, line) in lines.iter().enumerate().skip(start + 1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) == 0 {
            flush_pending_tool(&mut pending_tool, &mut registry, &mut tool_defects);
            break;
        }

        match leading_spaces(line) {
            2 => {
                flush_pending_tool(&mut pending_tool, &mut registry, &mut tool_defects);
                current_env_group = None;
                current_integration = None;
                current_integration_child = None;
                current_pack = None;
                current_webhook_event_index = None;
                in_webhook_event_payload = false;
                current_secret_rotation = None;
                if let Some(name) = webhook_event_name(trimmed) {
                    registry.webhook_events.push(WebhookEvent {
                        name: name.to_owned(),
                        payload: Vec::new(),
                        version: 1,
                        previous_version: None,
                        deprecated: false,
                        span_ref: None,
                    });
                    current_webhook_event_index = registry.webhook_events.len().checked_sub(1);
                    current_child = Some("webhook_event");
                } else {
                    current_child = registry_child(trimmed);
                    // Roadmap §1.10 — `secret_rotation <name>` opens a
                    // named block at indent-2. Stage the entry on the
                    // registry; indent-4 children populate it.
                    if current_child == Some("secret_rotation") {
                        if let Some(rest) = trimmed.strip_prefix("secret_rotation ") {
                            let name = rest.trim().to_owned();
                            if !name.is_empty() && !name.contains(char::is_whitespace) {
                                registry.secret_rotations.push(SecretRotation {
                                    name,
                                    cadence: String::new(),
                                    overlap: String::new(),
                                    auto_rollback: false,
                                    span_ref: None,
                                });
                                current_secret_rotation =
                                    registry.secret_rotations.len().checked_sub(1);
                            }
                        }
                    }
                }
            }
            4 => match current_child {
                Some("env") => {
                    if let Some(group) = parse_env_group_name(trimmed) {
                        current_env_group = Some(group.to_owned());
                    } else {
                        current_env_group = None;
                        if let Some(env_var) = parse_app_env_var(trimmed, None) {
                            registry.env.push(env_var);
                        }
                    }
                }
                Some("integrations") => {
                    if let Some((name, kind)) = parse_integration_header(trimmed) {
                        registry.integrations.push(AppIntegration {
                            name,
                            kind,
                            adapter: None,
                            adapter_provenance: None,
                            environments: Vec::new(),
                            credentials: None,
                            data_classification: None,
                        });
                        current_integration = registry.integrations.len().checked_sub(1);
                        current_integration_child = None;
                    } else {
                        current_integration = None;
                        current_integration_child = None;
                    }
                }
                Some("capabilities") => {
                    let parts: Vec<_> = trimmed.split_whitespace().collect();
                    if parts.len() == 2 {
                        registry.capabilities.push(AppCapability {
                            name: parts[0].to_owned(),
                            value: parts[1].to_owned(),
                        });
                    }
                }
                Some("packs") => {
                    if let Some((name, source)) = parse_pack_header(trimmed) {
                        registry.packs.push(AppPack {
                            name,
                            source,
                            version: None,
                            provides: Vec::new(),
                            requirements: Vec::new(),
                        });
                        current_pack = registry.packs.len().checked_sub(1);
                    } else {
                        current_pack = None;
                    }
                }
                Some("tools") => {
                    flush_pending_tool(&mut pending_tool, &mut registry, &mut tool_defects);
                    if let Some(rest) = trimmed.strip_prefix("tool ") {
                        let name = rest.trim().to_owned();
                        if !name.is_empty() {
                            pending_tool = Some(PendingTool {
                                name,
                                line: offset + 1,
                                effect: None,
                                effect_invalid: false,
                                pii_classes: Vec::new(),
                                adapter: None,
                            });
                        }
                    }
                }
                Some("webhook_events") => {
                    // Each indent-4 line under `webhook_events` opens a
                    // new envelope entry. The bare identifier is the
                    // catalog key (`crm_customer_upsert`,
                    // `stripe_invoice_paid`, etc.). Fields land at
                    // indent 6.
                    let name = trimmed.trim();
                    if name.is_empty() || name.contains(' ') {
                        current_webhook_event_index = None;
                    } else {
                        registry.webhook_events.push(WebhookEvent {
                            name: name.to_owned(),
                            payload: Vec::new(),
                            version: 1,
                            previous_version: None,
                            deprecated: false,
                            span_ref: None,
                        });
                        current_webhook_event_index = registry.webhook_events.len().checked_sub(1);
                    }
                }
                Some("webhook_event") => {
                    let Some(idx) = current_webhook_event_index else {
                        continue;
                    };
                    if trimmed == "payload" {
                        in_webhook_event_payload = true;
                    } else {
                        in_webhook_event_payload = false;
                        if let Some(rest) = trimmed.strip_prefix("version ") {
                            if let Ok(version) = rest.trim().parse::<u32>() {
                                registry.webhook_events[idx].version = version;
                            }
                        } else if let Some(rest) = trimmed.strip_prefix("previous_version ") {
                            if let Ok(version) = rest.trim().parse::<u32>() {
                                registry.webhook_events[idx].previous_version = Some(version);
                            }
                        } else if let Some(rest) = trimmed.strip_prefix("deprecated ") {
                            if let Some(value) = parse_bool(rest.trim()) {
                                registry.webhook_events[idx].deprecated = value;
                            }
                        }
                    }
                }
                // Roadmap §1.10 — body of the currently open
                // `secret_rotation <name>` entry. Closed catalog:
                // `cadence <duration>` / `overlap <duration>` /
                // `auto_rollback <bool>`.
                Some("secret_rotation") => {
                    let Some(rotation_index) = current_secret_rotation else {
                        continue;
                    };
                    let rotation = &mut registry.secret_rotations[rotation_index];
                    if let Some(rest) = trimmed.strip_prefix("cadence ") {
                        rotation.cadence = rest.trim().to_owned();
                    } else if let Some(rest) = trimmed.strip_prefix("overlap ") {
                        rotation.overlap = rest.trim().to_owned();
                    } else if let Some(rest) = trimmed.strip_prefix("auto_rollback ") {
                        if let Some(value) = parse_bool(rest.trim()) {
                            rotation.auto_rollback = value;
                        }
                    }
                }
                _ => {}
            },
            6 => {
                if current_child == Some("env") {
                    if let Some(group) = current_env_group.as_deref()
                        && let Some(env_var) = parse_app_env_var(trimmed, Some(group))
                    {
                        registry.env.push(env_var);
                    }
                } else if current_child == Some("integrations") {
                    let Some(integration_index) = current_integration else {
                        continue;
                    };
                    let integration = &mut registry.integrations[integration_index];
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
                } else if current_child == Some("packs") {
                    let Some(pack_index) = current_pack else {
                        continue;
                    };
                    let pack = &mut registry.packs[pack_index];
                    if let Some(rest) = trimmed.strip_prefix("version ") {
                        pack.version = Some(unquote(rest.trim()).to_owned());
                    } else if let Some(provide) = parse_pack_provide(trimmed) {
                        pack.provides.push(provide);
                    } else if let Some(requirement) = parse_pack_requirement(trimmed) {
                        pack.requirements.push(requirement);
                    }
                } else if current_child == Some("tools") {
                    let Some(pending) = pending_tool.as_mut() else {
                        continue;
                    };
                    if let Some(rest) = trimmed.strip_prefix("effect ") {
                        match rest.trim() {
                            "read" => pending.effect = Some(ToolEffect::Read),
                            "write" => pending.effect = Some(ToolEffect::Write),
                            _ => pending.effect_invalid = true,
                        }
                    } else if let Some(rest) = trimmed.strip_prefix("pii_classes ") {
                        pending.pii_classes = split_items(rest)
                            .into_iter()
                            .map(|raw| QualifiedName {
                                feature: None,
                                name: pii_class_name(&raw),
                            })
                            .collect();
                    } else if let Some(rest) = trimmed.strip_prefix("adapter ") {
                        pending.adapter = Some(QualifiedName {
                            feature: None,
                            name: rest.trim().to_owned(),
                        });
                    }
                } else if current_child == Some("webhook_events") {
                    let Some(idx) = current_webhook_event_index else {
                        continue;
                    };
                    if let Some(field) = parse_webhook_event_field(trimmed) {
                        registry.webhook_events[idx].payload.push(field);
                    }
                } else if current_child == Some("webhook_event") && in_webhook_event_payload {
                    let Some(idx) = current_webhook_event_index else {
                        continue;
                    };
                    if let Some(field) = parse_webhook_event_field(trimmed) {
                        registry.webhook_events[idx].payload.push(field);
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
                    let Some(credentials) =
                        &mut registry.integrations[integration_index].credentials
                    else {
                        continue;
                    };
                    if let Some((name, source)) = parse_credential_binding(trimmed) {
                        credentials
                            .bindings
                            .push(AppIntegrationCredentialBinding { name, source });
                    }
                }
            }
            _ => {}
        }
    }

    flush_pending_tool(&mut pending_tool, &mut registry, &mut tool_defects);

    RegistryParseOutput {
        registry: Some(registry),
        tool_defects,
    }
}

#[derive(Debug)]
struct PendingTool {
    name: String,
    line: usize,
    effect: Option<ToolEffect>,
    effect_invalid: bool,
    pii_classes: Vec<QualifiedName>,
    adapter: Option<QualifiedName>,
}

fn flush_pending_tool(
    pending: &mut Option<PendingTool>,
    registry: &mut AppRegistry,
    defects: &mut Vec<RegistryToolEntryDefect>,
) {
    let Some(tool) = pending.take() else { return };

    if tool.effect_invalid {
        defects.push(RegistryToolEntryDefect {
            line: tool.line,
            name: tool.name,
            reason: RegistryToolDefectReason::EffectInvalid,
        });
        return;
    }

    let Some(effect) = tool.effect else {
        defects.push(RegistryToolEntryDefect {
            line: tool.line,
            name: tool.name,
            reason: RegistryToolDefectReason::EffectMissing,
        });
        return;
    };

    registry.tools.push(RegistryToolEntry {
        name: tool.name,
        effect,
        pii_classes: tool.pii_classes,
        adapter: tool.adapter,
        span_ref: None,
    });
}

/// Normalise a raw `pii_classes` token (e.g. `contact`, `@pii.contact`)
/// to the canonical closed-namespace form. The IR keeps it as a string
/// inside `QualifiedName::name` so doctor can compare against the
/// agent-side `@pii.*` references uniformly.
fn pii_class_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("@pii.") {
        trimmed.to_owned()
    } else {
        format!("@pii.{trimmed}")
    }
}

pub fn parse_app_profiles(source: &str) -> Vec<AppProfile> {
    let lines: Vec<_> = source.lines().collect();
    let mut profiles = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        if leading_spaces(lines[index]) != 0 || !trimmed.starts_with("profile ") {
            index += 1;
            continue;
        }

        let Some(name) = trimmed.split_whitespace().nth(1) else {
            index += 1;
            continue;
        };
        if !is_identifier(name) {
            index += 1;
            continue;
        }

        let start = index;
        index += 1;
        while index < lines.len() {
            let next_trimmed = lines[index].trim_start();
            if leading_spaces(lines[index]) == 0
                && !next_trimmed.is_empty()
                && !next_trimmed.starts_with('#')
            {
                break;
            }
            index += 1;
        }

        profiles.push(parse_app_profile_block(name, &lines[start + 1..index]));
    }

    profiles
}

fn parse_app_profile_block(name: &str, lines: &[&str]) -> AppProfile {
    let mut profile = AppProfile {
        name: name.to_owned(),
        urls: Vec::new(),
        bindings: Vec::new(),
        integrations: Vec::new(),
        deploy: None,
    };
    let mut current_child: Option<&str> = None;

    for line in lines {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            2 => current_child = profile_child(trimmed),
            4 => match current_child {
                Some("urls") => {
                    let parts: Vec<_> = trimmed.split_whitespace().collect();
                    if parts.len() == 2 && is_identifier(parts[0]) {
                        profile.urls.push(AppProfileUrl {
                            target: parts[0].to_owned(),
                            url: unquote(parts[1]).to_owned(),
                        });
                    }
                }
                Some("bindings") => {
                    if let Some(binding) = parse_app_binding(trimmed) {
                        profile.bindings.push(binding);
                    }
                }
                Some("integrations") => {
                    let parts: Vec<_> = trimmed.split_whitespace().collect();
                    match parts.as_slice() {
                        [name, "environment", environment]
                            if is_identifier(name) && is_identifier(environment) =>
                        {
                            upsert_profile_integration(&mut profile, name).environment =
                                Some((*environment).to_owned());
                        }
                        [name, "adapter", adapter] if is_identifier(name) => {
                            let integration = upsert_profile_integration(&mut profile, name);
                            integration.adapter = Some((*adapter).to_owned());
                            integration.adapter_provenance =
                                adapter_source_provenance(adapter).map(str::to_owned);
                        }
                        _ => {}
                    }
                }
                Some("deploy") => {
                    let deploy = profile.deploy.get_or_insert_with(AppProfileDeploy::default);
                    let parts: Vec<_> = trimmed.split_whitespace().collect();
                    match parts.as_slice() {
                        ["topology", value] => deploy.topology = Some((*value).to_owned()),
                        ["migrations", value] => deploy.migrations = Some((*value).to_owned()),
                        ["migration_lock", value] => {
                            deploy.migration_lock = Some((*value).to_owned());
                        }
                        ["destructive_migrations", value] => {
                            deploy.destructive_migrations = Some((*value).to_owned());
                        }
                        ["rollback", value] => deploy.rollback = Some((*value).to_owned()),
                        _ => {}
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    profile
}

fn upsert_profile_integration<'a>(
    profile: &'a mut AppProfile,
    name: &str,
) -> &'a mut AppProfileIntegration {
    if let Some(index) = profile
        .integrations
        .iter()
        .position(|integration| integration.name == name)
    {
        return &mut profile.integrations[index];
    }

    profile.integrations.push(AppProfileIntegration {
        name: name.to_owned(),
        environment: None,
        adapter: None,
        adapter_provenance: None,
    });
    let index = profile.integrations.len() - 1;
    &mut profile.integrations[index]
}

fn workspace_child(trimmed: &str) -> Option<&'static str> {
    match trimmed.split_whitespace().next()? {
        "apps" => Some("apps"),
        "boundaries" => Some("boundaries"),
        "communication" => Some("communication"),
        _ => None,
    }
}

fn parse_workspace_app(trimmed: &str) -> Option<WorkspaceApp> {
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

fn parse_workspace_boundary(trimmed: &str) -> Option<WorkspaceBoundary> {
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

fn parse_workspace_gateway_route(trimmed: &str) -> Option<WorkspaceGatewayRoute> {
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

fn parse_quoted_prefix(value: &str) -> Option<(String, &str)> {
    let rest = value.strip_prefix('"')?;
    let end = rest.find('"')?;
    let quoted = &rest[..end];
    let tail = rest[end + 1..].trim();
    Some((quoted.to_owned(), tail))
}

fn parse_contract_import(trimmed: &str) -> Option<ContractImport> {
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

fn is_contract_import_format(value: &str) -> bool {
    matches!(
        value,
        "openapi" | "asyncapi" | "proto" | "json_schema" | "avro"
    )
}

fn named_block_name<'a>(trimmed: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = trimmed.strip_prefix(keyword)?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let name = rest.split_whitespace().next()?;
    is_identifier(name).then_some(name)
}

fn parse_contract_operation_error(rest: &str) -> Option<ContractOperationError> {
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

fn parse_contract_field(trimmed: &str) -> Option<ContractField> {
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

fn app_child(trimmed: &str) -> Option<&'static str> {
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
fn parse_webhook_event_field(trimmed: &str) -> Option<WebhookEventField> {
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

fn registry_child(trimmed: &str) -> Option<&'static str> {
    match trimmed.split_whitespace().next()? {
        "env" => Some("env"),
        "integrations" => Some("integrations"),
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

fn webhook_event_name(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix("webhook_event ")?;
    let name = rest.split_whitespace().next()?;
    (!name.is_empty()).then_some(name)
}

fn profile_child(trimmed: &str) -> Option<&'static str> {
    match trimmed.split_whitespace().next()? {
        "urls" => Some("urls"),
        "bindings" => Some("bindings"),
        "integrations" => Some("integrations"),
        "deploy" => Some("deploy"),
        _ => None,
    }
}

fn parse_bool(value: &str) -> Option<bool> {
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
fn parse_hsts_inline(rest: &str, hsts: &mut AppHsts) {
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
fn parse_cors_allow_origins(rest: &str) -> Option<AppCorsOriginRule> {
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

fn used_feature_name(trimmed: &str) -> Option<&str> {
    if trimmed.starts_with("feature ") {
        return trimmed.split_whitespace().nth(1);
    }
    trimmed
        .split(',')
        .next()
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

fn split_items(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

fn parse_env_group_name(trimmed: &str) -> Option<&str> {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if parts.len() == 2 && parts[0] == "group" && is_identifier(parts[1]) {
        Some(parts[1])
    } else {
        None
    }
}

fn parse_app_env_var(trimmed: &str, group: Option<&str>) -> Option<AppEnvVar> {
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

fn parse_integration_header(trimmed: &str) -> Option<(String, String)> {
    let (name, kind) = trimmed.split_once(':')?;
    let name = name.trim();
    let kind = kind.trim();
    if is_identifier(name) && is_type_name(kind) {
        Some((name.to_owned(), kind.to_owned()))
    } else {
        None
    }
}

fn adapter_source_provenance(source: &str) -> Option<&'static str> {
    if source
        .strip_prefix("@runtime/")
        .is_some_and(valid_pathish_tail)
    {
        Some("runtime")
    } else if source
        .strip_prefix("@plugin/")
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

fn valid_plugin_tail(value: &str) -> bool {
    value.split('/').filter(|part| !part.is_empty()).count() >= 2
        && value.split('/').all(valid_path_segment)
}

fn valid_pathish_tail(value: &str) -> bool {
    !value.is_empty() && value.split('/').all(valid_path_segment)
}

fn valid_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

fn parse_app_pack_use(trimmed: &str) -> Option<AppPackUse> {
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

fn parse_pack_header(trimmed: &str) -> Option<(String, String)> {
    let (name, source) = trimmed.split_once(" from ")?;
    let name = name.trim();
    let source = source.trim();
    if is_identifier(name) && is_pack_package_source(source) {
        Some((name.to_owned(), source.to_owned()))
    } else {
        None
    }
}

fn parse_pack_provide(trimmed: &str) -> Option<AppPackProvide> {
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

fn parse_pack_requirement(trimmed: &str) -> Option<FeatureRequirement> {
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

fn is_pack_source(source: &str) -> bool {
    pack_source_name(source).is_some_and(is_identifier)
}

fn pack_source_name(source: &str) -> Option<&str> {
    source
        .strip_prefix("packs.")
        .or_else(|| source.strip_prefix("registry.packs."))
}

fn is_pack_package_source(source: &str) -> bool {
    source.starts_with('@')
        || source.starts_with("./")
        || source.starts_with("../")
        || source.starts_with("http://")
        || source.starts_with("https://")
        || (source.starts_with('"') && source.ends_with('"'))
}

fn parse_credential_binding(trimmed: &str) -> Option<(String, String)> {
    let mut parts = trimmed.split_whitespace();
    let name = parts.next()?;
    let source = parts.collect::<Vec<_>>().join(" ");
    if is_identifier(name) && !source.is_empty() {
        Some((name.to_owned(), source))
    } else {
        None
    }
}

fn parse_app_binding(trimmed: &str) -> Option<AppBinding> {
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

fn is_integration_source(source: &str) -> bool {
    let Some(name) = integration_source_name(source) else {
        return false;
    };
    is_identifier(name)
}

fn integration_source_name(source: &str) -> Option<&str> {
    source
        .strip_prefix("integrations.")
        .or_else(|| source.strip_prefix("registry.integrations."))
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_type_name(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_uppercase())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::{
        parse_app_contracts, parse_app_manifest, parse_app_profiles, parse_app_registry,
        parse_app_workspace,
    };

    #[test]
    fn parses_operational_manifest() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
  error_page 404
    template "./views/404.tmpl"
    audience public
  uses
    customer
  packs
    customer_import from registry.packs.customer_import
  bindings
    customer.gateway = integrations.crm
  targets
    backend go
  environments
    production
  urls
    api production "https://api.acme.example"
  env
    server DATABASE_URL: Secret required
    group mailer
      server MAILER_API_KEY: Secret required in production
  integrations
    crm: CRMProvider
      adapter @adapter.crm
      environments production
      credentials platform
        webhook_secret env.CRM_WEBHOOK_SECRET
  capabilities
    database postgres
  architecture
    mode modular_monolith
    service_ready true
    enforce_service_boundaries true
  services
    service crm
      owns customer
      exposes
        query customer.query.list
      publishes customer.*
  communication
    internal sync rpc
    external http
    async event_bus
    propagate actor, tenant, trace_id, request_id
    timeout default "2s"
    retry default 2 backoff exponential
  runtime
    unit api
      serves queries, commands
      healthcheck "/healthz"
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#;

        let manifest = parse_app_manifest(source).unwrap();

        assert_eq!(manifest.name, "AcmeCRM");
        assert_eq!(manifest.error_pages.len(), 1);
        assert_eq!(manifest.error_pages[0].status, 404);
        assert_eq!(manifest.error_pages[0].template, "./views/404.tmpl");
        assert_eq!(manifest.error_pages[0].audience.as_deref(), Some("public"));
        assert_eq!(manifest.uses, ["customer"]);
        assert_eq!(manifest.packs[0].name, "customer_import");
        assert_eq!(manifest.packs[0].source, "registry.packs.customer_import");
        assert_eq!(manifest.bindings[0].target_feature, "customer");
        assert_eq!(manifest.bindings[0].target_slot, "gateway");
        assert_eq!(manifest.bindings[0].source, "integrations.crm");
        assert_eq!(manifest.targets, ["backend go"]);
        assert_eq!(manifest.environments, ["production"]);
        assert_eq!(manifest.urls[0].url, "https://api.acme.example");
        assert_eq!(manifest.env[0].name, "DATABASE_URL");
        assert_eq!(manifest.env[1].group.as_deref(), Some("mailer"));
        assert_eq!(manifest.env[1].name, "MAILER_API_KEY");
        assert_eq!(manifest.env[1].environments, ["production"]);
        assert_eq!(manifest.integrations[0].name, "crm");
        assert_eq!(manifest.integrations[0].kind, "CRMProvider");
        assert_eq!(
            manifest.integrations[0].adapter.as_deref(),
            Some("@adapter.crm")
        );
        assert_eq!(
            manifest.integrations[0].adapter_provenance.as_deref(),
            Some("local")
        );
        assert_eq!(
            manifest.integrations[0]
                .credentials
                .as_ref()
                .map(|credentials| credentials.scope.as_str()),
            Some("platform")
        );
        assert_eq!(manifest.capabilities[0].name, "database");
        assert_eq!(
            manifest
                .architecture
                .as_ref()
                .and_then(|architecture| architecture.mode.as_deref()),
            Some("modular_monolith")
        );
        assert_eq!(manifest.services[0].name, "crm");
        assert_eq!(manifest.services[0].owns, ["customer"]);
        assert_eq!(manifest.services[0].exposes[0].kind, "query");
        assert_eq!(
            manifest
                .communication
                .as_ref()
                .and_then(|communication| communication.internal.as_deref()),
            Some("sync rpc")
        );
        assert_eq!(manifest.runtime[0].name, "api");
        assert_eq!(manifest.runtime[0].serves, ["queries", "commands"]);
        assert_eq!(
            manifest
                .deploy
                .as_ref()
                .and_then(|deploy| deploy.rollback.as_deref()),
            Some("on_failed_healthcheck")
        );
    }

    #[test]
    fn parse_app_observability_block() {
        let source = r#"
app crm
  observability
    error_source dev,staging
    panic_recover false
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let observability = manifest.observability.expect("observability block");
        assert_eq!(observability.error_source, ["dev", "staging"]);
        assert!(!observability.panic_recover);
    }

    #[test]
    fn parses_workspace_contract() {
        let source = r#"
workspace AcmeERP
  apps
    crm at "./apps/crm/app.lzi"
    ai external contract "acme.ai.v1"
  shared_registry "./registry.lzi"
  boundaries
    crm publishes customer.*
    ai consumes customer.*
  communication
    propagate actor, tenant, trace_id, request_id
    default sync internal rpc
    default async event_bus
  gateway public_api
    route "/api/customers/*" to app crm
      auth propagate
      tenant propagate
      timeout "5s"
"#;

        let workspace = parse_app_workspace(source).unwrap();

        assert_eq!(workspace.name, "AcmeERP");
        assert_eq!(workspace.apps[0].name, "crm");
        assert_eq!(workspace.apps[0].kind, "local");
        assert_eq!(
            workspace.apps[0].path.as_deref(),
            Some("./apps/crm/app.lzi")
        );
        assert_eq!(workspace.apps[1].name, "ai");
        assert_eq!(workspace.apps[1].kind, "external");
        assert_eq!(workspace.apps[1].contract.as_deref(), Some("acme.ai.v1"));
        assert_eq!(workspace.shared_registry.as_deref(), Some("./registry.lzi"));
        assert_eq!(workspace.boundaries[0].direction, "publishes");
        assert_eq!(
            workspace
                .communication
                .as_ref()
                .and_then(|communication| communication.sync_default.as_deref()),
            Some("internal rpc")
        );
        assert_eq!(workspace.gateways[0].name, "public_api");
        assert_eq!(workspace.gateways[0].routes[0].path, "/api/customers/*");
        assert_eq!(workspace.gateways[0].routes[0].target, "crm");
        assert_eq!(
            workspace.gateways[0].routes[0].auth.as_deref(),
            Some("propagate")
        );
    }

    #[test]
    fn parses_external_contract() {
        let source = r#"
contract acme.ai.v1
  purpose "AI inference service."
  compatibility backward
  import openapi "./contracts/ai.openapi.json"

  record CustomerSummaryRequest
    customer_id: ID required
    email: @semantic.Email @pii.contact optional

  record CustomerSummaryResult
    summary: Text required
    generated_at: DateTime required

  operation summarize_customer
    transport http
    method POST
    path "/v1/customer-summary"
    input CustomerSummaryRequest
    output CustomerSummaryResult
    auth service
    timeout "10s"

  event summary_ready
    topic "ai.summary_ready"
    payload
      customer_id: ID required
      summary: Text required
"#;

        let contracts = parse_app_contracts(source);
        let contract = &contracts[0];

        assert_eq!(contract.name, "acme.ai.v1");
        assert_eq!(contract.purpose.as_deref(), Some("AI inference service."));
        assert_eq!(contract.compatibility.as_deref(), Some("backward"));
        assert_eq!(contract.imports[0].format, "openapi");
        assert_eq!(contract.records[0].name, "CustomerSummaryRequest");
        assert_eq!(contract.records[0].fields[1].type_name, "@semantic.Email");
        assert_eq!(contract.records[0].fields[1].markers, ["@pii.contact"]);
        assert_eq!(contract.operations[0].transport.as_deref(), Some("http"));
        assert_eq!(
            contract.operations[0].path.as_deref(),
            Some("/v1/customer-summary")
        );
        assert_eq!(
            contract.events[0].topic.as_deref(),
            Some("ai.summary_ready")
        );
        assert_eq!(contract.events[0].payload[0].name, "customer_id");
    }

    #[test]
    fn parses_package_registry() {
        let source = r#"
registry
  env
    group mercadopago
      server MERCADOPAGO_ACCESS_TOKEN: Secret required in production
  capabilities
    payment_gateway mercadopago
  packs
    payments from @runtime/payments
      version "0.1.0"
      provides feature payments
      requires integration gateway: PaymentGateway
  integrations
    mercadopago: PaymentGateway
      adapter @runtime/mercadopago
      environments sandbox, production
      credentials platform
        access_token env.MERCADOPAGO_ACCESS_TOKEN
"#;

        let registry = parse_app_registry(source).unwrap();

        assert_eq!(registry.env[0].group.as_deref(), Some("mercadopago"));
        assert_eq!(registry.capabilities[0].name, "payment_gateway");
        assert_eq!(registry.packs[0].name, "payments");
        assert_eq!(registry.packs[0].source, "@runtime/payments");
        assert_eq!(registry.packs[0].version.as_deref(), Some("0.1.0"));
        assert_eq!(registry.packs[0].provides[0].kind, "feature");
        assert_eq!(registry.packs[0].provides[0].name, "payments");
        assert_eq!(registry.packs[0].requirements[0].kind, "integration");
        assert_eq!(registry.packs[0].requirements[0].name, "gateway");
        assert_eq!(registry.packs[0].requirements[0].contract, "PaymentGateway");
        assert_eq!(registry.integrations[0].name, "mercadopago");
        assert_eq!(registry.integrations[0].kind, "PaymentGateway");
        assert_eq!(
            registry.integrations[0].adapter_provenance.as_deref(),
            Some("runtime")
        );
        assert_eq!(
            registry.integrations[0]
                .credentials
                .as_ref()
                .and_then(|credentials| credentials.bindings.first())
                .map(|binding| binding.source.as_str()),
            Some("env.MERCADOPAGO_ACCESS_TOKEN")
        );
    }

    #[test]
    fn parses_webhook_event_registry_kind_with_payload_and_version() {
        let source = r#"
registry MyApp
  webhook_event customer.created
    payload
      customer_id: ID
      email: @semantic.Email
      created_at: DateTime
    version 1
    deprecated false
"#;

        let registry = parse_app_registry(source).unwrap();
        let event = &registry.webhook_events[0];

        assert_eq!(event.name, "customer.created");
        assert_eq!(event.version, 1);
        assert_eq!(event.previous_version, None);
        assert!(!event.deprecated);
        assert_eq!(event.payload.len(), 3);
        assert_eq!(event.payload[1].name, "email");
        assert_eq!(event.payload[1].type_text, "@semantic.Email");
        assert!(event.payload[1].required);
    }

    #[test]
    fn parses_webhook_event_registry_kind_with_previous_version() {
        let source = r#"
registry
  webhook_event customer.archived
    payload
      customer_id: ID
      reason: Text
    version 2
    previous_version 1
"#;

        let registry = parse_app_registry(source).unwrap();
        let event = &registry.webhook_events[0];

        assert_eq!(event.name, "customer.archived");
        assert_eq!(event.version, 2);
        assert_eq!(event.previous_version, Some(1));
    }

    #[test]
    fn parses_webhook_event_registry_kind_with_deprecated_true() {
        let source = r#"
registry
  webhook_event customer.deleted
    payload
      customer_id: ID
    version 3
    previous_version 2
    deprecated true
"#;

        let registry = parse_app_registry(source).unwrap();
        let event = &registry.webhook_events[0];

        assert_eq!(event.name, "customer.deleted");
        assert!(event.deprecated);
    }

    #[test]
    fn parses_legacy_webhook_events_block_as_registry_payload() {
        let source = r#"
registry
  webhook_events
    crm_customer_upsert
      external_id: Text required
      email: @semantic.Email @pii.contact optional
"#;

        let registry = parse_app_registry(source).unwrap();
        let event = &registry.webhook_events[0];

        assert_eq!(event.name, "crm_customer_upsert");
        assert_eq!(event.version, 1);
        assert_eq!(event.payload.len(), 2);
        assert_eq!(event.payload[1].capabilities, ["@pii.contact"]);
        assert!(!event.payload[1].required);
    }

    #[test]
    fn parses_app_profiles() {
        let source = r#"
profile local
  urls
    web "http://localhost:3000"
    api "http://localhost:8080"
  bindings
    customer_import.crm = integrations.fake_crm
  integrations
    crm environment sandbox
    crm adapter @adapter.fake_crm
  deploy
    topology monolith
    migrations before_deploy

profile production
  urls
    web "https://app.acme.example"
  integrations
    crm environment production
  deploy
    topology split_services
"#;

        let profiles = parse_app_profiles(source);

        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].name, "local");
        assert_eq!(profiles[0].urls[0].target, "web");
        assert_eq!(profiles[0].bindings[0].target_feature, "customer_import");
        assert_eq!(profiles[0].integrations[0].name, "crm");
        assert_eq!(
            profiles[0].integrations[0].environment.as_deref(),
            Some("sandbox")
        );
        assert_eq!(
            profiles[0].integrations[0].adapter.as_deref(),
            Some("@adapter.fake_crm")
        );
        assert_eq!(
            profiles[0].integrations[0].adapter_provenance.as_deref(),
            Some("local")
        );
        assert_eq!(
            profiles[0]
                .deploy
                .as_ref()
                .and_then(|deploy| deploy.topology.as_deref()),
            Some("monolith")
        );
        assert_eq!(profiles[1].name, "production");
        assert_eq!(
            profiles[1]
                .deploy
                .as_ref()
                .and_then(|deploy| deploy.topology.as_deref()),
            Some("split_services")
        );
    }

    // Encryption bucket cycle — parses an `encryption` block with one
    // binding per `@key.<scope>`. Indent-2 `encryption` opens the
    // block; indent-4 `key @key.<scope>` opens a binding; indent-6
    // `source` / `algorithm` / `rotation` populates the binding.
    // See `docs/proposals/encryption-vocab.md` §Lowering.
    #[test]
    fn parses_encryption_block_with_one_tenant_binding() {
        use lazuli_ir::{EncryptionAlgorithm, EncryptionRotation, EncryptionTemplateAxis};

        let source = r#"
app AcmeCRM
  title "Acme CRM"
  encryption
    key @key.tenant
      source env.CRYPT_KEY_TENANT_{tenant_id}
      algorithm aes_256_gcm
      rotation manual
"#;

        let manifest = parse_app_manifest(source).unwrap();
        assert_eq!(manifest.encryption_bindings.len(), 1);
        let binding = &manifest.encryption_bindings[0];
        assert_eq!(binding.scope, "@key.tenant");
        assert_eq!(binding.algorithm, EncryptionAlgorithm::Aes256Gcm);
        assert_eq!(binding.rotation, EncryptionRotation::Manual);
        let template = binding.source.template();
        assert_eq!(template.literal, "CRYPT_KEY_TENANT_{tenant_id}");
        assert_eq!(template.axes, vec![EncryptionTemplateAxis::TenantId]);
    }

    #[test]
    fn parses_encryption_block_with_multiple_bindings() {
        let source = r#"
app AcmeCRM
  encryption
    key @key.app
      source env.CRYPT_KEY_APP
      algorithm aes_256_gcm
    key @key.tenant
      source env.CRYPT_KEY_TENANT_{tenant_id}
      algorithm aes_256_gcm
"#;

        let manifest = parse_app_manifest(source).unwrap();
        assert_eq!(manifest.encryption_bindings.len(), 2);
        assert_eq!(manifest.encryption_bindings[0].scope, "@key.app");
        assert_eq!(manifest.encryption_bindings[1].scope, "@key.tenant");
        assert!(manifest.encryption_bindings[0]
            .source
            .template()
            .axes
            .is_empty());
        assert_eq!(
            manifest.encryption_bindings[1].source.template().literal,
            "CRYPT_KEY_TENANT_{tenant_id}"
        );
    }

    #[test]
    fn encryption_block_absent_yields_empty_catalog() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
"#;
        let manifest = parse_app_manifest(source).unwrap();
        assert!(manifest.encryption_bindings.is_empty());
    }

    #[test]
    fn encryption_block_rejects_non_at_key_scope() {
        let source = r#"
app AcmeCRM
  encryption
    key tenant
      source env.CRYPT_KEY_TENANT
      algorithm aes_256_gcm
"#;
        let manifest = parse_app_manifest(source).unwrap();
        // Header without `@key.` prefix is silently dropped; doctor
        // surfaces this as a separate diagnostic. The block parser
        // only records well-shaped bindings.
        assert!(manifest.encryption_bindings.is_empty());
    }

    // -------------------------------------------------------------
    // Roadmap §1.10 — `app.headers` parser tests. Three+ cases per
    // primitive: scalar children parse, `hsts` inline + body forms,
    // closed-catalog values preserved verbatim.
    // -------------------------------------------------------------

    #[test]
    fn parses_app_headers_scalar_children() {
        let source = r#"
app AcmeCRM
  headers
    csp "default-src 'self'; script-src 'self' 'unsafe-inline'"
    x_frame_options DENY
    x_content_type_options nosniff
    referrer_policy strict-origin-when-cross-origin
    permissions_policy "geolocation=(), camera=()"
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let headers = manifest.headers.expect("headers block");
        assert_eq!(
            headers.csp.as_deref(),
            Some("default-src 'self'; script-src 'self' 'unsafe-inline'")
        );
        assert_eq!(headers.x_frame_options.as_deref(), Some("DENY"));
        assert_eq!(headers.x_content_type_options.as_deref(), Some("nosniff"));
        assert_eq!(
            headers.referrer_policy.as_deref(),
            Some("strict-origin-when-cross-origin")
        );
        assert_eq!(
            headers.permissions_policy.as_deref(),
            Some("geolocation=(), camera=()")
        );
    }

    #[test]
    fn parses_app_headers_hsts_inline() {
        let source = r#"
app AcmeCRM
  headers
    hsts max_age 31536000 include_subdomains preload
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let hsts = manifest
            .headers
            .expect("headers block")
            .hsts
            .expect("hsts sub-block");
        assert_eq!(hsts.max_age, 31_536_000);
        assert!(hsts.include_subdomains);
        assert!(hsts.preload);
    }

    #[test]
    fn parses_app_headers_hsts_body_form() {
        let source = r#"
app AcmeCRM
  headers
    hsts
      max_age 63072000
      include_subdomains
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let hsts = manifest
            .headers
            .expect("headers block")
            .hsts
            .expect("hsts sub-block");
        assert_eq!(hsts.max_age, 63_072_000);
        assert!(hsts.include_subdomains);
        assert!(!hsts.preload);
    }

    #[test]
    fn parses_app_headers_absent_yields_none() {
        let source = r#"
app AcmeCRM
  title "AcmeCRM"
"#;
        let manifest = parse_app_manifest(source).unwrap();
        assert!(manifest.headers.is_none());
    }

    // -------------------------------------------------------------
    // Roadmap §1.10 — `registry.secret_rotation` parser tests.
    // Three+ cases per primitive: single profile parses, multiple
    // profiles round-trip, encryption.key binding picks up the
    // referenced profile name.
    // -------------------------------------------------------------

    #[test]
    fn parses_registry_secret_rotation_default_profile() {
        let source = r#"
registry
  secret_rotation default
    cadence 90d
    overlap 24h
    auto_rollback true
"#;
        let registry = parse_app_registry(source).expect("registry");
        assert_eq!(registry.secret_rotations.len(), 1);
        let profile = &registry.secret_rotations[0];
        assert_eq!(profile.name, "default");
        assert_eq!(profile.cadence, "90d");
        assert_eq!(profile.overlap, "24h");
        assert!(profile.auto_rollback);
    }

    #[test]
    fn parses_registry_secret_rotation_multiple_profiles() {
        let source = r#"
registry
  secret_rotation default
    cadence 90d
    overlap 24h
    auto_rollback true

  secret_rotation tenant_keys
    cadence 30d
    overlap 0h
    auto_rollback false
"#;
        let registry = parse_app_registry(source).expect("registry");
        assert_eq!(registry.secret_rotations.len(), 2);
        assert_eq!(registry.secret_rotations[0].name, "default");
        assert_eq!(registry.secret_rotations[1].name, "tenant_keys");
        assert_eq!(registry.secret_rotations[1].cadence, "30d");
        assert_eq!(registry.secret_rotations[1].overlap, "0h");
        assert!(!registry.secret_rotations[1].auto_rollback);
    }

    #[test]
    fn parses_registry_secret_rotation_absent_yields_empty_catalog() {
        let source = r#"
registry
  env
    server CRYPT_KEY: Secret required
"#;
        let registry = parse_app_registry(source).expect("registry");
        assert!(registry.secret_rotations.is_empty());
    }

    #[test]
    fn parses_app_encryption_key_with_rotation_profile() {
        let source = r#"
app AcmeCRM
  encryption
    key @key.tenant
      source env.CRYPT_KEY_TENANT_{tenant_id}
      algorithm aes_256_gcm
      rotation manual
      rotation_profile default
"#;
        let manifest = parse_app_manifest(source).unwrap();
        assert_eq!(manifest.encryption_bindings.len(), 1);
        assert_eq!(
            manifest.encryption_bindings[0].rotation_profile.as_deref(),
            Some("default")
        );
    }

    // -------------------------------------------------------------
    // Roadmap §1.2 — `cookie` block parser tests.
    // -------------------------------------------------------------

    #[test]
    fn parses_cookie_block_with_default_profile() {
        let source = r#"
app AcmeCRM
  cookie
    default
      signed true
      secure true
      http_only true
      same_site strict
      max_age "7d"
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let cookie = manifest.cookie.expect("cookie block populated");
        assert_eq!(cookie.profiles.len(), 1);
        let default = &cookie.profiles[0];
        assert_eq!(default.name, "default");
        assert_eq!(default.signed, Some(true));
        assert_eq!(default.secure, Some(true));
        assert_eq!(default.http_only, Some(true));
        assert_eq!(default.same_site.as_deref(), Some("strict"));
        assert_eq!(default.max_age.as_deref(), Some("7d"));
    }

    #[test]
    fn parses_cookie_block_with_multiple_profiles() {
        let source = r#"
app AcmeCRM
  cookie
    default
      signed true
      same_site lax
      max_age "24h"
    session
      same_site strict
      max_age "12h"
    csrf
      http_only true
      same_site strict
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let cookie = manifest.cookie.expect("cookie block populated");
        let names: Vec<&str> = cookie.profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["default", "session", "csrf"]);
        assert_eq!(cookie.profiles[1].same_site.as_deref(), Some("strict"));
        assert_eq!(cookie.profiles[1].max_age.as_deref(), Some("12h"));
        assert_eq!(cookie.profiles[2].http_only, Some(true));
        // `session` doesn't declare `signed`, so the slot stays None.
        assert_eq!(cookie.profiles[1].signed, None);
    }

    #[test]
    fn cookie_block_absent_yields_none() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
"#;
        let manifest = parse_app_manifest(source).unwrap();
        assert!(manifest.cookie.is_none());
    }

    // -------------------------------------------------------------
    // Roadmap §1.2 — `proxy` block parser tests.
    // -------------------------------------------------------------

    #[test]
    fn parses_proxy_block_with_trusted_cidrs() {
        let source = r#"
app AcmeCRM
  proxy
    trusted 10.0.0.0/8, 172.16.0.0/12
    real_ip_header X-Forwarded-For
    forwarded_proto_header X-Forwarded-Proto
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let proxy = manifest.proxy.expect("proxy block populated");
        assert_eq!(proxy.trusted, vec!["10.0.0.0/8", "172.16.0.0/12"]);
        assert_eq!(proxy.real_ip_header.as_deref(), Some("X-Forwarded-For"));
        assert_eq!(
            proxy.forwarded_proto_header.as_deref(),
            Some("X-Forwarded-Proto")
        );
        assert!(proxy.forwarded_host_header.is_none());
    }

    #[test]
    fn parses_proxy_block_with_all_four_headers() {
        let source = r#"
app AcmeCRM
  proxy
    trusted 192.168.0.0/16
    real_ip_header X-Real-IP
    forwarded_proto_header X-Forwarded-Proto
    forwarded_host_header X-Forwarded-Host
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let proxy = manifest.proxy.expect("proxy block populated");
        assert_eq!(proxy.trusted, vec!["192.168.0.0/16"]);
        assert_eq!(proxy.real_ip_header.as_deref(), Some("X-Real-IP"));
        assert_eq!(
            proxy.forwarded_host_header.as_deref(),
            Some("X-Forwarded-Host")
        );
    }

    #[test]
    fn proxy_block_absent_yields_none() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
"#;
        let manifest = parse_app_manifest(source).unwrap();
        assert!(manifest.proxy.is_none());
    }

    // -------------------------------------------------------------
    // Roadmap §1.2 — `limits` block parser tests.
    // -------------------------------------------------------------

    #[test]
    fn parses_limits_block_with_all_four_slots() {
        let source = r#"
app AcmeCRM
  limits
    body_size "10mb"
    header_size "16kb"
    upload_size "100mb"
    timeout "30s"
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let limits = manifest.limits.expect("limits block populated");
        assert_eq!(limits.body_size.as_deref(), Some("10mb"));
        assert_eq!(limits.header_size.as_deref(), Some("16kb"));
        assert_eq!(limits.upload_size.as_deref(), Some("100mb"));
        assert_eq!(limits.timeout.as_deref(), Some("30s"));
    }

    #[test]
    fn parses_limits_block_with_partial_slots() {
        let source = r#"
app AcmeCRM
  limits
    body_size "5mb"
    timeout "10s"
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let limits = manifest.limits.expect("limits block populated");
        assert_eq!(limits.body_size.as_deref(), Some("5mb"));
        assert_eq!(limits.timeout.as_deref(), Some("10s"));
        // Unset slots stay None.
        assert!(limits.header_size.is_none());
        assert!(limits.upload_size.is_none());
    }

    #[test]
    fn limits_block_absent_yields_none() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
"#;
        let manifest = parse_app_manifest(source).unwrap();
        assert!(manifest.limits.is_none());
    }
}
