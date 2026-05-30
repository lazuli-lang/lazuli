/// Parsed `Lazurite.toml` — the single typed surface the CLI builds
/// against. Every other module in the crate accepts an
/// `Option<&Manifest>` so default-shaped projects keep working without
/// a manifest file.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Manifest {
    /// `[project]` block (name + version).
    pub project: Project,
    /// `[lazuli]` runtime pin.
    pub lazuli: LazuliPin,
    /// Optional `[lazurite]` block (app_dir + extras).
    pub lazurite: Option<Lazurite>,
    #[serde(default)]
    pub plugins: BTreeMap<String, Plugin>,
    #[serde(default)]
    pub generate: Generate,
    #[serde(default)]
    pub frontends: BTreeMap<String, Frontend>,
    pub migrations: Option<Migrations>,
    pub seeds: Option<Seeds>,
    pub dev: Option<DevOverrides>,
    /// Wave 0.5 + Wave 6 — optional `[doctor]` block. Carries severity
    /// overrides per category (Wave 0.5) and per-layer coverage thresholds
    /// (Wave 6). Absent on most projects today; when present
    /// `DOCTOR-OVERRIDE-NEEDS-REASON-001` enforces `reason = "..."` on every
    /// severity override entry. See
    /// `docs/proposals/tdd-bdd-first-2026-05-23.md` §Wave 0.5 + Wave 6.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doctor: Option<Doctor>,
    /// Optional `[knowledge]` block — declares CUSTOM knowledge sectors on
    /// top of the closed core catalog (`decisions`, `changes`, `gaps`,
    /// `lazuli-way`). Absent on most projects, which then rely solely on
    /// the core catalog + on-disk `knowledge/<sector>/` folders.
    /// `VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001` reads
    /// `[knowledge.sectors]` as the "declared" leg of its known-sector
    /// check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge: Option<Knowledge>,
    /// Sibling T0-T5 — `[testing]` section consumed by `lazuli test`.
    /// Optional; when absent the runner falls back to conventional
    /// discovery. Schema per
    /// `docs/proposals/lazuli-test-runner-2026-05-24.md` §3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub testing: Option<Testing>,
}

/// Canonical manifest filename. Capitalized following the Cargo
/// convention (`Cargo.toml`, `Cargo.lock`) — the capital signals
/// "this is THE manifest file" in a file tree. The lowercase
/// `lazurite.toml` form is also accepted during the migration window;
/// new projects emit the capitalized form.
pub const MANIFEST_FILENAME: &str = "Lazurite.toml";

/// Legacy lowercase filename. Recognized for back-compat with projects
/// scaffolded before the rename (2026-05-15). When found, `load()`
/// reads it transparently; future `lazuli upgrade` step will rename.
pub const LEGACY_MANIFEST_FILENAME: &str = "lazurite.toml";

/// Read `Lazurite.toml` (or the legacy lowercase `lazurite.toml`)
/// from `project_root`. Returns `Ok(None)` when no manifest exists —
/// pilots without a manifest run with default-shaped behaviour.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::lazurite_manifest::load;
///
/// // let manifest = load(Path::new("."))?;
/// ```
pub fn load(project_root: &Path) -> Result<Option<Manifest>, ManifestError> {
    // Prefer the canonical capitalized name; fall back to the legacy
    // lowercase form. On filesystems that are case-insensitive (NTFS,
    // APFS default) both `exists()` calls return true for a single
    // file — that's fine, we always read the canonical path first.
    let canonical = project_root.join(MANIFEST_FILENAME);
    let legacy = project_root.join(LEGACY_MANIFEST_FILENAME);
    let path = if canonical.exists() {
        canonical
    } else if legacy.exists() {
        legacy
    } else {
        return Ok(None);
    };

    let contents = std::fs::read_to_string(&path)?;
    let manifest: Manifest = toml::from_str(&contents)?;
    manifest.validate()?;
    Ok(Some(manifest))
}

/// Resolve `<project_root>/<app_dir>/<file>`, where `<app_dir>` comes
/// from `Lazurite.toml`'s `[lazurite] app_dir` field. Falls back to
/// `<project_root>/<file>` when no manifest exists or no `app_dir` is
/// set — preserving the original convention where Lazuli sources lived
/// at the project root.
///
/// Used by source-loaders (generate, inspect, doctor) that need to find
/// `app.lzi`, `design.lzi`, or `registry.lzi` without each callsite
/// re-implementing manifest-aware resolution.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::lazurite_manifest::resolve_in_app_dir;
///
/// let p = resolve_in_app_dir(Path::new("."), "app.lzi");
/// assert!(p.ends_with("app.lzi"));
/// ```
pub fn resolve_in_app_dir(project_root: &Path, file: &str) -> std::path::PathBuf {
    match load(project_root).ok().flatten() {
        Some(manifest) => manifest.app_root(project_root).join(file),
        None => project_root.join(file),
    }
}

