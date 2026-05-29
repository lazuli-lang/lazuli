//! `lazuli_doctor_config` — the single source of truth for doctor
//! profile / preset resolution and per-finding severity escalation.
//!
//! ## Why this crate exists
//!
//! Before this crate, the doctor's severity decision was scattered
//! across four sites in `lazuli_cli` (the category-aware
//! `doctor_severity_for`, the four `resolve_<cat>_severity` preset
//! helpers, and the `context_vocab_diagnostics` manifest-override
//! closure) plus the coverage `preset_severity_overrides` map in
//! `lazuli_doctor`. Each could drift from the others. Worse, the 3-value
//! profile enum (`SecurityProfile`) lived UP in `lazuli_lsp`, and the
//! CLI doctor reached up into the LSP to borrow it — an inverted
//! dependency.
//!
//! This crate lifts the resolution into ONE leaf crate that both the
//! CLI and (in a later wave) the LSP depend on. It owns:
//!
//! - [`DoctorProfile`] — the 3-value profile enum, relocated here from
//!   `lazuli_lsp::types::SecurityProfile`. `lazuli_lsp` re-exports it as
//!   `SecurityProfile` for ABI stability.
//! - [`ResolvedDoctorConfig`] — a fully-resolved, IO-free snapshot of
//!   the `[doctor]` / `[doctor.coverage]` / `[doctor.<cat>]` blocks plus
//!   the active profile.
//! - [`effective_severity`] — THE single pure severity resolver. Its
//!   answer is the exact union of the four CLI resolvers + the coverage
//!   escalation map, with precedence
//!   `manifest override > coverage preset > category preset > profile
//!   default`.
//!
//! ## Boundary
//!
//! The crate is `tower-lsp`-free and CLI-free: it depends only on
//! `lazuli_doctor` (for [`DoctorSeverity`], [`RuleCategory`], and the
//! per-category preset enums), `serde`, and `toml`. The
//! `DoctorSeverity` → editor `DiagnosticSeverity` mapping stays in
//! `lazuli_lsp`; the `--fail-on` / `--format` output gates stay in the
//! CLI. This keeps the crate a true leaf над `lazuli_doctor`.

use std::collections::BTreeMap;

use lazuli_doctor::coverage::{CoveragePreset, preset_severity_overrides};
use lazuli_doctor::error_handling::preset::{
    ErrorHandlingPreset, preset_rule_severity as error_handling_preset_rule_severity,
};
use lazuli_doctor::internal_hygiene::preset::{
    InternalHygienePreset, preset_rule_severity as internal_hygiene_preset_rule_severity,
};
use lazuli_doctor::lzi_hygiene::preset::{
    LziHygienePreset, preset_rule_severity as lzi_hygiene_preset_rule_severity,
};
use lazuli_doctor::test_discipline::preset::{
    TestDisciplinePreset, preset_rule_severity as test_discipline_preset_rule_severity,
};
pub use lazuli_doctor::{DoctorSeverity, RuleCategory};

mod manifest;

pub use manifest::{
    CoverageSection, Doctor, ErrorHandlingDoctor, InternalHygieneDoctor, LayerThresholdConfig,
    LziHygieneDoctor, SeverityOverride, TestDisciplineDoctor,
};

/// Selector for which subset of diagnostic codes the doctor / LSP emits
/// and at what severity.
///
/// Relocated from `lazuli_lsp::types::SecurityProfile` (which now
/// re-exports this type as `SecurityProfile` for ABI stability). The
/// CLI `doctor` reads this off the `--security-profile` flag (and, in a
/// later wave, off `[doctor] profile` in `Lazurite.toml`).
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor_config::DoctorProfile;
///
/// assert_eq!(DoctorProfile::parse("strict"), Some(DoctorProfile::Strict));
/// assert_eq!(DoctorProfile::Strict.as_str(), "strict");
/// assert_eq!(DoctorProfile::parse("nonsense"), None);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorProfile {
    /// Pre-production sandbox. Production-only codes are demoted so
    /// scaffolds can iterate without churn from rules that don't apply
    /// until deploy.
    Prototype,
    /// Default. Every catalog code fires at its declared severity.
    Strict,
    /// Production lock-in. A handful of warnings escalate to errors so
    /// `doctor` blocks deploy on weak postures (missing redact, open
    /// CORS, etc.).
    Production,
}

