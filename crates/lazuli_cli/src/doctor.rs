use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lazuli_analyzer::lower_feature_skeleton;
use lazuli_ir::{
    self as ir, Agent, AppContract, AppManifest, AppProfile, AppRegistry, AppWorkspace,
};
use lazuli_lsp::SecurityProfile;
use lazuli_syntax::{LzxDocument, LzxPlatform, LzxPlatformView, parse_feature_skeletons};
use tower_lsp::lsp_types::DiagnosticSeverity;

use crate::app_manifest::{
    RegistryParseOutput, RegistryToolDefectReason, parse_app_contracts, parse_app_manifest,
    parse_app_profiles, parse_app_registry_with_defects, parse_app_workspace,
};

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
    workspace: Option<DoctorAppWorkspace>,
    contracts: Vec<DoctorAppContract>,
    app: Option<DoctorAppManifest>,
    registry: Option<DoctorAppRegistry>,
    profiles: Vec<DoctorAppProfile>,
    commands: BTreeMap<CommandKey, CommandPolicy>,
    experiences: BTreeMap<String, ExperienceFacts>,
    operational: OperationalFacts,
    /// Cut A: agent IR per feature, loaded through
    /// `lazuli_syntax::parse_feature_skeletons` +
    /// `lazuli_analyzer::lower_feature_skeleton`.
    agents: Vec<AgentFacts>,
    /// Cut A: per-feature enum/record/query/command symbol tables used
    /// for discriminator + tool-policy cross-resolution.
    feature_symbols: BTreeMap<String, FeatureSymbols>,
    /// Cut A: registry `tool <name>` headers that lacked `effect`.
    registry_tool_defects: Vec<RegistryToolDefect>,
    /// Cut A.7: `api <name>` declarations harvested per feature; doctor
    /// cross-checks them against agent `expose_http` paths.
    api_paths: Vec<ApiPathFact>,
    /// Cut A.9: `command <name>` declarations carrying an `approval`
    /// block. Doctor validates the block + extends the write-tool
    /// guard so agents dispatching approval-gated commands satisfy
    /// `agent_tool_write_unguarded_diagnostics` without their own
    /// `safety` validator.
    command_approvals: Vec<CommandApprovalFact>,
    /// Phase L: lowered `auth` block per feature, paired with source
    /// line anchors for subblock-precise diagnostics.
    auth_facts: Vec<AuthFacts>,
    /// Phase L: per-feature resource declarations + field type text.
    /// Used to resolve `auth identity Customer.email` and
    /// `auth sessions resource CustomerSession` and to read
    /// `@cap.Hashed(algorithm:…)` axes off session resource fields.
    feature_resources: BTreeMap<String, BTreeMap<String, ResourceFact>>,
    /// Phase L: per-feature `extensions adapter <local>` declarations
    /// for the `auth_oauth_adapter_unbound` adapter resolution scope.
    feature_adapters: BTreeMap<String, BTreeSet<String>>,
    /// Phase L: per-feature `uses <other_feature>, ...` references so
    /// `auth identity Customer.email` in `feature customer_auth` can
    /// resolve `Customer` in `feature customer` when `uses customer` is
    /// declared.
    feature_uses: BTreeMap<String, BTreeSet<String>>,
}

impl DoctorPackage {
    fn load(input: &Path, security_profile: SecurityProfile) -> Result<Self> {
        let paths = collect_package_paths(input)?;
        if paths.is_empty() {
            bail!("no .lzi or .lzx files found for {}", input.display());
        }

        let mut files = Vec::new();
        let mut workspace = None;
        let mut contracts = Vec::new();
        let mut app = None;
        let mut registry = None;
        let mut profiles = Vec::new();
        let mut commands = BTreeMap::new();
        let mut experiences = BTreeMap::new();
        let mut operational = OperationalFacts::default();
        let mut agents: Vec<AgentFacts> = Vec::new();
        let mut feature_symbols: BTreeMap<String, FeatureSymbols> = BTreeMap::new();
        let mut registry_tool_defects: Vec<RegistryToolDefect> = Vec::new();
        let mut api_paths: Vec<ApiPathFact> = Vec::new();
        let mut command_approvals: Vec<CommandApprovalFact> = Vec::new();
        let mut auth_facts: Vec<AuthFacts> = Vec::new();
        let mut feature_resources: BTreeMap<String, BTreeMap<String, ResourceFact>> =
            BTreeMap::new();
        let mut feature_adapters: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut feature_uses: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

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
                contracts.extend(
                    parse_app_contracts(&file.source)
                        .into_iter()
                        .map(|manifest| DoctorAppContract {
                            path: file.path.clone(),
                            manifest,
                        }),
                );
                if let Some(manifest) = parse_app_workspace(&file.source) {
                    if workspace.is_none() {
                        workspace = Some(DoctorAppWorkspace {
                            path: file.path.clone(),
                            manifest,
                        });
                    } else {
                        file.local_diagnostics.push(DoctorDiagnostic {
                            path: file.path.clone(),
                            line: 1,
                            column: 1,
                            severity: DoctorSeverity::Error,
                            code: "WS-001".to_owned(),
                            message: "package should declare at most one workspace contract."
                                .to_owned(),
                        });
                    }
                }
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
                let RegistryParseOutput {
                    registry: parsed_registry,
                    tool_defects,
                } = parse_app_registry_with_defects(&file.source);
                if let Some(manifest) = parsed_registry {
                    if registry.is_none() {
                        registry = Some(DoctorAppRegistry {
                            path: file.path.clone(),
                            manifest,
                        });
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
                registry_tool_defects.extend(tool_defects.into_iter().map(|defect| {
                    RegistryToolDefect {
                        path: file.path.clone(),
                        line: defect.line,
                        name: defect.name,
                        reason: defect.reason,
                    }
                }));

                // Cut A — agent IR collection + feature symbol scan.
                match parse_feature_skeletons(&file.source) {
                    Ok(features) => {
                        for skeleton in &features {
                            match lower_feature_skeleton(skeleton) {
                                Ok(feature) => {
                                    let header_line =
                                        line_col_for_offset(&file.source, skeleton.span.start).0;
                                    for agent in feature.agents {
                                        let agent_line = agent
                                            .span_ref
                                            .as_ref()
                                            .map(|s| line_col_for_offset(&file.source, s.start).0)
                                            .unwrap_or(header_line);
                                        agents.push(AgentFacts {
                                            feature: feature.name.clone(),
                                            agent,
                                            path: file.path.clone(),
                                            line: agent_line,
                                        });
                                    }
                                    if let Some(auth) = feature.auth {
                                        let auth_line = auth
                                            .span_ref
                                            .as_ref()
                                            .map(|s| line_col_for_offset(&file.source, s.start).0)
                                            .unwrap_or(header_line);
                                        let anchors = collect_auth_anchors(&file.source, auth_line);
                                        auth_facts.push(AuthFacts {
                                            feature: feature.name.clone(),
                                            auth,
                                            path: file.path.clone(),
                                            line: auth_line,
                                            identity_line: anchors.identity_line,
                                            password_line: anchors.password_line,
                                            password_algorithm_line: anchors
                                                .password_algorithm_line,
                                            sessions_line: anchors.sessions_line,
                                            sessions_resource_line: anchors.sessions_resource_line,
                                            mfa_line: anchors.mfa_line,
                                            oauth_lines: anchors.oauth_lines,
                                        });
                                    }
                                }
                                Err(error) => {
                                    file.local_diagnostics.push(DoctorDiagnostic {
                                        path: file.path.clone(),
                                        line: line_col_for_offset(
                                            &file.source,
                                            skeleton.span.start,
                                        )
                                        .0,
                                        column: 1,
                                        severity: DoctorSeverity::Error,
                                        code: "agent_lower_failed_diagnostics".to_owned(),
                                        message: format!("agent lowering failed: {error}"),
                                    });
                                }
                            }
                        }
                    }
                    Err(error) => {
                        file.local_diagnostics.push(DoctorDiagnostic {
                            path: file.path.clone(),
                            line: line_col_for_offset(&file.source, error.span().start).0,
                            column: line_col_for_offset(&file.source, error.span().start).1,
                            severity: DoctorSeverity::Error,
                            code: "agent_parse_failed_diagnostics".to_owned(),
                            message: error.to_string(),
                        });
                    }
                }
                collect_feature_symbols(&file, &mut feature_symbols);
                collect_api_paths(&file, &mut api_paths);
                collect_command_approvals(&file, &mut command_approvals);
                collect_feature_resources(&file, &mut feature_resources);
                collect_feature_adapters(&file, &mut feature_adapters);
                collect_feature_uses(&file, &mut feature_uses);
                profiles.extend(parse_app_profiles(&file.source).into_iter().map(|profile| {
                    DoctorAppProfile {
                        path: file.path.clone(),
                        profile,
                    }
                }));
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
            workspace,
            contracts,
            app,
            registry,
            profiles,
            commands,
            experiences,
            operational,
            agents,
            feature_symbols,
            registry_tool_defects,
            api_paths,
            command_approvals,
            auth_facts,
            feature_resources,
            feature_adapters,
            feature_uses,
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
            &self.profiles,
            &self.operational,
        ));
        diagnostics.extend(workspace_contract_diagnostics(self.workspace.as_ref()));
        diagnostics.extend(external_contract_diagnostics(
            &self.contracts,
            self.workspace.as_ref(),
        ));

        // Cut A — agent + tool + eval + discriminator cross-feature checks.
        diagnostics.extend(registry_tool_effect_diagnostics(
            &self.registry_tool_defects,
        ));
        diagnostics.extend(agent_tool_diagnostics(
            &self.agents,
            &self.feature_symbols,
            self.registry.as_ref(),
            &self.command_approvals,
        ));
        diagnostics.extend(agent_discriminator_diagnostics(
            &self.agents,
            &self.feature_symbols,
        ));
        diagnostics.extend(agent_eval_diagnostics(&self.agents));

        // Cut A.7 — `expose http` cross-feature checks.
        let known_audiences = collect_known_audiences(&self.files);
        diagnostics.extend(agent_expose_diagnostics(
            &self.agents,
            &self.api_paths,
            &known_audiences,
        ));

        // Cut A.8 — built-in trace event reservation + subscriber
        // payload drift checks.
        diagnostics.extend(agent_run_trace_diagnostics(&self.files));

        // Cut A.9 — `approval` primitive contract + role resolution.
        let known_roles = collect_known_roles(&self.files);
        diagnostics.extend(approval_diagnostics(&self.command_approvals, &known_roles));

        // Cut A.11 — `cors` block cross-checks against the app's
        // declared environments + urls.
        diagnostics.extend(cors_diagnostics(self.app.as_ref()));

        // Observability bucket cycle row 36 — `app.logging` and
        // `app.tracing` closed-catalog + range + exporter binding
        // checks.
        diagnostics.extend(app_logging_tracing_diagnostics(
            self.app.as_ref(),
            self.registry.as_ref().map(|reg| &reg.manifest),
        ));

        // Phase L — auth block cross-feature diagnostics.
        diagnostics.extend(auth_diagnostics(
            &self.auth_facts,
            &self.feature_resources,
            &self.feature_adapters,
            &self.feature_uses,
            self.registry.as_ref(),
        ));

        // Row 30 — Storage bucket cycle: 5 typed `@cap.File`
        // diagnostics. See `docs/proposals/bucket-storage-cycle.md`
        // §Doctor/LSP.
        diagnostics.extend(cap_file_storage_diagnostics(&self.operational));

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
struct DoctorAppWorkspace {
    path: PathBuf,
    manifest: AppWorkspace,
}

#[derive(Debug)]
struct DoctorAppContract {
    path: PathBuf,
    manifest: AppContract,
}

#[derive(Debug)]
struct DoctorAppManifest {
    path: PathBuf,
    manifest: AppManifest,
}

#[derive(Debug)]
struct DoctorAppRegistry {
    path: PathBuf,
    manifest: AppRegistry,
}

#[derive(Debug)]
struct DoctorAppProfile {
    path: PathBuf,
    profile: AppProfile,
}

#[derive(Debug)]
struct DoctorFile {
    path: PathBuf,
    source: String,
    local_diagnostics: Vec<DoctorDiagnostic>,
    lzx: Option<LzxDocument>,
}

// Cut A — typed agent + symbol facts gathered for cross-feature checks.

#[derive(Debug, Clone)]
struct AgentFacts {
    feature: String,
    agent: Agent,
    path: PathBuf,
    /// 1-based source line where the `agent <name>` header lives.
    line: usize,
}

/// Phase L — typed `auth` block facts harvested per feature for the four
/// cross-feature diagnostics:
///   - `auth_password_algorithm_hash_mismatch`
///   - `auth_sessions_resource_unknown`
///   - `auth_identity_field_unknown`
///   - `auth_oauth_adapter_unbound`
///
/// The IR carries the lowered shape; the auxiliary `*_line` fields map
/// each subblock back to the source so diagnostics point at the exact
/// authored token rather than the `auth` header.
#[derive(Debug, Clone)]
struct AuthFacts {
    feature: String,
    auth: ir::Auth,
    path: PathBuf,
    /// 1-based line for the `auth` header.
    line: usize,
    identity_line: usize,
    password_line: Option<usize>,
    password_algorithm_line: Option<usize>,
    sessions_line: Option<usize>,
    sessions_resource_line: Option<usize>,
    mfa_line: Option<usize>,
    /// Per-provider `oauth <provider>` header line.
    oauth_lines: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Default)]
struct FeatureSymbols {
    enums: BTreeMap<String, SymbolFact>,
    records: BTreeMap<String, RecordFact>,
    /// Maps short query name (e.g. `by_id`, `list`) to its registered
    /// policy reference text and kind. Used for tool-policy compatibility
    /// checks.
    queries: BTreeMap<String, QuerySymbolFact>,
    /// Maps short command name (e.g. `archive`) to its registered policy
    /// + safety hint. Commands are inherently write-effect for Cut A.
    commands: BTreeMap<String, CommandSymbolFact>,
}

#[derive(Debug, Clone)]
struct SymbolFact {
    path: PathBuf,
    line: usize,
}

/// Phase L — typed shape of a `resource <Name>` declaration for the
/// `auth_*` cross-checks. Fields carry their verbatim type text so the
/// `@cap.Hashed(algorithm:…)` axis is readable without re-parsing.
#[derive(Debug, Clone, Default)]
struct ResourceFact {
    path: PathBuf,
    line: usize,
    fields: BTreeMap<String, ResourceFieldFact>,
}

#[derive(Debug, Clone)]
struct ResourceFieldFact {
    /// Verbatim type text, e.g. `@cap.Hashed(algorithm:argon2id)`,
    /// `@semantic.Email`, `Text`, `DateTime`.
    type_text: String,
    /// `optional`/`required`/etc. modifiers (verbatim trailing tokens
    /// after the type). Used by `auth_identity_field_unknown` to detect
    /// non-identity-shaped fields.
    modifiers: String,
    /// 1-based line where the field is declared. Currently unused by
    /// diagnostics; reserved for future field-anchored messages.
    #[allow(dead_code)]
    line: usize,
}

#[derive(Debug, Clone, Default)]
struct RecordFact {
    base: SymbolFact,
    /// Field name -> (field type text, whether the field has a
    /// `discriminator` marker).
    fields: BTreeMap<String, RecordFieldFact>,
}

#[derive(Debug, Clone)]
struct RecordFieldFact {
    type_text: String,
    is_discriminator: bool,
}

#[derive(Debug, Clone)]
struct QuerySymbolFact {
    base: SymbolFact,
    policy: Option<String>,
    kind: ir::ToolKind,
}

#[derive(Debug, Clone)]
struct CommandSymbolFact {
    base: SymbolFact,
    policy: Option<String>,
}

#[derive(Debug, Clone)]
struct RegistryToolDefect {
    path: PathBuf,
    line: usize,
    name: String,
    reason: RegistryToolDefectReason,
}

impl Default for SymbolFact {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            line: 0,
        }
    }
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
    external_calls: Vec<ExternalCallFact>,
    env_references: Vec<SourceFact>,
    file_capabilities: Vec<SourceFact>,
    /// Row 30 — typed `@cap.File(...)` sites carrying the lowered
    /// `FileCapability` + origin + binding context (`ResourceField` or
    /// `ApiOutput`). Populated alongside `file_capabilities` so the
    /// storage diagnostics can run against typed IR shape, while the
    /// existing text-pattern fact powers the `APP-CAP-001` check.
    file_capability_facts: Vec<FileCapabilityFact>,
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

#[derive(Debug, Clone)]
struct ExternalCallFact {
    path: PathBuf,
    line: usize,
    column: usize,
    feature: String,
    subject_kind: String,
    subject: String,
    slot: String,
    operation: String,
    has_timeout: bool,
    has_retry: bool,
    has_idempotency: bool,
}

/// Row 30 — one typed `@cap.File(...)` site harvested from a `.lzi`
/// source file. `feature` is the enclosing `feature <name>` block;
/// `binding` discriminates between a resource field and an api output
/// so the storage diagnostics can apply context-sensitive rules (e.g.
/// `visibility` is required on api outputs but defaults to `private`
/// on resource fields).
#[derive(Debug, Clone)]
struct FileCapabilityFact {
    path: PathBuf,
    line: usize,
    column: usize,
    feature: String,
    binding: FileCapabilityBinding,
    capability: lazuli_ir::FileCapability,
}

#[derive(Debug, Clone)]
enum FileCapabilityBinding {
    /// `<field>: @cap.File(...)` inside a `resource <Name>` block.
    ResourceField { resource: String, field: String },
    /// `output @cap.File(...)` inside an `api <Name>` block.
    ApiOutput { api: String },
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
    collect_file_capability_facts(file, &lines, operational);

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
        collect_feature_external_calls(file, &feature, start, &lines[start..index], operational);
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

fn collect_feature_external_calls(
    file: &DoctorFile,
    feature: &str,
    feature_start: usize,
    lines: &[&str],
    operational: &mut OperationalFacts,
) {
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        let leading = leading_spaces(lines[index]);

        if leading == 2 && (trimmed.starts_with("command ") || trimmed.starts_with("job ")) {
            let (subject_kind, subject_name) =
                if let Some(name) = named_block_name(trimmed, "command") {
                    ("command", name)
                } else if let Some(name) = named_block_name(trimmed, "job") {
                    ("job", name)
                } else {
                    index += 1;
                    continue;
                };
            let block_start = index;
            index += 1;

            while index < lines.len() && leading_spaces(lines[index]) > 2 {
                index += 1;
            }

            collect_external_calls_in_block(
                file,
                feature,
                feature_start,
                subject_kind,
                subject_name,
                block_start,
                &lines[block_start..index],
                operational,
            );
        } else {
            index += 1;
        }
    }
}

