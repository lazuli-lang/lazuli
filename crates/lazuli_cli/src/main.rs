use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

const DEFAULT_TEMPLATE: &str = include_str!("../../../examples/crm.lzi");

#[derive(Debug, Parser)]
#[command(name = "lazuli")]
#[command(about = "Lazuli application metalinguage compiler")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Parse {
        input: PathBuf,
    },
    Compile {
        input: PathBuf,
        #[arg(long, short)]
        out: PathBuf,
    },
    Inspect {
        input: PathBuf,
        #[arg(long, default_value = "none")]
        expand: String,
        #[arg(long, value_enum, default_value_t = InspectFormat::Json)]
        format: InspectFormat,
    },
    Init {
        path: PathBuf,
    },
    Lsp,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum InspectFormat {
    Json,
    Lazuli,
}

#[derive(Debug, Clone, Copy, Default)]
struct ExpandSet {
    events: bool,
    targets: bool,
    policies: bool,
    tests: bool,
    defaults: bool,
}

impl ExpandSet {
    fn all() -> Self {
        Self {
            events: true,
            targets: true,
            policies: true,
            tests: true,
            defaults: true,
        }
    }

    fn any(self) -> bool {
        self.events || self.targets || self.policies || self.tests || self.defaults
    }

    fn labels(self) -> Vec<&'static str> {
        let mut labels = Vec::new();
        if self.events {
            labels.push("events");
        }
        if self.targets {
            labels.push("targets");
        }
        if self.policies {
            labels.push("policies");
        }
        if self.tests {
            labels.push("tests");
        }
        if self.defaults {
            labels.push("defaults");
        }
        labels
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Parse { input } => parse_command(&input),
        Commands::Compile { input, out } => compile_command(&input, &out),
        Commands::Inspect {
            input,
            expand,
            format,
        } => inspect_command(&input, &expand, format),
        Commands::Init { path } => init_command(&path),
        Commands::Lsp => lsp_command(),
    }
}

fn parse_command(input: &Path) -> Result<()> {
    let app = compile_to_ir(input)?;
    println!("{}", serde_json::to_string_pretty(&app)?);
    Ok(())
}

fn compile_command(input: &Path, out: &Path) -> Result<()> {
    let app = compile_to_ir(input)?;
    let plan = lazuli_planner::plan_initial_generation(&app);

    fs::create_dir_all(out)
        .with_context(|| format!("failed to create output directory {}", out.display()))?;

    for file in lazuli_codegen_go::generate(&app) {
        write_generated_file(out, &file.path, &file.contents)?;
    }

    for file in lazuli_codegen_ts::generate(&app) {
        write_generated_file(out, &file.path, &file.contents)?;
    }

    println!("{}", serde_json::to_string_pretty(&plan)?);
    Ok(())
}

fn inspect_command(input: &Path, expand: &str, format: InspectFormat) -> Result<()> {
    let expansions = parse_expand_set(expand)?;
    let source =
        fs::read_to_string(input).with_context(|| format!("failed to read {}", input.display()))?;

    match format {
        InspectFormat::Json => {
            let report = inspect_canonical_source(&source, input, expansions);
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        InspectFormat::Lazuli => {
            if expansions.any() {
                print!("{}", expand_canonical_source_with(&source, expansions));
            } else {
                print!("{source}");
            }
        }
    }

    Ok(())
}

fn init_command(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
    }

    fs::write(path, DEFAULT_TEMPLATE)
        .with_context(|| format!("failed to write {}", path.display()))?;
    println!("created {}", path.display());
    Ok(())
}

fn lsp_command() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to start Lazuli LSP runtime")?;
    runtime.block_on(lazuli_lsp::serve_stdio());
    Ok(())
}

fn compile_to_ir(input: &Path) -> Result<lazuli_ir::Module> {
    let source =
        fs::read_to_string(input).with_context(|| format!("failed to read {}", input.display()))?;
    let document = lazuli_syntax::parse_document(&source).context("failed to parse .lzi file")?;
    lazuli_analyzer::lower_document(&document).context("failed to analyze .lzi file")
}

