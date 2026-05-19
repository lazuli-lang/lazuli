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
