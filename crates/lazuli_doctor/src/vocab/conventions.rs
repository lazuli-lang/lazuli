//! Conventions diagnostics — Cell C4 surface for the closed-catalog
//! `conventions [crud]` resource slot, extended by Cell M3 for the
//! `me` bundle.
//!
//! References:
//! - `docs/proposals/ir-resource-conventions-crud.md` §11.
//! - `docs/proposals/ir-resource-conventions-me.md` §11.
//!
//! Catalog (6 codes):
//!
//! | Code                              | Severity | Trigger                                                                                  |
//! |-----------------------------------|----------|------------------------------------------------------------------------------------------|
//! | `conventions_unknown`             | error    | `conventions [<name>]` references an identifier outside the closed catalog. Wired by C2. |
//! | `crud_synth_signature_mismatch`   | error    | An author override of a `crud` synth name diverges from the canonical signature. C3.     |
//! | `crud_synth_policy_not_found`     | error    | A resource opts into `conventions [crud]` but the feature has no `authenticated` policy. |
//! | `crud_synth_no_required_fields`   | error    | A `crud` synth's `create_<r>` would have an empty input (every required field is auto).  |
//! | `me_synth_no_actor_resolution`    | error    | A `conventions [me]` resource has no owner axis to filter on (no `user`, no `org`, not `User`). M2/M3. |
//! | `me_synth_signature_mismatch`     | error    | An author override of a `me` synth name diverges from the canonical signature. M2/M3.    |
//!
//! Each diagnostic message contains a suggested fix per the framework
//! error message contract (rubric C11). The actual *firing* of these
//! codes is wired upstream by Cells C2 (parser) and C3 (synthesis pass);
//! this module owns the verbatim message strings + code constants so
//! every wiring site emits a single canonical phrasing.

use std::path::PathBuf;

// =============================================================================
// conventions_unknown — emitted by Cell C2 (parser) when an identifier inside
//                       `conventions [..]` is not in the closed catalog.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownConventionFinding {
    pub path: PathBuf,
    /// The identifier the author wrote inside `conventions [..]`.
    pub name: String,
    /// Nearest-match suggestion from the closed catalog (today: always
    /// `crud`). `None` when the Levenshtein distance exceeds the cutoff
    /// — but for a single-member catalog the suggestion is always set.
    pub suggestion: Option<String>,
}

impl UnknownConventionFinding {
    pub const CODE: &'static str = "conventions_unknown";

    pub fn message(&self) -> String {
        match &self.suggestion {
            Some(suggested) => format!(
                "Unknown convention `{}`. Did you mean `{}`?",
                self.name, suggested
            ),
            None => format!("Unknown convention `{}`. Did you mean `crud`?", self.name),
        }
    }
}

// =============================================================================
// crud_synth_signature_mismatch — emitted by Cell C3 (synthesis pass) when an
//                                 author override of a `crud` synth name
//                                 diverges from the canonical shape.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrudSynthSignatureMismatchFinding {
    pub path: PathBuf,
    /// The author-written command name that shares a name with a `crud` synth.
    pub name: String,
}

impl CrudSynthSignatureMismatchFinding {
    pub const CODE: &'static str = "crud_synth_signature_mismatch";

    pub fn message(&self) -> String {
        format!(
            "Your `{}` command's signature diverges from the `crud` convention's canonical shape. \
             Either rename your command or restore the canonical input/return types. See \
             `docs/proposals/ir-resource-conventions-crud.md` §6.",
            self.name
        )
    }
}

// =============================================================================
// crud_synth_policy_not_found — emitted by Cell C3 when a resource opts into
//                               `conventions [crud]` but the feature has no
//                               `authenticated` policy to inherit.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrudSynthPolicyNotFoundFinding {
    pub path: PathBuf,
    /// Resource name whose `conventions [crud]` slot triggered the check.
    pub resource: String,
}

impl CrudSynthPolicyNotFoundFinding {
    pub const CODE: &'static str = "crud_synth_policy_not_found";

    pub fn message(&self) -> String {
        format!(
            "Resource `{}` opts into `conventions [crud]`, but the feature has no `authenticated` \
             policy. Author one.",
            self.resource
        )
    }
}

