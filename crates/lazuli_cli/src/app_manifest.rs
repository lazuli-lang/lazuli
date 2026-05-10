use lazuli_ir::{
    AppArchitecture, AppCapability, AppCommunication, AppDeploy, AppEnvVar, AppManifest,
    AppRuntimeUnit, AppService, AppServiceExposure, AppUrl,
};

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
        architecture: None,
        services: Vec::new(),
        communication: None,
        environments: Vec::new(),
        urls: Vec::new(),
        env: Vec::new(),
        capabilities: Vec::new(),
        runtime: Vec::new(),
        deploy: None,
        span_ref: None,
    };
    let mut current_child: Option<&str> = None;
    let mut current_runtime_unit: Option<usize> = None;
    let mut current_service: Option<usize> = None;
    let mut current_service_child: Option<&str> = None;

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
                    let parts: Vec<_> = trimmed.split_whitespace().collect();
                    if parts.len() == 4 && parts[1].ends_with(':') {
                        app.env.push(AppEnvVar {
                            scope: parts[0].to_owned(),
                            name: parts[1].trim_end_matches(':').to_owned(),
                            type_name: parts[2].to_owned(),
                            requiredness: parts[3].to_owned(),
                        });
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
                if current_child == Some("runtime") {
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
                if current_child == Some("services") && current_service_child == Some("exposes") {
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

fn app_child(trimmed: &str) -> Option<&'static str> {
    match trimmed.split_whitespace().next()? {
        "uses" => Some("uses"),
        "targets" => Some("targets"),
        "environments" => Some("environments"),
        "urls" => Some("urls"),
        "env" => Some("env"),
        "capabilities" => Some("capabilities"),
        "architecture" => Some("architecture"),
        "services" => Some("services"),
        "communication" => Some("communication"),
        "runtime" => Some("runtime"),
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

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

#[cfg(test)]
mod tests {
    use super::parse_app_manifest;

    #[test]
    fn parses_operational_manifest() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
  uses
    customer
  targets
    backend go
  environments
    production
  urls
    api production "https://api.acme.example"
  env
    server DATABASE_URL: Secret required
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
        assert_eq!(manifest.targets, ["backend go"]);
        assert_eq!(manifest.environments, ["production"]);
        assert_eq!(manifest.urls[0].url, "https://api.acme.example");
        assert_eq!(manifest.env[0].name, "DATABASE_URL");
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
}
