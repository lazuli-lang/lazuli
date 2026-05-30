//! `DoctorPackage` — the package-wide doctor fact bundle.
//!
//! Carries every IR slice + per-file source needed by the
//! `diagnostics()` dispatcher (which lives in `dispatch.rs`).
//! `load` runs once per `lazuli doctor` invocation and walks all
//! `.lzi` / `.lzx` sources in the package, lifting every Tier-3
//! fact family + auth / plan-gate facts into a single struct.
//!
//! ## Module layout
//!
//! - `state` — the `LoadAccumulator` + `LoadContext` bundles passed
//!   into each per-file processor.
//! - `load_lzi` — process one `.lzi` file (manifests, agents,
//!   Tier-3 facts, semantic-type / money / test-discipline
//!   diagnostics).
//! - `load_lzx` — process one `.lzx` file (experiences, view test
//!   discipline rules).
//! - `load_plan_gate` — final-pass plan + gate aggregation across
//!   every `.lzi` source.
//!
//! Extracted from `doctor/mod.rs` in rails-style R4-C Stage 2.
//! Fanned out into per-concern sub-modules in rails-style R7-4.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lazuli_analyzer::lower_feature_skeleton;
use lazuli_doctor_config::{DoctorProfile as SecurityProfile, ResolvedDoctorConfig};
use lazuli_manifest::lazurite_manifest::{self, Manifest};
use lazuli_syntax::parse_feature_skeletons;

use super::helpers::doctor_project_root;
use super::parsers::{is_lzi_path, is_lzx_path};
use super::{
    AgentFacts, ApprovalBlockPresence, AuthFacts, CommandKey, CommandPolicy, DoctorAppContract,
    DoctorAppManifest, DoctorAppProfile, DoctorAppRegistry, DoctorAppWorkspace, DoctorDiagnostic,
    DoctorFile, ExperienceFacts, FeatureSymbols, OperationalFacts, RegistryToolDefect,
    ResourceFact, Tier3FeatureFacts, populate_feature_symbols_from_ir,
};

mod load_lzi;
mod load_lzx;
mod load_plan_gate;
mod state;
mod tier3_harvest;

use load_lzi::process_lzi_file;
use load_lzx::process_lzx_file;
use load_plan_gate::build_plan_gate_facts;
use state::{LoadAccumulator, LoadContext};

#[derive(Debug)]
pub struct DoctorPackage {
    pub(super) project_root: PathBuf,
    pub(super) security_profile: SecurityProfile,
    /// v2 single-source residual — the caller-supplied
    /// [`ResolvedDoctorConfig`] that drives EVERY severity decision the
    /// package makes (profile + coverage/category presets + per-rule
    /// `severity_override`). The CLI builds it from the on-disk
    /// `Lazurite.toml` (byte-identical to the manifest it already loads);
    /// the LSP builds it from the UNSAVED `Lazurite.toml` editor buffer
    /// when that file is open, falling back to disk. `security_profile`
    /// above is derived from `config.profile.0` so the profile-only
    /// aggregators (which still take `self.security_profile`) follow the
    /// same source. The package no longer reads `self.lazurite_manifest`'s
    /// `[doctor]` section for severity — only for the non-severity uses
    /// (plugin alias maps / `semantic_type_unknown` suppression, app-root
    /// resolution).
    pub(super) config: ResolvedDoctorConfig,
    /// `true` when `lazuli doctor` was invoked on a single `.lzi`/`.lzx`
    /// file rather than a project directory. Single-file mode skips
    /// project-level checks (e.g. `MANIFEST-REQUIRED-001`) that depend
    /// on having a real project root with `app.lzi` + `Lazurite.toml`.
    pub(super) single_file_input: bool,
    pub(super) lazurite_manifest: Option<Manifest>,
    pub(super) files: Vec<DoctorFile>,
    pub(super) workspace: Option<DoctorAppWorkspace>,
    pub(super) contracts: Vec<DoctorAppContract>,
    pub(super) app: Option<DoctorAppManifest>,
    pub(super) registry: Option<DoctorAppRegistry>,
    pub(super) profiles: Vec<DoctorAppProfile>,
    pub(super) commands: BTreeMap<CommandKey, CommandPolicy>,
    pub(super) experiences: BTreeMap<String, ExperienceFacts>,
    pub(super) operational: OperationalFacts,
    /// Cut A: agent IR per feature, loaded through
    /// `lazuli_syntax::parse_feature_skeletons` +
    /// `lazuli_analyzer::lower_feature_skeleton`.
    pub(super) agents: Vec<AgentFacts>,
    /// Cut A: per-feature enum/record/query/command symbol tables used
    /// for discriminator + tool-policy cross-resolution.
    pub(super) feature_symbols: BTreeMap<String, FeatureSymbols>,
    /// Cut A: registry `tool <name>` headers that lacked `effect`.
    pub(super) registry_tool_defects: Vec<RegistryToolDefect>,
    /// Phase L Tier 4b — minimal text-pattern walk of `approval` blocks
    /// inside command bodies. Only used for the `missing children`
    /// variant of `approval_contract_diagnostics`; every other approval
    /// check reads `Command.approval` from `Tier3FeatureFacts` (IR).
    /// The walker exists because parse-error approval blocks never
    /// reach the IR — they short-circuit the feature lift.
    pub(super) approval_presences: Vec<ApprovalBlockPresence>,
    /// Phase L: lowered `auth` block per feature, paired with source
    /// line anchors for subblock-precise diagnostics.
    pub(super) auth_facts: Vec<AuthFacts>,
    /// Phase L: per-feature resource declarations + field type text.
    /// Used to resolve `auth identity Customer.email` and
    /// `auth sessions resource CustomerSession` and to read
    /// `@cap.Hashed(algorithm:…)` axes off session resource fields.
    pub(super) feature_resources: BTreeMap<String, BTreeMap<String, ResourceFact>>,
    /// Phase L: per-feature `extensions adapter <local>` declarations
    /// for the `auth_oauth_adapter_unbound` adapter resolution scope.
    pub(super) feature_adapters: BTreeMap<String, BTreeSet<String>>,
    /// Phase L: per-feature `uses <other_feature>, ...` references so
    /// `auth identity Customer.email` in `feature customer_auth` can
    /// resolve `Customer` in `feature customer` when `uses customer` is
    /// declared.
    pub(super) feature_uses: BTreeMap<String, BTreeSet<String>>,
    /// Phase L Tier 3: lifted `Job` / `Webhook` / `Notification` /
    /// `EventGroup` per feature, paired with source line anchors so the
    /// six new diagnostics (`JOB-*`, `WEBHOOK-SCOPE-*`,
    /// `NOTIF-CHANNEL-*`, `EVENTGROUP-NESTING-*`) attach to the right
    /// authoring site.
    pub(super) tier3_facts: Vec<Tier3FeatureFacts>,
    /// PG.B — package-wide plan-and-gate facts (closed plan catalog,
    /// subscription anchor, per-callable gate directives). `None` when
    /// the package authors no `plan` blocks and no `gate` directives.
    pub(super) plan_gate_facts: Option<lazuli_analyzer::PlanGateFacts>,
    /// D3 — caller-injected package-level findings folded into the same
    /// sorted + suppressed stream as the engine's own aggregators. Today
    /// this carries the CLI's `SCHEMA-RICH-001` (which needs the
    /// TS-codegen module loader this crate deliberately does not depend
    /// on). Empty when the caller has nothing to inject (e.g. the LSP).
    pub(super) injected: Vec<DoctorDiagnostic>,
}

