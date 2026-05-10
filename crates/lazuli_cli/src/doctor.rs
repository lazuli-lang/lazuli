use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lazuli_lsp::SecurityProfile;
use lazuli_syntax::{LzxDocument, LzxPlatformView};
use tower_lsp::lsp_types::DiagnosticSeverity;

pub fn doctor_command(input: &Path, security_profile: SecurityProfile) -> Result<()> {
    let package = DoctorPackage::load(input, security_profile)?;
    let diagnostics = package.diagnostics();
    let has_error = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == DoctorSeverity::Error);

    for diagnostic in &diagnostics {
        diagnostic.print();
    }

    if has_error {
        bail!("{} failed Lazuli doctor checks", input.display());
    }

    println!("{} passed Lazuli doctor checks", input.display());
    Ok(())
}

#[derive(Debug)]
struct DoctorPackage {
    files: Vec<DoctorFile>,
    commands: BTreeMap<CommandKey, CommandPolicy>,
    experiences: BTreeMap<String, ExperienceFacts>,
}

impl DoctorPackage {
    fn load(input: &Path, security_profile: SecurityProfile) -> Result<Self> {
        let paths = collect_package_paths(input)?;
        if paths.is_empty() {
            bail!("no .lzi or .lzx files found for {}", input.display());
        }

        let mut files = Vec::new();
        let mut commands = BTreeMap::new();
        let mut experiences = BTreeMap::new();

        for path in paths {
            let source = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let mut file = DoctorFile {
                path,
                source,
                local_diagnostics: Vec::new(),
                lzx: None,
            };

            for diagnostic in
                lazuli_lsp::diagnostics_for_source_with_profile(&file.source, security_profile)
            {
                file.local_diagnostics
                    .push(DoctorDiagnostic::from_lsp(file.path.clone(), &diagnostic));
            }

            if is_lzi_path(&file.path) {
                collect_canonical_facts(&file, &mut commands);
            } else if is_lzx_path(&file.path) {
                match lazuli_syntax::parse_lzx_document(&file.source) {
                    Ok(document) => {
                        collect_lzx_experience_facts(&document, &mut experiences);
                        file.lzx = Some(document);
                    }
                    Err(error) => file.local_diagnostics.push(DoctorDiagnostic {
                        path: file.path.clone(),
                        line: line_col_for_offset(&file.source, error.span().start).0,
                        column: line_col_for_offset(&file.source, error.span().start).1,
                        severity: DoctorSeverity::Error,
                        code: "LZX-PARSE".to_owned(),
                        message: error.to_string(),
                    }),
                }
            }

            files.push(file);
        }

        Ok(Self {
            files,
            commands,
            experiences,
        })
    }

    fn diagnostics(&self) -> Vec<DoctorDiagnostic> {
        let mut diagnostics = Vec::new();

        for file in &self.files {
            diagnostics.extend(file.local_diagnostics.clone());
        }

        diagnostics.extend(policy_reachability_diagnostics(
            &self.files,
            &self.experiences,
            &self.commands,
        ));

        diagnostics.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then(left.line.cmp(&right.line))
                .then(left.column.cmp(&right.column))
                .then(left.code.cmp(&right.code))
        });
        diagnostics
    }
}

#[derive(Debug)]
struct DoctorFile {
    path: PathBuf,
    source: String,
    local_diagnostics: Vec<DoctorDiagnostic>,
    lzx: Option<LzxDocument>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CommandKey {
    feature: String,
    command: String,
}

#[derive(Debug, Clone)]
struct CommandPolicy {
    reference: String,
    atoms: Vec<String>,
    routes: BTreeMap<String, CommandRouteSlot>,
}

#[derive(Debug, Clone)]
struct CommandRouteSlot {
    bound_from_context: bool,
}

#[derive(Debug, Clone)]
struct ResolvedCommandTarget {
    key: CommandKey,
    args: BTreeSet<String>,
}

#[derive(Debug, Clone, Default)]
struct ExperienceFacts {
    view_actions: BTreeMap<String, BTreeMap<String, String>>,
    view_routes: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DoctorSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Debug, Clone)]
struct DoctorDiagnostic {
    path: PathBuf,
    line: usize,
    column: usize,
    severity: DoctorSeverity,
    code: String,
    message: String,
}

