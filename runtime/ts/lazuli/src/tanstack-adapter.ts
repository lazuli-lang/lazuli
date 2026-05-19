import { redirect } from "@tanstack/react-router";

import type { LazuliClient } from "./client.js";
import { evaluatePolicy, type LazuliRouteGuardPolicy } from "./route-guard.js";

export type RouteGuardSpec<Component = unknown> = {
  readonly path?: string;
  readonly component?: Component;
  readonly policy: LazuliRouteGuardPolicy;
  readonly onUnauthenticated?: string | null;
  readonly onUnauthorized?: string | null;
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
