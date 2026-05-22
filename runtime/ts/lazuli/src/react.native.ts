// Concrete NATIVE entrypoint for `@lazuli/runtime/react`.
//
// Resolved by Metro/`tsc` via the `react-native` condition of the
// `./react` exports map. Re-exports every name from `react.ts` (the
// type contract) plus the native bodies of the platform-split hooks.
// The universal hook bodies are identical across `.web.ts` and
// `.native.ts`; each entrypoint embeds its own copy so the per-target
// resolved module is fully self-contained.
//
// See `docs/proposals/mobile-target.md` §3.1 + §3.3.

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationOptions,
  type UseMutationResult,
  type UseQueryOptions,
  type UseQueryResult,
} from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";

import { LazuliClient, type LazuliFlash } from "./client.js";
import type { CommandSpec, QuerySpec } from "./spec.js";
import { LazuliClientContext, useLazuliClient as useLazuliClientImpl } from "./use-actor.js";

// Universal view-helper re-exports — same in `.web.ts` and `.native.ts`.
export {
  canonicalizeSearch,
  parseSegments,
  useFilterState,
  useMultiSelection,
  type DrawerConfig,
  type DrawerSubView,
  type FilterConfig,
  type FilterState,
  type FilterStates,
  type MultiFilterState,
  type MultiSelection,
  type ParsedSegment,
  type SetUrlParams,
  type UrlParams,
} from "./view-helpers.js";

// Platform-split hook bodies (native variants).
export { useLocalSetting } from "./local-setting.native.js";
export { useDrawerSubView } from "./drawer-sub-view.native.js";
export {
  evaluatePolicy,
  withTanStackGuard,
  type LazuliActor,
  type LazuliRouteGuardPolicy,
  type RouteGuardVerdict,
  type WithTanStackGuardOptions,
} from "./route-guard.js";
export {
  RouteGuard,
  useRouteGuardSkeleton,
  withRouteGuard,
  type RouteGuardOptions,
  type RouteGuardProps,
} from "./route-guard-component.js";
export { useActor, useLazuliClient, type UseActorResult } from "./use-actor.js";
export {
  useLifecycleGate,
  withLifecycleGate,
  type LifecycleGateEvaluator,
  type LifecycleGateMetadata,
  type LifecycleVerdict,
} from "./lifecycle-gate.js";

// --- Universal client/query/command hooks --------------------------------

export interface LazuliProviderProps {
  client: LazuliClient;
  children: ReactNode;
}

export function LazuliProvider({ client, children }: LazuliProviderProps) {
  return createElement(LazuliClientContext.Provider, { value: client }, children);
}

export function queryKeyFor(spec: QuerySpec<unknown, unknown>, args: unknown): unknown[] {
  return ["lazuli", spec.name, args];
}

export type UseLazuliQueryOptions<Result> = Omit<
  UseQueryOptions<Result, Error, Result, unknown[]>,
  "queryKey" | "queryFn"
> & {
  client?: LazuliClient;
};

export function useLazuliQuery<Args, Result>(
  spec: QuerySpec<Args, Result>,
  args: Args,
  options: UseLazuliQueryOptions<Result> = {},
): UseQueryResult<Result, Error> {
  const { client: clientOverride, ...queryOptions } = options;
  const client = useLazuliClientImpl(clientOverride);
  return useQuery<Result, Error, Result, unknown[]>({
    queryKey: queryKeyFor(spec, args),
    queryFn: () => client.runQuery(spec, args),
    ...queryOptions,
  });
}

export type UseLazuliCommandOptions<Input, Output> = Omit<
  UseMutationOptions<Output, Error, Input>,
  "mutationFn"
> & {
  client?: LazuliClient;
};

export function useLazuliCommand<Input, Output>(
  spec: CommandSpec<Input, Output>,
  options: UseLazuliCommandOptions<Input, Output> = {},
): UseMutationResult<Output, Error, Input> {
  return useLazuliAction(spec, options);
}

export type LazuliActionOptions = {
  readonly back?: boolean;
  readonly redirect?: string;
  readonly flash?: LazuliFlash;
  readonly invalidates?: readonly string[];
  readonly replace?: boolean;
};

export type UseLazuliActionOptions<Input, Output> =
  UseLazuliCommandOptions<Input, Output> & LazuliActionOptions;

export type UseLazuliActionOptionsFor<Spec> =
  Spec extends CommandSpec<infer Input, infer Output>
    ? UseLazuliActionOptions<Input, Output>
    : never;

export function useLazuliAction<Input, Output>(
  spec: CommandSpec<Input, Output>,
  options: UseLazuliActionOptions<Input, Output> = {},
): UseMutationResult<Output, Error, Input> {
  const {
    client: clientOverride,
    onSuccess: userOnSuccess,
    back,
    redirect,
    flash,
    invalidates = [],
    replace,
    ...mutationOptions
  } = options;
  const client = useLazuliClientImpl(clientOverride);
  const queryClient = useQueryClient();
  return useMutation<Output, Error, Input>({
    mutationFn: (input) => client.runCommand(spec, input),
    onSuccess: async (...args) => {
      // Invalidate every TanStack Query cache entry whose key starts with
      // `["lazuli", <invalidated query name>]`. This matches the server's
      // `c.Invalidates` evictions: any args-variant of that query refetches.
      await Promise.all(
        uniqueStrings([...spec.invalidates, ...invalidates]).map((name) =>
          queryClient.invalidateQueries({ queryKey: ["lazuli", name] }),
        ),
      );
      if (flash) {
        client.onFlash?.(flash);
      }
      if (redirect) {
        client.router?.navigate({
          to: interpolateResultPath(redirect, args[0]),
          ...(replace ? { replace: true } : {}),
        });
      } else if (back) {
        client.router?.history?.back();
      }
      if (userOnSuccess) {
        await (userOnSuccess as (...a: typeof args) => unknown)(...args);
      }
    },
    ...mutationOptions,
  });
}

function uniqueStrings(values: readonly string[]): string[] {
  return Array.from(new Set(values));
}

function interpolateResultPath(template: string, result: unknown): string {
  return template.replace(/\{result\.([A-Za-z0-9_.]+)\}/g, (_match, path: string) => {
    const value = readResultPath(result, path);
    return value === undefined || value === null ? "" : encodeURIComponent(String(value));
  });
}

function readResultPath(value: unknown, path: string): unknown {
  let current = value;
  for (const segment of path.split(".")) {
    if (current === null || typeof current !== "object") {
      return undefined;
    }
    current = (current as Record<string, unknown>)[segment];
  }
  return current;
}