fn collect_external_calls_in_block(
    file: &DoctorFile,
    feature: &str,
    feature_start: usize,
    subject_kind: &str,
    subject_name: &str,
    block_start: usize,
    lines: &[&str],
    operational: &mut OperationalFacts,
) {
    let has_timeout = block_has_prefixed_line(lines, "timeout ");
    let has_retry = block_has_prefixed_line(lines, "retry ");
    let has_idempotency = block_has_prefixed_line(lines, "idempotency by ");
    let subject = format!("{feature}.{subject_kind}.{subject_name}");

    for (offset, line) in lines.iter().enumerate().skip(1) {
        let trimmed = line.trim_start();
        if leading_spaces(line) != 4 {
            continue;
        }

        let Some((slot, operation)) = parse_external_call_header(trimmed) else {
            continue;
        };

        operational.external_calls.push(ExternalCallFact {
            path: file.path.clone(),
            line: feature_start + block_start + offset + 1,
            column: leading_spaces(line) + 1,
            feature: feature.to_owned(),
            subject_kind: subject_kind.to_owned(),
            subject: subject.clone(),
            slot: slot.to_owned(),
            operation: operation.to_owned(),
            has_timeout,
            has_retry,
            has_idempotency,
        });
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
                    operational.web_routes.push(fact);
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
    profiles: &[DoctorAppProfile],
    operational: &OperationalFacts,
) -> Vec<DoctorDiagnostic> {
    let Some(app) = app else {
        if !profiles.is_empty() {
            return profiles
                .iter()
                .map(|profile| DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "PROFILE-APP-001".to_owned(),
                    message: format!(
                        "profile `{}` is declared, but no package app manifest was found.",
                        profile.profile.name
                    ),
                })
                .collect();
        }
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    let manifest = &app.manifest;
    let env_names = operational_env_names(manifest, registry);
    let used_features: BTreeSet<_> = manifest.uses.iter().map(String::as_str).collect();
    let pack_features = enabled_pack_provided_features(manifest, registry);

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
        if !operational.features.contains_key(used) && !pack_features.contains(used.as_str()) {
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
        diagnostics.extend(app_service_contract_diagnostics(
            app,
            operational,
            &pack_features,
        ));
    }

    diagnostics.extend(adapter_provenance_diagnostics(app, registry, profiles));
    diagnostics.extend(app_pack_contract_diagnostics(app, registry));
    diagnostics.extend(app_binding_contract_diagnostics(app, registry, operational));
    diagnostics.extend(external_call_contract_diagnostics(operational));
    diagnostics.extend(app_route_redirect_diagnostics(app, operational));
    diagnostics.extend(profile_contract_diagnostics(
        app,
        registry,
        profiles,
        operational,
    ));

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

    if !operational.web_routes.is_empty() && !app_has_url(manifest, profiles, "web") {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-URL-001",
            "package declares web routes, but app manifest URLs do not include a `web` URL.",
        ));
    }

    if (!operational.webhooks.is_empty() || !operational.apis.is_empty())
        && !app_has_url(manifest, profiles, "api")
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

fn app_route_redirect_diagnostics(
    app: &DoctorAppManifest,
    operational: &OperationalFacts,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let manifest = &app.manifest;

    let mut declared = BTreeSet::new();
    for fact in operational
        .web_routes
        .iter()
        .chain(operational.mobile_routes.iter())
    {
        declared.insert(fact.name.as_str());
    }

    for (field, value, code) in [
        (
            "auth_failed_redirect",
            manifest.auth_failed_redirect.as_deref(),
            "APP-ROUTE-001",
        ),
        ("not_found", manifest.not_found.as_deref(), "APP-ROUTE-002"),
    ] {
        let Some(target) = value else {
            continue;
        };
        let target = target.trim();
        if target.is_empty() {
            continue;
        }
        if !declared.contains(target) {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: code.to_owned(),
                message: format!(
                    "app `{field}` references route `{target}`, but no top-level `.lzx route {target}` was declared in this package."
                ),
            });
        }
    }

    diagnostics
}

fn workspace_contract_diagnostics(workspace: Option<&DoctorAppWorkspace>) -> Vec<DoctorDiagnostic> {
    let Some(workspace) = workspace else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    let mut app_names = BTreeSet::new();
    let mut published = Vec::new();

    for app in &workspace.manifest.apps {
        if !app_names.insert(app.name.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: workspace.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "WS-APP-001".to_owned(),
                message: format!(
                    "workspace declares app `{}` more than once; app ids must be unique.",
                    app.name
                ),
            });
        }

        if app.kind == "local"
            && app
                .path
                .as_deref()
                .is_some_and(|path| !path.ends_with(".lzi"))
        {
            diagnostics.push(DoctorDiagnostic {
                path: workspace.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "WS-APP-002".to_owned(),
                message: format!(
                    "workspace local app `{}` should point at an `app.lzi` entrypoint.",
                    app.name
                ),
            });
        }
    }

    for boundary in &workspace.manifest.boundaries {
        if !app_names.contains(boundary.app.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: workspace.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "WS-BOUNDARY-001".to_owned(),
                message: format!(
                    "workspace boundary references unknown app `{}`.",
                    boundary.app
                ),
            });
        }

        if boundary.direction == "publishes" {
            published.push(boundary.pattern.as_str());
        }
    }

    for boundary in &workspace.manifest.boundaries {
        if boundary.direction != "consumes" {
            continue;
        }
        if !published
            .iter()
            .any(|published| event_pattern_covers(published, &boundary.pattern))
        {
            diagnostics.push(DoctorDiagnostic {
                path: workspace.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "WS-EVENT-001".to_owned(),
                message: format!(
                    "workspace app `{}` consumes `{}`, but no workspace app publishes a compatible event pattern.",
                    boundary.app, boundary.pattern
                ),
            });
        }
    }

    for gateway in &workspace.manifest.gateways {
        for route in &gateway.routes {
            if route.target_kind != "app" {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "WS-GW-001".to_owned(),
                    message: format!(
                        "workspace gateway `{}` route `{}` targets `{}`; only `to app <name>` is supported in the language contract.",
                        gateway.name, route.path, route.target_kind
                    ),
                });
            } else if !app_names.contains(route.target.as_str()) {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "WS-GW-002".to_owned(),
                    message: format!(
                        "workspace gateway `{}` route `{}` targets unknown app `{}`.",
                        gateway.name, route.path, route.target
                    ),
                });
            }

            if route.auth.as_deref() != Some("propagate") {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "WS-GW-003".to_owned(),
                    message: format!(
                        "workspace gateway `{}` route `{}` should declare `auth propagate` so the runtime does not infer auth context.",
                        gateway.name, route.path
                    ),
                });
            }

            if route.tenant.as_deref() != Some("propagate") {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "WS-GW-004".to_owned(),
                    message: format!(
                        "workspace gateway `{}` route `{}` should declare `tenant propagate` so tenant context crosses app boundaries explicitly.",
                        gateway.name, route.path
                    ),
                });
            }
        }
    }

    if !workspace.manifest.gateways.is_empty() {
        let propagated: BTreeSet<_> = workspace
            .manifest
            .communication
            .as_ref()
            .map(|communication| {
                communication
                    .propagate
                    .iter()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        for required in ["tenant", "trace_id", "request_id"] {
            if !propagated.contains(required) {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "WS-COMM-001".to_owned(),
                    message: format!(
                        "workspace gateways should propagate `{required}` in the `communication` block."
                    ),
                });
            }
        }
    }

    diagnostics
}

fn event_pattern_covers(published: &str, consumed: &str) -> bool {
    if published == consumed {
        return true;
    }

    published
        .strip_suffix('*')
        .is_some_and(|prefix| consumed.starts_with(prefix))
}

fn external_contract_diagnostics(
    contracts: &[DoctorAppContract],
    workspace: Option<&DoctorAppWorkspace>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut contract_names = BTreeMap::new();

    for contract in contracts {
        if let Some(previous) = contract_names.insert(contract.manifest.name.as_str(), contract) {
            diagnostics.push(DoctorDiagnostic {
                path: contract.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "CONTRACT-001".to_owned(),
                message: format!(
                    "contract `{}` is declared more than once; first seen in {}.",
                    contract.manifest.name,
                    previous.path.display()
                ),
            });
        }

        if contract.manifest.imports.is_empty()
            && contract.manifest.operations.is_empty()
            && contract.manifest.events.is_empty()
        {
            diagnostics.push(DoctorDiagnostic {
                path: contract.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "CONTRACT-002".to_owned(),
                message: format!(
                    "contract `{}` declares no imports, operations, or events.",
                    contract.manifest.name
                ),
            });
        }

        for operation in &contract.manifest.operations {
            if operation.transport.is_none() {
                diagnostics.push(DoctorDiagnostic {
                    path: contract.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "CONTRACT-OP-001".to_owned(),
                    message: format!(
                        "contract `{}` operation `{}` should declare `transport http|rpc|event`.",
                        contract.manifest.name, operation.name
                    ),
                });
            }

            if operation.transport.as_deref() == Some("http")
                && (operation.method.is_none() || operation.path.is_none())
            {
                diagnostics.push(DoctorDiagnostic {
                    path: contract.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "CONTRACT-OP-002".to_owned(),
                    message: format!(
                        "contract `{}` HTTP operation `{}` should declare both `method` and `path`.",
                        contract.manifest.name, operation.name
                    ),
                });
            }

            if operation.input.is_none() || operation.output.is_none() {
                diagnostics.push(DoctorDiagnostic {
                    path: contract.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "CONTRACT-OP-003".to_owned(),
                    message: format!(
                        "contract `{}` operation `{}` should declare input and output records.",
                        contract.manifest.name, operation.name
                    ),
                });
            }

            if operation.timeout.is_none() {
                diagnostics.push(DoctorDiagnostic {
                    path: contract.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "CONTRACT-OP-004".to_owned(),
                    message: format!(
                        "contract `{}` operation `{}` should declare timeout so Go transport bindings do not infer it.",
                        contract.manifest.name, operation.name
                    ),
                });
            }
        }

        for event in &contract.manifest.events {
            if event.topic.is_none() {
                diagnostics.push(DoctorDiagnostic {
                    path: contract.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "CONTRACT-EVENT-001".to_owned(),
                    message: format!(
                        "contract `{}` event `{}` should declare a topic.",
                        contract.manifest.name, event.name
                    ),
                });
            }
        }
    }

    if let Some(workspace) = workspace
        && !contracts.is_empty()
    {
        for app in &workspace.manifest.apps {
            let Some(contract_name) = app.contract.as_deref() else {
                continue;
            };
            if !contract_names.contains_key(contract_name) {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "WS-CONTRACT-001".to_owned(),
                    message: format!(
                        "workspace app `{}` references external contract `{contract_name}`, but no local `contract {contract_name}` block was found in this package.",
                        app.name
                    ),
                });
            }
        }
    }

    diagnostics
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

    for (feature, slot, contract) in enabled_pack_integration_requirements(&app.manifest, registry)
    {
        requirement_index.insert((feature, slot), contract);

        let matching_binding = app
            .manifest
            .bindings
            .iter()
            .find(|binding| binding.target_feature == feature && binding.target_slot == slot);

        if matching_binding.is_none() {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-BIND-001".to_owned(),
                message: format!(
                    "enabled pack `{feature}` requires integration slot `{slot}`: `{contract}`, but app manifest does not bind `{feature}.{slot}`.",
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

fn external_call_contract_diagnostics(operational: &OperationalFacts) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let declared_slots: BTreeSet<_> = operational
        .integration_requirements
        .iter()
        .map(|requirement| (requirement.feature.as_str(), requirement.slot.as_str()))
        .collect();

    for call in &operational.external_calls {
        if !declared_slots.contains(&(call.feature.as_str(), call.slot.as_str())) {
            diagnostics.push(DoctorDiagnostic {
                path: call.path.clone(),
                line: call.line,
                column: call.column,
                severity: DoctorSeverity::Error,
                code: "INT-CALL-001".to_owned(),
                message: format!(
                    "`{}` calls `{}.{}`, but feature `{}` does not declare `requires integration {}: <Contract>`.",
                    call.subject, call.slot, call.operation, call.feature, call.slot
                ),
            });
        }

        if !call.has_timeout {
            diagnostics.push(DoctorDiagnostic {
                path: call.path.clone(),
                line: call.line,
                column: call.column,
                severity: DoctorSeverity::Error,
                code: "INT-CALL-002".to_owned(),
                message: format!(
                    "`{}` calls external operation `{}.{}` without an explicit `timeout \"...\"` on the {} block.",
                    call.subject, call.slot, call.operation, call.subject_kind
                ),
            });
        }

        if !call.has_retry {
            diagnostics.push(DoctorDiagnostic {
                path: call.path.clone(),
                line: call.line,
                column: call.column,
                severity: DoctorSeverity::Warning,
                code: "INT-CALL-003".to_owned(),
                message: format!(
                    "`{}` calls external operation `{}.{}` without a visible `retry <count> backoff <strategy>` policy.",
                    call.subject, call.slot, call.operation
                ),
            });
        }

        if call.subject_kind == "job" && !call.has_idempotency {
            diagnostics.push(DoctorDiagnostic {
                path: call.path.clone(),
                line: call.line,
                column: call.column,
                severity: DoctorSeverity::Warning,
                code: "INT-CALL-004".to_owned(),
                message: format!(
                    "`{}` calls external operation `{}.{}` without a visible job `idempotency by ...` key.",
                    call.subject, call.slot, call.operation
                ),
            });
        }
    }

    diagnostics
}

fn profile_contract_diagnostics(
    app: &DoctorAppManifest,
    registry: Option<&DoctorAppRegistry>,
    profiles: &[DoctorAppProfile],
    operational: &OperationalFacts,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let app_environments: BTreeSet<_> = app
        .manifest
        .environments
        .iter()
        .map(String::as_str)
        .collect();
    let integrations = operational_integrations(&app.manifest, registry);
    let mut requirement_index = BTreeMap::new();
    for requirement in &operational.integration_requirements {
        requirement_index.insert(
            (requirement.feature.as_str(), requirement.slot.as_str()),
            requirement.contract.as_str(),
        );
    }
    for (feature, slot, contract) in enabled_pack_integration_requirements(&app.manifest, registry)
    {
        requirement_index.insert((feature, slot), contract);
    }

    for profile in profiles {
        if !app_environments.contains(profile.profile.name.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: profile.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "PROFILE-001".to_owned(),
                message: format!(
                    "profile `{}` is not declared in app `environments`.",
                    profile.profile.name
                ),
            });
        }

        for url in &profile.profile.urls {
            if !profile_url_target_valid(&app.manifest, &url.target) {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "PROFILE-URL-001".to_owned(),
                    message: format!(
                        "profile `{}` declares URL target `{}`, but app targets do not expose that target.",
                        profile.profile.name, url.target
                    ),
                });
            }
        }

        for integration in &profile.profile.integrations {
            let Some(kind) = integrations.get(integration.name.as_str()).copied() else {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "PROFILE-INT-001".to_owned(),
                    message: format!(
                        "profile `{}` overrides integration `{}`, but no app/registry integration with that name exists.",
                        profile.profile.name, integration.name
                    ),
                });
                continue;
            };

            if let Some(environment) = &integration.environment
                && !integration_environment_allowed(
                    &app.manifest,
                    registry,
                    &integration.name,
                    environment,
                )
            {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "PROFILE-INT-002".to_owned(),
                    message: format!(
                        "profile `{}` selects `{}` environment `{environment}`, but `{}` does not list that environment.",
                        profile.profile.name, kind, integration.name
                    ),
                });
            }
        }

        for binding in &profile.profile.bindings {
            let target = (
                binding.target_feature.as_str(),
                binding.target_slot.as_str(),
            );
            let Some(expected_contract) = requirement_index.get(&target).copied() else {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "PROFILE-BIND-001".to_owned(),
                    message: format!(
                        "profile `{}` overrides binding `{}.{}`, but that feature slot has no requirement.",
                        profile.profile.name, binding.target_feature, binding.target_slot
                    ),
                });
                continue;
            };

            let Some(integration_name) = integration_source_name(&binding.source) else {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "PROFILE-BIND-002".to_owned(),
                    message: format!(
                        "profile `{}` binding `{}.{}` points to `{}`, but profile bindings must use `integrations.<name>` or `registry.integrations.<name>`.",
                        profile.profile.name,
                        binding.target_feature,
                        binding.target_slot,
                        binding.source
                    ),
                });
                continue;
            };

            let Some(actual_contract) = integrations.get(integration_name).copied() else {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "PROFILE-BIND-003".to_owned(),
                    message: format!(
                        "profile `{}` binding `{}.{}` references integration `{integration_name}`, but no app/registry integration with that name exists.",
                        profile.profile.name, binding.target_feature, binding.target_slot
                    ),
                });
                continue;
            };

            if actual_contract != expected_contract {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "PROFILE-BIND-004".to_owned(),
                    message: format!(
                        "profile `{}` binding `{}.{}` expects `{expected_contract}`, but integration `{integration_name}` is `{actual_contract}`.",
                        profile.profile.name, binding.target_feature, binding.target_slot
                    ),
                });
            }
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

fn app_pack_contract_diagnostics(
    app: &DoctorAppManifest,
    registry: Option<&DoctorAppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let integrations = operational_integrations(&app.manifest, registry);

    for pack_use in &app.manifest.packs {
        let Some(pack_name) = pack_source_name(&pack_use.source) else {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-PACK-001".to_owned(),
                message: format!(
                    "app pack `{}` points to `{}`, but packs must use `packs.<name>` or `registry.packs.<name>`.",
                    pack_use.name, pack_use.source
                ),
            });
            continue;
        };

        let Some(pack) = registry.and_then(|registry| {
            registry
                .manifest
                .packs
                .iter()
                .find(|pack| pack.name == pack_name)
        }) else {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-PACK-002".to_owned(),
                message: format!(
                    "app pack `{}` references registry pack `{pack_name}`, but no such pack exists in `registry.lzi`.",
                    pack_use.name
                ),
            });
            continue;
        };

        for requirement in &pack.requirements {
            if requirement.kind == "integration"
                && !integrations
                    .values()
                    .any(|contract| *contract == requirement.contract)
            {
                diagnostics.push(DoctorDiagnostic {
                    path: app.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "APP-PACK-003".to_owned(),
                    message: format!(
                        "enabled pack `{}` requires integration `{}`: `{}`, but app/registry declares no integration with that contract.",
                        pack_use.name, requirement.name, requirement.contract
                    ),
                });
            }
        }
    }

    diagnostics
}

fn adapter_provenance_diagnostics(
    app: &DoctorAppManifest,
    registry: Option<&DoctorAppRegistry>,
    profiles: &[DoctorAppProfile],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for integration in &app.manifest.integrations {
        if integration.adapter.is_some() && integration.adapter_provenance.is_none() {
            diagnostics.push(adapter_source_diagnostic(
                app.path.clone(),
                "APP-ADAPTER-001",
                &integration.name,
                integration.adapter.as_deref().unwrap_or_default(),
            ));
        }
    }

    if let Some(registry) = registry {
        for integration in &registry.manifest.integrations {
            if integration.adapter.is_some() && integration.adapter_provenance.is_none() {
                diagnostics.push(adapter_source_diagnostic(
                    registry.path.clone(),
                    "REG-ADAPTER-001",
                    &integration.name,
                    integration.adapter.as_deref().unwrap_or_default(),
                ));
            }
        }
    }

    for profile in profiles {
        for integration in &profile.profile.integrations {
            if integration.adapter.is_some() && integration.adapter_provenance.is_none() {
                diagnostics.push(adapter_source_diagnostic(
                    profile.path.clone(),
                    "PROFILE-ADAPTER-001",
                    &integration.name,
                    integration.adapter.as_deref().unwrap_or_default(),
                ));
            }
        }
    }

    diagnostics
}

fn adapter_source_diagnostic(
    path: PathBuf,
    code: &str,
    integration_name: &str,
    adapter: &str,
) -> DoctorDiagnostic {
    DoctorDiagnostic {
        path,
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: code.to_owned(),
        message: format!(
            "integration `{integration_name}` uses adapter `{adapter}`, but adapter sources must declare provenance with `@runtime/...`, `@plugin/publisher/name`, `@adapter.<local>`, or a local path."
        ),
    }
}

fn enabled_pack_provided_features<'a>(
    app: &'a AppManifest,
    registry: Option<&'a DoctorAppRegistry>,
) -> BTreeSet<&'a str> {
    let mut features = BTreeSet::new();
    let Some(registry) = registry else {
        return features;
    };

    for pack_use in &app.packs {
        let Some(pack_name) = pack_source_name(&pack_use.source) else {
            continue;
        };
        let Some(pack) = registry
            .manifest
            .packs
            .iter()
            .find(|pack| pack.name == pack_name)
        else {
            continue;
        };

        for provide in &pack.provides {
            if provide.kind == "feature" {
                features.insert(provide.name.as_str());
            }
        }
    }

    features
}

