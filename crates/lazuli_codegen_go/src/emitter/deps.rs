//! Transitive Go module dependencies required by generated imports.
//!
//! Some emitted import paths (e.g. PostGIS scanners) require a Go module
//! that the user's `go.mod` would not otherwise pull in. The `go.mod`
//! emitter consults [`TRANSITIVE_DEPS`] to add a pinned `require`
//! directive for every such import.

/// Known transitive Go module dependency.
///
/// Pairs a Go module path with the version constraint emitted into
/// `go.mod`. Versions are pinned (no range syntax) so generated output
/// is byte-equivalent across runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitiveDep {
    /// Go module path (e.g. `"github.com/foo/bar"`).
    pub module: &'static str,
    /// Pinned version constraint (e.g. `"v1.0.2"`).
    pub version: &'static str,
}

/// Import path emitted for PostGIS scanner support.
pub const GO_POSTGIS_IMPORT: &str = "github.com/cridenour/go-postgis";

/// Pinned transitive dep entry for [`GO_POSTGIS_IMPORT`].
pub const GO_POSTGIS_DEP: TransitiveDep = TransitiveDep {
    module: "github.com/cridenour/go-postgis",
    version: "v1.0.2",
};

/// Known transitive Go module deps keyed by import path.
pub const TRANSITIVE_DEPS: &[(&str, TransitiveDep)] = &[(GO_POSTGIS_IMPORT, GO_POSTGIS_DEP)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgis_entry_round_trips_through_table() {
        let found = TRANSITIVE_DEPS
            .iter()
            .find(|(import, _)| *import == GO_POSTGIS_IMPORT)
            .map(|(_, dep)| *dep);
        assert_eq!(found, Some(GO_POSTGIS_DEP));
    }
}
