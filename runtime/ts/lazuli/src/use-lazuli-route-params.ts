import { useContext } from "react";

import { LazuliError } from "./error.js";
import { LazuliClientContext } from "./use-actor.js";

export type RouteParamParser<T> = (raw: Record<string, string>) => T | null;

export interface UseLazuliRouteParamsOptions {
  readonly redirectOnInvalid?: string;
  /**
   * Wave §1 (2026-05-23): when true, the hook NEVER navigates from
   * inside render — it only parses and throws. Invalid params throw
   * `LazuliRouteParamsError` carrying the requested `redirectTo`; the
   * router-level `beforeLoad` (or a parent error boundary) is
   * responsible for the actual navigation BEFORE the screen mounts.
   *
   * Recommended for new code; will become the default once every
   * codegen-emitted `beforeLoad` adapter ships the invalid-param
   * redirect path. Today defaults to `false` (legacy navigate-from-
   * render behavior) to keep existing pilots green.
   */
  readonly pure?: boolean;
}

export class LazuliRouteParamsError extends LazuliError {
  readonly viewName: string;
  readonly redirectTo: string | undefined;

  constructor(viewName: string, redirectTo?: string, message?: string) {
    super(404, {
      code: "not_found",
      message: message ?? `useLazuliRouteParams(${viewName}): invalid params`,
      data: {
        view_name: viewName,
        ...(redirectTo ? { redirect_to: redirectTo } : {}),
      },
    });
    this.name = "LazuliRouteParamsError";
    this.viewName = viewName;
    this.redirectTo = redirectTo;
  }
}

export function isLazuliRouteParamsError(err: unknown): err is LazuliRouteParamsError {
  return err instanceof LazuliRouteParamsError;
}

export function useLazuliRouteParams<T>(
  viewName: string,
  parser: RouteParamParser<T>,
  raw: Record<string, string>,
  options: UseLazuliRouteParamsOptions = {},
): T {
  const client = useContext(LazuliClientContext);
  const parsed = parser(raw);
  if (parsed !== null) return parsed;

  const redirectTo = options.redirectOnInvalid;

  // Wave §1 purity mode — parse-or-throw, never navigate from render.
  // The error carries the requested redirectTo; the router's
  // `beforeLoad` adapter (or a parent error boundary) is responsible
  // for the actual navigation BEFORE the screen mounts.
  if (options.pure) {
    throw new LazuliRouteParamsError(viewName, redirectTo);
  }

  if (redirectTo) {
    const router = client?.router ?? null;
    if (!router) {
      throw new LazuliRouteParamsError(
        viewName,
        redirectTo,
        `useLazuliRouteParams(${viewName}): invalid params; redirect requested without a router`,
      );
    }
    // Legacy path — navigate from render. Will be removed once every
    // codegen-emitted `beforeLoad` adapter ships the invalid-param
    // redirect (wave §1 / §2 codegen follow-up). New code should
    // opt into `pure: true`.
    void router.navigate({ to: redirectTo, replace: true });
    throw new LazuliRouteParamsError(viewName, redirectTo);
  }

  throw new LazuliRouteParamsError(viewName);
}
