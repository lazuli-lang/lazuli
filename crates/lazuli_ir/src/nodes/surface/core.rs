//! Core surface shapes — `Surface`, audience, view enum, query/command refs.
//!
//! The four nested concerns of a Lzx ViewModel:
//! 1. **Surface** — the `(feature, target)` pair. One per Lzx file.
//! 2. **Audience** — a scope-gated section ([`Audience::requires`] uses
//!    OR-semantics across `@scope.<X>` atoms).
//! 3. **View** — closed catalog: `ViewList`, `ViewDetail`, `ViewCreate`.
//! 4. **Surface controls** — typed declarations the View composes
//!    (search, filter, sort, selection, settings, drawer — see siblings).

use serde::{Deserialize, Serialize};

use crate::{PolicyAtom, SpanRef};

use super::views::{ViewCreate, ViewDetail, ViewList};

/// Lzx ViewModel surface lowered from one `<feat>.<target>.lzx` file.
/// Carried on `Feature.surfaces`; one entry per platform target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Surface {
    /// `surface <feature> web|mobile` — feature name.
    pub feature: String,
    pub target: SurfaceTarget,
    pub audiences: Vec<Audience>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of surface platform targets. One [`Surface`] per
/// target; codegen wires each variant to its own emitter pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceTarget {
    /// `surface <feature> web` — React Web target.
    Web,
    /// `surface <feature> mobile` — React Native target.
    Mobile,
}

/// One audience block inside a surface. Maps to one `audience <name>
/// requires @scope.<X>` section in `.lzx`. The `requires` list uses
/// OR-semantics: the audience admits a caller whose policy carries ANY
/// of the listed scopes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Audience {
    /// `audience <name>` — kebab/snake case authoring identifier.
    pub name: String,
    /// `requires @scope.<name>` lines (one or more).
    pub requires: Vec<PolicyAtom>,
    pub views: Vec<View>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed view-kind catalog. New kinds enter via a Lazuli core proposal
/// (Rule Zero) plus a minor IR bump.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum View {
    List(ViewList),
    Detail(ViewDetail),
    Create(ViewCreate),
}

impl View {
    /// Return the authored view name regardless of which view kind this is.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// # use lazuli_ir::View;
    /// # let view: View = unimplemented!();
    /// let name = view.name();
    /// ```
    pub fn name(&self) -> &str {
        match self {
            View::List(v) => &v.name,
            View::Detail(v) => &v.name,
            View::Create(v) => &v.name,
        }
    }

    /// Return the optional `route` slot regardless of view kind. `None`
    /// for embedded views (no own URL).
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// # use lazuli_ir::View;
    /// # let view: View = unimplemented!();
    /// let route = view.route();
    /// ```
    pub fn route(&self) -> Option<&str> {
        match self {
            View::List(v) => v.route.as_deref(),
            View::Detail(v) => v.route.as_deref(),
            View::Create(v) => v.route.as_deref(),
        }
    }
}

/// Reference to a query declared in some feature. The `kind` field
/// surfaces the textual form (`query.list` / `query.lookup` / `query.sql`
/// / `query.view`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryRef {
    pub feature: String,
    pub kind: QueryKind,
    pub name: String,
}

/// Closed catalog distinguishing the four read shapes (mirrors the
/// authored prefix on the query reference: `query.list`, `query.lookup`,
/// `query.sql`, `query.view`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryKind {
    /// `query.list <name>` — collection read.
    List,
    /// `query.lookup <name>` — single-row read.
    Lookup,
    /// `query.sql <name>` — hand-rolled SQL.
    Sql,
    /// `query.sql.view <name>` — materialised view.
    View,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_target_round_trips() {
        let s = serde_json::to_string(&SurfaceTarget::Web).unwrap();
        assert_eq!(s, "\"web\"");
    }
}

/// Reference to a command. `feature` is set when the source uses the
/// qualified form (`slug.command.create`); for the bare local form
/// (`create` inside `actions`) the parser sets `feature` to the surface's
/// feature and `name` to the command name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRef {
    pub feature: String,
    pub name: String,
}

/// Slot binding for a list/detail/create view: `cells <field> @client.<slot>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellBinding {
    pub field: String,
    /// The slot identifier after the `@client.` prefix.
    pub slot: String,
}

/// `route <name>: <Type> from path` — a typed path parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteParam {
    pub name: String,
    /// Raw type label as authored (e.g. `Text`, `Customer.ID`). The
    /// analyzer leaves the literal verbatim; deeper resolution lifts in
    /// the codegen pipeline.
    pub type_ref: String,
}