// =============================================================================
// crud_synth_no_required_fields — emitted by Cell C3 when every required
//                                 field on a `conventions [crud]` resource
//                                 is in the Tenant or Auto group, leaving
//                                 the synthesized `create_<r>.input` empty.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrudSynthNoRequiredFieldsFinding {
    pub path: PathBuf,
    /// Resource whose `create_<r>` synth would have an empty input.
    pub resource: String,
}

impl CrudSynthNoRequiredFieldsFinding {
    pub const CODE: &'static str = "crud_synth_no_required_fields";

    pub fn message(&self) -> String {
        format!(
            "Resource `{}`'s `create_{}` would have empty input — every required field is in the \
             Tenant or Auto group. Remove `conventions [crud]` or add a non-tenant required field.",
            self.resource,
            self.resource.to_ascii_lowercase()
        )
    }
}

// =============================================================================
// me_synth_no_actor_resolution — emitted by Cell M2 (synthesis pass) when a
//                                resource opts into `conventions [me]` but has
//                                no field the synth can use as the actor key
//                                (no `user: User required unique`, no `org`
//                                axis, and the resource isn't named `User`).
//                                See ir-resource-conventions-me.md §5.3.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeSynthNoActorResolutionFinding {
    pub path: PathBuf,
    /// Resource name whose `conventions [me]` slot triggered the check.
    pub resource: String,
}

impl MeSynthNoActorResolutionFinding {
    pub const CODE: &'static str = "me_synth_no_actor_resolution";

    pub fn message(&self) -> String {
        format!(
            "Resource `{}` opts into `conventions [me]`, but its fields can't resolve an actor — \
             it has no `user: User required unique` field, no `org: Org required` field, and \
             isn't named `User`. Either add a tenant column, remove `me` from conventions, or \
             rename the resource to `User` if it represents the actor.",
            self.resource
        )
    }
}

// =============================================================================
// me_synth_signature_mismatch — emitted by Cell M2 (synthesis pass) when an
//                               author override of a `me` synth name diverges
//                               from the canonical shape (return type or
//                               route-param surface). Parallel to
//                               crud_synth_signature_mismatch.
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeSynthSignatureMismatchFinding {
    pub path: PathBuf,
    /// The resource snake-name suffix on the `lookup_my_<r>` query that
    /// shares a name with a `me` synth — used to interpolate the query
    /// name in the message.
    pub resource_snake: String,
}

impl MeSynthSignatureMismatchFinding {
    pub const CODE: &'static str = "me_synth_signature_mismatch";

    pub fn message(&self) -> String {
        format!(
            "Your `lookup_my_{}` query's signature diverges from the `me` convention's canonical \
             shape (returns `{}`, no route params). Either rename your query or restore the \
             canonical return type / parameter set. See \
             `docs/proposals/ir-resource-conventions-me.md` §6.",
            self.resource_snake,
            capitalize_first(&self.resource_snake)
        )
    }
}

