/// One finding of `MIGRATION-ALTER-MISSING-001`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.lzi` file the resource was authored in (diagnostic anchor).
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Resource name within the feature.
    pub resource: String,
    /// Why the rule fired — drives the message renderer.
    pub kind: FindingKind,
}

/// Variant payloads for [`Finding`]. The rule's two failure modes:
///
/// - [`FindingKind::MissingAlter`] — IR has columns not present in
///   `baseline ∪ forward_additions`. The high-confidence case.
/// - [`FindingKind::UnrecognisedMigration`] — the rule encountered
///   an `ALTER TABLE` form it could not parse (multi-column ADD,
///   transaction-wrapped). The rule's confidence is reduced; the
///   author audits manually. Per the proposal §"False-positive cases".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingKind {
    /// IR has columns the deployment history will never apply.
    MissingAlter {
        /// Path of the baseline migration (the original CREATE TABLE).
        baseline_migration: PathBuf,
        /// Column names in IR but not added by any ALTER migration.
        /// Sorted ASCII-ascending for stable diagnostics.
        missing: Vec<String>,
    },
    /// A migration carried an `ALTER TABLE` shape the parser could not
    /// decode. Confidence in the main check is reduced for this
    /// resource; emit the warning so the author can opt-out with a
    /// reason or reshape the migration.
    UnrecognisedMigration {
        /// Path of the unparseable migration file.
        migration: PathBuf,
        /// Short text snippet showing the unrecognised line.
        snippet: String,
    },
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "MIGRATION-ALTER-MISSING-001";

    /// Default severity per the doctor profile defaults. The CLI
    /// dispatcher may override per `--profile`. v0.1 returns `Warning`
    /// (the `strict` default); production callers should escalate to
    /// `Error` and prototype callers may downgrade to `Info`.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::correctness::migration_alter_missing_001::Finding;
    /// use lazuli_doctor::DoctorSeverity;
    ///
    /// assert_eq!(Finding::default_severity(), DoctorSeverity::Warning);
    /// ```
    pub fn default_severity() -> DoctorSeverity {
        DoctorSeverity::Warning
    }

    /// Render the finding's diagnostic prose.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::migration_alter_missing_001::{Finding, FindingKind};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("account.lzi"),
    ///     feature: "account".into(),
    ///     resource: "User".into(),
    ///     kind: FindingKind::MissingAlter {
    ///         baseline_migration: PathBuf::from("dist/go/migrations/004_account_user.sql"),
    ///         missing: vec!["updated_at".to_string()],
    ///     },
    /// };
    /// assert!(f.message().contains("updated_at"));
    /// assert!(f.message().contains("lazuli generate go ."));
    /// ```
    pub fn message(&self) -> String {
        match &self.kind {
            FindingKind::MissingAlter {
                baseline_migration,
                missing,
            } => format!(
                "resource '{feature}.{resource}' declares column(s) [{cols}] in the IR \
                 that no ALTER TABLE migration ever adds. Baseline at {baseline} has \
                 already been applied in production; editing it will not propagate. \
                 Run `lazuli generate go .` to emit an ALTER TABLE migration; do NOT \
                 modify the baseline file.",
                feature = self.feature,
                resource = self.resource,
                cols = missing.join(", "),
                baseline = baseline_migration.display(),
            ),
            FindingKind::UnrecognisedMigration { migration, snippet } => format!(
                "resource '{feature}.{resource}' has migration {migration} with an \
                 ALTER TABLE shape the doctor cannot parse ({snippet}). The \
                 MIGRATION-ALTER-MISSING-001 check is skipped for this resource. \
                 Reshape the migration to single-column ALTER ADD COLUMN, or add \
                 `# doctor:allow MIGRATION-ALTER-MISSING-001 — reason \"...\"` to \
                 the resource's .lzi after manual verification.",
                feature = self.feature,
                resource = self.resource,
                migration = migration.display(),
                snippet = snippet,
            ),
        }
    }
}

// ── public API ───────────────────────────────────────────────────────────────

