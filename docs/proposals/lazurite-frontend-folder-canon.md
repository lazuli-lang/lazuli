# Proposal — Lazurite Frontend Folder Canon

**Status:** L0 v0.1 PASS @ 9.04/10 — 2026-05-14 (graded by `lazuli-language-architect`; no blockers; 2 polish items applied inline; 4 polish items carved into L2 follow-up cells)
**Author:** Claude Opus 4.7 (orchestrator)
**Audit-ready target:** ≥ 9.0 via `lazuli-language-architect`
**Extends:** `docs/proposals/lazurite-scaffold.md` (§3 backend folder shape)
**Honors:** `docs/invariants.md:14-15` (boundary: app.lzi owns envs+urls; manifest owns codegen+plugins)
**Successors:** `docs/proposals/design-tokens.md` (L0 #2), `docs/proposals/lzx-integration-codegen.md` (L0 #3)

---

## §1. Status & motivation

Pleiades v2 backend builds end-to-end (lazuli@2b678e4, pleiades@3f5ce3b — 92 generated Go files, 11 features, exit 0). The next step is the web frontend. Three products (Pleiades, Atelier, Erudito) need to ship React UIs consuming the typed SDK; none has `.lzx` or `.tsx` yet.

`docs/proposals/lazurite-scaffold.md` §3 defines backend folder conventions (`features/<feat>/{<feat>.lzi, handlers/, queries/, jobs/, integrations/, templates/, i18n/}`). The frontend side is undefined: there is **no convention for where user-authored React/RN files live** relative to their declaring feature. A Pleiades developer (or LLM) opening the repo today must invent the structure, and the React/Vite default — `src/components/`, `src/pages/`, `src/hooks/`, `src/lib/`, `src/api/`, `src/types/` — actively breaks Lazuli's mental model:

- Domain (M), View Model (VM), and View (V) for the **same feature** live in **6 different directories**.
- LLM editing the `slug` feature must search 6 paths to gather context. Context window wasted on plumbing discovery.
- Diff in a PR touching `slug` is scattered across `src/types/Slug.ts`, `src/api/slugs.ts`, `src/hooks/useSlugs.ts`, `src/pages/slugs/index.tsx`, `src/components/SlugTable.tsx`. Review burden multiplied.
- Refactor risk: renaming the feature requires renaming files in 6 places.
- Doctor cannot enforce typed-hook usage if hooks can live anywhere.

This proposal fixes the gap by mandating **feature-based co-location** for the frontend, mirroring the backend convention already established. Doctor enforces. `lazuli new` and `lazuli generate feature` scaffold the canonical structure so users start correct on day one.

**Why now:** Pleiades web work begins immediately after L0 #3 (`lzx-integration-codegen`) ships. Without folder canon, the lzx codegen has no contract for where to emit `<feature>/views/<audience>/<view>.gen.ts` and no rule for where user-authored siblings live. L0 #1 is foundational.

**Boundary discipline:** This proposal defines folder *paths and ownership*. It does NOT define what those folders' contents render visually, what design system to use, or which components to include. Per the guiding principle below, **Lazurite owns structure and glue; product code owns interaction and rendering.**

---

## §2. Guiding principle

> **Lazurite owns structure and glue; product code owns interaction and rendering.**

Concrete projection:

| Lazurite owns | Product code owns |
|---|---|
| Path of every file (where it lives) | What that file renders |
| Naming convention (kebab-case filenames matching DSL refs) | Component decomposition inside a file |
| File-kind boundaries (`.lzi` = M, `.lzx` = VM, `.tsx` = V) | Library choice (TanStack Table vs AG Grid vs Mantine vs custom) |
| Where shared primitives live (`app/ui/`) | Whether to use Shadcn / brand custom / Headless UI |
| Co-location with the feature it serves | Layout strategy (CSS grid vs flex vs absolute) |
| Whether a file is allowed to exist (Doctor enforce) | Internal state shape inside a `.tsx` (`useState`, refs, modals) |

This boundary is the same one separating Rails (`app/models/`, `app/controllers/`, `app/views/`) from the ActiveRecord/ActionController/ActionView implementations — Rails decides path + naming + lifecycle; product code decides the business logic that lives there.

---

## §3. Scope

### In scope

1. **Top-level project layout** for a Lazurite-shaped product with one or more frontends declared in `lazurite.toml`.
2. **Per-feature frontend layout** (`features/<feat>/web/`, `features/<feat>/mobile/`) for cells (`@client.*` slot implementations) and views (audience-scoped page-level React/RN code).
3. **Cross-feature `app/` directory**: `shell/`, `theme/`, `ui/`, `lib/`.
4. **Doctor rules** that enforce the canon and reject React/Vite-style anti-patterns.
5. **`lazuli new` updates** — when `--frontends web|mobile|web,mobile` flag is set, scaffold the `app/shell/` and `app/theme/` skeleton.
6. **`lazuli generate feature <name>`** new subcommand — creates the canonical feature subfolders (`web/cells/`, `web/views/admin/`, `handlers/`, etc.) and a minimal `<feat>.lzi` stub.
7. **Migration path** for existing fixtures (`examples/full-capsule/`, etc.) and Pleiades/Atelier/Erudito repos.

### Non-goals

1. **Backend folder canon** — already defined in `docs/proposals/lazurite-scaffold.md` §3. This proposal only references it; no changes to `handlers/`/`queries/`/`jobs/`/`integrations/`/`templates/` shape.
2. **What's INSIDE `cells/`** — the user is free to organize each cell as a single file (`status_cell.tsx`) or a directory (`status_cell/{index.tsx, story.tsx, test.tsx}`). Doctor doesn't care.
3. **App-shell content** — `app/shell/web/root.tsx` is scaffolded once; its contents (which providers, which router, which layout) are user-owned. This proposal just creates the file with a sensible stub.
4. **Design tokens / `design.lzi`** — L0 #2 (`docs/proposals/design-tokens.md`). This proposal references the file's location (`design.lzi` at project root) but does not specify its grammar.
5. **lzx → typed hook emission** — L0 #3 (`docs/proposals/lzx-integration-codegen.md`). This proposal specifies WHERE the emitter outputs; it does not specify what the emitter generates.
6. **Component composition rules** — no rule about "a view must use ≤ N components" or "cells must be ≤ 100 LOC". Lazurite is path-aware, not size-aware.
7. **Style strategy** — Tailwind vs CSS-in-JS vs CSS modules vs vanilla-extract — user choice, no Lazurite opinion.
8. **Test file location** — co-located with the file under test (`list.tsx` + `list.test.tsx`) or in `__tests__/` subdirs — user choice.
9. **Scaffold packs for specific design systems** (Shadcn copy-paste, Mantine, MUI) — deferred to a future plugin proposal `@plugin/scaffold-<pack>`.

---

## §4. Canonical frontend layout

The complete layout a Lazurite product produces (showing both backend already-scaffolded paths and new frontend additions). New additions marked `← NEW`.

```
myapp/
├── lazurite.toml                       # workspace manifest
├── app.lzi                             # app envelope (envs, urls, deploy)
├── registry.lzi                        # capabilities + integrations
├── design.lzi                          # ← NEW: design tokens (L0 #2)
├── profiles.lzi                        # optional: env-specific overlays
├── package.json                        # node deps (root level — frontends share)
├── tsconfig.json                       # ← NEW: scaffolded once by `lazuli new`; user-owned thereafter (Lazuli never overwrites)
├── tailwind.config.ts                  # ← NEW: imports preset from dist; scaffolded once, user-owned
├── vite.config.ts                      # ← NEW: vite/next/expo config; scaffolded once, user-owned
├── README.md
├── .gitignore                          # ignores dist/, .lazuli/, node_modules/
│
├── features/                           # FEATURE-BASED (the source of truth)
│   ├── account/
│   │   ├── account.lzi                 # M — domain contract
│   │   │
│   │   ├── account.web.lzx             # ← NEW: VM web (audience + view + bindings)
│   │   ├── account.mobile.lzx          # ← NEW: VM mobile (optional)
│   │   │
│   │   ├── handlers/                   # @fn.* — Go
│   │   │   ├── verify_password.go
│   │   │   └── hash_password.go
│   │   ├── queries/                    # query.sql @file.* — raw SQL
│   │   ├── jobs/                       # job handler @fn.* — Go
│   │   ├── integrations/               # webhook verify, adapter — Go
│   │   ├── templates/                  # email/notif templates
│   │   ├── i18n/                       # per-feature translations
│   │   │
│   │   ├── web/                        # ← NEW: V (web) — user-authored React
│   │   │   ├── cells/                  # @client.<slot> typed implementations
│   │   │   │   └── status_cell.tsx
│   │   │   └── views/                  # page-level views, audience-scoped
│   │   │       ├── admin/
│   │   │       │   └── login.tsx
│   │   │       └── public/
│   │   │           └── signup.tsx
│   │   │
│   │   └── mobile/                     # ← NEW: V (RN/Expo)
│   │       ├── cells/
│   │       └── views/
│   │
│   └── slug/                           # ← same structure for every feature
│       └── ...
│
├── app/                                # ← NEW: CROSS-FEATURE (limited scope)
│   ├── shell/                          # app-root, navigation, providers
│   │   ├── web/
│   │   │   ├── root.tsx                # mount Lazuli + Router + Theme providers
│   │   │   ├── layout.tsx              # navigation + outlet
│   │   │   └── error_boundary.tsx
│   │   └── mobile/
│   │       └── root.tsx
│   │
│   ├── theme/                          # design token consumption
│   │   ├── globals.css                 # @import "@/dist/ts-web/design/tokens.css"
│   │   └── theme_provider.tsx          # data-theme switch
│   │
│   ├── ui/                             # SHARED primitives, opt-in
│   │   │   (Shadcn copies, brand Button, generic Input)
│   │   ├── button.tsx
│   │   ├── input.tsx
│   │   ├── dialog.tsx
│   │   └── form_field.tsx
│   │
│   └── lib/                            # truly app-wide helpers (rare)
│       ├── format_currency.ts
│       └── date_utils.ts
│
├── i18n/                               # app-wide catalogs
│   ├── common.en-US.json
│   └── common.pt-BR.json
│
├── public/                             # static assets
│   └── logo.svg
│
├── migrations/                         # SQL migrations (generated + manual)
│   └── 20260514_001_add_slug.sql
│
├── dist/                               # GENERATED — never user-edited
│   ├── go/                             # Go runtime user-code
│   ├── ts-web/                         # ← NEW: typed hooks + SDK + design preset
│   │   ├── design/                     # tokens emission
│   │   │   ├── tokens.ts
│   │   │   ├── tokens.css
│   │   │   └── tailwind.gen.ts
│   │   ├── slug/                       # per-feature emission
│   │   │   ├── slug.gen.ts             # types + commands + queries
│   │   │   ├── slug.zod.ts             # Zod schemas (companion)
│   │   │   ├── views/
│   │   │   │   ├── admin/
│   │   │   │   │   ├── list.gen.ts     # typed view spec + hook
│   │   │   │   │   ├── detail.gen.ts
│   │   │   │   │   └── create.gen.ts
│   │   │   │   └── public/
│   │   │   │       └── list.gen.ts
│   │   │   └── cells/
│   │   │       └── status_cell.gen.ts  # typed slot interface
│   │   └── routes.gen.ts               # router registration
│   │
│   └── ts-mobile/                      # ← NEW: same shape, RN/Expo flavor
│       └── ...
│
└── .lazuli/                            # internal cache (gitignored)
    ├── graph.json
    └── manifest.json
```

### §4.1 Feature subfolder per platform

Inside each feature, frontend code is partitioned by platform (`web/`, `mobile/`), each containing `cells/` and `views/<audience>/`:

```
features/slug/
├── slug.lzi                  # M
├── slug.web.lzx              # VM web (declares audiences + views)
├── slug.mobile.lzx           # VM mobile (optional; may differ in views/columns)
├── handlers/                 # Go (shared across frontends)
├── web/
│   ├── cells/                # @client.<slot> bound by slug.web.lzx
│   │   ├── status_cell.tsx
│   │   └── type_badge.tsx
│   └── views/                # one dir per audience
│       ├── admin/            # audience-scoped page components
│       │   ├── list.tsx
│       │   ├── detail.tsx
│       │   └── create.tsx
│       └── public/
│           └── list.tsx
└── mobile/
    ├── cells/
    │   └── status_cell.tsx   # same slot, RN-flavored (View+Text instead of div)
    └── views/
        └── admin/
            └── list.tsx      # FlatList instead of <table>
```

**Naming rule** (Doctor enforce):
- Cell filename matches the slot name: `slug.web.lzx` declares `cells tags @client.type_badge` → `features/slug/web/cells/type_badge.tsx` must exist and default-export a component matching `TypeBadgeProps` (from `dist/ts-web/slug/cells/type_badge.gen.ts`).
- View filename matches the view name: `slug.web.lzx` declares `view list slug_list at "/slugs"` inside `audience admin` → `features/slug/web/views/admin/list.tsx` (the view kind, `list`, becomes the filename; `slug_list` becomes the view's identifier in metadata).

The convention is **DSL declaration → file path is mechanical and deterministic**. No discovery, no ambiguity. Both LLM and human know exactly where to look.

**Shared types within a feature** (web ↔ mobile): derived helper types that both platforms need (e.g. a discriminator union, a computed-field shape) live in the generated `dist/ts-<target>/<feat>/<feat>.gen.ts` — both `web/` and `mobile/` import from the same generated module. No `features/<feat>/shared/` carve-out; if the type isn't in the generated module, it's specific to one platform and stays under that platform's tree.

### §4.2 Cross-feature `app/`

`app/` is the home for code that **legitimately spans features**. It is intentionally a small, closed catalog of subdirs:

| Subdir | Purpose | Examples |
|---|---|---|
| `app/shell/web/` | Web app root — providers, router setup, top-level layout | `root.tsx`, `layout.tsx`, `error_boundary.tsx` |
| `app/shell/mobile/` | RN/Expo app root | `root.tsx`, `navigation.tsx` |
| `app/theme/` | Theme consumption (NOT token declaration — that's `design.lzi`) | `globals.css`, `theme_provider.tsx` |
| `app/ui/` | Shared visual primitives (Shadcn copies, brand components) | `button.tsx`, `input.tsx`, `dialog.tsx` |
| `app/lib/` | Generic helpers used across features (rare; prefer in-feature) | `format_currency.ts`, `date_utils.ts` |

**Hard rule**: `app/` does NOT contain feature-specific code. If something is specific to `slug`, it goes in `features/slug/web/`. If it's generic enough to be reused by 3+ features, it goes in `app/ui/` or `app/lib/`. Doctor checks imports: `app/ui/<x>` cannot import from `features/<feat>/`.

The Rails analogy: `app/views/layouts/application.html.erb` is cross-feature shell; `app/views/customers/index.html.erb` is feature-specific. Same idea, same separation.

### §4.3 Why this is enough (and not more)

A common temptation is to add more cross-feature folders: `app/components/`, `app/hooks/`, `app/utils/`, `app/contexts/`, `app/services/`. Each is a slippery slope:

- **`app/components/`** → catch-all dumping ground that drifts from `ui/`. Either it's a generic primitive (use `ui/`) or it belongs to a feature (use `features/<feat>/web/cells/`). Reject.
- **`app/hooks/`** → hooks should be per-feature (in the emitted `dist/ts-web/<feat>/views/<a>/<v>.gen.ts`) or genuinely generic (use `lib/`). Custom hooks living in `app/hooks/` usually indicate missing Lazuli vocabulary or a feature boundary error.
- **`app/utils/`** → alias for `lib/`. Reject duplicate.
- **`app/contexts/`** → React Context for cross-cutting state (auth, theme, locale). These belong in `shell/` because they're providers mounted at root.
- **`app/services/`** → frontend "service" layer is a smell: data access goes through `useLazuliQuery` / `useLazuliCommand`, not service singletons. Reject.

The closed list (`shell/`, `theme/`, `ui/`, `lib/`) is a feature, not a limitation. Closing the catalog forces the user (and LLM) to think about WHICH FEATURE owns a piece of code before adding it. Most "I need a new app-level folder" thoughts are actually "I should think about which feature this belongs to."

---

## §5. Doctor rules

Doctor enforces folder canon via path + import checks. Rules listed by code; severity escalates from `prototype` (warning) to `production` (error) following the same scale as other doctor rules.

### §5.1 Forbidden React/Vite anti-patterns

| Code | Trigger | Severity | Resolution |
|---|---|---|---|
| `feature-orphan-component` | `.tsx` file under `src/`, `app/components/`, or any non-canonical path | error in production | Move to `features/<feat>/web/views/<audience>/` or `app/ui/` |
| `pages-bypass` | `pages/<…>.tsx` or `app/(routes)/<…>` (Next.js page dir at root) | error in production | Routes come from `.lzx` `view ... at "<route>"` declarations. Lazuli emits `dist/ts-web/routes.gen.ts` — wire it in `app/shell/web/root.tsx` |
| `type-duplicate` | `*.ts(x)` file declaring an interface matching a generated `dist/ts-web/<feat>/<feat>.gen.ts` type name | warning in strict, error in production | Import the generated type instead |
| `client-bypass` | Direct `fetch()` / `axios()` call to a Lazuli-managed API path | warning | Use `useLazuliQuery` / `useLazuliCommand` — they handle envelope, errors, cache, tenancy. Direct fetch loses all of that. Allowed for genuinely external endpoints |
| `lazuli-hook-bypassed` | Custom hook named `use<Feature>` / `use<Feature><View>` that does not import from `dist/ts-web/` | warning | Either delete (use the generated hook) or rename to avoid collision |
| `cross-feature-direct-import` | `features/A/web/<…>.tsx` imports from `features/B/web/cells/<…>` | error | Cross-feature view dependencies go via slot binding (`@client.<slot>`) declared in `B.web.lzx` — A's `.lzx` `cells x @client.foo` then resolves to B's typed slot interface |
| `external-state-store` | Redux / Zustand / Jotai / Valtio store referencing server data | warning | Server state lives in TanStack Query (TQ-managed via `useLazuliQuery`). UI-ephemeral state in `useState`/`useReducer` is fine; persistent client state in a store is fine — but server data should not be duplicated |

### §5.2 Required structural shapes

| Code | Trigger | Severity | Resolution |
|---|---|---|---|
| `cell-missing-impl` | `<feat>.web.lzx` declares `cells <field> @client.<slot>`, but `features/<feat>/web/cells/<slot>.tsx` does not exist | error | Create the file. Lazuli emits the slot interface at `dist/ts-web/<feat>/cells/<slot>.gen.ts`; user `.tsx` implements it |
| `cell-prop-mismatch` | Cell `.tsx` default-exports a component whose prop type does not satisfy the generated slot interface | error | Fix the prop shape. Generated interface is single source of truth |
| `view-missing-impl` | `<feat>.web.lzx` declares `view <kind> <name>` inside `audience <a>`, but `features/<feat>/web/views/<a>/<kind>.tsx` does not exist | warning | Create the file (or remove the view declaration) |
| `audience-frontend-empty` | `lazurite.toml [frontends.<x>] audiences = [...]` lists an audience that has zero views declared in any `<feat>.<x>.lzx` | warning | Either drop the audience from the frontend or declare at least one view for it |
| `app-ui-feature-import` | File in `app/ui/<…>` imports from `features/<feat>/` | error | Generic UI primitives must not depend on specific features. Move the import-er to `features/<feat>/web/cells/` if feature-specific |

### §5.3 Acceptable patterns Doctor explicitly allows

These look like anti-patterns at a glance but are valid; rules above carve them out:

- **Co-located test files**: `list.tsx` + `list.test.tsx` in the same dir. Doctor ignores `*.test.tsx` / `*.spec.tsx` from structural checks.
- **Storybook files**: `*.stories.tsx` allowed in any frontend dir.
- **Cell directory grouping**: `cells/status_cell/` directory with `{index.tsx, story.tsx, test.tsx}` allowed — Doctor recognizes both `cells/<slot>.tsx` and `cells/<slot>/index.tsx`.
- **`app/ui/` Shadcn-style copies**: copy-pasted Shadcn components in `app/ui/` are first-class citizens; they may pull from `@radix-ui/*`, `lucide-react`, etc.
- **Server-only imports**: `.tsx` files importing `@tanstack/react-query` etc. fine. Server-only imports go in `handlers/` (Go-side), so this rule doesn't fire on frontend.

---

## §6. `lazuli new` / `lazuli generate feature` updates

### §6.1 `lazuli new --frontends <list>`

Current behavior: scaffolds 5 files + `features/.gitkeep`. Extend to accept `--frontends web|mobile|web,mobile` flag.

```bash
lazuli new pleiades --frontends web
```

Produces the canonical layout including:

- `lazurite.toml` with `[frontends.web]` block pre-filled:
  ```toml
  [frontends.web]
  target = "vite-react"
  out = "dist/ts-web"
  audiences = ["admin", "public"]
  ```
- `app/shell/web/root.tsx` stub:
  ```tsx
  import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
  import { LazuliProvider, LazuliClient } from "@lazuli/runtime/react";
  // import { router } from "./router.gen"; // emitted by lazuli generate ts

  const queryClient = new QueryClient();
  const client = new LazuliClient({ baseUrl: import.meta.env.VITE_API_URL ?? "/api" });

  export function App() {
    return (
      <QueryClientProvider client={queryClient}>
        <LazuliProvider client={client}>
          {/* <RouterProvider router={router} /> — wire after first `lazuli generate ts` */}
        </LazuliProvider>
      </QueryClientProvider>
    );
  }
  ```
- `app/shell/web/layout.tsx` stub (empty `<Outlet />`).
- `app/theme/globals.css` with `@import "@/dist/ts-web/design/tokens.css"`.
- `package.json` with peer deps: `react`, `@tanstack/react-query`, `@tanstack/react-router` (or alternative per `target`), `@lazuli/runtime`, `react-hook-form`, `@hookform/resolvers`, `zod`, `tailwindcss`.
- `tailwind.config.ts` consuming the preset from `dist/ts-web/design/tailwind.gen.ts`.
- `tsconfig.json` with path aliases (`@/` → project root).
- `vite.config.ts` with sensible defaults.

### §6.2 `lazuli generate feature <name>` (new subcommand)

Adds a new feature with all canonical subfolders. Frontend-side subfolders only created when the manifest has frontends declared.

```bash
lazuli generate feature billing
```

Produces:
```
features/billing/
├── billing.lzi              # minimal stub
├── billing.web.lzx          # empty (only if [frontends.web] exists)
├── handlers/.gitkeep
├── queries/.gitkeep
├── jobs/.gitkeep
├── integrations/.gitkeep
├── templates/.gitkeep
├── i18n/.gitkeep
└── web/                     # only if [frontends.web] exists
    ├── cells/.gitkeep
    └── views/.gitkeep
```

`billing.lzi` stub:
```lazuli
feature billing
  purpose "..."

  domain
    # add resources, queries, commands here

  policies
    # add policy categories here
```

`billing.web.lzx` stub (when web frontend declared):
```lazuli
surface billing web
  uses feature billing

  # audience admin
  #   requires @scope.workspace_admin
  #
  #   view list billing_list at "/billing"
  #     source billing.query.list
  #     columns ...
  #     actions ...
```

Commented-out skeleton helps both LLM and human get started without ambiguity.

### §6.3 `lazuli generate cell <feature>.<slot>` (helper)

Slot scaffolding helper. Given a `.lzx` declaring `cells tags @client.type_badge`:

```bash
lazuli generate cell slug.type_badge
```

Creates `features/slug/web/cells/type_badge.tsx` (and `mobile/cells/type_badge.tsx` if mobile frontend exists) with:

```tsx
import type { TypeBadgeProps } from "@/dist/ts-web/slug/cells/type_badge.gen";

export default function TypeBadge({ value, row }: TypeBadgeProps) {
  // TODO: render the badge
  return <span>{String(value)}</span>;
}
```

User fills the JSX. Doctor stops complaining (`cell-missing-impl` resolves). Hook of code stays minimal.

---

## §7. Migration path

### §7.1 Existing fixtures

| Fixture | Today | Action |
|---|---|---|
| `examples/full-capsule/` | All `.lzx` at top level (`full-capsule.lzx`, `full-capsule.account.web.lzx`, `.admin.web.lzx`, `.public.web.lzx`, `.sales.mobile.lzx`); no `features/<feat>/` decomposition | **Grandfather** as "monolithic legacy fixture" via Doctor flag `legacy-monolithic-fixture` (allowed only when `lazurite.toml [doctor] legacy_monolithic_fixture = true`). Acts as backstop for cross-feature view extensions (`extends @anchor.*`). New fixtures must follow canon. |
| `examples/auth-roundtrip/` | `features/account/account.lzi` only | Add `account.web.lzx` + `web/views/admin/login.tsx` stub once L0 #3 ships |
| `examples/smoke-hello/` | Minimal | Leave alone (smoke fixture; no frontend) |
| `examples/marketplace-mini/` | Backend port reference | Add `web/` skeleton when used as a frontend port reference |
| `examples/lazurite-multifrontend/` | Has `features/property/property.admin.web.lzx`, `property.host.web.lzx` — `<audience>.<target>.lzx` naming | **Reject** the `<audience>.<target>` split. Canon is one `.lzx` per `(feature, target)` with audiences declared inside via `audience <name>` blocks. Migration cell collapses `property.admin.web.lzx` + `property.host.web.lzx` → single `property.web.lzx` with two audience blocks. |

Migration of existing fixtures is **separate cells**, not part of this proposal's L2 implementation. Each fixture migrates only when a real user-shaped task touches it. The two fixture-specific decisions above (grandfather full-capsule; reject multifrontend's per-audience filename split) are normative — Doctor implements them as written when L2 cells land.

### §7.2 Pleiades / Atelier / Erudito (the three dogfood products)

Per `project_three_products_lazuli_dogfood.md`, all three currently have only `.lzi` files. The migration is **additive**, not destructive:

1. **Phase A (after L0 #1 + #2 + #3 PASS)**: each product adds `<feat>.web.lzx` files for the features that need a UI. Pleiades first (per strategic pivot).
2. **Phase B**: scaffold `app/shell/web/`, `app/theme/`, `app/ui/` at the repo root using `lazuli new --frontends web --in-place` (a new variant that scaffolds frontend layer into an existing repo).
3. **Phase C**: per-feature `web/cells/` and `web/views/<audience>/` populated as features ship UI.

No backend code moves. No existing `.lzi` changes (except adding `.lzx` siblings). The migration is lift-shaped: pure addition.

### §7.3 Doctor as migration progress meter

Once L0 #1 ships and Doctor rules are live, running `lazuli doctor` against an in-progress migration produces a punch list of `view-missing-impl` and `cell-missing-impl` warnings. The list IS the migration roadmap.

---

## §8. Examples — Pleiades-shaped

### §8.1 `features/slug/` complete layout

```
features/slug/
├── slug.lzi                              # M (existing, see pleiades@3f5ce3b)
├── slug.web.lzx                          # VM (authored after L0 #3 ships)
│
├── handlers/                             # Go (existing or pending)
│   ├── search_slugs.go
│   └── ...
├── queries/
│   └── fulltext_search.sql               # (when query.sql @file.fulltext_search referenced)
├── i18n/
│   ├── slug.en-US.json
│   └── slug.pt-BR.json
│
├── web/
│   ├── cells/
│   │   └── type_badge.tsx                # implements TypeBadgeProps
│   └── views/
│       ├── admin/
│       │   ├── list.tsx                  # consumes useAdminSlugListView
│       │   ├── detail.tsx
│       │   └── create.tsx
│       └── public/
│           └── list.tsx
│
└── mobile/                               # optional, only if slug.mobile.lzx exists
    ├── cells/
    │   └── type_badge.tsx
    └── views/
        └── admin/
            └── list.tsx
```

### §8.2 Cross-platform sharing via slot pattern

A common question: "If `slug.web.lzx` and `slug.mobile.lzx` share most metadata (columns, actions), do I duplicate?"

Answer: yes, declarations are explicit. Per `docs/design-principles.md` ("Self-Contained Declarations", "Total Override Only"), Lazuli does not cascade. A web view and a mobile view may diverge on columns, actions, or routes — declaring both is the spec.

Migration helper (post-L0-#3): if `slug.web.lzx` and `slug.mobile.lzx` are byte-for-byte identical aside from `surface slug web` vs `surface slug mobile`, Doctor emits info-level diagnostic `lzx-redundant-mobile` suggesting either (a) keep both (intentional — mobile may diverge later), or (b) drop `slug.mobile.lzx` and configure mobile frontend to read web `.lzx`. The default is (a).

### §8.3 What an LLM sees opening `features/slug/`

Opening the directory listing:

```
slug.lzi           ← the domain
slug.web.lzx       ← the web VM
slug.mobile.lzx    ← the mobile VM
handlers/          ← Go side
queries/           ← raw SQL
i18n/              ← translations
web/cells/         ← typed React slot implementations
web/views/admin/   ← admin web pages
web/views/public/  ← public web pages
mobile/cells/      ← RN slot implementations
mobile/views/admin/← admin mobile pages
```

11 paths, all named after their role. Zero ambiguity. An LLM reading this directory has full feature context loaded into a single tool call. Compare to a Vite-default repo where finding "everything about slug" means 7 separate `Glob` / `Grep` operations across `src/types/`, `src/api/`, `src/hooks/`, `src/components/`, `src/pages/`, `src/i18n/`, `src/store/`.

---

## §9. Open questions / Future work

### §9.1 Scaffold packs (Phase 2, post-pilot)

Out of v0 scope, mentioned only as future direction: a `lazuli scaffold view <feat>.<audience>.<view> --style <pack>` command that generates a first-draft `.tsx` for a declared view, using a pluggable design pack (Shadcn, Mantine, MUI, plain Tailwind). After initial scaffold, Lazuli does NOT overwrite the file — it remains user-owned.

Implementation lives in `@plugin/scaffold-<pack>` repos, not the Lazuli core. Out of L0 #1, in a separate L0 once pilot evidence shows demand.

### §9.2 Per-frontend feature filtering

Some frontends consume only a subset of features (e.g. a public marketing site needs `slug` and `account` but not `trigger`). Should `lazurite.toml [frontends.<x>] features = [...]` filter? Or should that come from `audiences` (audience reachability already implies feature subset)?

Defer. Audience-based filtering is sufficient for v0; explicit feature filter only if real cases need it.

### §9.3 Multiple frontends from one feature

A product may have a web admin (TanStack Router/Vite) + an embedded customer portal (Next.js app router) + a mobile app (Expo). Three frontends, all consuming `features/slug/web/views/admin/` and `features/slug/web/views/public/`?

For v0: each frontend can declare `audiences = [...]` independently. The same `view` files serve all matching audiences. Frontends pick which audiences to bundle.

Future: per-frontend view overrides (`features/slug/web-admin/views/...`)? Defer until a real case appears.

### §9.4 Storybook / design system tooling

Storybook conventions (`*.stories.tsx`) are allowed but not scaffolded. A `@plugin/scaffold-storybook` could auto-generate stories per view. Defer to plugin space.

### §9.5 Backend folder canon refresh

`docs/proposals/lazurite-scaffold.md` §3 defines backend folder layout. It does NOT mention `<feat>.web.lzx` / `<feat>.mobile.lzx` (proposed at L121 but without frontend conventions for `cells/`/`views/`). This proposal is the **frontend half** of that spec. A future L0 may consolidate the two into a single `docs/conventions/folder-canon.md` reference doc; for now, the two proposals are siblings.

---

## §10. References

- `docs/proposals/lazurite-scaffold.md` §3 — backend folder shape (foundation this proposal extends)
- `docs/invariants.md:14-15` — `app.lzi` owns envs+urls; manifest owns codegen+plugins (boundary discipline)
- `docs/design-principles.md` — Rule Zero (Vocabulary Over Mechanism); Self-Contained Declarations; Total Override Only; No Cascade
- `docs/architecture.md` §"Lazuli vs Lazurite" — the framework / distro split
- `docs/quickref.md` — agent context pack
- Memory: `project_lazuli_drusa_philosophy.md`, `project_plugin_namespace_policy.md`, `feedback_grade_before_commit.md`
- Three products dogfood context: `project_three_products_lazuli_dogfood.md`, `project_pleiades_buildable_session_2026-05-14.md`
- Successor proposals: `docs/proposals/design-tokens.md` (L0 #2), `docs/proposals/lzx-integration-codegen.md` (L0 #3)

---

## §11. Acceptance criteria

L0 PASS condition: the proposal answers, for any Lazurite-shaped product, the following deterministically:

1. **Given a feature `<feat>` with a `.lzx` declaring view `V` under audience `A`, where does the user's React file live?**
   → `features/<feat>/web/views/<A>/<V>.tsx` (or `mobile/views/<A>/<V>.tsx`)
2. **Given a `.lzx` declaring cell slot `@client.foo`, where is the implementation?**
   → `features/<feat>/web/cells/foo.tsx` (or `mobile/cells/foo.tsx`)
3. **Where do shared UI primitives go?**
   → `app/ui/`
4. **Where does the app root / providers / router mount live?**
   → `app/shell/web/root.tsx` (or `mobile/root.tsx`)
5. **Where do design tokens live?**
   → `design.lzi` at project root (L0 #2)
6. **What does Doctor do when a user creates `src/components/SlugTable.tsx`?**
   → Emits `feature-orphan-component` warning (strict) or error (production), with a fix suggestion pointing at `features/slug/web/views/admin/list.tsx` or `app/ui/`.
7. **What does `lazuli new pleiades --frontends web` produce?**
   → The scaffold listed in §6.1.

If all seven answers are mechanical from the proposal text, L0 #1 passes.

L2 implementation cells (post-PASS):
- **Cell A**: Doctor rules `feature-orphan-component`, `pages-bypass`, `type-duplicate`, `cross-feature-direct-import` (path-based, low blast radius).
- **Cell B**: Doctor rules `cell-missing-impl`, `view-missing-impl` (cross-reference `.lzx` declarations against filesystem). **Note**: `cell-prop-mismatch` (from §5.2) needs TS compiler infra (`ts-morph` or `tsc --noEmit`) and is carved into **Cell B.1** as a follow-up — heavier blast radius, separate dependency.
- **Cell C**: `lazuli new --frontends web` scaffold templates.
- **Cell D**: `lazuli generate feature <name>` subcommand.
- **Cell E**: `lazuli generate cell <feat>.<slot>` helper (defer until L0 #3 ships and slot interface emission is live).
- **Cell F** (migration): `examples/lazurite-multifrontend/` collapses `property.admin.web.lzx` + `property.host.web.lzx` → single `property.web.lzx` with two audience blocks (per §7.1 decision).
- **Cell G** (deferred polish): tighten `external-state-store` (§5.1) detection heuristic. Today the rule says "Redux/Zustand/Jotai/Valtio store referencing server data" but Doctor cannot mechanically detect "referencing server data". Carved as follow-up to (a) ship as plain warning on store imports with allowlist directive, or (b) drop the "server data" qualifier entirely. Pending real-world false-positive data before deciding.
- **Cell H** (deferred polish): reconcile `view-missing-impl` (warning) and `cell-missing-impl` (error) severities (§5.2). Possible outcomes: (a) both `error in production`; (b) keep asymmetric with documented rationale (cells are required slot bindings; views are declared pages but may be intentionally TODO during construction). Pending pilot feedback on which is less annoying in real authoring.
