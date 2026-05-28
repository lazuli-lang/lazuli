//! `@owner_axis` diagnostics — Cell O3 surface for the
//! `ir-resource-conventions-owner-scope` proposal (§11.1).
//!
//! References:
//! - `docs/proposals/ir-resource-conventions-owner-scope.md` §7.4 + §11.1.
//!
//! Catalog (4 codes):
//!
//! | Code                                  | Severity | Trigger                                                                                                                                                |
//! |---------------------------------------|----------|--------------------------------------------------------------------------------------------------------------------------------------------------------|
//! | `owner_axis_on_non_fk`                | error    | `@owner_axis` applied to a primitive field (non-FK). Wired by O1 (parser).                                                                             |
//! | `owner_axis_unknown_through`          | error    | `@owner_axis(through: <col>)` references a column that doesn't exist on the FK target resource. Wired by O2 (analyzer).                                |
//! | `owner_axis_through_not_user_keyed`   | warning  | `@owner_axis(through: <col>)` resolves to a target resource without `user: User required unique` — the ownership chain can't bottom out at an actor.   |
//! | `owner_axis_collides_with_unique_user`| warning  | Resource has both `user: User required unique` AND `@owner_axis(through: <col>)`. The synth would emit conflicting WHERE clauses (user-keyed + chain). |
//!
//! Per the rubric's C11 error-message hygiene gate, the message strings
//! are pasted verbatim from §11.1 of the proposal. Cells O1 (parser) and
//! O2 (synth pass) fire the codes; this module owns the canonical
//! phrasing so every wiring site emits the same text.

use std::path::PathBuf;

// =============================================================================
// owner_axis_on_non_fk — emitted by Cell O1 (parser) when `@owner_axis` is
//                        applied to a field whose type is not another resource.
// =============================================================================

/// `@owner_axis` was attached to a field whose type isn't a resource
/// reference. Cell O1 (parser) builds this when validating annotations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerAxisOnNonFkFinding {
    /// Source `.lzi` file the offending annotation was authored in.
    pub path: PathBuf,
    /// Resource the offending field lives on.
    pub resource: String,
    /// The field carrying the spurious `@owner_axis`.
    pub field: String,
    /// Rendered type name of the field (e.g. `Text`, `@semantic.Email`).
    pub type_name: String,
}

impl OwnerAxisOnNonFkFinding {
    /// Canonical diagnostic code emitted with this finding.
    pub const CODE: &'static str = "owner_axis_on_non_fk";

    /// Render the verbatim message string from §11.1 of the proposal.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::owner_axis::OwnerAxisOnNonFkFinding;
    ///
    /// let f = OwnerAxisOnNonFkFinding {
    ///     path: PathBuf::from("catalog.lzi"),
    ///     resource: "Property".into(),
    ///     field: "name".into(),
    ///     type_name: "Text".into(),
    /// };
    /// assert!(f.message().contains("not a foreign-key reference"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "Field `{}.{}` declares `@owner_axis`, but the field's type is `{}` — not a foreign-key \
             reference to another resource. `@owner_axis` only makes sense on FK fields. Remove the \
             annotation or change the field type to a resource reference.",
            self.resource, self.field, self.type_name,
        )
    }
}

// =============================================================================
// owner_axis_unknown_through — emitted by Cell O2 (analyzer) when the
//                              `through:` column doesn't exist on the FK
//                              target resource.
// =============================================================================

/// The `through:` column on `@owner_axis` doesn't exist on the FK target
/// resource. Cell O2 (analyzer) emits this after walking the FK target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerAxisUnknownThroughFinding {
    /// Source `.lzi` file the offending annotation was authored in.
    pub path: PathBuf,
    /// Resource the field lives on (for context in the error).
    pub resource: String,
    /// The field name where `@owner_axis(through: ...)` was authored.
    pub field: String,
    /// The `through:` column the author wrote.
    pub through_column: String,
    /// Nearest-match column on the FK target resource — populated by the
    /// analyzer's Levenshtein helper. `None` when no close match exists.
    pub suggestion: Option<String>,
}