fn enabled_pack_integration_requirements<'a>(
    app: &'a AppManifest,
    registry: Option<&'a DoctorAppRegistry>,
) -> Vec<(&'a str, &'a str, &'a str)> {
    let mut requirements = Vec::new();
    let Some(registry) = registry else {
        return requirements;
    };

    for pack_use in &app.packs {
        let Some(pack_name) = pack_source_name(&pack_use.source) else {
            continue;
        };
        let Some(pack) = registry
            .manifest
            .packs
            .iter()
            .find(|pack| pack.name == pack_name)
        else {
            continue;
        };

        for requirement in &pack.requirements {
            if requirement.kind == "integration" {
                requirements.push((
                    pack_use.name.as_str(),
                    requirement.name.as_str(),
                    requirement.contract.as_str(),
                ));
            }
        }
    }

    requirements
}

fn integration_source_name(source: &str) -> Option<&str> {
    source
        .strip_prefix("integrations.")
        .or_else(|| source.strip_prefix("registry.integrations."))
}

fn pack_source_name(source: &str) -> Option<&str> {
    source
        .strip_prefix("packs.")
        .or_else(|| source.strip_prefix("registry.packs."))
}

fn integration_environment_allowed(
    app: &AppManifest,
    registry: Option<&DoctorAppRegistry>,
    name: &str,
    environment: &str,
) -> bool {
    app.integrations
        .iter()
        .chain(
            registry
                .into_iter()
                .flat_map(|registry| registry.manifest.integrations.iter()),
        )
        .find(|integration| integration.name == name)
        .is_some_and(|integration| {
            integration.environments.is_empty()
                || integration
                    .environments
                    .iter()
                    .any(|allowed| allowed == environment)
        })
}

fn app_service_contract_diagnostics(
    app: &DoctorAppManifest,
    operational: &OperationalFacts,
    pack_features: &BTreeSet<&str>,
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
        if !operational.features.contains_key(*owned) && !pack_features.contains(*owned) {
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

fn profile_url_target_valid(app: &AppManifest, target: &str) -> bool {
    target == "api" && app_has_target(app, "backend") || app_has_target(app, target)
}

fn app_has_url(app: &AppManifest, profiles: &[DoctorAppProfile], target: &str) -> bool {
    app.urls.iter().any(|url| url.target == target)
        || profiles
            .iter()
            .flat_map(|profile| profile.profile.urls.iter())
            .any(|url| url.target == target)
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

fn parse_external_call_header(trimmed: &str) -> Option<(&str, &str)> {
    let rest = trimmed.strip_prefix("calls ")?;
    let (slot, operation) = rest.trim().split_once('.')?;
    let slot = slot.trim();
    let operation = operation.trim();

    if is_identifier(slot) && is_identifier(operation) {
        Some((slot, operation))
    } else {
        None
    }
}

fn block_has_prefixed_line(lines: &[&str], prefix: &str) -> bool {
    lines
        .iter()
        .skip(1)
        .any(|line| leading_spaces(line) == 4 && line.trim_start().starts_with(prefix))
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

// =============================================================================
// Cut A — cross-feature diagnostics (8 ids per plan §5.2).
//
// Lives downstream of the typed `AgentFacts` collected during package
// load and the per-feature symbol tables populated by
// `collect_feature_symbols`. File-local checks (predicate shape, block
// well-formedness) stay in `crates/lazuli_lsp/src/lib.rs`; this module
// owns the workspace-wide work that the LSP cannot perform.
// =============================================================================

/// Walk a `.lzi` file's text and harvest the names that downstream Cut A
/// diagnostics need to resolve across features:
///
///   - `enum <Name>` headers (for `output discriminator <Enum>` targets)
///   - `record <Name>` headers + `<field>: <type>` children + the
///     `discriminator` marker on the disambiguation field
///   - `command <name>` and `query.{list,lookup,sql} <name>` headers with
///     their `policy @policy.<rule>` if present
///
/// The walker is text-based on purpose: the canonical-indent parser only
/// covers `agent` blocks today. When later cuts migrate the rest of the
/// feature body to typed AST, this collector collapses into the IR.
fn collect_feature_symbols(
    file: &DoctorFile,
    feature_symbols: &mut BTreeMap<String, FeatureSymbols>,
) {
    let lines: Vec<&str> = file.source.lines().collect();

    let mut feature_ranges: Vec<(String, usize, usize)> = Vec::new();
    let mut current_start: Option<(String, usize)> = None;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            if let Some((prev_name, prev_start)) = current_start.take() {
                feature_ranges.push((prev_name, prev_start, index));
            }
            if let Some(name) = trimmed.strip_prefix("feature ") {
                current_start = Some((name.trim().to_owned(), index));
            }
        }
    }
    if let Some((name, start)) = current_start {
        feature_ranges.push((name, start, lines.len()));
    }

    for (feature, start, end) in feature_ranges {
        let symbols = feature_symbols.entry(feature.clone()).or_default();
        scan_feature_range(file, &lines[start..end], start, symbols);
    }
}

fn scan_feature_range(
    file: &DoctorFile,
    lines: &[&str],
    feature_start: usize,
    symbols: &mut FeatureSymbols,
) {
    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i];
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }

        // The fixture's domain block lives at indent 2 with payload at 4.
        // Most enum / record / command / query declarations appear at
        // indent 4 inside `domain`, or at indent 2 directly under feature.
        // Either form is tolerated.
        let leading = leading_spaces(raw);
        if leading < 2 {
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("enum ") {
            let name = rest.split_whitespace().next().unwrap_or("").to_owned();
            if !name.is_empty() {
                symbols.enums.insert(
                    name,
                    SymbolFact {
                        path: file.path.clone(),
                        line: feature_start + i + 1,
                    },
                );
            }
        } else if let Some(rest) = trimmed.strip_prefix("record ") {
            let name = rest.split_whitespace().next().unwrap_or("").to_owned();
            if !name.is_empty() {
                let record_indent = leading;
                let mut record = RecordFact {
                    base: SymbolFact {
                        path: file.path.clone(),
                        line: feature_start + i + 1,
                    },
                    fields: BTreeMap::new(),
                };
                let mut j = i + 1;
                while j < lines.len() {
                    let inner = lines[j];
                    let inner_trim = inner.trim_start();
                    if inner_trim.is_empty() || inner_trim.starts_with('#') {
                        j += 1;
                        continue;
                    }
                    if leading_spaces(inner) <= record_indent {
                        break;
                    }
                    if let Some(field) = parse_record_field(inner_trim) {
                        record.fields.insert(field.0, field.1);
                    }
                    j += 1;
                }
                symbols.records.insert(name, record);
            }
        } else if let Some(rest) = trimmed.strip_prefix("command ") {
            let name = rest.split_whitespace().next().unwrap_or("").to_owned();
            if !name.is_empty() {
                let policy = scan_block_for_policy(&lines[i + 1..], leading_spaces(raw));
                symbols.commands.insert(
                    name,
                    CommandSymbolFact {
                        base: SymbolFact {
                            path: file.path.clone(),
                            line: feature_start + i + 1,
                        },
                        policy,
                    },
                );
            }
        } else if let Some(rest) = trimmed.strip_prefix("query.list ") {
            insert_query_symbol(
                rest,
                ir::ToolKind::QueryList,
                file,
                feature_start + i + 1,
                raw,
                &lines[i + 1..],
                symbols,
            );
        } else if let Some(rest) = trimmed.strip_prefix("query.lookup ") {
            insert_query_symbol(
                rest,
                ir::ToolKind::QueryLookup,
                file,
                feature_start + i + 1,
                raw,
                &lines[i + 1..],
                symbols,
            );
        } else if let Some(rest) = trimmed.strip_prefix("query.sql ") {
            insert_query_symbol(
                rest,
                ir::ToolKind::QuerySql,
                file,
                feature_start + i + 1,
                raw,
                &lines[i + 1..],
                symbols,
            );
        }
        i += 1;
    }
}

fn parse_record_field(trimmed: &str) -> Option<(String, RecordFieldFact)> {
    let (name_part, rest) = trimmed.split_once(':')?;
    let name = name_part.trim();
    if name.is_empty() {
        return None;
    }
    let mut tokens = rest.split_whitespace();
    let type_text = tokens.next()?.to_owned();
    let is_discriminator = tokens.any(|tok| tok == "discriminator");
    Some((
        name.to_owned(),
        RecordFieldFact {
            type_text,
            is_discriminator,
        },
    ))
}

fn insert_query_symbol(
    rest: &str,
    kind: ir::ToolKind,
    file: &DoctorFile,
    line_number: usize,
    raw_header: &str,
    body: &[&str],
    symbols: &mut FeatureSymbols,
) {
    let name = rest.split_whitespace().next().unwrap_or("").to_owned();
    if name.is_empty() {
        return;
    }
    let policy = scan_block_for_policy(body, leading_spaces(raw_header));
    symbols.queries.insert(
        name,
        QuerySymbolFact {
            base: SymbolFact {
                path: file.path.clone(),
                line: line_number,
            },
            policy,
            kind,
        },
    );
}

fn scan_block_for_policy(body: &[&str], parent_indent: usize) -> Option<String> {
    for line in body {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) <= parent_indent {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("policy ") {
            return Some(rest.trim().to_owned());
        }
    }
    None
}

// -----------------------------------------------------------------------------
// Diagnostic id: tool_registry_effect_required_diagnostics
// -----------------------------------------------------------------------------

fn registry_tool_effect_diagnostics(defects: &[RegistryToolDefect]) -> Vec<DoctorDiagnostic> {
    defects
        .iter()
        .map(|defect| DoctorDiagnostic {
            path: defect.path.clone(),
            line: defect.line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "tool_registry_effect_required_diagnostics".to_owned(),
            message: match defect.reason {
                RegistryToolDefectReason::EffectMissing => format!(
                    "registry tool `{}` is missing `effect: read | write`; doctor cannot derive the tool's effect from the registry without it.",
                    defect.name
                ),
                RegistryToolDefectReason::EffectInvalid => format!(
                    "registry tool `{}` declares an unknown `effect`; valid values are `read` and `write`.",
                    defect.name
                ),
            },
        })
        .collect()
}

// -----------------------------------------------------------------------------
// Diagnostic ids: agent_tool_policy / write_unguarded / pii_unsafetied
// -----------------------------------------------------------------------------

fn agent_tool_diagnostics(
    agents: &[AgentFacts],
    feature_symbols: &BTreeMap<String, FeatureSymbols>,
    registry: Option<&DoctorAppRegistry>,
    command_approvals: &[CommandApprovalFact],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let registry_tools: BTreeMap<String, &lazuli_ir::RegistryToolEntry> = registry
        .map(|r| {
            r.manifest
                .tools
                .iter()
                .map(|t| (t.name.clone(), t))
                .collect()
        })
        .unwrap_or_default();

    // Cut A.9: write-tool guard accepts `approval` on the target
    // command as an alternative to the agent's `safety` validator.
    // Build a quick lookup keyed by (feature, command) so the guard
    // check resolves per-tool in O(1).
    let approval_index: BTreeSet<(String, String)> = command_approvals
        .iter()
        .filter(|f| f.missing_children.is_empty())
        .map(|f| (f.feature.clone(), f.command.clone()))
        .collect();

    for fact in agents {
        let agent = &fact.agent;
        let agent_safety_empty = agent.safety.is_empty();
        let mut has_unguarded_write_tool = false;
        let mut has_pii_tool = false;
        let agent_policy_text = format_agent_policy(agent);

        for binding in &agent.tools {
            let (tool_label, resolved) =
                resolve_tool(fact, &binding.reference, feature_symbols, &registry_tools);

            if resolved.effect == ResolvedToolEffect::Write {
                // Check whether the target command carries an
                // `approval` block — that satisfies the write-tool
                // guard for this binding, regardless of the agent's
                // own `safety` list.
                let approved = match &binding.reference {
                    lazuli_ir::QualifiedToolRef::Local { kind, name }
                        if matches!(kind, lazuli_ir::ToolKind::Command) =>
                    {
                        approval_index.contains(&(fact.feature.clone(), name.clone()))
                    }
                    lazuli_ir::QualifiedToolRef::CrossFeature {
                        feature,
                        kind,
                        name,
                    } if matches!(kind, lazuli_ir::ToolKind::Command) => {
                        approval_index.contains(&(feature.clone(), name.clone()))
                    }
                    _ => false,
                };
                if !approved {
                    has_unguarded_write_tool = true;
                }
            }
            if !resolved.pii_classes.is_empty() {
                has_pii_tool = true;
            }

            // agent_tool_policy_diagnostics: when the tool's policy is
            // known and stricter than the agent's, emit. Cut A keeps the
            // comparison conservative: we report a gap only when both
            // sides resolve and the tool's policy is *more* restrictive
            // by surface (atom set is a strict superset). Full lattice
            // ranking lands when the policy lattice helper migrates here.
            if let Some(tool_policy) = &resolved.policy {
                if policy_atoms_more_restrictive(tool_policy, &agent_policy_text) {
                    diagnostics.push(DoctorDiagnostic {
                        path: fact.path.clone(),
                        line: fact.line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "agent_tool_policy_diagnostics".to_owned(),
                        message: format!(
                            "agent `{}` declares policy `{}`, but tool `{}` requires `{}` — agent policy must be at least as strict as every tool.",
                            agent.name,
                            agent_policy_text,
                            tool_label,
                            tool_policy,
                        ),
                    });
                }
            }
        }

        // agent_tool_write_unguarded_diagnostics: every write tool
        // must be guarded by either the agent's `safety` validator or
        // the target command's `approval` block (Cut A.9 extension).
        // `has_unguarded_write_tool` only stays true for write tools
        // whose command has no approval — agent.safety is the
        // fallback guard for those.
        if has_unguarded_write_tool && agent_safety_empty {
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "agent_tool_write_unguarded_diagnostics".to_owned(),
                message: format!(
                    "agent `{}` dispatches a `write` tool with neither `safety @validator.<name>` on the agent nor `approval` on the target command; Cut A requires at least one guard for write-effect tools.",
                    agent.name
                ),
            });
        }

        // agent_pii_unsafetied_warning: any PII-bearing tool plus an
        // empty safety list emits a warning.
        if has_pii_tool && agent_safety_empty {
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "agent_pii_unsafetied_warning".to_owned(),
                message: format!(
                    "agent `{}` invokes a tool that resolves to a `@pii.*` class but declares no `safety @validator.<name>`; consider adding a scrub validator.",
                    agent.name
                ),
            });
        }
    }
    diagnostics
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResolvedToolEffect {
    Read,
    Write,
    Unknown,
}

#[derive(Debug, Clone)]
struct ResolvedTool {
    effect: ResolvedToolEffect,
    policy: Option<String>,
    pii_classes: Vec<String>,
}

fn resolve_tool(
    fact: &AgentFacts,
    reference: &ir::QualifiedToolRef,
    feature_symbols: &BTreeMap<String, FeatureSymbols>,
    registry_tools: &BTreeMap<String, &lazuli_ir::RegistryToolEntry>,
) -> (String, ResolvedTool) {
    match reference {
        ir::QualifiedToolRef::Adapter { dotted } => {
            let key = dotted.join(".");
            let label = format!("@tool.{key}");
            let resolved = registry_tools
                .get(&key)
                .map(|entry| ResolvedTool {
                    effect: match entry.effect {
                        ir::ToolEffect::Read => ResolvedToolEffect::Read,
                        ir::ToolEffect::Write => ResolvedToolEffect::Write,
                    },
                    policy: None,
                    pii_classes: entry.pii_classes.iter().map(|q| q.name.clone()).collect(),
                })
                .unwrap_or(ResolvedTool {
                    effect: ResolvedToolEffect::Unknown,
                    policy: None,
                    pii_classes: Vec::new(),
                });
            (label, resolved)
        }
        ir::QualifiedToolRef::Local { kind, name }
        | ir::QualifiedToolRef::CrossFeature { kind, name, .. } => {
            let owning_feature = match reference {
                ir::QualifiedToolRef::CrossFeature { feature, .. } => feature.clone(),
                _ => fact.feature.clone(),
            };
            let kind_word = tool_kind_word(*kind);
            let label = format!("{}.{}.{}", owning_feature, kind_word, name);
            let symbols = feature_symbols.get(&owning_feature);
            let resolved = match (*kind, symbols) {
                (ir::ToolKind::Command, Some(syms)) => syms
                    .commands
                    .get(name)
                    .map(|cmd| ResolvedTool {
                        effect: ResolvedToolEffect::Write,
                        policy: cmd.policy.clone(),
                        pii_classes: Vec::new(),
                    })
                    .unwrap_or(ResolvedTool {
                        effect: ResolvedToolEffect::Write,
                        policy: None,
                        pii_classes: Vec::new(),
                    }),
                (
                    ir::ToolKind::QueryList
                    | ir::ToolKind::QueryLookup
                    | ir::ToolKind::QuerySql
                    | ir::ToolKind::QueryUnspecified,
                    Some(syms),
                ) => syms
                    .queries
                    .get(name)
                    .map(|q| ResolvedTool {
                        effect: ResolvedToolEffect::Read,
                        policy: q.policy.clone(),
                        pii_classes: Vec::new(),
                    })
                    .unwrap_or(ResolvedTool {
                        effect: ResolvedToolEffect::Read,
                        policy: None,
                        pii_classes: Vec::new(),
                    }),
                (ir::ToolKind::Command, None) => ResolvedTool {
                    effect: ResolvedToolEffect::Write,
                    policy: None,
                    pii_classes: Vec::new(),
                },
                _ => ResolvedTool {
                    effect: ResolvedToolEffect::Read,
                    policy: None,
                    pii_classes: Vec::new(),
                },
            };
            (label, resolved)
        }
    }
}

fn tool_kind_word(kind: ir::ToolKind) -> &'static str {
    match kind {
        ir::ToolKind::QueryList => "query.list",
        ir::ToolKind::QueryLookup => "query.lookup",
        ir::ToolKind::QuerySql => "query.sql",
        ir::ToolKind::QueryUnspecified => "query",
        ir::ToolKind::Command => "command",
        ir::ToolKind::Api => "api",
    }
}

fn format_agent_policy(agent: &Agent) -> String {
    match agent.policy.as_ref() {
        Some(ir::PolicyRef::Atom(name)) => format!("@{name}"),
        Some(ir::PolicyRef::Local(name)) => format!("@policy.{name}"),
        Some(ir::PolicyRef::External { feature, name }) => format!("{feature}.{name}"),
        Some(ir::PolicyRef::Unresolved(text)) => text.clone(),
        Some(ir::PolicyRef::None) | None => "<none>".to_owned(),
    }
}

/// Conservative `more restrictive than` check: a policy is considered
/// stricter than the agent's when both texts parse as `@policy.<x>` and
/// the names diverge in a documented hierarchy. For Cut A we surface a
/// gap whenever the tool policy text is a non-empty stricter category
/// (`delete`, `update`) and the agent's is a weaker one (`read`).
///
/// Plan §5.4 punts the full lattice migration to a later cut; this stub
/// keeps the diagnostic firing for the obvious cases without false
/// positives.
fn policy_atoms_more_restrictive(tool_policy: &str, agent_policy: &str) -> bool {
    let order = |text: &str| match text {
        s if s.contains("delete") => 3,
        s if s.contains("update") => 2,
        s if s.contains("create") => 1,
        s if s.contains("read") => 0,
        _ => 0,
    };
    order(tool_policy) > order(agent_policy)
}

// -----------------------------------------------------------------------------
// Diagnostic ids: agent_discriminator_target_invalid / field_invalid
// -----------------------------------------------------------------------------