fn write_generated_file(root: &Path, relative: &str, contents: &str) -> Result<()> {
    let path = root.join(relative);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn parse_expand_set(value: &str) -> Result<ExpandSet> {
    let mut set = ExpandSet::default();

    for raw_item in value.split(',') {
        let item = raw_item.trim();
        if item.is_empty() || item == "none" {
            continue;
        }

        if item == "all" {
            return Ok(ExpandSet::all());
        }

        match item {
            "events" => set.events = true,
            "targets" => set.targets = true,
            "policies" => set.policies = true,
            "tests" => set.tests = true,
            "defaults" => set.defaults = true,
            _ => bail!(
                "unknown inspect expansion `{item}`; use none, all, events, targets, policies, tests, or defaults"
            ),
        }
    }

    Ok(set)
}

#[derive(Debug, Serialize)]
struct InspectReport {
    schema: &'static str,
    source: String,
    expand: Vec<&'static str>,
    features: Vec<InspectFeature>,
}

#[derive(Debug, Serialize)]
struct InspectFeature {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    defaults: Option<Vec<InspectDefault>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    events: Option<Vec<InspectEvent>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    targets: Option<Vec<InspectTarget>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    policies: Option<Vec<InspectPolicy>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tests: Option<Vec<InspectTests>>,
}

#[derive(Debug, Serialize)]
struct InspectDefault {
    name: String,
    value: String,
    origin: &'static str,
    applies_to: Vec<String>,
}

#[derive(Debug, Serialize)]
struct InspectEvent {
    name: String,
    payload: Vec<InspectPayloadField>,
}

#[derive(Debug, Serialize)]
struct InspectPayloadField {
    name: String,
    ty: String,
    origin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expression: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    condition: Option<String>,
}

#[derive(Debug, Serialize)]
struct InspectTarget {
    command: String,
    target: String,
    origin: String,
}

#[derive(Debug, Serialize)]
struct InspectPolicy {
    subject: String,
    policy: String,
    atoms: Vec<String>,
    origin: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    requires: Vec<InspectPolicyRequirement>,
}

#[derive(Debug, Serialize)]
struct InspectPolicyRequirement {
    policy: String,
    atoms: Vec<String>,
    origin: String,
}

#[derive(Debug, Serialize)]
struct InspectTests {
    subject: String,
    groups: BTreeMap<String, Vec<String>>,
}

fn inspect_canonical_source(source: &str, input: &Path, expansions: ExpandSet) -> InspectReport {
    let lines: Vec<String> = source.lines().map(str::to_owned).collect();

    InspectReport {
        schema: "lazuli.inspect.v0",
        source: input.display().to_string(),
        expand: expansions.labels(),
        features: inspect_features(&lines, expansions),
    }
}

fn inspect_features(lines: &[String], expansions: ExpandSet) -> Vec<InspectFeature> {
    let mut features = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 0 && lines[index].trim_start().starts_with("feature ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                if leading_spaces(&lines[index]) == 0
                    && lines[index].trim_start().starts_with("feature ")
                {
                    break;
                }
                index += 1;
            }

            features.push(inspect_feature(&lines[start..index], expansions));
        } else {
            index += 1;
        }
    }

    features
}

fn inspect_feature(lines: &[String], expansions: ExpandSet) -> InspectFeature {
    let name = lines
        .first()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("unknown")
        .to_owned();
    let policies = collect_policy_atoms(lines);

    InspectFeature {
        name,
        defaults: expansions.defaults.then(|| inspect_defaults(lines)),
        events: expansions.events.then(|| inspect_events(lines)),
        targets: expansions.targets.then(|| inspect_targets(lines)),
        policies: expansions
            .policies
            .then(|| inspect_policies(lines, &policies)),
        tests: expansions.tests.then(|| inspect_tests(lines)),
    }
}

