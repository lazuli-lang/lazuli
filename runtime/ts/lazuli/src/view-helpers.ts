// Universal view-state helper hooks for `.lzx`-emitted ViewModels
// (L0 #6 Terminal grammar). Every export in this file is pure React +
// JS — no DOM, no React Native — so it imports identically into
// `react.web.ts` and `react.native.ts`.
//
// Platform-coupled hooks (`useLocalSetting`, `useDrawerSubView`) live
// in sibling `.web.ts` / `.native.ts` files and are wired by the per-
// target entrypoints. See `docs/proposals/mobile-target.md` §3.1 for
// the full split table.
//
// See `docs/proposals/lzx-terminal-grammar.md` §3 + §5.1 for the
// design context; cell C.4b of §6 originally shipped this file.

import { useCallback, useRef, useState } from "react";
import { parse as parseSearchQuery, type SearchParserOptions, type SearchParserResult } from "search-query-parser";

// --- §3.3 filters ----------------------------------------------------------

export interface FilterState<T> {
  value: T;
  set(value: T): void;
  clear(): void;
}

export interface MultiFilterState<T> {
  value: T[];
  add(value: T): void;
  remove(value: T): void;
  toggle(value: T): void;
  set(values: T[]): void;
  clear(): void;
}

// Router-shaped params interface — TanStack / Next App Router / Expo
// Router all expose URLSearchParams-compatible reads via `.get` and
// `.getAll`, and accept a setter callback that the codegen wires per
// target. Helpers stay agnostic by depending on this shape only.
export interface UrlParams {
  get(key: string): string | null;
  getAll(key: string): string[];
}

export type SetUrlParams = (
  updater: (prev: URLSearchParams) => URLSearchParams,
) => void;

export type FilterConfig<T> =
  | {
      mode: "single";
      urlKey?: string;
      params?: UrlParams;
      setParams?: SetUrlParams;
      initial?: T;
      empty?: T;
    }
  | {
      mode: "multi";
      urlKey?: string;
      params?: UrlParams;
      setParams?: SetUrlParams;
      initial?: T[];
    };

export type FilterStates<TConfig extends Record<string, FilterConfig<unknown>>> = {
  [K in keyof TConfig]: TConfig[K] extends { mode: "multi" }
    ? MultiFilterState<unknown>
    : FilterState<unknown>;
};

/**
 * Manage a record of single- and multi-value filter states. URL-synced
 * filters mutate the router's search params via `setParams` (repeated-key
 * for multi-value per §3.3 spec); unsynced filters live in component
 * state. The hook is router-agnostic: codegen passes a `params`/
 * `setParams` pair scraped from the target's router API at the call site.
 */