fn agent_discriminator_diagnostics(
    agents: &[AgentFacts],
    feature_symbols: &BTreeMap<String, FeatureSymbols>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for fact in agents {
        let agent = &fact.agent;
        match (&agent.output_kind, agent.output_discriminator.as_ref()) {
            (ir::AgentOutputKind::DiscriminatedEnum, Some(ir::DiscriminatorRef::Enum(qn))) => {
                let enum_name = &qn.name;
                let found = feature_symbols
                    .values()
                    .any(|symbols| symbols.enums.contains_key(enum_name));
                if !found {
                    diagnostics.push(DoctorDiagnostic {
                        path: fact.path.clone(),
                        line: fact.line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "agent_discriminator_target_invalid_diagnostics".to_owned(),
                        message: format!(
                            "agent `{}` declares `output discriminator {}` but no enum named `{}` exists in any reachable feature.",
                            agent.name, enum_name, enum_name,
                        ),
                    });
                }
            }
            // DiscriminatedRecord lowering produces output_kind=Text +
            // output_type=Unresolved("X") today; the expand pass (Phase 5)
            // is what promotes to DiscriminatedRecord and resolves the
            // discriminator field. Until then `agent_discriminator_field_invalid`
            // has no producer in IR, but we still report when the bare
            // `output <Record>` references an unknown record so the
            // author gets a fast signal.
            (ir::AgentOutputKind::Text, _) => {
                if let Some(ir::TypeRef::Unresolved(name)) = agent.output_type.as_ref() {
                    // Heuristic: titlecase first letter means it's an
                    // intended record/enum reference (vs `Text`/`Integer`
                    // which match Builtin earlier).
                    let first = name.chars().next();
                    if first.is_some_and(|c| c.is_ascii_uppercase()) {
                        let found = feature_symbols.values().any(|symbols| {
                            symbols.records.contains_key(name) || symbols.enums.contains_key(name)
                        });
                        if !found {
                            diagnostics.push(DoctorDiagnostic {
                                path: fact.path.clone(),
                                line: fact.line,
                                column: 1,
                                severity: DoctorSeverity::Error,
                                code: "agent_discriminator_target_invalid_diagnostics".to_owned(),
                                message: format!(
                                    "agent `{}` declares `output {}` but no enum or record named `{}` exists in any reachable feature.",
                                    agent.name, name, name,
                                ),
                            });
                            continue;
                        }
                        // Validate field-level discriminator marker on
                        // records — proposal §A2 requires exactly one
                        // field carrying the marker, and its type must
                        // resolve to an enum.
                        for symbols in feature_symbols.values() {
                            if let Some(record) = symbols.records.get(name) {
                                diagnostics.extend(check_record_discriminator(
                                    fact,
                                    agent,
                                    name,
                                    record,
                                    feature_symbols,
                                ));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    diagnostics
}

fn check_record_discriminator(
    fact: &AgentFacts,
    agent: &Agent,
    record_name: &str,
    record: &RecordFact,
    feature_symbols: &BTreeMap<String, FeatureSymbols>,
) -> Vec<DoctorDiagnostic> {
    let markers: Vec<&String> = record
        .fields
        .iter()
        .filter(|(_, f)| f.is_discriminator)
        .map(|(name, _)| name)
        .collect();

    if markers.is_empty() {
        // No discriminator: it's a legacy `output <Record>` shape, not a
        // DiscriminatedRecord. Cut A's soft-warn for legacy output is
        // emitted in the LSP file-local layer (Phase 4); nothing to do
        // here.
        return Vec::new();
    }

    let mut diagnostics = Vec::new();

    if markers.len() > 1 {
        diagnostics.push(DoctorDiagnostic {
            path: fact.path.clone(),
            line: fact.line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "agent_discriminator_field_invalid_diagnostics".to_owned(),
            message: format!(
                "agent `{}` references record `{}` with {} `discriminator` markers; at most one field per record may carry the marker.",
                agent.name,
                record_name,
                markers.len(),
            ),
        });
        return diagnostics;
    }

    let field_name = markers[0];
    let field = &record.fields[field_name];
    let enum_exists = feature_symbols
        .values()
        .any(|s| s.enums.contains_key(&field.type_text));
    if !enum_exists {
        diagnostics.push(DoctorDiagnostic {
            path: fact.path.clone(),
            line: fact.line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "agent_discriminator_field_invalid_diagnostics".to_owned(),
            message: format!(
                "agent `{}` references record `{}` whose discriminator field `{}` has type `{}`, but no enum by that name exists; the marked field must resolve to an enum.",
                agent.name, record_name, field_name, field.type_text,
            ),
        });
    }

    diagnostics
}

// -----------------------------------------------------------------------------
// Diagnostic ids: eval_ordered_op_invalid / eval_nondeterministic_warning
// -----------------------------------------------------------------------------

fn agent_eval_diagnostics(agents: &[AgentFacts]) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for fact in agents {
        let agent = &fact.agent;

        if !agent.evals.is_empty() && (agent.temperature != Some(0.0) || agent.seed.is_none()) {
            let reason = if agent.temperature != Some(0.0) {
                "missing `temperature 0`"
            } else {
                "missing `seed <int>`"
            };
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "eval_nondeterministic_warning".to_owned(),
                message: format!(
                    "agent `{}` declares `evals` but the agent is non-deterministic ({}); cases run as informational results until both `temperature 0` and `seed <int>` are pinned.",
                    agent.name, reason,
                ),
            });
        }

        for case in &agent.evals {
            for assertion in &case.assertions {
                if let ir::EvalPredicate::Closed(ir::Predicate::Comparison { left, op, right }) =
                    &assertion.predicate
                {
                    if matches!(
                        op,
                        ir::CompareOp::Lt
                            | ir::CompareOp::Le
                            | ir::CompareOp::Gt
                            | ir::CompareOp::Ge
                    ) && !operand_resolves_numeric(left)
                        && !operand_resolves_numeric(right)
                    {
                        diagnostics.push(DoctorDiagnostic {
                            path: fact.path.clone(),
                            line: fact.line,
                            column: 1,
                            severity: DoctorSeverity::Error,
                            code: "eval_ordered_op_invalid_diagnostics".to_owned(),
                            message: format!(
                                "agent `{}` eval case `{}` uses an ordered operator but neither operand resolves to a numeric type; ordered comparisons require numeric refs (`<ref>.length`, `<ref>.count`, integer fields).",
                                agent.name, case.name,
                            ),
                        });
                    }
                }
            }
        }
    }

    diagnostics
}

/// Best-effort numeric-operand check. The closed-predicate IR doesn't
/// carry resolved types yet, so we accept the canonical numeric paths
/// (`<x>.length`, `<x>.count`) and integer literals. Everything else is
/// rejected as non-numeric — authors who hit a false positive can split
/// the case until type resolution arrives.
fn operand_resolves_numeric(expr: &ir::Expr) -> bool {
    match expr {
        ir::Expr::Integer(_) => true,
        ir::Expr::Path(path) => {
            let last = path.segments.last().map(String::as_str);
            matches!(last, Some("length") | Some("count") | Some("size"))
        }
        _ => false,
    }
}

// -----------------------------------------------------------------------------
// Cut A.7 — `expose http` cross-feature diagnostics
// -----------------------------------------------------------------------------

/// Walk every agent with `expose_http` plus every `api` path
/// declared in source. Reject cross-feature collisions on (normalised
/// path, method) and `audience` references that don't resolve to any
/// known `.lzx` surface or `app.lzi` audience declaration.
fn agent_expose_diagnostics(
    agents: &[AgentFacts],
    api_paths: &[ApiPathFact],
    known_audiences: &BTreeSet<String>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // Collect every (method, normalized_path) pair from agent expose
    // blocks + every api block, anchored to their source location.
    let mut pairs: Vec<ExposePathFact> = Vec::new();
    for fact in agents {
        let Some(expose) = fact.agent.expose_http.as_ref() else {
            continue;
        };
        pairs.push(ExposePathFact {
            path_normalised: normalise_path(&expose.path),
            path_raw: expose.path.clone(),
            method: http_method_word(expose.method).to_owned(),
            origin: format!("agent {}.{}", fact.feature, fact.agent.name),
            owner_path: fact.path.clone(),
            line: fact.line,
        });
    }
    for api in api_paths {
        pairs.push(ExposePathFact {
            path_normalised: normalise_path(&api.path),
            path_raw: api.path.clone(),
            method: api.method.clone(),
            origin: format!("api {}.{}", api.feature, api.name),
            owner_path: api.source_path.clone(),
            line: api.line,
        });
    }

    // Cross-feature path collision detection. Two facts collide when
    // they share (normalized_path, method) but originate from
    // different feature/api ids — same feature/agent collisions are
    // file-local and surface in LSP instead.
    for (i, a) in pairs.iter().enumerate() {
        for b in pairs.iter().skip(i + 1) {
            if a.path_normalised == b.path_normalised
                && a.method == b.method
                && a.origin != b.origin
            {
                diagnostics.push(DoctorDiagnostic {
                    path: a.owner_path.clone(),
                    line: a.line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "agent_expose_path_conflict_cross_feature_diagnostics".to_owned(),
                    message: format!(
                        "{origin_a} declares HTTP path `{path}` ({method}) that conflicts with {origin_b}; same method+path must originate from a single feature.",
                        origin_a = a.origin,
                        origin_b = b.origin,
                        path = a.path_raw,
                        method = a.method,
                    ),
                });
            }
        }
    }

    // Audience reachability check.
    for fact in agents {
        let Some(expose) = fact.agent.expose_http.as_ref() else {
            continue;
        };
        let Some(audience) = expose.audience.as_ref() else {
            continue;
        };
        if !known_audiences.contains(audience) {
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "agent_expose_audience_unknown_diagnostics".to_owned(),
                message: format!(
                    "agent `{}` declares `expose http audience {audience}`, but no `.lzx` surface or `app.lzi` audience declares it.",
                    fact.agent.name,
                ),
            });
        }
    }

    diagnostics
}

#[derive(Debug, Clone)]
struct ApiPathFact {
    feature: String,
    name: String,
    method: String,
    path: String,
    source_path: PathBuf,
    line: usize,
}

#[derive(Debug, Clone)]
struct ExposePathFact {
    path_normalised: String,
    path_raw: String,
    method: String,
    origin: String,
    owner_path: PathBuf,
    line: usize,
}

/// Normalise a URL path so `:foo` and `:bar` become a single
/// placeholder. Two paths with the same shape but different slot
/// names collide at the gateway, so the check should treat them as
/// equal.
fn normalise_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for segment in path.split('/') {
        if !out.is_empty() {
            out.push('/');
        } else if path.starts_with('/') {
            // preserve leading `/`
        }
        if let Some(_name) = segment.strip_prefix(':') {
            out.push_str(":_");
        } else {
            out.push_str(segment);
        }
    }
    if path.starts_with('/') && !out.starts_with('/') {
        format!("/{out}")
    } else {
        out
    }
}

fn http_method_word(method: ir::HttpMethod) -> &'static str {
    match method {
        ir::HttpMethod::Get => "GET",
        ir::HttpMethod::Post => "POST",
        ir::HttpMethod::Put => "PUT",
        ir::HttpMethod::Patch => "PATCH",
        ir::HttpMethod::Delete => "DELETE",
    }
}

/// Collect every `api <name>` block in a feature body. Used by Cut A.7
/// to cross-check paths against `expose http`. Text-pattern matches
/// the rest of doctor's feature scanning until the canonical-indent
/// slice covers `api`.
fn collect_api_paths(file: &DoctorFile, api_paths: &mut Vec<ApiPathFact>) {
    let lines: Vec<&str> = file.source.lines().collect();
    let mut feature: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            feature = trimmed
                .strip_prefix("feature ")
                .map(|name| name.trim().to_owned());
            i += 1;
            continue;
        }
        if leading_spaces(line) == 2 && trimmed.starts_with("api ") {
            let name = trimmed
                .strip_prefix("api ")
                .map(|n| n.split_whitespace().next().unwrap_or("").to_owned())
                .unwrap_or_default();
            let feature_name = feature.clone().unwrap_or_default();
            let api_line = i + 1;
            let mut method: Option<String> = None;
            let mut path: Option<String> = None;
            let mut j = i + 1;
            while j < lines.len() {
                let inner = lines[j];
                let inner_trim = inner.trim_start();
                if inner_trim.is_empty() || inner_trim.starts_with('#') {
                    j += 1;
                    continue;
                }
                if leading_spaces(inner) <= 2 {
                    break;
                }
                if leading_spaces(inner) == 4 {
                    if let Some(rest) = inner_trim.strip_prefix("method ") {
                        method = Some(rest.trim().to_owned());
                    } else if let Some(rest) = inner_trim.strip_prefix("path ") {
                        path = Some(strip_quotes(rest.trim()).to_owned());
                    }
                }
                j += 1;
            }
            if let (Some(method), Some(path)) = (method, path) {
                if !name.is_empty() {
                    api_paths.push(ApiPathFact {
                        feature: feature_name,
                        name,
                        method,
                        path,
                        source_path: file.path.clone(),
                        line: api_line,
                    });
                }
            }
            i = j;
            continue;
        }
        i += 1;
    }
}

/// Collect every audience that's a first-class declaration in the
/// workspace. Today, surfaces in `.lzx` files are the canonical source
/// (`surface customer web` ... `audience admin`). A future cut may
/// elevate audiences to top-level `app.lzi` declarations; until then,
/// `.lzi` text scans are intentionally avoided — counting `audience X`
/// references inside e.g. `expose http audience X` would defeat the
/// `agent_expose_audience_unknown_diagnostics` check by self-resolving.
fn collect_known_audiences(files: &[DoctorFile]) -> BTreeSet<String> {
    let mut audiences = BTreeSet::new();
    for file in files {
        if let Some(document) = file.lzx.as_ref() {
            for surface in &document.surfaces {
                for audience in &surface.audiences {
                    audiences.insert(audience.name.clone());
                }
            }
        }
    }
    audiences
}

fn strip_quotes(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(text)
}

// -----------------------------------------------------------------------------
// Cut A.11 — `cors` block cross-feature checks
//
// CORS lives in `app.lzi` (language-light tier) alongside `urls`.
// Doctor validates origins against the declared environments + urls
// and catches the CORS-spec violation of `allow_origins "*"` with
// `allow_credentials true`.
// -----------------------------------------------------------------------------

fn cors_diagnostics(app: Option<&DoctorAppManifest>) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let Some(app_manifest) = app else {
        return diagnostics;
    };
    let Some(cors) = app_manifest.manifest.cors.as_ref() else {
        return diagnostics;
    };

    let environments: BTreeSet<&str> = app_manifest
        .manifest
        .environments
        .iter()
        .map(String::as_str)
        .collect();
    let declared_urls: Vec<&lazuli_ir::AppUrl> = app_manifest.manifest.urls.iter().collect();

    let mut has_wildcard = false;
    for rule in &cors.allow_origins {
        // Environment must be declared in `app.environments`.
        if !environments.contains(rule.environment.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: app_manifest.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "cors_unknown_environment_diagnostics".to_owned(),
                message: format!(
                    "`cors allow_origins {} ...` references an environment that is not in `app.environments` ({}).",
                    rule.environment,
                    environments_summary(&environments),
                ),
            });
        }

        for origin in &rule.origins {
            if origin == "*" {
                has_wildcard = true;
                continue;
            }
            // Wildcards in subdomain (`https://*.example.com`) skip
            // url-match — they're intentionally broader than any
            // single URL declaration.
            if origin.contains("*") {
                continue;
            }
            // Compare against declared urls in the same environment.
            // The match is prefix-based: a declared URL
            // `https://app.example.com` allows the origin
            // `https://app.example.com` (exact) — query string and
            // path differences are tolerated by the CORS layer, so
            // we compare scheme+host.
            let documented = declared_urls
                .iter()
                .filter(|u| u.environment == rule.environment)
                .any(|u| same_origin(&u.url, origin));
            if !documented {
                diagnostics.push(DoctorDiagnostic {
                    path: app_manifest.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "cors_origin_undocumented_diagnostics".to_owned(),
                    message: format!(
                        "`cors allow_origins {env} \"{origin}\"` does not match any `url <target> {env} ...` declaration. If the origin is a third-party caller, ignore; otherwise update `urls` so the source-of-truth stays consistent.",
                        env = rule.environment,
                    ),
                });
            }
        }
    }

    // CORS spec forbids `allow_origins "*"` with `allow_credentials true`.
    if has_wildcard && cors.allow_credentials {
        diagnostics.push(DoctorDiagnostic {
            path: app_manifest.path.clone(),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "cors_credentials_wildcard_conflict_diagnostics".to_owned(),
            message: "`cors allow_origins ... \"*\"` cannot be combined with `allow_credentials true`. Per CORS spec, browsers reject the response. Either narrow the origin list or set `allow_credentials false`.".to_owned(),
        });
    }

    diagnostics
}

fn environments_summary(environments: &BTreeSet<&str>) -> String {
    if environments.is_empty() {
        "none declared".to_owned()
    } else {
        environments
            .iter()
            .map(|e| format!("`{e}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Compare two URLs by scheme + host (ignoring path, query, port
/// where absent). A declared `url` is the canonical reference; the
/// origin must match its scheme + authority for the CORS layer to
/// recognise it as the same browser origin.
fn same_origin(declared_url: &str, origin: &str) -> bool {
    let canon = |raw: &str| {
        let raw = raw.trim();
        // Strip path / query — keep scheme + authority only.
        let cut = raw
            .find("://")
            .and_then(|idx| {
                let after = &raw[idx + 3..];
                let tail_start = after.find('/').map(|p| idx + 3 + p);
                tail_start.map(|p| raw[..p].to_owned())
            })
            .unwrap_or_else(|| raw.to_owned());
        cut.trim_end_matches('/').to_owned()
    };
    canon(declared_url) == canon(origin)
}

// =============================================================================
// Observability bucket cycle row 36 — `app.logging` + `app.tracing`
//
// Six diagnostics check the closed catalogs and the sample-rate range:
//   - `app_logging_level_invalid_diagnostics`        error
//   - `app_logging_format_invalid_diagnostics`       error
//   - `app_logging_redact_unknown_diagnostics`       error
//   - `app_logging_sample_rate_range_diagnostics`    error
//   - `app_tracing_sample_rate_range_diagnostics`    error
//   - `app_tracing_exporter_unbound_diagnostics`     error
//
// The closed catalogs are deliberately small (4 levels, 2 formats, 2
// redact strategies). New catalog entries require a language cut.
//
// See `docs/proposals/bucket-observability-cycle.md` §3.1 §3.2.
// =============================================================================

/// Closed catalog shared with `event.trace <name> level <level>` in
/// row 37. Mirrors `log/slog` level discipline.
const LOG_LEVEL_CATALOG: &[&str] = &["debug", "info", "warn", "error"];

/// Closed catalog for `app.logging.format`. JSON for production
/// pipelines, text for local development.
const LOG_FORMAT_CATALOG: &[&str] = &["json", "text"];

/// Closed catalog for `app.logging.redact`. `pii` consumes `@pii.*`
/// tags; `none` opts out entirely.
const LOG_REDACT_CATALOG: &[&str] = &["pii", "none"];

fn app_logging_tracing_diagnostics(
    app: Option<&DoctorAppManifest>,
    registry: Option<&ir::AppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let Some(app_manifest) = app else {
        return diagnostics;
    };
    let manifest_path = app_manifest.path.clone();

    if let Some(logging) = app_manifest.manifest.logging.as_ref() {
        if let Some(level) = logging.level.as_deref() {
            if !LOG_LEVEL_CATALOG.contains(&level) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_logging_level_invalid_diagnostics".to_owned(),
                    message: format!(
                        "`app.logging.level {level}` is not in the closed catalog. Allowed values: {}.",
                        catalog_list(LOG_LEVEL_CATALOG),
                    ),
                });
            }
        }
        if let Some(format) = logging.format.as_deref() {
            if !LOG_FORMAT_CATALOG.contains(&format) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_logging_format_invalid_diagnostics".to_owned(),
                    message: format!(
                        "`app.logging.format {format}` is not in the closed catalog. Allowed values: {}.",
                        catalog_list(LOG_FORMAT_CATALOG),
                    ),
                });
            }
        }
        if let Some(redact) = logging.redact.as_deref() {
            if !LOG_REDACT_CATALOG.contains(&redact) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_logging_redact_unknown_diagnostics".to_owned(),
                    message: format!(
                        "`app.logging.redact {redact}` is not in the closed catalog. Allowed values: {}.",
                        catalog_list(LOG_REDACT_CATALOG),
                    ),
                });
            }
        }
        if let Some(rate) = logging.sample_rate {
            if !(0.0..=1.0).contains(&rate) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_logging_sample_rate_range_diagnostics".to_owned(),
                    message: format!(
                        "`app.logging.sample_rate {rate}` must be a float in `[0.0, 1.0]`. Use `1.0` for full capture and `0.0` to disable."
                    ),
                });
            }
        }
    }

    if let Some(tracing) = app_manifest.manifest.tracing.as_ref() {
        if let Some(rate) = tracing.sample_rate {
            if !(0.0..=1.0).contains(&rate) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_tracing_sample_rate_range_diagnostics".to_owned(),
                    message: format!(
                        "`app.tracing.sample_rate {rate}` must be a float in `[0.0, 1.0]`. Use `1.0` for full capture and `0.0` to disable."
                    ),
                });
            }
        }
        if let Some(exporter) = tracing.exporter.as_deref() {
            // The exporter slot must resolve to a `registry.capabilities
            // <name> tracing` entry (declared as the `name`, valued as
            // `tracing`) or to an integration. We accept any
            // `AppCapability` whose value is `tracing` *or* whose name
            // matches the exporter literal.
            let resolved = registry
                .map(|reg| {
                    reg.capabilities
                        .iter()
                        .any(|cap| cap.name == exporter && cap.value == "tracing")
                })
                .unwrap_or(false);
            if !resolved {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_tracing_exporter_unbound_diagnostics".to_owned(),
                    message: format!(
                        "`app.tracing.exporter {exporter}` does not resolve to a `registry.capabilities` entry of kind `tracing`. Declare it in `registry.capabilities`, or remove the line to let the runtime pick a default.",
                    ),
                });
            }
        }
    }

    diagnostics
}

