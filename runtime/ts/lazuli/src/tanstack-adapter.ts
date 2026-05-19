import type { LazuliClient } from "./client.js";
import { evaluatePolicy, type LazuliRouteGuardPolicy } from "./route-guard.js";

export type TanStackRedirect = (opts: { to: string }) => unknown;

export type TanStackGuardRedirects = {
  readonly onUnauthenticated?: string | null;
  readonly onUnauthorized?: string | null;
  readonly redirect: TanStackRedirect;
};

export type TanStackGuardContext = { readonly context: { readonly client: LazuliClient } };

export function withTanStackGuard(
  routeOpts: { readonly beforeLoad?: (ctx: TanStackGuardContext) => unknown },
  policy: LazuliRouteGuardPolicy,
  redirects: TanStackGuardRedirects,
) {
  return async (ctx: TanStackGuardContext): Promise<void> => {
    await routeOpts.beforeLoad?.(ctx);
    const actor = await ctx.context.client.resolveActor();
    const verdict = evaluatePolicy(actor, policy);
    const to =
      verdict === "unauthenticated"
        ? redirects.onUnauthenticated
        : verdict === "unauthorized"
          ? redirects.onUnauthorized
          : null;
    if (to) throw redirects.redirect({ to });
  };
}
