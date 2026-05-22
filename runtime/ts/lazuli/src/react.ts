// PUBLIC TYPE CONTRACT for `@lazuli/runtime/react`.
//
// This file is the `types` branch of the `./react` exports map in
// `package.json`. Both `react.web.ts` and `react.native.ts` must
// re-export every name listed here so the consumer sees a uniform
// surface regardless of platform. The runtime body lives in those
// concrete entrypoints; this file declares the contract that both
// implementations satisfy.
//
// Constraint: this file must NOT import `react-native` modules even
// at type position. Web consumers running `tsc --noEmit` without RN
// installed should typecheck cleanly. Inline type aliases (or
// declarations of the public surface) replace any would-be RN imports.
//
// See `docs/proposals/mobile-target.md` §3.3 + §3.5 for the rationale.

import type {
  UseMutationOptions,
  UseMutationResult,
  UseQueryOptions,
  UseQueryResult,
} from "@tanstack/react-query";
import type { ComponentType, ReactNode } from "react";

import type { LazuliClient, LazuliFlash, LazuliRouter } from "./client.js";
import type {
  LifecycleGateEvaluator,
  LifecycleGateMetadata,
  LifecycleVerdict,
} from "./lifecycle-gate.js";
import type {
  LazuliActor,
  LazuliRouteGuardPolicy,
  RouteGuardVerdict,
} from "./route-guard.js";
import type { CommandSpec, QuerySpec } from "./spec.js";
import type { DrawerConfig, DrawerSubView } from "./view-helpers.js";

// Universal view-helper types and functions are re-exported as-is from
// `view-helpers.ts`; they are pure React + JS (no DOM, no RN) and so
// both entrypoints can re-export them identically through this file.
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

// --- LazuliProvider / client hooks ----------------------------------------

export interface LazuliProviderProps {
  client: LazuliClient;
  children: ReactNode;
}

export declare function LazuliProvider(props: LazuliProviderProps): ReactNode;

export declare function useLazuliClient(override?: LazuliClient): LazuliClient;

/**
 * Build the TanStack Query cache key for a typed query. Splitting the
 * prefix `["lazuli", spec.name]` from the args lets command hooks
 * invalidate a whole query (across every args set) by passing
 * `{ queryKey: ["lazuli", spec.name] }`.
 */
export declare function queryKeyFor(
  spec: QuerySpec<unknown, unknown>,
  args: unknown,
): unknown[];

export type UseLazuliQueryOptions<Result> = Omit<
  UseQueryOptions<Result, Error, Result, unknown[]>,
  "queryKey" | "queryFn"
> & {
  client?: LazuliClient;
};

export declare function useLazuliQuery<Args, Result>(
  spec: QuerySpec<Args, Result>,
  args: Args,
  options?: UseLazuliQueryOptions<Result>,
): UseQueryResult<Result, Error>;

export type UseLazuliCommandOptions<Input, Output> = Omit<
  UseMutationOptions<Output, Error, Input>,
  "mutationFn"
> & {
  client?: LazuliClient;
};

export declare function useLazuliCommand<Input, Output>(
  spec: CommandSpec<Input, Output>,
  options?: UseLazuliCommandOptions<Input, Output>,
): UseMutationResult<Output, Error, Input>;

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

export declare function useLazuliAction<Input, Output>(
  spec: CommandSpec<Input, Output>,
  options?: UseLazuliActionOptions<Input, Output>,
): UseMutationResult<Output, Error, Input>;

// --- Route guards ---------------------------------------------------------

export type {
  LazuliActor,
  LazuliRouteGuardPolicy,
  RouteGuardVerdict,
};

export type RouteGuardSpec<Component = unknown> = {
  readonly path?: string;
  readonly component?: Component;
  readonly policy: LazuliRouteGuardPolicy;
  readonly onUnauthenticated?: string | null;
  readonly onUnauthorized?: string | null;
};

export type RouteGuard = RouteGuardSpec<unknown>;
export type ActorQueryRef = string;
export type RouteGuardRegistry = {
  readonly defaults: {
    readonly policy: LazuliRouteGuardPolicy | null;
    readonly unauthenticated: string | null;
    readonly unauthorized: string | null;
    readonly skeleton: string | null;
  };
  readonly actorQuery: ActorQueryRef | null;
  readonly routes: Readonly<Record<string, RouteGuard>>;
};

