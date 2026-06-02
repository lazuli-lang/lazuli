//! `.lzi` source-shape hygiene rules.
//!
//! Mirrors the framework's own [`crate::internal_hygiene`] dogfooding,
//! but applied to the user's `.lzi` surface. Three rules:
//!
//! - [`file_size_001`] (`LZI-FILE-SIZE-001`) — flags `.lzi` files above
//!   the configured LOC threshold. Default `warn` at 300 LOC, `warn` at
//!   600 LOC (no `error` tier; the architect review explicitly rejected
//!   error-tier file-size enforcement because there is no measurable
//!   LSP failure mode at any `.lzi` size).
//! - [`feature_naming_matches_file_001`] (`LZI-FEATURE-NAMING-MATCHES-FILE-001`)
//!   — flags `.lzi` files whose basename doesn't appear as one of the
//!   declared feature names. Rails' "file name organizes, header decides"
//!   convention adapted to Lazuli — catches drift when a file is renamed
//!   without updating its `feature <Name>` header.
//! - [`feature_cohesion_001`] (`LZI-FEATURE-COHESION-001`) — flags `.lzi`
//!   files containing multiple features that DON'T share an anchor /
//!   resource / domain prefix. Bundles of cohesive features (e.g.
//!   `customer`, `customer_auth`, `customer_tags`) pass; arbitrary
//!   bundling of unrelated features fires.
//! - [`feature_cohesion_002`] (`LZI-FEATURE-COHESION-002`) — the
//!   resource-graph **sibling** of -001: flags a single feature whose
//!   intra-feature resource graph (nodes = resources; edges = FK fields
//!   + `has_many` + `on_delete`) splits into ≥2 disconnected components,
//!   i.e. it bundles independent capabilities. Shares the
//!   [`cohesion_graph`] union-find helper. Non-waivable-in-spirit; warns
//!   by default. Carries two info-level cross-feature companions (`uses`
//!   fan-out, resource-name similarity).
//! - [`cohesion_graph`] — infrastructure (not a rule): the shared
//!   intra-feature relation graph + connected-components builder that
//!   `feature_cohesion_002` (and 0009's splits) read.
//! - [`lzi_comment_noise`] (`LZI-COMMENT-NOISE-001`) — advisory comment-noise
//!   lint: decorative dividers + high comment-to-semantic ratio. Fires when a
//!   `.lzi`/`.lzx` is comment-dominant or carries a divider ruler. Preventive
//!   (pilots' `.lzi` are already clean); never gates.
//!
//! ## Why these three (and not "1 feature per file")
//!
//! The initial proposal included `LZI-FEATURE-PER-FILE-001` as an
//! across-the-board error. The architect review (2026-05-27) rejected
//! that rule because Lazuli `feature` is a namespacing/grouping primitive,
//! not a load unit (unlike Rails `class`). Features that share a `resource`
//! or `@policy.*` anchor legitimately co-locate — splitting them across
//! files forces import-chase and fragments cold-read, violating the
//! "Self-Contained Declarations" principle. Cohesion-001 captures the
//! real anti-pattern (arbitrary bundling) while letting the canonical
//! `examples/full-capsule/full-capsule.lzi` (6 features sharing
//! `customer*` prefix + `resource customer`) pass intact.
//!
//! ## Preset interaction
//!
//! Governed by `[doctor.lzi_hygiene].preset` in `Lazurite.toml`. Under
//! `tdd-iron-hand`, every `LZI-*` rule fires at `Error`; `tdd-strict`
//! at `Warning`; `tdd-mature` defers to per-rule defaults; `off` at
//! `Info`. See [`preset::preset_rule_severity`].
//!
//! ## Exempt paths
//!
//! The dispatcher skips these by convention:
//!
//! - `vocab-fixtures/`, `anti-patterns/`, `proposals/` directories
//!   (test fixtures intentionally deviant)
//! - Files with `# @generated` or `# Code generated` in the first
//!   non-blank line (future codegen targets)
//! - `app.lzi`, `registry.lzi`, `workspace.lzi`, `profiles.lzi`, and
//!   any file under `contracts/` (zero-feature files — they declare
//!   `app`, `registry`, `workspace`, `contract` toplevels, not `feature`)

pub mod cohesion_graph;
pub mod feature_cohesion_001;
pub mod feature_cohesion_002;
pub mod feature_naming_matches_file_001;
pub mod file_size_001;
pub mod legacy_comment_allow_001;
pub mod lzi_comment_noise;
pub mod preset;
pub mod walker;

pub use feature_cohesion_001::Finding as FeatureCohesionFinding;
pub use feature_cohesion_002::Finding as FeatureCohesion002Finding;
pub use feature_naming_matches_file_001::Finding as FeatureNamingMatchesFileFinding;
pub use file_size_001::Finding as LziFileSizeFinding;
pub use preset::{LziHygienePreset, preset_rule_severity};
pub use walker::{LziSourceFile, is_exempt_path, walk_lzi_sources};
