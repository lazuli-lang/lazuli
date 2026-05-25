//! Stub IR types for the `.lzx` ViewModel surface used by the `lzx-*` Doctor
//! rules (L0 #3 §6 shape).
//!
//! The richer `.lzx` IR shape (Audience, ViewList, ViewDetail, ViewCreate,
//! QueryRef, CommandRef, CellBinding, RouteParam, PolicyAtom) is not yet on
//! `main` — L2 cells A.1/A.2/A.3 land it in `lazuli_ir` per
//! `docs/proposals/lzx-integration-codegen.md` §16.
//!
//! In the interim, these stubs let the seven `lzx-*` rules and their tests
//! compile and exercise the full check logic. The shapes are intentionally
//! **minimal** — only the fields each rule actually walks, not the full
//! authored surface. When L2 lands richer types in `lazuli_ir`, the
//! orchestrator drops this module and threads the production types through
//! `mod.rs`.
//!
//! Determinism note: all shapes are plain owned structs with no interior
//! mutability; ordering of slices is significant where the rules walk them.

// =============================================================================
// Module / Feature root (sub-shape of `lazuli_ir::Module` and `Feature`)
// =============================================================================

/// Subset of `lazuli_ir::Module` that the `lzx-*` rules consume. Just the
/// feature list — the rules do not look at `app`, `registry`, `design`, etc.
#[derive(Debug, Clone, Default)]
pub struct Module {
    pub features: Vec<Feature>,
}

/// Subset of `lazuli_ir::Feature` that the `lzx-*` rules consume.
#[derive(Debug, Clone, Default)]
pub struct Feature {
    pub name: String,
    pub resources: Vec<Resource>,
    pub queries: Vec<ListQuery>,
    pub commands: Vec<Command>,
    pub surfaces: Vec<Surface>,
}

// =============================================================================
// Resource / Query / Command stubs
// =============================================================================

/// Sub-shape of `lazuli_ir::Resource` carrying just the column-bearing
/// fields the lzx-source-resource-mismatch rule reads.
#[derive(Debug, Clone)]
pub struct Resource {
    pub name: String,
    /// Canonical ID type for selection bindings. Examples: `ID`,
    /// `Item.ID`, `ItemID`.
    pub id_type: String,
    pub fields: Vec<String>,
}

/// Sub-shape of `lazuli_ir::ListQuery` (and similar). `backs_resource` is the
/// query's primary backing resource — Lazuli's planner already computes this
/// per `docs/proposals/lzx-integration-codegen.md` §5.2.
#[derive(Debug, Clone)]
pub struct ListQuery {
    pub name: String,
    pub input_slots: Vec<QueryInput>,
    pub backs_resource: Option<String>,
    /// Names of slots in the query's input block. Search segmented bindings
    /// resolve `source.<name>` against this list.
    #[allow(dead_code)]
    pub input_slot_meta: Vec<QueryInputSlot>,
    /// Query input slots as TypedSlot (for sort_source rule). Mirrors
    /// `input_slots` content but in the typed-name form.
    pub params: Vec<TypedSlot>,
}

/// Sub-shape of `lazuli_ir::TypedSlot` for query/command parameters.
/// Stub carries both a string label (used by sort_source) and a typed
/// `TypeRef` (used by bulk_actions).
#[derive(Debug, Clone)]
pub struct TypedSlot {
    pub name: String,
    pub type_label: String,
    pub type_ref: TypeRef,
    pub required: bool,
}

/// Query input slot metadata consumed by `lzx-search-*` doctor rules.
#[derive(Debug, Clone)]
pub struct QueryInputSlot {
    pub name: String,
    /// `None` means the source input exists, but the lowering pipeline did not
    /// preserve enough metadata for search codegen to compute `alwaysArray`.
    pub cardinality: Option<SearchTargetCardinality>,
}

/// Cardinality model shared by search bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchTargetCardinality {
    Single,
    Multi,
}

