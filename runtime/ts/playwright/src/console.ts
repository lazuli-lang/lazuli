import type { Page } from '@playwright/test';

export interface CaptureRuntimeErrorsOptions {
  readonly ignorablePatterns?: ReadonlyArray<RegExp>;
}

export interface RuntimeErrorCapture {
  /** Every collected error, in arrival order, unfiltered. */
  readonly errors: ReadonlyArray<string>;
  /** Errors after subtracting `IGNORABLE_PATTERNS`. The list a test should assert against. */
  getFatal(): ReadonlyArray<string>;
}

// Errors we knowingly accept during smoke. Keep this list short.
export const IGNORABLE_PATTERNS: ReadonlyArray<RegExp> = [
  // 401/403 on /api/v1/c/account.me before a user signs in.
  /Request failed with status code 40[13]/i,
  // Wrong-password / validation errors can surface as handled 500 logs today.
  /Request failed with status code 500/i,
  // PWA service-worker registration noise on first dev load. Cosmetic.
  /Failed to register a ServiceWorker/i,
];

export function captureRuntimeErrors(
  page: Page,
  opts: CaptureRuntimeErrorsOptions = {},
): RuntimeErrorCapture {
  const patterns = opts.ignorablePatterns ?? IGNORABLE_PATTERNS;
  const errors: string[] = [];

  page.on('pageerror', (err) => {
    errors.push(`pageerror: ${err.message}`);
  });
  page.on('console', (msg) => {
    if (msg.type() === 'error') {
      errors.push(`console.error: ${msg.text()}`);
    }
  });

  return {
    errors,
    getFatal() {
      return errors.filter((m) => !patterns.some((re) => re.test(m)));
    },
  };
}
