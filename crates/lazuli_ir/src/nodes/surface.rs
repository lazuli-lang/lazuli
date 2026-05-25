//! Lzx ViewModel surface IR — `<feat>.<target>.lzx` lowered shape.
//!
//! A *Surface* is what an audience sees on one platform target. The
//! author writes one `<feature>.web.lzx` and one `<feature>.mobile.lzx`
//! (or fewer); the analyzer lowers each into one [`Surface`] carried on
//! `Feature.surfaces`. Codegen projects the surface into React /
//! React Native / future targets, but the language never talks transport
//! — the surface declares **what** the audience sees, never **how**
//! React Router or Expo wires it.
//!
//! ## The four nested concerns
//!
//! 1. **Surface** — the `(feature, target)` pair. One per Lzx file.
//! 2. **Audience** — a scope-gated section ([`Audience::requires`] uses
//!    OR-semantics across `@scope.<X>` atoms). Multiple audiences may
//!    share a surface to model "public / authed / admin" splits in one
//!    file.
//! 3. **View** — closed catalog: [`ViewList`], [`ViewDetail`],
//!    [`ViewCreate`]. New kinds enter via a Lazuli core proposal plus a
//!    minor IR bump (Rule Zero).
//! 4. **Surface controls** — typed declarations the View composes:
//!    search, filter, sort, selection, settings, drawer.
//!
//! Every control is its own typed shape — search is not a free-form
//! string, filter is not a duck-typed property, selection isn't
//! "anything goes." That tightness is what lets the LLM compose the
//! whole surface cold from a one-line spec.
//!
//! ## Closed-catalog discipline
//!
//! Multiple enums in this family are intentionally narrow:
//! [`SelectionMode`] (`none / single / multi`), [`SortDir`], [`SearchMode`]
//! (`columns / segmented`), [`FilterCardinality`] (`single / multi`),
//! [`SettingValueSpace`] (`enum / bool / int`), [`SettingPersistence`]
//! (`none / local / workspace`). Each closure exists because product
//! surfaces drift into chaos when controls accept open-ended
//! configuration; doctor enforces every catalog with proposal-level
//! cost to extend.
//!
//! ## `on_success` orchestration
//!
//! [`OnSuccessSpec`] captures the post-submit orchestration for
//! `ViewCreate` views — `back`, `redirect`, `flash`, `invalidates`,
//! `replace`. The shape is a *declaration of intent*, not a callback:
//! codegen emits the JavaScript/React Router / Expo Router moves; the
//! language stays out of the navigation library.
//!
//! ## See also
//!
//! - `docs/proposals/lzx-integration-codegen.md` §5 (grammar) + §6
//!   (emission shapes).
//! - [`crate::PolicyAtom`] — atom used by [`Audience::requires`]
//!   (defined in crate root because it's shared with command / query /
//!   workflow policy expressions).
//! - [`crate::InvalidatesSpec`] — cache-invalidation declaration (shared
//!   with command/query lowering).
//! - [`crate::TranslationKeyRef`] — i18n key reference shape.

use serde::{Deserialize, Serialize};

