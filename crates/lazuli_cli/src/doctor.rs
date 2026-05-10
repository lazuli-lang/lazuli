use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lazuli_ir::{AppManifest, AppRegistry};
use lazuli_lsp::SecurityProfile;
use lazuli_syntax::{LzxDocument, LzxPlatform, LzxPlatformView};
use tower_lsp::lsp_types::DiagnosticSeverity;

use crate::app_manifest::{parse_app_manifest, parse_app_registry};

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
    app: Option<DoctorAppManifest>,
    registry: Option<DoctorAppRegistry>,
    commands: BTreeMap<CommandKey, CommandPolicy>,
    experiences: BTreeMap<String, ExperienceFacts>,
    operational: OperationalFacts,
}

impl DoctorPackage {
    fn load(input: &Path, security_profile: SecurityProfile) -> Result<Self> {
        let paths = collect_package_paths(input)?;
        if paths.is_empty() {
            bail!("no .lzi or .lzx files found for {}", input.display());
        }

        let mut files = Vec::new();
        let mut app = None;
        let mut registry = None;
        let mut commands = BTreeMap::new();
        let mut experiences = BTreeMap::new();
        let mut operational = OperationalFacts::default();

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
                if let Some(manifest) = parse_app_manifest(&file.source) {
                    if app.is_none() {
                        app = Some(DoctorAppManifest {
                            path: file.path.clone(),
                            manifest,
                        });
                    } else {
                        file.local_diagnostics.push(DoctorDiagnostic {
                            path: file.path.clone(),
                            line: 1,
                            column: 1,
                            severity: DoctorSeverity::Error,
                            code: "APP-001".to_owned(),
                            message: "package should declare exactly one app manifest entrypoint."
                                .to_owned(),
                        });
                    }
                }
                if let Some(manifest) = parse_app_registry(&file.source) {
                    if registry.is_none() {
                        registry = Some(DoctorAppRegistry { manifest });
                    } else {
                        file.local_diagnostics.push(DoctorDiagnostic {
                            path: file.path.clone(),
                            line: 1,
                            column: 1,
                            severity: DoctorSeverity::Error,
                            code: "REG-001".to_owned(),
                            message: "package should declare at most one registry manifest."
                                .to_owned(),
                        });
                    }
                }
                collect_canonical_facts(&file, &mut commands, &mut operational);
            } else if is_lzx_path(&file.path) {
                match lazuli_syntax::parse_lzx_document(&file.source) {
                    Ok(document) => {
                        collect_lzx_experience_facts(&document, &mut experiences);
                        collect_lzx_operational_facts(&file, &document, &mut operational);
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
            app,
            registry,
            commands,
            experiences,
            operational,
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
        diagnostics.extend(app_contract_diagnostics(
            self.app.as_ref(),
            self.registry.as_ref(),
            &self.operational,
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
struct DoctorAppManifest {
    path: PathBuf,
    manifest: AppManifest,
}

#[derive(Debug)]
struct DoctorAppRegistry {
    manifest: AppRegistry,
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

#[derive(Debug, Clone)]
struct SourceFact {
    path: PathBuf,
    line: usize,
    column: usize,
    name: String,
}

#[derive(Debug, Default)]
struct OperationalFacts {
    features: BTreeMap<String, SourceFact>,
    integration_requirements: Vec<IntegrationRequirementFact>,
    env_references: Vec<SourceFact>,
    file_capabilities: Vec<SourceFact>,
    jobs: Vec<SourceFact>,
    schedules: Vec<SourceFact>,
    webhooks: Vec<SourceFact>,
    apis: Vec<SourceFact>,
    web_surfaces: Vec<SourceFact>,
    mobile_surfaces: Vec<SourceFact>,
    web_routes: Vec<SourceFact>,
    mobile_routes: Vec<SourceFact>,
}

#[derive(Debug, Clone)]
struct IntegrationRequirementFact {
    path: PathBuf,
    line: usize,
    column: usize,
    feature: String,
    slot: String,
    contract: String,
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

fn collect_canonical_facts(
    file: &DoctorFile,
    commands: &mut BTreeMap<CommandKey, CommandPolicy>,
    operational: &mut OperationalFacts,
) {
    let lines: Vec<_> = file.source.lines().collect();
    collect_operational_lzi_facts(file, &lines, operational);

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
        collect_feature_integration_requirements(
            file,
            &feature,
            start,
            &lines[start..index],
            operational,
        );
    }
}

fn collect_feature_integration_requirements(
    file: &DoctorFile,
    feature: &str,
    feature_start: usize,
    lines: &[&str],
    operational: &mut OperationalFacts,
) {
    let mut in_requires_block = false;

    for (offset, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading == 2 {
            in_requires_block = trimmed == "requires";
            if let Some(requirement) = trimmed.strip_prefix("requires ") {
                if let Some((slot, contract)) = parse_integration_requirement(requirement) {
                    operational
                        .integration_requirements
                        .push(IntegrationRequirementFact {
                            path: file.path.clone(),
                            line: feature_start + offset + 1,
                            column: leading + 1,
                            feature: feature.to_owned(),
                            slot: slot.to_owned(),
                            contract: contract.to_owned(),
                        });
                }
            }
            continue;
        }

        if leading <= 2 {
            in_requires_block = false;
        }

        if in_requires_block
            && leading == 4
            && let Some((slot, contract)) = parse_integration_requirement(trimmed)
        {
            operational
                .integration_requirements
                .push(IntegrationRequirementFact {
                    path: file.path.clone(),
                    line: feature_start + offset + 1,
                    column: leading + 1,
                    feature: feature.to_owned(),
                    slot: slot.to_owned(),
                    contract: contract.to_owned(),
                });
        }
    }
}

fn collect_operational_lzi_facts(
    file: &DoctorFile,
    lines: &[&str],
    operational: &mut OperationalFacts,
) {
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let line_number = index + 1;
        let column = leading_spaces(line) + 1;

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            if let Some(feature) = trimmed.split_whitespace().nth(1) {
                operational.features.insert(
                    feature.to_owned(),
                    SourceFact {
                        path: file.path.clone(),
                        line: line_number,
                        column,
                        name: feature.to_owned(),
                    },
                );
            }
        }

        for reference in path_references(trimmed, "env.") {
            operational.env_references.push(SourceFact {
                path: file.path.clone(),
                line: line_number,
                column,
                name: reference.to_owned(),
            });
        }

        if trimmed.contains("@cap.File") {
            operational.file_capabilities.push(SourceFact {
                path: file.path.clone(),
                line: line_number,
                column,
                name: "@cap.File".to_owned(),
            });
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("api ") {
            operational.apis.push(SourceFact {
                path: file.path.clone(),
                line: line_number,
                column,
                name: named_block_name(trimmed, "api")
                    .unwrap_or("unknown")
                    .to_owned(),
            });
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("webhook ") {
            operational.webhooks.push(SourceFact {
                path: file.path.clone(),
                line: line_number,
                column,
                name: named_block_name(trimmed, "webhook")
                    .unwrap_or("unknown")
                    .to_owned(),
            });
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("job ") {
            let job_name = named_block_name(trimmed, "job").unwrap_or("unknown");
            let fact = SourceFact {
                path: file.path.clone(),
                line: line_number,
                column,
                name: job_name.to_owned(),
            };
            if job_block_has_schedule(lines, index) {
                operational.schedules.push(fact.clone());
            }
            operational.jobs.push(fact);
        }
    }
}

fn named_block_name<'a>(trimmed: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    rest.split_whitespace().next()
}

fn job_block_has_schedule(lines: &[&str], start: usize) -> bool {
    let mut index = start + 1;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        if !trimmed.is_empty() && leading_spaces(line) <= 2 {
            break;
        }
        if leading_spaces(line) == 4 && trimmed.starts_with("trigger schedule ") {
            return true;
        }
        index += 1;
    }
    false
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

fn collect_lzx_operational_facts(
    file: &DoctorFile,
    document: &LzxDocument,
    operational: &mut OperationalFacts,
) {
    for route in &document.routes {
        let (line, column) = line_col_for_offset(&file.source, route.span.start);
        let fact = SourceFact {
            path: file.path.clone(),
            line,
            column,
            name: route.name.clone(),
        };
        match lzx_route_surface_platform(route.surface.as_deref()) {
            Some(LzxPlatform::Web) => operational.web_routes.push(fact),
            Some(LzxPlatform::Mobile) => operational.mobile_routes.push(fact),
            None => {
                if route.path.is_some() {
                    operational.web_routes.push(fact.clone());
                }
                if route.stack.is_some() {
                    operational.mobile_routes.push(fact);
                }
            }
        }
    }

    for surface in &document.surfaces {
        let (line, column) = line_col_for_offset(&file.source, surface.span.start);
        let fact = SourceFact {
            path: file.path.clone(),
            line,
            column,
            name: surface.experience.clone(),
        };
        match surface.platform {
            LzxPlatform::Web => operational.web_surfaces.push(fact),
            LzxPlatform::Mobile => operational.mobile_surfaces.push(fact),
        }
    }
}

fn lzx_route_surface_platform(surface: Option<&str>) -> Option<LzxPlatform> {
    match surface?.split_whitespace().last()? {
        "web" => Some(LzxPlatform::Web),
        "mobile" => Some(LzxPlatform::Mobile),
        _ => None,
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

fn app_contract_diagnostics(
    app: Option<&DoctorAppManifest>,
    registry: Option<&DoctorAppRegistry>,
    operational: &OperationalFacts,
) -> Vec<DoctorDiagnostic> {
    let Some(app) = app else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    let manifest = &app.manifest;
    let env_names = operational_env_names(manifest, registry);
    let used_features: BTreeSet<_> = manifest.uses.iter().map(String::as_str).collect();

    for feature in operational.features.values() {
        if !used_features.contains(feature.name.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "APP-USES-001".to_owned(),
                message: format!(
                    "app manifest does not list local feature `{}` in `uses`; generated app registration may omit it.",
                    feature.name
                ),
            });
        }
    }

    for used in &manifest.uses {
        if !operational.features.contains_key(used) {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "APP-USES-002".to_owned(),
                message: format!(
                    "app manifest lists `{used}` in `uses`, but no local feature with that name was found in this package."
                ),
            });
        }
    }

    if !manifest.services.is_empty() {
        diagnostics.extend(app_service_contract_diagnostics(app, operational));
    }

    diagnostics.extend(app_binding_contract_diagnostics(app, registry, operational));

    for env_ref in &operational.env_references {
        if !env_names.contains(env_ref.name.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: env_ref.path.clone(),
                line: env_ref.line,
                column: env_ref.column,
                severity: DoctorSeverity::Error,
                code: "APP-ENV-001".to_owned(),
                message: format!(
                    "environment reference `env.{}` is not declared in `app.lzi` or `registry.lzi` env.",
                    env_ref.name
                ),
            });
        }
    }

    if !operational.file_capabilities.is_empty()
        && !app_has_any_capability(manifest, registry, &["object_storage", "storage"])
    {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-CAP-001",
            "package uses `@cap.File`, but app/registry contract does not declare `object_storage` or `storage` capability.",
        ));
    }

    if !operational.jobs.is_empty() && !app_runtime_runs(manifest, "jobs") {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-RUNTIME-001",
            "package declares jobs, but app manifest runtime does not declare a unit that `runs jobs *`.",
        ));
    }

    if !operational.schedules.is_empty() && !app_runtime_runs(manifest, "schedules") {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-RUNTIME-002",
            "package declares scheduled jobs, but app manifest runtime does not declare a unit that `runs schedules *`.",
        ));
    }

    if !operational.webhooks.is_empty() && !app_runtime_serves(manifest, "webhooks") {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-RUNTIME-003",
            "package declares webhooks, but app manifest runtime does not declare a unit that `serves webhooks`.",
        ));
    }

    if !operational.apis.is_empty() && !app_runtime_serves(manifest, "apis") {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-RUNTIME-004",
            "package declares custom APIs, but app manifest runtime does not declare a unit that `serves apis`.",
        ));
    }

    if (!operational.web_routes.is_empty() || !operational.web_surfaces.is_empty())
        && !app_has_target(manifest, "web")
    {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-TARGET-001",
            "package declares web routes/surfaces, but app manifest targets do not include `web <runtime>`.",
        ));
    }

    if (!operational.mobile_routes.is_empty() || !operational.mobile_surfaces.is_empty())
        && !app_has_target(manifest, "mobile")
    {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-TARGET-002",
            "package declares mobile routes/surfaces, but app manifest targets do not include `mobile <runtime>`.",
        ));
    }

    if !operational.web_routes.is_empty() && !app_has_url(manifest, "web") {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-URL-001",
            "package declares web routes, but app manifest URLs do not include a `web` URL.",
        ));
    }

    if (!operational.webhooks.is_empty() || !operational.apis.is_empty())
        && !app_has_url(manifest, "api")
    {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-URL-002",
            "package declares webhooks or custom APIs, but app manifest URLs do not include an `api` URL.",
        ));
    }

    diagnostics
}