#[derive(Debug, Clone)]
pub struct QueryInput {
    pub name: String,
    pub type_label: String,
}

/// Sub-shape of `lazuli_ir::Command` carrying the input slot names + the
/// command's effective policy atoms (one entry per atom).
///
/// `policy_atoms` is the **lowered** form. Adapters that consume a raw
/// `PolicyRef` should resolve it (local refs follow the feature's `policies`
/// declarations to their atom set) and store the flattened result here.
#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    /// Names of slots in the command's `input` block (regardless of typed /
    /// short form).
    pub input_fields: Vec<String>,
    /// Typed input shape consumed by `lzx-bulk-action-input-shape`.
    pub input: CommandInput,
    /// Lowered policy atoms (`@scope.X`, `@role.X`, `@actor.X`). Empty = the
    /// command is public.
    pub policy_atoms: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum CommandInput {
    Short(Vec<String>),
    Typed(Vec<TypedSlot>),
    Empty,
}

impl Default for CommandInput {
    fn default() -> Self {
        CommandInput::Empty
    }
}

/// Closed type catalog the bulk-action input rule walks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    Id,
    QualifiedId(String),
    List(Box<TypeRef>),
    Other(String),
}

// =============================================================================
// Surface / Audience / View
// =============================================================================

/// Top-level `.lzx` file body — one `Surface` per `feature.<target>.lzx`.
#[derive(Debug, Clone)]
pub struct Surface {
    /// The feature this surface targets (`uses feature <feature>`).
    pub feature: String,
    /// `"web"` or `"mobile"`.
    pub target: String,
    pub audiences: Vec<Audience>,
}

/// One `audience <name>` block.
#[derive(Debug, Clone)]
pub struct Audience {
    pub name: String,
    /// Lowered `requires @scope.X` atoms (one entry per `requires` clause).
    pub requires: Vec<String>,
    pub views: Vec<View>,
    /// Source line where `audience <name>` appears — used to anchor
    /// audience-level findings (e.g. `lzx-audience-empty-sdk`).
    pub line: usize,
}

/// Closed view kinds per `docs/proposals/lzx-integration-codegen.md` §5.
#[derive(Debug, Clone)]
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

    pub fn line(&self) -> usize {
        match self {
            View::List(v) => v.line,
            View::Detail(v) => v.line,
            View::Create(v) => v.line,
        }
    }

    pub fn route(&self) -> Option<&str> {
        match self {
            View::List(v) => v.at.as_deref(),
            View::Detail(v) => v.at.as_deref(),
            View::Create(v) => v.at.as_deref(),
        }
    }
}

