// Native body for `useLocalSetting` — backed by `AsyncStorage`. Reads
// the persisted value asynchronously on first render (returns
// `defaultValue` initially; the persisted value resolves on the next
// render after the microtask completes).
//
// See `docs/proposals/mobile-target.md` §4.1 for the per-platform
// contract; the web counterpart is `local-setting.web.ts`. First-render
// divergence is documented in the JSDoc on `useLocalSetting`.

import AsyncStorage from "@react-native-async-storage/async-storage";
import { useCallback, useEffect, useRef, useState } from "react";

/**
 * `useState`-shaped tuple backed by `AsyncStorage`. JSON-serialized.
 *
 * IMPORTANT — first-render contract differs by platform:
 *   - Web (`local-setting.web.ts`): returns the persisted value
 *     synchronously on first render (uses useSyncExternalStore +
 *     localStorage).
 *   - Native (this file): returns `defaultValue` on first render; the
 *     persisted value becomes visible on the next render after
 *     AsyncStorage resolves.
 *
 * Callers MUST treat the value as eventually-consistent. Code that
 * depends on the persisted value being present on the first frame
 * (e.g., to pick a theme before paint) must read storage explicitly
 * via the platform's lower-level API, not via this hook.
 */
export function useLocalSetting<T>(key: string, defaultValue: T): [T, (next: T) => void] {
  const [value, setValue] = useState<T>(defaultValue);
  const loaded = useRef(false);

  useEffect(() => {
    let cancelled = false;
    AsyncStorage.getItem(key)
      .then((raw: string | null) => {
        if (cancelled) return;
        loaded.current = true;
        if (raw === null) return;
        try {
          setValue(JSON.parse(raw) as T);
        } catch {
          // Malformed entry — fall back to defaultValue (already set).
        }
      })
      .catch(() => {
        // Best-effort: read failures leave defaultValue in place.
      });
    return () => {
      cancelled = true;
    };
  }, [key]);

  const update = useCallback(
    (next: T) => {
      setValue(next);
      AsyncStorage.setItem(key, JSON.stringify(next)).catch(() => {
        // Best-effort: write failures don't propagate; the in-memory
        // state already reflects the user's intent.
      });
    },
    [key],
  );

  return [value, update];
}
