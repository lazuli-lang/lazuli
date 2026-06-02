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
import { createElement, useCallback, useState, type ReactNode } from "react";

import { LazuliClient } from "./client.js";
import { LazuliError } from "./error.js";
import { lookupPreflight } from "./preflight.js";
import type { CommandSpec, QuerySpec } from "./spec.js";
import { LazuliClientContext, useLazuliClient as useLazuliClientImpl } from "./use-actor.js";
import { createUseLazuliAction } from "./use-lazuli-action.js";
import { createUseLazuliForm } from "./use-lazuli-form.js";

// Universal view-helper re-exports — same in `.web.ts` and `.native.ts`.
export {
  canonicalizeSearch,
  parseSegments,
  useFilterState,
  useMultiSelection,
  useViewMode,
  useTabGroup,
  useWizardSteps,
  useInlineTable,
  useBoard,
  useRepeatableGroup,
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
  type ViewModeState,
  type TabGroupCase,
  type TabGroupState,
  type WizardStepsState,
  type InlineTableCommand,
  type InlineTableState,
  type BoardConfig,
  type BoardLane,
  type BoardState,
  type RepeatableFieldDef,
  type RepeatableGroupConfig,
  type RepeatableGroupState,
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
  LazuliFlashAdapterContext,
  LazuliFlashAdapterProvider,
  type FlashAdapter,
  type FlashKind,
} from "./flash-adapter.js";
export {
  LazuliRouterAdapterContext,
  LazuliRouterAdapterProvider,
  type RouterAdapter,
} from "./router-adapter.js";
export {
  type UseLazuliActionOptions,
  type UseLazuliActionResult,
} from "./use-lazuli-action.js";
export {
  useLifecycleGate,
  withLifecycleGate,
  type LifecycleGateEvaluator,
  type LifecycleGateMetadata,
  type LifecycleVerdict,
} from "./lifecycle-gate.js";
export type { UseLazuliFormOptions, UseLazuliFormResult } from "./use-lazuli-form.js";
export {
  LazuliRouteParamsError,
  isLazuliRouteParamsError,
  useLazuliRouteParams,
  type RouteParamParser,
  type UseLazuliRouteParamsOptions,
} from "./use-lazuli-route-params.js";

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
  const { client: clientOverride, onSuccess: userOnSuccess, ...mutationOptions } = options;
  const client = useLazuliClientImpl(clientOverride);
  const queryClient = useQueryClient();
  return useMutation<Output, Error, Input>({
    mutationFn: (input) => {
      // LAZ-SEMANTIC-AUTO-VALIDATE — see react.web.ts for the rationale.
      const preflight = lookupPreflight(spec.name);
      if (preflight) {
        const result = preflight(input);
        if (!result.ok) {
          const data: Record<string, unknown> = { fields: result.fields };
          if (result.fieldMessageKeys && Object.keys(result.fieldMessageKeys).length > 0) {
            data.field_message_keys = result.fieldMessageKeys;
          }
          return Promise.reject(
            new LazuliError(400, {
              code: "validation_failed",
              message: "validation failed",
              data: data as never,
            }),
          );
        }
      }
      return client.runCommand(spec, input);
    },
    onSuccess: async (...args) => {
      // Invalidate every TanStack Query cache entry whose key starts with
      // `["lazuli", <invalidated query name>]`. This matches the server's
      // `c.Invalidates` evictions: any args-variant of that query refetches.
      await Promise.all(
        spec.invalidates.map((name) =>
          queryClient.invalidateQueries({ queryKey: ["lazuli", name] }),
        ),
      );
      if (userOnSuccess) {
        await (userOnSuccess as (...a: typeof args) => unknown)(...args);
      }
    },
    ...mutationOptions,
  });
}

export const useLazuliAction = createUseLazuliAction(useLazuliCommand);
export const useLazuliForm = createUseLazuliForm(useLazuliQuery, useLazuliCommand);

// React + react-query primitives re-exported (parity with react.web.ts) —
// generated code (cap_file hooks) imports via @lazuli/runtime/react.
// TanStack Router re-exports live in @lazuli/runtime/react/tanstack.
export { createElement, useCallback, useState };
export type { ComponentType, FunctionComponent, ReactElement } from "react";
export { useQueryClient };
// LAZ-SEMANTIC-AUTO-VALIDATE — preflight registry (parity with web).
export {
  registerPreflight,
  lookupPreflight,
  type PreflightFn,
  type PreflightFieldErrors,
  type PreflightResult,
} from "./preflight.js";
