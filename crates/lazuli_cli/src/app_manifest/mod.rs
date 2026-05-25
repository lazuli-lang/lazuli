//! Operational manifest parsers for `app.lzi`, `registry.lzi`, `workspace.lzi`,
//! `contracts/*.lzi`, and `profiles.lzi`.
//!
//! Rails parity: each entry point (`parse_app_manifest`, `parse_app_registry`,
//! `parse_app_workspace`, `parse_app_contracts`, `parse_app_profiles`) lives in
//! its own sub-file. Shared low-level line/identifier helpers live in
//! `parsers.rs`. Side-channel doctor-visible defect types live in `types.rs`.
//!
//! All parsers are deliberately line-oriented and lenient: they preserve
//! enough source signal to feed doctor without ever erroring out on a
//! malformed block. Validation is doctor's job; the parser only refuses to
//! emit IR for shapes that cannot be represented at all.
//!
//! See: `lazuli_ir::nodes::app_manifest`,
//!      `lazuli_syntax::ast::feature::PackageSkeleton`.

mod contracts;
mod manifest;
mod parsers;
mod profiles;
mod types;
mod workspace;

pub use contracts::parse_app_contracts;
pub use manifest::parse_app_manifest;
pub use profiles::parse_app_profiles;
pub use types::{RegistryParseOutput, RegistryToolDefectReason, RegistryToolEntryDefect};
pub use workspace::parse_app_workspace;

use parsers::{
    adapter_source_provenance, leading_spaces, parse_app_env_var, parse_bindings_sugar_line,
    parse_bool, parse_credential_binding, parse_env_group_name, parse_integration_header,
    parse_pack_header, parse_pack_provide, parse_pack_requirement, parse_webhook_event_field,
    registry_child, split_items, unquote, webhook_event_name,
};

use lazuli_ir::{
    AppCapability, AppIntegration, AppIntegrationCredentialBinding, AppIntegrationCredentials,
    AppPack, AppRegistry, QualifiedName, RegistryToolEntry, SecretRotation, ToolEffect,
    WebhookEvent,
};


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
                    } else if let Some(bindings) = parse_bindings_sugar_line(trimmed) {
                        // B1 (W3-blockers) — `bindings` registry sugar.
                        // The author writes `endpoint env.X` or
                        // `auth keys env.A env.B` at indent-6 directly
                        // under the integration header instead of nesting
                        // under `credentials platform`. The parser lowers
                        // each sugar line into the equivalent credential
                        // binding(s); the synthesized credentials block
                        // defaults to `platform` scope.
                        let credentials = integration
                            .credentials
                            .get_or_insert_with(|| AppIntegrationCredentials {
                                scope: "platform".to_owned(),
                                bindings: Vec::new(),
                            });
                        credentials.bindings.extend(bindings.into_iter().map(
                            |(name, source)| AppIntegrationCredentialBinding { name, source },
                        ));
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
    fn parses_app_route_guard_and_actor_query() {
        let source = r#"
app AcmeCRM
  actor_query "account.query.me"
  route_guard
    default_policy @policy.authenticated
    on_unauthenticated redirect "/sign-in"
    on_unauthorized redirect "/403"
    skeleton @client.route_guard_skeleton
"#;

        let manifest = parse_app_manifest(source).unwrap();
        let route_guard = manifest.route_guard.as_ref().expect("route_guard");

        assert_eq!(manifest.actor_query.as_deref(), Some("account.query.me"));
        assert_eq!(
            route_guard.default_policy.as_deref(),
            Some("@policy.authenticated")
        );
        assert_eq!(route_guard.on_unauthenticated.as_deref(), Some("/sign-in"));
        assert_eq!(route_guard.on_unauthorized.as_deref(), Some("/403"));
        assert_eq!(
            route_guard.skeleton.as_deref(),
            Some("@client.route_guard_skeleton")
        );
        assert!(route_guard.span_ref.is_some());
    }

    #[test]
    fn auth_failed_redirect_lowers_to_route_guard_when_absent() {
        let source = r#"
app AcmeCRM
  auth_failed_redirect public_login
"#;

        let manifest = parse_app_manifest(source).unwrap();
        let route_guard = manifest.route_guard.as_ref().expect("route_guard");

        assert_eq!(
            route_guard.on_unauthenticated.as_deref(),
            Some("public_login")
        );
        assert!(route_guard.span_ref.is_some());
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
    fn parses_registry_bindings_sugar_lowers_to_integration_credentials() {
        // B1 (W3-blockers) — `bindings` is registry-level sugar over
        // `integrations`. The simplified shape (endpoint + auth keys)
        // lowers to the canonical `credentials platform` + bindings.
        let source = r#"
registry
  bindings
    object_store: ObjectStore
      adapter @lazuli/plugin-object-store
      endpoint env.S3_ENDPOINT
      auth keys env.S3_ACCESS_KEY_ID env.S3_SECRET_ACCESS_KEY
"#;

        let registry = parse_app_registry(source).expect("registry");
        assert_eq!(registry.integrations.len(), 1);
        let integration = &registry.integrations[0];
        assert_eq!(integration.name, "object_store");
        assert_eq!(integration.kind, "ObjectStore");
        assert_eq!(integration.adapter.as_deref(), Some("@lazuli/plugin-object-store"));
        assert_eq!(integration.adapter_provenance.as_deref(), Some("plugin"));

        let credentials = integration
            .credentials
            .as_ref()
            .expect("sugar must synthesize implicit `credentials platform`");
        assert_eq!(credentials.scope, "platform");

        // Sugar lowers to three credential bindings in declaration order:
        // endpoint (from `endpoint`), access_key_id + secret_access_key
        // (from positional `auth keys`).
        let by_name: std::collections::BTreeMap<&str, &str> = credentials
            .bindings
            .iter()
            .map(|binding| (binding.name.as_str(), binding.source.as_str()))
            .collect();
        assert_eq!(by_name.get("endpoint"), Some(&"env.S3_ENDPOINT"));
        assert_eq!(
            by_name.get("access_key_id"),
            Some(&"env.S3_ACCESS_KEY_ID")
        );
        assert_eq!(
            by_name.get("secret_access_key"),
            Some(&"env.S3_SECRET_ACCESS_KEY")
        );
    }

    #[test]
    fn registry_bindings_additive_with_integrations_block() {
        // The legacy `integrations` block must still parse alongside the
        // new `bindings` block — additive, not breaking.
        let source = r#"
registry
  integrations
    payment_gateway: PaymentGateway
      adapter @lazuli/plugin-mercadopago
  bindings
    object_store: ObjectStore
      adapter @lazuli/plugin-object-store
      endpoint env.S3_ENDPOINT
      auth keys env.S3_ACCESS_KEY_ID env.S3_SECRET_ACCESS_KEY
"#;

        let registry = parse_app_registry(source).expect("registry");
        assert_eq!(registry.integrations.len(), 2);
        assert_eq!(registry.integrations[0].name, "payment_gateway");
        assert_eq!(registry.integrations[1].name, "object_store");
        // Legacy integration carries no synthesized credentials.
        assert!(registry.integrations[0].credentials.is_none());
        // Sugar integration carries the synthesized `platform` scope.
        assert_eq!(
            registry.integrations[1]
                .credentials
                .as_ref()
                .map(|credentials| credentials.scope.as_str()),
            Some("platform")
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