fn app_missing_contract_diagnostic(
    app: &DoctorAppManifest,
    code: &str,
    message: &str,
) -> DoctorDiagnostic {
    DoctorDiagnostic {
        path: app.path.clone(),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

fn app_binding_contract_diagnostics(
    app: &DoctorAppManifest,
    registry: Option<&DoctorAppRegistry>,
    operational: &OperationalFacts,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut requirement_index = BTreeMap::new();

    for requirement in &operational.integration_requirements {
        requirement_index.insert(
            (requirement.feature.as_str(), requirement.slot.as_str()),
            requirement.contract.as_str(),
        );

        let matching_binding = app.manifest.bindings.iter().find(|binding| {
            binding.target_feature == requirement.feature && binding.target_slot == requirement.slot
        });

        if matching_binding.is_none() {
            diagnostics.push(DoctorDiagnostic {
                path: requirement.path.clone(),
                line: requirement.line,
                column: requirement.column,
                severity: DoctorSeverity::Error,
                code: "APP-BIND-001".to_owned(),
                message: format!(
                    "feature `{}` requires integration slot `{}`: `{}`, but app manifest does not bind `{}.{}`.",
                    requirement.feature,
                    requirement.slot,
                    requirement.contract,
                    requirement.feature,
                    requirement.slot
                ),
            });
        }
    }

    let integrations = operational_integrations(&app.manifest, registry);

    for binding in &app.manifest.bindings {
        let target = (
            binding.target_feature.as_str(),
            binding.target_slot.as_str(),
        );
        let Some(expected_contract) = requirement_index.get(&target).copied() else {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "APP-BIND-005".to_owned(),
                message: format!(
                    "app binding `{}.{}` has no matching feature requirement.",
                    binding.target_feature, binding.target_slot
                ),
            });
            continue;
        };

        let Some(integration_name) = integration_source_name(&binding.source) else {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-BIND-002".to_owned(),
                message: format!(
                    "app binding `{}.{}` points to `{}`, but bindings must use `integrations.<name>` or `registry.integrations.<name>`.",
                    binding.target_feature, binding.target_slot, binding.source
                ),
            });
            continue;
        };

        let Some(actual_contract) = integrations.get(integration_name) else {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-BIND-003".to_owned(),
                message: format!(
                    "app binding `{}.{}` references integration `{integration_name}`, but no app/registry integration with that name exists.",
                    binding.target_feature, binding.target_slot
                ),
            });
            continue;
        };

        if *actual_contract != expected_contract {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-BIND-004".to_owned(),
                message: format!(
                    "app binding `{}.{}` expects `{expected_contract}`, but integration `{integration_name}` is `{actual_contract}`.",
                    binding.target_feature, binding.target_slot
                ),
            });
        }
    }

    diagnostics
}