impl OwnerAxisUnknownThroughFinding {
    /// Canonical diagnostic code emitted with this finding.
    pub const CODE: &'static str = "owner_axis_unknown_through";

    /// Render the verbatim "did you mean" message string from §11.1.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::owner_axis::OwnerAxisUnknownThroughFinding;
    ///
    /// let f = OwnerAxisUnknownThroughFinding {
    ///     path: PathBuf::from("catalog.lzi"),
    ///     resource: "Host".into(),
    ///     field: "host".into(),
    ///     through_column: "usr".into(),
    ///     suggestion: Some("user".into()),
    /// };
    /// assert!(f.message().contains("Did you mean `user`?"));
    /// ```
    pub fn message(&self) -> String {
        let suggestion = self.suggestion.as_deref().unwrap_or(&self.through_column);
        format!(
            "Field `{}` declares `@owner_axis(through: {})`, but `{}` is not a column on resource \
             `{}`. Did you mean `{}`? Or remove the annotation.",
            self.field, self.through_column, self.through_column, self.resource, suggestion,
        )
    }
}

// =============================================================================
// owner_axis_through_not_user_keyed — emitted by Cell O2 (analyzer) when the
//                                     `through:` column resolves to a target
//                                     resource without `user: User required unique`.
//                                     Severity: warning (per §7.4 — leaves
//                                     room for service-account ownership).
// =============================================================================

/// The `through:` column resolves to a resource without
/// `user: User required unique`, so the ownership chain can't bottom out
/// at an actor. Severity warning — service-account ownership stays viable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerAxisThroughNotUserKeyedFinding {
    /// Source `.lzi` file the offending annotation was authored in.
    pub path: PathBuf,
    /// Resource the offending field lives on.
    pub resource: String,
    /// The field carrying `@owner_axis(through: ...)`.
    pub field: String,
    /// The `through:` column the author wrote.
    pub through_column: String,
    /// The FK target resource the field references.
    pub target_resource: String,
}

impl OwnerAxisThroughNotUserKeyedFinding {
    /// Canonical diagnostic code emitted with this finding.
    pub const CODE: &'static str = "owner_axis_through_not_user_keyed";

    /// Render the verbatim message advising adding `user` to the target.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::owner_axis::OwnerAxisThroughNotUserKeyedFinding;
    ///
    /// let f = OwnerAxisThroughNotUserKeyedFinding {
    ///     path: PathBuf::from("catalog.lzi"),
    ///     resource: "Property".into(),
    ///     field: "host".into(),
    ///     through_column: "user".into(),
    ///     target_resource: "Host".into(),
    /// };
    /// assert!(f.message().contains("cannot resolve"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "Field `{}` declares `@owner_axis(through: {})` referencing resource `{}`, but `{}` has \
             no `user: User required unique` field — the ownership chain cannot resolve to an \
             actor. Either add a `user` field to `{}` or remove `@owner_axis` from `{}.{}`.",
            self.field,
            self.through_column,
            self.target_resource,
            self.target_resource,
            self.target_resource,
            self.resource,
            self.field,
        )
    }
}

// =============================================================================
// owner_axis_collides_with_unique_user — emitted by Cell O2 (analyzer) when a
//                                        resource declares both
//                                        `user: User required unique` AND
//                                        `@owner_axis(through: <col>)` on
//                                        another field. Severity: warning.
// =============================================================================

/// The resource declares both `user: User required unique` AND
/// `@owner_axis(through: <col>)`. The synth would emit conflicting WHERE
/// clauses, so doctor warns and asks the author to pick one mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerAxisCollidesWithUniqueUserFinding {
    /// Source `.lzi` file the offending annotation was authored in.
    pub path: PathBuf,
    /// The resource carrying both `user: User required unique` and
    /// `@owner_axis(through: ...)` on another field.
    pub resource: String,
    /// The `through:` column the author wrote on the colliding field.
    pub through_column: String,
}

