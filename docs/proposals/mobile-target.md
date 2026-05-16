# Proposal — Mobile Target (Lazuli → React Native via Expo)

**Status:** L0 v0.2 DRAFT — 2026-05-15 (v0.1 graded 8.42/10 BLOCK via `lazuli-language-architect`; v0.2 applies 7 blockers + polish items inline)
**Author:** Claude Opus 4.7 (orchestrator)
**Audit-ready target:** ≥ 9.0 via `lazuli-language-architect`
**Driver:** Hostpoint mobile pilot (memory `project_strategic_pivot_2026-05-15`)
**Depends on:** `docs/proposals/lazurite-scaffold.md` (manifest), `docs/proposals/lazurite-frontend-folder-canon.md` (L0 #1, file canon), `docs/proposals/lzx-integration-codegen.md` (L0 #3, view emitters), `docs/proposals/lzx-terminal-grammar.md` (L0 #6, view-helpers)
**Honors:** `docs/invariants.md`, `docs/design-principles.md` (Rule Zero), `docs/architecture.md` (founding principle — wire, not reimplement)

---

## §1. Status & motivation

Lazuli ships a working web target. `lazuli new --frontends web` + `lazuli generate ts` + `pnpm dev` produces a Vite/React/TanStack app that consumes the Go runtime. The mobile half is **half-built**:

| Layer | Web | Mobile | Gap |
|---|---|---|---|
| `.lzx` grammar | `surface customer web` | `surface customer mobile` | ✅ Both parse; fixture `examples/full-capsule/full-capsule.sales.mobile.lzx` |
| IR | `SurfaceTarget::Web` | `SurfaceTarget::Mobile` | ✅ Both walk |
| View hook emission | `dist/ts-web/<feat>/views/...` | `dist/ts-mobile/<feat>/views/...` | ✅ Path canon emitted; emitters are platform-neutral by design |
| Router adapter | `RouterTarget::ViteReact` (`@tanstack/react-router`) | `RouterTarget::Expo` (`expo-router`) | ✅ Imports + `:key`→`[key]` translation |
| Per-feature SDK | `dist/ts-web/<feat>/<feat>.gen.ts` | `dist/ts-mobile/<feat>/<feat>.gen.ts` | ✅ Walker dispatches by `[frontends.<x>] target = "expo"` |
| Frontend scaffold | `frontends/web/` with shell, theme, Tailwind, Vite config, `package.json` | `frontends/mobile/shell/root.tsx` placeholder | ⚠️ Missing Expo Router `app/` directory; runtime not wired |
| Runtime hooks (`@lazuli/runtime/react`) | `useLazuliQuery`, `useLazuliCommand`, `useFilterState`, `useMultiSelection`, `useLocalSetting`, `useDrawerSubView`, `parseSegments`, `canonicalizeSearch` | Same package; two hooks silently no-op | ⚠️ `useLocalSetting` (uses `window.localStorage`) and `useDrawerSubView` (uses `window.addEventListener("keydown")`) degrade silently on RN |
| `@lazuli/runtime` package | `peerDependencies: react, @tanstack/react-query` | No `react-native` peer; no conditional exports | ⚠️ Metro resolves via default entry; works by accident, not declared |
| Doctor coverage | `lzx-*` rules cover web | Same rules apply to mobile surfaces | ✅ Doctor is target-agnostic |
| Smoke fixture | `examples/full-capsule/` web compiles, builds, serves | `examples/full-capsule/full-capsule.sales.mobile.lzx` parses; no end-to-end smoke | ⚠️ No fixture verifies `expo start` boots the generated app |

**Why now:** the active pilot is Hostpoint mobile (memory `project_strategic_pivot_2026-05-15`). Web is validated by Pleiades v2 (`project_pleiades_web_mvvm_proof_2026-05-14`). The next pilot port needs RN. Without this proposal, Hostpoint mobile reinvents the runtime split locally and the framework absorbs the pattern post-facto — exactly the negative reference the founding principle exists to avoid.

**Boundary discipline:**

> Lazurite owns structure and glue; product code owns interaction and rendering. The mobile target is **the same Lazuli abstraction projected onto a different rendering substrate**. No new vocabulary. No new IR types. No new doctor rules unique to mobile.

---

## §2. Scope

**This proposal is target-closing, not boundary-moving.** It adds zero new `.lzx` keywords, zero new IR types, zero new `@-namespace` entries, and zero new vocabulary. It locks the runtime + scaffold story for an IR target (`SurfaceTarget::Mobile`) that already exists. The `≥3-pilot evidence` rule from `feedback_scope_discipline_2026-05-14.md` does not apply because no new framework primitive is being introduced — the Hostpoint pilot is sufficient to validate that the already-shipped web target's primitives project cleanly onto mobile.

### In scope

1. **Runtime split via conditional exports** (§3). One `@lazuli/runtime` package, two implementations of `react.web.ts` and `react.native.ts`, resolved by Metro vs Vite via `package.json` `exports` conditions. Generated code stays target-agnostic.
2. **Native counterparts for web-only hooks** (§4): `useLocalSetting` (AsyncStorage instead of localStorage), `useDrawerSubView` (RN `BackHandler` + no-op for hardware Escape). Same exported signature; native bodies.
3. **Expo Router file-based scaffold** (§5). `frontends/mobile/app/_layout.tsx` + per-route page files emitted by `lazuli new --frontends mobile` and **regenerated** as views are added. Author owns each page body (View); Lazuli owns the routing tree.
4. **`Lazurite.toml` mobile manifest** (§6). Lock `[frontends.mobile]` schema (Expo SDK pin, app icon path, splash, scheme). Wire dev/build/start commands.
5. **Per-target SDK emission audit** (§7). Make sure `dist/ts-mobile/<feat>/<feat>.gen.ts` matches the web sibling byte-for-byte modulo router import — no accidental web-only types leaking.
6. **Hostpoint mobile fixture** (§8). `examples/marketplace-mini-mobile/` end-to-end: `.lzi` + `.mobile.lzx` + manifest + scaffold + smoke test that runs `lazuli generate ts` and asserts emitted file paths + does a Metro-style import-graph dry-run.
7. **Doctor coverage parity** (§9). Confirm all `lzx-*` rules + `cell-*` rules apply to mobile surfaces without modification; add **two** new rules: `MOBILE-WEB-ONLY-CELL` (a `cells … @client.<slot>` whose implementation file lives only under `web/`, not `mobile/`) and `MOBILE-ROUTE-COLLISION` (two views in the same audience emit the same Expo Router path).
8. **Headless v0** — no opinion on the visual library (no Tamagui pre-wire, no NativeWind in core). User picks RN component lib; cell `.tsx` files are V territory per L0 #3 §4.

### Non-goals

1. **No new `.lzx` keywords.** Mobile uses the same closed catalog as web.
2. **No new IR types.** `SurfaceTarget::Mobile` already exists.
3. **No cross-target single-file `.lzx`.** L0 #3 §14.4 deferred this; remains deferred until 2+ products hit duplication.
4. **No Tamagui / NativeWind / Gluestack opinion.** Headless v0. Scaffold packs (`@plugin/scaffold-tamagui`, etc.) post-pilot only, per L0 #3 §12.
5. **No EAS / app store publishing automation.** Expo CLI handles that; Lazuli stays out of deploy mechanics (per `docs/invariants.md` §"deploy in framework is out of scope").
6. **No native-module bridges authored from `.lzi`.** RN native modules are V territory.
7. **No bespoke mobile router.** Expo Router is the single supported choice (matches L0 #3 §6.2 table).
8. **No web-to-mobile auto-translation.** A `.web.lzx` is not auto-projected to `.mobile.lzx`; the user authors per-target deliberately (matches L0 #3 §14.4).
9. **No platform-specific cell-impl auto-generation.** If a slot needs both web and mobile implementations, the user authors `cells/<slot>.tsx` under `web/` and `cells/<slot>.tsx` under `mobile/` independently.
10. **No support for Flutter or other RN alternatives.** Single mobile target locked per `docs/target-stack.md`.

---

## §3. Runtime split — conditional exports

Three candidates considered and one selected.

| Option | Mechanism | Verdict |
|---|---|---|
| **A. Single package + `Platform.OS` / `typeof window` guards** | One file per hook with `if (Platform.OS === "web") …` ladders. | **REJECTED** — leaks `react-native`'s `Platform` import into web bundles (or vice versa); silent-degradation surface area grows linearly with hook count; smells of "wire-fat compromise" from the founding principle. |
| **B. Hard split: `@lazuli/runtime-web` + `@lazuli/runtime-native`** | Two packages, independent versioning. Generated code imports the right one per target. | **REJECTED** — forces emitter to track which package to reference; doubles publish surface; independent versioning is a non-goal because Lazuli ships runtime+codegen as one product. |
| **C. Conditional exports** | One package. `react.web.ts` + `react.native.ts` + shared types in `react.ts`. `package.json` `exports` resolve via `"react-native"` condition. | **SELECTED** — emitter stays target-agnostic; each implementation is idiomatic per platform; matches ecosystem precedent (Tamagui, NativeWind, Sentry, react-native-web). |

### §3.1 Layout

The current `runtime/web/lazuli/src/` has two files that touch web-only APIs (`view-helpers.ts` uses `window.localStorage`/`window.addEventListener`; `react.ts` is otherwise universal). The split isolates the two non-portable hooks into per-platform files. Folder rename `runtime/web/lazuli/` → `runtime/ts/lazuli/` happens in Cell A.0 **before** the split so each file moves once, not twice.

```
runtime/ts/lazuli/                  # renamed from runtime/web/lazuli/ in Cell A.0
├── package.json                    # @lazuli/runtime; exports map updated (§3.2)
├── src/
│   ├── index.ts                    # transport-only public API — universal, unchanged
│   ├── client.ts                   # LazuliClient — universal (fetch is universal), unchanged
│   ├── error.ts                    # universal, unchanged
│   ├── spec.ts                     # universal, unchanged
│   ├── types.ts                    # universal, unchanged
│   ├── view-helpers.ts             # universal hooks ONLY: useFilterState, useMultiSelection, parseSegments, canonicalizeSearch + all related types. useLocalSetting + useDrawerSubView REMOVED from here.
│   ├── local-setting.web.ts        # NEW: useLocalSetting body (localStorage + useSyncExternalStore)
│   ├── local-setting.native.ts     # NEW: useLocalSetting body (AsyncStorage + useState/useEffect)
│   ├── drawer-sub-view.web.ts      # NEW: useDrawerSubView body (window.addEventListener("keydown"))
│   ├── drawer-sub-view.native.ts   # NEW: useDrawerSubView body (BackHandler)
│   ├── react.ts                    # PUBLIC TYPE CONTRACT — declares signatures; re-exports universal hooks from view-helpers.ts; type-only references to useLocalSetting + useDrawerSubView
│   ├── react.web.ts                # CONCRETE WEB ENTRYPOINT — re-exports types + universal hooks; binds local-setting.web.ts + drawer-sub-view.web.ts
│   └── react.native.ts             # CONCRETE NATIVE ENTRYPOINT — same shape, bound to .native.ts files
```

### §3.1.1 Per-symbol migration table

Every currently-exported symbol from `runtime/web/lazuli/src/react.ts` and `runtime/web/lazuli/src/view-helpers.ts` is enumerated below with its destination. Wave 1 cells (A.1–A.5) execute this exact mapping.

| Symbol | Today's source | After split — declaration | After split — web body | After split — native body | Universal? |
|---|---|---|---|---|---|
| `LazuliClient` | `client.ts` | `client.ts` | (no body change) | (no body change) | Yes |
| `LazuliProvider` | `react.ts` | `react.ts` (declaration) | `react.web.ts` (re-export from `react.ts`) | `react.native.ts` (re-export from `react.ts`) | Yes — uses `createContext`/`createElement` |
| `useLazuliClient` | `react.ts` | `react.ts` | re-export | re-export | Yes |
| `queryKeyFor` | `react.ts` | `react.ts` | re-export | re-export | Yes — pure function |
| `useLazuliQuery` | `react.ts` | `react.ts` | re-export | re-export | Yes — pure TanStack Query |
| `useLazuliCommand` | `react.ts` | `react.ts` | re-export | re-export | Yes |
| `UseLazuliQueryOptions` | `react.ts` | `react.ts` | (type-only) | (type-only) | Yes |
| `UseLazuliCommandOptions` | `react.ts` | `react.ts` | (type-only) | (type-only) | Yes |
| `useFilterState` + types | `view-helpers.ts` | `view-helpers.ts` | re-export from `view-helpers.ts` | re-export from `view-helpers.ts` | Yes — pure React |
| `useMultiSelection` + types | `view-helpers.ts` | `view-helpers.ts` | re-export | re-export | Yes — pure React |
| `parseSegments` + types | `view-helpers.ts` | `view-helpers.ts` | re-export | re-export | Yes — pure JS |
| `canonicalizeSearch` | `view-helpers.ts` | `view-helpers.ts` | re-export | re-export | Yes — pure JS |
| `FilterConfig`, `FilterState`, `MultiFilterState`, `FilterStates`, `MultiSelection`, `ParsedSegment`, `UrlParams`, `SetUrlParams` | `view-helpers.ts` | `view-helpers.ts` | re-export | re-export | Yes — types |
| **`useLocalSetting`** | `view-helpers.ts` | declaration in `react.ts`; **REMOVED** from `view-helpers.ts` | body in `local-setting.web.ts` | body in `local-setting.native.ts` | **No — platform-split** |
| **`useDrawerSubView`** | `view-helpers.ts` | declaration in `react.ts`; **REMOVED** from `view-helpers.ts` | body in `drawer-sub-view.web.ts` | body in `drawer-sub-view.native.ts` | **No — platform-split** |
| `DrawerConfig`, `DrawerSubView` (types) | `view-helpers.ts` | `view-helpers.ts` (types stay universal) | re-export | re-export | Yes — types |

After Cell A.5, `runtime/ts/lazuli/src/view-helpers.ts` no longer contains the two web-coupled bodies; it carries only universal hooks. The `view-helpers.test.ts` cases for `useLocalSetting` move to `local-setting.web.test.ts`; a sibling `local-setting.native.test.ts` is added in Cell A.2 with React Native Testing Library + jest-expo preset (Open Question §12.2 closed: native tests run via `jest-expo` because RN's official testing path is Jest; web tests stay on Vitest).

### §3.2 `package.json` exports

```json
{
  "name": "@lazuli/runtime",
  "exports": {
    ".": {
      "types": "./src/index.ts",
      "default": "./src/index.ts"
    },
    "./react": {
      "types": "./src/react.ts",
      "react-native": "./src/react.native.ts",
      "default": "./src/react.web.ts"
    }
  },
  "peerDependencies": {
    "@tanstack/react-query": ">=5.0.0",
    "react": ">=18.0.0",
    "react-native": ">=0.74.0",
    "@react-native-async-storage/async-storage": ">=1.23.0"
  },
  "peerDependenciesMeta": {
    "@tanstack/react-query": { "optional": true },
    "react": { "optional": true },
    "react-native": { "optional": true },
    "@react-native-async-storage/async-storage": { "optional": true }
  }
}
```

The `"react-native"` condition is what Metro looks for. Vite/webpack ignore it and fall through to `default`. TypeScript reads `types` regardless of bundler — `react.ts` contains the **type contract** that both `.web.ts` and `.native.ts` implement.

### §3.3 Type contract (`react.ts`)

```ts
// Type-only entrypoint. Declares the public surface of @lazuli/runtime/react.
// Both react.web.ts and react.native.ts re-export every name listed here.
import type { UseMutationOptions, UseMutationResult, UseQueryOptions, UseQueryResult } from "@tanstack/react-query";
import type { LazuliClient } from "./client.js";
import type { CommandSpec, QuerySpec } from "./spec.js";

export type UseLazuliQueryOptions<Args, Result> = …;
export type UseLazuliCommandOptions<Input, Output> = …;
export declare function useLazuliQuery<Args, Result>(spec: QuerySpec<Args, Result>, args: Args, options?: UseLazuliQueryOptions<Args, Result>): UseQueryResult<Result, Error>;
export declare function useLazuliCommand<Input, Output>(spec: CommandSpec<Input, Output>, options?: UseLazuliCommandOptions<Input, Output>): UseMutationResult<Output, Error, Input>;
export declare function useLazuliClient(override?: LazuliClient): LazuliClient;
export declare const LazuliProvider: React.ComponentType<{ client: LazuliClient; children: React.ReactNode }>;
export declare function useFilterState<T extends Record<string, FilterConfig<unknown>>>(config: T): FilterStates<T>;
export declare function useMultiSelection<TId>(items: { id: TId }[]): MultiSelection<TId>;
export declare function useLocalSetting<T>(key: string, defaultValue: T): [T, (next: T) => void];
export declare function useDrawerSubView(config: DrawerConfig): DrawerSubView;
export declare function parseSegments(input: string, keywords: readonly string[], alwaysArray: readonly string[]): readonly ParsedSegment[];
export declare function canonicalizeSearch(input: { /* … */ }): string;
// + every view-helper type from view-helpers.ts
```

`.web.ts` and `.native.ts` are required to export every name from `.ts`. Cell A.5 (`exports-parity.test.ts`) typechecks both files against `react.ts` via `tsc --noEmit` and asserts the export sets match.

### §3.4.1 Type contract for `useLocalSetting` — first-render divergence

Web's current `useLocalSetting` uses `useSyncExternalStore`, so reading the value on first render returns the **persisted** value (if present). Native's RN body resolves `AsyncStorage` asynchronously — first render returns `defaultValue` even when storage has the key; the persisted value becomes visible on the **next** render after the microtask resolves.

This is a contract divergence, not just an implementation detail. Resolution:

```ts
/**
 * Returns the persisted setting, falling back to `defaultValue`.
 *
 * IMPORTANT — first-render contract differs by platform:
 *   - Web: returns the persisted value synchronously on first render
 *     (uses useSyncExternalStore + localStorage).
 *   - Native: returns `defaultValue` on first render; the persisted
 *     value becomes visible on the next render after AsyncStorage
 *     resolves.
 *
 * Callers MUST treat the value as eventually-consistent. Code that
 * depends on the persisted value being present on the first frame
 * (e.g., to pick a theme before paint) must read storage explicitly
 * via the platform's lower-level API, not via this hook.
 */
export declare function useLocalSetting<T>(key: string, defaultValue: T): [T, (next: T) => void];
```

The JSDoc is the contract. A future polish item (`LOCAL-SETTING-CRITICAL-PATH`) could add a doctor lint that flags `useLocalSetting` usage in code paths annotated `// critical:first-paint` — deferred to a separate cell because it requires AST analysis of user code, which Lazuli doesn't do today.

A more aggressive alternative — change the return type to `[T, (next: T) => void, { isLoaded: boolean }]` and propagate to every caller — is rejected because it forces a breaking change on web consumers for a divergence that's documented and easy to avoid. The polish item gates on a real bug.

### §3.5 Typecheck-only consumers (no RN install)

`@lazuli/runtime` declares both `react-native` and `@react-native-async-storage/async-storage` as **optional peers**. A web-only consumer that runs `tsc --noEmit` against generated TS without installing the RN packages should not fail.

Two-part mechanism:

1. **`react.ts` (the type contract) references RN types via type-only imports.** Example: `import type { Platform } from "react-native"` is used **only** at type position. If `react-native` is absent, `tsc` reports the missing module — that's a hard fail today. Fix: declare the few RN types Lazuli touches (currently zero in `react.ts`; potentially `AsyncStorageStatic` from `@react-native-async-storage/async-storage` inside `local-setting.native.ts`) as **inline type aliases** in `react.ts`. The `.native.ts` body imports the real RN module and adapts at call site.

2. **`react.web.ts` does not reference any RN module, even at type position.** Web consumers running `tsc` see only DOM types + `@tanstack/react-query` + React. The `package.json` `exports` map serves `react.web.ts` via the `default` condition; `tsc` (which respects exports conditions in moduleResolution `node16`/`nodenext`) reads `react.web.ts` and stays happy.

3. **Native consumers** install RN + AsyncStorage as direct deps (Expo manages this). `tsc` resolves the `react-native` condition to `react.native.ts`, which imports the real modules.

The exports map's `types` branch points to `react.ts`. To prevent `tsc` from following the `types` path to a file referencing RN types it can't resolve, **`react.ts` must be type-only and must not import RN modules directly.** Cell A.3 enforces this with a lint test (`exports-parity.test.ts`) that compiles `react.ts` in a web-only `tsconfig` and asserts zero errors.

### §3.4 Why generated code needs no new emitter work

The per-view emitter is **already target-aware** today. `crates/lazuli_codegen_ts/src/lzx_router_adapter.rs:35-45` switches `useParams` imports across four targets:

```rust
match target {
    RouterTarget::ViteReact | RouterTarget::Tauri =>
        "import { useParams } from \"@tanstack/react-router\";\n",
    RouterTarget::NextJs =>
        "import { useParams } from \"next/navigation\";\n",
    RouterTarget::Expo =>
        "import { useLocalSearchParams as useParams } from \"expo-router\";\n",
}
```

The walker at `crates/lazuli_cli/src/main.rs:1044-1058` already routes `SurfaceTarget::Mobile` to `RouterTarget::Expo`. Every emission that calls `useParams` (currently just `lzx_view_detail`) gets the right import today.

The constant `@lazuli/runtime/react` in the runtime hook imports (`useLazuliQuery`, `useLazuliCommand`, etc.) is the **only** import this proposal needs to make target-resolved — and it does so at the **package boundary** via `package.json exports` conditions, not by adding emitter branches. The emitter keeps emitting one string; Metro vs Vite resolve it differently.

This is the "abstraction is wire" win restated: the framework moves where it can be cheap (in the package boundary, where ecosystem tooling does the work), not where it would be expensive (in the codegen, where every emitter would have to learn an additional target dimension).

---

## §4. Native hook counterparts

Two hooks need real native bodies; the rest are universal (React stdlib only).

### §4.1 `useLocalSetting` (RN)

```ts
// react.native.ts excerpt
import AsyncStorage from "@react-native-async-storage/async-storage";
import { useCallback, useEffect, useRef, useState } from "react";

export function useLocalSetting<T>(key: string, defaultValue: T): [T, (next: T) => void] {
  const [value, setValue] = useState<T>(defaultValue);
  const loaded = useRef(false);

  useEffect(() => {
    let cancelled = false;
    AsyncStorage.getItem(key).then((raw) => {
      if (cancelled || raw === null) return;
      try { setValue(JSON.parse(raw) as T); } catch { /* keep default */ }
      loaded.current = true;
    });
    return () => { cancelled = true; };
  }, [key]);

  const update = useCallback((next: T) => {
    setValue(next);
    AsyncStorage.setItem(key, JSON.stringify(next)).catch(() => { /* surface via console in dev */ });
  }, [key]);

  return [value, update];
}
```

Behavior differences from the web version, made explicit:

| Property | Web | Native |
|---|---|---|
| Storage backend | `localStorage` (synchronous) | `AsyncStorage` (asynchronous) |
| Initial value | Read synchronously on first render | Read asynchronously; first render returns `defaultValue` |
| Cross-tab sync | `storage` event | N/A (no multi-tab on RN) |
| SSR safety | `typeof window === "undefined"` guard | N/A (no SSR for RN) |

The async-on-first-render difference is the only behavior gap. Documented in `docs/runtime-handoff.md` (added by §11 Cell C.4).

### §4.2 `useDrawerSubView` (RN)

```ts
// react.native.ts excerpt
import { BackHandler } from "react-native";
import { useCallback, useEffect, useState } from "react";

export function useDrawerSubView(config: DrawerConfig): DrawerSubView {
  const [id, setId] = useState<string | null>(null);
  const open = useCallback((next: string) => setId(next), []);
  const close = useCallback(() => setId(null), []);

  // Close-on-disappear from the source query.
  useEffect(() => {
    if (id !== null && config.itemMissing) setId(null);
  }, [id, config.itemMissing]);

  // Android back button closes the drawer.
  useEffect(() => {
    if (id === null) return;
    const handler = BackHandler.addEventListener("hardwareBackPress", () => {
      setId(null);
      return true; // consume the event
    });
    return () => handler.remove();
  }, [id]);

  return {
    isOpen: id !== null,
    id,
    item: id !== null ? (config.item ?? null) : null,
    open,
    close,
  };
}
```

iOS swipe-back gesture: handled by Expo Router automatically when the drawer is a navigated stack screen. When the drawer is a `<Modal>` (sheet variant), the consumer wires `onRequestClose` to `close()` — that's user-authored View territory.

### §4.3 Universal hooks (no split needed)

`useLazuliQuery`, `useLazuliCommand`, `useLazuliClient`, `LazuliProvider`, `useFilterState`, `useMultiSelection`, `parseSegments`, `canonicalizeSearch` — all pure React + TanStack Query + JS. They live in `view-helpers.ts` and `react.ts` and are re-exported identically from `.web.ts` and `.native.ts`.

### §4.4 Why not `Platform.OS` in `view-helpers.ts`?

Considered: keep `view-helpers.ts` single-file, branch on `Platform.OS` inside `useLocalSetting`. Rejected because:

1. **Bundle pollution**: web bundles would import `react-native`'s `Platform` (~3KB minified gzipped + transitive deps), purely to never execute that branch.
2. **Type pollution**: `AsyncStorage` types leak into web's TypeScript checking, requiring `@types/react-native` in the web `package.json`'s `devDependencies`.
3. **Hidden behavior**: a developer reading `view-helpers.ts` sees three different storage strategies braided together. Conditional exports keep each implementation locally readable.

The conditional-exports approach is more code (two files instead of one) and **less mechanism** (no runtime branching, no platform-detection imports). Founding principle alignment: wire, not mechanism.

---

## §5. Expo Router file-based scaffold

Expo Router uses file-based routing under an `app/` directory. The current scaffold (`templates::FRONTEND_MOBILE_ROOT_TSX`) creates `shell/root.tsx` with `registerRootComponent` and a placeholder — that path doesn't wire any routes.

### §5.1 New scaffold shape — territory boundary preserved

The proposal preserves the existing **two-class boundary**:
- `dist/` is regen-only, never user-edited.
- `frontends/`, `features/`, `cells/` are user-owned. Lazuli may scaffold **once** here, then never overwrites.

Expo Router scans `app/` (which lives under `frontends/mobile/`) for routes, so `_layout.tsx` cannot live under `dist/` directly. The solution is a **one-line re-export**: the regenerated body lives in `dist/ts-mobile/runtime/layout.tsx`; the user-owned `frontends/mobile/app/_layout.tsx` is a one-line wrapper that re-exports it. User-owned wrapper IS replaceable (e.g., user wants extra providers); the regen file is always the canonical body.

```
frontends/mobile/                                # USER-OWNED (scaffolded once, never overwritten)
├── app/
│   ├── _layout.tsx                              # one-liner: export { default } from "@/dist/ts-mobile/runtime/layout"
│   └── index.tsx                                # placeholder home (lists registered surfaces); user replaces
├── shell/
│   └── client.ts                                # LazuliClient construction (baseUrl from env); user owns
├── babel.config.js                              # expo-router preset
├── app.json                                     # Expo manifest (name, slug, scheme, splash, icon)
├── metro.config.js                              # default Expo Metro config
├── tsconfig.json                                # extends expo's; paths to dist/ts-mobile
├── package.json                                 # Expo SDK pin, react-native, expo-router, AsyncStorage, runtime peer
└── .gitignore                                   # .expo/, node_modules/, ios/, android/

dist/ts-mobile/                                  # REGEN-ONLY (rewritten every `lazuli generate ts`)
├── runtime/
│   └── layout.tsx                               # NEW: canonical Stack root + LazuliProvider + QueryClientProvider body (§5.4)
├── <feature>/<feature>.gen.ts                   # SDK (audience-filtered)
├── <feature>/<feature>.zod.ts                   # Zod schemas
├── <feature>/views/<audience>/<view>.gen.ts     # per-view hook
└── <feature>/cells/<slot>.gen.ts                # slot interface
```

The user's `frontends/mobile/app/_layout.tsx` is **user-replaceable**: if a project wants to inject an extra provider (e.g., a sentry transport, a theme provider), they replace the one-liner with their own JSX wrapping `<DistLayout>`. The boundary is now clean: `dist/` is regen; `frontends/` is user-owned end-to-end. No `MOBILE-LAYOUT-MUTATED` lint needed (dropped from §9).

### §5.2 Per-view scaffold generation

`lazuli generate ts` reads `[frontends.mobile] audiences = [...]` and **once** materializes a `frontends/mobile/app/<audience>/<view-route-path>.tsx` per (audience, view) tuple where the view declares `at "<route>"`. The file is **scaffolded once** (idempotent — never overwrites user edits, same contract as L0 #1 §6.1 `cmd_new_frontends`).

Example: a `.mobile.lzx` declaring

```lzx
surface customer mobile
  uses feature customer

  audience sales
    view list customer_list at "/customers"
      source customer.query.list
      fields name, status, tier

    view detail customer_detail at "/customers/:id"
      source customer.query.by_id
      route id: ID from path
      sections header, timeline
```

scaffolds:

```
frontends/mobile/app/sales/
├── customers/
│   ├── index.tsx                # consumes useSalesCustomerListView (scaffolded once)
│   └── [id].tsx                 # consumes useSalesCustomerDetailView (scaffolded once)
```

with bodies pre-wired to the generated hooks. Author replaces the placeholder JSX with real RN components. Second `lazuli generate ts` does not touch these files (`write_if_absent` guard).

### §5.3 Why scaffold-not-codegen for the View

The scaffold is a one-shot **starter** — same pattern as `lazuli generate feature` per `cmd_generate_feature.rs`. Reasons:

1. Honors §4 of L0 #3: "View owns JSX, layout, component library choice."
2. RN component libraries vary wildly (RN core, Tamagui, NativeWind, Gluestack, Restyle). No one-size-fits-all generated JSX.
3. Avoids the Aerocoding negative reference: template-driven full codegen of view bodies grew unbounded.

A future `@plugin/scaffold-tamagui-rn` (post-pilot) re-runs the scaffold with a different starter body — same mechanism, different template body.

### §5.4 Layout — split between regen body and user-owned wrapper

**Regen-only body** at `dist/ts-mobile/runtime/layout.tsx`:

```tsx
// Code generated by lazuli; DO NOT EDIT.
import { Stack } from "expo-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { LazuliProvider } from "@lazuli/runtime/react";
import { client } from "../../../frontends/mobile/shell/client";

const queryClient = new QueryClient();

export default function RootLayout() {
  return (
    <LazuliProvider client={client}>
      <QueryClientProvider client={queryClient}>
        <Stack />
      </QueryClientProvider>
    </LazuliProvider>
  );
}
```

**User-owned wrapper** at `frontends/mobile/app/_layout.tsx` (scaffolded **once**, never overwritten by Lazuli):

```tsx
// Replace with your own wrapper if you need extra providers.
// `lazuli generate ts` will not overwrite this file.
export { default } from "@/dist/ts-mobile/runtime/layout";
```

Customization paths, from least to most invasive:

1. **No customization:** keep the one-liner; let Lazuli update the body.
2. **Extra providers around the Lazuli stack:** replace the wrapper with custom JSX that imports the regen body as a child component.
3. **Replace the runtime layout entirely:** delete the re-export, author the full layout. User opts out of regen for this concern; downstream changes to `LazuliProvider`/`QueryClientProvider` wiring are theirs to absorb.

The boundary is clean: `dist/` is regen, `frontends/` is user-owned. No new territory class. No `MOBILE-LAYOUT-MUTATED` doctor rule needed.

---

## §6. `Lazurite.toml [frontends.mobile]` schema

Today `[frontends.mobile]` accepts `target` / `out` / `audiences` (per `lazurite_manifest::Frontend`). This proposal adds keys **only when Lazuli does something with them.**

```toml
[frontends.mobile]
target = "expo"
source = "frontends/mobile"
out = "dist/ts-mobile"
audiences = ["sales", "buyer"]
```

That's the whole schema. No new keys. Reasoning per key considered and rejected:

- **`expo_sdk`** — rejected. `package.json` is the source of truth for the SDK version. A manifest entry that "is informational only" is mechanism without semantics (Rule Zero). If a drift-detection rule lands later, it'd read `package.json` directly.
- **`scheme`, `app_name`, `icon`, `splash`** — rejected. These belong in `app.json` (Expo's manifest), not Lazuli's. Lazuli does not codegen `app.json`; it's a user-owned scaffold artifact written once. Forwarding through `Lazurite.toml` would be passthrough mechanism that earns nothing.
- **`[generate.ts.mobile]`** — rejected as a separate section. The existing `[generate.ts]` block (from L0 #3 §9.1) already applies to both targets; if mobile-specific overrides emerge, add per-target keys to `[generate.ts]` then.

### §6.1 Strict-mode follow-up

`MANIFEST-STRICT-FRONTEND` (reject unknown keys in `[frontends.<x>]`) is tracked as a separate polish cell in §11. Not blocking this proposal.

---

## §7. Per-target SDK emission audit

The walker at [crates/lazuli_cli/src/main.rs:1062-1093](crates/lazuli_cli/src/main.rs#L1062-L1093) already enumerates targets correctly. But `emit_feature_sdk_ts` (called for both `ts-web` and `ts-mobile`) emits **byte-identical** files — that's correct for now (the SDK is platform-neutral) but worth pinning as an invariant:

> **Invariant MOBILE-SDK-PARITY:** `dist/ts-mobile/<feat>/<feat>.gen.ts` must be byte-identical to `dist/ts-web/<feat>/<feat>.gen.ts` for any (audience, feature) tuple where both targets carry the same audience set. Audience-scoping per L0 #3 §7 is the only legitimate divergence.

A test in `crates/lazuli_codegen_ts/tests/parity.rs` enumerates the full-capsule fixture, calls `emit_feature_sdk_ts` for both targets, and asserts equality modulo audience filtering.

The view hooks **do** legitimately diverge per target (router-adapter import line, route-path translation). That's the contract — the view file is the per-target seam, the SDK file is the universal seam.

---

## §8. Hostpoint mobile fixture — `examples/marketplace-mini-mobile/`

The current `examples/marketplace-mini/` is the canonical generic fixture (memory `project_public_vs_private_repo`). It's web-only today. Mobile gets a sibling fixture rather than a `.mobile.lzx` add-on, because the user audiences and command surface diverge between sales-staff (web admin) and buyer-on-mobile.

```
examples/marketplace-mini-mobile/
├── Lazurite.toml                # [frontends.mobile] target=expo, audiences=["buyer"]
├── app.lzi
├── registry.lzi
├── features/
│   ├── catalog/
│   │   ├── catalog.lzi          # listings, listing query, lookup-by-id
│   │   └── catalog.mobile.lzx   # buyer audience, list + detail views
│   └── booking/
│       ├── booking.lzi          # create-booking command
│       └── booking.mobile.lzx   # create view
└── frontends/
    └── mobile/                  # scaffolded once, committed
        ├── app/
        │   └── _layout.tsx
        ├── shell/{root.tsx,client.ts}
        ├── app.json
        ├── babel.config.js
        ├── metro.config.js
        ├── tsconfig.json
        └── package.json
```

### §8.1 Smoke test

A new integration test in `crates/lazuli_cli/tests/mobile_smoke.rs`:

1. Run `lazuli generate ts` against the fixture.
2. Assert exact set of emitted paths under `dist/ts-mobile/` (3 view files + 2 SDK files + 2 Zod files + 2 cell interface files + 1 `runtime/layout.tsx`).
3. Assert `dist/ts-mobile/runtime/layout.tsx` body matches the canonical template byte-for-byte.
4. Assert `frontends/mobile/app/_layout.tsx` is the canonical one-line re-export (when fresh-scaffolded).
5. Assert scaffolded `frontends/mobile/app/buyer/listings/index.tsx` exists with the right hook import line.
6. **Typecheck pass.** Shell out to `pnpm` + `tsc --noEmit` against the fixture's `tsconfig.json` (which extends Expo's). Asserts zero TS errors over the emitted `dist/ts-mobile/` + scaffolded `frontends/mobile/`. Cell E.2 owns this.

**CI gating mechanism — adopt the existing pattern** from `crates/lazuli_codegen_ts/tests/smoke_ts_typecheck.rs:1,40-43`:

```rust
#[cfg(feature = "smoke")]
mod mobile_smoke {
    use std::env;

    #[test]
    fn marketplace_mini_mobile_typechecks_under_tsc() {
        if env::var("LAZULI_TS_SMOKE").ok().as_deref() != Some("1") {
            eprintln!("skipping mobile smoke; set LAZULI_TS_SMOKE=1 to run");
            return;
        }
        // ... pnpm install + tsc --noEmit on the fixture's tsconfig ...
    }
}
```

Two gates: (1) Cargo feature `smoke` (already declared in `lazuli_codegen_ts`; Cell E.2 mirrors it for `lazuli_cli`), and (2) runtime env-var `LAZULI_TS_SMOKE=1`. Both must be set; CI workflow `.github/workflows/smoke.yml` (or equivalent) opts in. Default `cargo test` is unaffected. The repo's smoke convention is already proven by `full_capsule_typechecks_under_tsc`; the mobile test is one more in the same shape.

Reasons we chose this over alternatives:
- A Rust-to-TS binding stub adds a build-time dependency on `tsserver` Rust bindings, none of which are mature.
- The runtime test infra already has Node (Vitest tests `runtime/ts/lazuli/`). Adding `pnpm install` for one fixture is incremental.
- "Compiles to RN" is the proposal's headline; verification has to actually compile.

No live `expo start` in CI — that needs Node + Metro + a simulator + a long-lived process. Tracked in §11 as a follow-up manual-smoke artifact under §10.4.

### §8.2 Why a separate fixture, not a `.mobile.lzx` add-on to `marketplace-mini`

1. **Audience independence:** the mobile buyer audience has different policy gates than the web seller audience. Mixing them in one fixture obscures the audience-scoping mechanism.
2. **Boundary clarity:** the fixture is a single-purpose evidence artifact. "What does a mobile-target Lazuli project look like?" should map to one directory.
3. **Hostpoint pilot tracking:** Hostpoint's port plan references this fixture as the reference shape. A separate fixture is a stable target for the pilot to compare against.

The web-only `marketplace-mini` remains the canonical web fixture. Both are kept in sync via shared `.lzi` modules where the contract overlaps (a future polish item).

---

## §9. Doctor rules

Existing `lzx-*` rules + `cell-missing-impl` + `cell-prop-mismatch` all apply to mobile surfaces unchanged — they're target-agnostic. Two **generalized** rules ship:

| Code | Trigger | Severity | Resolution |
|---|---|---|---|
| `lzx-cell-missing-impl` *(FIRST IMPL)* | `cells … @client.<slot>` references a slot but `features/<feat>/<target>/cells/<slot>.tsx` does not exist (`<target>` is `web` or `mobile`, derived from the enclosing surface's target). The rule was announced in L0 #1 §5.2 but never shipped; this proposal lands it for the first time, target-aware from day 1. Distinct from the existing `lzx-cell-slot-orphan` (which fires when a cell binds to a field absent from `columns`/`fields`/`sections`). | error | Author the slot impl under the correct platform subdirectory, OR remove the cell binding from the surface. |
| `lzx-route-collision` *(NEW)* | Two views in the same (audience, target) tuple emit the same router-translated path. E.g., authored routes `at "/users/:id"` and `at "/users/:user_id"` both translate to `/users/[id]` under Expo, even though they're distinct under TanStack Router (`$id` vs `$user_id`). | error | Rename one of the route placeholders so they translate to distinct router paths. |

**Both rules follow the existing catalog style** (`crates/lazuli_cli/src/doctor/lzx/<rule>.rs` per current convention) with `code`, `trigger`, `severity`, `fix`, and an `example .lzx snippet` showing both the violation and the corrected form. Catalog entries are added to `docs/error-contract.md` alongside the other `lzx-*` rules.

### §9.1 Example — `cell-missing-impl` (extended)

**Violation** — `examples/marketplace-mini-mobile/features/catalog/catalog.mobile.lzx`:

```lzx
surface catalog mobile
  uses feature catalog

  audience buyer
    view list listings
      source catalog.query.list
      fields title, price, badge
      cells badge @client.price_badge   # ← Doctor: cell-missing-impl
```

Doctor implementation lands in `crates/lazuli_cli/src/doctor/lzx/cell_missing_impl.rs` (sibling of the existing `cell_slot_orphan.rs`). Doctor message:

```
error[lzx-cell-missing-impl]: slot `@client.price_badge` referenced from mobile surface, but features/catalog/mobile/cells/price_badge.tsx does not exist.

   --> examples/marketplace-mini-mobile/features/catalog/catalog.mobile.lzx:8:14
8 |       cells badge @client.price_badge
                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Hint: features/catalog/web/cells/price_badge.tsx exists, but mobile surfaces need their own implementation. Author features/catalog/mobile/cells/price_badge.tsx, OR remove the cell binding from this view.
```

**Fix:**

```lzx
# Option A: author the mobile cell impl at features/catalog/mobile/cells/price_badge.tsx
# Option B: remove the cell binding
  cells badge @client.price_badge      # ← deleted
```

### §9.2 Example — `lzx-route-collision`

**Violation** — same surface declares two views whose routes collide under Expo:

```lzx
audience buyer
  view detail listing_detail
    at "/listings/:id"
    source catalog.query.by_id
    route id: ID from path

  view detail listing_by_slug
    at "/listings/:slug"               # ← Doctor: lzx-route-collision
    source catalog.query.by_slug
    route slug: Text from path
```

Doctor message:

```
error[lzx-route-collision]: routes `/listings/:id` and `/listings/:slug` both translate to `/listings/[id]` (or `/listings/[slug]`, depending on declaration order) under Expo Router, producing a file-system collision.

   --> examples/marketplace-mini-mobile/features/catalog/catalog.mobile.lzx:14:8
14 |     at "/listings/:slug"
            ^^^^^^^^^^^^^^^^

Hint: Expo Router maps single-segment dynamic params to `[name].tsx`. Two views in the same audience with single-param dynamic segments at the same path depth produce identical file paths. Disambiguate by making one route deeper, e.g., `/listings/by-slug/:slug`.
```

### §9.3 Rules NOT added

Earlier drafts proposed `MOBILE-LAYOUT-MUTATED` (resolved by §5.4's two-territory split — no rule needed) and `MOBILE-EXPO-SDK-DRIFT` (resolved by §6's drop of `expo_sdk` — no key to drift from). Both dropped.

---

## §10. Examples — Hostpoint-shaped

### §10.1 Hostpoint `booking.mobile.lzx`

```lzx
surface booking mobile
  uses feature booking

  audience buyer
    requires @scope.tenant_member

    view list my_bookings at "/bookings"
      source booking.query.mine
      fields listing_title, status, check_in, check_out
      actions cancel

    view detail booking_detail at "/bookings/:id"
      source booking.query.by_id
      route id: ID from path
      sections header, dates, host_contact, cancellation
      actions cancel, contact_host

    view create new_booking at "/bookings/new"
      submit booking.command.create
      fields listing_id, check_in, check_out, guests
```

### §10.2 Generated files

```
dist/ts-mobile/booking/
├── booking.gen.ts                          # filtered to buyer audience: createBooking, listMineBookings, lookupBookingById, cancelBooking, contactHost
├── booking.zod.ts                          # createBookingInputSchema + cancelBookingInputSchema + contactHostInputSchema
└── views/buyer/
    ├── my_bookings.gen.ts                  # useBuyerMyBookingsView (TanStack Query bundle, no JSX)
    ├── booking_detail.gen.ts               # useBuyerBookingDetailView with Expo Router useLocalSearchParams
    └── new_booking.gen.ts                  # useBuyerNewBookingView with RHF + zodResolver

frontends/mobile/app/buyer/
├── bookings/
│   ├── index.tsx                           # SCAFFOLDED ONCE; consumes useBuyerMyBookingsView
│   ├── [id].tsx                            # SCAFFOLDED ONCE; consumes useBuyerBookingDetailView
│   └── new.tsx                             # SCAFFOLDED ONCE; consumes useBuyerNewBookingView
└── _layout.tsx                             # REGEN-ONLY; LazuliProvider + Stack
```

### §10.3 Scaffolded `app/buyer/bookings/index.tsx`

```tsx
import { FlatList, Text, View } from "react-native";
import { useBuyerMyBookingsView } from "@/dist/ts-mobile/booking/views/buyer/my_bookings.gen";

export default function MyBookingsScreen() {
  const { query, actions } = useBuyerMyBookingsView();

  if (query.isLoading) return <Text>Loading…</Text>;
  if (query.error) return <Text>Error: {query.error.message}</Text>;

  return (
    <FlatList
      data={query.data ?? []}
      keyExtractor={(item) => item.id}
      renderItem={({ item }) => (
        <View style={{ padding: 16 }}>
          <Text>{item.listing_title}</Text>
          <Text>{item.status}</Text>
        </View>
      )}
    />
  );
}
```

Headless v0: this is raw RN. User replaces `<FlatList>` with Tamagui / Gluestack / native components as they prefer.

### §10.4 Manual-smoke artifact (Wave 4 deliverable)

After all 4 waves merge, an operator with Node + Xcode/Android Studio installed runs:

```bash
git clone <repo> && cd lazuli
cargo build --release -p lazuli_cli
cp target/release/lazuli ~/.local/bin/

cd examples/marketplace-mini-mobile
lazuli generate ts                                  # emits dist/ts-mobile/
pnpm install                                        # resolves expo, react-native, @lazuli/runtime via workspace
pnpm exec expo start                                # opens Metro bundler
# Press 'i' for iOS simulator or 'a' for Android emulator
```

**Expected device output:**

1. App launches; `<Stack />` from Expo Router mounts.
2. Buyer audience landing screen (`app/buyer/listings/index.tsx`) renders, fires `useBuyerListingsListView` → `LazuliClient.runQuery(listMineListings, {})` → POST `http://localhost:8080/api/v1/q/catalog.list`.
3. Empty state shows (because no backend running in this fixture) — confirms the request path.
4. Tapping a placeholder list item navigates to `app/buyer/listings/[id].tsx`; Expo Router params resolve; view consumer is wired.
5. Creating a booking via `app/buyer/bookings/new.tsx` exercises RHF + zodResolver pre-binding.

This is a one-time manual verification step. It is **not** automated in CI. It IS documented in `docs/runtime-handoff.md` (Cell F.1) as the canonical proof-of-life procedure for Hostpoint mobile and any future RN consumer.

---

## §11. Implementation cells (post-PASS)

### §11.0 Cell A.0 — folder rename precedes split

The first cell of Wave 1 is a **pure rename**: `runtime/web/lazuli/` → `runtime/ts/lazuli/`. Every downstream cell touches the renamed path. Doing the rename first means each file moves exactly once. Downstream projects (Pleiades, Hostpoint, Erudito, Atelier) that pin `dev_replace` paths under `runtime/web/lazuli/` get a coordinated update in the same commit — they're all in private workspaces, but the public template + `Lazurite.toml`'s `dev_replace` field is updated to match. The rename does NOT change the package name `@lazuli/runtime` (already correct).

### §11.1 Cell table

| Cell | Owner | Scope | Risk |
|---|---|---|---|
| **A.0** | Claude (orchestrator) | Rename `runtime/web/lazuli/` → `runtime/ts/lazuli/`. Update every `dev_replace`, every doc reference, every CI workflow path. Single commit. | Low (mechanical; widely scoped) |
| **A.1** | Codex | Extract `useLocalSetting` from `view-helpers.ts` into `local-setting.web.ts` (web body — unchanged) and `local-setting.native.ts` (RN body per §4.1). Move existing `view-helpers.test.ts` cases to `local-setting.web.test.ts`. | Medium (AsyncStorage signature, async-on-first-render semantic) |
| **A.2** | Codex | Extract `useDrawerSubView` from `view-helpers.ts` into `drawer-sub-view.web.ts` and `drawer-sub-view.native.ts` (per §4.2). | Low (BackHandler is a documented RN API) |
| **A.3** | Claude | Author the type-only `react.ts` (per §3.3) declaring every public symbol. Confirm zero RN imports (per §3.5 part 1). | Medium (type contract surface) |
| **A.4** | Codex | Author `react.web.ts` and `react.native.ts` as concrete entrypoints re-exporting universal hooks + binding the split bodies. Both files must export every name from `react.ts` (per §3.3). | Low (deterministic re-export wiring) |
| **A.5** | Codex | `runtime/ts/lazuli/tests/exports-parity.test.ts` — typecheck both entrypoints against `react.ts`. Runs via existing Vitest infra. | Medium (TS compiler invocation from Vitest) |
| **A.6** | Claude | `runtime/ts/lazuli/package.json` exports map + peerDeps + peerDepsMeta (§3.2). | Low |
| **B.1** | Codex | `crates/lazuli_cli/src/templates.rs` — replace `FRONTEND_MOBILE_ROOT_TSX` body to re-export `dist/ts-mobile/runtime/layout`; add 5 new templates (`FRONTEND_MOBILE_APP_INDEX_TSX`, `FRONTEND_MOBILE_APP_JSON`, `FRONTEND_MOBILE_BABEL_CONFIG`, `FRONTEND_MOBILE_METRO_CONFIG`, `FRONTEND_MOBILE_TSCONFIG`); update `FRONTEND_MOBILE_PACKAGE_JSON` (add expo-router + AsyncStorage deps). | Low (deterministic template work) |
| **B.2** | Claude (sequenced after B.1) | `crates/lazuli_cli/src/cmd_new_frontends.rs::scaffold_frontend_mobile` writes the new files; idempotency tests for every new file. **Sequenced after B.1 because both touch `templates.rs`'s pub const surface.** | Low |
| **C.1** | Claude | `crates/lazuli_codegen_ts/src/` — add a per-feature emitter for `dist/ts-mobile/runtime/layout.tsx` (the regen body). Wire into `emit_feature_ts_artifacts` walker. | Medium (single-file emission, deterministic body) |
| **C.2** | Claude | `crates/lazuli_cli/src/main.rs::generate_ts` — scaffold per-view route file in `frontends/mobile/app/<audience>/<route>.tsx` for each mobile surface view, idempotent (`write_if_absent`). Route-path → file-path translation reuses `lzx_router_adapter::translate_route_path(RouterTarget::Expo, route)`. | Medium (route-path → Expo file-path mapping) |
| **C.3** | Codex | Three Expo-Router scaffold body templates (one per view kind: list, detail, create). Plain RN — no Tamagui/NativeWind opinion per §2 non-goals. | Low |
| **D.1** | Codex | Land `lzx-cell-missing-impl` for the first time in a new module `crates/lazuli_cli/src/doctor/lzx/cell_missing_impl.rs` (sibling of `cell_slot_orphan.rs`). Target-aware from day 1: walks the surface's `target` and checks for the slot `.tsx` under `features/<feat>/web/cells/` or `features/<feat>/mobile/cells/`. The rule was announced in L0 #1 §5.2 but never shipped — this cell ships it. Distinct from `cell_slot_orphan`; both rules coexist (the orphan rule fires on `field-not-in-columns`, the missing-impl rule fires on `slot-not-on-disk`). | Low |
| **D.2** | Codex | New doctor rule `lzx-route-collision` in `crates/lazuli_cli/src/doctor/lzx/` per §9.2. Per-target router translation; emits when collisions occur. | Low |
| **D.3** | Claude | `crates/lazuli_codegen_ts/tests/parity.rs` — SDK byte-identity invariant per §7 MOBILE-SDK-PARITY. | Low |
| **E.1** | Claude | Create `examples/marketplace-mini-mobile/` fixture per §8. `.lzi` + `.mobile.lzx` + scaffolded `frontends/mobile/`. | Medium (full-stack fixture, must doctor-green) |
| **E.2** | Claude | `crates/lazuli_cli/tests/mobile_smoke.rs` integration test (§8.1) — path assertions + `tsc --noEmit` via Node toolchain. Gated via `#[cfg(feature = "smoke")]` + `LAZULI_TS_SMOKE=1` env var, mirroring the existing convention at `crates/lazuli_codegen_ts/tests/smoke_ts_typecheck.rs`. Adds a `smoke` feature to `lazuli_cli`'s `Cargo.toml`; updates `.github/workflows/smoke.yml` if present, else documents the manual invocation. Default `cargo test` unaffected. | Medium (CI workflow touch) |
| **F.1** | Claude | `docs/runtime-handoff.md` update: §3 runtime split contract + §4 per-hook divergence + §10.4 manual-smoke procedure. Normative-only per `feedback_normative_not_narrative_2026-05-15`. | Low |
| **F.2** | Claude | `docs/error-contract.md` catalog entries for `lzx-route-collision` + updated `cell-missing-impl`. Update `docs/target-stack.md` mobile section to ref this proposal. Update `docs/architecture.md` glossary if needed. | Low |
| **G** (polish, deferred) | — | `MANIFEST-STRICT-FRONTEND` doctor rule (§6.1 follow-up). Tracked separately; not part of this proposal. | — |
| **H** (polish, deferred) | — | `LOCAL-SETTING-CRITICAL-PATH` doctor lint (§3.4.1 follow-up). Tracked separately. | — |

### §11.2 Wave layout

Per `feedback_wave_workflow_lucas_preferred.md` (Claude orchestrates; Codex executes mechanical L2; up to 5 Codex agents in parallel; never Codex on shared files).

- **Wave 1 — Runtime split** (sequencing strict): A.0 (rename, blocks everything; Claude) → A.1 + A.2 in parallel (Codex; each touches its own new file pair) → A.3 (Claude; type contract) → A.4 in parallel with A.5 (Codex on `react.{web,native}.ts`; Codex on tests) → A.6 (Claude on `package.json`). **6 cells total; ~half-day with parallel slots.**
- **Wave 2 — Scaffold + manifest**: B.1 → B.2 (sequenced; both touch `templates.rs` then `cmd_new_frontends.rs`). **2 cells.**
- **Wave 3 — Codegen + doctor**: C.1 + C.3 in parallel (Codex on layout emitter; Codex on view templates) → C.2 (Claude; orchestrator scaffold walker) → D.1 + D.2 + D.3 in parallel (Codex/Codex/Claude). **6 cells.**
- **Wave 4 — Fixture + smoke + docs**: E.1 (Claude) → E.2 (Claude; depends on E.1) → F.1 + F.2 in parallel (Claude × 2 doc files; OK to parallel as different files). **4 cells.**

**Total: 18 cells across 4 waves.** Critical path: A.0 → A.3 → C.2 → E.1 → E.2. Approximately 2 days of orchestrator-bound work + ~5 Codex hours.

### §11.3 Shared-file conflict avoidance

Per `CLAUDE.md` §"Division of labor": Codex agents must not touch shared files (`templates.rs`, `mod.rs`, `main.rs`, etc.) in parallel. The wave layout above enforces this via sequencing:

- `templates.rs` is touched by B.1 only (sequenced before B.2 which depends on its exports).
- `main.rs` `generate_ts` is touched by C.2 only (sequenced; Claude-owned).
- `cmd_new_frontends.rs` is touched by B.2 only.
- Codex-parallel cells (A.1, A.2, A.4, A.5, C.1, C.3, D.1, D.2) each own a distinct new file or a leaf-rule file; no shared state.

---

## §12. Open questions

1. **Mobile-specific design tokens?** Current `dist/ts-web/design/` emits Tailwind + CSS variables. Mobile equivalent is StyleSheet / NativeWind / Tamagui themes. **Resolution:** defer to a `docs/proposals/design-tokens-native.md` once a mobile pilot actually needs themed tokens. The pure-color-and-spacing emission today is portable; users can hand-author a `theme.ts` consuming `dist/ts-mobile/design/tokens.ts` (the underlying `tokens.ts` file IS portable). The existing `lazuli_codegen_ts::design::emit_tokens_ts` already targets a tree-shakeable ESM shape that works on RN.

2. **iPhone form-factor adaptive layouts in `.lzx`?** A `view list` declared `fields name, status, tier` looks fine on iPhone but cramped on iPad. **Resolution:** out of scope; V owns layout adaptivity. Lazuli emits the data shape, not the visual response.

3. **Push notifications surface in `.lzi`?** Lazuli already has `notification` in the domain DSL. The mobile-specific binding (Expo Push token registration, FCM device tokens) is per `@plugin/push-expo` / `@plugin/push-fcm` — plugin namespace, not core. Tracked separately under the notifications bucket.

4. **Deep-link param binding to `view detail`?** `route id: ID from path` already covers it (Expo Router supports deep-link URLs to `[id].tsx` natively). No new mechanism needed.

5. **Background tasks / Expo TaskManager?** Out of scope. Background work is a Go runtime + jobs concern; on-device tasks for offline-first sync are a future cut, plugin namespace.

6. **Offline-first caching?** TanStack Query supports persistence via `@tanstack/query-async-storage-persister`. **Resolution:** documented as a normative addendum in `docs/runtime-handoff.md` (Cell F.1) — "Optional persistence: install `@tanstack/query-async-storage-persister` and `@react-native-async-storage/async-storage`; wrap `<QueryClientProvider>` with `<PersistQueryClientProvider>`. Universal pattern; not part of the runtime core." Recipes-elsewhere concern (`feedback_normative_not_narrative_2026-05-15`) addressed by keeping the addendum strictly contractual: "this is how to plug X in", not narrative.

7. **What about the iOS- and Android-specific files (`ios/`, `android/`)?** Generated by `expo prebuild` on demand. Lazuli stays out — they're build artifacts. `.gitignore` template excludes them.

8. **Doctor severity ladder for Expo-Router group conflicts?** A view declaring `at "/buyer/profile"` may conflict with Expo's `app/(group)` routing syntax (`(profile)` is a group, not a route). **Resolution:** Cell D.2 (`lzx-route-collision`) is `error`. A future `warn` rule `LZX-ROUTE-EXPO-GROUP-CONFLICT` can fire when a route segment matches `(...)` group syntax. Tracked as Cell I polish; not blocking.

9. **Native testing infrastructure for `local-setting.native.test.ts` and `drawer-sub-view.native.test.ts`?** **Resolution:** Cells A.1 / A.2 ship native tests via `jest-expo` preset (the canonical RN testing path). Vitest is web-only; `jest-expo` is native-only. Both run in the same `pnpm test` invocation via `package.json` scripts: `"test:web": "vitest run"`, `"test:native": "jest-expo --testPathPattern=\\\\.native\\\\.test\\\\.ts$"`, `"test": "pnpm run test:web && pnpm run test:native"`.

10. **Cell G rename downstream impact.** Cell A.0 (`runtime/web/lazuli/` → `runtime/ts/lazuli/`) touches public template paths AND `Lazurite.toml [dev]` `plugin_paths` entries. **Resolution:** the rename happens **before** any external project pins to the new path. The current downstream pilots (Hostpoint, Pleiades) have not yet pinned because L0 #3 is still landing per-feature; A.0 lands during this proposal's wave 1 and updates the template + the one `dev_replace` in the public repo's `Cargo.toml`/`pnpm-workspace.yaml`. No deprecation alias needed.

---

## §13. References

- `docs/architecture.md` — three-layer architecture; mobile is the same Lazuli abstraction, different runtime substrate.
- `docs/target-stack.md` §"Mobile: Expo" — locked choice rationale.
- `docs/proposals/lzx-integration-codegen.md` (L0 #3) — view emitters, audience-scoped SDK projection, slot interfaces.
- `docs/proposals/lazurite-frontend-folder-canon.md` (L0 #1) — file canon.
- `docs/proposals/lzx-terminal-grammar.md` (L0 #6) — view-helpers vocabulary.
- `docs/invariants.md` — closed-catalog discipline.
- `docs/design-principles.md` — Rule Zero, Self-Contained Declarations.
- Memory: `project_strategic_pivot_2026-05-15` (Hostpoint pilot driver), `feedback_cement_over_ship_until_users_2026-05-15` (cement-first posture), `feedback_wave_workflow_lucas_preferred` (parallel wave dispatch), `feedback_grade_before_commit` (this proposal must pass `lazuli-language-architect`).
- Ecosystem precedent for conditional exports: Tamagui (`packages/core/package.json`), Sentry (`@sentry/react-native`), react-native-web.

---

## §14. Acceptance criteria

L0 PASS condition: this proposal answers, deterministically, for any Lazurite-shaped product targeting mobile:

1. **What changes in the IR or `.lzx` grammar to support mobile?** → **Nothing.** `SurfaceTarget::Mobile` + `surface … mobile` already exist (§1).
2. **What changes in the codegen emitters?** → **One new per-feature emitter** for `dist/ts-mobile/runtime/layout.tsx` (Cell C.1). The per-view emitters (`lzx_view_list/detail/create`) already branch on `RouterTarget::Expo` via `lzx_router_adapter::router_useparams_import` (§3.4); zero new emitter work for them.
3. **What does `lazuli new --frontends mobile` produce?** → User-owned scaffold at `frontends/mobile/` (idempotent, scaffolded-once: `app/_layout.tsx` one-line re-export, `app/index.tsx` placeholder, `shell/client.ts`, `babel.config.js`, `app.json`, `metro.config.js`, `tsconfig.json`, `package.json`, `.gitignore`) plus `[frontends.mobile]` block in `Lazurite.toml` (§5.1).
4. **What does `lazuli generate ts` produce for a mobile surface?** → Regen-only under `dist/ts-mobile/`: `<feat>/<feat>.gen.ts` + `.zod.ts` + per-view `<feat>/views/<audience>/<view>.gen.ts` + `<feat>/cells/<slot>.gen.ts` + the new `runtime/layout.tsx`. Plus one-shot scaffold of `frontends/mobile/app/<audience>/<route>.tsx` per declared view route (§5.2).
5. **Where does the runtime split live?** → `runtime/ts/lazuli/src/{react.ts, react.web.ts, react.native.ts, local-setting.{web,native}.ts, drawer-sub-view.{web,native}.ts, view-helpers.ts}` resolved by `package.json` exports `"react-native"` condition (§3.1 + §3.1.1 migration table).
6. **How do `useLocalSetting` and `useDrawerSubView` differ between web and native?** → Web uses `localStorage`+`useSyncExternalStore` (synchronous-on-first-render); native uses `AsyncStorage`+`useState/useEffect` (default-on-first-render, persisted value visible on next render). The JSDoc on the type declaration in `react.ts` is the contract (§3.4.1). `useDrawerSubView`: web closes on `Escape` via `window.addEventListener("keydown")`; native closes on Android hardware back via `BackHandler.addEventListener("hardwareBackPress")`. iOS swipe-back is delegated to Expo Router's stack behavior (§4.2).
7. **What new doctor rules?** → Two **generalized** rules per Rule Zero: `cell-missing-impl` extended to honor surface target (web vs mobile cells directory), and new `lzx-route-collision` (per-target router-translated path collisions). Both follow the existing catalog style with example snippets (§9.1, §9.2). No mobile-prefixed rules.
8. **What's the parity guarantee between `dist/ts-web/<feat>/<feat>.gen.ts` and `dist/ts-mobile/<feat>/<feat>.gen.ts`?** → Byte-identical modulo audience filtering (§7 MOBILE-SDK-PARITY). Tested by `crates/lazuli_codegen_ts/tests/parity.rs` (Cell D.3).
9. **Where is the smoke fixture?** → `examples/marketplace-mini-mobile/` (§8) with `crates/lazuli_cli/tests/mobile_smoke.rs` (Cell E.2). CI runs `tsc --noEmit` against the fixture via the Node toolchain (§8.1).
10. **What's the pilot binding?** → Hostpoint mobile (memory `project_strategic_pivot_2026-05-15`); §10 examples are Hostpoint-shaped. The proposal is **target-closing, not boundary-moving** (§2); the `≥3-pilot evidence` rule does not apply.
11. **What's out of scope?** → Single-file cross-target `.lzx`, native component-library opinion, EAS publish automation, push notification surface (separate `@plugin/push-expo`), design-tokens-native, in-`.lzx` adaptive layouts (§2 non-goals + §12 open questions).
12. **How is "compiles to RN" verified end-to-end?** → CI: §8.1 path assertions + `tsc --noEmit`. Manual smoke (post-Wave-4): §10.4 procedure walks the operator through `pnpm install && pnpm exec expo start` against `examples/marketplace-mini-mobile/`, with expected device output enumerated.
13. **How does typechecking work for web-only consumers that don't install RN?** → `react.ts` and `react.web.ts` reference zero RN modules at any position; `tsc --noEmit` against a web-only `tsconfig` resolves to `react.web.ts` via the `default` export condition and passes (§3.5).
14. **What's the migration risk for downstream pilots pinning `runtime/web/lazuli/`?** → Cell A.0 renames the folder to `runtime/ts/lazuli/` first; pilots have not yet pinned (per §12.10). No deprecation alias.

If all 14 answers are mechanical from the proposal text, L0 passes.
