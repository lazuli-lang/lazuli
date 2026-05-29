// Host web shell — TanStack Router wiring for route guards.
//
// Run `lazuli generate ts` first to emit `dist/ts-web-host/route_guards.gen.ts`.
// That file exports `routeGuardRegistry`, a typed object keyed by route path
// containing per-route policy metadata derived from the `audience host` block
// in `features/property/property.host.web.lzx`.
//
// Excerpt of the generated registry shape:
//
//   import type { RouteGuardRegistry } from "@lazuli/runtime/react";
//   export const routeGuardRegistry = {
//     defaults: {
//       policy: { atoms: [{ namespace: "scope", name: "authenticated" }] },
//       unauthenticated: "/sign-in",
//       unauthorized: null,
//       skeleton: null,
//     },
//     actorQuery: null,
//     routes: { ... },   // one entry per declared host route
//   } as const satisfies RouteGuardRegistry;

import { LazuliClient } from "@lazuli/runtime";
import {
  createRootRouteWithContext,
  createRoute,
  createRouter,
  redirect,
  withTanStackGuard,
} from "@lazuli/runtime/react/tanstack";
import type { RouteGuardRegistry } from "@lazuli/runtime/react";

// Replace with the real generated import after `lazuli generate ts`:
//   import { routeGuardRegistry } from "../../../dist/ts-web-host/route_guards.gen.js";
//
// Inline stub — mirrors the shape emitted when `audience host` carries
// `policy @policy.authenticated` with `on_unauthenticated redirect "/sign-in"`.
const routeGuardRegistry = {
  defaults: {
    policy: { atoms: [{ namespace: "scope" as const, name: "authenticated" }] },
    unauthenticated: "/sign-in",
    unauthorized: null,
    skeleton: null,
  },
  actorQuery: null,
  routes: {},
} as const satisfies RouteGuardRegistry;

export { routeGuardRegistry };

// ---------------------------------------------------------------------------
// Example TanStack Router setup consuming the registry.
// ---------------------------------------------------------------------------

const baseUrl = process.env.VITE_API_URL ?? "http://localhost:8080";
export const client = new LazuliClient({ baseUrl });

type RouterContext = { client: LazuliClient };

const rootRoute = createRootRouteWithContext<RouterContext>()({});

// Wire a host-audience route using withTanStackGuard so the policy from
// routeGuardRegistry.defaults is enforced before the component renders.
const hostPropertiesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/host/properties",
  beforeLoad: withTanStackGuard(
    {} as RouterContext,
    routeGuardRegistry.defaults.policy,
    {
      onUnauthenticated: routeGuardRegistry.defaults.unauthenticated,
      onUnauthorized: routeGuardRegistry.defaults.unauthorized ?? undefined,
      redirect,
    },
  ),
});

export const router = createRouter({
  routeTree: rootRoute.addChildren([hostPropertiesRoute]),
  context: { client },
});
