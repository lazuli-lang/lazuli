# Route Guards — Adoption Playbook

This guide covers everything a downstream Lazuli product team needs to adopt
declarative route guards end-to-end: DSL syntax, generated artifacts, runtime
wiring, verification, e2e testing, and the cascade-removal migration pattern.

**Shipped by**:
LAZ-67 (ANALYZE-1, diagnostics `eed34420`),
LAZ-68 (CODEGEN-1, `84d69337`),
LAZ-69 (RUNTIME-1, `75c716c4`),
LAZ-70 (LSP, `5ec97229`),
LAZ-72 (TanStack adapter, `0d1eb45a`),
LAZ-73 (docs merge, `f7d9b234`).

---

## 1. `.lzx` Declaration

### App-level fallback (`app.lzi`)

Before individual views or audiences declare guards, give the app a fallback.
`actor_query` is required whenever any non-public route guard exists:

```lazuli
app HostPoint
  actor_query account.query.me
  route_guard
    default_policy @scope.authenticated
    on_unauthenticated redirect "/sign-in"
    on_unauthorized redirect "/explore"
    skeleton @client.route_guard_skeleton
```

`actor_query <feature>.query.<name>` names the query the generated SDK uses to
hydrate the current `LazuliActor | null`. The `skeleton` slot renders while that
query is in-flight; omit it to render nothing while the verdict is pending.

### Audience-level policy (platform surface file)

The most common placement is an `audience` block inside a platform surface.
All views in that audience inherit the guard unless they set their own:

```lazuli
surface property web
  uses experience property

  audience host
    policy @policy.host_only
      on_unauthenticated redirect "/sign-in"
      on_unauthorized redirect "/explore"

    view list List
      fields name, address, status

    view create Form
      submit property.command.create
```

The snippet above reflects the expected state of
`examples/lazurite-multifrontend/features/property/property.host.web.lzx`
after LAZ-72.B lands. The pre-LAZ-72.B file declares the `audience host` block
without the `policy` guard; adding the guard is the migration step the cascade-
removal section describes.

### View-level policy (per-view override)

A single view can override or add an audience policy. Per-slot cascade applies:
a view that declares `on_unauthenticated` but not `policy` inherits the audience
(or app) policy while keeping its own redirect:

```lazuli
experience property
  view detail SidePanel
    source property.query.by_id(id: route.id)
    policy @policy.host_only
      on_unauthenticated redirect "/sign-in"
      on_unauthorized redirect "/explore"
```

### Policy combination patterns

| Pattern | Declaration form |
|---------|-----------------|
| Single: any authenticated user | `policy @scope.authenticated` |
| Role-based: host only | `policy @policy.host_only` (resolves to `@scope.authenticated, @role.host`) |
| Anonymous only (public route) | no `policy` child; omit the guard entirely |
| AND semantics (role + org scope) | define a combined policy category: `manager_scoped: @scope.same_org, @role.manager`, then `policy @policy.manager_scoped` |

Policy categories live in the `.lzi` `policies` block:

```lazuli
feature property
  policies
    host_only: @scope.authenticated, @role.host
    admin_only: @scope.authenticated, @role.admin
```

Named categories (`@policy.*`) are the only valid policy refs inside `.lzx`
view/audience guards. Bare `@scope.*` or `@role.*` are valid at the view level
but cannot share a redirect override with a named category in the same slot.

### Redirect cardinality

`on_unauthenticated` and `on_unauthorized` are each `0..1` per `policy` guard
block. Both target a declared route path string (not a route name). Doctor
`ROUTE-GUARD-003` rejects a redirect target that does not resolve to any
declared route path.

---

## 2. HOC vs `beforeLoad` Decision Tree

Two integration patterns are available:

```
Does the app use TanStack Router?
│
├─ YES ─► Does the guard need to redirect BEFORE the route component is
│          even imported (SSR, lazy chunk, auth-gated shell)?
│          │
│          ├─ YES ─► tanstackBeforeLoadGuard  (redirect before mount)
│          │
│          └─ NO  ─► RouteGuard HOC  (fine for CSR; component imports
│                     happen but children don't render)
│
└─ NO  ─► RouteGuard HOC  (works with any React 18+ router)
```

