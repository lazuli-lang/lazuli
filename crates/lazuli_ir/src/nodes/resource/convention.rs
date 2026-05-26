//! Convention bundles — resource-level auto-synthesis triggers.
//!
//! A `Resource` may opt into one or more **convention bundles** (`crud`,
//! `me`, ...) that expand at analysis time into a closed set of
//! synthesised commands and queries. The IR records:
//!
//! - **`ConventionRef`** — which bundles the resource subscribes to.
//!   Closed catalog; adding a variant is an IR change requiring a
//!   proposal and parser updates.
//! - **`ConventionOrigin`** — how each synth-eligible entry name
//!   ended up in `Feature.synth_origins`. Inspect renders the marker
//!   so authors can see exactly which command/query came from which
//!   bundle and which were author-overridden.
//!
//! The two enums are independent of `Resource` — they live here so
//! they can be imported by the analyser and inspect surfaces without
//! pulling the full resource IR.

use serde::{Deserialize, Serialize};

/// Closed catalog of resource-level convention bundles. Adding a
/// variant is an IR change requiring a proposal; the parser MUST
/// reject any identifier not in this enum.
///
/// See `docs/proposals/ir-resource-conventions-crud.md` §4.2 and
/// `docs/proposals/ir-resource-conventions-me.md` §4.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConventionRef {
    /// `crud` — auto-synthesizes 3 commands + 2 queries (5 entries
    /// total) per `ir-resource-conventions-crud.md` §5.1.
    Crud,
    /// `me` — auto-synthesizes one `lookup_my_<resource>` query keyed
    /// by `ctx.User.ID` (or `ctx.User.OrgID` for org-only resources).
    /// See `ir-resource-conventions-me.md` §5.
    Me,
    // Future variants (NOT in this proposal):
    //   Timestamped, PiiAware, SoftDelete, Slugged, Paginated.
}

/// Origin of a synth-eligible entry name in `Feature.synth_origins`.
///
/// Cell C3's synthesis pass marks each name that a convention bundle
/// would have produced. The marker distinguishes the two relevant
/// states the inspect surface (§11) renders:
///
/// * `Synthesized(<bundle>)` — C3 appended this command/query as part
///   of the named bundle's expansion. Inspect annotates with
///   `[conv:<bundle>]`.
/// * `AuthorOverride(<bundle>)` — the name was in the bundle's set but
///   an author-written command/query already existed with the same
///   name. C3 skipped its synthesis. Inspect annotates with
///   `[author override; convention skipped]`.
///
/// Entries C3 does not touch (pure author-written commands not in any
/// convention's set) carry no entry in `synth_origins`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "convention", rename_all = "snake_case")]
pub enum ConventionOrigin {
    /// Entry was synthesized by the named bundle.
    Synthesized(ConventionRef),
    /// Author wrote this name; the named bundle would have synthesized it.
    AuthorOverride(ConventionRef),
}

impl ConventionOrigin {
    /// The bundle that produced (or would have produced) this entry.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::{ConventionOrigin, ConventionRef};
    ///
    /// let o = ConventionOrigin::Synthesized(ConventionRef::Crud);
    /// assert_eq!(o.convention(), ConventionRef::Crud);
    /// ```
    pub fn convention(&self) -> ConventionRef {
        match self {
            ConventionOrigin::Synthesized(c) | ConventionOrigin::AuthorOverride(c) => *c,
        }
    }

    /// `true` when an author wrote a command/query with this name and the
    /// convention's synth for that name was skipped.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::{ConventionOrigin, ConventionRef};
    ///
    /// let o = ConventionOrigin::AuthorOverride(ConventionRef::Crud);
    /// assert!(o.is_author_override());
    /// ```
    pub fn is_author_override(&self) -> bool {
        matches!(self, ConventionOrigin::AuthorOverride(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convention_origin_round_trips() {
        let o = ConventionOrigin::Synthesized(ConventionRef::Crud);
        let s = serde_json::to_string(&o).unwrap();
        let back: ConventionOrigin = serde_json::from_str(&s).unwrap();
        assert_eq!(o, back);
    }
}