impl Manifest {
    /// Read the pinned `lazuli.runtime` version (`Some(version_str)`
    /// when the manifest sets it; `None` otherwise — the loader
    /// ensures the field exists today, so callers can treat `None`
    /// as a corrupt-manifest signal).
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let v = manifest.lazuli_runtime_version();
    /// ```
    pub fn lazuli_runtime_version(&self) -> Option<&str> {
        Some(self.lazuli.runtime.as_str())
    }

    /// Resolved path to the directory containing `app.lzi`, `design.lzi`,
    /// and `registry.lzi`. Returns `project_root.join(app_dir)` when
    /// `[lazurite] app_dir` is set, otherwise `project_root` itself
    /// (backwards-compatible default).
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::Path;
    /// use lazuli_cli::lazurite_manifest::Manifest;
    ///
    /// // let dir = manifest.app_root(Path::new("."));
    /// ```
    pub fn app_root(&self, project_root: &Path) -> std::path::PathBuf {
        match self.lazurite.as_ref().and_then(|l| l.app_dir.as_deref()) {
            Some(subdir) => project_root.join(subdir),
            None => project_root.to_path_buf(),
        }
    }

    /// Frente 1 — resolve `[generate.go]` with canonical defaults
    /// applied when the block is omitted. Pilots that follow the
    /// canonical layout can skip the section entirely.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let go = manifest.generate_go_or_default();
    /// ```
    pub fn generate_go_or_default(&self) -> GenerateGo {
        self.generate.go.clone().unwrap_or_default()
    }

    /// Frente 1 — resolve `[migrations]` with canonical defaults
    /// applied when the block is omitted.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let m = manifest.migrations_or_default();
    /// ```
    pub fn migrations_or_default(&self) -> Migrations {
        self.migrations.clone().unwrap_or_default()
    }

    /// Frente 1 — resolve `[seeds]` with canonical defaults applied
    /// when the block is omitted.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let s = manifest.seeds_or_default();
    /// ```
    pub fn seeds_or_default(&self) -> Seeds {
        self.seeds.clone().unwrap_or_default()
    }

    /// Frente 1 — resolve effective `[testing.ts]` config. Authored
    /// `[testing.ts]` (or missing fields filled with layout-derived
    /// canonical defaults) > pure layout defaults > `None`.
    ///
    /// Layout-derived defaults (when `detect_frontend_layout` resolves):
    /// - `runner = "vitest"`
    /// - `config  = "<layout>/vite.config.ts"`
    /// - `discovery_root = "<layout>/src"`
    ///
    /// Returns `None` only when the project is neither in the canonical
    /// layout nor declares `[testing.ts]` (back-compat).
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::Path;
    /// // let ts = manifest.testing_ts_resolved(Path::new("."));
    /// ```
    pub fn testing_ts_resolved(&self, project_root: &Path) -> Option<TestingTs> {
        let authored = self.testing.as_ref().and_then(|t| t.ts.as_ref());
        let layout = self.detect_frontend_layout(project_root);
        match (authored, layout) {
            (Some(cfg), layout) => {
                let mut cfg = cfg.clone();
                if cfg.config.is_none() {
                    if let Some(l) = layout.as_ref() {
                        cfg.config = Some(format!("{l}/vite.config.ts"));
                    }
                }
                if cfg.discovery_root.is_none() {
                    if let Some(l) = layout.as_ref() {
                        cfg.discovery_root = Some(format!("{l}/src"));
                    }
                }
                Some(cfg)
            }
            (None, Some(l)) => Some(TestingTs {
                runner: "vitest".to_string(),
                flags: Vec::new(),
                config: Some(format!("{l}/vite.config.ts")),
                discovery_root: Some(format!("{l}/src")),
                coverage: false,
            }),
            (None, None) => None,
        }
    }

    /// Frente 1 — resolve effective `[testing.playwright]` config.
    /// Same precedence as `testing_ts_resolved`. Layout-derived
    /// defaults:
    /// - `config = "<layout>/playwright.config.ts"`
    /// - `discovery_root = "<layout>/e2e"`
    /// - `workers = Some(4)`
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::Path;
    /// // let pw = manifest.testing_playwright_resolved(Path::new("."));
    /// ```
    pub fn testing_playwright_resolved(&self, project_root: &Path) -> Option<TestingPlaywright> {
        let authored = self.testing.as_ref().and_then(|t| t.playwright.as_ref());
        let layout = self.detect_frontend_layout(project_root);
        match (authored, layout) {
            (Some(cfg), layout) => {
                let mut cfg = cfg.clone();
                if cfg.config.is_none() {
                    if let Some(l) = layout.as_ref() {
                        cfg.config = Some(format!("{l}/playwright.config.ts"));
                    }
                }
                if cfg.discovery_root.is_none() {
                    if let Some(l) = layout.as_ref() {
                        cfg.discovery_root = Some(format!("{l}/e2e"));
                    }
                }
                if cfg.workers.is_none() {
                    cfg.workers = Some(4);
                }
                Some(cfg)
            }
            (None, Some(l)) => Some(TestingPlaywright {
                config: Some(format!("{l}/playwright.config.ts")),
                workers: Some(4),
                project: None,
                discovery_root: Some(format!("{l}/e2e")),
                flags: Vec::new(),
            }),
            (None, None) => None,
        }
    }