### `<RouteGuard>` — React HOC

**Source**: `runtime/ts/lazuli/src/route-guard-component.tsx` (LAZ-69).

- Runs **after** the component tree mounts.
- Uses `useActor()` to hydrate the current actor, then `useEffect` to trigger
  the redirect once the verdict is known.
- While the actor query is in-flight the `skeleton` prop is rendered; once the
  redirect fires, `null` is rendered (the children never render on a bad actor).
- No flash-of-protected-content on the initial render because the guard renders
  `skeleton` (not children) until the verdict resolves.
- Works with any router that exposes a `navigate` method via `LazuliClient`.

```tsx
import { RouteGuard } from "@lazuli/runtime/react";
import { hostHomeRoute } from "../dist/ts-host/route-guards.gen.js";

function HostHomePage() {
  return (
    <RouteGuard
      policy={hostHomeRoute.policy}
      onUnauthenticated={hostHomeRoute.onUnauthenticated}
      onUnauthorized={hostHomeRoute.onUnauthorized}
      skeleton={<RouteGuardSkeleton />}
    >
      <HostHomeScreen />
    </RouteGuard>
  );
}
```

**Trade-offs**: simpler JSX wiring; redirect fires after first render cycle.
Acceptable for full CSR apps. Avoid for SSR routes where the 401/403 shell must
not be sent to the client at all.

### `tanstackBeforeLoadGuard` — TanStack Router `beforeLoad`

**Source**: `@lazuli/runtime/react/tanstack` (LAZ-72, separate entrypoint to
avoid importing `@tanstack/react-router` in non-TanStack projects).

- Runs **before** the route component loads, in TanStack Router's `beforeLoad`
  phase.
- Calls `client.resolveActor()` (async), evaluates the policy, and throws
  TanStack's `redirect()` if the verdict is not `authorized`.
- Redirect happens before the component tree is instantiated — no flash, no
  chunk import, SSR-safe.
- Requires `context.client` to be a `LazuliClient` instance threaded through
  the TanStack router context.

```tsx
import { tanstackBeforeLoadGuard } from "@lazuli/runtime/react/tanstack";
import { redirect } from "@tanstack/react-router";
import { hostHomeRoute } from "../dist/ts-host/route-guards.gen.js";

const hostHomeFile = createFileRoute("/host/")({
  beforeLoad: tanstackBeforeLoadGuard(client, {
    policy: hostHomeRoute.policy,
    onUnauthenticated: hostHomeRoute.onUnauthenticated ?? "/sign-in",
    onUnauthorized: hostHomeRoute.onUnauthorized ?? "/explore",
    redirect,
  }),
  component: HostHomeScreen,
});
```

**Trade-offs**: redirect-before-mount, SSR-compatible. Requires TanStack Router
as a peer dep and the `context.client` convention. Suspense-compatible (the
guard is async; TanStack Router already suspends during `beforeLoad`).

### Compatibility matrix

| Concern | `<RouteGuard>` | `tanstackBeforeLoadGuard` |
|---------|----------------|--------------------------|
| Redirect timing | After first render | Before component loads |
| SSR safety | No (guard is CSR) | Yes |
| Suspense compatible | Yes (skeleton prop) | Yes (async beforeLoad) |
| Flash-of-protected-content | No (skeleton replaces) | No |
| Router dependency | Any (`LazuliRouter`) | `@tanstack/react-router` |
| Setup complexity | Low (JSX wrap) | Medium (context thread) |

---

## 3. Codegen Surface

### What `route_guards.gen.ts` exports

The codegen emitter (LAZ-68, `crates/lazuli_codegen_ts/src/audience_sdk.rs`)
generates one registry file per frontend audience set:

**`dist/ts-<frontend>/route-guards.gen.ts`**