export function useFilterState<TConfig extends Record<string, FilterConfig<unknown>>>(
  config: TConfig,
): FilterStates<TConfig> {
  // One useState bucket holds the in-memory state for every filter; URL
  // reads override on each render so the URL is authoritative when synced.
  const [local, setLocal] = useState<Record<string, unknown>>(() => {
    const init: Record<string, unknown> = {};
    for (const [name, cfg] of Object.entries(config) as [string, FilterConfig<unknown>][]) {
      if (cfg.mode === "multi") {
        init[name] = cfg.initial ?? [];
      } else {
        init[name] = cfg.initial ?? cfg.empty ?? null;
      }
    }
    return init;
  });

  const result = {} as Record<string, FilterState<unknown> | MultiFilterState<unknown>>;
  for (const [name, cfg] of Object.entries(config) as [string, FilterConfig<unknown>][]) {
    const synced = cfg.urlKey && cfg.params && cfg.setParams;

    if (cfg.mode === "multi") {
      const value: unknown[] = synced
        ? (cfg.params!.getAll(cfg.urlKey!) as unknown[])
        : ((local[name] ?? []) as unknown[]);

      const writeMulti = (next: unknown[]) => {
        if (synced) {
          cfg.setParams!((prev) => {
            const out = new URLSearchParams(prev);
            out.delete(cfg.urlKey!);
            for (const v of next) {
              if (v != null) out.append(cfg.urlKey!, String(v));
            }
            return out;
          });
        } else {
          setLocal((s) => ({ ...s, [name]: next }));
        }
      };

      result[name] = {
        value,
        set: writeMulti,
        add: (v) => writeMulti(value.includes(v) ? value : [...value, v]),
        remove: (v) => writeMulti(value.filter((x) => x !== v)),
        toggle: (v) => writeMulti(value.includes(v) ? value.filter((x) => x !== v) : [...value, v]),
        clear: () => writeMulti([]),
      } satisfies MultiFilterState<unknown>;
    } else {
      const value: unknown = synced
        ? (cfg.params!.get(cfg.urlKey!) ?? cfg.empty ?? null)
        : (local[name] ?? cfg.empty ?? null);

      const writeSingle = (next: unknown) => {
        if (synced) {
          cfg.setParams!((prev) => {
            const out = new URLSearchParams(prev);
            if (next == null || next === "" || next === cfg.empty) {
              out.delete(cfg.urlKey!);
            } else {
              out.set(cfg.urlKey!, String(next));
            }
            return out;
          });
        } else {
          setLocal((s) => ({ ...s, [name]: next }));
        }
      };

      result[name] = {
        value,
        set: writeSingle,
        clear: () => writeSingle(cfg.empty ?? null),
      } satisfies FilterState<unknown>;
    }
  }
  return result as FilterStates<TConfig>;
}

// --- §3.6 selection (multi) ------------------------------------------------

export interface MultiSelection<TId extends string> {
  ids: Set<TId>;
  has(id: TId): boolean;
  toggle(id: TId, shiftKey?: boolean): void;
  selectRange(fromId: TId, toId: TId): void;
  clear(): void;
}

/**
 * Multi-selection state with shift-range semantics. The range is computed
 * against the `items` snapshot captured at call time (not the live list)
 * so refetches mid-range don't drift the endpoints. `lastSelectedId` is
 * a ref — never triggers a re-render itself.
 */
export function useMultiSelection<TId extends string>(items: ReadonlyArray<{ id: TId }>): MultiSelection<TId> {
  const [ids, setIds] = useState<Set<TId>>(() => new Set());
  const lastSelectedRef = useRef<TId | null>(null);
  const itemsRef = useRef(items);
  itemsRef.current = items;

  const has = useCallback((id: TId) => ids.has(id), [ids]);

  const selectRange = useCallback((fromId: TId, toId: TId) => {
    const snapshot = itemsRef.current;
    const from = snapshot.findIndex((i) => i.id === fromId);
    const to = snapshot.findIndex((i) => i.id === toId);
    if (from === -1 || to === -1) return;
    const lo = Math.min(from, to);
    const hi = Math.max(from, to);
    setIds((prev) => {
      const next = new Set(prev);
      for (let i = lo; i <= hi; i++) {
        const item = snapshot[i];
        if (item) next.add(item.id);
      }
      return next;
    });
    lastSelectedRef.current = toId;
  }, []);

  const toggle = useCallback(
    (id: TId, shiftKey?: boolean) => {
      if (shiftKey && lastSelectedRef.current && lastSelectedRef.current !== id) {
        selectRange(lastSelectedRef.current, id);
        return;
      }
      setIds((prev) => {
        const next = new Set(prev);
        if (next.has(id)) next.delete(id);
        else next.add(id);
        return next;
      });
      lastSelectedRef.current = id;
    },
    [selectRange],
  );

  const clear = useCallback(() => {
    setIds(new Set());
    lastSelectedRef.current = null;
  }, []);

  return { ids, has, toggle, selectRange, clear };
}

// --- §3.9 drawer sub-view (types only — bodies live per-platform) --------

