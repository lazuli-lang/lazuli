// Native body for `useDrawerSubView` — state machine that auto-closes
// on pathname change, on delete success, on missing item, and on
// Android hardware back via React Native's `BackHandler`. iOS swipe-back
// is handled by Expo Router automatically for navigated stack screens;
// `<Modal>` consumers wire `onRequestClose` to `close()` themselves.
//
// See `docs/proposals/mobile-target.md` §4.2. The web counterpart is
// `drawer-sub-view.web.ts` (uses `window.addEventListener("keydown")`
// for the Escape key).

import { useCallback, useEffect, useRef, useState } from "react";
import { BackHandler } from "react-native";

import type { DrawerConfig, DrawerSubView } from "./view-helpers.js";

/**
 * Drawer state machine — open(id)/close() + auto-close on pathname
 * change, on delete success, on missing item, and on Android hardware
 * back press. Hook stays router-agnostic: caller passes `pathname` +
 * `lastDeleteSuccess` from the target's router/command hooks.
 */
export function useDrawerSubView<TInput, TItem>(
  config: DrawerConfig<TInput, TItem> = {},
): DrawerSubView<TInput, TItem> {
  const [id, setId] = useState<TInput | null>(null);

  const close = useCallback(() => setId(null), []);
  const open = useCallback((next: TInput) => setId(next), []);

  // Auto-close on pathname change.
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

  // Android hardware back closes the drawer.
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