impl DoctorProfile {
    /// Parse a profile name as authored in `Lazurite.toml [doctor]
    /// profile = "..."` (or the `--security-profile` CLI flag). Returns
    /// `None` for any unrecognized name.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor_config::DoctorProfile;
    ///
    /// assert_eq!(DoctorProfile::parse("prototype"), Some(DoctorProfile::Prototype));
    /// assert_eq!(DoctorProfile::parse("  production  "), Some(DoctorProfile::Production));
    /// assert_eq!(DoctorProfile::parse("loose"), None);
    /// ```
    pub fn parse(input: &str) -> Option<Self> {
        match input.trim() {
            "prototype" => Some(Self::Prototype),
            "strict" => Some(Self::Strict),
            "production" => Some(Self::Production),
            _ => None,
        }
    }

    /// Stable lowercase identifier matching the `profile = "..."` TOML
    /// value. Round-trips with [`parse`](Self::parse).
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor_config::DoctorProfile;
    ///
    /// assert_eq!(DoctorProfile::Production.as_str(), "production");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prototype => "prototype",
            Self::Strict => "strict",
            Self::Production => "production",
        }
    }
}

/// Error returned by [`ResolvedDoctorConfig::resolve`] when the supplied
/// manifest TOML cannot be parsed.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor_config::{ResolvedDoctorConfig, DoctorProfile};
///
/// let err = ResolvedDoctorConfig::resolve(Some("this is = = not toml"), DoctorProfile::Strict);
/// assert!(err.is_err());
/// ```
#[derive(Debug)]
pub struct ConfigError(pub String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "doctor config parse error: {}", self.0)
    }
}

impl std::error::Error for ConfigError {}

/// Fully-resolved, IO-free doctor configuration. Built once per
/// `lazuli doctor` run (and, in a later wave, once per LSP workspace
/// config refresh). Holds the active profile + every preset / override
/// the four scattered CLI resolvers used to read piecemeal, so
/// [`effective_severity`] can answer with a single pure function.
///
/// Field semantics mirror today's CLI exactly:
///
/// - `profile` — the active [`DoctorProfile`] (`--security-profile`
///   flag, default `Strict`).
/// - `coverage_preset` — parsed `[doctor.coverage] preset = "..."`;
///   `None` when absent or unparseable.
/// - the four `*_preset` fields — parsed `[doctor.<cat>] preset = "..."`
///   for the category families that ship a preset escalation.
/// - `overrides` — every `[doctor.<cat>].severity_override.<CODE>`
///   merged into one map keyed by code.
#[derive(Debug, Clone, Default)]
pub struct ResolvedDoctorConfig {
    /// Active security profile.
    pub profile: ProfileSlot,
    /// `[doctor.coverage] preset`.
    pub coverage_preset: Option<CoveragePreset>,
    /// `[doctor.test_discipline] preset`.
    pub test_discipline_preset: Option<TestDisciplinePreset>,
    /// `[doctor.internal_hygiene] preset`.
    pub internal_hygiene_preset: Option<InternalHygienePreset>,
    /// `[doctor.error_handling] preset`.
    pub error_handling_preset: Option<ErrorHandlingPreset>,
    /// `[doctor.lzi_hygiene] preset`.
    pub lzi_hygiene_preset: Option<LziHygienePreset>,
    /// All `[doctor.<cat>].severity_override.<CODE>` rows merged, keyed
    /// by rule code. Mirrors the CLI's per-category lookup (today only
    /// `[doctor.test_discipline]` is consulted at severity-resolution
    /// time; this map is the superset so the LSP and future categories
    /// share one lookup).
    pub overrides: BTreeMap<String, SeverityOverride>,
}

/// Wrapper so [`ResolvedDoctorConfig`] can `derive(Default)` to the
/// historical default profile (`Strict`) without making `DoctorProfile`
/// itself carry a `Default` impl that could mask an unset profile
/// elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileSlot(pub DoctorProfile);

impl Default for ProfileSlot {
    fn default() -> Self {
        ProfileSlot(DoctorProfile::Strict)
    }
}

impl From<DoctorProfile> for ProfileSlot {
    fn from(p: DoctorProfile) -> Self {
        ProfileSlot(p)
    }
}