fn inspect_defaults(lines: &[String]) -> Vec<InspectDefault> {
    let resources = collect_resource_names(lines);
    let jobs_and_webhooks = collect_job_and_webhook_names(lines);
    let mut defaults = Vec::new();
    let mut in_defaults = false;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 {
            in_defaults = trimmed == "defaults";
            continue;
        }

        if !in_defaults || leading_spaces(line) != 4 || trimmed.is_empty() {
            continue;
        }

        if trimmed == "timestamps" {
            defaults.push(InspectDefault {
                name: "timestamps".to_owned(),
                value: "true".to_owned(),
                origin: "defaults",
                applies_to: resources.clone(),
            });
        } else if let Some(value) = trimmed.strip_prefix("tenancy ") {
            defaults.push(InspectDefault {
                name: "tenancy".to_owned(),
                value: value.to_owned(),
                origin: "defaults",
                applies_to: resources.clone(),
            });
        } else if let Some(value) = trimmed.strip_prefix("policy ") {
            defaults.push(InspectDefault {
                name: "policy".to_owned(),
                value: value.to_owned(),
                origin: "defaults",
                applies_to: jobs_and_webhooks.clone(),
            });
        }
    }

    defaults
}

fn inspect_events(lines: &[String]) -> Vec<InspectEvent> {
    let event_groups = collect_event_groups(lines);
    let mut events = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.trim_start();
        let Some(name) = event_name(trimmed) else {
            index += 1;
            continue;
        };

        let mut payload = Vec::new();
        for group in &event_groups {
            if name.starts_with(&group.prefix) {
                for entry in &group.payload {
                    payload.push(inspect_inherited_payload_field(
                        entry,
                        format!("event_group:{}", group.pattern),
                    ));
                }
            }
        }

        let mut child_index = index + 1;
        while child_index < lines.len()
            && leading_spaces(&lines[child_index]) > leading_spaces(line)
        {
            if leading_spaces(&lines[child_index]) == leading_spaces(line) + 2 {
                if let Some(field) =
                    inspect_explicit_payload_field(lines[child_index].trim_start(), name)
                {
                    payload.push(field);
                }
            }
            child_index += 1;
        }

        events.push(InspectEvent {
            name: name.to_owned(),
            payload,
        });
        index = child_index;
    }

    events
}

fn inspect_targets(lines: &[String]) -> Vec<InspectTarget> {
    let mut targets = Vec::new();
    let has_id_lookup = feature_has_id_lookup(lines);

    for command in command_blocks(lines) {
        let name = command_name(command[0].trim_start()).unwrap_or("unknown");
        let explicit = command.iter().find_map(|line| {
            if leading_spaces(line) == 4 {
                line.trim_start().strip_prefix("target ").map(str::to_owned)
            } else {
                None
            }
        });

        if let Some(target) = explicit {
            targets.push(InspectTarget {
                command: name.to_owned(),
                target,
                origin: "explicit".to_owned(),
            });
        } else if has_id_lookup && command_needs_inferred_target(command) {
            targets.push(InspectTarget {
                command: name.to_owned(),
                target: "query.by_id(id: route.id)".to_owned(),
                origin: "inferred from local route id and query.lookup by_id".to_owned(),
            });
        }
    }

    targets
}

