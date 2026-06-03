/// Backwards-compatible entry: returns just the well-formed registry IR.
/// Doctor uses `parse_app_registry_with_defects` to also collect the
/// `tool <name>` entries that lack an `effect` child.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::app_manifest::registry::parse_app_registry;
///
/// // let registry = parse_app_registry(src);
/// ```
pub fn parse_app_registry(source: &str) -> Option<AppRegistry> {
    parse_app_registry_with_defects(source).registry
}

/// Parse a `registry.lzi` body and surface both the well-formed
/// [`AppRegistry`] and the side-channel of [`RegistryToolEntryDefect`]
/// for `tool <name>` entries that can't be represented in IR.
///
/// The defect channel exists because the closed catalog requires every
/// `tool` to declare a valid `effect` — without it, the IR cannot keep
/// the entry. Doctor uses the side channel to render `effect required`
/// diagnostics with line numbers.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::app_manifest::registry::parse_app_registry_with_defects;
///
/// let src = "registry\n  tools\n    tool ping\n      effect read\n";
/// let out = parse_app_registry_with_defects(src);
/// let registry = out.registry.expect("registry");
/// assert_eq!(registry.tools.len(), 1);
/// assert!(out.tool_defects.is_empty());
/// ```
pub fn parse_app_registry_with_defects(source: &str) -> RegistryParseOutput {
    let lines: Vec<_> = source.lines().collect();
    let Some(start) = lines.iter().position(|line| {
        leading_spaces(line) == 0
            && line
                .split_whitespace()
                .next()
                .is_some_and(|keyword| keyword == "registry")
    }) else {
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
        // Spec 0028 Gap B: a `@doctor.allow(...)` waiver node is valid anywhere a
        // `# doctor:allow` comment used to be — skip it (the module loader
        // captures the waiver separately).
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || lazuli_syntax::doctor_allow::is_node_line(trimmed)
        {
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
                    if current_child == Some("secret_rotation")
                        && let Some(rest) = trimmed.strip_prefix("secret_rotation ") {
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
                        } else if let Some(rest) = trimmed.strip_prefix("deprecated ")
                            && let Some(value) = parse_bool(rest.trim()) {
                                registry.webhook_events[idx].deprecated = value;
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
                    } else if let Some(rest) = trimmed.strip_prefix("auto_rollback ")
                        && let Some(value) = parse_bool(rest.trim()) {
                            rotation.auto_rollback = value;
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
                        let credentials = integration.credentials.get_or_insert_with(|| {
                            AppIntegrationCredentials {
                                scope: "platform".to_owned(),
                                bindings: Vec::new(),
                            }
                        });
                        credentials
                            .bindings
                            .extend(bindings.into_iter().map(|(name, source)| {
                                AppIntegrationCredentialBinding { name, source }
                            }));
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
                } else if current_child == Some("webhook_events")
                    || (current_child == Some("webhook_event") && in_webhook_event_payload)
                {
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