fn operational_integrations<'a>(
    app: &'a AppManifest,
    registry: Option<&'a DoctorAppRegistry>,
) -> BTreeMap<&'a str, &'a str> {
    let mut integrations = BTreeMap::new();
    for integration in &app.integrations {
        integrations.insert(integration.name.as_str(), integration.kind.as_str());
    }
    if let Some(registry) = registry {
        for integration in &registry.manifest.integrations {
            integrations.insert(integration.name.as_str(), integration.kind.as_str());
        }
    }
    integrations
}

fn integration_source_name(source: &str) -> Option<&str> {
    source
        .strip_prefix("integrations.")
        .or_else(|| source.strip_prefix("registry.integrations."))
}

fn app_service_contract_diagnostics(
    app: &DoctorAppManifest,
    operational: &OperationalFacts,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut owners: BTreeMap<&str, Vec<&str>> = BTreeMap::new();

    for service in &app.manifest.services {
        for owned in &service.owns {
            owners
                .entry(owned.as_str())
                .or_default()
                .push(service.name.as_str());
        }

        for exposure in &service.exposes {
            if let Some(feature_name) = exposure.target.split('.').next()
                && !service.owns.iter().any(|owned| owned == feature_name)
            {
                diagnostics.push(DoctorDiagnostic {
                    path: app.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "APP-SVC-003".to_owned(),
                    message: format!(
                        "service `{}` exposes `{}` from feature `{feature_name}`, but does not own that feature.",
                        service.name, exposure.target
                    ),
                });
            }
        }
    }

    for feature in operational.features.values() {
        match owners.get(feature.name.as_str()) {
            Some(service_names) if service_names.len() == 1 => {}
            Some(service_names) => diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-SVC-001".to_owned(),
                message: format!(
                    "feature `{}` is owned by multiple app services: {}.",
                    feature.name,
                    service_names.join(", ")
                ),
            }),
            None => diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "APP-SVC-002".to_owned(),
                message: format!(
                    "feature `{}` is not assigned to any app service boundary.",
                    feature.name
                ),
            }),
        }
    }

    for owned in owners.keys() {
        if !operational.features.contains_key(*owned) {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "APP-SVC-004".to_owned(),
                message: format!(
                    "app service owns `{owned}`, but no local feature with that name was found in this package."
                ),
            });
        }
    }

    diagnostics
}

