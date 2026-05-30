//! Surface AST — modern hand-written mirror for the per-target lzx
//! ViewModel files (`features/<feat>/<feat>.{web,mobile}.lzx`).
//!
//! Reference: `docs/proposals/lzx-integration-codegen.md` §5 (closed
//! keyword catalog) + §5.1 (per-view-kind matrix). Field-level type
//! references are kept as raw text; the analyzer lifts them to `ir::*`
//! in `lower_surface`. The indentation-based parser populates this via
//! `parse_surface_decl`.
//!
//! The closed view-kind catalog (`ViewAst`) is `List | Detail | Create`.
//! Each shape has its own struct so the analyzer can switch on kind at
//! lowering time without re-walking the AST. The catalog is intentionally
//! small — adding a view kind is an IR + analyzer change requiring a
//! proposal.
//!
//! Authoring shape (excerpt):
//!
//! ```text
//! surface customer_management web
//!   uses feature customer
//!   audience admin
//!     requires @scope.admin
//!     view list customers
//!       source customer.query.list
//!       route "/customers"
//!       columns name, email, owner
//!       filters
//!         tier: @semantic.CustomerTier multi url_sync
//!         status: @semantic.CustomerStatus single
//!       search columns name, email
//!       sort
//!         allowed name, email, created_at
//!         default created_at desc
//!       selection multi
//!         bulk_actions assign_owner, archive
//!       settings
//!         columns: enum [name, email, owner] default name persistence local
//!       drawer
//!         trigger select
//!         source customer.query.lookup.by_id
//!         sections summary
//!         actions edit_customer
//!       cells owner @client.OwnerCell
//!       actions create_customer
//! ```
//!
//! `RouteParamAst` doubles as the typed-route-param surface for both
//! lzx routes and view-detail/view-create headers (`route id: ID from
//! path`). It's exported from this file because it lives closest to the
//! views that consume it.

use serde::{Deserialize, Serialize};

use super::{InvalidatesDecl, PolicyAtomAst, Span, TranslationKeyRefAst};

/// A `.lzx` surface declaration — one per `<feat>.<target>.lzx` file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceAst {
    /// `surface <feature> web|mobile` — first token after `surface`.
    pub feature: String,
    /// `web` or `mobile`. Validated at parse time.
    pub target: SurfaceTargetAst,
    /// Optional `uses feature <feature>` override. When absent, the
    /// surface's owning feature (derived from the enclosing file path)
    /// is assumed.
    pub uses_feature: Option<String>,
    pub audiences: Vec<AudienceAst>,
    pub span: Span,
}

/// `surface <feature> web|mobile` target catalog. Closed; adding a
/// target is an IR + codegen change requiring a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceTargetAst {
    /// `surface <feature> web` — React / web target.
    Web,
    /// `surface <feature> mobile` — React Native / Expo target.
    Mobile,
}

/// `audience <name>` block inside a `.lzx` surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudienceAst {
    pub name: String,
    /// `requires @scope.<name>` (and reserved future `@role.<name>` /
    /// `@actor.<name>`) — one entry per `requires` line.
    pub requires: Vec<PolicyAtomAst>,
    pub views: Vec<ViewAst>,
    /// Wave-W6 audience-level containers (`tabs`, `wizard`). Empty by default.
    #[serde(default, skip_serializing_if = "AudienceUxAst::is_empty")]
    pub ux: AudienceUxAst,
    pub span: Span,
}

/// Closed view-kind catalog mirroring `ir::View`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ViewAst {
    List(ViewListAst),
    Detail(ViewDetailAst),
    Create(ViewCreateAst),
}

impl ViewAst {
    /// The view name, irrespective of variant.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_syntax::{Span, ViewAst, ViewListAst};
    ///
    /// let v = ViewAst::List(ViewListAst {
    ///     name: "customers".into(),
    ///     route: None,
    ///     source: "customer.query.list".into(),
    ///     columns: vec![],
    ///     search: None,
    ///     filter: vec![],
    ///     filters: vec![],
    ///     cells_slot: None,
    ///     cells: vec![],
    ///     drawer: None,
    ///     sort: None,
    ///     selection: None,
    ///     settings: vec![],
    ///     actions: vec![],
    ///     redacted_fields: vec![],
    ///     ux: Default::default(),
    ///     span: Span::new(0, 0),
    /// });
    /// assert_eq!(v.name(), "customers");
    /// ```
    pub fn name(&self) -> &str {
        match self {
            ViewAst::List(v) => &v.name,
            ViewAst::Detail(v) => &v.name,
            ViewAst::Create(v) => &v.name,
        }
    }
}