fn inspect_policies(
    lines: &[String],
    policy_atoms: &BTreeMap<String, Vec<String>>,
) -> Vec<InspectPolicy> {
    let mut policies = Vec::new();

    for command in command_blocks(lines) {
        let name = command_name(command[0].trim_start()).unwrap_or("unknown");

        if let Some(policy) = direct_child_value(command, "policy ") {
            policies.push(InspectPolicy {
                subject: format!("command.{name}"),
                atoms: resolve_policy_atoms(&policy, policy_atoms),
                policy,
                origin: "explicit".to_owned(),
                requires: Vec::new(),
            });
        }
    }

    let mut workflow_name = None;
    let mut workflow_policy = None;

    for line in lines {
        let trimmed = line.trim_start();

        if trimmed.is_empty() {
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("workflow ") {
            workflow_name = trimmed.split_whitespace().nth(1).map(str::to_owned);
            workflow_policy = None;
        } else if leading_spaces(line) == 4 && workflow_name.is_some() {
            if let Some(policy) = trimmed.strip_prefix("policy ") {
                workflow_policy = Some(policy.to_owned());
            } else if is_transition_line(trimmed) {
                let transition = transition_name(trimmed).unwrap_or("unknown");
                let policy = workflow_policy.clone().unwrap_or_else(|| "none".to_owned());
                let mut requires = Vec::new();

                if let Some(required) = transition_requires(trimmed) {
                    requires.push(InspectPolicyRequirement {
                        atoms: resolve_policy_atoms(&required, policy_atoms),
                        policy: required,
                        origin: "transition.requires".to_owned(),
                    });
                }

                policies.push(InspectPolicy {
                    subject: format!(
                        "workflow.{}.{}",
                        workflow_name.as_deref().unwrap_or("unknown"),
                        transition
                    ),
                    atoms: resolve_policy_atoms(&policy, policy_atoms),
                    policy,
                    origin: "workflow.policy".to_owned(),
                    requires,
                });
            }
        } else if leading_spaces(line) <= 2 {
            workflow_name = None;
            workflow_policy = None;
        }
    }

    policies
}

fn inspect_tests(lines: &[String]) -> Vec<InspectTests> {
    let mut tests = Vec::new();
    let mut subject_stack: Vec<(usize, String)> = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() {
            index += 1;
            continue;
        }

        while subject_stack
            .last()
            .is_some_and(|(indent, _)| *indent >= leading)
        {
            subject_stack.pop();
        }

        if let Some(subject) = inspect_subject(trimmed) {
            subject_stack.push((leading, subject));
        }

        if trimmed == "tests" {
            let subject = subject_stack
                .last()
                .map(|(_, subject)| subject.clone())
                .unwrap_or_else(|| "unknown".to_owned());
            let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
            let mut child_index = index + 1;

            while child_index < lines.len() && leading_spaces(&lines[child_index]) > leading {
                let assertion = lines[child_index].trim_start();
                if !assertion.is_empty() {
                    groups
                        .entry(test_group(assertion).to_owned())
                        .or_default()
                        .push(assertion.to_owned());
                }
                child_index += 1;
            }

            tests.push(InspectTests { subject, groups });
            index = child_index;
            continue;
        }

        index += 1;
    }

    tests
}

fn collect_policy_atoms(lines: &[String]) -> BTreeMap<String, Vec<String>> {
    let mut policies = BTreeMap::new();
    let mut in_policies = false;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 {
            in_policies = trimmed == "policies";
            continue;
        }

        if !in_policies || leading_spaces(line) != 4 {
            continue;
        }

        let Some((name, atoms)) = trimmed.split_once(':') else {
            continue;
        };

        if name == "fields" || name.contains(' ') {
            continue;
        }

        policies.insert(
            name.trim().to_owned(),
            atoms
                .split(',')
                .map(|atom| atom.trim().to_owned())
                .collect(),
        );
    }

    policies
}

fn collect_resource_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 4 && trimmed.starts_with("resource ") {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

fn collect_job_and_webhook_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 2
                && (trimmed.starts_with("job ") || trimmed.starts_with("webhook "))
            {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

fn command_blocks(lines: &[String]) -> Vec<&[String]> {
    let mut blocks = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 2 && lines[index].trim_start().starts_with("command ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                let trimmed = lines[index].trim_start();
                if leading_spaces(&lines[index]) == 2 && !trimmed.is_empty() {
                    break;
                }
                index += 1;
            }

            blocks.push(&lines[start..index]);
        } else {
            index += 1;
        }
    }

    blocks
}

fn command_name(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()? == "command" {
        parts.next()
    } else {
        None
    }
}

fn command_needs_inferred_target(lines: &[String]) -> bool {
    let has_route_id = lines
        .iter()
        .any(|line| leading_spaces(line) == 4 && line.trim_start() == "route id: ID");
    let mutates_existing = lines.iter().any(|line| {
        leading_spaces(line) == 4
            && (line.trim_start().starts_with("updates ")
                || line.trim_start().starts_with("deletes "))
    });

    has_route_id && mutates_existing
}

fn direct_child_value(lines: &[String], prefix: &str) -> Option<String> {
    lines.iter().find_map(|line| {
        if leading_spaces(line) == 4 {
            line.trim_start().strip_prefix(prefix).map(str::to_owned)
        } else {
            None
        }
    })
}

fn resolve_policy_atoms(policy: &str, policies: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    policies
        .get(policy)
        .cloned()
        .unwrap_or_else(|| vec![policy.to_owned()])
}