/// Helper: capitalize the first ASCII byte of `input`. Used only to
/// reconstruct the resource PascalCase from its snake suffix for the
/// `me_synth_signature_mismatch` message ("returns `<R>`").
fn capitalize_first(input: &str) -> String {
    let mut chars = input.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// Closed-catalog list of `conventions [<name>]` identifiers accepted by
/// the parser. Mirrored from `lazuli_ir::ConventionRef`. Adding a name
/// here without the matching IR variant + proposal is incorrect.
pub const CONVENTIONS_CATALOG: &[&str] = &["crud", "me"];

/// Suggest the nearest closed-catalog convention name for a typo'd
/// identifier. Returns the first catalog entry as a defensive default;
/// authoritative nearest-match logic lives in the parser/analyzer
/// (`conventions_unknown_suggestion`).
pub fn suggest_convention(_input: &str) -> Option<&'static str> {
    CONVENTIONS_CATALOG.first().copied()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conventions_unknown_message_includes_suggestion() {
        let f = UnknownConventionFinding {
            path: PathBuf::from("feat.lzi"),
            name: "crd".to_owned(),
            suggestion: Some("crud".to_owned()),
        };
        assert_eq!(
            f.message(),
            "Unknown convention `crd`. Did you mean `crud`?"
        );
        assert_eq!(UnknownConventionFinding::CODE, "conventions_unknown");
    }

    #[test]
    fn conventions_unknown_message_falls_back_to_crud() {
        let f = UnknownConventionFinding {
            path: PathBuf::from("feat.lzi"),
            name: "wat".to_owned(),
            suggestion: None,
        };
        assert_eq!(
            f.message(),
            "Unknown convention `wat`. Did you mean `crud`?"
        );
    }

    #[test]
    fn crud_synth_signature_mismatch_message_matches_spec() {
        let f = CrudSynthSignatureMismatchFinding {
            path: PathBuf::from("customer.lzi"),
            name: "update_customer".to_owned(),
        };
        assert_eq!(
            f.message(),
            "Your `update_customer` command's signature diverges from the `crud` convention's \
             canonical shape. Either rename your command or restore the canonical input/return \
             types. See `docs/proposals/ir-resource-conventions-crud.md` §6."
        );
        assert_eq!(
            CrudSynthSignatureMismatchFinding::CODE,
            "crud_synth_signature_mismatch"
        );
    }

    #[test]
    fn crud_synth_policy_not_found_message_matches_spec() {
        let f = CrudSynthPolicyNotFoundFinding {
            path: PathBuf::from("customer.lzi"),
            resource: "Customer".to_owned(),
        };
        assert_eq!(
            f.message(),
            "Resource `Customer` opts into `conventions [crud]`, but the feature has no \
             `authenticated` policy. Author one."
        );
        assert_eq!(
            CrudSynthPolicyNotFoundFinding::CODE,
            "crud_synth_policy_not_found"
        );
    }

    #[test]
    fn crud_synth_no_required_fields_message_matches_spec() {
        let f = CrudSynthNoRequiredFieldsFinding {
            path: PathBuf::from("customer.lzi"),
            resource: "Customer".to_owned(),
        };
        assert_eq!(
            f.message(),
            "Resource `Customer`'s `create_customer` would have empty input — every required \
             field is in the Tenant or Auto group. Remove `conventions [crud]` or add a \
             non-tenant required field."
        );
        assert_eq!(
            CrudSynthNoRequiredFieldsFinding::CODE,
            "crud_synth_no_required_fields"
        );
    }

    #[test]
    fn closed_catalog_lists_crud_and_me() {
        assert_eq!(CONVENTIONS_CATALOG, &["crud", "me"]);
        // Suggestion helper returns a defensive default; the parser
        // owns the authoritative nearest-match logic.
        assert_eq!(suggest_convention("xyz"), Some("crud"));
    }

    #[test]
    fn me_synth_no_actor_resolution_message_matches_spec() {
        let f = MeSynthNoActorResolutionFinding {
            path: PathBuf::from("orders.lzi"),
            resource: "Order".to_owned(),
        };
        assert_eq!(
            f.message(),
            "Resource `Order` opts into `conventions [me]`, but its fields can't resolve an \
             actor — it has no `user: User required unique` field, no `org: Org required` field, \
             and isn't named `User`. Either add a tenant column, remove `me` from conventions, \
             or rename the resource to `User` if it represents the actor."
        );
        assert_eq!(
            MeSynthNoActorResolutionFinding::CODE,
            "me_synth_no_actor_resolution"
        );
    }

    #[test]
    fn me_synth_signature_mismatch_message_matches_spec() {
        let f = MeSynthSignatureMismatchFinding {
            path: PathBuf::from("traveler.lzi"),
            resource_snake: "traveler".to_owned(),
        };
        assert_eq!(
            f.message(),
            "Your `lookup_my_traveler` query's signature diverges from the `me` convention's \
             canonical shape (returns `Traveler`, no route params). Either rename your query or \
             restore the canonical return type / parameter set. See \
             `docs/proposals/ir-resource-conventions-me.md` §6."
        );
        assert_eq!(
            MeSynthSignatureMismatchFinding::CODE,
            "me_synth_signature_mismatch"
        );
    }
}