export interface DrawerSubView<TInput, TItem> {
  isOpen: boolean;
  id: TInput | null;
  item: TItem | null;
  open(id: TInput): void;
  close(): void;
}

export interface DrawerConfig<TInput, TItem> {
  /** Latest item resolved for the open id; null if not yet loaded or not open. */
  item?: TItem | null;
  /** Truthy when the open id no longer resolves; triggers close per §3.9. */
  itemMissing?: boolean;
  /** Current pathname (passed in from the consumer's router). */
  pathname?: string;
  /** Bumps every time a delete command in this drawer's action set succeeds. */
  lastDeleteSuccess?: number | null;
  /** Subscribe to selection.clear() to drop the open id if it matches. */
  selectionContainsOpenId?: boolean;
}

// `useDrawerSubView` body lives in `drawer-sub-view.web.ts` / `.native.ts`
// per `docs/proposals/mobile-target.md` §3.1. The web body uses the
// keyboard `Escape` key; the native body uses Android `BackHandler`.

// --- §3.7 local-persisted settings (body per-platform) -------------------

// `useLocalSetting` body lives in `local-setting.web.ts` / `.native.ts`
// per `docs/proposals/mobile-target.md` §3.1. Web reads `localStorage`
// synchronously; native reads `AsyncStorage` asynchronously. The
// first-render contract divergence is documented on each body's JSDoc.

// --- §3.4 search helpers --------------------------------------------------

/** Pure canonical search-string emitter — alphabetical keys, repeated for
 *  multi-value, free text appended last. Caller is responsible for value
 *  escaping (tag values typically have no spaces; this is by convention). */
export function canonicalizeSearch(
  state: { [key: string]: string | string[] | null | undefined },
  freeText: string,
): string {
  const keys = Object.keys(state).filter((k) => k !== "free").sort();
  const segments: string[] = [];
  for (const key of keys) {
    const v = state[key];
    if (v == null || v === "") continue;
    if (Array.isArray(v)) {
      for (const each of v) {
        if (each == null || each === "") continue;
        segments.push(`${key}:${each}`);
      }
    } else {
      segments.push(`${key}:${v}`);
    }
  }
  const trimmed = freeText.trim();
  if (trimmed) segments.push(trimmed);
  return segments.join(" ");
}

export interface ParsedSegment {
  kind: "filter" | "free";
  field?: string;
  value: string;
}

/** Thin wrapper over `search-query-parser` that flattens its output into
 *  an ordered `ParsedSegment[]`. Multi-value keys come out as repeated
 *  segments. */
export function parseSegments(
  raw: string,
  keywords: readonly string[],
  alwaysArray: readonly string[],
): ParsedSegment[] {
  // The bundled `SearchParserOptions` types declare `alwaysArray` as boolean
  // (an upstream `.d.ts` bug); the runtime accepts `string[]`. Cast through
  // unknown to honour the well-documented runtime contract.
  const options = {
    keywords: [...keywords],
    alwaysArray: [...alwaysArray],
  } as unknown as SearchParserOptions;
  const parsed = parseSearchQuery(raw, options) as string | SearchParserResult;

  // search-query-parser returns the raw string when no keyword matches.
  if (typeof parsed === "string") {
    const free = parsed.trim();
    return free ? [{ kind: "free", value: free }] : [];
  }

  const out: ParsedSegment[] = [];
  for (const key of keywords) {
    const value = parsed[key];
    if (value == null) continue;
    if (Array.isArray(value)) {
      for (const each of value) {
        out.push({ kind: "filter", field: key, value: String(each) });
      }
    } else {
      out.push({ kind: "filter", field: key, value: String(value) });
    }
  }
  const text = parsed.text;
  if (typeof text === "string" && text.trim()) {
    out.push({ kind: "free", value: text.trim() });
  } else if (Array.isArray(text)) {
    for (const t of text) {
      const trimmed = String(t).trim();
      if (trimmed) out.push({ kind: "free", value: trimmed });
    }
  }
  return out;
}