```typescript
// Code generated by lazuli; DO NOT EDIT.
//lazuli:pattern route_guard v1
import type { ActorQueryRef, RouteGuard, RouteGuardRegistry } from "@lazuli/runtime/react";
import { hostHomeRoute, hostPropertyCreateRoute } from "../host/host.web.host.gen.js";
import { exploreRoute } from "../host/host.web.public.gen.js";

const resolvedRouteGuards: Record<string, RouteGuard> = {
  "/explore": exploreRoute,
  "/host": hostHomeRoute,
  "/host/properties/new": hostPropertyCreateRoute,
};

export const routeGuardRegistry = {
  defaults: {
    policy: { atoms: [{ namespace: "scope", name: "authenticated" }] },
    unauthenticated: "/sign-in",
    unauthorized: "/explore",
    skeleton: "@client.route_guard_skeleton",
  },
  actorQuery: "account.query.me" as ActorQueryRef,
  routes: resolvedRouteGuards,
} as const satisfies RouteGuardRegistry;
```

Each per-audience slot file (`<feature>.web.<audience>.gen.ts`) exports typed
route consts:

```typescript
// dist/ts-host/host/host.web.host.gen.ts
export const hostHomeRoute = {
  path: "/host",
  component: HostHomeScreen,
  policy: {
    name: "@policy.host_only",
    atoms: [
      { namespace: "scope", name: "authenticated" },
      { namespace: "role", name: "host" },
    ],
  },
  onUnauthenticated: "/host/sign-in",
  onUnauthorized: "/host",
} as const satisfies RouteGuardSpec<typeof HostHomeScreen>;
```

### Wiring in `App.tsx` — registry pattern

The registry is the single wiring point. Pass it to the Lazuli client at
bootstrap:

```tsx
// App.tsx
import { LazuliClient } from "@lazuli/runtime/react";
import { routeGuardRegistry } from "./dist/ts-host/route-guards.gen.js";

const client = new LazuliClient({
  baseUrl: import.meta.env.VITE_API_URL,
  actorQuery: routeGuardRegistry.actorQuery,
  router: tanstackRouter, // or your own LazuliRouter adapter
});

export function App() {
  return (
    <LazuliClientProvider client={client}>
      <RouterProvider router={tanstackRouter} />
    </LazuliClientProvider>
  );
}
```

For TanStack Router, thread the client through router context so `beforeLoad`
guards can call `ctx.context.client.resolveActor()`:

```tsx
const tanstackRouter = createRouter({
  routeTree,
  context: { client },
});
```

Each route that needs a guard uses the generated const directly — no manual
policy authoring in application code:

```tsx
// features/property/routes.tsx
import { tanstackBeforeLoadGuard } from "@lazuli/runtime/react/tanstack";
import { redirect } from "@tanstack/react-router";
import { hostPropertyCreateRoute } from "../../dist/ts-host/property/property.web.host.gen.js";

export const propertyCreateRoute = createFileRoute("/host/properties/new")({
  beforeLoad: tanstackBeforeLoadGuard(client, {
    policy: hostPropertyCreateRoute.policy,
    onUnauthenticated: hostPropertyCreateRoute.onUnauthenticated ?? "/sign-in",
    onUnauthorized: hostPropertyCreateRoute.onUnauthorized ?? "/explore",
    redirect,
  }),
  component: PropertyCreateScreen,
});
```

### Regenerating after `.lzx` changes

```bash
lazuli generate ts --frontend host
```

The output lands in `dist/ts-host/`. Commit the generated files alongside the
`.lzx` change; Doctor enforces parity.

---

## 4. Verification

### Doctor — expected-zero on happy path

Run `lazuli doctor <app-dir>` after wiring guards. On a correct setup all three
route-guard diagnostics must be zero:

| Code | Meaning | Fix |
|------|---------|-----|
| `ROUTE-GUARD-001` | View hosts a gated backend command/query but resolves only to the built-in public guard (no explicit `policy` at view, audience, or app level) | Add `policy @policy.<name>` to the audience or app `route_guard` block |
| `ROUTE-GUARD-002` | Resolved view guard is laxer than a backend policy it hosts; specific atoms are listed in the message | Tighten the view/audience guard to cover the missing atoms |
| `ROUTE-GUARD-003` | A `redirect` target path does not resolve to any declared `.lzx` route | Add the missing route declaration or fix the path string |
| `ROUTE-GUARD-004` *(warning)* | App has non-public route guards but no `actor_query` | Add `actor_query <feature>.query.<name>` to `app.lzi` |

