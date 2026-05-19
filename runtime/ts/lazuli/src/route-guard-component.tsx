import { useEffect, type ReactNode } from "react";

import type { LazuliRouter } from "./client.js";
import { evaluatePolicy, type LazuliRouteGuardPolicy } from "./route-guard.js";
import { useActor, useLazuliClient } from "./use-actor.js";

export interface RouteGuardProps {
  readonly policy: LazuliRouteGuardPolicy;
  readonly onUnauthenticated?: string | null;
  readonly onUnauthorized?: string | null;
  readonly children: ReactNode;
  readonly skeleton?: ReactNode;
  readonly router?: LazuliRouter | null;
}

export function RouteGuard({
  policy,
  onUnauthenticated = null,
  onUnauthorized = null,
  children,
  skeleton = null,
  router: routerOverride,
}: RouteGuardProps): ReactNode {
  const actor = useActor();
  const router = useRouter(routerOverride);
  const verdict = actor.isLoading ? "authorized" : evaluatePolicy(actor.data, policy);
  const to =
    verdict === "unauthenticated"
      ? onUnauthenticated
      : verdict === "unauthorized"
        ? onUnauthorized
        : null;

  useEffect(() => {
    if (to) void router?.navigate({ to, replace: true });
  }, [router, to]);

  if (actor.isLoading) return skeleton;
  if (to) {
    if (!router) throw new Error("RouteGuard: redirect requested without a router.");
    return null;
  }
  return <>{children}</>;
}

function useRouter(router?: LazuliRouter | null): LazuliRouter | null {
  return router ?? useLazuliClient().router;
}