impl DoctorDiagnostic {
    fn from_lsp(path: PathBuf, diagnostic: &tower_lsp::lsp_types::Diagnostic) -> Self {
        let severity = match diagnostic.severity {
            Some(DiagnosticSeverity::ERROR) => DoctorSeverity::Error,
            Some(DiagnosticSeverity::WARNING) => DoctorSeverity::Warning,
            Some(DiagnosticSeverity::INFORMATION) => DoctorSeverity::Info,
            Some(DiagnosticSeverity::HINT) => DoctorSeverity::Hint,
            _ => DoctorSeverity::Warning,
        };
        let code = diagnostic
            .code
            .as_ref()
            .map(|code| match code {
                tower_lsp::lsp_types::NumberOrString::String(value) => value.clone(),
                tower_lsp::lsp_types::NumberOrString::Number(value) => value.to_string(),
            })
            .unwrap_or_else(|| "diagnostic".to_owned());

        Self {
            path,
            line: diagnostic.range.start.line as usize + 1,
            column: diagnostic.range.start.character as usize + 1,
            severity,
            code,
            message: diagnostic.message.clone(),
        }
    }

    fn print(&self) {
        let severity = match self.severity {
            DoctorSeverity::Error => "error",
            DoctorSeverity::Warning => "warning",
            DoctorSeverity::Info => "info",
            DoctorSeverity::Hint => "hint",
        };
        println!(
            "{}:{}:{}: {severity} [{}]: {}",
            self.path.display(),
            self.line,
            self.column,
            self.code,
            self.message
        );
    }
}