fn catalog_list(items: &[&str]) -> String {
    items
        .iter()
        .map(|i| format!("`{i}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

// -----------------------------------------------------------------------------
// Cut A.9 — `approval` primitive on commands
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CommandApprovalFact {
    feature: String,
    command: String,
    path: PathBuf,
    line: usize,
    by: Vec<String>,
    timeout: Option<String>,
    then: Option<String>,
    required_when_present: bool,
    missing_children: Vec<&'static str>,
}

/// Text-walk every `.lzi` for `command <name>` headers at indent 2,
/// then look for an `approval` indent-4 child and harvest its
/// children (`by`, `timeout`, `then`, `required_when`). The slice
/// captures presence + values; doctor + LSP consume them.
///
/// Lives next to `collect_api_paths`; same approach for the same
/// reason — the canonical-indent parser slice does not yet cover
/// commands, so we use text-pattern with stable column anchors until
/// the Phase L migration arrives.
fn collect_command_approvals(file: &DoctorFile, out: &mut Vec<CommandApprovalFact>) {
    let lines: Vec<&str> = file.source.lines().collect();
    let mut feature: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            feature = trimmed
                .strip_prefix("feature ")
                .map(|n| n.trim().to_owned());
            i += 1;
            continue;
        }
        if leading_spaces(line) == 2 && trimmed.starts_with("command ") {
            let name = trimmed
                .strip_prefix("command ")
                .map(|n| n.split_whitespace().next().unwrap_or("").to_owned())
                .unwrap_or_default();
            let feature_name = feature.clone().unwrap_or_default();

            let mut j = i + 1;
            let mut found_approval_at: Option<usize> = None;
            while j < lines.len() {
                let inner = lines[j];
                let inner_trim = inner.trim_start();
                if inner_trim.is_empty() || inner_trim.starts_with('#') {
                    j += 1;
                    continue;
                }
                if leading_spaces(inner) <= 2 {
                    break;
                }
                if leading_spaces(inner) == 4 && inner_trim == "approval" {
                    found_approval_at = Some(j);
                    break;
                }
                j += 1;
            }

            if let Some(approval_at) = found_approval_at {
                let mut fact = CommandApprovalFact {
                    feature: feature_name,
                    command: name,
                    path: file.path.clone(),
                    line: approval_at + 1,
                    by: Vec::new(),
                    timeout: None,
                    then: None,
                    required_when_present: false,
                    missing_children: Vec::new(),
                };

                let mut k = approval_at + 1;
                while k < lines.len() {
                    let body = lines[k];
                    let body_trim = body.trim_start();
                    if body_trim.is_empty() || body_trim.starts_with('#') {
                        k += 1;
                        continue;
                    }
                    if leading_spaces(body) <= 4 {
                        break;
                    }
                    if leading_spaces(body) == 6 {
                        if let Some(rest) = body_trim.strip_prefix("by ") {
                            fact.by = rest
                                .split(',')
                                .map(|s| s.trim().to_owned())
                                .filter(|s| !s.is_empty())
                                .collect();
                        } else if let Some(rest) = body_trim.strip_prefix("timeout ") {
                            let unquoted = rest
                                .trim()
                                .strip_prefix('"')
                                .and_then(|s| s.strip_suffix('"'))
                                .unwrap_or(rest.trim());
                            fact.timeout = Some(unquoted.to_owned());
                        } else if let Some(rest) = body_trim.strip_prefix("then ") {
                            fact.then = Some(rest.trim().to_owned());
                        } else if body_trim.starts_with("required_when ") {
                            fact.required_when_present = true;
                        }
                    }
                    k += 1;
                }

                // Capture missing children for `approval_contract_diagnostics`.
                if fact.by.is_empty() {
                    fact.missing_children.push("by");
                }
                if fact.timeout.is_none() {
                    fact.missing_children.push("timeout");
                }
                if fact.then.is_none() {
                    fact.missing_children.push("then");
                }

                out.push(fact);
                i = k;
                continue;
            }
        }
        i += 1;
    }
}

/// Doctor-side diagnostics for the `approval` primitive. Three
/// dedicated ids plus the write-tool guard extension; the latter
/// reaches inside `agent_tool_write_unguarded_diagnostics` so write
/// tools whose target command carries `approval` no longer require
/// the agent's `safety` validator.
fn approval_diagnostics(
    approvals: &[CommandApprovalFact],
    known_roles: &BTreeSet<String>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for fact in approvals {
        // Required-children contract.
        if !fact.missing_children.is_empty() {
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "approval_contract_diagnostics".to_owned(),
                message: format!(
                    "command `{}.{}` declares `approval` but is missing required children: {}.",
                    fact.feature,
                    fact.command,
                    fact.missing_children.join(", "),
                ),
            });
            continue;
        }

        // Timeout shape.
        if let Some(timeout) = fact.timeout.as_deref() {
            if !approval_timeout_well_formed(timeout) {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "approval_timeout_invalid_diagnostics".to_owned(),
                    message: format!(
                        "command `{}.{}` declares `approval timeout {:?}` which is not a recognised duration shape (e.g. `\"24h\"`, `\"30 minutes\"`, `\"7d\"`).",
                        fact.feature, fact.command, timeout,
                    ),
                });
            }
        }

        // Closed catalog for `then`.
        if let Some(then) = fact.then.as_deref() {
            if !matches!(then, "deny" | "proceed") {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "approval_contract_diagnostics".to_owned(),
                    message: format!(
                        "command `{}.{}` declares `approval then {then}` — the closed catalog is `deny` or `proceed`.",
                        fact.feature, fact.command,
                    ),
                });
            }
        }

        // Role resolution. `by` entries are `@role.<name>` only.
        for role_ref in &fact.by {
            let Some(suffix) = role_ref.strip_prefix("@role.") else {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "approval_role_unresolved_diagnostics".to_owned(),
                    message: format!(
                        "command `{}.{}` approval `by {role_ref}` is not a `@role.<name>` reference; approvers are roles, not scopes.",
                        fact.feature, fact.command,
                    ),
                });
                continue;
            };
            if !known_roles.contains(suffix) {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "approval_role_unresolved_diagnostics".to_owned(),
                    message: format!(
                        "command `{}.{}` approval `by @role.{suffix}` references a role that no `policies` block or `app.lzi` `policy_for` declares.",
                        fact.feature, fact.command,
                    ),
                });
            }
        }
    }

    diagnostics
}

/// Shape-check a duration string. Accepts `<digits> <unit>` or
/// `<digits><unit>` where unit ∈ s, m, h, d, w, second(s),
/// minute(s), hour(s), day(s), week(s). The runtime adapter parses
/// the canonical form; this layer rejects obviously malformed input.
fn approval_timeout_well_formed(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let mut chars = trimmed.chars();
    let mut saw_digit = false;
    let mut unit_start = 0;
    for (i, c) in chars.by_ref().enumerate() {
        if c.is_ascii_digit() {
            saw_digit = true;
            continue;
        }
        unit_start = i;
        break;
    }
    if !saw_digit {
        return false;
    }
    let unit = trimmed[unit_start..].trim();
    matches!(
        unit,
        "s" | "m"
            | "h"
            | "d"
            | "w"
            | "second"
            | "seconds"
            | "minute"
            | "minutes"
            | "hour"
            | "hours"
            | "day"
            | "days"
            | "week"
            | "weeks"
    )
}

/// Collect every role name declared by a feature's `policies` block
/// (children at indent 4 referencing `@role.<name>`) or by an
/// `app.lzi` `policy_for ...: @role.<name>` default. Used by
/// `approval_role_unresolved_diagnostics`.
///
/// Intentionally scoped: scanning every `@role.X` reference in the
/// file would self-resolve the very `by @role.X` line we're trying
/// to validate. Only first-class declarations count.
fn collect_known_roles(files: &[DoctorFile]) -> BTreeSet<String> {
    let mut roles = BTreeSet::new();
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let lines: Vec<&str> = file.source.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                i += 1;
                continue;
            }
            // Feature-level `policies` block at indent 2.
            if leading_spaces(line) == 2 && trimmed == "policies" {
                let mut j = i + 1;
                while j < lines.len() {
                    let inner = lines[j];
                    let inner_trim = inner.trim_start();
                    if inner_trim.is_empty() || inner_trim.starts_with('#') {
                        j += 1;
                        continue;
                    }
                    if leading_spaces(inner) <= 2 {
                        break;
                    }
                    // `<name>: @role.x, @scope.y, ...` — harvest only
                    // the @role.<name> entries.
                    if let Some((_, refs)) = inner_trim.split_once(':') {
                        extract_role_atoms(refs, &mut roles);
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
            // Top-level `app.lzi` `policy_for <kinds>: @role.x, ...`
            // (or feature-level `policy_for` inside `defaults`).
            if let Some(rest) = trimmed.strip_prefix("policy_for ") {
                if let Some((_, refs)) = rest.split_once(':') {
                    extract_role_atoms(refs, &mut roles);
                }
            }
            i += 1;
        }
    }
    roles
}

fn extract_role_atoms(refs: &str, roles: &mut BTreeSet<String>) {
    for token in refs.split(',') {
        let token = token.trim();
        if let Some(name) = token.strip_prefix("@role.") {
            let end = name
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(name.len());
            if end > 0 {
                roles.insert(name[..end].to_owned());
            }
        }
    }
}

// =============================================================================
// Phase L — auth block cross-feature diagnostics.
//
// Four ids per `docs/proposals/bucket-auth-cycle.md` §Doctor/LSP:
//   - `auth_password_algorithm_hash_mismatch`
//   - `auth_sessions_resource_unknown`
//   - `auth_identity_field_unknown`
//   - `auth_oauth_adapter_unbound`
//
// The lowered `ir::Auth` block arrives via `lower_feature_skeleton`
// (Phase L Tier 1). This module owns the text-pattern collection of
// neighbouring resources + extensions adapter slots and the
// cross-feature lookup that the LSP cannot perform.
// =============================================================================

#[derive(Debug, Default)]
struct AuthAnchors {
    identity_line: usize,
    password_line: Option<usize>,
    password_algorithm_line: Option<usize>,
    sessions_line: Option<usize>,
    sessions_resource_line: Option<usize>,
    mfa_line: Option<usize>,
    oauth_lines: BTreeMap<String, usize>,
}

/// Walk the source under the `auth` block (starting at `auth_line`) and
/// map each subblock onto its 1-based source line. Used to anchor
/// diagnostics at the offending keyword rather than the `auth` header.
fn collect_auth_anchors(source: &str, auth_line: usize) -> AuthAnchors {
    let mut anchors = AuthAnchors {
        identity_line: auth_line,
        ..Default::default()
    };
    let lines: Vec<&str> = source.lines().collect();
    if auth_line == 0 || auth_line > lines.len() {
        return anchors;
    }
    // `auth_line` is 1-based; index = auth_line - 1 points at the
    // `auth` keyword. Body starts the next line.
    let header_index = auth_line - 1;
    let auth_indent = leading_spaces(lines[header_index]);
    let child_indent = auth_indent + 2;
    let grand_indent = auth_indent + 4;

    let mut i = header_index + 1;
    let mut current_password = false;
    let mut current_sessions = false;
    let mut current_mfa = false;
    let mut current_oauth: Option<String> = None;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        let indent = leading_spaces(line);
        if indent <= auth_indent {
            break;
        }
        if indent == child_indent {
            current_password = false;
            current_sessions = false;
            current_mfa = false;
            current_oauth = None;
            if let Some(rest) = trimmed.strip_prefix("identity ") {
                let _ = rest;
                anchors.identity_line = i + 1;
            } else if trimmed == "password" {
                anchors.password_line = Some(i + 1);
                current_password = true;
            } else if trimmed == "sessions" {
                anchors.sessions_line = Some(i + 1);
                current_sessions = true;
            } else if let Some(rest) = trimmed.strip_prefix("mfa ") {
                let _ = rest;
                anchors.mfa_line = Some(i + 1);
                current_mfa = true;
            } else if let Some(rest) = trimmed.strip_prefix("oauth ") {
                let provider = rest.split_whitespace().next().unwrap_or("").to_owned();
                if !provider.is_empty() {
                    anchors.oauth_lines.insert(provider.clone(), i + 1);
                    current_oauth = Some(provider);
                }
            }
        } else if indent == grand_indent {
            if current_password {
                if trimmed.starts_with("algorithm ") {
                    anchors.password_algorithm_line = Some(i + 1);
                }
            } else if current_sessions && trimmed.starts_with("resource ") {
                anchors.sessions_resource_line = Some(i + 1);
            } else if current_mfa || current_oauth.is_some() {
                // body lines for mfa/oauth carry adapter/enroll/verify
                // refs but we don't need per-line anchors today.
            }
        }
        i += 1;
    }
    anchors
}

/// Harvest `resource <Name>` declarations under each `feature <name>`
/// block, recording field name + verbatim type text + trailing
/// modifiers. The walk tolerates the canonical fixture's
/// `domain` (indent 2) and `domain.resource` (indent 4 / fields at 6)
/// shape; resources declared directly under `feature` are also picked
/// up.
fn collect_feature_resources(
    file: &DoctorFile,
    out: &mut BTreeMap<String, BTreeMap<String, ResourceFact>>,
) {
    if !is_lzi_path(&file.path) {
        return;
    }
    let lines: Vec<&str> = file.source.lines().collect();
    let mut feature: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            feature = trimmed
                .strip_prefix("feature ")
                .map(|n| n.trim().to_owned());
            i += 1;
            continue;
        }
        // `resource <Name>` only counts under `domain` (indent 4) or
        // directly under `feature` (indent 2). Other `resource` lines
        // are slot references — e.g. `auth.sessions.resource Session`
        // at indent 6 — and must not be treated as declarations.
        let resource_indent = leading_spaces(line);
        if let Some(rest) = trimmed.strip_prefix("resource ") {
            if resource_indent != 2 && resource_indent != 4 {
                i += 1;
                continue;
            }
            let name = rest.split_whitespace().next().unwrap_or("").to_owned();
            if name.is_empty() {
                i += 1;
                continue;
            }
            let mut fact = ResourceFact {
                path: file.path.clone(),
                line: i + 1,
                fields: BTreeMap::new(),
            };
            let mut j = i + 1;
            while j < lines.len() {
                let inner = lines[j];
                let inner_trim = inner.trim_start();
                if inner_trim.is_empty() || inner_trim.starts_with('#') {
                    j += 1;
                    continue;
                }
                if leading_spaces(inner) <= resource_indent {
                    break;
                }
                if let Some((field_name, field_fact)) = parse_resource_field(inner_trim, j + 1) {
                    fact.fields.insert(field_name, field_fact);
                }
                j += 1;
            }
            if let Some(feature_name) = feature.as_ref() {
                out.entry(feature_name.clone())
                    .or_default()
                    .insert(name, fact);
            }
            i = j;
            continue;
        }
        i += 1;
    }
}

/// Parse `<field>: <Type> [modifiers...]`. The type text is whatever
/// follows the first colon up to the first whitespace (so `@cap.Hashed(
/// algorithm:argon2id)` round-trips intact because the args are
/// parenthesised). Modifiers are the remainder, used to detect
/// `optional` / `required`.
fn parse_resource_field(trimmed: &str, line: usize) -> Option<(String, ResourceFieldFact)> {
    let (name_part, rest) = trimmed.split_once(':')?;
    let name = name_part.trim();
    if name.is_empty() {
        return None;
    }
    let rest = rest.trim();
    // Split into type + modifiers honouring parenthesised arg lists.
    let mut depth = 0i32;
    let mut split_at = rest.len();
    for (idx, ch) in rest.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            c if c.is_whitespace() && depth == 0 => {
                split_at = idx;
                break;
            }
            _ => {}
        }
    }
    let type_text = rest[..split_at].to_owned();
    let modifiers = rest[split_at..].trim().to_owned();
    if type_text.is_empty() {
        return None;
    }
    Some((
        name.to_owned(),
        ResourceFieldFact {
            type_text,
            modifiers,
            line,
        },
    ))
}

/// Harvest each feature's `extensions adapter <name>: <Type> at "..."`
/// declarations. Only the local name is stored; the type contract is
/// checked elsewhere.
fn collect_feature_adapters(file: &DoctorFile, out: &mut BTreeMap<String, BTreeSet<String>>) {
    if !is_lzi_path(&file.path) {
        return;
    }
    let lines: Vec<&str> = file.source.lines().collect();
    let mut feature: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            feature = trimmed
                .strip_prefix("feature ")
                .map(|n| n.trim().to_owned());
            i += 1;
            continue;
        }
        if leading_spaces(line) == 2 && trimmed == "extensions" {
            let mut j = i + 1;
            while j < lines.len() {
                let inner = lines[j];
                let inner_trim = inner.trim_start();
                if inner_trim.is_empty() || inner_trim.starts_with('#') {
                    j += 1;
                    continue;
                }
                if leading_spaces(inner) <= 2 {
                    break;
                }
                if let Some(rest) = inner_trim.strip_prefix("adapter ") {
                    let name_segment = rest.split([':', ' ']).next().unwrap_or("").trim();
                    if !name_segment.is_empty() {
                        if let Some(feature_name) = feature.as_ref() {
                            out.entry(feature_name.clone())
                                .or_default()
                                .insert(name_segment.to_owned());
                        }
                    }
                }
                j += 1;
            }
            i = j;
            continue;
        }
        i += 1;
    }
}