fn inspect_inherited_payload_field(entry: &str, origin: String) -> InspectPayloadField {
    let Some((name, expression)) = entry.split_once('=') else {
        return InspectPayloadField {
            name: entry.to_owned(),
            ty: "Unknown".to_owned(),
            origin,
            expression: None,
            condition: None,
        };
    };
    let (expression, condition) = expression
        .split_once(" when ")
        .map(|(value, condition)| (value.trim(), Some(condition.trim().to_owned())))
        .unwrap_or((expression.trim(), None));

    InspectPayloadField {
        name: name.trim().to_owned(),
        ty: infer_payload_type(name.trim(), expression).to_owned(),
        origin,
        expression: Some(expression.to_owned()),
        condition,
    }
}

fn inspect_explicit_payload_field(line: &str, event_name: &str) -> Option<InspectPayloadField> {
    let (name, rest) = line.split_once(':')?;
    let ty = rest.split_whitespace().next()?;

    Some(InspectPayloadField {
        name: name.trim().to_owned(),
        ty: ty.to_owned(),
        origin: format!("event:{event_name}"),
        expression: None,
        condition: None,
    })
}

fn infer_payload_type(name: &str, expression: &str) -> &'static str {
    if name.ends_with("_id") || expression == "id" || expression.ends_with(".id") {
        "ID"
    } else {
        "Unknown"
    }
}

fn transition_name(trimmed_line: &str) -> Option<&str> {
    trimmed_line.split_once(':')?.0.split_whitespace().next()
}

fn is_transition_line(trimmed_line: &str) -> bool {
    let Some((left, right)) = trimmed_line.split_once(':') else {
        return false;
    };

    !left.trim().is_empty() && right.contains("->")
}

fn transition_requires(trimmed_line: &str) -> Option<String> {
    let (_, rhs) = trimmed_line.split_once(':')?;
    let (_, after_arrow) = rhs.trim().split_once("->")?;
    let mut tokens = after_arrow.split_whitespace();
    tokens.next()?;

    while let Some(token) = tokens.next() {
        if token == "requires" {
            return tokens.next().map(str::to_owned);
        }
    }

    None
}

fn inspect_subject(trimmed_line: &str) -> Option<String> {
    if let Some(name) = command_name(trimmed_line) {
        Some(format!("command.{name}"))
    } else if trimmed_line.starts_with("rule ") {
        Some(format!(
            "rule.{}",
            trimmed_line
                .trim_start_matches("rule ")
                .trim_matches('"')
                .to_owned()
        ))
    } else if view_anchor_line(trimmed_line) {
        trimmed_line
            .split(" id ")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .map(|anchor| format!("view.{anchor}"))
            .or_else(|| Some("view.anchor".to_owned()))
    } else if is_transition_line(trimmed_line) {
        transition_name(trimmed_line).map(|name| format!("transition.{name}"))
    } else {
        None
    }
}

fn view_anchor_line(trimmed_line: &str) -> bool {
    trimmed_line.starts_with("view ") && trimmed_line.contains(" id @anchor.")
}

fn test_group(assertion: &str) -> &'static str {
    if assertion.starts_with("allow @")
        || assertion.starts_with("deny @")
        || assertion.contains(" as @")
    {
        "authz"
    } else if assertion.contains(" from ") {
        "transition"
    } else if assertion.contains(" when ") {
        "predicate"
    } else if assertion.starts_with("accepted by ") || assertion.starts_with("rejected by ") {
        "anchor"
    } else {
        "other"
    }
}

#[cfg(test)]
fn expand_canonical_source(source: &str) -> String {
    expand_canonical_source_with(source, ExpandSet::all())
}

fn expand_canonical_source_with(source: &str, expansions: ExpandSet) -> String {
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let lines: Vec<String> = source.lines().map(str::to_owned).collect();
    let inferred = if expansions.targets {
        infer_local_targets(&lines)
    } else {
        lines
    };
    let expanded = expand_feature_syntax(&inferred, expansions);
    let mut output = expanded.join(newline);

    if source.ends_with('\n') {
        output.push_str(newline);
    }

    output
}

