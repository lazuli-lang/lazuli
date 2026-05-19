import { createElement, useEffect, type ComponentType, type ReactNode } from "react";

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

export type RouteGuardOptions = Omit<RouteGuardProps, "children">;

export function useRouteGuardSkeleton(): ReactNode {
  return null;
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
  const fallbackSkeleton = useRouteGuardSkeleton();
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

  if (actor.isLoading) return skeleton ?? fallbackSkeleton;
  if (to) {
    if (!router) throw new Error("RouteGuard: redirect requested without a router.");
    return null;
  }
  return <>{children}</>;
}

export function withRouteGuard<P extends object>(
  Component: ComponentType<P>,
  guard: RouteGuardOptions,
): ComponentType<P> {
  return function RouteGatedComponent(props: P) {
    return (
      <RouteGuard
        policy={guard.policy}
        onUnauthenticated={guard.onUnauthenticated ?? null}
        onUnauthorized={guard.onUnauthorized ?? null}
        skeleton={guard.skeleton ?? null}
        router={guard.router ?? null}
      >
        {createElement(Component, props)}
      </RouteGuard>
    );
  };
}

function useRouter(router?: LazuliRouter | null): LazuliRouter | null {
  return router ?? useLazuliClient().router;
}
