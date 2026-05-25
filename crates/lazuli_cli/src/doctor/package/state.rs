//! Mutable accumulators + immutable context for `DoctorPackage::load`.
//!
//! `LoadAccumulator` owns the ~25 buffers the lzi/lzx per-file
//! processors push into. Bundling them keeps the orchestrator
//! readable (single `&mut acc` arg) and lets each per-file
//! processor live in its own module without 25-arg fn signatures.
//!
//! `LoadContext` carries the immutable inputs every processor needs
//! (project root, manifest, security profile). Both structs are
//! `package`-private — they never leak past `load`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lazuli_lsp::SecurityProfile;

use super::super::{
    AgentFacts, ApprovalBlockPresence, AuthFacts, CommandKey, CommandPolicy, DoctorAppContract,
    DoctorAppManifest, DoctorAppProfile, DoctorAppRegistry, DoctorAppWorkspace, ExperienceFacts,
    FeatureSymbols, OperationalFacts, RegistryToolDefect, ResourceFact, Tier3FeatureFacts,
};
use crate::lazurite_manifest::Manifest;

/// Immutable inputs every per-file processor reads.
pub(super) struct LoadContext<'a> {
    pub(super) project_root: PathBuf,
    pub(super) security_profile: SecurityProfile,
    pub(super) lazurite_manifest: &'a Option<Manifest>,
}

/// Mutable buffers the per-file processors push into. Drained into
/// the final `DoctorPackage` at the tail of `load`.
#[derive(Default)]
pub(super) struct LoadAccumulator {
    pub(super) workspace: Option<DoctorAppWorkspace>,
    pub(super) contracts: Vec<DoctorAppContract>,
    pub(super) app: Option<DoctorAppManifest>,
    pub(super) registry: Option<DoctorAppRegistry>,
    pub(super) profiles: Vec<DoctorAppProfile>,
    pub(super) commands: BTreeMap<CommandKey, CommandPolicy>,
    pub(super) experiences: BTreeMap<String, ExperienceFacts>,
    pub(super) operational: OperationalFacts,
    pub(super) agents: Vec<AgentFacts>,
    pub(super) feature_symbols: BTreeMap<String, FeatureSymbols>,
    pub(super) registry_tool_defects: Vec<RegistryToolDefect>,
    pub(super) approval_presences: Vec<ApprovalBlockPresence>,
    pub(super) auth_facts: Vec<AuthFacts>,
    pub(super) feature_resources: BTreeMap<String, BTreeMap<String, ResourceFact>>,
    pub(super) feature_adapters: BTreeMap<String, BTreeSet<String>>,
    pub(super) feature_uses: BTreeMap<String, BTreeSet<String>>,
    pub(super) tier3_facts: Vec<Tier3FeatureFacts>,
}
