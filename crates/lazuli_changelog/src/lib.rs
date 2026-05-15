//! IR-diff changelog emitter.
//!
//! Consumes two `lazuli_ir::Module` snapshots (e.g. from `lazuli inspect
//! --format=json` at two git refs) and produces a markdown report
//! classifying changes as `Added`, `Removed`, `Deprecated`, `Breaking`,
//! or `Non-breaking`.

use lazuli_ir as ir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangelogReport {
    pub added: Vec<OperationRef>,
    pub removed: Vec<OperationRef>,
    pub deprecated: Vec<DeprecatedEntry>,
    pub breaking: Vec<BreakingChange>,
    pub non_breaking: Vec<NonBreakingChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationRef {
    pub feature: String,
    pub kind: &'static str,
    pub name: String,
    pub method: &'static str,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeprecatedEntry {
    pub op: OperationRef,
    pub since: Option<String>,
    pub replacement: Option<String>,
    pub sunset: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreakingChange {
    pub op: OperationRef,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonBreakingChange {
    pub op: OperationRef,
    pub reason: String,
}

/// Diff two IR modules. Emits a `ChangelogReport` covering commands
/// (HTTP-bearing commands across all features).
pub fn diff(old: &ir::Module, new: &ir::Module) -> ChangelogReport {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut deprecated = Vec::new();
    let mut breaking = Vec::new();
    let mut non_breaking = Vec::new();

    // Build index by (feature, command-name).
    let old_cmds = index_commands(old);
    let new_cmds = index_commands(new);

    for (key, new_cmd) in &new_cmds {
        match old_cmds.get(key) {
            None => {
                added.push(operation_ref(&key.0, new_cmd));
            }
            Some(old_cmd) => {
                // Check input changes.
                if let Some(reason) = input_breaking_change(&old_cmd.input, &new_cmd.input) {
                    breaking.push(BreakingChange {
                        op: operation_ref(&key.0, new_cmd),
                        reason,
                    });
                }
                // Deprecation flagged in new but not old?
                if new_cmd.deprecated.is_some() && old_cmd.deprecated.is_none() {
                    let dep = new_cmd.deprecated.as_ref().unwrap();
                    deprecated.push(DeprecatedEntry {
                        op: operation_ref(&key.0, new_cmd),
                        since: dep.since.clone(),
                        replacement: dep.replacement.as_ref().map(render_replacement),
                        sunset: dep.sunset.clone(),
                    });
                }
                // Added optional input field is non-breaking.
                if let Some(reason) = input_non_breaking_change(&old_cmd.input, &new_cmd.input) {
                    non_breaking.push(NonBreakingChange {
                        op: operation_ref(&key.0, new_cmd),
                        reason,
                    });
                }
            }
        }
    }

    for (key, old_cmd) in &old_cmds {
        if !new_cmds.contains_key(key) {
            removed.push(operation_ref(&key.0, old_cmd));
        }
    }

    ChangelogReport {
        added,
        removed,
        deprecated,
        breaking,
        non_breaking,
    }
}

/// Render the report as markdown.
pub fn render_markdown(report: &ChangelogReport) -> String {
    let mut out = String::with_capacity(2048);
    out.push_str("# API Changelog\n\n");

    if !report.removed.is_empty() {
        out.push_str("## Removed\n\n");
        for op in &report.removed {
            out.push_str(&format!(
                "- `{} {}` ({}.command.{})\n",
                op.method, op.path, op.feature, op.name
            ));
        }
        out.push('\n');
    }
    if !report.added.is_empty() {
        out.push_str("## Added\n\n");
        for op in &report.added {
            out.push_str(&format!(
                "- `{} {}` ({}.command.{})\n",
                op.method, op.path, op.feature, op.name
            ));
        }
        out.push('\n');
    }
    if !report.deprecated.is_empty() {
        out.push_str("## Deprecated\n\n");
        for entry in &report.deprecated {
            let mut bits = Vec::new();
            if let Some(s) = &entry.since {
                bits.push(format!("since {}", s));
            }
            if let Some(r) = &entry.replacement {
                bits.push(format!("replacement {}", r));
            }
            if let Some(s) = &entry.sunset {
                bits.push(format!("sunset {}", s));
            }
            let suffix = if bits.is_empty() {
                String::new()
            } else {
                format!(" ({})", bits.join(", "))
            };
            out.push_str(&format!(
                "- `{} {}` ({}.command.{}){}\n",
                entry.op.method, entry.op.path, entry.op.feature, entry.op.name, suffix
            ));
        }
        out.push('\n');
    }
    if !report.breaking.is_empty() {
        out.push_str("## Breaking\n\n");
        for change in &report.breaking {
            out.push_str(&format!(
                "- `{} {}` ({}.command.{}) — {}\n",
                change.op.method, change.op.path, change.op.feature, change.op.name, change.reason
            ));
        }
        out.push('\n');
    }
    if !report.non_breaking.is_empty() {
        out.push_str("## Non-breaking\n\n");
        for change in &report.non_breaking {
            out.push_str(&format!(
                "- `{} {}` ({}.command.{}) — {}\n",
                change.op.method, change.op.path, change.op.feature, change.op.name, change.reason
            ));
        }
        out.push('\n');
    }
    out
}

fn index_commands(m: &ir::Module) -> std::collections::BTreeMap<(String, String), ir::Command> {
    let mut out = std::collections::BTreeMap::new();
    for feature in &m.features {
        for cmd in &feature.commands {
            out.insert((feature.name.clone(), cmd.name.clone()), cmd.clone());
        }
    }
    out
}

fn operation_ref(feature: &str, cmd: &ir::Command) -> OperationRef {
    let method = match cmd.kind {
        ir::CommandKind::Create => "POST",
        ir::CommandKind::Update => "PATCH",
        ir::CommandKind::Delete => "DELETE",
        ir::CommandKind::Returns => "POST",
    };
    let mut path = format!("/api/{}/{}", feature, cmd.name);
    for slot in &cmd.route {
        path.push_str(&format!("/{{{}}}", slot.name));
    }
    OperationRef {
        feature: feature.to_owned(),
        kind: "command",
        name: cmd.name.clone(),
        method,
        path,
    }
}

fn render_replacement(r: &ir::DeprecationReplacement) -> String {
    match r {
        ir::DeprecationReplacement::LocalCommand(name) => name.clone(),
        ir::DeprecationReplacement::Qualified(qn) => format!(
            "{}.command.{}",
            qn.feature.clone().unwrap_or_default(),
            qn.name
        ),
        ir::DeprecationReplacement::Url(u) => u.clone(),
    }
}

fn input_breaking_change(old: &ir::CommandInput, new: &ir::CommandInput) -> Option<String> {
    // Detect newly-required fields, removed fields. The minimal classifier
    // — full type-comparison lives in follow-up cuts.
    match (old, new) {
        (ir::CommandInput::Typed(old_slots), ir::CommandInput::Typed(new_slots)) => {
            // Removed required field?
            for old_slot in old_slots {
                if !new_slots.iter().any(|s| s.name == old_slot.name) {
                    return Some(format!("removed input field `{}`", old_slot.name));
                }
            }
            // New required field?
            for new_slot in new_slots {
                if new_slot.required && !old_slots.iter().any(|s| s.name == new_slot.name) {
                    return Some(format!("added required input field `{}`", new_slot.name));
                }
                // Optional -> required?
                if new_slot.required {
                    if let Some(old_slot) = old_slots.iter().find(|s| s.name == new_slot.name) {
                        if !old_slot.required {
                            return Some(format!(
                                "input field `{}` changed from optional to required",
                                new_slot.name
                            ));
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn input_non_breaking_change(old: &ir::CommandInput, new: &ir::CommandInput) -> Option<String> {
    match (old, new) {
        (ir::CommandInput::Typed(old_slots), ir::CommandInput::Typed(new_slots)) => {
            // Added optional field?
            for new_slot in new_slots {
                if !new_slot.required && !old_slots.iter().any(|s| s.name == new_slot.name) {
                    return Some(format!("added optional input field `{}`", new_slot.name));
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::*;

    fn cmd(name: &str, kind: CommandKind) -> Command {
        Command {
            name: name.to_owned(),
            kind,
            route: Vec::new(),
            input: CommandInput::Empty,
            target: None,
            lets: Vec::new(),
            effect: CommandEffect::None,
            policy: PolicyRef::None,
            policy_expr: None,
            emits: Vec::new(),
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
            span_ref: None,
        }
    }

    fn module_with(commands: Vec<Command>) -> Module {
        let feature = Feature {
            name: "customer".to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults::default(),
            uses: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies::default(),
            commands,
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        };
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: vec![feature],
        }
    }

    #[test]
    fn detects_added_command() {
        let old = module_with(vec![cmd("create", CommandKind::Create)]);
        let new = module_with(vec![
            cmd("create", CommandKind::Create),
            cmd("reassign", CommandKind::Update),
        ]);
        let report = diff(&old, &new);
        assert_eq!(report.added.len(), 1);
        assert_eq!(report.added[0].name, "reassign");
        assert!(report.removed.is_empty());
    }

    #[test]
    fn detects_removed_command() {
        let old = module_with(vec![
            cmd("create", CommandKind::Create),
            cmd("reassign", CommandKind::Update),
        ]);
        let new = module_with(vec![cmd("create", CommandKind::Create)]);
        let report = diff(&old, &new);
        assert_eq!(report.removed.len(), 1);
        assert_eq!(report.removed[0].name, "reassign");
    }

    #[test]
    fn detects_new_deprecation() {
        let old = module_with(vec![cmd("reassign", CommandKind::Update)]);
        let mut deprecated_cmd = cmd("reassign", CommandKind::Update);
        deprecated_cmd.deprecated = Some(Deprecation {
            since: Some("2026.04".to_owned()),
            replacement: Some(DeprecationReplacement::LocalCommand(
                "reassign_v2".to_owned(),
            )),
            sunset: Some("2026-12-31".to_owned()),
        });
        let new = module_with(vec![deprecated_cmd]);
        let report = diff(&old, &new);
        assert_eq!(report.deprecated.len(), 1);
        assert_eq!(report.deprecated[0].since.as_deref(), Some("2026.04"));
    }
}