fn infer_local_targets(lines: &[String]) -> Vec<String> {
    let mut output = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 0 && lines[index].trim_start().starts_with("feature ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                if leading_spaces(&lines[index]) == 0
                    && lines[index].trim_start().starts_with("feature ")
                {
                    break;
                }
                index += 1;
            }

            output.extend(infer_local_targets_in_feature(&lines[start..index]));
        } else {
            output.push(lines[index].to_owned());
            index += 1;
        }
    }

    output
}

fn infer_local_targets_in_feature(lines: &[String]) -> Vec<String> {
    if !feature_has_id_lookup(lines) {
        return lines.to_vec();
    }

    let mut output = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 2 && lines[index].trim_start().starts_with("command ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                let trimmed = lines[index].trim_start();
                if leading_spaces(&lines[index]) == 2 && !trimmed.is_empty() {
                    break;
                }
                index += 1;
            }

            output.extend(infer_local_target_in_command(&lines[start..index]));
        } else {
            output.push(lines[index].to_owned());
            index += 1;
        }
    }

    output
}

fn feature_has_id_lookup(lines: &[String]) -> bool {
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 4 && trimmed.starts_with("query.lookup by_id by id:") {
            return true;
        }

        if leading_spaces(line) == 4 && trimmed == "query.lookup by_id" {
            let mut child_index = index + 1;

            while child_index < lines.len() && leading_spaces(&lines[child_index]) > 4 {
                if lines[child_index].trim_start() == "key id = params.id" {
                    return true;
                }
                child_index += 1;
            }
        }
    }

    false
}

fn infer_local_target_in_command(lines: &[String]) -> Vec<String> {
    let has_target = lines
        .iter()
        .any(|line| leading_spaces(line) == 4 && line.trim_start().starts_with("target "));
    let has_route_id = lines
        .iter()
        .any(|line| leading_spaces(line) == 4 && line.trim_start() == "route id: ID");
    let mutates_existing = lines.iter().any(|line| {
        leading_spaces(line) == 4
            && (line.trim_start().starts_with("updates ")
                || line.trim_start().starts_with("deletes "))
    });

    if has_target || !has_route_id || !mutates_existing {
        return lines.to_vec();
    }

    let mut output = Vec::new();
    let mut inserted = false;

    for line in lines {
        if !inserted && leading_spaces(line) == 4 && line.trim_start().starts_with("policy ") {
            output.push("    target query.by_id(id: route.id)".to_owned());
            inserted = true;
        }

        output.push(line.to_owned());
    }

    output
}

fn expand_feature_syntax(lines: &[String], expansions: ExpandSet) -> Vec<String> {
    let mut output = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 0 && lines[index].trim_start().starts_with("feature ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                if leading_spaces(&lines[index]) == 0
                    && lines[index].trim_start().starts_with("feature ")
                {
                    break;
                }
                index += 1;
            }

            output.extend(expand_feature_block(&lines[start..index], expansions));
        } else {
            output.push(lines[index].to_owned());
            index += 1;
        }
    }

    output
}

#[derive(Debug, Clone)]
struct EventGroup {
    pattern: String,
    prefix: String,
    payload: Vec<String>,
}

fn expand_feature_block(lines: &[String], expansions: ExpandSet) -> Vec<String> {
    let event_groups = collect_event_groups(lines);
    let mut output = Vec::new();
    let mut index = 0;
    let mut in_command = false;
    let mut command_inputs = Vec::new();

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if leading == 2 && !trimmed.is_empty() {
            in_command = trimmed.starts_with("command ");
            command_inputs.clear();
        }

        if expansions.events && is_event_group_start(line) {
            index = skip_nested_block(lines, index, 4);
            continue;
        }

        if in_command && leading == 4 && trimmed == "input" {
            command_inputs.clear();
        } else if in_command && leading == 4 && trimmed.starts_with("input ") {
            command_inputs = parse_ident_list(trimmed.trim_start_matches("input "));
        }

        if expansions.defaults
            && let Some(expanded) = expand_lookup_shorthand(line)
        {
            output.extend(expanded);
        } else if expansions.defaults
            && let Some(expanded) = expand_creates_from_input(line, &command_inputs)
        {
            output.extend(expanded);
        } else if expansions.defaults
            && let Some(expanded) = expand_transition_clauses(line)
        {
            output.extend(expanded);
        } else {
            output.push(line.to_owned());

            if expansions.events
                && let Some(event_name) = event_name(trimmed)
            {
                for group in &event_groups {
                    if event_name.starts_with(&group.prefix) {
                        let child_indent = " ".repeat(leading + 2);
                        for payload in &group.payload {
                            output.push(format!("{child_indent}{}", expand_payload_entry(payload)));
                        }
                    }
                }
            }
        }

        index += 1;
    }

    output
}