Example clean run:

```
$ lazuli doctor .
✓  0 errors, 0 warnings
```

After a partial migration (some guards added, some still missing) you will see
`ROUTE-GUARD-001` for every unguarded view that sources or submits a
non-public backend operation. Treat them as a punch-list: address each with an
audience-level policy block.

### Inspect — route-guard coverage

Once `--expand=route-guard-coverage` lands (tracked separately), the intended
output is:

```bash
lazuli inspect --expand=route-guard-coverage .
```

```json
{
  "route_guard_coverage": [
    { "route": "host_home", "path": "/host", "policy_source": "audience", "atoms": ["scope.authenticated", "role.host"] },
    { "route": "explore",   "path": "/explore", "policy_source": "builtin", "atoms": [] }
  ]
}
```

A completed migration has zero rows where `policy_source: "unguarded"` on
gated-command routes. Until the expansion ships, use the Doctor diagnostics
above as the equivalent gate.

---

## 5. Playwright Recipe — No Flash-of-Protected-Content

This recipe verifies that a logged-out user navigating directly to a gated route
is redirected **before the screen mounts** — not after a flash of the protected
UI.

```typescript
// e2e/route-guard.spec.ts
import { test, expect } from "@playwright/test";

test("logged-out user hitting a gated route is redirected before screen renders", async ({ page }) => {
  // Arrange: ensure no session cookie is present
  await page.context().clearCookies();

  // Act: navigate directly to the gated route
  const navigationPromise = page.waitForURL("**/sign-in");
  await page.goto("/host/properties/new");
  await navigationPromise;

  // Assert: landed on sign-in, not on the gated page
  expect(page.url()).toContain("/sign-in");

  // Assert: the protected screen's DOM marker never appeared
  // Replace "#property-create-form" with the actual root element selector.
  await expect(page.locator("#property-create-form")).toHaveCount(0);
});

test("logged-in non-host user hitting a host-only route is redirected to /explore", async ({ page, request }) => {
  // Arrange: sign in as a traveler (no host role)
  const loginRes = await request.post("/api/auth/sign-in", {
    data: { email: "traveler@example.com", password: "test-password" },
  });
  const cookies = loginRes.headers()["set-cookie"];
  await page.context().addCookies(parseCookies(cookies));

  // Act: navigate to host-only route
  const navigationPromise = page.waitForURL("**/explore");
  await page.goto("/host");
  await navigationPromise;

  // Assert: unauthorized actor lands on the unauthorized redirect
  expect(page.url()).toContain("/explore");
  await expect(page.locator("#host-dashboard")).toHaveCount(0);
});
```

For TanStack Router's `beforeLoad` pattern, the redirect fires **synchronously
during navigation** before any component code runs. The `waitForURL` assertion
catches the final URL; the `toHaveCount(0)` assertion proves the component root
never mounted.

For the `<RouteGuard>` HOC pattern, the redirect fires after the actor query
resolves. The timing assertion is the same (`waitForURL`) but the component may
briefly render the `skeleton` prop before the redirect. If a skeleton is
declared, also assert `await expect(page.locator("#property-create-form")).not.toBeVisible()`.

---

## 6. Cascade-Removal Migration Pattern

Downstream products commonly have manual redirect cascades in their React
components:

```tsx
// BEFORE: manual cascade — replicate this in ~17 sites in hostpoint-app
function PropertyCreatePage() {
  const { actor, isLoading } = useCurrentUser();
  const navigate = useNavigate();

  useEffect(() => {
    if (!isLoading && !actor) navigate("/sign-in", { replace: true });
    if (!isLoading && actor && actor.role !== "host") navigate("/explore", { replace: true });
  }, [actor, isLoading, navigate]);

  if (isLoading) return <Spinner />;
  if (!actor || actor.role !== "host") return null;

  return <PropertyCreateScreen />;
}
```

