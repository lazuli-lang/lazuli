# Lazuli Playwright Helpers

## TL;DR

Install `@lazuli/playwright` only in apps that run Playwright e2e tests.
Use the helpers to seed sessions, create users, and capture runtime errors without testing UI login every time.
Generated Playwright specs should target Lazuli contracts, not component internals.

## Install

```bash
pnpm add -D @lazuli/playwright @playwright/test
```

## Helper examples

`captureRuntimeErrors` collects page errors and console errors while filtering
the short default ignore list.

```ts
import { expect, test } from '@playwright/test';
import { captureRuntimeErrors } from '@lazuli/playwright';

test('home has no fatal runtime errors', async ({ page }) => {
  const runtime = captureRuntimeErrors(page);
  await page.goto('/');
  expect(runtime.getFatal()).toEqual([]);
});
```

`signInAs` writes the Lazuli session token into browser storage before
navigation.

```ts
import { test } from '@playwright/test';
import { signInAs } from '@lazuli/playwright';

test('admin dashboard loads', async ({ page }) => {
  await signInAs(page, process.env.ADMIN_TOKEN!);
  await page.goto('/admin');
});
```

`registerUser` calls the generated account register command through
Playwright's API client.

```ts
import { expect, test } from '@playwright/test';
import { registerUser } from '@lazuli/playwright';

test('registers an actor', async ({ request }) => {
  const { user, password } = await registerUser(request, { slug: 'owner' });
  expect(user.email).toContain('e2e-owner-');
  expect(password).toBeTruthy();
});
```

`registerWithRole` creates a user, applies a role, logs in, and returns the
token for browser seeding.

```ts
import { test } from '@playwright/test';
import { registerWithRole, signInAs } from '@lazuli/playwright';

test('billing owner can open settings', async ({ page, request }) => {
  const actor = await registerWithRole(request, 'owner');
  await signInAs(page, actor.token);
  await page.goto('/settings/billing');
});
```

## Codegen examples

Generate policy-matrix specs from command and query policy declarations:

```bash
lazuli generate playwright --target=api-policy
```

Generate route/view coverage specs from lifecycle-gate metadata:

```bash
lazuli generate playwright --target=lifecycle-gate
```

Generated specs should remain regen-only. Product-specific setup belongs in the
Playwright project config or normal test fixtures.

## Config knobs

| Knob | Default | Used by |
|---|---|---|
| `storageKey` | `lazuli_session_token` | `signInAs`, `clearSession` |
| `apiUrl` | `process.env.LAZULI_API_URL ?? "http://localhost:8080"` | `registerUser`, `loginUser`, `setRole`, `registerWithRole` |
| `commandNamespace` | `account` | API helpers that call `/api/v1/c/<namespace>.<command>` |
| `ignorablePatterns` | `IGNORABLE_PATTERNS` | `captureRuntimeErrors` |

## Non-goals

- No UI helpers.
- No CI templates.
- No Cypress/Vitest support.