impl ResolvedDoctorConfig {
    /// Parse a `Lazurite.toml` body + the CLI/LSP-selected profile into a
    /// resolved config. `manifest_toml = None` yields a config with only
    /// the profile set (no manifest present — every preset/override is
    /// absent), matching the CLI's behavior on single-file / scratch-dir
    /// invocations.
    ///
    /// The `profile` argument is authoritative: today the CLI takes the
    /// profile from the `--security-profile` flag, NOT from `[doctor]
    /// profile`, so `resolve` does the same. (Wiring `[doctor] profile`
    /// into profile selection is a later wave; this method intentionally
    /// does not read `[doctor] profile` to drive `profile`, preserving
    /// today's behavior byte-for-byte.)
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor_config::{ResolvedDoctorConfig, DoctorProfile};
    ///
    /// let cfg = ResolvedDoctorConfig::resolve(None, DoctorProfile::Strict).unwrap();
    /// assert_eq!(cfg.profile.0, DoctorProfile::Strict);
    /// assert!(cfg.coverage_preset.is_none());
    ///
    /// let toml = r#"
    /// [doctor.coverage]
    /// preset = "tdd-iron-hand"
    /// "#;
    /// let cfg = ResolvedDoctorConfig::resolve(Some(toml), DoctorProfile::Strict).unwrap();
    /// assert!(cfg.coverage_preset.is_some());
    /// ```
    pub fn resolve(
        manifest_toml: Option<&str>,
        profile: DoctorProfile,
    ) -> Result<Self, ConfigError> {
        let doctor: Option<Doctor> = match manifest_toml {
            Some(body) => {
                let manifest: ManifestShim =
                    toml::from_str(body).map_err(|e| ConfigError(e.to_string()))?;
                manifest.doctor
            }
            None => None,
        };
        Ok(Self::from_doctor(doctor.as_ref(), profile))
    }

    /// Parse a `Lazurite.toml` body, reading the active profile FROM
    /// `[doctor] profile` (default [`DoctorProfile::Strict`] when the key
    /// is absent or unparseable).
    ///
    /// This is the LSP's workspace-config entry point (W2): unlike
    /// [`resolve`](Self::resolve) — which takes the profile from the
    /// caller (the CLI's `--security-profile` flag) — this method honors
    /// the authored `[doctor] profile`. `manifest_toml = None` yields a
    /// profile-only `Strict` config.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor_config::{ResolvedDoctorConfig, DoctorProfile};
    ///
    /// let toml = "[doctor]\nprofile = \"production\"\n";
    /// let cfg = ResolvedDoctorConfig::resolve_reading_profile(Some(toml)).unwrap();
    /// assert_eq!(cfg.profile.0, DoctorProfile::Production);
    ///
    /// // Absent / no manifest -> Strict default.
    /// let cfg = ResolvedDoctorConfig::resolve_reading_profile(None).unwrap();
    /// assert_eq!(cfg.profile.0, DoctorProfile::Strict);
    /// ```
    pub fn resolve_reading_profile(manifest_toml: Option<&str>) -> Result<Self, ConfigError> {
        let doctor: Option<Doctor> = match manifest_toml {
            Some(body) => {
                let manifest: ManifestShim =
                    toml::from_str(body).map_err(|e| ConfigError(e.to_string()))?;
                manifest.doctor
            }
            None => None,
        };
        let profile = doctor
            .as_ref()
            .and_then(|d| d.profile.as_deref())
            .and_then(DoctorProfile::parse)
            .unwrap_or(DoctorProfile::Strict);
        Ok(Self::from_doctor(doctor.as_ref(), profile))
    }

