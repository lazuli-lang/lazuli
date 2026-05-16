// Web body for `useLocalSetting` — backed by `localStorage` and synced
// across tabs via the `storage` event. The hook returns the persisted
// value synchronously on first render (uses `useSyncExternalStore`).
//
// See `docs/proposals/mobile-target.md` §4.1 for the per-platform
// contract; the native counterpart is `local-setting.native.ts`. Both
// implementations share the JSDoc-documented divergence on first-render
// behavior (web: sync; native: async).

import { useCallback, useRef, useSyncExternalStore } from "react";

function subscribeLocal(key: string, onChange: () => void): () => void {
  if (typeof window === "undefined") return () => {};
  const handler = (e: StorageEvent) => {
    if (e.key === key || e.key === null) onChange();
  };
  window.addEventListener("storage", handler);
  return () => window.removeEventListener("storage", handler);
}

/**
 * `useState`-shaped tuple backed by `localStorage`. Listens for `storage`
 * events so two tabs editing the same key converge. JSON-serialized.
 *
 * IMPORTANT — first-render contract differs by platform:
 *   - Web (this file): returns the persisted value synchronously on
 *     first render (uses useSyncExternalStore + localStorage).
 *   - Native (`local-setting.native.ts`): returns `defaultValue` on
 *     first render; the persisted value becomes visible on the next
 *     render after AsyncStorage resolves.
 *
 * Callers MUST treat the value as eventually-consistent. Code that
 * depends on the persisted value being present on the first frame
 * (e.g., to pick a theme before paint) must read storage explicitly
 * via the platform's lower-level API, not via this hook.
 */
export function useLocalSetting<T>(key: string, defaultValue: T): [T, (next: T) => void] {
  // Use a ref to bridge useSyncExternalStore (which expects a snapshot
  // identity) and JSON.parse (which always produces fresh objects).
  const cacheRef = useRef<{ raw: string | null; value: T }>({ raw: null, value: defaultValue });

  const getSnapshot = useCallback((): T => {
    if (typeof window === "undefined" || !window.localStorage) return defaultValue;
    const raw = window.localStorage.getItem(key);
    if (raw === cacheRef.current.raw) return cacheRef.current.value;
    try {
      const parsed = raw === null ? defaultValue : (JSON.parse(raw) as T);
      cacheRef.current = { raw, value: parsed };
      return parsed;
    } catch {
      cacheRef.current = { raw, value: defaultValue };
      return defaultValue;
    }
  }, [key, defaultValue]);

  const subscribe = useCallback((onChange: () => void) => subscribeLocal(key, onChange), [key]);

  const getServerSnapshot = useCallback((): T => defaultValue, [defaultValue]);

  const value = useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);

  const setValue = useCallback(
    (next: T) => {
      if (typeof window === "undefined" || !window.localStorage) return;
      const serialized = JSON.stringify(next);
      window.localStorage.setItem(key, serialized);
      cacheRef.current = { raw: serialized, value: next };
      // Fire a synthetic storage event so the same-tab subscriber re-reads.
      window.dispatchEvent(new StorageEvent("storage", { key, newValue: serialized }));
    },
    [key],
  );

  return [value, setValue];
}