fn collect_package_paths(input: &Path) -> Result<Vec<PathBuf>> {
    if input.is_dir() {
        let mut paths = Vec::new();
        collect_lazuli_paths_recursive(input, &mut paths)?;
        paths.sort();
        return Ok(paths);
    }

    if !input.exists() {
        bail!("{} does not exist", input.display());
    }

    let Some(parent) = input.parent() else {
        return Ok(vec![input.to_path_buf()]);
    };
    let Some(input_package_stem) = package_stem(input) else {
        return Ok(vec![input.to_path_buf()]);
    };

    let mut paths = Vec::new();
    for entry in
        fs::read_dir(parent).with_context(|| format!("failed to list {}", parent.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read {}", parent.display()))?;
        let path = entry.path();
        if path.is_file()
            && (is_lzi_path(&path) || is_lzx_path(&path))
            && package_stem(&path).as_deref() == Some(input_package_stem.as_str())
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn collect_lazuli_paths_recursive(root: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("failed to list {}", root.display()))? {
        let entry = entry.with_context(|| format!("failed to read {}", root.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_lazuli_paths_recursive(&path, paths)?;
        } else if path.is_file() && (is_lzi_path(&path) || is_lzx_path(&path)) {
            paths.push(path);
        }
    }
    Ok(())
}

fn package_stem(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    if let Some(stem) = file_name.strip_suffix(".lzi") {
        return Some(stem.to_owned());
    }

    let stem = file_name.strip_suffix(".lzx")?;
    Some(stem.split('.').next().unwrap_or(stem).to_owned())
}

fn is_lzi_path(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("lzi")
}

fn is_lzx_path(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("lzx")
}

fn collect_canonical_facts(file: &DoctorFile, commands: &mut BTreeMap<CommandKey, CommandPolicy>) {
    let lines: Vec<_> = file.source.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        if leading_spaces(lines[index]) != 0 || !trimmed.starts_with("feature ") {
            index += 1;
            continue;
        }

        let feature = trimmed
            .split_whitespace()
            .nth(1)
            .unwrap_or("<anonymous>")
            .to_owned();
        let start = index;
        index += 1;
        while index < lines.len()
            && !(leading_spaces(lines[index]) == 0
                && lines[index].trim_start().starts_with("feature "))
        {
            index += 1;
        }

        collect_feature_commands(&feature, &lines[start..index], commands);
    }
}

fn collect_feature_commands(
    feature: &str,
    lines: &[&str],
    commands: &mut BTreeMap<CommandKey, CommandPolicy>,
) {
    let policies = collect_policy_atoms(lines);
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();

        if leading_spaces(line) != 2 || !trimmed.starts_with("command ") {
            index += 1;
            continue;
        }

        let command = trimmed
            .split_whitespace()
            .nth(1)
            .unwrap_or("<anonymous>")
            .to_owned();
        let mut policy_reference = None;
        let mut routes = BTreeMap::new();
        index += 1;

        while index < lines.len() {
            let child = lines[index];
            let child_trimmed = child.trim_start();
            if leading_spaces(child) <= 2 && !child_trimmed.is_empty() {
                break;
            }
            if leading_spaces(child) == 4
                && let Some(policy) = child_trimmed.strip_prefix("policy ")
            {
                policy_reference = Some(policy.trim().to_owned());
            } else if leading_spaces(child) == 4
                && let Some(route) = command_route_slot(child_trimmed)
            {
                routes.insert(route.name, route.slot);
            }
            index += 1;
        }

        let Some(reference) = policy_reference else {
            continue;
        };
        let atoms = resolve_policy_atoms(&reference, &policies);
        commands.insert(
            CommandKey {
                feature: feature.to_owned(),
                command,
            },
            CommandPolicy {
                reference,
                atoms,
                routes,
            },
        );
    }
}

fn collect_policy_atoms(lines: &[&str]) -> BTreeMap<String, Vec<String>> {
    let mut policies = BTreeMap::new();
    let mut in_policies = false;

    for line in lines {
        let trimmed = line.trim_start();
        let indent = leading_spaces(line);

        if indent == 2 {
            in_policies = trimmed == "policies";
            continue;
        }

        if !in_policies || indent != 4 {
            continue;
        }

        let Some((name, atoms)) = trimmed.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if !is_identifier(name) {
            continue;
        }
        policies.insert(
            name.to_owned(),
            atoms
                .split(',')
                .map(str::trim)
                .filter(|atom| atom.starts_with('@'))
                .map(str::to_owned)
                .collect(),
        );
    }

    policies
}

fn resolve_policy_atoms(reference: &str, policies: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    if let Some(policy_name) = reference.strip_prefix("@policy.") {
        return policies.get(policy_name).cloned().unwrap_or_default();
    }

    reference
        .split(',')
        .map(str::trim)
        .filter(|atom| atom.starts_with('@'))
        .map(str::to_owned)
        .collect()
}

fn collect_lzx_experience_facts(
    document: &LzxDocument,
    experiences: &mut BTreeMap<String, ExperienceFacts>,
) {
    for experience in &document.experiences {
        let facts = experiences.entry(experience.name.clone()).or_default();
        for view in &experience.views {
            facts.view_routes.insert(
                view.name.clone(),
                view.routes
                    .iter()
                    .filter_map(|route| route_slot_name(route).map(str::to_owned))
                    .collect(),
            );
            let actions = facts.view_actions.entry(view.name.clone()).or_default();
            for action in &view.actions {
                actions.insert(action.name.clone(), action.target.clone());
            }
        }
    }
}

fn policy_reachability_diagnostics(
    files: &[DoctorFile],
    experiences: &BTreeMap<String, ExperienceFacts>,
    commands: &BTreeMap<CommandKey, CommandPolicy>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for file in files {
        let Some(document) = file.lzx.as_ref() else {
            continue;
        };

        for surface in &document.surfaces {
            let experience_name = surface
                .uses_experience
                .as_deref()
                .unwrap_or(surface.experience.as_str());
            let experience = experiences.get(experience_name);

            for audience in &surface.audiences {
                for view in &audience.views {
                    if let Some(submit) = view.submit.as_deref() {
                        if let Some(target) = resolve_command_target(submit, &surface.experience) {
                            diagnostics.extend(command_reachability_diagnostic(
                                file,
                                view,
                                &audience.name,
                                &audience.qualifiers,
                                "submit",
                                &target.key,
                                commands,
                            ));
                            diagnostics.extend(command_route_binding_diagnostics(
                                file,
                                view,
                                experience.and_then(|facts| facts.view_routes.get(&view.name)),
                                "submit",
                                &target,
                                commands,
                            ));
                        }
                    }

                    for action in &view.actions {
                        let target = resolve_platform_action_target(
                            action,
                            &surface.experience,
                            experience.and_then(|facts| facts.view_actions.get(&view.name)),
                        );
                        if let Some(target) = target {
                            diagnostics.extend(command_reachability_diagnostic(
                                file,
                                view,
                                &audience.name,
                                &audience.qualifiers,
                                "action",
                                &target.key,
                                commands,
                            ));
                            diagnostics.extend(command_route_binding_diagnostics(
                                file,
                                view,
                                experience.and_then(|facts| facts.view_routes.get(&view.name)),
                                "action",
                                &target,
                                commands,
                            ));
                        }
                    }
                }
            }
        }
    }

    diagnostics
}

fn command_reachability_diagnostic(
    file: &DoctorFile,
    view: &LzxPlatformView,
    audience: &str,
    qualifiers: &[String],
    source_kind: &str,
    target: &CommandKey,
    commands: &BTreeMap<CommandKey, CommandPolicy>,
) -> Vec<DoctorDiagnostic> {
    let (line, column) = line_col_for_offset(&file.source, view.span.start);
    let Some(policy) = commands.get(target) else {
        return vec![DoctorDiagnostic {
            path: file.path.clone(),
            line,
            column,
            severity: DoctorSeverity::Warning,
            code: "LZX-POL-002".to_owned(),
            message: format!(
                "{source_kind} targets unresolved command `{}.command.{}`; doctor could not prove policy reachability.",
                target.feature, target.command
            ),
        }];
    };

    if audience_can_reach_policy(audience, qualifiers, &policy.atoms) {
        return Vec::new();
    }

    vec![DoctorDiagnostic {
        path: file.path.clone(),
        line,
        column,
        severity: DoctorSeverity::Error,
        code: "LZX-POL-001".to_owned(),
        message: format!(
            "audience `{audience}` {source_kind} reaches `{}.command.{}`, but its policy `{}` resolves to {}; change the surface target or expose a command policy reachable by this audience.",
            target.feature,
            target.command,
            policy.reference,
            if policy.atoms.is_empty() {
                "no known atoms".to_owned()
            } else {
                policy.atoms.join(", ")
            }
        ),
    }]
}

fn command_route_binding_diagnostics(
    file: &DoctorFile,
    view: &LzxPlatformView,
    view_routes: Option<&BTreeSet<String>>,
    source_kind: &str,
    target: &ResolvedCommandTarget,
    commands: &BTreeMap<CommandKey, CommandPolicy>,
) -> Vec<DoctorDiagnostic> {
    let Some(command) = commands.get(&target.key) else {
        return Vec::new();
    };
    let missing: Vec<_> = command
        .routes
        .iter()
        .filter(|(name, slot)| {
            !slot.bound_from_context
                && !target.args.contains(*name)
                && !view_routes.is_some_and(|routes| routes.contains(*name))
        })
        .map(|(name, _)| name.clone())
        .collect();

    if missing.is_empty() {
        return Vec::new();
    }

    let (line, column) = line_col_for_offset(&file.source, view.span.start);
    vec![DoctorDiagnostic {
        path: file.path.clone(),
        line,
        column,
        severity: DoctorSeverity::Error,
        code: "LZX-ROUTE-001".to_owned(),
        message: format!(
            "{source_kind} reaches `{}.command.{}` but does not bind required command route slot(s) {}; pass them in the target call or bind the command route from context.",
            target.key.feature,
            target.key.command,
            missing.join(", ")
        ),
    }]
}

fn resolve_platform_action_target(
    action: &str,
    default_feature: &str,
    abstract_actions: Option<&BTreeMap<String, String>>,
) -> Option<ResolvedCommandTarget> {
    if let Some((_, target)) = action.split_once("->") {
        return resolve_command_target(target.trim(), default_feature);
    }
    if let Some(target) = resolve_command_target(action, default_feature) {
        return Some(target);
    }
    let target = abstract_actions?.get(action)?;
    resolve_command_target(target, default_feature)
}

fn resolve_command_target(target: &str, default_feature: &str) -> Option<ResolvedCommandTarget> {
    let target = target.trim();
    let (callee, args) = split_target_call(target);

    if let Some(command) = callee.strip_prefix("command.") {
        return Some(ResolvedCommandTarget {
            key: CommandKey {
                feature: default_feature.to_owned(),
                command: command.to_owned(),
            },
            args,
        });
    }

    let parts: Vec<_> = callee.split('.').collect();
    match parts.as_slice() {
        [feature, "command", command] => Some(ResolvedCommandTarget {
            key: CommandKey {
                feature: (*feature).to_owned(),
                command: (*command).to_owned(),
            },
            args,
        }),
        _ => None,
    }
}

fn split_target_call(target: &str) -> (&str, BTreeSet<String>) {
    let Some((callee, rest)) = target.split_once('(') else {
        return (target, BTreeSet::new());
    };
    let args = rest
        .trim_end_matches(')')
        .split(',')
        .filter_map(|arg| {
            arg.split_once(':')
                .or_else(|| arg.split_once('='))
                .map(|(name, _)| name.trim())
        })
        .filter(|name| is_identifier(name))
        .map(str::to_owned)
        .collect();
    (callee.trim(), args)
}

fn command_route_slot(trimmed_line: &str) -> Option<ParsedCommandRouteSlot> {
    let rest = trimmed_line.strip_prefix("route ")?;
    let name = route_slot_name(rest)?.to_owned();
    Some(ParsedCommandRouteSlot {
        name,
        slot: CommandRouteSlot {
            bound_from_context: rest.contains(" from "),
        },
    })
}

#[derive(Debug)]
struct ParsedCommandRouteSlot {
    name: String,
    slot: CommandRouteSlot,
}

fn route_slot_name(route: &str) -> Option<&str> {
    route
        .split_once(':')
        .map(|(name, _)| name.trim())
        .or_else(|| route.split_whitespace().next())
        .filter(|name| is_identifier(name))
}

fn audience_can_reach_policy(audience: &str, qualifiers: &[String], atoms: &[String]) -> bool {
    if atoms.iter().any(|atom| atom == "@scope.public") {
        return true;
    }

    if audience == "public" {
        return false;
    }

    let allowed_roles = audience_roles(audience, qualifiers);
    if atoms.iter().any(|atom| allowed_roles.contains(atom)) {
        return true;
    }

    audience == "account"
        && atoms
            .iter()
            .any(|atom| atom == "@scope.same_org" || atom == "@scope.current_customer")
}

fn audience_roles(audience: &str, qualifiers: &[String]) -> BTreeSet<String> {
    let mut roles = BTreeSet::new();
    roles.insert(format!("@role.{audience}"));

    for qualifier in qualifiers {
        if qualifier == "role" || qualifier == "roles" {
            continue;
        }
        if let Some(role) = qualifier.strip_prefix("@role.") {
            roles.insert(format!("@role.{role}"));
        } else {
            roles.insert(format!("@role.{qualifier}"));
        }
    }

    roles
}

fn line_col_for_offset(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;

    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    (line, column)
}

fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

fn is_identifier(source: &str) -> bool {
    let mut chars = source.chars();
    matches!(chars.next(), Some(first) if first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package_from_sources(sources: Vec<(&str, &str)>) -> DoctorPackage {
        let mut files = Vec::new();
        let mut commands = BTreeMap::new();
        let mut experiences = BTreeMap::new();

        for (path, source) in sources {
            let mut file = DoctorFile {
                path: PathBuf::from(path),
                source: source.to_owned(),
                local_diagnostics: Vec::new(),
                lzx: None,
            };

            if path.ends_with(".lzi") {
                collect_canonical_facts(&file, &mut commands);
            } else {
                let document = lazuli_syntax::parse_lzx_document(&file.source).unwrap();
                collect_lzx_experience_facts(&document, &mut experiences);
                file.lzx = Some(document);
            }

            files.push(file);
        }

        DoctorPackage {
            files,
            commands,
            experiences,
        }
    }

    #[test]
    fn doctor_reports_public_surface_reaching_staff_command() {
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  policies
    create: @role.admin, @role.sales

  command create
    policy @policy.create
"#,
            ),
            (
                "customer.public.web.lzx",
                r#"
surface customer web
  uses experience customer

  audience public
    view lead_capture Form
      submit customer.command.create
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "LZX-POL-001"
                && diagnostic.message.contains("audience `public`")
                && diagnostic.message.contains("customer.command.create")
        }));
    }

    #[test]
    fn doctor_allows_public_surface_reaching_public_command() {
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  policies
    capture_lead: @scope.public

  command capture_lead
    policy @policy.capture_lead
"#,
            ),
            (
                "customer.public.web.lzx",
                r#"
surface customer web
  uses experience customer

  audience public
    view lead_capture Form
      submit customer.command.capture_lead
"#,
            ),
        ]);

        assert!(package.diagnostics().is_empty());
    }

    #[test]
    fn doctor_resolves_platform_action_through_abstract_experience() {
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  policies
    create: @role.admin

  command create
    policy @policy.create
"#,
            ),
            (
                "customer.lzx",
                r#"
experience customer
  imports customer

  view list
    action create -> customer.command.create
"#,
            ),
            (
                "customer.web.lzx",
                r#"
surface customer web
  uses experience customer

  audience admin
    view list Table
      actions create
"#,
            ),
        ]);

        assert!(package.diagnostics().is_empty());
    }

    #[test]
    fn doctor_reports_command_route_not_bound_by_surface_target() {
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  policies
    update: @role.admin

  command reassign
    route id: ID
    policy @policy.update
"#,
            ),
            (
                "customer.web.lzx",
                r#"
surface customer web
  uses experience customer

  audience admin
    view detail Form
      submit customer.command.reassign
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "LZX-ROUTE-001"
                && diagnostic
                    .message
                    .contains("required command route slot(s) id")
        }));
    }

    #[test]
    fn doctor_allows_command_route_bound_from_context() {
        let package = package_from_sources(vec![
            (
                "customer_auth.lzi",
                r#"
feature customer_auth
  policies
    update: @scope.same_org

  command enable_mfa
    route customer_id: ID from ctx.customer.id
    policy @policy.update
"#,
            ),
            (
                "customer_auth.web.lzx",
                r#"
surface customer_auth web
  uses experience customer_auth

  audience account
    view enable_mfa Form
      submit customer_auth.command.enable_mfa
"#,
            ),
        ]);

        assert!(package.diagnostics().is_empty());
    }
}