    /// Frente 1 — `[testing] default_layers` with canonical default
    /// `["handler_go", "view_extensibility"]` applied when the field
    /// (or the entire `[testing]` block) is missing.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let layers = manifest.testing_default_layers();
    /// ```
    pub fn testing_default_layers(&self) -> Vec<String> {
        self.testing
            .as_ref()
            .and_then(|t| t.default_layers.clone())
            .unwrap_or_else(|| vec!["handler_go".to_string(), "view_extensibility".to_string()])
    }

    /// Frente 1 — detect the canonical frontend layout. Returns
    /// `Some("app/web")` when the singular scaffold layout is in use,
    /// `Some("app/clients/<name>")` when the multi-client layout is
    /// in use (and exactly one client exists), `None` otherwise.
    ///
    /// Used to apply default `[testing.ts]` / `[testing.playwright]`
    /// config + discovery_root paths so pilots on the canonical layout
    /// don't need to author either block.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::Path;
    /// // let layout = manifest.detect_frontend_layout(Path::new("."));
    /// ```
    pub fn detect_frontend_layout(&self, project_root: &Path) -> Option<String> {
        let singular = project_root.join("app").join("web");
        if singular.is_dir() {
            return Some("app/web".to_string());
        }
        let clients = project_root.join("app").join("clients");
        if !clients.is_dir() {
            return None;
        }
        // Only auto-detect when exactly one client dir exists; with
        // multiple clients the pilot must spell out which to use.
        let mut entries: Vec<String> = std::fs::read_dir(&clients)
            .ok()?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        entries.sort();
        if entries.len() == 1 {
            return Some(format!("app/clients/{}", entries[0]));
        }
        None
    }

    /// Declared custom knowledge sectors from `[knowledge.sectors]`, in
    /// deterministic order. Empty when the `[knowledge]` block is absent.
    /// Threaded to `VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001` as the "declared"
    /// leg of its known-sector check (alongside the closed core catalog
    /// and the on-disk folder leg).
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let sectors = manifest.declared_knowledge_sectors();
    /// ```
    pub fn declared_knowledge_sectors(&self) -> Vec<String> {
        self.knowledge
            .as_ref()
            .map(Knowledge::declared_sectors)
            .unwrap_or_default()
    }

    /// Run the post-parse manifest invariants:
    /// `project.schema == 1`, every `[plugins.<key>]` starts with the
    /// `@lazuli/plugin-` namespace, and no two `[frontends.<name>]`
    /// blocks share an `out` directory.
    ///
    /// Called by [`load`] before returning a manifest so callers never
    /// see an invalid `Manifest`.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // manifest.validate()?;
    /// ```
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.project.schema != 1 {
            return Err(ManifestError::UnsupportedSchema(self.project.schema));
        }

        for key in self.plugins.keys() {
            if !key.starts_with("@lazuli/plugin-") {
                return Err(ManifestError::InvalidPluginNamespace(key.clone()));
            }
        }

        let mut seen_outs = HashSet::new();
        for (name, frontend) in &self.frontends {
            if !seen_outs.insert(frontend.out.as_str()) {
                return Err(ManifestError::FrontendOutCollision(
                    name.clone(),
                    frontend.out.clone(),
                ));
            }
        }

        Ok(())
    }

    /// Project the manifest into the borrow-friendly
    /// [`InspectManifest`] shape consumed by `lazuli inspect
    /// --format=json`. Flattens the `plugins` + `frontends` maps so
    /// the JSON output stays stable across schema iterations.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let view = manifest.inspect_view();
    /// ```
    pub fn inspect_view(&self) -> InspectManifest<'_> {
        InspectManifest {
            origin: MANIFEST_FILENAME,
            project: &self.project,
            lazuli: &self.lazuli,
            lazurite: self.lazurite.as_ref(),
            plugins: self
                .plugins
                .iter()
                .map(|(plugin_ref, plugin)| match plugin {
                    Plugin::Remote { module, version } => InspectPlugin {
                        plugin_ref,
                        module: Some(module),
                        version: Some(version),
                        path: None,
                        source: "remote",
                    },
                    Plugin::Local { path } => InspectPlugin {
                        plugin_ref,
                        module: None,
                        version: None,
                        path: Some(path),
                        source: "local",
                    },
                })
                .collect(),
            generate: &self.generate,
            frontends: self
                .frontends
                .iter()
                .map(|(name, frontend)| InspectFrontend {
                    name,
                    target: &frontend.target,
                    source: frontend.source.as_deref(),
                    out: &frontend.out,
                    audiences: &frontend.audiences,
                })
                .collect(),
            migrations: self.migrations.as_ref(),
            seeds: self.seeds.as_ref(),
            dev: self.dev.as_ref(),
        }
    }
}
