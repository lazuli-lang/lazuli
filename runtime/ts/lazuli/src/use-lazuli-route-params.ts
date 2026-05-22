import { useContext } from "react";

import { LazuliError } from "./error.js";
import { LazuliClientContext } from "./use-actor.js";

export type RouteParamParser<T> = (raw: Record<string, string>) => T | null;

export interface UseLazuliRouteParamsOptions {
  readonly redirectOnInvalid?: string;
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
  if (redirectTo) {
    const router = client?.router ?? null;
    if (!router) {
      throw new LazuliRouteParamsError(
        viewName,
        redirectTo,
        `useLazuliRouteParams(${viewName}): invalid params; redirect requested without a router`,
      );
    }
    void router.navigate({ to: redirectTo, replace: true });
    throw new LazuliRouteParamsError(viewName, redirectTo);
  }

  throw new LazuliRouteParamsError(viewName);
}
