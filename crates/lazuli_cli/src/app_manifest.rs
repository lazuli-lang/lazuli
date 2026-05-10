use lazuli_ir::{
    AppArchitecture, AppBinding, AppCapability, AppCommunication, AppContract, AppDeploy,
    AppEnvVar, AppIntegration, AppIntegrationCredentialBinding, AppIntegrationCredentials,
    AppManifest, AppPack, AppPackProvide, AppPackUse, AppProfile, AppProfileDeploy,
    AppProfileIntegration, AppProfileUrl, AppRegistry, AppRuntimeUnit, AppService,
    AppServiceExposure, AppUrl, AppWorkspace, ContractEvent, ContractField, ContractImport,
    ContractOperation, ContractOperationError, ContractRecord, FeatureRequirement, WorkspaceApp,
    WorkspaceBoundary, WorkspaceCommunication, WorkspaceGateway, WorkspaceGatewayRoute,
};

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
        targets: Vec::new(),
        default_locale: None,
        default_timezone: None,
        auth_failed_redirect: None,
        not_found: None,
        uses: Vec::new(),
        packs: Vec::new(),
        bindings: Vec::new(),
        architecture: None,
        services: Vec::new(),
        communication: None,
        environments: Vec::new(),
        urls: Vec::new(),
        env: Vec::new(),
        integrations: Vec::new(),
        capabilities: Vec::new(),
        runtime: Vec::new(),
        deploy: None,
        span_ref: None,
    };
    let mut current_child: Option<&str> = None;
    let mut current_runtime_unit: Option<usize> = None;
    let mut current_service: Option<usize> = None;
    let mut current_service_child: Option<&str> = None;
    let mut current_env_group: Option<String> = None;
    let mut current_integration: Option<usize> = None;
    let mut current_integration_child: Option<&str> = None;

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
                if let Some(rest) = trimmed.strip_prefix("title ") {
                    app.title = Some(unquote(rest.trim()).to_owned());
                    current_child = None;
                } else if let Some(rest) = trimmed.strip_prefix("version ") {
                    app.version = Some(unquote(rest.trim()).to_owned());
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
                        _ => {}
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
                }
            }
            _ => {}
        }
    }

    Some(app)
}

pub fn parse_app_registry(source: &str) -> Option<AppRegistry> {
    let lines: Vec<_> = source.lines().collect();
    let start = lines
        .iter()
        .position(|line| leading_spaces(line) == 0 && line.trim_start() == "registry")?;

    let mut registry = AppRegistry {
        env: Vec::new(),
        integrations: Vec::new(),
        capabilities: Vec::new(),
        packs: Vec::new(),
    };
    let mut current_child: Option<&str> = None;
    let mut current_env_group: Option<String> = None;
    let mut current_integration: Option<usize> = None;
    let mut current_integration_child: Option<&str> = None;
    let mut current_pack: Option<usize> = None;

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
                current_env_group = None;
                current_integration = None;
                current_integration_child = None;
                current_pack = None;
                current_child = registry_child(trimmed);
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

    Some(registry)
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
        "env" => Some("env"),
        "integrations" => Some("integrations"),
        "capabilities" => Some("capabilities"),
        "architecture" => Some("architecture"),
        "services" => Some("services"),
        "communication" => Some("communication"),
        "runtime" => Some("runtime"),
        "deploy" => Some("deploy"),
        _ => None,
    }
}

fn registry_child(trimmed: &str) -> Option<&'static str> {
    match trimmed.split_whitespace().next()? {
        "env" => Some("env"),
        "integrations" => Some("integrations"),
        "capabilities" => Some("capabilities"),
        "packs" => Some("packs"),
        _ => None,
    }
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
}