impl OwnerAxisCollidesWithUniqueUserFinding {
    /// Canonical diagnostic code emitted with this finding.
    pub const CODE: &'static str = "owner_axis_collides_with_unique_user";

    /// Render the verbatim message asking the author to pick one scope mode.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::owner_axis::OwnerAxisCollidesWithUniqueUserFinding;
    ///
    /// let f = OwnerAxisCollidesWithUniqueUserFinding {
    ///     path: PathBuf::from("catalog.lzi"),
    ///     resource: "Host".into(),
    ///     through_column: "user".into(),
    /// };
    /// assert!(f.message().contains("conflicting WHERE clauses"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "Resource `{}` has both `user: User required unique` AND `@owner_axis(through: {})`. \
             The synth would emit conflicting WHERE clauses (user-keyed AND owner-chained). Remove \
             `@owner_axis` if user-keyed mode suffices, OR remove the unique constraint on user if \
             the chain is the intended scope.",
            self.resource, self.through_column,
        )
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_axis_on_non_fk_message_matches_spec() {
        let f = OwnerAxisOnNonFkFinding {
            path: PathBuf::from("catalog.lzi"),
            resource: "Property".to_owned(),
            field: "name".to_owned(),
            type_name: "Text".to_owned(),
        };
        assert_eq!(
            f.message(),
            "Field `Property.name` declares `@owner_axis`, but the field's type is `Text` — not a \
             foreign-key reference to another resource. `@owner_axis` only makes sense on FK \
             fields. Remove the annotation or change the field type to a resource reference."
        );
        assert_eq!(OwnerAxisOnNonFkFinding::CODE, "owner_axis_on_non_fk");
    }

    #[test]
    fn owner_axis_unknown_through_message_matches_spec() {
        let f = OwnerAxisUnknownThroughFinding {
            path: PathBuf::from("catalog.lzi"),
            resource: "Host".to_owned(),
            field: "host".to_owned(),
            through_column: "usr".to_owned(),
            suggestion: Some("user".to_owned()),
        };
        assert_eq!(
            f.message(),
            "Field `host` declares `@owner_axis(through: usr)`, but `usr` is not a column on \
             resource `Host`. Did you mean `user`? Or remove the annotation."
        );
        assert_eq!(
            OwnerAxisUnknownThroughFinding::CODE,
            "owner_axis_unknown_through"
        );
    }

    #[test]
    fn owner_axis_through_not_user_keyed_message_matches_spec() {
        let f = OwnerAxisThroughNotUserKeyedFinding {
            path: PathBuf::from("catalog.lzi"),
            resource: "Property".to_owned(),
            field: "host".to_owned(),
            through_column: "user".to_owned(),
            target_resource: "Host".to_owned(),
        };
        assert_eq!(
            f.message(),
            "Field `host` declares `@owner_axis(through: user)` referencing resource `Host`, but \
             `Host` has no `user: User required unique` field — the ownership chain cannot resolve \
             to an actor. Either add a `user` field to `Host` or remove `@owner_axis` from \
             `Property.host`."
        );
        assert_eq!(
            OwnerAxisThroughNotUserKeyedFinding::CODE,
            "owner_axis_through_not_user_keyed"
        );
    }

    #[test]
    fn owner_axis_collides_with_unique_user_message_matches_spec() {
        let f = OwnerAxisCollidesWithUniqueUserFinding {
            path: PathBuf::from("catalog.lzi"),
            resource: "Host".to_owned(),
            through_column: "user".to_owned(),
        };
        assert_eq!(
            f.message(),
            "Resource `Host` has both `user: User required unique` AND `@owner_axis(through: \
             user)`. The synth would emit conflicting WHERE clauses (user-keyed AND owner-chained). \
             Remove `@owner_axis` if user-keyed mode suffices, OR remove the unique constraint on \
             user if the chain is the intended scope."
        );
        assert_eq!(
            OwnerAxisCollidesWithUniqueUserFinding::CODE,
            "owner_axis_collides_with_unique_user"
        );
    }
}
