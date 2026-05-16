# Proposal — `.lzx` Terminal Grammar (Rich-View ViewModel Primitives)

**Status:** L0 v0.2 PASS @ 9.05/10 — 2026-05-14 (v0.1 graded BLOCK 7.7/10 via `lazuli-language-architect`; v0.2 applies 4 blockers + 10 polish items inline; v0.2 re-graded PASS, 7 secondary polish items applied below + 4 tracked cuts for `docs/next-checklist.md`)
**Author:** Claude Opus 4.7 (orchestrator)
**Audit-ready target:** ≥ 9.0 via `lazuli-language-architect`
**Depends on:** `docs/proposals/lzx-integration-codegen.md` (L0 #3 — current `.lzx` v1 grammar), `docs/proposals/lazurite-frontend-folder-canon.md` (L0 #1)
**Honors:** `docs/invariants.md`, `docs/design-principles.md` (Rule Zero — Vocabulary Over Mechanism)

---

## §1. Status & motivation

`.lzx` v1 (shipped 2026-05-14 in `lazuli@bcec1b2`) gives products three view kinds — `view list`, `view detail`, `view create`. Pleiades, Atelier, and Erudito all need a fourth pattern that v1 cannot express: a **rich grid surface** where:

- N items render via a user-authored cell component (not table rows or detail sections).
- Selecting an item opens an **in-place drawer** (same URL, lateral state), not a new route.
- A **typed filter bar** (type / status / tags / slug etc.) drives the query input with URL persistence.
- A **segmented search** parses `slug:foo type:doc +free text` into structured filter values.
- Sort order, multi-select / bulk actions, and presentation settings (grid density) are part of the view contract.

In `pleiades-os/apps/terminal/src/features/terminal/` the legacy implementation is **26 files, 6062 LOC** (15 of those are `.tsx`, ~4300 LOC of UI; the rest are domain, types, search-segments, selection, api/hooks, tests, and the AI context doc). All of the cross-cutting state — filters, search, selection, drawer, settings — is ad-hoc React `useState`. The pattern repeats: Atelier's media library, Erudito's lesson catalog, Pleiades' Crafting and Patterns views all reach for the same shape. Authoring each as raw React reintroduces the drift-prone plumbing layer L0 #3 was written to eliminate.

This proposal extends the `.lzx` ViewModel grammar with the **eight primitives that v1 is missing for grid-shaped views**. All eight stay strictly in viewmodel territory — no JSX, no styling, no rendering primitives. Codegen emits typed hooks; user `.tsx` consumes them with whatever component library it wants (shadcn, Mantine, custom). The Terminal feature becomes ~35 lines of `.lzx` + ~600 LOC of presentational `.tsx` spread across 4-5 files (cell, drawer body, drawer edit, content viewer, filters bar — UI separation the legacy already used and which has nothing to do with the cross-cutting state we are lifting).

**Why now**: Pleiades web is live (`pleiades-web-mvvm-proof-2026-05-14`). Terminal is the next feature gate. Without this proposal each product reinvents grid + drawer + filters wiring; with it, the pattern is one declarative spec consumable by all three.

---

## §2. Guiding principle — the Flutter-2 line

`.lzx` describes **what data and state a view needs**, never **what the view looks like**.

| `.lzx` CAN declare | `.lzx` CANNOT declare |
|---|---|
| "this view has a drawer with sub-source X" | "drawer slides in from the right with 200ms easing" |
| "filters are `type: ItemType, tags: list of Text`" | "render a Popover of checkboxes" |
| "search parses `slug:foo type:doc +free text`" | "input with rounded chips, badge color X" |
| "view supports multi-select with bulk delete" | "checkbox appears top-left on hover" |
| "settings.grid_size persists local" | "three radio buttons in a row" |

Every primitive in §3 below is tested against that table. If a primitive would force a rendering decision into IR, it is **rejected** (annotated in §7).

Same rule that already governs `view list`: today it doesn't emit a `<Table>` — it emits `query.data`. The user picks ul, table, or divs. The new primitives follow the identical pattern.

---

## §3. Grammar additions

### §3.1 `view grid` kind (or `view list` accepting `cells` in place of `columns`)

**Today (`.lzx` v1):**

```
view list slug_list at "/slugs"
  source slug.query.mine
  columns key, title, created_at
  actions create, update, delete
```

**Proposed — Option A (new kind):**

```
view grid item_terminal at "/"
  source item.query.search
  cell @client.item_card
  actions update, delete
```

**Proposed — Option B (unify under `view list`, with explicit `@client.` namespace):**

```
view list item_terminal at "/"
  source item.query.search
  cells @client.item_card     # mutually exclusive with `columns`
  actions update, delete
```

**Recommendation:** Option B. `view list` already declares "this view shows N rows of resource T"; whether rows render as table cells or cards is a presentation choice. Adding `cells @client.<slot>` mutually exclusive with `columns [...]` extends v1 without inflating the kind axis. Doctor rule `lzx-list-cells-or-columns` enforces exactly one.

**Disambiguation with v1 `cells` (per-column slot binding).** v1 syntax is `cells <field> @client.<slot>` (two tokens, field-first); v0.2 grid form is `cells @client.<slot>` (one token, immediately namespaced). Forcing the `@client.` prefix on the grid form means the parser disambiguates on the **first token after `cells`**: if it is `@client.<x>`, the form is a grid-row slot; if it is a bare identifier, the form is a per-column field binding. The grammar stays context-free and the v1 catalog rule "all client-implemented slots are namespaced `@client.<name>`" is preserved.

**Discriminator scope.** The rule is bound specifically to the `@client.` prefix as the grid-form sentinel. Future namespace prefixes added to `.lzx` (e.g. a hypothetical `@semantic.<x>` or `@plugin.<x>` after `cells`) are **NOT** auto-routed to the grid form — each new namespace requires an explicit grammar amendment naming which form it joins. Doctor messages on the mixed-form rule include the hint "grid form is `cells @client.<slot>`; per-column form is `cells <field> @client.<slot>`" so cold-reading authors do not need to learn the discriminator internals.

A new doctor rule `lzx-cells-mixed-form` rejects views that try both forms in the same `view list` block (e.g. a per-column binding for `tags` AND a grid-row cell slot for `item_card`) — those are semantically different views and should be split.

**IR change** in `lazuli_ir::ViewList`:

```rust
pub struct ViewList {
    // ...existing fields...
    pub render: ListRender,  // NEW
    // existing `columns: Vec<String>` becomes part of ListRender::Table
}

pub enum ListRender {
    Table { columns: Vec<String> },
    Cells { slot: String },  // "@client.<slot>" reference
}
```

**Emitted hook shape** (Cells variant):

```typescript
export function useAdminItemTerminalView() {
  // ...existing query/actions/meta...
  // PLUS: typed cell slot interface AdminItemTerminalCells
  return { query, actions, meta /* includes cell slot name */ };
}

export interface AdminItemTerminalCells {
  ItemCard: React.ComponentType<{ item: Item }>;
}
```

User `.tsx` implements `ItemCard` and renders `{query.data.map(item => <ItemCard item={item}/>)}`. No grid CSS in IR — just "this is the slot you implement, one per item".

### §3.2 `drawer` sub-view

**Problem:** Today `view detail` is route-based (`/items/$id`). Terminal selects an item and pops a side panel **at the same URL**. The state machine is "drawer.open(id) | drawer.close()", parameterized by a sub-query.

**Proposed grammar:**

```
view list item_terminal at "/"
  source item.query.search
  cells item_card
  actions update, delete

  drawer item_detail on select
    source item.query.by_id
    route key from selection
    sections header, content, metadata
    cells related @client.related_items
    actions update, delete
```

The `drawer` block is a **sub-view declaration** inside its host. It inherits the host's audience (drawer's `actions` are checked against the same audience as host actions — same policy graph traversal, same `lzx-action-not-in-audience` rule). `on select` declares the trigger; see §3.9 for the precise click-vs-select disambiguation when `selection multi` is also declared. `route key from selection` binds the host's selection state to the sub-query's input (the keyword is `from`, matching v1's `route key: Text from path`).

**IR additions:**

```rust
pub struct ViewList {
    // ...existing...
    pub drawer: Option<DrawerSubView>,  // NEW
}

pub struct DrawerSubView {
    pub name: String,                    // "item_detail"
    pub trigger: DrawerTrigger,          // Select | ManualOpen
    pub source: QueryRef,                // sub-query
    pub route_binding: Option<RouteBinding>,
    pub sections: Vec<String>,
    pub cells: Vec<CellBinding>,
    pub actions: Vec<CommandRef>,
}

pub enum DrawerTrigger {
    Select,        // click on a host cell opens drawer with that item
    ManualOpen,    // user code calls .open(id) explicitly
}
```

**Emitted hook shape:**

```typescript
export function useAdminItemTerminalView() {
  const list = useLazuliQuery(...);
  const [drawerId, setDrawerId] = useState<string | null>(null);
  const drawerQuery = useLazuliQuery(lookupItemById, { id: drawerId! }, { enabled: drawerId !== null });

  return {
    query: list,
    actions: { /* host actions */ },
    drawer: {
      isOpen: drawerId !== null,
      item: drawerQuery.data ?? null,
      query: drawerQuery,
      open: (id: string) => setDrawerId(id),
      close: () => setDrawerId(null),
      actions: { /* drawer-scoped actions */ },
    },
    meta: adminItemTerminalView,
  } as const;
}
```

User .tsx:

```tsx
<TerminalGrid items={query.data}
  onCellClick={(item) => drawer.open(item.id)}
  renderCell={({ item }) => <ItemCard item={item} />}
/>
<Sheet open={drawer.isOpen} onOpenChange={(o) => o || drawer.close()}>
  <SheetContent>{drawer.item && <ItemDrawerBody item={drawer.item} />}</SheetContent>
</Sheet>
```

Lazuli does NOT pick `<Sheet>`. User decides Sheet vs Dialog vs custom slide-out.

### §3.3 Filters — typed, multi-value, optional URL sync

**Problem:** v1 `filter [tags]` only lists which fields are filterable, with no type info or state machine.

**Proposed grammar:**

```
view list item_terminal at "/"
  source item.query.search
  cells item_card

  filters
    type: ItemType
    status: ItemStatus
    confidence: Confidence
    tags: list of Text
    slug: Text from query
```

- `type: ItemType` declares a single-value enum filter.
- `tags: list of Text` declares a multi-value filter.
- `from query` declares URL persistence: single-value syncs as `?slug=foo`; multi-value syncs as **repeated key** `?tags=a&tags=b` (chosen over `?tags=a,b` because commas are valid in tag values; chosen over `?tags[]=a&tags[]=b` because TanStack Router does not auto-decode bracket-notation as arrays — repeated-key is the lowest-common-denominator across TanStack / Next App Router / Expo Router and matches the URLSearchParams spec).
- Missing `from query` → filter state is React-only (no URL persistence).

**IR additions:**

```rust
pub struct FilterDecl {
    pub name: String,             // "type"
    pub type_ref: String,         // "ItemType" — resolves to an enum on the resource OR a scalar
    pub cardinality: FilterCardinality,
    pub url_sync: bool,
}

pub enum FilterCardinality {
    Single,
    Multi,
}
```

**Emitted hook shape:**

```typescript
filters: {
  type: FilterState<ItemType | "all">;         // single-value, "all" = unset
  status: FilterState<ItemStatus | "all">;
  tags: MultiFilterState<string>;
  slug: FilterState<string | null>;            // url-synced
},

interface FilterState<T> {
  value: T;
  set(value: T): void;
  clear(): void;
}

interface MultiFilterState<T> {
  value: T[];
  add(value: T): void;
  remove(value: T): void;
  toggle(value: T): void;
  clear(): void;
}
```

Filters automatically pass through to the source query's input. URL-synced filters use `useSearchParams()` from the router adapter.

### §3.4 Search — segmented parsing (wires `search-query-parser`)

**Problem:** Terminal's search input parses `slug:welcome type:doc tag:onboarding +free text` into structured segments. v1's `search [columns]` only declares which fields free-text search hits.

**Proposed grammar:**

```
view list item_terminal at "/"
  source item.query.search
  cells @client.item_card
  filters ...

  search segmented
    field slug binds filters.slug
    field type binds filters.type
    field tag binds filters.tags
    free text into source.q
```

**Parser is wired, not reimplemented.** The codegen emits a hook that calls `searchQuery.parse(raw, { keywords, alwaysArray })` from the OSS [`search-query-parser`](https://github.com/nepsilon/search-query-parser) library (1.6.0, MIT, zero deps, ~187 KB unpacked, exact-fit API for `key:value +free text` with multi-value support via `alwaysArray`). The mapping is:

| `.lzx` decl | `search-query-parser` config |
|---|---|
| `field <key> binds filters.<single-value>` | `keywords: [...key]` |
| `field <key> binds filters.<multi-value>` | `keywords: [...key], alwaysArray: [...key]` |
| `free text into <target>` | parser's `text` output → target |

No `@lazuli/runtime/search-segments` module. Cell E.1 from v0.1 is **deleted**; the runtime is the library, end of story. Lazuli's contribution is the field-binding metadata + the round-trip emission rule below, both ≤ 40 LOC of generated code.

**IR additions:**

```rust
pub struct SearchDecl {
    pub mode: SearchMode,
    pub fields: Vec<SearchField>,
    pub free_text_target: Option<BindingRef>,  // e.g. "source.q"
}

pub enum SearchMode {
    Columns(Vec<String>),    // v1 behavior
    Segmented,
}

pub struct SearchField {
    pub key: String,                  // "slug"
    pub binds_to: BindingRef,         // typed reference into the host view's state
}
```

`BindingRef` (used here, §3.5, §3.6 too) is a typed enum, not a `String`:

```rust
pub enum BindingRef {
    Filter { name: String },          // filters.<name>
    SourceInput { name: String },     // source.<input-name>
    SelectionScalar,                  // selection (current single-selected id)
}
```

Resolution happens in lowering; doctor rule `lzx-search-binds-target-exists` checks each ref resolves against the host view's declared `filters` / `source.inputs` / `selection`.

**Canonical user-`.tsx` pattern** (the one v0.2 codegen optimizes for):

```tsx
<input value={search.raw} onChange={(e) => search.setRaw(e.target.value)} />
{search.segments.map(seg => <Chip key={...} segment={seg} />)}
{/* derivedFromFilters used only for "reset to canonical" buttons or dirty-state markers */}
```

Authors touch `derivedFromFilters` only for a reset button (`onClick={() => search.setRaw(search.derivedFromFilters)}`) or to visually mark the input dirty when `raw !== derivedFromFilters`. The dual fields exist to give those affordances without forcing the hook to auto-resync.

**Emitted hook shape — dual fields for round-trip (resolves BLOCKER-D):**

```typescript
search: {
  raw: string;                        // user-typed input — bind to <input value>
  derivedFromFilters: string;         // canonical re-emission of current filter+free state
  segments: ParsedSegment[];          // exposed for chip-style display
  setRaw(input: string): void;        // user typed in the input
  clear(): void;
},

interface ParsedSegment {
  kind: "filter" | "free";
  field?: string;
  value: string;
}
```

**Round-trip semantics (canonical, codified in v0.2):**

1. `raw` is the literal user input. User-`.tsx` binds `<input value={search.raw} onChange={e => search.setRaw(e.target.value)}/>`.
2. `setRaw(input)` parses with `search-query-parser` and writes each parsed segment to its `binds_to` target. Filters and `source.q` update.
3. `derivedFromFilters` is recomputed every render from current filter + free state, in deterministic canonical form: `<field>:<value>` segments in **alphabetical key order**, each repeated for multi-value, then a single space, then free text. Example: current state `{filters.type = "doc", filters.tags = ["a","b"], source.q = "onboarding"}` → `"tag:a tag:b type:doc onboarding"`.
4. **Authority:** `raw` is authoritative for what the user sees in the input field; `derivedFromFilters` is authoritative for the query input. User-`.tsx` typically binds the `<input>` to `raw` and surfaces a "reset to canonical" button (or a chip UI) that calls `search.setRaw(search.derivedFromFilters)`.
5. **Drift detection:** consumers that want chip+input sync at all times can compare `raw === derivedFromFilters` and visually mark the input dirty when they diverge. The hook does NOT auto-resync — divergence is a meaningful UX signal (user typed something but didn't commit).

This makes the round-trip a user-decided UX policy, not a hidden hook behavior. Doctor rule `lzx-search-binds-target-exists` is unchanged from v0.1; an additional rule `lzx-search-field-multi-cardinality` flags `field <k> binds filters.<f>` where the binding's cardinality (single vs multi) is unclear — every binding must resolve to a known cardinality so the `alwaysArray` config is computable at emit time.

### §3.5 Sort

**Proposed grammar (matches v1's `columns key, title, tags` lexical form — no brackets):**

```
sort
  by title, type, priority, updated
  default updated desc
```

**IR:**

```rust
pub struct SortDecl {
    pub allowed: Vec<String>,
    pub default_field: String,
    pub default_dir: SortDir,
}

pub enum SortDir { Asc, Desc }
```

**Emitted:**

```typescript
sort: {
  field: "title" | "type" | "priority" | "updated";
  dir: "asc" | "desc";
  set(field: SortField, dir?: SortDir): void;
}
```

Passes through to the source query as `{ sort: "title", dir: "asc" }` query input fields. Doctor verifies the source query accepts `sort` + `dir` inputs.

### §3.6 Selection — single / multi + bulk actions

**Proposed grammar:**

```
selection single             # exactly one item selected at a time (default if `drawer ... on select`)
selection multi              # zero or more items, Set semantics
bulk_actions delete
```

`selection single` is the **implicit default when `drawer ... on select` is declared and no explicit `selection` line exists** — selecting an item opens the drawer, no checkbox UI. `selection multi` opts in to the Set-of-IDs state machine (shift-range, toggle, clear). `bulk_actions` lists commands that operate on the selection; only valid under `selection multi`. Doctor rule `lzx-bulk-actions-require-multi` flags `bulk_actions` paired with `selection single` or no `selection` line.

**IR:**

```rust
pub struct SelectionDecl {
    pub mode: SelectionMode,
    pub bulk_actions: Vec<CommandRef>,
}

pub enum SelectionMode {
    None,
    Single,
    Multi,
}
```

**Emitted (multi):**

```typescript
selection: {
  mode: "multi";
  ids: Set<string>;
  has(id: string): boolean;
  toggle(id: string, shiftKey?: boolean): void;     // see §3.9 for click-vs-toggle dispatch
  selectRange(fromId: string, toId: string): void;
  clear(): void;
  bulk: {
    delete: ReturnType<typeof useLazuliCommand<{ ids: ID[] }, void>>;
  };
}
```

**Emitted (single):**

```typescript
selection: {
  mode: "single";
  id: string | null;
  set(id: string | null): void;
  clear(): void;
}
```

**Canonical bulk-action input shape:** every `bulk_actions <cmd>` declaration assumes the underlying `.lzi` command accepts `{ ids: ID[] }` (or `{ <resource>_ids: ID[] }` for a resource-prefixed variant; doctor accepts either). The hook passes `Array.from(selection.ids)` to the command — single resource only. **Multi-resource bulk operations** (legacy pleiades-os bulkDelete takes `{itemIds, patternIds}` because items and patterns are separate resources) are **out of scope** for v0.2 — author them as a custom non-bulk command and call from user-`.tsx` directly. Tracked in §7 OQ-7.

Doctor rule `lzx-bulk-action-input-shape` flags `bulk_actions <cmd>` where the `.lzi` command's input does not have an `ids: ID[]` or `<feature>_ids: ID[]` field.

`selectRange` works against `query.data` (the rendered order at the moment of the call — captured to avoid race when data refetches mid-range).

### §3.7 Settings — local-persisted view-level state

**Proposed grammar (field declaration on first line matches `.lzi` field syntax; persistence on a separate indented line):**

```
settings
  grid_size: Enum [sm, md, lg] default sm
    persist local
```

The first line of each setting parses identically to a `.lzi` field declaration (`<name>: <type> [constraints] [default <v>]`). Child lines configure storage — `persist local | workspace | none` (defaults to `none` if omitted).

**IR:**

```rust
pub struct SettingDecl {
    pub name: String,
    pub value_space: SettingValueSpace,
    pub default: String,
    pub persistence: SettingPersistence,
}

pub enum SettingValueSpace {
    Enum(Vec<String>),
    Bool,
    Int { min: i64, max: i64 },
}

pub enum SettingPersistence {
    None,        // ephemeral useState
    Local,       // localStorage
    Workspace,   // server-side per workspace (future)
}
```

**Emitted:**

```typescript
settings: {
  gridSize: "sm" | "md" | "lg";
  setGridSize(value: "sm" | "md" | "lg"): void;
}
```

Persistence is wired to localStorage under a key derived from the view name (`pleiades:item-terminal:grid_size`). `workspace` persistence is **deferred** — needs a `view_settings` resource in the runtime; tracked in §7.

### §3.9 Interaction semantics — click dispatch, auto-close, focus

When `selection multi` and `drawer ... on select` coexist, a click on a cell needs an unambiguous default. v0.2 codifies:

| Modifier on click | Action |
|---|---|
| no modifier, `selection.ids.size === 0` | drawer.open(id) |
| no modifier, `selection.ids.size > 0`  | selection.toggle(id) (drawer NOT opened — user is already in selection mode) |
| shift                                  | selection.selectRange(lastSelectedId, id) |
| meta/ctrl                              | selection.toggle(id) (drawer NOT opened) |

The user-`.tsx` calls a single emitted helper, `view.cellClick(id, event)`, which encodes this table. Authors who want a different dispatch override the helper at the call site:

```tsx
<ItemCard onClick={(e) => view.selection.toggle(item.id, e.shiftKey)} />  // pure selection mode
<ItemCard onClick={() => view.drawer.open(item.id)} />                    // pure drawer mode
<ItemCard onClick={(e) => view.cellClick(item.id, e)} />                  // default dispatch
```

**Drawer auto-close rules** (codegen emits these in the hook body):

| Trigger | Behavior |
|---|---|
| **Pathname change** of the host view's route (NOT search-param changes — filter URL sync from §3.3 mutates `?slug=&tags=` without auto-closing the drawer) | drawer.close() |
| `selection.clear()` called | drawer.close() (single mode: implicit; multi mode: only if drawer is showing the cleared id) |
| Source query refetch returns `null` for the open id | drawer.close() |
| Underlying command in `drawer.actions` succeeds and the command is `delete` | drawer.close() |
| Escape key | drawer.close() — emitted as a `useEffect` listener; user can opt out via `drawer.preventAutoCloseOnEscape: true` setter |

**Focus** is **not** owned by the hook. Sheet/Dialog/Drawer components (Radix, Reach, custom) each have their own focus management. Lazuli does not interfere — the hook exposes `drawer.isOpen` and `drawer.close()`; the rest is user-`.tsx` choice.

These rules close the "silent semantics" gap flagged in the v0.1 grade (AI ergonomics dimension). An LLM reading §3.2 + §3.6 + §3.9 should be able to write the user-`.tsx` without inferring behavior from the legacy.

### §3.10 Cross-feature source (no grammar change; convention)

Terminal's data source is "items + patterns merged with slug metadata". Today `source feature.query.name` accepts only one query. Rather than extending the grammar, **the merge moves into the `.lzi` query**:

```lzi
feature item
  query search of list of TerminalItem
    inputs q: Text, slug: Text?, type: ItemType?, ...
    @fn item.search_terminal
```

The Go handler does the join. `.lzx` stays target-portable. No new keyword.

This is a deliberate choice: cross-feature joins are **domain concerns**, not view concerns. They belong in `.lzi`.

---

## §4. Composition example — Pleiades Terminal as `.lzx`

```
surface item web
  uses feature item

  audience admin
    requires @scope.workspace_member

    view list item_terminal at "/"
      source item.query.search
      cells item_card
      actions update, delete

      filters
        type: ItemType
        status: ItemStatus
        confidence: Confidence
        tags: list of Text
        slug: Text from query

      search segmented
        field slug binds filters.slug
        field type binds filters.type
        field tag binds filters.tags
        free text into source.q

      sort
        by title, type, priority, updated
        default updated desc

      selection multi
      bulk_actions delete

      settings
        grid_size: Enum [sm, md, lg] default sm
          persist local

      drawer item_detail on select
        source item.query.by_id
        route id from selection
        sections header, content, metadata
        cells related @client.related_items
        actions update, delete
```

~30 lines of `.lzx` replace ~4300 LOC of `pleiades-os` Terminal. The user-authored `.tsx` consuming `useAdminItemTerminalView()` becomes pure presentation against typed state.

---

## §5. Codegen — emission summary + concrete slice

For the §4 surface, `lazuli generate ts apps/api` emits (in addition to existing §6 of L0 #3):

```
dist/ts-web/item/views/admin/item_terminal.gen.ts
```

containing:

1. `adminItemTerminalView` — compile-time view spec const (route, cell slot, filter decls, search decl, sort decl, selection decl, settings decl, drawer ref).
2. `AdminItemTerminalCells` — slot interface (`ItemCard`, `RelatedItems`).
3. `useAdminItemTerminalView()` — the hook returning `{ query, filters, search, sort, selection, settings, drawer, actions, cellClick, meta }`.

Estimated hook body length: ~140 LOC of generated TypeScript. All wire-through to existing runtime hooks (`useLazuliQuery`, `useLazuliCommand`, `useSearchParams`) + `search-query-parser` (npm dep).

### §5.1 Concrete slice — the §4 hook body (target for Codex cell C.2/C.3/C.4/C.5)

```typescript
// Code generated by lazuli; DO NOT EDIT.
import { useLazuliQuery, useLazuliCommand } from "@lazuli/runtime/react";
import { useSearchParams, useNavigate, useRouterState } from "@tanstack/react-router";
import { parse as parseSearchQuery, type SearchParserResult } from "search-query-parser";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  searchItems, lookupItemById, updateItem, deleteItem, bulkDeleteItems,
  type Item, type ItemType, type ItemStatus, type Confidence,
} from "../../item.gen.js";

const SETTING_KEY_GRID_SIZE = "pleiades:item-terminal:grid_size";
const SEARCH_KEYWORDS = ["slug", "type", "tag"] as const;
const SEARCH_ALWAYS_ARRAY = ["tag"] as const;

export function useAdminItemTerminalView() {
  // Filters (typed) — single + multi cardinality, with URL sync.
  const [params, setParams] = useSearchParams();
  const filters = useFilterState({
    type:       { mode: "single", values: [...ITEM_TYPE_VALUES] as const, urlKey: undefined },
    status:     { mode: "single", values: [...ITEM_STATUS_VALUES] as const, urlKey: undefined },
    confidence: { mode: "single", values: [...CONFIDENCE_VALUES] as const, urlKey: undefined },
    tags:       { mode: "multi",  urlKey: "tags",  params, setParams },
    slug:       { mode: "single", urlKey: "slug",  params, setParams },
  });

  // Sort.
  const [sort, setSort] = useState<{ field: "title" | "type" | "priority" | "updated"; dir: "asc" | "desc" }>({
    field: "updated", dir: "desc",
  });

  // Selection (multi).
  const selection = useMultiSelection<string>(/* lastSelectedId tracked internally */);

  // Settings (local-persisted).
  const settings = useLocalSetting<{ gridSize: "sm" | "md" | "lg" }>(SETTING_KEY_GRID_SIZE, { gridSize: "sm" });

  // Source query — bound to filters + sort + free text.
  const query = useLazuliQuery(searchItems, {
    q: rawFreeText(),
    slug: filters.slug.value,
    type: filters.type.value,
    status: filters.status.value,
    confidence: filters.confidence.value,
    tags: filters.tags.value,
    sort: sort.field,
    dir: sort.dir,
  });

  // Search — dual-field round-trip (BLOCKER-D resolution).
  const [raw, setRaw] = useState("");
  function rawFreeText(): string { return parseSearchQuery(raw, { keywords: SEARCH_KEYWORDS, alwaysArray: SEARCH_ALWAYS_ARRAY }).text ?? ""; }
  const setRawSearch = useCallback((input: string) => {
    setRaw(input);
    const parsed = parseSearchQuery(input, { keywords: SEARCH_KEYWORDS, alwaysArray: SEARCH_ALWAYS_ARRAY }) as SearchParserResult;
    if (typeof parsed !== "string") {
      if (parsed.slug) filters.slug.set(parsed.slug as string);
      if (parsed.type) filters.type.set(parsed.type as ItemType);
      if (parsed.tag)  filters.tags.set(Array.isArray(parsed.tag) ? parsed.tag : [parsed.tag]);
    }
  }, [filters]);
  const derivedFromFilters = useMemo(() => canonicalizeSearch({
    slug: filters.slug.value, type: filters.type.value, tag: filters.tags.value, free: rawFreeText(),
  }), [filters.slug.value, filters.type.value, filters.tags.value, raw]);

  // Drawer — sub-view state machine.
  const drawer = useDrawerSubView({
    sourceQuery: lookupItemById,
    selection,
    actions: { update: updateItem, delete: deleteItem },
    autoCloseOnDelete: true,                                  // §3.9 rule
    autoCloseOnRouteChange: true,                             // §3.9 rule
  });

  // §3.9 unified click dispatch.
  const cellClick = useCallback((id: string, event: React.MouseEvent) => {
    if (event.shiftKey)      { selection.selectRange(/* lastSelectedId */, id); return; }
    if (event.metaKey || event.ctrlKey) { selection.toggle(id); return; }
    if (selection.ids.size > 0) { selection.toggle(id); return; }
    drawer.open(id);
  }, [selection, drawer]);

  // Top-level actions.
  const update = useLazuliCommand(updateItem);
  const delete_ = useLazuliCommand(deleteItem);
  const bulkDelete = useLazuliCommand(bulkDeleteItems);

  return {
    query,
    filters,
    search: { raw, derivedFromFilters, segments: parseSegments(raw), setRaw: setRawSearch, clear: () => setRawSearch("") },
    sort: { field: sort.field, dir: sort.dir, set: (field, dir = "desc") => setSort({ field, dir }) },
    selection: { mode: "multi" as const, ids: selection.ids, has: selection.has, toggle: selection.toggle, selectRange: selection.selectRange, clear: selection.clear, bulk: { delete: bulkDelete } },
    settings,
    drawer,
    actions: { update, delete: delete_ },
    cellClick,
    meta: adminItemTerminalView,
  } as const;
}
```

The `useFilterState`, `useMultiSelection`, `useLocalSetting`, `useDrawerSubView`, `canonicalizeSearch`, `parseSegments` helpers live in `@lazuli/runtime/react` (NEW — each is a thin `useState` + `useEffect` adapter, total ~150 LOC of runtime, wires `useSyncExternalStore` for localStorage and `useSearchParams` for URL sync). Cell C.4b in §6 emits these helpers; the bulk of the code each one needs is React stdlib (`useState`, `useCallback`, `useMemo`, `useSyncExternalStore`).

**Router adapter (per frontend target).** The import line `import { useSearchParams, useNavigate, useRouterState } from "@tanstack/react-router"` in the slice above is target-specific. v0.2 codegen emits a per-target import (same switch already used by §6.2 of L0 #3 for `useParams`):

| target | import |
|---|---|
| `vite-react` / `tauri` | `@tanstack/react-router` (the slice above) |
| `nextjs` | `next/navigation` (uses `useSearchParams`, `useRouter`, `usePathname`) |
| `expo` | `expo-router` (uses `useLocalSearchParams`, `useRouter`, `useSegments`) |

The `useFilterState` helper has three implementations under `@lazuli/runtime/react` — one per target — and the codegen picks at emit time. This is the same router-adapter pattern v1 already uses; v0.2 extends it from `useParams` (one hook) to four hooks. Cell C.4b emits all three implementations together.

`canonicalizeSearch` (§3.4 round-trip) is ~20 LOC of pure string assembly: alphabetical keys, multi-value flattened, free text last. No third-party dep.

`adminItemTerminalView` const carries the literal grammar — `route`, `cells: "@client.item_card"`, `filters: { type: { values: [...] }, ... }`, `sort: { allowed: [...], default: { field: "updated", dir: "desc" } }`, `selection: { mode: "multi", bulk_actions: ["delete"] }`, `settings: { grid_size: { values: [...], default: "sm", persist: "local" } }`, `drawer: { name: "item_detail", trigger: "select", sections: [...], actions: [...] }`. Doctor reads this const at typecheck time for cross-file consistency.

---

## §6. Decomposition into L2 cells

Mechanical, single-file-per-cell where feasible (per `feedback_claude_plans_codex_executes.md`):

| Cell | Crate | Files | LOC est. | Codex-able |
|---|---|---|---|---|
| **A.0** Refactor: convert view-body line dispatcher at `parser.rs:1180-1206` from flat `else-if` ladder to a handler-registry pattern (each view-body keyword registers a parser fn). Prerequisite to running A.1–A.5 in parallel — without A.0 they all edit the same dispatcher block and cherry-pick conflicts. | lazuli_syntax | parser.rs (refactor only, no new behavior) | +80 | Yes |
| **A.1** Parser: `cells @client.<slot>` in `view list` (disambiguate with v1 `cells <field> @client.<slot>` per §3.1). **Also** removes v1's mandatory-`columns` hard error at `parser.rs:1220-1222`; exclusivity check moves to doctor rule D.1. | lazuli_syntax | parser.rs | +60 | Yes |
| **A.2** Parser: `drawer` sub-view block (incl. `route key from selection`) | lazuli_syntax | parser.rs | +120 | Yes |
| **A.3** Parser: `filters` block (typed + url_sync) | lazuli_syntax | parser.rs | +100 | Yes |
| **A.4** Parser: `search segmented` block (incl. `field <k> binds <BindingRef>`) | lazuli_syntax | parser.rs | +90 | Yes |
| **A.5** Parser: `sort` / `selection single \| multi` / `bulk_actions` / `settings` (with `persist` child line) | lazuli_syntax | parser.rs | +120 | Yes |
| **B.1** IR types — `ListRender`, `DrawerSubView`, `FilterDecl`, `SearchDecl`, `SortDecl`, `SelectionDecl`, `SettingDecl`, **`BindingRef`** | lazuli_ir | lib.rs | +220 | Yes |
| **B.2** Lowering — AST → IR for new nodes; resolves `BindingRef` and `FilterDecl.type_ref` against host audience/feature | lazuli_syntax (lowering module) | lowering.rs | +180 | Yes |
| **C.1** Codegen: list-view emitter accepts `cells @client.<slot>` variant | lazuli_codegen_ts | lzx_view_list.rs | +80 | Yes |
| **C.2** Codegen: drawer hook emission (uses `useDrawerSubView` runtime helper; encodes §3.9 auto-close rules + `cellClick` dispatcher) | lazuli_codegen_ts | lzx_view_list.rs | +140 | Yes |
| **C.3** Codegen: filter state emission (`useFilterState`-based, URL sync per §3.3 repeated-key convention) | lazuli_codegen_ts | `lzx_filters.rs` | +180 | Yes |
| **C.4a** Codegen: search-segmented emission (calls `search-query-parser` directly; emits `canonicalizeSearch` helper inline; dual `raw`/`derivedFromFilters` fields per §3.4) | lazuli_codegen_ts | `lzx_search.rs` | +140 | Yes |
| **C.4b** Runtime: `@lazuli/runtime/react` adds `useFilterState`, `useMultiSelection`, `useDrawerSubView`, `useLocalSetting` (thin `useState` + `useSyncExternalStore` adapters; ~150 LOC total, no third-party deps beyond React) | runtime/ts/lazuli | `react/view-helpers.ts` | +150 | Yes |
| **C.5** Codegen: sort / selection (single+multi) / settings emission | lazuli_codegen_ts | `lzx_aux.rs` | +220 | Yes |
| **D.1** Doctor rule `lzx-list-cells-or-columns` (exactly one of `cells`/`columns`) | lazuli_cli | doctor/lzx/cells_or_columns.rs | +60 | Yes |
| **D.1b** Doctor rule `lzx-cells-mixed-form` (per-column `cells <field> @client.<slot>` AND grid `cells @client.<slot>` cannot coexist in one view) | lazuli_cli | doctor/lzx/cells_mixed_form.rs | +60 | Yes |
| **D.2** Doctor rule `lzx-drawer-source-shape` (sub-query input must accept the `from selection` slot's type; output resource must match host's source resource) | lazuli_cli | doctor/lzx/drawer_source.rs | +80 | Yes |
| **D.3** Doctor rule `lzx-filter-type-resolves` (each `FilterDecl.type_ref` resolves to a known enum on the resource or a scalar) | lazuli_cli | doctor/lzx/filter_resolves.rs | +80 | Yes |
| **D.4** Doctor rule `lzx-search-binds-target-exists` (`BindingRef` resolves) + `lzx-search-field-multi-cardinality` (each binding has known cardinality) | lazuli_cli | doctor/lzx/search_binds.rs | +100 | Yes |
| **D.5** Doctor rule `lzx-sort-source-accepts` (source query has `sort` + `dir` inputs) | lazuli_cli | doctor/lzx/sort_source.rs | +60 | Yes |
| **D.6** Doctor rule `lzx-bulk-action-input-shape` (each bulk action's command accepts `{ ids: ID[] }` or `{ <feature>_ids: ID[] }`) + `lzx-bulk-actions-require-multi` | lazuli_cli | doctor/lzx/bulk_actions.rs | +80 | Yes |
| **F.1** `item.web.lzx` authoring (Pleiades) | apps/api | features/item/item.web.lzx | +35 | No (Claude) |
| **F.2** Terminal user .tsx — entry + cell + drawer body (consumer of `useAdminItemTerminalView`) | apps/api | features/item/web/views/admin/{terminal.tsx, item-cell.tsx, item-drawer.tsx} | +600 | No (Claude) |
| **F.3** `searchItems` query + `bulkDeleteItems` command in `item.lzi` (with `q, slug, type, status, confidence, tags, sort, dir` inputs + `{ids: ID[]}` shape) | apps/api | features/item/item.lzi | +40 | No (Claude — domain authoring) |

**Wave estimate:**
- Wave 0 (Refactor prereq, A.0): 1 cell, ~80 LOC, single Codex agent.
- Wave 1 (Parser + IR + Lowering, A.1-A.5 + B.1 + B.2 — parallel **only after A.0 merges**): 7 cells, ~890 LOC, parallel via Codex.
- Wave 2 (Codegen + Runtime, C.1 + C.2 + C.3 + C.4a + C.4b + C.5): 6 cells, ~910 LOC, parallel.
- Wave 3 (Doctor, D.1 + D.1b + D.2 + D.3 + D.4 + D.5 + D.6): 7 cells, ~520 LOC, parallel.
- Wave 4 (Pleiades authoring, F.3 → F.1 → F.2 sequentially): Claude.

**Cherry-pick discipline note:** Per `feedback_review_codex_batches.md`, Codex agents in parallel worktrees must each write one new isolated file or a small additive edit. A.0 makes the view-body dispatcher a registry, so A.1–A.5 each REGISTER a handler in a new file rather than editing a shared `else-if` ladder. Without A.0 the Wave 1 parallel claim is wrong and the wave should run sequentially in one Codex session (cheaper to coordinate, ~slower).

Total: ~2400 LOC framework + 675 LOC Pleiades authoring (~600 of which is presentational `.tsx`, the rest is `.lzx`/`.lzi`). ~3-4 sessions if Codex waves go clean.

---

## §7. Open questions / future work

(Resolved in v0.2: search round-trip via dual `raw` / `derivedFromFilters` hook fields — §3.4. Old OQ-1 is closed.)

1. **`workspace` persistence for settings.** §3.7 declares `persist local | workspace | none` but only `local` is implemented in v0.2 codegen. `workspace` requires a `view_settings` resource shape in the runtime (key by `(workspace_id, user_id, view_id, setting_name)`). Carve out as a separate cell when 2+ products need cross-device sync.

2. **`drawer` as detail-route fallback.** Should `drawer` automatically *also* register a route like `/items/$id` so deep-links work? v0.2 says **no** — that's a separate `view detail` author choice. Sugar like `drawer item_detail on select mirrors view item_full` could be added later; for now an author who wants both writes both.

3. **`view grid` as separate kind vs `view list cells`.** §3.1 picks the latter (Option B). Concrete promotion trigger: if **2+ products** need declarative virtualization (`paginate windowing rows`, `cell height fixed | dynamic`), declarative drag-and-drop reorder (`reorder by priority`), or a fundamentally different data shape (tree of items, not a flat list), open L0 #7 to split `view grid` into its own kind. Until then, virtualization stays a user-`.tsx` choice (TanStack Virtual / react-window) consumed alongside `query.data`.

4. **Selection state ↔ URL.** Should multi-selection serialize to URL (`?selected=a,b,c`) for shareable bulk-action links? Deferred — first product feedback needed.

5. **Pagination.** Today `view list` returns all rows. Terminal in pleiades-os was unpaginated. Once a product exceeds a few hundred items, we need `paginate cursor | offset` declarations. Out of scope for this proposal; carve into L0 #7.

6. **Filter dependencies.** Some filters depend on others (e.g. "show items in slug X" → tag autocomplete should be scoped to items in X). Add `depends on filters.slug` later if 2+ filters need it.

7. **Multi-resource bulk actions.** v0.2 §3.6 codifies single-resource bulk: `bulk_actions delete` assumes the command takes `{ ids: ID[] }` or `{ <feature>_ids: ID[] }`. The legacy `bulkDeleteItems({itemIds, patternIds})` straddles items and patterns and **cannot** be expressed as a single `bulk_actions` declaration on `view item ...`. Two paths if 2+ products surface this:
   - Add `bulk_actions delete with payload <command-input-shape>` to let authors pick a custom shape and pass `{ resource: id[] }` maps. Codegen relaxes the cardinality doctor rule.
   - Promote bulk to its own primitive `bulk delete on selection commits to (items.command.bulk_delete, pattern.command.bulk_delete) by resource_kind` where each cell selection carries a `resourceKind` tag (compile-time discriminated). Heavier — needs IR work on `Item` resource discrimination too.

   Until either ships, multi-resource bulk authors call commands directly from user-`.tsx` over `selection.ids` (which they group by resource kind themselves).

---

## §8. Tests / acceptance

- Parser round-trips a §4 example without loss.
- IR snapshot stable; `cargo test -p lazuli_ir` green.
- Codegen emits §5 file deterministically (offset-stable; alphabetical filter/setting order).
- Doctor rules each fire once on the canonical violation; zero false positives on `examples/full-capsule/`.
- **Hook-bundle integration test** (folded into C.4b): a React Testing Library suite renders a minimal harness that exercises (a) `cellClick` dispatch table — no-modifier/shift/meta variants land correctly, (b) drawer auto-close on `delete` command success, (c) drawer auto-close on pathname change (NOT on search-param change), (d) filter URL sync round-trip via repeated-key serialization, (e) `search.setRaw("type:doc onboarding")` mutates filters and `derivedFromFilters` reflects new state. Lives in `runtime/ts/lazuli/__tests__/view-helpers.test.ts`.
- `item.web.lzx` authoring lifts the §4 example through parse → IR → emit → vite serves the resulting `.tsx` consumer.
- `lazuli-language-architect` PASS at ≥ 9.0/10 with no individual dimension < 7. **v0.2 achieved 9.05/10.**

---

## §9. References

- `docs/proposals/lzx-integration-codegen.md` — L0 #3, current `.lzx` v1 grammar.
- `docs/proposals/lazurite-frontend-folder-canon.md` — L0 #1, file canon for `dist/ts-<target>/…`.
- `pleiades-os/apps/terminal/src/features/terminal/` — legacy Terminal (negative reference; what we are replacing).
- `docs/design-principles.md` — Rule Zero (Vocabulary Over Mechanism), Flutter-2 line.
- `docs/invariants.md` — closed grammar discipline.
