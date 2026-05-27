# @lazuli/playwright

Playwright helpers for Lazuli-driven apps.

This package is opt-in. Apps that do not use Playwright do not need to install
or import it.

The helpers are wire-thin wrappers around Lazuli's canonical e2e conventions:

- browser sessions use localStorage key `lazuli_session_token`
- account commands live at `POST /api/v1/c/account.<command>`
- the default API URL is `process.env.LAZULI_API_URL ?? http://localhost:8080`
- runtime error capture ignores known benign browser-console noise

Exports:

```ts
import {
  captureRuntimeErrors,
  signInAs,
  clearSession,
  registerUser,
  loginUser,
  setRole,
  registerWithRole,
  freshEmail,
} from '@lazuli/playwright';
```

Use `signInAs(page, token)` to seed a page before navigation. Use the API
helpers from Playwright's `APIRequestContext` when a spec needs an actor but
does not care about the UI sign-in flow.

Every helper keeps an override for the corresponding app-level escape hatch:
`storageKey`, `apiUrl`, `commandNamespace`, and `ignorablePatterns`.

Design context: `external-ops/docs/proposals/playwright-plugin.md`.