impl DoctorPackage {
    /// Test-only convenience: load with a no-op file-local injector.
    ///
    /// In-crate engine tests assert on **package-level** aggregator codes
    /// (cross-feature / project-wide), which don't depend on the file-local
    /// "shape" pass the LSP provides. Tests that need real file-local rows
    /// (e.g. the `env-contract` dedupe path) live in `lazuli_cli`'s test
    /// suite where `lazuli_lsp` is available.
    #[cfg(test)]
    pub(crate) fn load(input: &Path, security_profile: SecurityProfile) -> Result<Self> {
        let config = ResolvedDoctorConfig {
            profile: security_profile.into(),
            ..ResolvedDoctorConfig::default()
        };
        Self::load_with(input, &config, &|_, _| Vec::new())
    }

    /// Load the package with a caller-supplied file-local diagnostic
    /// injector (D3).
    ///
    /// The injector is called once per discovered `.lzi` / `.lzx` source
    /// with that file's path + source; it returns the file-local "shape"
    /// diagnostics for that file (the rows the LSP's
    /// `diagnostics_for_source_with_profile` produces). This is the exact
    /// integration point where the engine used to import `lazuli_lsp`
    /// directly — now severed so this crate depends on neither
    /// `lazuli_lsp` nor `lazuli_cli`.
    pub(crate) fn load_with(
        input: &Path,
        config: &ResolvedDoctorConfig,
        file_local: &super::FileLocalInjector<'_>,
    ) -> Result<Self> {
        // v2 — the profile that flows to every profile-keyed aggregator is
        // the config's profile. The manifest's `[doctor]` is still loaded
        // below, but ONLY for non-severity uses; severity (profile +
        // preset + overrides) now rides `config`.
        let security_profile = config.profile.0;
        let paths = super::collect_package_paths(input)?;
        if paths.is_empty() {
            bail!("no .lzi or .lzx files found for {}", input.display());
        }
        // Track whether the doctor was invoked on a single file vs a
        // project directory so project-level rules
        // (MANIFEST-REQUIRED-001) can skip when no real project root
        // is present. Audit ref: R1.C sweep produced 12 false
        // positives because the parent dir of standalone `.lzi`
        // fixtures was scanned for @lazuli/plugin-* refs from sibling
        // files.
        let single_file_input = input.is_file();
        let project_root = doctor_project_root(input);
        let lazurite_manifest = lazurite_manifest::load(&project_root).with_context(|| {
            format!(
                "failed to load {}",
                project_root.join("Lazurite.toml").display()
            )
        })?;

        let ctx = LoadContext {
            project_root: project_root.clone(),
            security_profile,
            config,
            lazurite_manifest: &lazurite_manifest,
        };
        let mut acc = LoadAccumulator::default();
        let mut files = Vec::new();

        for path in paths {
            let source = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let mut file = DoctorFile {
                path,
                source,
                local_diagnostics: Vec::new(),
                lzx: None,
            };

            // D3 — file-local "shape" diagnostics are injected at the
            // boundary instead of imported. The caller (CLI or LSP)
            // supplies them via `file_local`; `security_profile` is the
            // same posture the caller resolves its file-local pass at, so
            // the merged stream stays byte-identical to the pre-lift CLI.
            file.local_diagnostics = file_local(&file.path, &file.source);

            if is_lzi_path(&file.path) {
                process_lzi_file(&mut file, &mut acc, &ctx);
            } else if is_lzx_path(&file.path) {
                process_lzx_file(&mut file, &mut acc, &ctx);
            }

            files.push(file);
        }

        // Phase L Tier 4 follow-up (final wave) — `feature_symbols.commands`
        // is now populated from the typed `Tier3FeatureFacts.commands` slot
        // built earlier in this loop. Replaces the per-file
        // `collect_feature_symbols` text walker. Runs once after every file
        // has lifted its IR slice so cross-feature command policy hints are
        // available to `agent_tool_diagnostics`.
        populate_feature_symbols_from_ir(&acc.tier3_facts, &mut acc.feature_symbols);

        // PG.B — aggregate package-wide plan-and-gate facts. Walks every
        // .lzi source once to collect top-level `plan` blocks +
        // per-feature `gate ...` directives, and reads the subscription
        // anchor from app.lzi.
        let plan_gate_facts = build_plan_gate_facts(&files, acc.app.as_ref());

        Ok(Self {
            project_root,
            security_profile,
            config: config.clone(),
            single_file_input,
            lazurite_manifest,
            files,
            workspace: acc.workspace,
            contracts: acc.contracts,
            app: acc.app,
            registry: acc.registry,
            profiles: acc.profiles,
            commands: acc.commands,
            experiences: acc.experiences,
            operational: acc.operational,
            agents: acc.agents,
            feature_symbols: acc.feature_symbols,
            registry_tool_defects: acc.registry_tool_defects,
            approval_presences: acc.approval_presences,
            auth_facts: acc.auth_facts,
            feature_resources: acc.feature_resources,
            feature_adapters: acc.feature_adapters,
            feature_uses: acc.feature_uses,
            tier3_facts: acc.tier3_facts,
            plan_gate_facts,
            injected: Vec::new(),
        })
    }

