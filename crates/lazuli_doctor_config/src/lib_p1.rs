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
    /// One-knob meta-bundle. Behaves exactly like [`Production`](Self::Production)
    /// for the global profile→severity escalation, AND additionally
    /// defaults EVERY discipline family (coverage, test_discipline,
    /// error_handling, lzi_hygiene, internal_hygiene) to its
    /// `tdd-iron-hand` preset when the corresponding `[doctor.<family>]`
    /// block is absent. So `[doctor] profile = "iron-hand"` alone reproduces
    /// the full six-block iron-hand stance (coverage 90/95 gating +
    /// `VOCAB-CONTEXT-*` → error + every hygiene/test rule at Error). A
    /// per-family `[doctor.<family>] preset` still overrides the default.
    IronHand,
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
            "iron-hand" => Some(Self::IronHand),
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
            Self::IronHand => "iron-hand",
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
        // The iron-hand meta-bundle: when `profile = "iron-hand"` and a
        // discipline family declares no `[doctor.<family>] preset`, default
        // that family to its `tdd-iron-hand` preset. An explicit per-family
        // preset still wins (the six-block form stays the low-level escape
        // hatch). For every other profile the default stays `None`, so
        // behavior is byte-identical to before.
        let iron_hand = profile == DoctorProfile::IronHand;

        let coverage_preset = doctor
            .and_then(|d| d.coverage.as_ref())
            .and_then(|c| c.preset.as_deref())
            .and_then(CoveragePreset::parse)
            .or_else(|| iron_hand.then(|| CoveragePreset::parse("tdd-iron-hand")).flatten());
        let test_discipline_preset = doctor
            .and_then(|d| d.test_discipline.as_ref())
            .and_then(|td| td.preset.as_deref())
            .and_then(TestDisciplinePreset::parse)
            .or_else(|| {
                iron_hand
                    .then(|| TestDisciplinePreset::parse("tdd-iron-hand"))
                    .flatten()
            });
        let internal_hygiene_preset = doctor
            .and_then(|d| d.internal_hygiene.as_ref())
            .and_then(|ih| ih.preset.as_deref())
            .and_then(InternalHygienePreset::parse)
            .or_else(|| {
                iron_hand
                    .then(|| InternalHygienePreset::parse("tdd-iron-hand"))
                    .flatten()
            });
        let error_handling_preset = doctor
            .and_then(|d| d.error_handling.as_ref())
            .and_then(|eh| eh.preset.as_deref())
            .and_then(ErrorHandlingPreset::parse)
            .or_else(|| {
                iron_hand
                    .then(|| ErrorHandlingPreset::parse("tdd-iron-hand"))
                    .flatten()
            });
        let lzi_hygiene_preset = doctor
            .and_then(|d| d.lzi_hygiene.as_ref())
            .and_then(|lh| lh.preset.as_deref())
            .and_then(LziHygienePreset::parse)
            .or_else(|| {
                iron_hand
                    .then(|| LziHygienePreset::parse("tdd-iron-hand"))
                    .flatten()
            });

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