/// `view list <name>` — paginated/filterable collection view. Carries
/// the full surface for list-style screens (columns, filters, search,
/// sort, selection, settings, drawer, cells, actions).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewListAst {
    pub name: String,
    pub route: Option<String>,
    /// `source <feature>.query.<name>` — kept as raw `feature.query.name`
    /// text; analyzer splits into a `QueryRef`.
    pub source: String,
    pub columns: Vec<String>,
    pub search: Option<SearchDeclAst>,
    pub filter: Vec<String>,
    /// `filters` block declarations for typed view-local filter state.
    pub filters: Vec<FilterDeclAst>,
    /// `cells @client.<slot>` grid-row slot. `None` means the view either
    /// uses per-column `cells <field> @client.<slot>` bindings or no cells.
    pub cells_slot: Option<String>,
    pub cells: Vec<CellBindingAst>,
    pub drawer: Option<DrawerSubViewAst>,
    pub sort: Option<SortDeclAst>,
    pub selection: Option<SelectionDeclAst>,
    pub settings: Vec<SettingDeclAst>,
    /// `actions <cmd>, <cmd>` — comma-separated short names or qualified
    /// `<feature>.command.<name>` references. Analyzer normalizes.
    pub actions: Vec<String>,
    /// `fields <name> redacted` rows declared inside the view.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redacted_fields: Vec<String>,
    /// Wave-W6 view-level primitives (`wizard_steps`, `tab_group`,
    /// `view_mode`, `view.inline_table`). Empty by default.
    #[serde(default, skip_serializing_if = "ViewUxAst::is_empty")]
    pub ux: ViewUxAst,
    pub span: Span,
}

/// `search` block inside a [`ViewListAst`] — declares the search input
/// surface and its binding into the underlying query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchDeclAst {
    pub mode: SearchModeAst,
    pub fields: Vec<SearchFieldAst>,
    /// Optional free-text bind target (e.g. when one field acts as the
    /// catch-all input). `None` when only typed `fields` are authored.
    pub free_text_target: Option<BindingRefAst>,
    pub span: Span,
}

/// `search` mode catalog — `columns name, email, ...` (closed list) or
/// `segmented` (UI-driven facet selector).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SearchModeAst {
    /// `columns <list>` mode — typed column projection.
    Columns(Vec<String>),
    /// `segmented` mode — UI-driven facet picker.
    Segmented,
}

/// One typed `<key> binds_to <ref>` row inside [`SearchDeclAst`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchFieldAst {
    pub key: String,
    pub binds_to: BindingRefAst,
    pub span: Span,
}

/// Where a filter / search input binds — one of the view's named
/// filters, a source-query input, or the selection scalar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum BindingRefAst {
    /// Bind into a named filter declared via [`FilterDeclAst`].
    Filter { name: String },
    /// Bind into one of the source query's typed `params` slots.
    SourceInput { name: String },
    /// Bind into the row-selection scalar (single-select id).
    SelectionScalar,
}

/// `view detail <name>` — single-row detail screen driven by a lookup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewDetailAst {
    pub name: String,
    pub route: Option<String>,
    pub source: String,
    pub route_params: Vec<RouteParamAst>,
    pub sections: Vec<String>,
    pub cells: Vec<CellBindingAst>,
    pub actions: Vec<String>,
    /// `fields <name> redacted` rows declared inside the view.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redacted_fields: Vec<String>,
    /// Wave-W6 view-level primitives (`wizard_steps`, `tab_group`).
    /// Empty by default.
    #[serde(default, skip_serializing_if = "ViewUxAst::is_empty")]
    pub ux: ViewUxAst,
    pub span: Span,
}

/// `view create <name>` — submit-driven create screen with optional
/// `on_success` orchestration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewCreateAst {
    pub name: String,
    pub route: Option<String>,
    /// `submit <feature>.command.<name>` — qualified reference text.
    pub submit: String,
    /// `on_success` post-submit orchestration block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_success: Option<OnSuccessSpecAst>,
    pub fields: Vec<String>,
    pub cells: Vec<CellBindingAst>,
    /// `fields <name> redacted` rows declared inside the view.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redacted_fields: Vec<String>,
    pub span: Span,
}

/// `on_success` sub-block on a [`ViewCreateAst`] — post-submit
/// orchestration (back / redirect / flash / invalidates / replace).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnSuccessSpecAst {
    /// `back` flag — pop the route stack after submit.
    pub back: bool,
    /// `redirect "<path>"` — explicit redirect target.
    pub redirect: Option<String>,
    /// `flash` toast specification.
    pub flash: Option<FlashSpecAst>,
    /// `invalidates query.<name>` references to bust after submit.
    pub invalidates: Vec<InvalidatesDecl>,
    /// `replace` flag — when redirecting, use `history.replaceState`.
    pub replace: bool,
    pub span: Span,
}