/// Harvest each feature's `uses <feature>, <feature>, ...` declarations.
/// Cross-feature resource resolution (e.g. `auth identity Customer.email`
/// in `customer_auth uses customer`) reads this map.
fn collect_feature_uses(file: &DoctorFile, out: &mut BTreeMap<String, BTreeSet<String>>) {
    if !is_lzi_path(&file.path) {
        return;
    }
    let lines: Vec<&str> = file.source.lines().collect();
    let mut feature: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            feature = trimmed
                .strip_prefix("feature ")
                .map(|n| n.trim().to_owned());
            i += 1;
            continue;
        }
        if leading_spaces(line) == 2 && trimmed.starts_with("uses ") {
            if let Some(rest) = trimmed.strip_prefix("uses ") {
                if let Some(feature_name) = feature.as_ref() {
                    let entry = out.entry(feature_name.clone()).or_default();
                    for token in rest.split(',') {
                        let name = token.trim();
                        if !name.is_empty() {
                            entry.insert(name.to_owned());
                        }
                    }
                }
            }
        }
        i += 1;
    }
}

/// Read the `algorithm:<X>` axis out of a `@cap.Hashed(...)` type text.
/// Returns `None` when the field is not a `@cap.Hashed(...)` decorator
/// or omits the axis.
fn cap_hashed_algorithm(type_text: &str) -> Option<&str> {
    let rest = type_text.strip_prefix("@cap.Hashed(")?;
    let args = rest.strip_suffix(')')?;
    for part in args.split(',') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("algorithm:") {
            return Some(value.trim());
        }
    }
    None
}

/// Heuristic: is this field's declared shape a plausible login
/// identifier? Identity fields are unique-shaped — either tagged with
/// `@semantic.Email` / `@semantic.Phone`, declared as an `ID`, or
/// carry a `unique` modifier. The check is conservative; rejected
/// shapes are obvious authoring errors (e.g. a `Text` free-form note
/// field used as the login identity).
fn is_identity_shaped(field: &ResourceFieldFact) -> bool {
    let type_text = field.type_text.as_str();
    if type_text.starts_with("@semantic.Email") || type_text.starts_with("@semantic.Phone") {
        return true;
    }
    if type_text == "ID" {
        return true;
    }
    field.modifiers.contains("unique")
}

/// Resolve `<Resource>` for a feature by searching its own resources
/// first, then falling back to resources declared in features it
/// `uses`. Returns the first hit.
fn resolve_resource_for_feature<'a>(
    feature: &str,
    resource_name: &str,
    feature_resources: &'a BTreeMap<String, BTreeMap<String, ResourceFact>>,
    feature_uses: &BTreeMap<String, BTreeSet<String>>,
) -> Option<&'a ResourceFact> {
    if let Some(local) = feature_resources
        .get(feature)
        .and_then(|m| m.get(resource_name))
    {
        return Some(local);
    }
    if let Some(uses) = feature_uses.get(feature) {
        for dep in uses {
            if let Some(hit) = feature_resources
                .get(dep)
                .and_then(|m| m.get(resource_name))
            {
                return Some(hit);
            }
        }
    }
    None
}

/// Emit the four `auth_*` cross-feature diagnostics. Each diagnostic
/// is anchored at the offending subblock line; the `auth` header is
/// only used as a fallback.
fn auth_diagnostics(
    auth_facts: &[AuthFacts],
    feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
    feature_adapters: &BTreeMap<String, BTreeSet<String>>,
    feature_uses: &BTreeMap<String, BTreeSet<String>>,
    registry: Option<&DoctorAppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let registry_integrations: BTreeSet<String> = registry
        .map(|r| {
            r.manifest
                .integrations
                .iter()
                .map(|i| i.name.clone())
                .collect()
        })
        .unwrap_or_default();

    for fact in auth_facts {
        let feature = fact.feature.as_str();

        // 1. `auth_identity_field_unknown` — resource and field must
        //    resolve in the same feature (or one it `uses`), and the
        //    field must be identity-shaped.
        let identity_resource = fact.auth.identity.field.resource.name.as_str();
        let identity_field = fact.auth.identity.field.field.as_str();
        let identity_resource_fact = resolve_resource_for_feature(
            feature,
            identity_resource,
            feature_resources,
            feature_uses,
        );
        match identity_resource_fact {
            None => diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.identity_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "auth_identity_field_unknown".to_owned(),
                message: format!(
                    "auth.identity `{identity_resource}.{identity_field}` does not resolve: resource not found in feature `{feature}`.",
                ),
            }),
            Some(resource) => match resource.fields.get(identity_field) {
                None => diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.identity_line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "auth_identity_field_unknown".to_owned(),
                    message: format!(
                        "auth.identity `{identity_resource}.{identity_field}` does not resolve: field not found on `{identity_resource}`.",
                    ),
                }),
                Some(field) => {
                    if !is_identity_shaped(field) {
                        diagnostics.push(DoctorDiagnostic {
                            path: fact.path.clone(),
                            line: fact.identity_line,
                            column: 1,
                            severity: DoctorSeverity::Error,
                            code: "auth_identity_field_unknown".to_owned(),
                            message: format!(
                                "auth.identity `{identity_resource}.{identity_field}` does not resolve: field is not identity-shaped (missing @semantic.Email / @semantic.Phone / unique).",
                            ),
                        });
                    }
                }
            },
        }

        // 2. `auth_sessions_resource_unknown` — sessions resource must
        //    resolve in the same feature (or one it `uses`).
        if let Some(sessions) = fact.auth.sessions.as_ref() {
            let sessions_name = sessions.resource.name.as_str();
            let resolved = resolve_resource_for_feature(
                feature,
                sessions_name,
                feature_resources,
                feature_uses,
            );
            if resolved.is_none() {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.sessions_resource_line.unwrap_or(fact.line),
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "auth_sessions_resource_unknown".to_owned(),
                    message: format!(
                        "auth.sessions.resource `{sessions_name}` does not name a resource declared in feature `{feature}`.",
                    ),
                });
            }
        }

        // 3. `auth_password_algorithm_hash_mismatch` — when both
        //    `auth.password.algorithm` and the session resource carry
        //    a `@cap.Hashed(algorithm:…)` field, the two axes must
        //    match.
        if let (Some(password), Some(sessions)) =
            (fact.auth.password.as_ref(), fact.auth.sessions.as_ref())
        {
            let pw_algo = password.algorithm.trim();
            if !pw_algo.is_empty() {
                let sessions_name = sessions.resource.name.as_str();
                if let Some(resource) = resolve_resource_for_feature(
                    feature,
                    sessions_name,
                    feature_resources,
                    feature_uses,
                ) {
                    // Find the first hash-shaped field on the session
                    // resource that carries a `@cap.Hashed(...)`
                    // decorator. Multiple is allowed; we pin the
                    // first divergence.
                    let mut found_hash_axis = None;
                    for (field_name, field) in &resource.fields {
                        if let Some(axis) = cap_hashed_algorithm(&field.type_text) {
                            found_hash_axis = Some((field_name.clone(), axis.to_owned()));
                            if axis != pw_algo {
                                diagnostics.push(DoctorDiagnostic {
                                    path: fact.path.clone(),
                                    line: fact
                                        .password_algorithm_line
                                        .unwrap_or(fact.password_line.unwrap_or(fact.line)),
                                    column: 1,
                                    severity: DoctorSeverity::Error,
                                    code: "auth_password_algorithm_hash_mismatch".to_owned(),
                                    message: format!(
                                        "auth.password.algorithm `{pw_algo}` must match `@cap.Hashed(algorithm:{pw_algo})` on the session resource's hash field (found `{axis}` on `{sessions_name}.{field_name}`).",
                                        pw_algo = pw_algo,
                                        axis = axis,
                                        sessions_name = sessions_name,
                                        field_name = field_name,
                                    ),
                                });
                                break;
                            }
                        }
                    }
                    let _ = found_hash_axis;
                }
            }
        }

        // 4. `auth_oauth_adapter_unbound` — each oauth provider's
        //    adapter must resolve in the feature's `extensions
        //    adapter <name>` list or `registry.integrations`.
        let feature_adapter_names = feature_adapters.get(feature);
        for provider in &fact.auth.oauth {
            let adapter_ref = provider.adapter.as_str();
            let local_name = adapter_ref.strip_prefix("@adapter.").unwrap_or("");
            let in_feature = !local_name.is_empty()
                && feature_adapter_names
                    .map(|s| s.contains(local_name))
                    .unwrap_or(false);
            let in_registry = !local_name.is_empty() && registry_integrations.contains(local_name);
            if !in_feature && !in_registry {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact
                        .oauth_lines
                        .get(provider.provider.as_str())
                        .copied()
                        .unwrap_or(fact.line),
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "auth_oauth_adapter_unbound".to_owned(),
                    message: format!(
                        "auth.oauth.`{provider}`.adapter `{adapter_ref}` is not declared in `extensions` of feature `{feature}` or `integrations` in `registry.lzi`.",
                        provider = provider.provider,
                        adapter_ref = adapter_ref,
                        feature = feature,
                    ),
                });
            }
        }
    }
    diagnostics
}

// -----------------------------------------------------------------------------
// Cut A.8 — built-in trace event diagnostics
//
// `agent_run` is registered by the IR as a built-in trace event. The
// language reserves the name (authored `event.trace agent_run` is
// rejected) and validates subscriber jobs against the canonical
// payload schema so a job referencing a non-existent field doesn't
// fail silently at runtime.
//
// See `docs/proposals/ai-primitives-cut-a-8.md`.
// -----------------------------------------------------------------------------

fn agent_run_trace_diagnostics(files: &[DoctorFile]) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    let canonical_payload: BTreeSet<String> = ir::built_in_trace_events()
        .into_iter()
        .find(|e| e.name == "agent_run")
        .map(|e| e.payload.iter().map(|f| f.name.clone()).collect())
        .unwrap_or_default();

    // Observability bucket cycle row 35 — pre-compute the set of
    // built-in trace event names once per check so `trigger
    // @trace.<X>` and `trigger event.trace <X>` resolution both
    // consult the same registry. Authored `event.trace <name>`
    // declarations in scope are gathered per file below.
    let built_in_names: BTreeSet<String> = ir::built_in_trace_events()
        .into_iter()
        .map(|e| e.name)
        .collect();

    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let lines: Vec<&str> = file.source.lines().collect();

        // Observability bucket cycle row 35 — collect authored
        // `event.trace <name>` declarations *in this file* so
        // `trigger_trace_unknown` doesn't false-positive on
        // legitimate subscriber references to authored events.
        let authored_trace_names: BTreeSet<String> = lines
            .iter()
            .filter_map(|line| {
                let trimmed = line.trim_start();
                if trimmed.starts_with('#') || trimmed.is_empty() {
                    return None;
                }
                trimmed
                    .strip_prefix("event.trace ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_owned)
            })
            .collect();

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                i += 1;
                continue;
            }

            // Reserved-name: `event.trace <name>` where <name> is a
            // built-in. Reject the authored declaration.
            if let Some(rest) = trimmed.strip_prefix("event.trace ") {
                let name = rest.split_whitespace().next().unwrap_or("");
                if ir::is_reserved_trace_event_name(name) {
                    diagnostics.push(DoctorDiagnostic {
                        path: file.path.clone(),
                        line: i + 1,
                        column: leading_spaces(line) + 1,
                        severity: DoctorSeverity::Error,
                        code: "event_trace_reserved_name_diagnostics".to_owned(),
                        message: format!(
                            "`event.trace {name}` is reserved by the IR as a built-in trace event; the runtime emits it automatically. Authoring this declaration is rejected — remove the block and subscribe via `job ... trigger event.trace {name}` instead."
                        ),
                    });
                }
            }

            // Payload-drift: `trigger event.trace agent_run` inside a
            // job, followed by `payload <field>` references at deeper
            // indent. Reject any field not in the canonical schema.
            if trimmed.starts_with("trigger event.trace ") {
                let name = trimmed
                    .strip_prefix("trigger event.trace ")
                    .map(|n| n.trim().to_owned())
                    .unwrap_or_default();
                if canonical_payload_event(&name, &canonical_payload) {
                    diagnostics.extend(scan_payload_field_drift(
                        file,
                        &lines,
                        i,
                        &name,
                        &canonical_payload,
                    ));
                }
            }

            // Observability bucket cycle row 35 — `trigger_trace_unknown`.
            // The `@trace.<name>` namespace and the bare-form
            // `trigger event.trace <name>` both have to resolve to a
            // built-in trace event or an authored `event.trace <name>`
            // in the same file. We catch the failure here so a typo
            // doesn't fall through to runtime as "subscriber wired to
            // an event that nobody emits."
            let trace_ref = trimmed
                .strip_prefix("trigger @trace.")
                .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned())
                .or_else(|| {
                    trimmed
                        .strip_prefix("trigger event.trace ")
                        .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned())
                });
            if let Some(name) = trace_ref {
                if !name.is_empty()
                    && !built_in_names.contains(&name)
                    && !authored_trace_names.contains(&name)
                {
                    let mut known: Vec<String> = built_in_names.iter().cloned().collect();
                    known.extend(authored_trace_names.iter().cloned());
                    diagnostics.push(DoctorDiagnostic {
                        path: file.path.clone(),
                        line: i + 1,
                        column: leading_spaces(line) + 1,
                        severity: DoctorSeverity::Error,
                        code: "trigger_trace_unknown_diagnostics".to_owned(),
                        message: format!(
                            "`trigger @trace.{name}` does not resolve. Built-in trace events: {}. Authored trace events in scope: {}.",
                            format_name_list(&built_in_names),
                            format_name_list(&authored_trace_names),
                        ),
                    });
                }
            }

            i += 1;
        }
    }

    diagnostics
}