    /// Build a resolved config from an already-parsed `[doctor]` block.
    /// This is the path the CLI uses, since it has already deserialized
    /// the manifest into its own `Manifest` type — it hands the
    /// `[doctor]` block straight in, avoiding a second TOML parse.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor_config::{ResolvedDoctorConfig, DoctorProfile};
    ///
    /// let cfg = ResolvedDoctorConfig::from_doctor(None, DoctorProfile::Production);
    /// assert_eq!(cfg.profile.0, DoctorProfile::Production);
    /// ```
    pub fn from_doctor(doctor: Option<&Doctor>, profile: DoctorProfile) -> Self {
        let coverage_preset = doctor
            .and_then(|d| d.coverage.as_ref())
            .and_then(|c| c.preset.as_deref())
            .and_then(CoveragePreset::parse);
        let test_discipline_preset = doctor
            .and_then(|d| d.test_discipline.as_ref())
            .and_then(|td| td.preset.as_deref())
            .and_then(TestDisciplinePreset::parse);
        let internal_hygiene_preset = doctor
            .and_then(|d| d.internal_hygiene.as_ref())
            .and_then(|ih| ih.preset.as_deref())
            .and_then(InternalHygienePreset::parse);
        let error_handling_preset = doctor
            .and_then(|d| d.error_handling.as_ref())
            .and_then(|eh| eh.preset.as_deref())
            .and_then(ErrorHandlingPreset::parse);
        let lzi_hygiene_preset = doctor
            .and_then(|d| d.lzi_hygiene.as_ref())
            .and_then(|lh| lh.preset.as_deref())
            .and_then(LziHygienePreset::parse);

        // Merge every per-category override into one map. Today the CLI
        // only reads `[doctor.test_discipline].severity_override` at
        // severity-resolution time; merging the rest here is forward-safe
        // (no code collisions across categories in the closed catalog)
        // and gives the LSP one lookup surface.
        let mut overrides: BTreeMap<String, SeverityOverride> = BTreeMap::new();
        if let Some(d) = doctor {
            for section in [
                d.test_discipline.as_ref().map(|s| &s.severity_override),
                d.internal_hygiene.as_ref().map(|s| &s.severity_override),
                d.error_handling.as_ref().map(|s| &s.severity_override),
                d.lzi_hygiene.as_ref().map(|s| &s.severity_override),
            ]
            .into_iter()
            .flatten()
            {
                for (code, ov) in section {
                    overrides.entry(code.clone()).or_insert_with(|| ov.clone());
                }
            }
        }

        Self {
            profile: ProfileSlot(profile),
            coverage_preset,
            test_discipline_preset,
            internal_hygiene_preset,
            error_handling_preset,
            lzi_hygiene_preset,
            overrides,
        }
    }
}

/// Internal serde shim so [`ResolvedDoctorConfig::resolve`] can pull just
/// the `[doctor]` block out of a full `Lazurite.toml` body, ignoring
/// every other section.
#[derive(serde::Deserialize)]
struct ManifestShim {
    #[serde(default)]
    doctor: Option<Doctor>,
}

/// THE single severity resolver. Pure; identical answer for the CLI and
/// (later) the LSP.
///
/// Precedence (highest first), the exact union of the four scattered CLI
/// resolvers + the coverage escalation map:
///
/// 1. **manifest `severity_override.<code>`** — a parsed override always
///    wins (reason-checking is a separate meta-finding, not folded in
///    here). An unparseable override severity string falls through to
///    the next level, matching the CLI's `parse_doctor_severity` →
///    fall-back behavior.
/// 2. **active coverage-preset escalation** — `preset_severity_overrides`
///    (today: the three `VOCAB-CONTEXT-*` codes under `tdd-iron-hand`).
/// 3. **category preset escalation** — the per-family
///    `<cat>::preset::preset_rule_severity`, dispatched on `category`.
/// 4. **category default per profile** — the historical
///    `doctor_severity_for` match (TestDiscipline carries its own
///    per-profile posture; every other category uses the global
///    profile→severity mapping).
///
/// Returns `None` when the rule is SILENT under this config — today the
/// only silent path is the `Off` coverage preset for the
/// `VOCAB-CONTEXT-*` family (the CLI's `context_vocab_diagnostics`
/// short-circuits to an empty vec under `Off`). Callers emit no
/// diagnostic for `None`.
///
/// `# doctor:allow <CODE>` suppression is ORTHOGONAL and lives at emit
/// time (`lazuli_doctor::allow_comment`), not here.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor_config::{
///     effective_severity, DoctorProfile, DoctorSeverity, ResolvedDoctorConfig, RuleCategory,
/// };
///
/// // Plain strict profile: a vocab rule lands at its default.
/// let cfg = ResolvedDoctorConfig::resolve(None, DoctorProfile::Strict).unwrap();
/// assert_eq!(
///     effective_severity(
///         "VOCAB-TESTS-MISSING-001",
///         DoctorSeverity::Warning,
///         RuleCategory::Vocabulary,
///         &cfg,
///     ),
///     Some(DoctorSeverity::Warning),
/// );
///
/// // Iron-hand coverage preset escalates the VOCAB-CONTEXT family.
/// let toml = "[doctor.coverage]\npreset = \"tdd-iron-hand\"\n";
/// let cfg = ResolvedDoctorConfig::resolve(Some(toml), DoctorProfile::Strict).unwrap();
/// assert_eq!(
///     effective_severity(
///         "VOCAB-CONTEXT-PURPOSE-001",
///         DoctorSeverity::Warning,
///         RuleCategory::Vocabulary,
///         &cfg,
///     ),
///     Some(DoctorSeverity::Error),
/// );
/// ```
pub fn effective_severity(
    code: &str,
    base_severity: DoctorSeverity,
    category: RuleCategory,
    config: &ResolvedDoctorConfig,
) -> Option<DoctorSeverity> {
    // Levels 1-3 — manifest override / coverage escalation / category
    // preset, or `None` when coverage-`Off` suppresses the code.
    match resolve_levels_1_to_3(code, category, config) {
        Resolution::Suppress => None,
        Resolution::Severity(sev) => Some(sev),
        // Level 4 — category default per profile (the
        // `doctor_severity_for` match). Today every category has an
        // opinion, so `base_severity` is the documented "level-4
        // fallback" the LSP and future rules can rely on without losing
        // per-rule calibration.
        Resolution::None => Some(category_default_for_profile(
            category,
            config.profile.0,
            base_severity,
        )),
    }
}