/**
 * TanStack Router beforeLoad support is intentionally split out at
 * `@lazuli/runtime/react/tanstack` so plain React imports do not load
 * `@tanstack/react-router`.
 */

export type UseActorResult = {
  readonly data: LazuliActor | null;
  readonly isLoading: boolean;
  readonly isError: boolean;
};

export declare function evaluatePolicy(
  actor: LazuliActor | null,
  policy: LazuliRouteGuardPolicy,
): RouteGuardVerdict;

export type WithTanStackGuardOptions = {
  readonly onUnauthenticated?: string | null;
  readonly onUnauthorized?: string | null;
  readonly redirect: (params: { to: string }) => unknown;
};

/**
 * TanStack Router beforeLoad guard. Caller passes the `redirect` import
 * from `@tanstack/react-router` so `@lazuli/runtime/react` itself does
 * NOT take a hard dependency on the router (consumers without TanStack
 * don't pay the bundle cost). The router context must carry the
 * LazuliClient on `context.client`.
 */
export declare function withTanStackGuard<TContext extends { client: LazuliClient }>(
  phantomContext: Partial<TContext>,
  policy: LazuliRouteGuardPolicy,
  options: WithTanStackGuardOptions,
): (params: { context: TContext }) => Promise<void>;

export declare function useActor(): UseActorResult;

export interface RouteGuardProps {
  readonly policy: LazuliRouteGuardPolicy;
  readonly onUnauthenticated?: string | null;
  readonly onUnauthorized?: string | null;
  readonly children: ReactNode;
  readonly skeleton?: ReactNode;
  readonly router?: LazuliRouter | null;
}

export declare function RouteGuard(props: RouteGuardProps): ReactNode;

export type RouteGuardOptions = Omit<RouteGuardProps, "children">;

export declare function useRouteGuardSkeleton(): ReactNode;

export declare function withRouteGuard<P extends object>(
  Component: ComponentType<P>,
  guard: RouteGuardOptions,
): ComponentType<P>;

// --- Lifecycle route gates -----------------------------------------------

export type {
  LifecycleGateEvaluator,
  LifecycleGateMetadata,
  LifecycleVerdict,
};

export declare function useLifecycleGate(
  metadata: LifecycleGateMetadata,
  evaluator: LifecycleGateEvaluator,
): { verdict: LifecycleVerdict | "hydrating" };

export declare function withLifecycleGate<P extends object>(
  Component: ComponentType<P>,
  metadata: LifecycleGateMetadata,
  evaluator: LifecycleGateEvaluator,
): ComponentType<P>;

// --- Platform-split hooks --------------------------------------------------

/**
 * Returns the persisted setting, falling back to `defaultValue`.
 *
 * IMPORTANT — first-render contract differs by platform:
 *   - Web: returns the persisted value synchronously on first render
 *     (uses useSyncExternalStore + localStorage).
 *   - Native: returns `defaultValue` on first render; the persisted
 *     value becomes visible on the next render after AsyncStorage
 *     resolves.
 *
 * Callers MUST treat the value as eventually-consistent. Code that
 * depends on the persisted value being present on the first frame
 * (e.g., to pick a theme before paint) must read storage explicitly
 * via the platform's lower-level API, not via this hook.
 */
export declare function useLocalSetting<T>(
  key: string,
  defaultValue: T,
): [T, (next: T) => void];

/**
 * Drawer state machine. Auto-closes on pathname change, on delete
 * success, on missing item, and on a platform-specific dismissal
 * gesture:
 *   - Web: the `Escape` keyboard key.
 *   - Native: the Android hardware back button (iOS swipe-back is
 *     handled by Expo Router for navigated stack screens; `<Modal>`
 *     consumers wire `onRequestClose` to `close()` themselves).
 */
export declare function useDrawerSubView<TInput, TItem>(
  config?: DrawerConfig<TInput, TItem>,
): DrawerSubView<TInput, TItem>;