/// Render a `{name1, name2, ...}`-style list for diagnostic messages.
/// Empty sets render as `<none>` so the message stays unambiguous.
fn format_name_list(names: &BTreeSet<String>) -> String {
    if names.is_empty() {
        "<none>".to_owned()
    } else {
        names
            .iter()
            .map(|n| format!("`{n}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn canonical_payload_event(name: &str, canonical: &BTreeSet<String>) -> bool {
    !canonical.is_empty() && ir::is_reserved_trace_event_name(name)
}

/// After spotting `trigger event.trace agent_run`, walk subsequent
/// lines at deeper indent and flag any `<field> = <expr>` reference
/// where `<field>` is not part of the canonical payload. The check is
/// scoped to the job block that owns the trigger — we stop at the
/// next sibling at the same or shallower indent.
fn scan_payload_field_drift(
    file: &DoctorFile,
    lines: &[&str],
    trigger_line: usize,
    event_name: &str,
    canonical: &BTreeSet<String>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let trigger_indent = leading_spaces(lines[trigger_line]);
    let mut i = trigger_line + 1;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        let leading = leading_spaces(line);
        // Stop at the next sibling-or-shallower line. Trigger's
        // subscriber body is everything deeper than the trigger.
        if leading <= trigger_indent {
            break;
        }
        // Lines like `tokens_input = payload.tokens_input` or
        // `<field>: <expr>` reference a payload field on the LHS.
        if let Some(field) = trimmed
            .split_once('=')
            .or_else(|| trimmed.split_once(':'))
            .map(|(lhs, _)| lhs.trim())
        {
            // The LHS may include a dotted prefix (e.g.
            // `payload.tokens_input`). Strip the leading segment if
            // it's `payload`.
            let candidate = field
                .strip_prefix("payload.")
                .unwrap_or(field)
                .split_whitespace()
                .next()
                .unwrap_or("");
            if !candidate.is_empty()
                && candidate
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !canonical.contains(candidate)
            {
                // Surface as drift only if the LHS resembles a
                // field reference (lowercase ident); avoid false
                // positives on full expressions.
                if candidate
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_lowercase())
                {
                    diagnostics.push(DoctorDiagnostic {
                        path: file.path.clone(),
                        line: i + 1,
                        column: leading + 1,
                        severity: DoctorSeverity::Error,
                        code: "agent_run_subscriber_payload_drift_diagnostics".to_owned(),
                        message: format!(
                            "subscriber references `{candidate}` but `agent_run`'s canonical payload does not declare it. Valid fields: {}.",
                            payload_field_list(canonical),
                        ),
                    });
                    let _ = event_name; // pin for future per-event errors
                }
            }
        }
        i += 1;
    }
    diagnostics
}

fn payload_field_list(canonical: &BTreeSet<String>) -> String {
    let mut fields: Vec<&String> = canonical.iter().collect();
    fields.sort();
    fields
        .iter()
        .map(|f| format!("`{f}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

// ============================================================================
// Row 30 — Storage bucket cycle diagnostics
//
// Five typed `@cap.File(...)` checks run against `OperationalFacts.
// file_capability_facts`, populated by `collect_file_capability_facts`.
// Codes:
//   - `cap_file_visibility_undeclared`              error
//   - `cap_file_accept_input_output_mismatch`       error
//   - `cap_file_visibility_signed_ttl_mismatch`     error
//   - `cap_file_size_unit_invalid`                  error
//   - `cap_file_mime_family_unknown`                warning
//
// See `docs/proposals/bucket-storage-cycle.md` §Doctor/LSP.
// ============================================================================

/// IANA top-level MIME families recognised by Lazuli's `@cap.File(accept:)`
/// closed catalog. Subtype `*` and family `*` are also accepted at the
/// shape level, but emitted under the wildcard match.
const KNOWN_MIME_FAMILIES: &[&str] = &[
    "text",
    "image",
    "application",
    "audio",
    "video",
    "font",
    "*",
];

fn collect_file_capability_facts(
    file: &DoctorFile,
    lines: &[&str],
    operational: &mut OperationalFacts,
) {
    if !is_lzi_path(&file.path) {
        return;
    }

    let mut current_feature: Option<String> = None;
    let mut current_resource: Option<(String, usize)> = None;
    let mut current_api: Option<(String, usize)> = None;

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        let line_number = index + 1;
        let column = indent + 1;

        // Top-level feature header anchors all enclosed sites.
        if indent == 0 && trimmed.starts_with("feature ") {
            current_feature = trimmed.split_whitespace().nth(1).map(str::to_owned);
            current_resource = None;
            current_api = None;
            continue;
        }

        // Resource and api headers; close on any line that retreats to
        // the header indent or shallower (matching `inspect_storage_projection`).
        if let Some(rest) = trimmed.strip_prefix("resource ") {
            current_resource = Some((
                rest.split_whitespace().next().unwrap_or("").to_owned(),
                indent,
            ));
            current_api = None;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("api ") {
            current_api = Some((
                rest.split_whitespace().next().unwrap_or("").to_owned(),
                indent,
            ));
            current_resource = None;
            continue;
        }
        if let Some((_, header_indent)) = &current_resource {
            if indent <= *header_indent {
                current_resource = None;
            }
        }
        if let Some((_, header_indent)) = &current_api {
            if indent <= *header_indent {
                current_api = None;
            }
        }

        let Some(feature) = current_feature.as_deref() else {
            continue;
        };

        // Resource field shape: `<field>: @cap.File(...)`.
        if let Some((resource, _)) = &current_resource {
            if let Some((field_name, cap_text)) = extract_cap_file_field_line(trimmed) {
                if let lazuli_ir::TypeRef::Capability(lazuli_ir::CapabilityRef::File(file_cap)) =
                    lazuli_analyzer::type_ref_from_syntax_public(&cap_text)
                {
                    operational.file_capability_facts.push(FileCapabilityFact {
                        path: file.path.clone(),
                        line: line_number,
                        column,
                        feature: feature.to_owned(),
                        binding: FileCapabilityBinding::ResourceField {
                            resource: resource.clone(),
                            field: field_name,
                        },
                        capability: file_cap,
                    });
                }
            }
        }

        // Api output shape: `output @cap.File(...)`.
        if let Some((api, _)) = &current_api {
            if let Some(rest) = trimmed.strip_prefix("output ") {
                let rest = rest.trim();
                if rest.starts_with("@cap.File(") {
                    if let Some(close) = rest.find(')') {
                        let cap_text = &rest[..=close];
                        if let lazuli_ir::TypeRef::Capability(lazuli_ir::CapabilityRef::File(
                            file_cap,
                        )) = lazuli_analyzer::type_ref_from_syntax_public(cap_text)
                        {
                            operational.file_capability_facts.push(FileCapabilityFact {
                                path: file.path.clone(),
                                line: line_number,
                                column,
                                feature: feature.to_owned(),
                                binding: FileCapabilityBinding::ApiOutput { api: api.clone() },
                                capability: file_cap,
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Extract `(field_name, "@cap.File(...)")` from a resource-field line.
/// Mirrors `crates/lazuli_cli/src/main.rs:extract_cap_file_field` but is
/// re-implemented here to keep the doctor crate's dependency surface
/// unchanged (no new pub item needed).
fn extract_cap_file_field_line(trimmed: &str) -> Option<(String, String)> {
    let (name_part, type_part) = trimmed.split_once(':')?;
    let name = name_part.trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    let type_tokens = type_part.trim();
    let cap_start = type_tokens.find("@cap.File(")?;
    let from_cap = &type_tokens[cap_start..];
    let close = from_cap.find(')')?;
    let cap_text = &from_cap[..=close];
    Some((name.to_owned(), cap_text.to_owned()))
}

fn cap_file_storage_diagnostics(operational: &OperationalFacts) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // (1) cap_file_visibility_undeclared — api output without `visibility:`.
    // (3) cap_file_visibility_signed_ttl_mismatch — visibility/signed_ttl
    //     coherence (api outputs and resource fields both).
    // (5) cap_file_mime_family_unknown — MIME family outside the IANA
    //     closed catalog.
    for fact in &operational.file_capability_facts {
        if matches!(fact.binding, FileCapabilityBinding::ApiOutput { .. })
            && fact.capability.visibility.is_none()
        {
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.line,
                column: fact.column,
                severity: DoctorSeverity::Error,
                code: "cap_file_visibility_undeclared".to_owned(),
                message: format!(
                    "api `{}` output declares `@cap.File(...)` without `visibility:`; ambiguous visibility on a file URL is a security contract gap. Declare `visibility:` as `public`, `private`, or `signed`.",
                    match &fact.binding {
                        FileCapabilityBinding::ApiOutput { api } => api.as_str(),
                        _ => "<unknown>",
                    }
                ),
            });
        }

        match (
            fact.capability.visibility,
            fact.capability.signed_ttl.as_deref(),
        ) {
            (Some(lazuli_ir::FileVisibility::Signed), None) => {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.line,
                    column: fact.column,
                    severity: DoctorSeverity::Error,
                    code: "cap_file_visibility_signed_ttl_mismatch".to_owned(),
                    message:
                        "`@cap.File(visibility:signed)` requires `signed_ttl:<duration>` (e.g. `1h`); signed URLs without a TTL contract leak forever."
                            .to_owned(),
                });
            }
            (Some(other), Some(_)) if !matches!(other, lazuli_ir::FileVisibility::Signed) => {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.line,
                    column: fact.column,
                    severity: DoctorSeverity::Error,
                    code: "cap_file_visibility_signed_ttl_mismatch".to_owned(),
                    message: format!(
                        "`@cap.File(visibility:{})` forbids `signed_ttl`; `signed_ttl` only applies when `visibility:signed`.",
                        format_visibility(other),
                    ),
                });
            }
            _ => {}
        }

        for mime in &fact.capability.accept {
            if !KNOWN_MIME_FAMILIES.contains(&mime.family.as_str()) {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.line,
                    column: fact.column,
                    severity: DoctorSeverity::Warning,
                    code: "cap_file_mime_family_unknown".to_owned(),
                    message: format!(
                        "`@cap.File(accept:{}/{}` uses unknown MIME family `{}`; known families: {}.",
                        mime.family,
                        mime.subtype,
                        mime.family,
                        KNOWN_MIME_FAMILIES.join(", "),
                    ),
                });
            }
        }
    }

    // (4) cap_file_size_unit_invalid — typed promotion. The IR rejects
    //     unknown units at parse time (the analyzer falls through to
    //     `UserDefined`), so any line that matched `@cap.File(...)`
    //     literally but did NOT produce a typed `FileCapability` fact
    //     is the candidate. We re-walk operational.file_capabilities
    //     (the text-pattern facts) and cross-reference; sites that
    //     have NO typed fact for the same path:line are typing
    //     failures — promote with a typed error.
    for text_fact in &operational.file_capabilities {
        let has_typed = operational
            .file_capability_facts
            .iter()
            .any(|tf| tf.path == text_fact.path && tf.line == text_fact.line);
        if !has_typed {
            diagnostics.push(DoctorDiagnostic {
                path: text_fact.path.clone(),
                line: text_fact.line,
                column: text_fact.column,
                severity: DoctorSeverity::Error,
                code: "cap_file_size_unit_invalid".to_owned(),
                message:
                    "`@cap.File(max_size:<literal>)` must use a positive integer with unit `kb`, `mb`, or `gb`; the surrounding `@cap.File(...)` shape did not lower to typed IR."
                        .to_owned(),
            });
        }
    }

    // (2) cap_file_accept_input_output_mismatch — per-feature, pair
    //     resource-field `@cap.File` inputs with api-output `@cap.File`
    //     outputs and require the accept sets to intersect.
    let mut by_feature: BTreeMap<&str, (Vec<&FileCapabilityFact>, Vec<&FileCapabilityFact>)> =
        BTreeMap::new();
    for fact in &operational.file_capability_facts {
        let entry = by_feature.entry(fact.feature.as_str()).or_default();
        match fact.binding {
            FileCapabilityBinding::ResourceField { .. } => entry.0.push(fact),
            FileCapabilityBinding::ApiOutput { .. } => entry.1.push(fact),
        }
    }
    for (_, (inputs, outputs)) in by_feature {
        if inputs.is_empty() || outputs.is_empty() {
            continue;
        }
        for output in &outputs {
            for input in &inputs {
                if !mime_sets_intersect(&output.capability.accept, &input.capability.accept) {
                    let api_name = match &output.binding {
                        FileCapabilityBinding::ApiOutput { api } => api.as_str(),
                        _ => "<unknown>",
                    };
                    let (resource_name, field_name) = match &input.binding {
                        FileCapabilityBinding::ResourceField { resource, field } => {
                            (resource.as_str(), field.as_str())
                        }
                        _ => ("<unknown>", "<unknown>"),
                    };
                    diagnostics.push(DoctorDiagnostic {
                        path: output.path.clone(),
                        line: output.line,
                        column: output.column,
                        severity: DoctorSeverity::Error,
                        code: "cap_file_accept_input_output_mismatch".to_owned(),
                        message: format!(
                            "api `{api_name}` output declares `@cap.File(accept:{})` but resource `{resource_name}.{field_name}` declares `@cap.File(accept:{})`; accept lists must intersect for the round-trip to be valid.",
                            format_accept_list(&output.capability.accept),
                            format_accept_list(&input.capability.accept),
                        ),
                    });
                }
            }
        }
    }

    diagnostics
}

fn mime_sets_intersect(left: &[lazuli_ir::MimeType], right: &[lazuli_ir::MimeType]) -> bool {
    for l in left {
        for r in right {
            if mime_matches(l, r) {
                return true;
            }
        }
    }
    false
}

fn mime_matches(left: &lazuli_ir::MimeType, right: &lazuli_ir::MimeType) -> bool {
    let family_ok = left.family == right.family || left.family == "*" || right.family == "*";
    let subtype_ok = left.subtype == right.subtype || left.subtype == "*" || right.subtype == "*";
    family_ok && subtype_ok
}

fn format_visibility(v: lazuli_ir::FileVisibility) -> &'static str {
    match v {
        lazuli_ir::FileVisibility::Public => "public",
        lazuli_ir::FileVisibility::Private => "private",
        lazuli_ir::FileVisibility::Signed => "signed",
    }
}

fn format_accept_list(accept: &[lazuli_ir::MimeType]) -> String {
    accept
        .iter()
        .map(|m| format!("{}/{}", m.family, m.subtype))
        .collect::<Vec<_>>()
        .join("|")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package_from_sources(sources: Vec<(&str, &str)>) -> DoctorPackage {
        let mut files = Vec::new();
        let mut workspace = None;
        let mut contracts = Vec::new();
        let mut app = None;
        let mut registry = None;
        let mut profiles = Vec::new();
        let mut commands = BTreeMap::new();
        let mut experiences = BTreeMap::new();
        let mut operational = OperationalFacts::default();
        let mut agents: Vec<AgentFacts> = Vec::new();
        let mut feature_symbols: BTreeMap<String, FeatureSymbols> = BTreeMap::new();
        let mut registry_tool_defects: Vec<RegistryToolDefect> = Vec::new();
        let mut api_paths: Vec<ApiPathFact> = Vec::new();
        let mut command_approvals: Vec<CommandApprovalFact> = Vec::new();
        let mut auth_facts: Vec<AuthFacts> = Vec::new();
        let mut feature_resources: BTreeMap<String, BTreeMap<String, ResourceFact>> =
            BTreeMap::new();
        let mut feature_adapters: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut feature_uses: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

        for (path, source) in sources {
            let mut file = DoctorFile {
                path: PathBuf::from(path),
                source: source.to_owned(),
                local_diagnostics: Vec::new(),
                lzx: None,
            };

            if path.ends_with(".lzi") {
                contracts.extend(
                    parse_app_contracts(&file.source)
                        .into_iter()
                        .map(|manifest| DoctorAppContract {
                            path: file.path.clone(),
                            manifest,
                        }),
                );
                if let Some(manifest) = parse_app_workspace(&file.source) {
                    workspace = Some(DoctorAppWorkspace {
                        path: file.path.clone(),
                        manifest,
                    });
                }
                if let Some(manifest) = parse_app_manifest(&file.source) {
                    app = Some(DoctorAppManifest {
                        path: file.path.clone(),
                        manifest,
                    });
                }
                let RegistryParseOutput {
                    registry: parsed_registry,
                    tool_defects,
                } = parse_app_registry_with_defects(&file.source);
                if let Some(manifest) = parsed_registry {
                    registry = Some(DoctorAppRegistry {
                        path: file.path.clone(),
                        manifest,
                    });
                }
                registry_tool_defects.extend(tool_defects.into_iter().map(|defect| {
                    RegistryToolDefect {
                        path: file.path.clone(),
                        line: defect.line,
                        name: defect.name,
                        reason: defect.reason,
                    }
                }));
                profiles.extend(parse_app_profiles(&file.source).into_iter().map(|profile| {
                    DoctorAppProfile {
                        path: file.path.clone(),
                        profile,
                    }
                }));
                collect_canonical_facts(&file, &mut commands, &mut operational);

                // Cut A — typed agent + feature-symbol collection.
                if let Ok(features) = parse_feature_skeletons(&file.source) {
                    for skeleton in &features {
                        if let Ok(feature) = lower_feature_skeleton(skeleton) {
                            let header_line =
                                line_col_for_offset(&file.source, skeleton.span.start).0;
                            for agent in feature.agents {
                                let agent_line = agent
                                    .span_ref
                                    .as_ref()
                                    .map(|s| line_col_for_offset(&file.source, s.start).0)
                                    .unwrap_or(header_line);
                                agents.push(AgentFacts {
                                    feature: feature.name.clone(),
                                    agent,
                                    path: file.path.clone(),
                                    line: agent_line,
                                });
                            }
                            if let Some(auth) = feature.auth {
                                let auth_line = auth
                                    .span_ref
                                    .as_ref()
                                    .map(|s| line_col_for_offset(&file.source, s.start).0)
                                    .unwrap_or(header_line);
                                let anchors = collect_auth_anchors(&file.source, auth_line);
                                auth_facts.push(AuthFacts {
                                    feature: feature.name.clone(),
                                    auth,
                                    path: file.path.clone(),
                                    line: auth_line,
                                    identity_line: anchors.identity_line,
                                    password_line: anchors.password_line,
                                    password_algorithm_line: anchors.password_algorithm_line,
                                    sessions_line: anchors.sessions_line,
                                    sessions_resource_line: anchors.sessions_resource_line,
                                    mfa_line: anchors.mfa_line,
                                    oauth_lines: anchors.oauth_lines,
                                });
                            }
                        }
                    }
                }
                collect_feature_symbols(&file, &mut feature_symbols);
                collect_api_paths(&file, &mut api_paths);
                collect_command_approvals(&file, &mut command_approvals);
                collect_feature_resources(&file, &mut feature_resources);
                collect_feature_adapters(&file, &mut feature_adapters);
                collect_feature_uses(&file, &mut feature_uses);
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
            workspace,
            contracts,
            app,
            registry,
            profiles,
            commands,
            experiences,
            operational,
            agents,
            feature_symbols,
            registry_tool_defects,
            api_paths,
            command_approvals,
            auth_facts,
            feature_resources,
            feature_adapters,
            feature_uses,
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
    output @cap.File(max_size:10mb,accept:text/csv,visibility:signed,signed_ttl:1h)
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
    fn doctor_rejects_unknown_auth_failed_redirect_route() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  auth_failed_redirect public_login
  not_found public_not_found
  uses
    customer
  targets
    web react
  environments
    production
  urls
    web production "https://app.acme.example"
  runtime
    unit web
      serves surfaces web
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "customer.lzi",
                r#"
feature customer
"#,
            ),
            (
                "app.lzx",
                r#"
route public_login
  path "/login"
  to customer.view.login
  surface customer web
  audience public
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(
            !codes.contains("APP-ROUTE-001"),
            "did not expect APP-ROUTE-001 for declared route, got: {diagnostics:#?}",
        );
        assert!(
            codes.contains("APP-ROUTE-002"),
            "expected APP-ROUTE-002 for missing not_found route, got: {diagnostics:#?}",
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
    fn doctor_resolves_features_and_requirements_from_enabled_packs() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  uses
    payments
  packs
    payments from registry.packs.payments
  bindings
    payments.gateway = registry.integrations.mercadopago
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
  packs
    payments from @runtime/payments
      version "0.1.0"
      provides feature payments
      requires integration gateway: PaymentGateway
"#,
            ),
        ]);

        assert!(
            package.diagnostics().is_empty(),
            "expected enabled pack to satisfy uses and binding contracts"
        );
    }

    #[test]
    fn doctor_reports_unknown_enabled_pack() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  uses
    payments
  packs
    payments from registry.packs.payments
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
        )]);

        let diagnostics = package.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("APP-PACK-002"));
        assert!(codes.contains("APP-USES-002"));
    }

    #[test]
    fn doctor_reports_unknown_adapter_provenance() {
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
    local
  integrations
    crm: CRMProvider
      adapter @unknown.crm
  runtime
    unit api
      serves commands
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
  integrations
    serasa: CreditBureau
      adapter @unknown.serasa
"#,
            ),
            (
                "profiles.lzi",
                r#"
profile local
  integrations
    crm adapter @unknown.fake_crm
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("APP-ADAPTER-001"));
        assert!(codes.contains("REG-ADAPTER-001"));
        assert!(codes.contains("PROFILE-ADAPTER-001"));
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
    fn doctor_validates_external_calls_against_feature_requirements() {
        let valid = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  uses
    imports
  bindings
    imports.crm = integrations.crm
  targets
    backend go
  environments
    production
  runtime
    unit worker
      runs jobs *
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
    crm: CRMProvider
      adapter @adapter.crm
"#,
            ),
            (
                "imports.lzi",
                r#"
feature imports
  requires integration crm: CRMProvider

  job process_import
    trigger event import_uploaded
    idempotency by payload.batch_id
    retry 3 backoff exponential
    calls crm.normalize_import_batch
      batch_id = payload.batch_id
    timeout "30s"
    handler "./jobs/process_import.go"
"#,
            ),
        ]);

        assert!(
            valid.diagnostics().is_empty(),
            "expected external call contract to pass doctor"
        );

        let invalid = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  uses
    imports
  targets
    backend go
  environments
    production
  runtime
    unit worker
      runs jobs *
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "imports.lzi",
                r#"
feature imports
  job process_import
    trigger event import_uploaded
    calls crm.normalize_import_batch
      batch_id = payload.batch_id
    handler "./jobs/process_import.go"
"#,
            ),
        ]);

        let diagnostics = invalid.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("INT-CALL-001"));
        assert!(codes.contains("INT-CALL-002"));
        assert!(codes.contains("INT-CALL-003"));
        assert!(codes.contains("INT-CALL-004"));
    }

    #[test]
    fn doctor_validates_profiles_against_app_and_registry_contracts() {
        let valid = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  uses
    imports
  bindings
    imports.crm = integrations.crm
  targets
    backend go
    web react
  environments
    local
    production
  runtime
    unit worker
      runs jobs *
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
    crm: CRMProvider
      adapter @adapter.crm
      environments sandbox, production
"#,
            ),
            (
                "imports.lzi",
                r#"
feature imports
  requires integration crm: CRMProvider
"#,
            ),
            (
                "profiles.lzi",
                r#"
profile local
  urls
    web "http://localhost:3000"
    api "http://localhost:8080"
  bindings
    imports.crm = integrations.crm
  integrations
    crm environment sandbox
    crm adapter @adapter.fake_crm
  deploy
    topology monolith
"#,
            ),
        ]);

        assert!(
            valid.diagnostics().is_empty(),
            "expected profile contract to pass doctor"
        );

        let invalid = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  uses
    imports
  targets
    backend go
  environments
    production
  runtime
    unit worker
      runs jobs *
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
      environments production
