//! Transitive Go module dependencies required by generated imports.

/// Known transitive Go module dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitiveDep {
    pub module: &'static str,
    pub version: &'static str,
}

pub const GO_POSTGIS_IMPORT: &str = "github.com/cridenour/go-postgis";

pub const GO_POSTGIS_DEP: TransitiveDep = TransitiveDep {
    module: "github.com/cridenour/go-postgis",
    version: "v1.0.2",
};

/// Known transitive Go module deps keyed by import path.
pub const TRANSITIVE_DEPS: &[(&str, TransitiveDep)] = &[(GO_POSTGIS_IMPORT, GO_POSTGIS_DEP)];
