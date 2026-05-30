//! `Lazurite.toml` parsing + the resolver helpers that apply the
//! canonical layout defaults so most pilots can omit boilerplate.
//!
//! ## Module layout
//!
//! - `project.rs` — `[project]`, `[lazuli]`, `[lazurite]`, and
//!   `[plugins]` schema.
//! - `generate.rs` — `[generate]` / `[generate.go]` schema.
//! - `frontends.rs` — `[frontends.<name>]` schema.
//! - `migrations.rs` — `[migrations]`, `[seeds]`, `[dev]` schema.
//! - `doctor.rs` — `[doctor.*]` schema (severity overrides, presets,
//!   coverage thresholds).
//! - `testing.rs` — `[testing.*]` schema.
//! - `inspect.rs` — borrow-friendly `lazuli inspect` projection.
//! - `error.rs` — `ManifestError` envelope + `From` impls.
//!
//! The top-level `Manifest` struct lives here so the impl block
//! (`load`, `validate`, `app_root`, `testing_*_resolved`,
//! `detect_frontend_layout`, `inspect_view`) keeps its access to
//! every sub-section without re-exporting helper traits.

mod doctor;
mod error;
mod frontends;
mod generate;
mod inspect;
mod knowledge;
mod migrations;
mod project;
mod testing;

#[cfg(test)]
mod doctor_tests;
#[cfg(test)]
mod frontends_tests;
#[cfg(test)]
mod generate_tests;
#[cfg(test)]
mod migrations_tests;
#[cfg(test)]
mod project_tests;
#[cfg(test)]
mod testing_tests;

pub use doctor::{
    CoverageSection, Doctor, InternalHygieneDoctor, LayerThresholdConfig, SeverityOverride,
    TestDisciplineDoctor,
};
pub use error::ManifestError;
pub use frontends::{Frontend, FrontendTarget};
pub use generate::{Generate, GenerateGo};
pub use inspect::{InspectFrontend, InspectManifest, InspectPlugin};
pub use knowledge::Knowledge;
pub use migrations::{DevOverrides, MigrationStrategy, Migrations, Seeds};
pub use project::{Lazurite, LazuliPin, Plugin, Project};
pub use testing::{Testing, TestingGo, TestingPlaywright, TestingSpec, TestingTs};

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

include!("mod_p1.rs");
include!("mod_p2.rs");