"#,
            ),
            (
                "imports.lzi",
                r#"
feature imports
  requires integration crm: CRMProvider
"#,
            ),
            (
                "profiles.lzi",
                r#"
profile local
  urls
    web "http://localhost:3000"
  bindings
    imports.crm = integrations.serasa
  integrations
    crm environment sandbox
"#,
            ),
        ]);

        let diagnostics = invalid.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("APP-BIND-001"));
        assert!(codes.contains("PROFILE-001"));
        assert!(codes.contains("PROFILE-INT-001"));
        assert!(codes.contains("PROFILE-BIND-004"));
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

    #[test]
    fn doctor_validates_workspace_contract_edges() {
        let valid = package_from_sources(vec![(
            "workspace.lzi",
            r#"
workspace AcmeERP
  apps
    crm at "./apps/crm/app.lzi"
    ai external contract "acme.ai.v1"
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
"#,
        )]);

        assert!(
            valid.diagnostics().is_empty(),
            "expected valid workspace contract to pass doctor"
        );

        let invalid = package_from_sources(vec![(
            "workspace.lzi",
            r#"
workspace AcmeERP
  apps
    crm at "./apps/crm/app.lzi"
  boundaries
    ai consumes ai.*
  communication
    propagate actor
  gateway public_api
    route "/api/ai/*" to app ai
"#,
        )]);

        let diagnostics = invalid.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("WS-BOUNDARY-001"));
        assert!(codes.contains("WS-EVENT-001"));
        assert!(codes.contains("WS-GW-002"));
        assert!(codes.contains("WS-GW-003"));
        assert!(codes.contains("WS-GW-004"));
        assert!(codes.contains("WS-COMM-001"));
    }

    #[test]
    fn doctor_validates_external_contracts() {
        let valid = package_from_sources(vec![
            (
                "workspace.lzi",
                r#"
workspace AcmeERP
  apps
    ai external contract "acme.ai.v1"
"#,
            ),
            (
                "contracts/ai.lzi",
                r#"
contract acme.ai.v1
  import openapi "./contracts/ai.openapi.json"
  record CustomerSummaryRequest
    customer_id: ID required
  record CustomerSummaryResult
    summary: Text required
  operation summarize_customer
    transport http
    method POST
    path "/v1/customer-summary"
    input CustomerSummaryRequest
    output CustomerSummaryResult
    timeout "10s"
  event summary_ready
    topic "ai.summary_ready"
    payload
      customer_id: ID required
"#,
            ),
        ]);

        assert!(
            valid.diagnostics().is_empty(),
            "expected external contract to pass doctor"
        );

        let invalid = package_from_sources(vec![
            (
                "workspace.lzi",
                r#"
workspace AcmeERP
  apps
    ai external contract "acme.ai.v2"
"#,
            ),
            (
                "contracts/ai.lzi",
                r#"
contract acme.ai.v1
  operation summarize_customer
    transport http
"#,
            ),
        ]);

        let diagnostics = invalid.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("CONTRACT-OP-002"));
        assert!(codes.contains("CONTRACT-OP-003"));
        assert!(codes.contains("CONTRACT-OP-004"));
        assert!(codes.contains("WS-CONTRACT-001"));
    }

    // -------------------------------------------------------------------------
    // Cut A — cross-feature diagnostics (§5.3 snapshot pattern)
    // -------------------------------------------------------------------------

    fn codes(diagnostics: &[DoctorDiagnostic]) -> BTreeSet<&str> {
        diagnostics.iter().map(|d| d.code.as_str()).collect()
    }

    #[test]
    fn doctor_rejects_tool_with_stricter_policy_than_agent() {
        // Agent declares `policy @policy.read` but invokes a `command`
        // whose policy is `@policy.delete` — the conservative lattice
        // ordering flags this as `agent_tool_policy_diagnostics`.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  command archive
    policy @policy.delete
    deletes Customer

  agent triage
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    safety @validator.pii_scrub
    tools
      command.archive
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_tool_policy_diagnostics"),
            "expected agent_tool_policy_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_write_tool_without_safety() {
        // Same write-tool fan-in but with no `safety` declared — Cut A
        // requires safety as the write-tool guard (Q-impl-4 deferred
        // `idempotency by` to Cut B).
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  command archive
    policy @policy.delete
    deletes Customer

  agent triage
    policy @policy.delete
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      command.archive
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_tool_write_unguarded_diagnostics"),
            "expected agent_tool_write_unguarded_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_warns_pii_tool_without_safety() {
        // Registry declares `@tool.web_search` with `pii_classes contact`
        // and the agent invokes it with no safety — emit
        // `agent_pii_unsafetied_warning`.
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
registry
  tools
    tool web_search
      effect read
      pii_classes contact
      adapter @adapter.serp

feature customer
  agent triage
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      @tool.web_search
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_pii_unsafetied_warning"),
            "expected agent_pii_unsafetied_warning; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_unknown_discriminator_target() {
        // No `enum Intent` is declared anywhere — emit
        // `agent_discriminator_target_invalid_diagnostics`.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer_support
  agent classify_intent
    policy @policy.read
    output discriminator Intent
    model @llm.classifier
    temperature 0
    seed 42
    prompt "./p.md"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_discriminator_target_invalid_diagnostics"),
            "expected agent_discriminator_target_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_warns_evals_without_determinism_pin() {
        // Agent has evals but no `temperature 0` and no `seed` — emit
        // `eval_nondeterministic_warning`.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  agent flaky
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0.7
    prompt "./p.md"
    evals
      case smoke
        requires output contains "ok"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("eval_nondeterministic_warning"),
            "expected eval_nondeterministic_warning; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_registry_tool_missing_effect() {
        // `tool_registry_effect_required_diagnostics` is the only id
        // that fires off the registry-side IR. The parser collects a
        // defect for every `tool <name>` whose block omits `effect`.
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
registry
  tools
    tool calendar_create_event
      adapter @adapter.google_calendar
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("tool_registry_effect_required_diagnostics"),
            "expected tool_registry_effect_required_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_eval_ordered_op_on_non_numeric_operands() {
        // `requires customer.email < "x"` is an ordered op on text —
        // emit `eval_ordered_op_invalid_diagnostics`.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  agent bounded
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      case bad
        requires customer.email < "z@example.com"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("eval_ordered_op_invalid_diagnostics"),
            "expected eval_ordered_op_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_cors_origin_in_unknown_environment() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  environments
    local
    production

  cors
    allow_origins staging "https://staging.example.com"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cors_unknown_environment_diagnostics"),
            "expected cors_unknown_environment_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // Observability bucket cycle row 36 — `app.logging` / `app.tracing`
    // closed catalogs + sample-rate range + exporter binding.

    #[test]
    fn doctor_rejects_app_logging_level_outside_catalog() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  logging
    level verbose
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_logging_level_invalid_diagnostics"),
            "expected app_logging_level_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_app_logging_format_outside_catalog() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  logging
    format yaml
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_logging_format_invalid_diagnostics"),
            "expected app_logging_format_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_app_logging_redact_outside_catalog() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  logging
    redact secrets
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_logging_redact_unknown_diagnostics"),
            "expected app_logging_redact_unknown_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_app_logging_sample_rate_above_one() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  logging
    sample_rate 2.5
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_logging_sample_rate_range_diagnostics"),
            "expected app_logging_sample_rate_range_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_app_tracing_sample_rate_below_zero() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  tracing
    sample_rate -0.1
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_tracing_sample_rate_range_diagnostics"),
            "expected app_tracing_sample_rate_range_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_app_tracing_exporter_unbound() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  tracing
    exporter mystery
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_tracing_exporter_unbound_diagnostics"),
            "expected app_tracing_exporter_unbound_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_app_logging_with_canonical_values() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  logging
    level info
    format json
    redact pii
    sample_rate 1.0
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes_set = codes(&diagnostics);
        assert!(
            !codes_set.contains("app_logging_level_invalid_diagnostics"),
            "canonical logging must not fire level diagnostic; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            !codes_set.contains("app_logging_format_invalid_diagnostics"),
            "canonical logging must not fire format diagnostic; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            !codes_set.contains("app_logging_redact_unknown_diagnostics"),
            "canonical logging must not fire redact diagnostic; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            !codes_set.contains("app_logging_sample_rate_range_diagnostics"),
            "canonical logging must not fire sample_rate diagnostic; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_cors_wildcard_with_credentials() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  environments
    local
    production

  cors
    allow_origins production "https://app.example.com"
    allow_origins local "*"
    allow_credentials true
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cors_credentials_wildcard_conflict_diagnostics"),
            "expected cors_credentials_wildcard_conflict_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_warns_cors_origin_not_in_urls() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  environments
    local
    production

  urls
    web production "https://app.example.com"

  cors
    allow_origins production "https://stranger.example.com"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cors_origin_undocumented_diagnostics"),
            "expected cors_origin_undocumented_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_cors_origin_matching_declared_url() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  environments
    local
    production

  urls
    web production "https://app.example.com"

  cors
    allow_origins production "https://app.example.com"
    allow_credentials true
    max_age "1h"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes_set = codes(&diagnostics);
        for code in [
            "cors_unknown_environment_diagnostics",
            "cors_credentials_wildcard_conflict_diagnostics",
            "cors_origin_undocumented_diagnostics",
        ] {
            assert!(
                !codes_set.contains(code),
                "well-formed CORS must not produce {code}; got {:?}",
                diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn doctor_rejects_approval_with_unknown_role() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  policies
    delete: @role.admin

  command archive
    policy @policy.delete
    approval
      required_when target.tier = enterprise
      by @role.nonexistent
      timeout "24h"
      then deny
    deletes Customer
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("approval_role_unresolved_diagnostics"),
            "expected approval_role_unresolved_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_approval_with_malformed_timeout() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  policies
    delete: @role.admin

  command archive
    approval
      by @role.admin
      timeout "soon"
      then deny
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("approval_timeout_invalid_diagnostics"),
            "expected approval_timeout_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_approval_satisfies_write_tool_guard_without_agent_safety() {
        // Agent dispatches a write tool whose target command carries
        // `approval` — the guard is satisfied even though the agent
        // has no `safety` declaration.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  policies
    delete: @role.admin
    read: @scope.same_org

  command archive
    policy @policy.delete
    approval
      by @role.admin
      timeout "24h"
      then deny
    deletes Customer

  agent triage
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      command.archive
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("agent_tool_write_unguarded_diagnostics"),
            "approval on target command must satisfy the write-tool guard; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_approval_missing_required_children() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  policies
    delete: @role.admin

  command archive
    approval
      by @role.admin
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("approval_contract_diagnostics"),
            "expected approval_contract_diagnostics for missing children; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_authored_event_trace_agent_run() {
        // `event.trace agent_run` is reserved by the IR — authoring
        // it as a domain event must fail with the reserved-name
        // diagnostic.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event.trace agent_run
      payload
        agent_id: ID
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("event_trace_reserved_name_diagnostics"),
            "expected event_trace_reserved_name_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_subscriber_referencing_unknown_payload_field() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  job aggregate_costs
    trigger event.trace agent_run
      fictional_field = payload.fictional_field
      cost_usd = payload.cost_usd
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_run_subscriber_payload_drift_diagnostics"),
            "expected agent_run_subscriber_payload_drift_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_subscriber_with_canonical_fields_only() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  job aggregate_costs
    trigger event.trace agent_run
      cost_usd = payload.cost_usd
      tokens_total = payload.tokens_total
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("agent_run_subscriber_payload_drift_diagnostics"),
            "canonical fields must not drift; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // Observability bucket cycle row 35 — the 3 new reserved trace
    // event names (`command_run`, `job_run`, `webhook_run`) must
    // reuse the same `event_trace_reserved_name_diagnostics` path as
    // the Cut A.8 `agent_run` case. Authoring any of them is rejected.

    #[test]
    fn doctor_rejects_authored_event_trace_command_run() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event.trace command_run
      payload
        cmd: Text
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("event_trace_reserved_name_diagnostics"),
            "expected event_trace_reserved_name_diagnostics for command_run; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_authored_event_trace_job_run() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event.trace job_run
      payload
        job_id: ID
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("event_trace_reserved_name_diagnostics"),
            "expected event_trace_reserved_name_diagnostics for job_run; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_authored_event_trace_webhook_run() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event.trace webhook_run
      payload
        url: Text
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("event_trace_reserved_name_diagnostics"),
            "expected event_trace_reserved_name_diagnostics for webhook_run; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // Observability bucket cycle row 35 — `trigger_trace_unknown`. A
    // subscriber referencing `@trace.<name>` or
    // `trigger event.trace <name>` must resolve to a built-in trace
    // event or an authored `event.trace <name>` in the same file.

    #[test]
    fn doctor_rejects_trigger_trace_unknown_namespace_form() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  job dangling
    trigger @trace.fictional_event
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("trigger_trace_unknown_diagnostics"),
            "expected trigger_trace_unknown_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_trigger_trace_namespace_for_built_in() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  job collect
    trigger @trace.agent_run
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("trigger_trace_unknown_diagnostics"),
            "built-in @trace.agent_run must resolve; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_trigger_trace_namespace_for_authored_event() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event.trace customer_authored
      payload
        id: ID
  job collect
    trigger @trace.customer_authored
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("trigger_trace_unknown_diagnostics"),
            "authored event.trace in same file must satisfy @trace.<name>; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_expose_http_path_colliding_cross_feature_with_api() {
        // Agent in `customer` exposes the same (method, path) as an
        // `api` block in `customer_outreach`. Cross-feature collision
        // fires `agent_expose_path_conflict_cross_feature_diagnostics`.
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/customers/:customer_id/summary"
"#,
            ),
            (
                "customer_outreach.lzi",
                r#"
feature customer_outreach
  api customer_summary_stream
    method POST
    path "/api/customers/:id/summary"
    handler "./x.go"
"#,
            ),
        ]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_expose_path_conflict_cross_feature_diagnostics"),
            "expected agent_expose_path_conflict_cross_feature_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_unknown_audience_on_expose_http() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  agent restricted
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/x"
      audience nonexistent_audience
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_expose_audience_unknown_diagnostics"),
            "expected agent_expose_audience_unknown_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_audience_declared_in_surface() {
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  agent admin_only
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/admin/x"
      audience admin
"#,
            ),
            (
                "customer.web.lzx",
                r#"
surface customer web
  uses experience customer

  audience admin
"#,
            ),
        ]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("agent_expose_audience_unknown_diagnostics"),
            "audience declared in .lzx must be honored; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // ---------------------------------------------------------------------
    // Row 30 — Storage bucket cycle: 5 typed `@cap.File` diagnostics.
    // ---------------------------------------------------------------------

    #[test]
    fn doctor_emits_cap_file_visibility_undeclared() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_export
  domain
    resource Export
      id: ID required

  api download
    method GET
    path "/api/x/download"
    output @cap.File(max_size:10mb,accept:text/csv)
    policy @policy.global_read
    handler "./api/x/download.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_visibility_undeclared"),
            "expected cap_file_visibility_undeclared on api output; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_skips_visibility_undeclared_on_resource_field() {
        // Resource fields default `visibility` to private; the
        // diagnostic only fires on api outputs.
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_field
  domain
    resource Export
      file: @cap.File(max_size:10mb,accept:text/csv) required
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("cap_file_visibility_undeclared"),
            "resource fields default to private; should not emit; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_cap_file_accept_input_output_mismatch() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_pipeline
  domain
    resource ImportBatch
      file: @cap.File(max_size:25mb,accept:application/json,visibility:private) required

  api download
    method GET
    path "/api/x/download"
    output @cap.File(max_size:10mb,accept:text/csv,visibility:signed,signed_ttl:1h)
    policy @policy.global_read
    handler "./api/x/download.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_accept_input_output_mismatch"),
            "expected cap_file_accept_input_output_mismatch; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_overlapping_accept_lists() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_pipeline_ok
  domain
    resource ImportBatch
      file: @cap.File(max_size:25mb,accept:text/csv,visibility:private) required

  api download
    method GET
    path "/api/x/download"
    output @cap.File(max_size:10mb,accept:text/csv,visibility:signed,signed_ttl:1h)
    policy @policy.global_read
    handler "./api/x/download.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("cap_file_accept_input_output_mismatch"),
            "overlapping accept lists should not emit; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_cap_file_visibility_signed_ttl_mismatch_when_ttl_missing() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_signed
  api download
    method GET
    path "/api/x/download"
    output @cap.File(max_size:10mb,accept:text/csv,visibility:signed)
    policy @policy.global_read
    handler "./api/x/download.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_visibility_signed_ttl_mismatch"),
            "signed visibility without signed_ttl must emit; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_cap_file_visibility_signed_ttl_mismatch_when_ttl_with_private() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_private_ttl
  domain
    resource Export
      file: @cap.File(max_size:10mb,accept:text/csv,visibility:private,signed_ttl:1h) required
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_visibility_signed_ttl_mismatch"),
            "private visibility with signed_ttl must emit; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_cap_file_size_unit_invalid() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_size
  domain
    resource Export
      blob: @cap.File(max_size:large,accept:text/csv) required
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_size_unit_invalid"),
            "expected cap_file_size_unit_invalid; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_cap_file_mime_family_unknown() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_mime
  domain
    resource Export
      blob: @cap.File(max_size:10mb,accept:gibberish/csv,visibility:private) required
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_mime_family_unknown"),
            "expected cap_file_mime_family_unknown; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_well_formed_agent() {
        // Sanity gate: an agent that pins determinism, supplies safety,
        // and uses local read tools whose targets exist emits none of
        // the Cut A error codes.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    query.lookup by_id by id: ID
      policy @policy.read

  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    safety @validator.pii_email_scrub
    tools
      query.lookup.by_id
    evals
      case mentions_status
        requires output contains "active"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let cut_a_errors = [
            "agent_tool_policy_diagnostics",
            "agent_tool_write_unguarded_diagnostics",
            "agent_discriminator_target_invalid_diagnostics",
            "agent_discriminator_field_invalid_diagnostics",
            "eval_ordered_op_invalid_diagnostics",
            "tool_registry_effect_required_diagnostics",
        ];
        let surfaced = codes(&diagnostics);
        for code in cut_a_errors {
            assert!(
                !surfaced.contains(code),
                "well-formed agent should not emit {code}; got {:?}",
                diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
            );
        }
    }

    // -------------------------------------------------------------------------
    // Phase L — `auth` block cross-feature diagnostics.
    //
    // Four ids per docs/proposals/bucket-auth-cycle.md §Doctor/LSP:
    //   - auth_password_algorithm_hash_mismatch
    //   - auth_sessions_resource_unknown
    //   - auth_identity_field_unknown
    //   - auth_oauth_adapter_unbound
    // -------------------------------------------------------------------------

    #[test]
    fn doctor_emits_auth_password_algorithm_hash_mismatch() {
        // `auth.password.algorithm bcrypt` diverges from
        // `@cap.Hashed(algorithm:argon2id)` on the session resource's
        // hash field.
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity Session.email
    password
      algorithm bcrypt
      hash @fn.h
      verify @fn.v
      rate_limit "5 per 10 minutes"

    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        let mismatch: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "auth_password_algorithm_hash_mismatch")
            .collect();
        assert_eq!(
            mismatch.len(),
            1,
            "expected exactly one auth_password_algorithm_hash_mismatch; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            mismatch[0].message.contains("bcrypt"),
            "diagnostic should cite authored algorithm: {}",
            mismatch[0].message
        );
        assert!(
            mismatch[0].message.contains("argon2id"),
            "diagnostic should cite resource axis: {}",
            mismatch[0].message
        );
    }

    #[test]
    fn doctor_emits_auth_sessions_resource_unknown() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity Session.email
    sessions
      resource BogusSession
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("auth_sessions_resource_unknown"),
            "expected auth_sessions_resource_unknown; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_auth_identity_field_unknown_for_missing_field() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required

  auth
    identity Session.email
    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "auth_identity_field_unknown")
            .collect();
        assert!(
            !hits.is_empty(),
            "expected auth_identity_field_unknown; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(hits[0].message.contains("field not found"));
    }

    #[test]
    fn doctor_emits_auth_identity_field_unknown_for_non_identity_shape() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      note: Text required
      expires_at: DateTime required

  auth
    identity Session.note
    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "auth_identity_field_unknown")
            .collect();
        assert!(
            !hits.is_empty(),
            "expected auth_identity_field_unknown; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(hits[0].message.contains("identity-shaped"));
    }

    #[test]
    fn doctor_emits_auth_oauth_adapter_unbound() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity Session.email
    oauth google
      adapter @adapter.bogus_google_oauth

    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("auth_oauth_adapter_unbound"),
            "expected auth_oauth_adapter_unbound; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_resolves_oauth_adapter_via_feature_extensions() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity Session.email
    oauth google
      adapter @adapter.google_oauth

    sessions
      resource Session
      ttl "1 day"
      refresh false

  extensions
    adapter google_oauth: IntegrationAdapter[GoogleOAuth] at "./oauth.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("auth_oauth_adapter_unbound"),
            "extension adapter must satisfy oauth adapter lookup; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_well_formed_auth_emits_no_auth_diagnostics() {
        // The canonical-shape positive case. None of the four auth_*
        // diagnostics should fire on a coherent block.
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity Session.email
    password
      algorithm argon2id
      hash @fn.h
      verify @fn.v
      rate_limit "5 per 10 minutes"

    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        let surfaced = codes(&diagnostics);
        for code in [
            "auth_password_algorithm_hash_mismatch",
            "auth_sessions_resource_unknown",
            "auth_identity_field_unknown",
            "auth_oauth_adapter_unbound",
        ] {
            assert!(
                !surfaced.contains(code),
                "well-formed auth should not emit {code}; got {:?}",
                diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn doctor_resolves_identity_resource_via_feature_uses() {
        // `customer_auth uses customer` — Customer.email is declared
        // in the `customer` feature; auth identity in customer_auth
        // must resolve via the `uses` graph.
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature customer
  domain
    resource Customer
      email: @semantic.Email required

feature customer_auth
  uses customer

  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required

  auth
    identity Customer.email
    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("auth_identity_field_unknown"),
            "uses-relative identity resolution failed: {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }
}
