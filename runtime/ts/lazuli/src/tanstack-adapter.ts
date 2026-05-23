import { redirect } from "@tanstack/react-router";

import type { LazuliClient } from "./client.js";
import { evaluatePolicy, type LazuliRouteGuardPolicy } from "./route-guard.js";

// TanStack Router + React-Query primitives re-exported so generated codegen
// (routes.gen.tsx, future router-bound emitters) can import everything via
// @lazuli/runtime/react/tanstack — keeping TanStack Router OUT of the
// default @lazuli/runtime/react surface (consumers without TanStack don't
// pay the bundle cost).
export {
  Link,
  Outlet,
  createRoute,
  createRouter,
  createRootRouteWithContext,
  lazyRouteComponent,
  redirect,
} from "@tanstack/react-router";
export type { LinkProps, NotFoundRouteComponent } from "@tanstack/react-router";
export type { QueryClient } from "@tanstack/react-query";

export type RouteGuardSpec<Component = unknown> = {
  readonly path?: string;
  readonly component?: Component;
  readonly policy: LazuliRouteGuardPolicy;
  readonly onUnauthenticated?: string | null;
  readonly onUnauthorized?: string | null;
  readonly substep?: string;
};

export function tanstackBeforeLoadGuard(client: LazuliClient, spec: RouteGuardSpec) {
  return async (): Promise<void> => {
    const actor = await client.resolveActor();
    const verdict = evaluatePolicy(actor, spec.policy);
    if (verdict === "unauthenticated") {
      throw redirect({ to: spec.onUnauthenticated ?? "/sign-in" });
    }
    if (verdict === "unauthorized") {
      throw redirect({ to: spec.onUnauthorized ?? "/forbidden" });
    }
  };
}
