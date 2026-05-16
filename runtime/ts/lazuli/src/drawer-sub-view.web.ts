// Web body for `useDrawerSubView` — state machine that auto-closes on
// pathname change, on delete success, on missing item, and on Escape
// (via `window.addEventListener("keydown")`).
//
// See `docs/proposals/lzx-terminal-grammar.md` §3.9 for the spec and
// `docs/proposals/mobile-target.md` §4.2 for the per-platform contract.
// The native counterpart is `drawer-sub-view.native.ts` (uses
// `BackHandler` for Android hardware back; iOS swipe-back is handled by
// Expo Router automatically for navigated screens).

import { useCallback, useEffect, useRef, useState } from "react";

import type { DrawerConfig, DrawerSubView } from "./view-helpers.js";

/**
 * Drawer state machine — open(id)/close() + auto-close on pathname change,
 * on delete success, on missing item, and on Escape (§3.9). Hook stays
 * router-agnostic: caller passes `pathname` + `lastDeleteSuccess` from
 * the target's router/command hooks.
 */
export function useDrawerSubView<TInput, TItem>(
  config: DrawerConfig<TInput, TItem> = {},
): DrawerSubView<TInput, TItem> {
  const [id, setId] = useState<TInput | null>(null);

  const close = useCallback(() => setId(null), []);
  const open = useCallback((next: TInput) => setId(next), []);

  // Auto-close on pathname change (NOT search-param change — filters
  // mutate search params freely without dismissing the drawer).
  const lastPathRef = useRef<string | undefined>(config.pathname);
  useEffect(() => {
    if (lastPathRef.current !== undefined && lastPathRef.current !== config.pathname) {
      setId(null);
    }
    lastPathRef.current = config.pathname;
  }, [config.pathname]);

  // Auto-close on delete success.
  const lastDeleteRef = useRef<number | null>(config.lastDeleteSuccess ?? null);
  useEffect(() => {
    if (config.lastDeleteSuccess != null && config.lastDeleteSuccess !== lastDeleteRef.current) {
      setId(null);
    }
    lastDeleteRef.current = config.lastDeleteSuccess ?? null;
  }, [config.lastDeleteSuccess]);

  // Auto-close when the resolved item disappears from the source query.
  useEffect(() => {
    if (id !== null && config.itemMissing) setId(null);
  }, [id, config.itemMissing]);

  // Auto-close on Escape.
  useEffect(() => {
    if (id === null) return;
    if (typeof window === "undefined") return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setId(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [id]);

  return {
    isOpen: id !== null,
    id,
    item: id !== null ? (config.item ?? null) : null,
    open,
    close,
  };
}