The canonical replacement shape:

**Step 1 — Add audience policy guard in the `.lzx` surface file.**

```lazuli
surface property web
  uses experience property

  audience host
    policy @policy.host_only
      on_unauthenticated redirect "/sign-in"
      on_unauthorized redirect "/explore"

    view create Form
      submit property.command.create
```

**Step 2 — Regenerate.**

```bash
lazuli generate ts --frontend host
```

**Step 3a — TanStack Router sites (preferred for new code).**

```tsx
// AFTER: TanStack beforeLoad
import { tanstackBeforeLoadGuard } from "@lazuli/runtime/react/tanstack";
import { redirect } from "@tanstack/react-router";
import { propertyCreateRoute } from "../../dist/ts-host/property/property.web.host.gen.js";

export const propertyCreateFileRoute = createFileRoute("/host/properties/new")({
  beforeLoad: tanstackBeforeLoadGuard(client, {
    policy: propertyCreateRoute.policy,
    onUnauthenticated: propertyCreateRoute.onUnauthenticated ?? "/sign-in",
    onUnauthorized: propertyCreateRoute.onUnauthorized ?? "/explore",
    redirect,
  }),
  component: PropertyCreateScreen, // no guard logic inside the component
});
```

**Step 3b — HOC sites (for CSR components not yet on TanStack Router).**

```tsx
// AFTER: RouteGuard HOC
import { RouteGuard } from "@lazuli/runtime/react";
import { propertyCreateRoute } from "../../dist/ts-host/property/property.web.host.gen.js";

export function PropertyCreatePage() {
  return (
    <RouteGuard
      policy={propertyCreateRoute.policy}
      onUnauthenticated={propertyCreateRoute.onUnauthenticated}
      onUnauthorized={propertyCreateRoute.onUnauthorized}
      skeleton={<Spinner />}
    >
      <PropertyCreateScreen />
    </RouteGuard>
  );
}
// PropertyCreateScreen: remove isLoading + actor checks + useEffect redirect.
```

**Step 4 — Verify with Doctor.**

```bash
lazuli doctor .
# expect: 0 ROUTE-GUARD-001 errors for migrated routes
```

**Step 5 — Run the Playwright e2e recipe** (§5) for each migrated route.

### Migration checklist (per site)

- [ ] Add audience `policy` block to `.lzx` surface file.
- [ ] Run `lazuli generate ts`.
- [ ] Replace `useEffect` redirect + `isLoading` + role check with HOC or `beforeLoad`.
- [ ] Remove in-component `useCurrentUser` / `useNavigate` guard logic.
- [ ] `lazuli doctor` — zero ROUTE-GUARD-001/002/003 on this route.
- [ ] Playwright spec passes (§5).

---

## 7. Cross-links

- **Auth guide**: `docs/auth-guide.md §Route guards` — covers `actor_query`,
  `route_guard` app block syntax, and OAuth interaction.
- **Grammar**: `docs/grammar.lzx.md §4` (`view_guard_decl`, `route_guard_redirect`
  EBNF), `§5` (`audience_body` with optional guard).
- **Canonical semantics**: `docs/canonical-semantics.md §View-level policy`,
  `§Audience-level policy`, `§App-level route_guard block`, `§Resolution chain`.
- **Invariants**: `docs/invariants.md §Source And Derived Views` — `lazuli check`
  is file-local; `lazuli doctor` enforces audience reachability and the
  guard-vs-backend parity rule.
- **Runtime React exports** (`@lazuli/runtime/react`):
  - `RouteGuard` — HOC component.
  - `useActor` — hydrates the current actor.
  - `evaluatePolicy` — policy verdict (useful for unit tests).
  - `LazuliRouteGuardPolicy`, `RouteGuardSpec`, `RouteGuardRegistry` — TS types.
- **Runtime TanStack entrypoint** (`@lazuli/runtime/react/tanstack`):
  - `tanstackBeforeLoadGuard` — generates a `beforeLoad` handler.
  - Import separately; this module peers on `@tanstack/react-router`.