/// Run `MIGRATION-ALTER-MISSING-001` for every resource in `feature`,
/// anchored at the capsule root `root` (the directory that holds
/// `dist/go/migrations/`).
///
/// `feature_path` is the diagnostic anchor (typically the `.lzi`
/// file). It is also scanned for the `# doctor:allow
/// MIGRATION-ALTER-MISSING-001` opt-out — when present, no findings
/// are emitted for any resource in this feature file.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::migration_alter_missing_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with resources");
/// let _ = check(&feature, Path::new("account.lzi"), Path::new("/app"));
/// ```
pub fn check(feature: &Feature, feature_path: &Path, root: &Path) -> Vec<Finding> {
    // Whole-file opt-out — silence ALL findings from this feature
    // when the `.lzi` carries the canonical allow comment.
    if file_contains_doctor_allow(feature_path, Finding::CODE) {
        return Vec::new();
    }

    let migrations_dir = root.join("dist").join("go").join("migrations");
    let entries = list_migration_entries(&migrations_dir);
    let feature_slug = lower_snake(&feature.name);

    let mut out = Vec::new();
    for resource in &feature.resources {
        let resource_slug = lower_snake(&resource.name);
        let prefix = format!("{feature_slug}_{resource_slug}");
        let table = lower_snake(&resource.name);

        // Filter + sort migrations by NNN ascending.
        let mut owned: Vec<&MigrationEntry> = entries
            .iter()
            .filter(|e| migration_matches(&e.stem, &prefix))
            .collect();
        owned.sort_by_key(|e| e.number);

        // No migrations for the resource — sibling rule
        // (@correctness.migration_out_of_sync) owns MigrationMissing.
        let Some(baseline) = owned.first() else {
            continue;
        };

        let Ok(baseline_sql) = fs::read_to_string(&baseline.path) else {
            continue;
        };
        let baseline_cols = parse_create_table_columns(&baseline_sql, &table);
        let mut deployed: BTreeSet<String> = baseline_cols;

        // Walk subsequent migrations for ALTER ADD COLUMN additions.
        // If we see an `ALTER TABLE` shape we cannot parse for this
        // table, emit UnrecognisedMigration and skip the MissingAlter
        // check (reduced confidence — proposal §"False-positive cases").
        let mut unrecognised: Option<(PathBuf, String)> = None;
        for entry in &owned[1..] {
            let Ok(sql) = fs::read_to_string(&entry.path) else {
                continue;
            };
            match parse_alter_add_columns(&sql, &table) {
                AlterParseResult::Parsed(cols) => {
                    for c in cols {
                        deployed.insert(c);
                    }
                }
                AlterParseResult::Unrecognised(snippet) => {
                    unrecognised = Some((entry.path.clone(), snippet));
                    break;
                }
            }
        }

        if let Some((path, snippet)) = unrecognised {
            out.push(Finding {
                path: feature_path.to_path_buf(),
                feature: feature.name.clone(),
                resource: resource.name.clone(),
                kind: FindingKind::UnrecognisedMigration {
                    migration: path,
                    snippet,
                },
            });
            continue;
        }

        let ir_cols = expected_columns_for(feature, resource);
        let missing: Vec<String> = ir_cols.difference(&deployed).cloned().collect();
        if missing.is_empty() {
            continue;
        }

        out.push(Finding {
            path: feature_path.to_path_buf(),
            feature: feature.name.clone(),
            resource: resource.name.clone(),
            kind: FindingKind::MissingAlter {
                baseline_migration: baseline.path.clone(),
                missing,
            },
        });
    }
    out
}

// ── migration directory walk ────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct MigrationEntry {
    path: PathBuf,
    number: u32,
    stem: String,
}

fn list_migration_entries(dir: &Path) -> Vec<MigrationEntry> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_owned(),
            None => continue,
        };
        if !name.ends_with(".sql") || name.ends_with(".down.sql") {
            continue;
        }
        let trimmed = name.trim_end_matches(".sql");
        let Some((number_text, stem)) = trimmed.split_once('_') else {
            continue;
        };
        let Ok(number) = number_text.parse::<u32>() else {
            continue;
        };
        out.push(MigrationEntry {
            path,
            number,
            stem: stem.to_owned(),
        });
    }
    out
}

/// Owner-test mirrors [`crate::correctness::schema_migration_present::
/// migration_matches`]. Match when the stem is exactly the prefix
/// (baseline shape) or carries a non-empty `_<tail>` (ALTER shape).
fn migration_matches(stem: &str, prefix: &str) -> bool {
    if stem == prefix {
        return true;
    }
    if let Some(rest) = stem.strip_prefix(&format!("{prefix}_"))
        && !rest.is_empty() {
            return true;
        }
    false
}

// ── IR → expected column names (mirror schema_migration_present) ────────────

fn expected_columns_for(feature: &Feature, resource: &Resource) -> BTreeSet<String> {
    let mut cols = BTreeSet::new();

    let composite_primary = resource.composite_key.as_ref().is_some_and(|ck| ck.primary);
    if !composite_primary {
        cols.insert("id".to_string());
    }

    let tenancy = resource
        .tenancy
        .clone()
        .or_else(|| feature.defaults.tenancy.clone());
    if matches!(tenancy, Some(Tenancy::Org)) {
        cols.insert("org_id".to_string());
    }

    let explicit_currency_overrides: BTreeSet<String> = resource
        .fields
        .iter()
        .filter_map(|f| {
            if matches!(f.type_ref, TypeRef::Builtin(BuiltinType::SemanticCurrency)) {
                f.name.strip_suffix("_currency").map(|stem| stem.to_owned())
            } else {
                None
            }
        })
        .collect();

    for field in &resource.fields {
        if matches!(
            field.type_ref,
            TypeRef::Capability(CapabilityRef::File { .. })
        ) {
            cols.insert(field.name.clone());
            continue;
        }
        cols.insert(field.name.clone());
        if let TypeRef::Builtin(BuiltinType::SemanticMoney { .. }) = field.type_ref
            && !explicit_currency_overrides.contains(&field.name) {
                cols.insert(format!("{}_currency", field.name));
            }
    }

    let timestamps = resource.timestamps.unwrap_or(feature.defaults.timestamps);
    if timestamps {
        cols.insert("created_at".to_string());
        cols.insert("updated_at".to_string());
    }
    if resource.soft_delete {
        cols.insert("deleted_at".to_string());
        // Spec 0015 — `soft_delete by` also projects `deleted_by`.
        if resource.soft_delete_actor {
            cols.insert("deleted_by".to_string());
        }
    }

    cols
}

// ── minimal SQL CREATE TABLE column parser ──────────────────────────────────