use crate::{InvalidatesSpec, PolicyAtom, SpanRef, TranslationKeyRef, is_false};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceTarget {
    Web,
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
    pub fn name(&self) -> &str {
        match self {
            View::List(v) => &v.name,
            View::Detail(v) => &v.name,
            View::Create(v) => &v.name,
        }
    }

    pub fn route(&self) -> Option<&str> {
        match self {
            View::List(v) => v.route.as_deref(),
            View::Detail(v) => v.route.as_deref(),
            View::Create(v) => v.route.as_deref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewList {
    pub name: String,
    /// Optional `at "<path>"` route binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    /// `source <feature>.query.<name>`.
    pub source: QueryRef,
    /// How this list renders its rows (`columns` table form or grid `cells` slot).
    pub render: ListRender,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<SearchDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filter: Vec<FilterDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<CellBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<CommandRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawer: Option<DrawerSubView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<SortDecl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<SelectionDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<SettingDecl>,
    /// `view ... fields <X> redacted` — field names that this view
    /// must mask before emission. Codegen emits a redaction wrapper.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redacted_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewDetail {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    pub source: QueryRef,
    /// `route <name>: <Type> from path` declarations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route_params: Vec<RouteParam>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<CellBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<CommandRef>,
    /// `view ... fields <X> redacted` — field names that this view
    /// must mask before emission. Codegen emits a redaction wrapper.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redacted_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewCreate {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    /// `submit <feature>.command.<name>` (required).
    pub submit: CommandRef,
    /// Submit-success orchestration lowered from `.lzx on_success`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_success: Option<OnSuccessSpec>,
    /// `fields <name>, <name>` — subset of the command's input slots.
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<CellBinding>,
    /// `view ... fields <X> redacted` — field names that this view
    /// must mask before emission. Codegen emits a redaction wrapper.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redacted_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnSuccessSpec {
    #[serde(default, skip_serializing_if = "is_false")]
    pub back: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flash: Option<FlashSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidates: Vec<InvalidatesSpec>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub replace: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlashSpec {
    pub kind: String,
    pub message_key: TranslationKeyRef,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryKind {
    List,
    Lookup,
    Sql,
    View,
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

// ---- L0 #6 Terminal grammar IR ----

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ListRender {
    Table {
        columns: Vec<String>,
    },
    /// Grid form: slot identifier after `@client.`.
    Cells {
        slot: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawerSubView {
    pub name: String,
    pub trigger: DrawerTrigger,
    pub source: QueryRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_binding: Option<DrawerRouteBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<CellBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<CommandRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawerTrigger {
    /// Click on host cell opens drawer with that item.
    Select,
    /// User code calls `.open(id)` explicitly.
    ManualOpen,
}

/// `route <slot> from selection` binds the drawer's source query input
/// to the host's selection state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawerRouteBinding {
    /// The sub-query input name, e.g. `key`.
    pub target: String,
    pub source: DrawerBindingSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawerBindingSource {
    Selection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterDecl {
    pub name: String,
    /// Raw type label as authored (e.g. `ItemType`, `Text`). Resolution
    /// to a concrete enum-on-resource or scalar happens in lowering.
    pub type_ref: String,
    pub cardinality: FilterCardinality,
    /// `from query` flag: true if filter state syncs to URL params.
    pub url_sync: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterCardinality {
    Single,
    Multi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchDecl {
    pub mode: SearchMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<SearchField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_text_target: Option<BindingRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SearchMode {
    /// v1 behavior already represented by `ViewList.search` today.
    Columns {
        columns: Vec<String>,
    },
    Segmented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchField {
    pub key: String,
    pub binds_to: BindingRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BindingRef {
    /// `filters.<name>`.
    Filter { name: String },
    /// `source.<input-name>`.
    SourceInput { name: String },
    /// `selection` in single-selection mode.
    SelectionScalar,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortDecl {
    pub allowed: Vec<String>,
    pub default_field: String,
    pub default_dir: SortDir,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionDecl {
    pub mode: SelectionMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bulk_actions: Vec<CommandRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    None,
    Single,
    Multi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingDecl {
    pub name: String,
    pub value_space: SettingValueSpace,
    /// Raw token, e.g. `sm`, `true`, or `42`.
    pub default: String,
    pub persistence: SettingPersistence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SettingValueSpace {
    Enum { values: Vec<String> },
    Bool,
    Int { min: i64, max: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingPersistence {
    None,
    Local,
    /// v0.2: declared but lowering warns until the cell ships.
    Workspace,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn surface_target_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(SurfaceTarget::Mobile).unwrap(),
            json!("mobile")
        );
    }

    #[test]
    fn view_tags_kind() {
        let v = View::List(ViewList {
            name: "customers".to_owned(),
            route: Some("/customers".to_owned()),
            source: QueryRef {
                feature: "customer".to_owned(),
                kind: QueryKind::List,
                name: "list".to_owned(),
            },
            render: ListRender::Table {
                columns: vec!["name".to_owned(), "email".to_owned()],
            },
            search: None,
            filter: vec![],
            cells: vec![],
            actions: vec![],
            drawer: None,
            sort: None,
            selection: None,
            settings: vec![],
            redacted_fields: vec![],
            span_ref: None,
        });
        let value = serde_json::to_value(&v).unwrap();
        assert_eq!(value["kind"], json!("list"));
        assert_eq!(v.name(), "customers");
        assert_eq!(v.route(), Some("/customers"));
    }

    #[test]
    fn list_render_table_serializes_with_kind() {
        let r = ListRender::Table {
            columns: vec!["a".to_owned()],
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["kind"], json!("table"));
        let back: ListRender = serde_json::from_value(v).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn selection_mode_round_trips() {
        for m in [SelectionMode::None, SelectionMode::Single, SelectionMode::Multi] {
            let value = serde_json::to_value(m).unwrap();
            let back: SelectionMode = serde_json::from_value(value).unwrap();
            assert_eq!(back, m);
        }
    }

    #[test]
    fn binding_ref_round_trips_filter() {
        let b = BindingRef::Filter {
            name: "status".to_owned(),
        };
        let v = serde_json::to_value(&b).unwrap();
        assert_eq!(v["kind"], json!("filter"));
        let back: BindingRef = serde_json::from_value(v).unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn surface_round_trips_through_json() {
        let s = Surface {
            feature: "customer".to_owned(),
            target: SurfaceTarget::Web,
            audiences: vec![],
            span_ref: None,
        };
        let v = serde_json::to_value(&s).unwrap();
        let back: Surface = serde_json::from_value(v).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn on_success_omits_empty_slots() {
        let os = OnSuccessSpec {
            back: false,
            redirect: None,
            flash: None,
            invalidates: vec![],
            replace: false,
        };
        let v = serde_json::to_value(&os).unwrap();
        let obj = v.as_object().unwrap();
        assert!(obj.is_empty(), "OnSuccessSpec with all-default fields must serialize empty");
    }
}