/// Variant of [`effective_severity`] that floors on the rule's intrinsic
/// `base_severity` instead of applying the level-4 per-profile category
/// default.
///
/// This is the resolver for the LSP's `doctor_local` hardcoded-severity
/// bridge (W2). Those file-local doctor codes are emitted by the CLI's
/// aggregators at a hardcoded posture (mostly `Error`, a handful
/// `Warning`) — NOT through the level-4 `category_default_for_profile`
/// match. So the LSP must keep that same intrinsic base as the floor and
/// only let levels 1-3 *move* it:
///
/// 1. **manifest `severity_override.<code>`** — wins absolutely.
/// 2. **coverage-preset escalation** — `tdd-iron-hand` escalates the
///    `VOCAB-CONTEXT-*` family; `off` suppresses it (→ `None`).
/// 3. **category preset escalation** — per-family
///    `preset_rule_severity`.
/// 4. **`base_severity`** — the intrinsic posture, when no override /
///    preset applies. (NOT the profile default — that would clobber an
///    `Error`-base correctness rule down to `Warning` at `strict`,
///    diverging from what `lazuli doctor` emits.)
///
/// Returns `None` only when the code is SILENT under the active config
/// (coverage-`Off`).
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor_config::{
///     effective_severity_over_base, DoctorProfile, DoctorSeverity, ResolvedDoctorConfig,
///     RuleCategory,
/// };
///
/// // No preset: an Error-base correctness rule stays Error at strict
/// // (unlike `effective_severity`, which would return Warning).
/// let cfg = ResolvedDoctorConfig::resolve(None, DoctorProfile::Strict).unwrap();
/// assert_eq!(
///     effective_severity_over_base(
///         "HOOK-TARGET-001",
///         DoctorSeverity::Error,
///         RuleCategory::Correctness,
///         &cfg,
///     ),
///     Some(DoctorSeverity::Error),
/// );
///
/// // Iron-hand coverage preset still escalates the VOCAB-CONTEXT family.
/// let toml = "[doctor.coverage]\npreset = \"tdd-iron-hand\"\n";
/// let cfg = ResolvedDoctorConfig::resolve(Some(toml), DoctorProfile::Strict).unwrap();
/// assert_eq!(
///     effective_severity_over_base(
///         "VOCAB-CONTEXT-PURPOSE-001",
///         DoctorSeverity::Warning,
///         RuleCategory::Vocabulary,
///         &cfg,
///     ),
///     Some(DoctorSeverity::Error),
/// );
///
/// // `off` coverage preset suppresses the family (silent).
/// let toml = "[doctor.coverage]\npreset = \"off\"\n";
/// let cfg = ResolvedDoctorConfig::resolve(Some(toml), DoctorProfile::Strict).unwrap();
/// assert_eq!(
///     effective_severity_over_base(
///         "VOCAB-CONTEXT-PURPOSE-001",
///         DoctorSeverity::Warning,
///         RuleCategory::Vocabulary,
///         &cfg,
///     ),
///     None,
/// );
/// ```
pub fn effective_severity_over_base(
    code: &str,
    base_severity: DoctorSeverity,
    category: RuleCategory,
    config: &ResolvedDoctorConfig,
) -> Option<DoctorSeverity> {
    match resolve_levels_1_to_3(code, category, config) {
        Resolution::Suppress => None,
        Resolution::Severity(sev) => Some(sev),
        Resolution::None => Some(base_severity),
    }
}

/// Outcome of the shared levels-1-to-3 resolution.
enum Resolution {
    /// Code is silent under the active config (coverage-`Off`).
    Suppress,
    /// A level-1/2/3 rule produced a concrete severity.
    Severity(DoctorSeverity),
    /// No level-1/2/3 rule applied; caller supplies the level-4 / base
    /// fallback.
    None,
}

/// Shared resolution of precedence levels 1-3 (override > coverage
/// escalation/suppression > category preset). The level-4 / base
/// fallback differs between [`effective_severity`] (profile default) and
/// [`effective_severity_over_base`] (intrinsic base), so it stays with
/// the callers.
fn resolve_levels_1_to_3(
    code: &str,
    category: RuleCategory,
    config: &ResolvedDoctorConfig,
) -> Resolution {
    // Level 1 — manifest per-rule override wins absolutely (when its
    // severity string parses). Matches `doctor_severity_for` and the
    // `context_vocab_diagnostics` closure.
    if let Some(ov) = config.overrides.get(code) {
        if let Some(parsed) = parse_severity(&ov.severity) {
            return Resolution::Severity(parsed);
        }
    }

    // Level 2 — coverage-preset escalation map. The `Off` preset
    // suppresses the VOCAB-CONTEXT family entirely (silent → Suppress),
    // mirroring the CLI short-circuit at
    // `context_vocab_diagnostics` (`package_methods.rs`).
    if let Some(preset) = config.coverage_preset {
        if matches!(preset, CoveragePreset::Off) && is_coverage_preset_governed(code) {
            return Resolution::Suppress;
        }
        let escalations = preset_severity_overrides(preset);
        if let Some(sev_str) = escalations.get(code) {
            if let Some(parsed) = parse_severity(sev_str) {
                return Resolution::Severity(parsed);
            }
        }
    }

    // Level 3 — per-category preset escalation. Dispatched on the
    // explicit category so a code only ever consults the preset for its
    // own family (each `preset_rule_severity` already prefix-guards).
    if let Some(sev) = category_preset_severity(code, category, config) {
        return Resolution::Severity(sev);
    }

    Resolution::None
}

/// `true` when the rule code is one the coverage preset escalation map
/// governs (today: the three `VOCAB-CONTEXT-*` codes). Used to reproduce
/// the CLI's `Off`-preset short-circuit, which suppresses exactly these
/// codes.
fn is_coverage_preset_governed(code: &str) -> bool {
    matches!(
        code,
        "VOCAB-CONTEXT-PURPOSE-001" | "VOCAB-CONTEXT-NONGOALS-001" | "VOCAB-CONTEXT-CTXMD-001"
    )
}

/// Resolve the per-category preset escalation for `code` under the
/// active config — precedence level 3 of [`effective_severity`] in
/// isolation. Returns `None` when no category preset is active or the
/// preset has no opinion on this code, in which case the caller keeps
/// its per-rule default.
///
/// Exposed so call sites that historically applied ONLY the category
/// preset escalation over a per-rule default (the CLI's four
/// `resolve_<cat>_severity` helpers) can share this exact logic instead
/// of re-implementing it. Those sites deliberately do NOT consult the
/// profile default, so they call this rather than [`effective_severity`].
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor_config::{
///     category_preset_severity, DoctorProfile, DoctorSeverity, ResolvedDoctorConfig, RuleCategory,
/// };
///
/// let toml = "[doctor.test_discipline]\npreset = \"tdd-iron-hand\"\n";
/// let cfg = ResolvedDoctorConfig::resolve(Some(toml), DoctorProfile::Strict).unwrap();
/// assert_eq!(
///     category_preset_severity("TEST-MISSING-AUTHORED-001", RuleCategory::TestDiscipline, &cfg),
///     Some(DoctorSeverity::Error),
/// );
/// // No preset active -> None (caller keeps its per-rule default).
/// let bare = ResolvedDoctorConfig::resolve(None, DoctorProfile::Strict).unwrap();
/// assert_eq!(
///     category_preset_severity("TEST-MISSING-AUTHORED-001", RuleCategory::TestDiscipline, &bare),
///     None,
/// );
/// ```
pub fn category_preset_severity(
    code: &str,
    category: RuleCategory,
    config: &ResolvedDoctorConfig,
) -> Option<DoctorSeverity> {
    match category {
        RuleCategory::TestDiscipline => config
            .test_discipline_preset
            .and_then(|p| test_discipline_preset_rule_severity(p, code)),
        RuleCategory::InternalHygiene => config
            .internal_hygiene_preset
            .and_then(|p| internal_hygiene_preset_rule_severity(p, code)),
        RuleCategory::ErrorHandling => config
            .error_handling_preset
            .and_then(|p| error_handling_preset_rule_severity(p, code)),
        RuleCategory::LziHygiene => config
            .lzi_hygiene_preset
            .and_then(|p| lzi_hygiene_preset_rule_severity(p, code)),
        _ => None,
    }
}

/// The historical `doctor_severity_for` category-default match: the
/// per-profile posture each category falls back to when no override and
/// no preset escalation applies.
///
/// `base_severity` is the documented level-4 fallback for categories
/// that carry no per-profile opinion. The closed catalog today gives
/// every category an opinion, so `base_severity` is currently
/// unreachable via this match — but it preserves the per-rule
/// calibration the LSP's hardcoded literals carry, so it is wired
/// through rather than dropped.
fn category_default_for_profile(
    category: RuleCategory,
    profile: DoctorProfile,
    _base_severity: DoctorSeverity,
) -> DoctorSeverity {
    match (category, profile) {
        // Test-discipline rules carry their own per-profile posture.
        (RuleCategory::TestDiscipline, DoctorProfile::Production) => DoctorSeverity::Error,
        (RuleCategory::TestDiscipline, DoctorProfile::Strict) => DoctorSeverity::Warning,
        (RuleCategory::TestDiscipline, DoctorProfile::Prototype) => DoctorSeverity::Info,
        // Everything else: the legacy global mapping.
        (_, DoctorProfile::Production) => DoctorSeverity::Error,
        (_, DoctorProfile::Prototype | DoctorProfile::Strict) => DoctorSeverity::Warning,
    }
}

/// Parse a TOML override / preset-map severity string into a
/// [`DoctorSeverity`]. Mirrors the CLI's `parse_doctor_severity`:
/// `"error" | "warning" | "warn" | "info" | "hint"`, case-insensitive;
/// `None` for anything else so callers fall back to the next precedence
/// level.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor_config::{parse_severity, DoctorSeverity};
///
/// assert_eq!(parse_severity("Error"), Some(DoctorSeverity::Error));
/// assert_eq!(parse_severity("warn"), Some(DoctorSeverity::Warning));
/// assert_eq!(parse_severity("bogus"), None);
/// ```
pub fn parse_severity(s: &str) -> Option<DoctorSeverity> {
    match s.to_ascii_lowercase().as_str() {
        "error" => Some(DoctorSeverity::Error),
        "warning" | "warn" => Some(DoctorSeverity::Warning),
        "info" => Some(DoctorSeverity::Info),
        "hint" => Some(DoctorSeverity::Hint),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_strict() -> ResolvedDoctorConfig {
        ResolvedDoctorConfig::resolve(None, DoctorProfile::Strict).unwrap()
    }

    #[test]
    fn profile_round_trips() {
        for p in [
            DoctorProfile::Prototype,
            DoctorProfile::Strict,
            DoctorProfile::Production,
        ] {
            assert_eq!(DoctorProfile::parse(p.as_str()), Some(p));
        }
    }

    #[test]
    fn category_default_matches_doctor_severity_for() {
        // Vocabulary: strict -> warning, production -> error.
        let strict = empty_strict();
        assert_eq!(
            effective_severity(
                "VOCAB-X-001",
                DoctorSeverity::Warning,
                RuleCategory::Vocabulary,
                &strict
            ),
            Some(DoctorSeverity::Warning),
        );
        let prod = ResolvedDoctorConfig::resolve(None, DoctorProfile::Production).unwrap();
        assert_eq!(
            effective_severity(
                "VOCAB-X-001",
                DoctorSeverity::Warning,
                RuleCategory::Vocabulary,
                &prod
            ),
            Some(DoctorSeverity::Error),
        );
        // TestDiscipline carries its own per-profile posture.
        let proto = ResolvedDoctorConfig::resolve(None, DoctorProfile::Prototype).unwrap();
        assert_eq!(
            effective_severity(
                "TEST-X-001",
                DoctorSeverity::Warning,
                RuleCategory::TestDiscipline,
                &proto
            ),
            Some(DoctorSeverity::Info),
        );
    }

    #[test]
    fn manifest_override_wins() {
        let toml = r#"
[doctor.test_discipline.severity_override."VOCAB-CONTEXT-CTXMD-001"]
severity = "warning"
reason = "backfill scheduled"

[doctor.coverage]
preset = "tdd-iron-hand"
"#;
        let cfg = ResolvedDoctorConfig::resolve(Some(toml), DoctorProfile::Strict).unwrap();
        // Coverage iron-hand would escalate to error, but the manifest
        // override downgrades it back to warning.
        assert_eq!(
            effective_severity(
                "VOCAB-CONTEXT-CTXMD-001",
                DoctorSeverity::Warning,
                RuleCategory::Vocabulary,
                &cfg
            ),
            Some(DoctorSeverity::Warning),
        );
    }

    #[test]
    fn coverage_preset_escalates_vocab_context() {
        let toml = "[doctor.coverage]\npreset = \"tdd-iron-hand\"\n";
        let cfg = ResolvedDoctorConfig::resolve(Some(toml), DoctorProfile::Strict).unwrap();
        for code in [
            "VOCAB-CONTEXT-PURPOSE-001",
            "VOCAB-CONTEXT-NONGOALS-001",
            "VOCAB-CONTEXT-CTXMD-001",
        ] {
            assert_eq!(
                effective_severity(
                    code,
                    DoctorSeverity::Warning,
                    RuleCategory::Vocabulary,
                    &cfg
                ),
                Some(DoctorSeverity::Error),
                "{code} should escalate under iron-hand coverage preset",
            );
        }
    }

    #[test]
    fn off_coverage_preset_suppresses_vocab_context() {
        let toml = "[doctor.coverage]\npreset = \"off\"\n";
        let cfg = ResolvedDoctorConfig::resolve(Some(toml), DoctorProfile::Strict).unwrap();
        assert_eq!(
            effective_severity(
                "VOCAB-CONTEXT-PURPOSE-001",
                DoctorSeverity::Warning,
                RuleCategory::Vocabulary,
                &cfg
            ),
            None,
        );
        // Non-governed codes still resolve normally under `off`.
        assert_eq!(
            effective_severity(
                "VOCAB-TESTS-MISSING-001",
                DoctorSeverity::Warning,
                RuleCategory::Vocabulary,
                &cfg
            ),
            Some(DoctorSeverity::Warning),
        );
    }

    #[test]
    fn category_preset_escalation() {
        // test_discipline iron-hand -> Error.
        let toml = "[doctor.test_discipline]\npreset = \"tdd-iron-hand\"\n";
        let cfg = ResolvedDoctorConfig::resolve(Some(toml), DoctorProfile::Strict).unwrap();
        assert_eq!(
            effective_severity(
                "TEST-MISSING-AUTHORED-001",
                DoctorSeverity::Warning,
                RuleCategory::TestDiscipline,
                &cfg
            ),
            Some(DoctorSeverity::Error),
        );
        // error_handling off -> Info.
        let toml = "[doctor.error_handling]\npreset = \"off\"\n";
        let cfg = ResolvedDoctorConfig::resolve(Some(toml), DoctorProfile::Strict).unwrap();
        assert_eq!(
            effective_severity(
                "HANDLER-NO-PANIC-001",
                DoctorSeverity::Error,
                RuleCategory::ErrorHandling,
                &cfg
            ),
            Some(DoctorSeverity::Info),
        );
    }
}
