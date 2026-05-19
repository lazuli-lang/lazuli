import type { LazuliClient } from "./client.js";
import type { PolicyAtom } from "./spec.js";
export type LazuliActor = {
  readonly id?: string; readonly role?: string; readonly roles?: readonly string[];
  readonly kind?: string; readonly actor?: string;
  readonly orgId?: string | null; readonly tenantId?: string | null;
} & Record<string, unknown>;
export type LazuliRouteGuardPolicy = {
  readonly atoms: readonly PolicyAtom[];
  readonly name?: string | null;
};
export type RouteGuardVerdict = "authorized" | "unauthenticated" | "unauthorized";
export function evaluatePolicy(
  actor: LazuliActor | null,
  policy: LazuliRouteGuardPolicy,
): RouteGuardVerdict {
  if (policy.atoms.length === 0) return "authorized";
  for (const atom of policy.atoms) {
    if (matches(actor, atom)) return "authorized";
  }
  return actor === null ? "unauthenticated" : "unauthorized";
}
// TanStack Router beforeLoad adapter — caller passes `redirect` from
// `@tanstack/react-router` so this module stays free of that dependency
// (consumers without TanStack Router don't pay the bundle cost). The
// router context must carry the LazuliClient on `context.client`.
export type WithTanStackGuardOptions = {
  readonly onUnauthenticated?: string | null;
  readonly onUnauthorized?: string | null;
  // Caller passes `redirect` from @tanstack/react-router. Returning the
  // sentinel object is what TanStack Router treats as a navigation
  // signal when thrown from beforeLoad.
  readonly redirect: (params: { to: string }) => unknown;
};
export function withTanStackGuard<TContext extends { client: LazuliClient }>(
  _phantomContext: Partial<TContext>,
  policy: LazuliRouteGuardPolicy,
  options: WithTanStackGuardOptions,
): (params: { context: TContext }) => Promise<void> {
  return async ({ context }) => {
    const actor = await context.client.resolveActor();
    const verdict = evaluatePolicy(actor, policy);
    if (verdict === "unauthenticated") {
      throw options.redirect({ to: options.onUnauthenticated ?? "/sign-in" });
    }
    if (verdict === "unauthorized") {
      throw options.redirect({ to: options.onUnauthorized ?? "/forbidden" });
    }
  };
}
function matches(actor: LazuliActor | null, atom: PolicyAtom): boolean {
  if (atom.namespace === "scope") {
    if (atom.name === "public") return true;
    if (atom.name === "authenticated") return actor !== null;
    if (atom.name === "same_org") return !!(actor?.orgId ?? actor?.tenantId);
    return false;
  }
  if (atom.namespace === "role") {
    return actor !== null && (actor.role === atom.name || actor.roles?.includes(atom.name) === true);
  }
  if (atom.namespace === "actor") {
    const kind = actor?.kind ?? actor?.actor ?? (actor === null ? "anonymous" : "user");
    return kind === atom.name;
  }
  return false;
}