/// `flash <kind> @translation.<key>` toast specification used by
/// [`OnSuccessSpecAst`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlashSpecAst {
    /// Flash kind (`info`, `success`, `error`, ...). Verbatim identifier.
    pub kind: String,
    /// `@translation.<key>` reference for the toast body.
    pub message_key: TranslationKeyRefAst,
    pub span: Span,
}

/// `drawer` sub-view inside a [`ViewListAst`] — slide-over detail view
/// triggered from a row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawerSubViewAst {
    pub name: String,
    pub trigger: DrawerTriggerAst,
    pub source: String,
    pub route_binding: Option<DrawerRouteBindingAst>,
    pub sections: Vec<String>,
    pub cells: Vec<CellBindingAst>,
    pub actions: Vec<String>,
    pub span: Span,
}

/// Drawer-open trigger catalog. `select` opens on row-click;
/// `manual_open` waits for an explicit `actions ...` toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawerTriggerAst {
    /// Opens on row selection.
    Select,
    /// Opens on an explicit action toggle.
    ManualOpen,
}

/// One `route <target>: <source>` binding on a [`DrawerSubViewAst`] —
/// hooks the drawer's source-query slot into the selected row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawerRouteBindingAst {
    pub target: String,
    pub source: DrawerBindingSourceAst,
}

/// Where the drawer route binding sources its value from. Today: only
/// `selection` (the selected-row id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawerBindingSourceAst {
    /// `from selection` — derived from the row selection.
    Selection,
}

/// `cells <field> @client.<slot>` parsed binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellBindingAst {
    pub field: String,
    /// Slot identifier (without the `@client.` prefix).
    pub slot: String,
    pub span: Span,
}

/// `filters` block field declaration inside `view list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterDeclAst {
    pub name: String,
    /// Raw label as authored; resolved during lowering.
    pub type_ref: String,
    pub cardinality: FilterCardinalityAst,
    pub url_sync: bool,
    pub span: Span,
}

/// Closed three-arm catalog for filter cardinality
/// (`single` / `multi` / `date_range`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterCardinalityAst {
    /// `single` — one selection at a time.
    Single,
    /// `multi` — multiple selections (chip / pill UI).
    Multi,
    /// `date_range` — paired from/to date picker (GAP-UX-07). Surfaces two
    /// query params (`<name>_from` / `<name>_to`) bound to a single Date /
    /// DateTime field on the resource.
    DateRange,
}

/// `route <name>: <Type> from path` — typed path parameter. Shared by
/// `.lzx` app-level routes (lifted via `LzxRoute.route_params`) and the
/// per-view-detail/create headers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteParamAst {
    pub name: String,
    pub type_ref: String,
    pub span: Span,
}

/// `sort` block inside a `view list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortDeclAst {
    pub allowed: Vec<String>,
    pub default_field: String,
    pub default_dir: SortDirAst,
    pub span: Span,
}

/// Closed two-arm catalog for sort direction (`asc` / `desc`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirAst {
    /// `asc` — ascending order.
    Asc,
    /// `desc` — descending order.
    Desc,
}

/// `selection single|multi` plus optional `bulk_actions` folded in at
/// view assembly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionDeclAst {
    pub mode: SelectionModeAst,
    pub bulk_actions: Vec<String>,
    pub span: Span,
}

/// Closed three-arm catalog for view selection mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionModeAst {
    /// `selection none` — no row selection UI.
    None,
    /// `selection single` — one row selected at a time.
    Single,
    /// `selection multi` — multi-row selection (drives bulk actions).
    Multi,
}

/// One child declaration inside a `settings` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingDeclAst {
    pub name: String,
    pub value_space: SettingValueSpaceAst,
    pub default: String,
    pub persistence: SettingPersistenceAst,
    pub span: Span,
}

/// Closed catalog of value spaces for a [`SettingDeclAst`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SettingValueSpaceAst {
    /// `enum [a, b, c]` — closed list of identifiers.
    Enum(Vec<String>),
    /// `bool` — two-state toggle.
    Bool,
    /// `int [<min>, <max>]` — bounded integer range; either bound may
    /// be open.
    Int { min: Option<i64>, max: Option<i64> },
}

/// Closed catalog of persistence scopes for a [`SettingDeclAst`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingPersistenceAst {
    /// `persistence none` — ephemeral, per-session.
    None,
    /// `persistence local` — per-browser via localStorage.
    Local,
    /// `persistence workspace` — server-stored, follows the user's workspace.
    Workspace,
}

// ===========================================================================
// Wave-W6 surface UX primitives (GAP-UX-01..04). Mirrors `lazuli_ir::ux`.
// ===========================================================================

