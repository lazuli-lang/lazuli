import type { Page } from '@playwright/test';

export interface SessionOptions {
  readonly storageKey?: string;
}

const STORAGE_KEY = 'lazuli_session_token';

export async function signInAs(
  page: Page,
  token: string,
  opts: SessionOptions = {},
): Promise<void> {
  const key = opts.storageKey ?? STORAGE_KEY;
  await page.addInitScript(
    ({ key, value }: { key: string; value: string }) => {
      try {
        window.localStorage.setItem(key, value);
      } catch {
        // Storage quota / private mode; the spec will fail on the auth-required step.
      }
    },
    { key, value: token },
  );
}

export async function clearSession(page: Page, opts: SessionOptions = {}): Promise<void> {
  const key = opts.storageKey ?? STORAGE_KEY;
  await page.addInitScript((storageKey: string) => {
    try {
      window.localStorage.removeItem(storageKey);
    } catch {
      // ignored
    }
  }, key);
}
