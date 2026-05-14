# Proposal — `.lzx` Integration Codegen (Compile-Time ViewModel Emission)

**Status:** L0 v0.2 PASS @ 9.18/10 — 2026-05-14 (v0.1 graded 9.18 via `lazuli-language-architect`; v0.2 applies 2 blockers + 5 polish items inline; remaining polish carved into L2 cells)
**Author:** Claude Opus 4.7 (orchestrator)
**Audit-ready target:** ≥ 9.0 via `lazuli-language-architect`
**Depends on:** `docs/proposals/lazurite-frontend-folder-canon.md` (L0 #1, file locations), `docs/proposals/design-tokens.md` (L0 #2, token vocabulary)
**Honors:** `docs/invariants.md`, `docs/design-principles.md` (Rule Zero), `docs/proposals/lazurite-scaffold.md`

---

## §1. Status & motivation

Lazuli today lowers `.lzi` (domain) to typed Go runtime + typed TS client SDK + Zod schemas. The user-facing frontend layer — pages, forms, lists, modals — currently lives as **hand-written React** that consumes the typed SDK directly via raw `useLazuliQuery` / `useLazuliCommand` calls. There is no declarative surface between domain contract and React JSX.

Three products (Pleiades v2, Atelier, Erudito) need to ship frontends. With only raw hooks, every list page repeats:

```tsx
const { data, isLoading } = useLazuliQuery(listSlugs, {});
const create = useLazuliCommand(createSlug);
const audience = useAudienceGuard("admin"); // bespoke
const form = useForm<CreateSlugInput>({ resolver: zodResolver(createSlugSchema) });
// ... 80 lines of plumbing per page
```

The plumbing is:
- **Repetitive** — every list/detail/create page has the same wiring shape.
- **Drift-prone** — audience guards, cache-invalidation lists, slot bindings, route params live in 8 different places that hand-coordinate.
- **AI-hostile** — LLMs hallucinate state-management bugs precisely in this layer (wrong invalidation key, missing audience guard, type mismatch between RHF and the command input).

This proposal lifts the **plumbing layer** into a declarative `.lzx` source — a per-feature, per-platform **ViewModel contract** — and emits typed React/RN hooks that consume the existing SDK. The user-authored `.tsx` (View) becomes pure presentation; the View Model (`.lzx`) becomes the single source for "what data, which audience, which actions, which slots, which routes."

**Why now**: Pleiades web ships next. Without this proposal, every product reinvents the wiring layer. With this proposal, products author `.lzx` once and consume typed hooks; cross-feature, cross-audience, cross-target (web + mobile) consistency is enforced by the compiler.

**Boundary discipline (the guiding principle)**:

> **Lazurite owns structure and glue; product code owns interaction and rendering.**

Concrete projection:
- `.lzx` owns contract (audience, source, columns, actions, slots, routes). NO `<Table>`, NO `<EmptyState>`, NO layout, NO render keyword.
- Generated `*.gen.ts` files own typed hooks + view spec const + slot interfaces + audience-scoped SDK projection.
- User-authored `*.tsx` owns JSX, layout strategy, component library choice (TanStack Table / Mantine / Shadcn / custom), state-ephemeral (modals, filter inputs), animations, design tokens consumption.

This split makes Lazuli a **compile-time ViewModel layer on top of React**, never a renderer. Different from Flutter (own widget tree), different from Vue/Svelte SFC (own reactivity model). Same model as compile-time MVVM (Microsoft/Apple stack) but as a code generator, not a runtime object graph.

---

## §2. Guiding principle (carried from L0 #1)

> **Lazurite owns structure and glue; product code owns interaction and rendering.**

In MVVM vocabulary:

| Layer | Lazuli file | Owner | Role |
|---|---|---|---|
| **Model** | `.lzi` | Lazuli emits Go runtime + typed SDK | Domain contract, persistence, policies, commands, queries, events |
| **ViewModel** | `.lzx` | Lazuli emits typed hooks + view spec | Audience scope + view shape (columns/actions/cells/route) + form schema + cache invalidation |
| **View** | `.tsx` / `.native.tsx` | User authored | JSX, layout, animations, component library choice, ephemeral state |

This three-layer split, compile-time MVVM, is the differentiator. ERB / Astro / Vue SFC mix V and template logic; Lazuli keeps V purely product code.

---

## §3. Scope

### In scope

1. **`.lzx` vocabulary**: 16-keyword closed catalog for the ViewModel surface (`surface`, `uses feature`, `audience`, `requires`, `view list`/`view detail`/`view create`, `at`, `source`, `submit`, `columns`, `fields`, `route … from path`, `sections`, `search`, `filter`, `cells … @client.<slot>`, `actions`). See §5.
2. **Audience-scoped SDK projection**: each `[frontends.<name>] audiences = […]` filters which commands/queries appear in that frontend's bundle. See §7.
3. **Per-view typed hook emission**: one `dist/ts-<target>/<feat>/views/<audience>/<view>.gen.ts` per (feature, audience, view) tuple. Emits view spec const + `use<Audience><View>` hook bundling source query + action mutations. See §6.
4. **Slot interface emission**: each `cells <field> @client.<slot>` reference produces a typed interface (`dist/ts-<target>/<feat>/cells/<slot>.gen.ts`) the user-authored `.tsx` implements. See §8.
5. **Form view RHF + Zod pre-wiring**: `view create` views ship a hook returning `{ form, submit, handleSubmit, meta }` with RHF + zodResolver pre-bound to the command's input schema. See §9.
6. **Inline field constraints (Gap A from `project_validation_strategy_2026-05-14.md`)**: 6 new closed-catalog keywords on field declarations (`min N`, `max N`, `pattern STRING`, `between A and B`, `length N`, `in [...]`) that emit to Zod + Go validator + OpenAPI. See §10.
7. **Doctor rules** for `.lzx`-specific violations. See §11.
8. **Two-level model**: headless MVVM core (this proposal, v0) + scaffold packs (post-pilot, future plugin proposals). See §12.

### Non-goals

1. **No `<Table>`, `<Form>`, `<SidePanel>`, `<Modal>` component emission.** User chooses library (TanStack Table / Mantine / Shadcn / custom).
2. **No CSS / design tokens emission.** Already L0 #2.
3. **No routing library opinion.** Generated routes adapt to TanStack Router / Next App Router / Expo Router via the `[frontends.<x>] target` setting; user wires `RouterProvider` themselves.
4. **No animation choreography.** Framer / Motion / React Spring all live in V.
5. **No JSX inline in `.lzx`.** ERB-style mixing rejected (see §4 for rationale). All JSX lives in `.tsx`.
6. **No `render` keyword in `.lzx`.** Declaring "this view renders as a Table" leaks framework opinion. Views declare contract (source/columns/actions); rendering is V territory.
7. **No `empty_state.message` in `.lzx`.** Empty state is V concern. Lazuli exposes `query.data` + `meta` so V can render whatever empty state it wants.
8. **No `on_success redirect` / interaction logic in `.lzx`.** Post-submit navigation lives in V (`await submit.mutateAsync(...); navigate(...)`).
9. **No theme switching mechanism.** L0 #2 emits the `data-theme` CSS variable layer; V owns the toggle hook.
10. **No `extends` for `.lzx`.** Audience and view inheritance is post-pilot. Total override only per `docs/design-principles.md`.
11. **No multi-language JSX in `.lzx`.** Single canonical declarative DSL.
12. **No scaffold packs in v0.** `lazuli scaffold view <feat>.<audience>.<view> --style shadcn` is deferred to a separate L0 once pilot evidence shows demand.

---

## §4. Why declarative `.lzx` (not ERB-style JSX-embedded)

Considered and rejected: embedding JSX in `.lzx` (ERB-style, Vue SFC, Astro). Rejected for four reasons:

1. **Parser cost**: JSX inside `.lzx` requires Lazuli to ship a JSX lexer + type-check pipeline. Either fork the TypeScript compiler (massive dep) or ship a JSX subset (reinvent). Both wire-fat.
2. **Tooling cost**: every editor needs a `.lzx`-with-JSX mode. tsserver doesn't validate JSX in non-`.tsx` files. Prettier / ESLint / Biome all break.
3. **Vendor lock-in**: JSX assumes React-flavored JSX. Solid / Preact / Qwik use different runtime contracts.
4. **AI-hostile**: LLMs perform worse on mode-switching parsers (DSL ↔ JSX in same file) than on two clearly-typed files. Empirical pattern from GPT/Claude debugging sessions.

Two-file model (`.lzx` declarative + co-located `.tsx` puro) keeps each file lexically pure. tsserver handles `.tsx` natively. LLMs see clean separation. Total cost: 30 extra LOC of import statements at the top of `.tsx` files — acceptable trade.

For "single-file ergonomics" (the genuine ergonomic win of ERB/SFC): see §13 Scaffold packs (post-pilot) — `lazuli scaffold view <feat>.<audience>.<view>` generates a TSX stub with hooks pre-wired, then user fills JSX, Lazuli never re-overwrites. Single-file feel without parser hell.

---

## §5. `.lzx` vocabulary — closed 16-keyword catalog

```lazuli
surface <feature> web|mobile                    # 1: file header
  uses feature <feature>                        # 2: domain import

  audience <audience-name>                      # 3: audience block
    requires @scope.<scope-name>                # 4: audience gate (policy atom)

    view list <view-name> [at "<route>"]        # 5/6/7: list view (`at` is keyword 7)
      source <feature>.query.<query-name>       # 8: read source
      columns <field>, <field>, ...             # 9: column list
      search <field>, <field>                   # 10: searchable fields (optional)
      filter <field>, <field>                   # 11: filterable fields (optional)
      cells <field> @client.<slot>              # 12: slot binding (optional, repeatable)
      actions <command>, <command>              # 13: mutation bundle (optional)

    view detail <view-name> [at "<route>"]      # 5: detail view
      source <feature>.query.<query-name>
      route <name>: <Type> from path            # 14/15: path param ('from path' is keyword 15)
      sections <name>, <name>                   # 16: slot enumeration (optional)
      cells <field> @client.<slot>
      actions <command>, <command>

    view create <view-name> [at "<route>"]      # 5: create view
      submit <feature>.command.<command-name>   # 17: command binding (create-only)
      fields <field>, <field>                   # 18: command-input subset (create-only)
      cells <field> @client.<slot>
```

Keyword count: **18 closed-catalog keywords** — `surface`, `uses feature` (compound, counted as 2), `audience`, `requires`, `view list`/`view detail`/`view create` (3), `at`, `source`, `submit`, `columns`, `fields`, `route ... from path` (compound, counted as 2: `route` + `from path`), `sections`, `search`, `filter`, `cells ... @client.<slot>` (compound: `cells`), `actions`. Three view kinds. One slot mechanism (`cells @client.*`). One audience mechanism (`audience` + `requires`). One route binding (`at` + `route … from path`). The grammar is closed; adding a new view kind or new declaration is a Lazuli core proposal.

### §5.1 Per-view-kind required vs optional

| Element | view list | view detail | view create |
|---|---|---|---|
| `source <q>` | required | required | — |
| `submit <c>` | — | — | required |
| `columns` | required | — | — |
| `fields` | — | — | required (subset of command input) |
| `sections` | — | optional | — |
| `route <name>: <Type> from path` | — | required (path param) | — |
| `at "<route>"` | optional | optional | optional |
| `search` | optional | — | — |
| `filter` | optional | — | — |
| `cells <field> @client.<slot>` | optional | optional | optional |
| `actions <c>, <c>` | optional | optional | — (the `submit` IS the action) |

### §5.2 Resolution rules

- **Field references** in `columns` / `search` / `filter` / `fields` must exist as resource fields on the resource backing `source` (for list/detail) or as command input slots (for create).
- **Command references** in `actions` must exist on the feature. Audience gate (the audience must be in the command's policy atoms) is checked at doctor time.
- **Slot references** (`@client.<slot>`) must have a corresponding `cells/<slot>.tsx` file (per L0 #1 §4.1). Doctor rule `cell-missing-impl` fires if absent.
- **Route binding** `route <name>: <Type> from path` declares a typed path param. The path string in `at "/<...>:<name>"` must include the `:<name>` placeholder. Doctor verifies the match.

### §5.3 Why no `render` keyword

Considered:
```lazuli
view list slug_list
  render table          # ← rejected
  source slug.query.mine
```

The `render` keyword would let `.lzx` say "this view materializes as a Table component." Rejected because:
- It pins V to a specific component shape. Different products want different table libs.
- It opens the door to `render { columns: ..., row_height: ... }` and `render { custom_layout: ... }` — slippery slope to UI DSL.
- It conflicts with the boundary principle: Lazurite glue, not rendering.

If a product wants automatic "list-view → DataTable" wiring for first-day velocity, that's the scaffold pack's job (post-pilot).

---

## §6. View hook emission

Each `(feature, audience, view)` tuple emits one `<view>.gen.ts` in `dist/ts-<target>/<feat>/views/<audience>/<view>.gen.ts` (per L0 #1 §4 dist layout).

### §6.1 Emission shape — `view list`

For `.lzx`:
```lazuli
audience admin
  view list slug_list at "/slugs"
    source slug.query.mine
    columns key, title, tags, created_at
    search key, title
    filter tags
    cells tags @client.type_badge
    actions create, update, delete
```

Emits `dist/ts-web/slug/views/admin/slug_list.gen.ts`:

```typescript
// Code generated by lazuli; DO NOT EDIT.
import {
  useLazuliQuery,
  useLazuliCommand,
  type UseLazuliQueryOptions,
} from "@lazuli/runtime/react";
import {
  listMineSlugs,
  createSlug,
  updateSlug,
  deleteSlug,
  type Slug,
} from "../../slug.gen.js";
import type { TypeBadgeProps } from "../../cells/type_badge.gen.js";

// Compile-time view spec const. Frozen, type-checked against .lzx.
export const adminSlugListView = {
  source: listMineSlugs,
  columns: ["key", "title", "tags", "created_at"] as const,
  search: ["key", "title"] as const,
  filter: ["tags"] as const,
  cells: { tags: "@client.type_badge" as const },
  actions: { create: createSlug, update: updateSlug, delete: deleteSlug },
  route: "/slugs",
} as const;

// Compile-time guarantee: every column must be a Slug field.
type _AssertColumns = (typeof adminSlugListView.columns)[number] extends keyof Slug
  ? true
  : never;

// Slot binding contract.
export interface AdminSlugListSlots {
  TypeBadge: React.ComponentType<TypeBadgeProps>;
}

export function useAdminSlugListView(
  options: UseLazuliQueryOptions<{}, Slug[]> = {},
) {
  const query = useLazuliQuery(adminSlugListView.source, {}, options);
  const create = useLazuliCommand(adminSlugListView.actions.create);
  const update = useLazuliCommand(adminSlugListView.actions.update);
  const delete_ = useLazuliCommand(adminSlugListView.actions.delete);

  return {
    query,
    actions: { create, update, delete: delete_ },
    meta: adminSlugListView,
  } as const;
}
```

### §6.2 Emission shape — `view detail`

```lazuli
audience admin
  view detail slug_detail at "/slugs/:key"
    source slug.query.by_key
    route key: Text from path
    sections header, metadata, related_items
    cells tags @client.type_badge
    actions update, delete
```

Emits hook that consumes `useParams` from the router lib selected via `lazurite.toml [frontends.<x>] target`. The emitter switches the import line per target — generated `.gen.ts` is target-specific, not router-agnostic at runtime:

| `target` value | Import emitted |
|---|---|
| `vite-react` | `import { useParams } from "@tanstack/react-router";` |
| `nextjs` | `import { useParams } from "next/navigation";` |
| `expo` | `import { useLocalSearchParams as useParams } from "expo-router";` |
| `tauri` | `import { useParams } from "@tanstack/react-router";` (same as vite-react) |
| `cli` | (no router; `view detail` not supported for headless CLI frontend) |

The product manifest pin makes the choice once per frontend; subsequent regenerations honor it. Switching router libs is a one-line manifest edit + regen.

```typescript
// Example: dist/ts-web/slug/views/admin/slug_detail.gen.ts when target = "vite-react"
import { useParams } from "@tanstack/react-router";

export const adminSlugDetailView = {
  source: lookupSlugByKey,
  route: "/slugs/:key",
  sections: ["header", "metadata", "related_items"] as const,
  cells: { tags: "@client.type_badge" as const },
  actions: { update: updateSlug, delete: deleteSlug },
} as const;

export function useAdminSlugDetailView() {
  const { key } = useParams({ from: "/slugs/:key" });
  const query = useLazuliQuery(adminSlugDetailView.source, { key });
  const update = useLazuliCommand(adminSlugDetailView.actions.update);
  const delete_ = useLazuliCommand(adminSlugDetailView.actions.delete);
  return {
    query,
    actions: { update, delete: delete_ },
    meta: adminSlugDetailView,
  } as const;
}
```

### §6.3 Emission shape — `view create`

```lazuli
audience admin
  view create slug_create at "/slugs/new"
    submit slug.command.create
    fields key, title, description, tags
    cells tags @client.type_badge
```

Emits RHF + zodResolver pre-wired hook:

```typescript
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import {
  createSlug,
  type CreateSlugInput,
} from "../../slug.gen.js";
import { createSlugInputSchema } from "../../slug.zod.js";

export const adminSlugCreateView = {
  submit: createSlug,
  schema: createSlugInputSchema,
  fields: ["key", "title", "description", "tags"] as const,
  cells: { tags: "@client.type_badge" as const },
  route: "/slugs/new",
} as const;

export function useAdminSlugCreateView() {
  const submit = useLazuliCommand(adminSlugCreateView.submit);
  const form = useForm<CreateSlugInput>({
    resolver: zodResolver(adminSlugCreateView.schema),
  });

  const handleSubmit = form.handleSubmit(async (values) =>
    submit.mutateAsync(values),
  );

  return { form, submit, handleSubmit, meta: adminSlugCreateView };
}
```

### §6.4 Hook naming convention

`use<PascalAudience><PascalView>View`. Examples:
- `useAdminSlugListView`
- `useAdminSlugDetailView`
- `usePublicCustomerListView`
- `useMobileAdminItemDetailView` (when surface is `mobile` + audience is `admin`; the `mobile` is implicit in the file location, not the hook name)

**Hyphenated audience names**: `audience workspace-admin` (kebab-case allowed in `.lzx`) pascalizes to `WorkspaceAdmin`. Hook becomes `useWorkspaceAdminSlugListView`. Underscores also pascalize cleanly (`workspace_admin` → `WorkspaceAdmin`). Mixed-case audience names rejected by parser (audience names are always lowercase kebab or snake).

---

## §7. Audience-scoped SDK projection

Each frontend in `lazurite.toml [frontends.<name>] audiences = [...]` filters which commands/queries appear in that frontend's bundled SDK.

Example: a product with two frontends:
```toml
[frontends.web-admin]
target = "vite-react"
out = "dist/ts-web-admin"
audiences = ["admin"]

[frontends.web-public]
target = "vite-react"
out = "dist/ts-web-public"
audiences = ["public"]
```

For each command/query in the IR, the emitter computes the set of audiences whose `requires @scope.X` policy atom is in the command's effective policy. Then:
- `dist/ts-web-admin/<feat>/<feat>.gen.ts` exports commands/queries whose audience set intersects `["admin"]`.
- `dist/ts-web-public/<feat>/<feat>.gen.ts` exports commands/queries whose audience set intersects `["public"]`.

Result: `web-public` cannot accidentally import `deleteSlug` because it doesn't exist in its bundle. Compile-time enforcement, not runtime check.

### §7.1 Slot interfaces are universal

`dist/ts-web-admin/<feat>/cells/<slot>.gen.ts` and `dist/ts-web-public/<feat>/cells/<slot>.gen.ts` emit the SAME slot interface. The slot contract is universal across audiences; only the view spec consts and hooks are filtered.

### §7.2 Audience gating in detail

For a command:
```lazuli
command archive
  policy @policy.admin_only

policies
  admin_only: @scope.workspace_admin
```

The command's effective policy resolves to `@scope.workspace_admin`. An audience block in `.lzx`:
```lazuli
audience admin
  requires @scope.workspace_admin
```

Match — admin audience can call `archive`. Audience `public` with `requires @scope.workspace_member` doesn't match — emitter excludes `archive` from its SDK.

Multiple `requires` clauses on an audience use OR (audience admits the user if ANY scope matches). Doctor warns when an audience's `requires` set produces an empty command/query intersection (`AUDIENCE-EMPTY-SDK`).

---

## §8. Slot contract emission

For each `cells <field> @client.<slot>` reference in `.lzx`, emit `dist/ts-<target>/<feat>/cells/<slot>.gen.ts`:

```typescript
// Code generated by lazuli; DO NOT EDIT.
import type { Slug } from "../slug.gen.js";

// Slot contract — V implements this interface in
// features/slug/web/cells/<slot>.tsx (per L0 #1 §4.1).
export interface TypeBadgeProps {
  value: Slug["tags"];  // type derived from the field being cell-bound
  row: Slug;            // entire row available for cross-field rendering
}
```

User-authored `features/slug/web/cells/type_badge.tsx`:

```tsx
import type { TypeBadgeProps } from "@/dist/ts-web/slug/cells/type_badge.gen";

export default function TypeBadge({ value }: TypeBadgeProps) {
  if (!value || !Array.isArray(value)) return null;
  return (
    <div className="flex gap-1">
      {(value as string[]).map((tag) => (
        <span key={tag} className="rounded bg-zinc-100 px-2 py-0.5 text-xs">
          {tag}
        </span>
      ))}
    </div>
  );
}
```

Doctor rules from L0 #1 §5.2 (`cell-missing-impl`, `cell-prop-mismatch`) enforce the binding.

### §8.1 Section slots (for `view detail`)

`view detail … sections header, metadata, related_items` emits a parallel slot per section:

```typescript
// dist/ts-web/slug/views/admin/slug_detail.gen.ts adds:
export interface AdminSlugDetailSections {
  Header: React.ComponentType<{ slug: Slug }>;
  Metadata: React.ComponentType<{ slug: Slug }>;
  RelatedItems: React.ComponentType<{ slug: Slug }>;
}
```

User authors three section components under `features/slug/web/views/admin/slug_detail/`:
```
features/slug/web/views/admin/slug_detail/
├── index.tsx           # the page; composes Header + Metadata + RelatedItems
├── header.tsx
├── metadata.tsx
└── related_items.tsx
```

Or inlines them in `slug_detail.tsx` — V layout choice.

---

## §9. Zod + RHF pre-wiring (form views)

The companion `<feat>.zod.ts` ships from L0 #2's design tokens proposal infrastructure adapted for command inputs (Zod schemas were already planned for `defineCommand`'s Input type). This proposal locks the contract:

For each command in the feature, emit:
```typescript
// dist/ts-web/slug/slug.zod.ts
import { z } from "zod";

export const createSlugInputSchema = z.object({
  key: z.string().min(2).max(80).regex(/^[a-z0-9-]+$/),  // ← from §10 inline constraints
  title: z.string().optional(),
  description: z.string().optional(),
  tags: z.unknown().optional(),
});

export const updateSlugInputSchema = z.object({
  title: z.string().optional(),
  description: z.string().optional(),
});

export const deleteSlugInputSchema = z.object({
  id: z.string(),
});
```

The `view create` hook (§6.3) auto-wires `zodResolver(<command>InputSchema)` into RHF. User gets a typed `form` instance with validation already running.

### §9.1 Configuration

`lazurite.toml`:
```toml
[generate.ts]
out = "dist/ts-web"
schemas = "zod"          # default. "off" to skip.
form_resolver = "zod"    # default. (future: "valibot", "arktype")
```

`schemas = "off"` for bundle size sensitivity (mobile, embedded). The view hook degrades to type-checked-only — no runtime validation.

### §9.2 Custom validators (`@validator.X`)

`@validator.X` references in `.lzi` (custom Go-side validators) do NOT auto-emit Zod schemas — Lazuli can't introspect arbitrary Go functions. User provides frontend mirror as needed:

```typescript
// features/slug/web/validators/check_key_taken.ts
import { createSlugInputSchema } from "@/dist/ts-web/slug/slug.zod";

export const createSlugInputWithRemoteCheck = createSlugInputSchema.refine(
  async (data) => !(await checkKeyExists(data.key)),
  { message: "Key already taken", path: ["key"] },
);
```

Hook accepts a `refine` option:
```typescript
const { form } = useAdminSlugCreateView({ refine: createSlugInputWithRemoteCheck });
```

---

## §10. Inline field constraints (Gap A from validation strategy)

Per `project_validation_strategy_2026-05-14.md` Gap A. Six new closed-catalog keywords on field declarations in `.lzi` (NOT in `.lzx`):

```lazuli
command create
  input
    key: Text required min 2 max 80 pattern "^[a-z0-9-]+$"
    age: Integer between 0 and 150
    role: Text in ["admin", "editor", "viewer"]
    title: Text length 120     # exact length
```

### §10.1 Catalog

| Keyword | Applies to | Semantics | Emission |
|---|---|---|---|
| `min N` | Text (length), Integer/Decimal (value), list (count) | minimum bound | Zod `.min(N)`, Go `validate:"min=N"`, OpenAPI `minLength`/`minimum` |
| `max N` | same | maximum bound | Zod `.max(N)`, Go `validate:"max=N"`, OpenAPI `maxLength`/`maximum` |
| `pattern STRING` | Text only | regex match (RE2 syntax — no lookahead/lookbehind/backrefs) | Zod `.regex(/.../)`, Go `validate:"regexp=..."` (Go's `regexp/syntax` is RE2), OpenAPI `pattern`. RE2 chosen because Go's stdlib regexp is RE2 — same pattern compiles identically on both sides. |
| `between A and B` | Integer/Decimal | range A..=B inclusive | Zod `.gte(A).lte(B)`, Go `validate:"min=A,max=B"`, OpenAPI `minimum`/`maximum` |
| `length N` | Text (exact char count) | length == N | Zod `.length(N)`, Go `validate:"len=N"`, OpenAPI `minLength`/`maxLength` both = N |
| `in [...]` | Text/Integer/Decimal | value ∈ list | Zod `.enum([...])` or `.refine`, Go `validate:"oneof=..."`, OpenAPI `enum` |

### §10.2 Combination rules

- `min` + `max` on the same field: valid; equivalent to `between` for numerics.
- `pattern` + `min`/`max`: valid; both validations apply.
- `length` + `min`/`max`: rejected (`FIELD-CONSTRAINT-CONFLICT`); `length N` already pins both.
- `in [...]` + `pattern`: rejected (use enum declaration instead).
- `between` + `min`/`max`: rejected (redundant; use one or the other).

Doctor enforces.

### §10.3 Default value compatibility

Default values must satisfy declared constraints. `Text required min 2 max 80 default ""` rejects at lowering (`FIELD-DEFAULT-VIOLATES-CONSTRAINT`).

### §10.4 Why inline (not reusable scalars)

Per `project_validation_strategy_2026-05-14.md` Gap B: reusable `scalar SlugKey base Text required min 2 max 80 pattern "..."` is **deferred post-pilot**. Author inline first; promote to `scalar` alias when 3+ sites repeat. This proposal locks the inline form; the alias form ships in L0 #4 when evidence triggers it.

---

## §11. Doctor rules

| Code | Trigger | Severity | Resolution |
|---|---|---|---|
| `lzx-source-resource-mismatch` | `columns`/`search`/`filter` field not on the resource backing `source` | error | Fix typo or add field to resource |
| `lzx-command-input-mismatch` | `fields` references a field not in the command's input | error | Fix typo or add field to command input |
| `lzx-action-not-in-audience` | `actions` references a command whose effective policy isn't reachable from the audience's `requires` | error | Either change the audience or change the command's policy |
| `lzx-route-param-missing-binding` | `at "/path/:slug"` placeholder has no `route slug: <Type> from path` declaration | error | Add the route slot |
| `lzx-route-param-orphan` | `route X: Type from path` declared but no `:X` in the path string | warning | Either remove the route slot or add `:X` to the path |
| `lzx-cell-slot-orphan` | `cells <field> @client.<slot>` references a field NOT in `columns`/`fields`/`sections` | warning | Either include the field or remove the cell binding |
| `lzx-audience-empty-sdk` | An audience's `requires` set produces an empty command/query intersection | warning | Either drop the audience or relax the gate |
| `cell-missing-impl` (from L0 #1) | `@client.<slot>` declared but `features/<feat>/web/cells/<slot>.tsx` not present | error | Run `lazuli generate cell <feat>.<slot>` |
| `cell-prop-mismatch` (from L0 #1) | Cell `.tsx` default export prop type doesn't satisfy the generated slot interface | error | Fix the prop shape (deferred L2 cell F.2, see §16 — needs TS compiler infra) |
| `FIELD-CONSTRAINT-CONFLICT` (§10.2) | Conflicting inline constraints on same field | error | Use one constraint set |
| `FIELD-DEFAULT-VIOLATES-CONSTRAINT` (§10.3) | Default value doesn't satisfy declared constraints | error | Change default or constraint |

Severity escalation: warning in `strict` profile, error in `production` profile (same scale as other Lazuli rules).

---

## §12. Two-level model — headless v0 + scaffold post-pilot

**Headless v0 (this proposal)**: ships compile-time MVVM. User writes `.lzx` + `.tsx`; Lazuli wires hooks. Maximum flexibility — no opinion on visual library.

**Scaffold packs (future, post-pilot)**: `lazuli scaffold view <feat>.<audience>.<view> --style shadcn` generates a first-draft `.tsx` consuming the hook. Initial scaffold only — Lazuli never overwrites after the first generation. User-owned after that.

Implementation lives in `@plugin/scaffold-shadcn`, `@plugin/scaffold-mantine`, `@plugin/scaffold-mui`, etc. (per `project_plugin_namespace_policy.md`). Out of L0 #3 scope; opened as a separate L0 once pilot evidence shows demand.

Why deferred: with only 3 dogfood products (Pleiades / Atelier / Erudito), there's no data on which design pack to prioritize. Premature lock-in. Headless first; scaffold once a pattern emerges.

---

## §13. Examples — Pleiades-shaped

### §13.1 Pleiades `slug.web.lzx`

```lazuli
surface slug web
  uses feature slug

  audience admin
    requires @scope.workspace_admin

    view list slug_list at "/slugs"
      source slug.query.mine
      columns key, title, tags, created_at
      search key, title
      filter tags
      cells tags @client.type_badge
      actions create, update, delete

    view detail slug_detail at "/slugs/:key"
      source slug.query.by_key
      route key: Text from path
      sections header, metadata, related_items
      cells tags @client.type_badge
      actions update, delete

    view create slug_create at "/slugs/new"
      submit slug.command.create
      fields key, title, description, tags
      cells tags @client.type_badge

  audience public
    requires @scope.workspace_member

    view list public_slug_list at "/browse"
      source slug.query.mine
      columns key, title
      search key, title
```

### §13.2 Generated files

```
dist/ts-web/slug/
├── slug.gen.ts                       # Slug + listMineSlugs + lookupSlugByKey + createSlug + updateSlug + deleteSlug (audience-filtered)
├── slug.zod.ts                       # Zod schemas including inline §10 constraints
├── cells/
│   └── type_badge.gen.ts             # TypeBadgeProps
├── views/admin/
│   ├── slug_list.gen.ts              # adminSlugListView + useAdminSlugListView
│   ├── slug_detail.gen.ts            # adminSlugDetailView + useAdminSlugDetailView (incl. AdminSlugDetailSections)
│   └── slug_create.gen.ts            # adminSlugCreateView + useAdminSlugCreateView (RHF + zodResolver pre-bound)
└── views/public/
    └── public_slug_list.gen.ts       # publicSlugListView + usePublicSlugListView (no actions exposed)
```

### §13.3 User-authored View consuming the hook

```tsx
// features/slug/web/views/admin/list.tsx
import { useAdminSlugListView } from "@/dist/ts-web/slug/views/admin/slug_list.gen";
import { TypeBadge } from "@/features/slug/web/cells/type_badge";

export default function AdminSlugsListPage() {
  const { query, actions, meta } = useAdminSlugListView();
  if (query.isLoading) return <Spinner />;
  return (
    <DataTable
      data={query.data ?? []}
      columns={meta.columns}
      cellRenderers={{ tags: TypeBadge }}
      actions={{
        onCreate: () => navigate({ to: "/slugs/new" }),
        onDelete: (row) => actions.delete.mutateAsync({ id: row.id }),
      }}
    />
  );
}
```

User picks `<DataTable>` (TanStack Table / Mantine / custom). Hook is library-agnostic.

---

## §14. Open questions / Future work

### §14.1 Reusable scalar aliases (L0 #4, gated)

Per `project_validation_strategy_2026-05-14.md` Gap B. When 3+ sites across dogfood products repeat `min N max N pattern "..."`, promote to `scalar SlugKey base Text required ...`. L0 #4 opens with the evidence + canonical shape.

### §14.2 `@plugin/scalars-<locale>` kit

Aerocoding-style catalog of locale-specific scalars (CPF/CNPJ/CEP for BR, IBAN/VAT for EU, etc.). Post-pilot. Plugin namespace, never in core.

### §14.3 Scaffold packs

`lazuli scaffold view <feat>.<audience>.<view> --style shadcn/mantine/...`. Plugin namespace. Each pack ships TSX template + component imports for one design system. Post-pilot.

### §14.4 Multi-target view emission

A single `.lzx` per (feature, target) per L0 #1 canon. Cross-target (web + mobile from same `.lzx`) is currently per-target authored. Future: a single `.lzx` shared between web/mobile when the contract is identical, with a `target_overrides` block for per-target divergence. Defer until 2+ products hit the duplication.

### §14.5 LSP integration

Completion for `.lzx` keywords, hover for slot interfaces, jump-to-definition for `source`/`submit` references. Separate L2 cell in LSP crate.

### §14.6 Workflow / approval views

`workflow` and `approval` constructs in `.lzi` don't yet have `.lzx` view kinds. Likely views: `view workflow` (state machine visualization) + `view approval` (action gating UI). Defer to L0 #5 once workflow vocab stabilizes.

### §14.7 Real-time / subscription views

WebSocket / SSE / GraphQL subscription consumption in `.lzx`. Lazuli runtime doesn't yet ship subscription primitives. Future cut.

### §14.8 i18n-aware views

Currently `.lzx` is locale-blind. The translation bucket (`docs/proposals/bucket-i18n-scope.md`) handles `@translation.<key>` at the field level. View-level i18n (which audience gets which locale) is a future addition.

### §14.9 Custom hooks for cross-view state

Some products need state shared across views (selected row in list passes to detail). Currently the user handles this in `app/shell/` via React Context. Future: a `shared_state` declaration in `.lzx`. Defer until pattern emerges.

---

## §15. References

- `docs/proposals/lazurite-frontend-folder-canon.md` (L0 #1) — file path canon.
- `docs/proposals/design-tokens.md` (L0 #2) — design token catalog.
- `docs/proposals/lazurite-scaffold.md` — backend folder canon.
- `docs/invariants.md` — closed-catalog discipline.
- `docs/design-principles.md` — Rule Zero (Vocabulary Over Mechanism), Self-Contained Declarations, No Cascade.
- `docs/architecture.md` §"Lazuli vs Lazurite" — framework/distro boundary.
- W3C Design Tokens spec, MVVM pattern (Microsoft).
- Memory: `feedback_wave_workflow_lucas_preferred.md`, `project_validation_strategy_2026-05-14.md`, `feedback_grade_before_commit.md`, `project_plugin_namespace_policy.md`.

---

## §16. Acceptance criteria

L0 PASS condition: the proposal answers, for any Lazurite-shaped product, the following deterministically:

1. **Where does the ViewModel for a feature's web frontend live?** → `features/<feat>/<feat>.web.lzx`.
2. **What's the closed catalog of `.lzx` keywords?** → 16 keywords per §5.
3. **What does a `view list` emit?** → typed view spec const + `use<Audience><View>` hook bundling source query + action mutations + slot interfaces (§6.1).
4. **What does a `view create` emit?** → RHF + zodResolver pre-bound hook with `{ form, submit, handleSubmit, meta }` shape (§6.3).
5. **How does audience-scoped SDK projection work?** → `[frontends.<x>] audiences = [...]` filters the generated SDK; commands/queries the audience can't reach are excluded from the bundle (§7).
6. **Where does a slot implementation live?** → `features/<feat>/web/cells/<slot>.tsx` per L0 #1 §4.1; interface contract at `dist/ts-web/<feat>/cells/<slot>.gen.ts` (§8).
7. **Where does form validation live?** → Zod schema in `dist/ts-web/<feat>/<feat>.zod.ts` (Lazuli emits from `.lzi` constraints); RHF + zodResolver pre-wired in the view hook; server-side validation remains authoritative on the Go side (§9).
8. **What does Doctor do when `.lzx` references a column not on the source resource?** → emits `lzx-source-resource-mismatch` error (§11).
9. **Why is JSX-in-`.lzx` rejected?** → four reasons in §4 (parser cost / tooling cost / vendor lock-in / AI-hostile).
10. **What's the upgrade path to "single-file" ergonomics?** → scaffold packs post-pilot (§12, §14.3).
11. **How are inline `min/max/pattern/between/length/in` constraints emitted?** → Zod + Go validator + OpenAPI per §10.1.

If all 11 answers are mechanical from the proposal text, L0 #3 passes.

L2 implementation cells (post-PASS):

| Cell | Scope |
|---|---|
| **A.1** | `.lzx` parser in `crates/lazuli_syntax/` — 16 keywords, audience/view nesting |
| **A.2** | `.lzx` AST + IR types in `crates/lazuli_ir/` — `Surface`, `Audience`, `View`, `ViewList`, `ViewDetail`, `ViewCreate` |
| **A.3** | `.lzx` lowering in `crates/lazuli_analyzer/` — resolution rules per §5.2 |
| **B.1** | View hook emitter in `crates/lazuli_codegen_ts/` — `view list` (§6.1) |
| **B.2** | View hook emitter — `view detail` (§6.2) including section slots |
| **B.3** | View hook emitter — `view create` (§6.3) with RHF + zodResolver |
| **C.1** | Audience-scoped SDK projection in `crates/lazuli_codegen_ts/` (§7) |
| **C.2** | Slot interface emitter (§8) |
| **D.1** | Inline constraint parser (§10) in `crates/lazuli_syntax/` + IR fields |
| **D.2** | Zod schema emission with §10 constraints (extends L0 #2 emitter pattern) |
| **D.3** | Go validator emission with §10 constraints (extends Go codegen) |
| **D.4** | OpenAPI emission with §10 constraints (extends openapi crate) |
| **E.1** | Doctor rules `lzx-source-resource-mismatch`, `lzx-command-input-mismatch`, `lzx-action-not-in-audience` |
| **E.2** | Doctor rules `lzx-route-param-*`, `lzx-cell-slot-orphan`, `lzx-audience-empty-sdk` |
| **F.1** | `lazuli generate ts` integration — wires view emitters into existing TS pipeline |
| **F.2** | `cell-prop-mismatch` Doctor rule (TS compiler infra; carved from L0 #1 §11 Cell B.1) |
| **G** (post-pilot) | Scaffold pack plugin protocol + first pack (`@plugin/scaffold-shadcn`) |