fn app_has_target(app: &AppManifest, target: &str) -> bool {
    app.targets
        .iter()
        .any(|entry| entry.split_whitespace().next() == Some(target))
}

fn app_has_url(app: &AppManifest, target: &str) -> bool {
    app.urls.iter().any(|url| url.target == target)
}

fn operational_env_names<'a>(
    app: &'a AppManifest,
    registry: Option<&'a DoctorAppRegistry>,
) -> BTreeSet<&'a str> {
    let mut names: BTreeSet<_> = app.env.iter().map(|env| env.name.as_str()).collect();
    if let Some(registry) = registry {
        names.extend(registry.manifest.env.iter().map(|env| env.name.as_str()));
    }
    names
}

fn app_has_any_capability(
    app: &AppManifest,
    registry: Option<&DoctorAppRegistry>,
    names: &[&str],
) -> bool {
    app.capabilities
        .iter()
        .any(|capability| names.contains(&capability.name.as_str()))
        || registry.is_some_and(|registry| {
            registry
                .manifest
                .capabilities
                .iter()
                .any(|capability| names.contains(&capability.name.as_str()))
        })
}

fn app_runtime_serves(app: &AppManifest, service: &str) -> bool {
    app.runtime
        .iter()
        .flat_map(|unit| unit.serves.iter())
        .any(|item| runtime_item_matches(item, service))
}

