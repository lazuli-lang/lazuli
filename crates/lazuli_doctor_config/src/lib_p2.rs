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
        // iron-hand inherits production's posture (warnings → errors).
        (RuleCategory::TestDiscipline, DoctorProfile::Production | DoctorProfile::IronHand) => {
            DoctorSeverity::Error
        }
        (RuleCategory::TestDiscipline, DoctorProfile::Strict) => DoctorSeverity::Warning,
        (RuleCategory::TestDiscipline, DoctorProfile::Prototype) => DoctorSeverity::Info,
        // Everything else: the legacy global mapping. iron-hand == production.
        (_, DoctorProfile::Production | DoctorProfile::IronHand) => DoctorSeverity::Error,
        (_, DoctorProfile::Prototype | DoctorProfile::Strict) => DoctorSeverity::Warning,
    }
}