fn collect_event_groups(lines: &[String]) -> Vec<EventGroup> {
    let mut groups = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = &lines[index];

        if !is_event_group_start(line) {
            index += 1;
            continue;
        }

        let Some((pattern, prefix)) = event_group_pattern(line.trim_start()) else {
            index += 1;
            continue;
        };

        let mut payload = Vec::new();
        let mut payload_block = false;
        let mut child_index = index + 1;

        while child_index < lines.len() && leading_spaces(&lines[child_index]) > 4 {
            let child = &lines[child_index];
            let child_trimmed = child.trim_start();

            if leading_spaces(child) == 6 {
                payload_block = child_trimmed == "payload";
            } else if payload_block && leading_spaces(child) == 8 && !child_trimmed.is_empty() {
                payload.push(child_trimmed.to_owned());
            }

            child_index += 1;
        }

        groups.push(EventGroup {
            pattern,
            prefix,
            payload,
        });
        index = child_index;
    }

    groups
}

fn is_event_group_start(line: &str) -> bool {
    leading_spaces(line) == 4 && line.trim_start().starts_with("events ")
}

fn event_group_pattern(trimmed_line: &str) -> Option<(String, String)> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()? != "events" {
        return None;
    }

    let pattern = parts.next()?;
    pattern
        .strip_suffix('*')
        .map(|prefix| (pattern.to_owned(), prefix.to_owned()))
}

fn skip_nested_block(lines: &[String], start: usize, parent_indent: usize) -> usize {
    let mut index = start + 1;

    while index < lines.len() && leading_spaces(&lines[index]) > parent_indent {
        index += 1;
    }

    index
}

fn event_name(trimmed_line: &str) -> Option<&str> {
    let rest = trimmed_line
        .strip_prefix("event.trace ")
        .or_else(|| trimmed_line.strip_prefix("event "))?;
    rest.split_whitespace().next()
}

fn expand_payload_entry(entry: &str) -> String {
    let Some((name, expression)) = entry.split_once('=') else {
        return entry.to_owned();
    };
    let name = name.trim();
    let expression = expression
        .split_once(" when ")
        .map(|(value, _)| value)
        .unwrap_or(expression)
        .trim();
    let ty = if name.ends_with("_id") || expression == "id" || expression.ends_with(".id") {
        "ID"
    } else {
        "Unknown"
    };

    format!("{name}: {ty}")
}

fn expand_lookup_shorthand(line: &str) -> Option<Vec<String>> {
    let leading = leading_spaces(line);
    let indent = " ".repeat(leading);
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("query.lookup ")?;
    let (name, key) = rest.split_once(" by ")?;
    let (field, ty) = key.split_once(':')?;
    let name = name.trim();
    let field = field.trim();
    let ty = ty.trim();

    if name.is_empty() || field.is_empty() || ty.is_empty() {
        return None;
    }

    Some(vec![
        format!("{indent}query.lookup {name}"),
        format!("{indent}  params"),
        format!("{indent}    {field}: {ty}"),
        String::new(),
        format!("{indent}  key {field} = params.{field}"),
    ])
}

fn expand_creates_from_input(line: &str, inputs: &[String]) -> Option<Vec<String>> {
    if inputs.is_empty() {
        return None;
    }

    let leading = leading_spaces(line);
    let indent = " ".repeat(leading);
    let child_indent = " ".repeat(leading + 2);
    let trimmed = line.trim_start();
    let resource = trimmed
        .strip_prefix("creates ")?
        .strip_suffix(" from input")?
        .trim();

    if resource.is_empty() {
        return None;
    }

    let mut expanded = vec![format!("{indent}creates {resource}")];
    for input in inputs {
        expanded.push(format!("{child_indent}{input} = input.{input}"));
    }
    Some(expanded)
}