fn app_runtime_runs(app: &AppManifest, service: &str) -> bool {
    app.runtime
        .iter()
        .flat_map(|unit| unit.runs.iter())
        .any(|item| runtime_item_matches(item, service))
}

fn runtime_item_matches(item: &str, service: &str) -> bool {
    item == "*"
        || item == service
        || item
            .split_whitespace()
            .next()
            .is_some_and(|first| first == service)
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

fn parse_integration_requirement(trimmed: &str) -> Option<(&str, &str)> {
    let rest = trimmed.trim().strip_prefix("integration ")?;
    let (slot, contract) = rest.split_once(':')?;
    let slot = slot.trim();
    let contract = contract.trim();

    if is_identifier(slot) && is_type_name(contract) {
        Some((slot, contract))
    } else {
        None
    }
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

fn is_type_name(source: &str) -> bool {
    let mut chars = source.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_uppercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn path_references<'a>(source: &'a str, prefix: &str) -> Vec<&'a str> {
    let mut references = Vec::new();
    let mut rest = source;

    while let Some(start) = rest.find(prefix) {
        let after_prefix = &rest[start + prefix.len()..];
        let end = after_prefix
            .find(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
            .unwrap_or(after_prefix.len());
        if end > 0 {
            references.push(&after_prefix[..end]);
        }
        rest = &after_prefix[end..];
    }

    references
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package_from_sources(sources: Vec<(&str, &str)>) -> DoctorPackage {
        let mut files = Vec::new();
        let mut app = None;
        let mut registry = None;
        let mut commands = BTreeMap::new();
        let mut experiences = BTreeMap::new();
        let mut operational = OperationalFacts::default();

        for (path, source) in sources {
            let mut file = DoctorFile {
                path: PathBuf::from(path),
                source: source.to_owned(),
                local_diagnostics: Vec::new(),
                lzx: None,
            };

            if path.ends_with(".lzi") {
                if let Some(manifest) = parse_app_manifest(&file.source) {
                    app = Some(DoctorAppManifest {
                        path: file.path.clone(),
                        manifest,
                    });
                }
                if let Some(manifest) = parse_app_registry(&file.source) {
                    registry = Some(DoctorAppRegistry { manifest });
                }
                collect_canonical_facts(&file, &mut commands, &mut operational);
            } else {
                let document = lazuli_syntax::parse_lzx_document(&file.source).unwrap();
                collect_lzx_experience_facts(&document, &mut experiences);
                collect_lzx_operational_facts(&file, &document, &mut operational);
                file.lzx = Some(document);
            }

            files.push(file);
        }

        DoctorPackage {
            files,
            app,
            registry,
            commands,
            experiences,
            operational,
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

    #[test]
    fn doctor_reports_app_manifest_operational_gaps() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  uses
    customer
  targets
    backend go
  environments
    production
  runtime
    unit api
      serves queries, commands
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "customer.lzi",
                r#"
feature customer
  domain
    resource Customer
      csv: @cap.File(max_size:10mb,accept:text/csv) optional

  job import
    trigger schedule "0 2 * * *"

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_SECRET
"#,
            ),
            (
                "customer.web.lzx",
                r#"
route customer_list
  path "/customers"
  to customer.view.list
  surface customer web
  audience admin
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("APP-ENV-001"));
        assert!(codes.contains("APP-CAP-001"));
        assert!(codes.contains("APP-RUNTIME-001"));
        assert!(codes.contains("APP-RUNTIME-002"));
        assert!(codes.contains("APP-RUNTIME-003"));
        assert!(codes.contains("APP-TARGET-001"));
        assert!(codes.contains("APP-URL-001"));
        assert!(codes.contains("APP-URL-002"));
    }

    #[test]
    fn doctor_accepts_app_manifest_operational_contract() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  uses
    customer
  targets
    backend go
    web react
  environments
    production
  urls
    web production "https://app.acme.example"
    api production "https://api.acme.example"
  env
    group webhooks
      server INBOUND_SECRET: Secret required in production
  integrations
    crm: CRMProvider
      adapter @adapter.crm
      environments production
      credentials platform
        webhook_secret env.INBOUND_SECRET
  capabilities
    object_storage files
    integration crm
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
  runtime
    unit api
      serves queries, commands, webhooks, apis
      healthcheck "/healthz"
    unit worker
      runs jobs *
    unit scheduler
      runs schedules *
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "customer.lzi",
                r#"
feature customer
  domain
    resource Customer
      csv: @cap.File(max_size:10mb,accept:text/csv) optional

  api export
    method GET
    path "/api/export"
    output @cap.File(max_size:10mb,accept:text/csv)
    policy @scope.public
    handler "./api/export.go"

  job import
    trigger schedule "0 2 * * *"

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_SECRET
"#,
            ),
            (
                "customer.web.lzx",
                r#"
route customer_list
  path "/customers"
  to customer.view.list
  surface customer web
  audience admin
"#,
            ),
        ]);

        assert!(package.diagnostics().is_empty());
    }

    #[test]
    fn doctor_uses_registry_for_env_and_capabilities() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  uses
    customer
  targets
    backend go
  environments
    production
  urls
    api production "https://api.acme.example"
  runtime
    unit api
      serves webhooks
      healthcheck "/healthz"
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  env
    group webhooks
      server INBOUND_SECRET: Secret required in production
  capabilities
    object_storage files