/// `view list <name> at "<route>"` body.
#[derive(Debug, Clone)]
pub struct ViewList {
    pub name: String,
    pub at: Option<String>,
    pub source: QueryRef,
    /// How this list renders its rows. `Table` is the v1 columns form;
    /// `Cells` is the L0 #6 grid-row slot form (`cells @client.<slot>`).
    pub render: ListRender,
    /// Legacy table columns kept for the existing v1 doctor rules while the
    /// stub remains in use.
    pub columns: Vec<String>,
    /// Legacy `search <field>, <field>` columns used by
    /// `lzx-source-resource-mismatch`.
    pub search: Vec<String>,
    /// Legacy `filter <field>, <field>` names used by
    /// `lzx-source-resource-mismatch`.
    pub filter: Vec<String>,
    /// L0 #6 segmented-search declaration.
    pub search_decl: Option<SearchDecl>,
    /// L0 #6 typed filter declarations.
    pub filter_decls: Vec<FilterDecl>,
    /// L0 #6 selection declaration.
    pub selection: Option<SelectionDecl>,
    /// L0 #6 sort declaration.
    pub sort: Option<SortDecl>,
    pub cells: Vec<CellBinding>,
    pub actions: Vec<CommandRef>,
    pub drawer: Option<DrawerSubView>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct SortDecl {
    pub allowed: Vec<String>,
    pub default_field: String,
    pub default_dir: SortDir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

/// `view list` render discriminator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListRender {
    Table { columns: Vec<String> },
    Cells { slot: String },
}

/// `view detail <name> at "<route>"` body.
#[derive(Debug, Clone)]
pub struct ViewDetail {
    pub name: String,
    pub at: Option<String>,
    pub source: QueryRef,
    /// `route X: Type from path` declarations.
    pub route_params: Vec<RouteParam>,
    pub sections: Vec<String>,
    pub cells: Vec<CellBinding>,
    pub actions: Vec<CommandRef>,
    pub line: usize,
}

/// `view create <name> at "<route>"` body.
#[derive(Debug, Clone)]
pub struct ViewCreate {
    pub name: String,
    pub at: Option<String>,
    pub submit: CommandRef,
    pub fields: Vec<String>,
    pub cells: Vec<CellBinding>,
    pub line: usize,
}

// =============================================================================
// References / bindings
// =============================================================================

/// `<feature>.query.<name>` source reference (the `feature` part may be
/// elided when authored as `<name>` against the current feature).
#[derive(Debug, Clone)]
pub struct QueryRef {
    pub feature: Option<String>,
    pub name: String,
}

/// `<feature>.command.<name>` reference used by `view create.submit` and
/// `view list.actions` / `view detail.actions`.
#[derive(Debug, Clone)]
pub struct CommandRef {
    pub feature: Option<String>,
    pub name: String,
    /// Source line — used to anchor `lzx-action-not-in-audience` findings
    /// to the exact `actions <command>, <command>` line.
    pub line: usize,
}

/// `cells <field> @client.<slot>` binding.
#[derive(Debug, Clone)]
pub struct CellBinding {
    pub field: String,
    /// `@client.<slot>` — stored verbatim including the prefix so the rule
    /// can reproduce it in error messages without re-stitching.
    pub slot: String,
    pub line: usize,
}

/// `route <name>: <Type> from path` declaration.
#[derive(Debug, Clone)]
pub struct RouteParam {
    pub name: String,
    /// Verbatim type label (`Text`, `Id`, etc.) — opaque to the doctor
    /// rules; the lowering pass enforces a closed catalog.
    pub type_label: String,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct DrawerSubView {
    pub name: String,
    pub source: QueryRef,
    pub route_binding: Option<DrawerRouteBinding>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct DrawerRouteBinding {
    pub target: String,
    pub source: DrawerBindingSource,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawerBindingSource {
    Selection,
}

// =============================================================================
// L0 #6 terminal-search declarations
// =============================================================================

/// Sub-shape of `lazuli_ir::FilterDecl`.
#[derive(Debug, Clone)]
pub struct FilterDecl {
    pub name: String,
    pub cardinality: SearchTargetCardinality,
}

/// Sub-shape of `lazuli_ir::SearchDecl`.
#[derive(Debug, Clone)]
pub struct SearchDecl {
    pub fields: Vec<SearchField>,
    pub free_text_target: Option<BindingRef>,
}

/// Sub-shape of `lazuli_ir::SearchField`.
#[derive(Debug, Clone)]
pub struct SearchField {
    pub key: String,
    pub binds_to: BindingRef,
}

/// Sub-shape of `lazuli_ir::BindingRef`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingRef {
    Filter { name: String },
    SourceInput { name: String },
    SelectionScalar,
}

/// Sub-shape of `lazuli_ir::SelectionDecl`.
#[derive(Debug, Clone)]
pub struct SelectionDecl {
    pub mode: SelectionMode,
    /// Lzx-bulk-actions list of command refs. Used by
    /// `lzx-bulk-action-input-shape` and `lzx-bulk-actions-require-multi`.
    pub bulk_actions: Vec<CommandRef>,
}

/// Sub-shape of `lazuli_ir::SelectionMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    None,
    Single,
    Multi,
}