    /// Wave 6 — extract `(features, lzx_views)` from the loaded package
    /// for `lazuli_doctor::coverage::build_coverage_report`. Re-parses
    /// `.lzi` sources (cheap; the existing `parse_feature_skeletons` +
    /// `lower_feature_skeleton` are already cached at the syntax layer
    /// for the per-feature loop, so this second pass is mostly metadata
    /// extraction). Walks `file.lzx` documents directly for view refs.
    pub(super) fn coverage_inputs(
        &self,
    ) -> (
        Vec<lazuli_ir::Feature>,
        Vec<lazuli_doctor::coverage::LzxViewRef>,
    ) {
        let mut features: Vec<lazuli_ir::Feature> = Vec::new();
        let mut lzx_views: Vec<lazuli_doctor::coverage::LzxViewRef> = Vec::new();
        for file in &self.files {
            if is_lzi_path(&file.path) {
                if let Ok(skeletons) = parse_feature_skeletons(&file.source) {
                    for skeleton in &skeletons {
                        if let Ok(feature) = lower_feature_skeleton(skeleton) {
                            features.push(feature);
                        }
                    }
                }
            } else if is_lzx_path(&file.path)
                && let Some(document) = &file.lzx
            {
                for experience in &document.experiences {
                    for view in &experience.views {
                        lzx_views.push(lazuli_doctor::coverage::LzxViewRef {
                            experience: experience.name.clone(),
                            view: view.name.clone(),
                        });
                    }
                }
            }
        }
        (features, lzx_views)
    }

    /// Iron-hand meta-bundle — return the active `[doctor.coverage]
    /// preset` parsed into `CoveragePreset`, or `None` when no manifest
    /// / no preset / unknown preset name. Drives both the coverage
    /// thresholds and the rule-severity escalation map applied by
    /// dispatchers (see `context_vocab_diagnostics`).
    pub(super) fn coverage_preset(&self) -> Option<lazuli_doctor::coverage::CoveragePreset> {
        // v2 — read the coverage preset off the caller-supplied severity
        // config (built from disk by the CLI, from the unsaved buffer by
        // the LSP), NOT from the on-disk-loaded manifest. The CLI's config
        // is built from the same on-disk `[doctor.coverage] preset`, so the
        // value is byte-identical to the previous manifest read.
        self.config.coverage_preset
    }
}