"#,
            ),
            (
                "customer.lzi",
                r#"
feature customer
  domain
    resource Customer
      csv: @cap.File(max_size:10mb,accept:text/csv) optional

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_SECRET
    idempotency by payload.id
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();

        assert!(
            diagnostics.is_empty(),
            "expected registry to satisfy app contract, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn doctor_validates_feature_integration_bindings() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  uses
    payments
  bindings
    payments.gateway = integrations.mercadopago
  targets
    backend go
  environments
    production
  runtime
    unit api
      serves commands
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  integrations
    mercadopago: PaymentGateway
      adapter @adapter.mercadopago
"#,
            ),
            (
                "payments.lzi",
                r#"
feature payments
  requires integration gateway: PaymentGateway
"#,
            ),
        ]);

        assert!(package.diagnostics().is_empty());
    }

    #[test]
    fn doctor_reports_missing_and_mismatched_feature_integration_bindings() {
        let missing = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  uses
    payments
  targets
    backend go
  environments
    production
  runtime
    unit api
      serves commands
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "payments.lzi",
                r#"
feature payments
  requires integration gateway: PaymentGateway
"#,
            ),
        ]);

        assert!(
            missing
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "APP-BIND-001")
        );

        let mismatched = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  uses
    payments
  bindings
    payments.gateway = integrations.serasa
  targets
    backend go
  environments
    production
  runtime
    unit api
      serves commands
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  integrations
    serasa: CreditBureau
      adapter @adapter.serasa
"#,
            ),
            (
                "payments.lzi",
                r#"
feature payments
  requires integration gateway: PaymentGateway
"#,
            ),
        ]);

        assert!(
            mismatched
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "APP-BIND-004")
        );
    }

    #[test]
    fn doctor_reports_app_service_ownership_gaps() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  uses
    customer
    billing
  targets
    backend go
  services
    service crm
      owns customer
      exposes
        query billing.query.invoice_by_id

    service finance
      owns customer, billing
"#,
            ),
            (
                "customer.lzi",
                r#"
feature customer
"#,
            ),
            (
                "billing.lzi",
                r#"
feature billing
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "APP-SVC-001"
                && diagnostic
                    .message
                    .contains("feature `customer` is owned by multiple app services")
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "APP-SVC-003"
                && diagnostic
                    .message
                    .contains("service `crm` exposes `billing.query.invoice_by_id`")
        }));
    }
}