/// Aggregate of view-level W6 primitives carried on a list/detail view.
/// Defaults to "no extra UX surface".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewUxAst {
    /// `wizard_steps <total> current <field>` (GAP-UX-01).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wizard_steps: Option<WizardStepsAst>,
    /// `tab_group derived_from <field> { ... }` (GAP-UX-02).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_group: Option<TabGroupAst>,
    /// `view_mode { table; kanban }` (GAP-UX-04).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub view_modes: Vec<String>,
    /// `view.inline_table on_change @command.X` (GAP-UX-04).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_table: Option<InlineTableAst>,
    /// `view.board <name> / lanes derived_from <field>` (GAP-UX-05).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<BoardAst>,
    /// `repeatable input <name> group <f>: <T>, … validates sum(<f>) = <n>`
    /// (GAP-UX-05).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repeatable_groups: Vec<RepeatableGroupAst>,
}

impl ViewUxAst {
    /// True when no W6/GAP-UX-05 view primitive is declared.
    pub fn is_empty(&self) -> bool {
        self.wizard_steps.is_none()
            && self.tab_group.is_none()
            && self.view_modes.is_empty()
            && self.inline_table.is_none()
            && self.board.is_none()
            && self.repeatable_groups.is_empty()
    }
}

/// Aggregate of audience-level W6 containers (`tabs`, `wizard`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudienceUxAst {
    /// `tabs { tab "X" -> view v }` static containers (GAP-UX-03).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<TabsAst>,
    /// `wizard <name> steps { step N: ref }` containers (GAP-UX-03).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wizards: Vec<WizardAst>,
}

impl AudienceUxAst {
    /// True when the audience declares no tabs or wizards.
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty() && self.wizards.is_empty()
    }
}

/// `wizard_steps <total> current <field>` — step indicator (GAP-UX-01).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WizardStepsAst {
    pub total: u32,
    pub current_field: String,
    pub span: Span,
}

/// `tab_group derived_from <field>` runtime-derived tabs (GAP-UX-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabGroupAst {
    pub derived_from: String,
    pub cases: Vec<TabGroupCaseAst>,
    pub span: Span,
}

/// One `case <V1, V2> -> tab "<label>"` arm of a [`TabGroupAst`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabGroupCaseAst {
    pub variants: Vec<String>,
    pub label: String,
    pub span: Span,
}

/// `tabs { tab "<label>" -> view <name> [audience <a>] }` (GAP-UX-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabsAst {
    pub entries: Vec<TabEntryAst>,
    pub span: Span,
}

/// One `tab "<label>" -> view <name>` row of a [`TabsAst`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabEntryAst {
    pub label: String,
    pub view: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    pub span: Span,
}

/// `wizard <name> steps { step N: <ref> }` multi-step container (GAP-UX-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WizardAst {
    pub name: String,
    pub steps: Vec<WizardStepAst>,
    pub span: Span,
}

/// One `step <N>: <ref>` row of a [`WizardAst`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WizardStepAst {
    pub index: u32,
    pub ref_name: String,
    pub span: Span,
}

/// `view.inline_table on_change @command.<name>` (GAP-UX-04). `on_change`
/// is kept as raw `@command.<name>` text; the analyzer normalizes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineTableAst {
    pub on_change: String,
    pub span: Span,
}

/// `view.board <name>` + `lanes derived_from <field>` (GAP-UX-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardAst {
    /// Optional board name from the header (`view.board <name>`); empty when
    /// omitted.
    pub name: String,
    /// `lanes derived_from <field>` — the enum field / has_many relation.
    pub lanes_source: String,
    pub span: Span,
}

/// `repeatable input <name> group <fields> validates sum(<f>) = <n>`
/// (GAP-UX-05). The `sum_target` is kept verbatim (parser-validated numeric).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatableGroupAst {
    pub name: String,
    pub fields: Vec<RepeatableFieldAst>,
    pub sum_field: String,
    pub sum_target: String,
    pub span: Span,
}

/// One `<name>: <Type>` field inside a [`RepeatableGroupAst`]'s `group { … }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatableFieldAst {
    pub name: String,
    pub type_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_target_ast_serde_snake_case() {
        assert_eq!(
            serde_json::to_value(SurfaceTargetAst::Web).unwrap(),
            serde_json::json!("web")
        );
        assert_eq!(
            serde_json::to_value(SurfaceTargetAst::Mobile).unwrap(),
            serde_json::json!("mobile")
        );
    }

    #[test]
    fn selection_mode_ast_default_via_serde_token() {
        assert_eq!(
            serde_json::to_value(SelectionModeAst::Multi).unwrap(),
            serde_json::json!("multi")
        );
    }

    #[test]
    fn binding_ref_ast_filter_serde_carries_name() {
        let r = BindingRefAst::Filter {
            name: "status".into(),
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["kind"], "filter");
        assert_eq!(v["value"]["name"], "status");
    }
}