fn expand_transition_clauses(line: &str) -> Option<Vec<String>> {
    let leading = leading_spaces(line);
    let indent = " ".repeat(leading);
    let child_indent = " ".repeat(leading + 2);
    let trimmed = line.trim_start();
    let (left, right) = trimmed.split_once(':')?;
    let (from, after_arrow) = right.trim().split_once("->")?;
    let mut tokens = after_arrow.split_whitespace();
    let to = tokens.next()?;
    let remaining: Vec<&str> = tokens.collect();

    if remaining.is_empty() {
        return None;
    }

    let mut requires = None;
    let mut emits = None;
    let mut index = 0;

    while index < remaining.len() {
        match remaining[index] {
            "requires" if index + 1 < remaining.len() && requires.is_none() => {
                requires = Some(remaining[index + 1]);
                index += 2;
            }
            "emits" if index + 1 < remaining.len() && emits.is_none() => {
                emits = Some(remaining[index + 1]);
                index += 2;
            }
            _ => return None,
        }
    }

    let mut expanded = vec![format!(
        "{indent}{}: {} -> {}",
        left.trim(),
        from.trim(),
        to
    )];
    if let Some(policy) = requires {
        expanded.push(format!("{child_indent}requires {policy}"));
    }
    if let Some(event) = emits {
        expanded.push(format!("{child_indent}emits {event}"));
    }

    Some(expanded)
}

fn parse_ident_list(source: &str) -> Vec<String> {
    source
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{ExpandSet, expand_canonical_source, inspect_canonical_source, parse_expand_set};

    #[test]
    fn inspect_expand_rewrites_local_sugars() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required
      email: @semantic.Email required

    query.lookup by_id by id: ID

    events customer_* on Customer
      payload
        customer_id = id
        org_id = org.id

    event customer_created
      email: @semantic.Email

  command create
    input name, email
    policy create
    creates Customer from input

  command rename
    route id: ID
    input name
    policy update
    updates Customer
      name = input.name

  workflow lifecycle on Customer.status
    policy update

    activate: lead -> active requires delete emits customer_activated
"#;

        let expanded = expand_canonical_source(source);

        assert!(expanded.contains("    query.lookup by_id\n      params\n        id: ID"));
        assert!(expanded.contains("    event customer_created\n      customer_id: ID\n      org_id: ID\n      email: @semantic.Email"));
        assert!(
            expanded.contains(
                "    creates Customer\n      name = input.name\n      email = input.email"
            )
        );
        assert!(expanded.contains("    target query.by_id(id: route.id)\n    policy update"));
        assert!(expanded.contains(
            "    activate: lead -> active\n      requires delete\n      emits customer_activated"
        ));
        assert!(!expanded.contains("events customer_* on Customer"));
        assert!(!expanded.contains("from input"));
    }

    #[test]
    fn inspect_json_reports_selected_expansions_with_origin() {
        let source = r#"
feature customer
  purpose "Customers"

  defaults
    tenancy org

  domain
    resource Customer

    query.lookup by_id by id: ID

    events customer_* on Customer
      payload
        customer_id = id

    event customer_created
      email: @semantic.Email

  policies
    update: @role.admin

  command rename
    route id: ID
    input name
    policy update
    updates Customer
      name = input.name
"#;
        let mut expansions = ExpandSet::default();
        expansions.events = true;
        expansions.targets = true;
        expansions.policies = true;
        expansions.defaults = true;

        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_string(&report).unwrap();

        assert!(json.contains("\"schema\":\"lazuli.inspect.v0\""));
        assert!(json.contains("\"origin\":\"event_group:customer_*\""));
        assert!(
            json.contains("\"origin\":\"inferred from local route id and query.lookup by_id\"")
        );
        assert!(json.contains("\"origin\":\"explicit\""));
        assert!(json.contains("\"origin\":\"defaults\""));
    }

    #[test]
    fn inspect_expand_flags_are_explicit() {
        let expansions = parse_expand_set("events,targets").unwrap();

        assert!(expansions.events);
        assert!(expansions.targets);
        assert!(!expansions.tests);
        assert!(parse_expand_set("crud").is_err());
    }
}
