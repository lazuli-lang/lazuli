// React bindings for the typed LazuliClient. Built on TanStack Query so
// every typed feature inherits caching, refetch-on-focus, optimistic
// updates, suspense boundaries, etc., without ad-hoc wiring.
//
//   const list = useLazuliQuery(listCustomers, {});
//   const create = useLazuliCommand(createCustomer);
//   await create.mutateAsync({ name, email });
//
// The hooks read the ambient client from `<LazuliProvider>`, which in turn
// reads it from React context. Apps that don't use the provider can pass
// the client directly via `options.client`.

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationOptions,
  type UseMutationResult,
  type UseQueryOptions,
  type UseQueryResult,
} from "@tanstack/react-query";
import { createContext, createElement, useContext, type ReactNode } from "react";

import { LazuliClient } from "./client.js";
import type { CommandSpec, QuerySpec } from "./spec.js";

export {
  canonicalizeSearch,
  parseSegments,
  useDrawerSubView,
  useFilterState,
  useLocalSetting,
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

const LazuliClientContext = createContext<LazuliClient | null>(null);

export interface LazuliProviderProps {
  client: LazuliClient;
  children: ReactNode;
}

export function LazuliProvider({ client, children }: LazuliProviderProps) {
  return createElement(LazuliClientContext.Provider, { value: client }, children);
}

export function useLazuliClient(override?: LazuliClient): LazuliClient {
  if (override) {
    return override;
  }
  const ctx = useContext(LazuliClientContext);
  if (!ctx) {
    throw new Error(
      "useLazuliClient: no LazuliClient in context. Wrap the app in <LazuliProvider client={...}>.",
    );
  }
  return ctx;
}

// queryKeyFor builds the TanStack Query cache key for a typed query.
// Splitting the prefix `["lazuli", spec.name]` from the args lets command
// hooks invalidate a whole query (across every args set) by passing
// `{ queryKey: ["lazuli", spec.name] }`.
export function queryKeyFor(spec: QuerySpec<unknown, unknown>, args: unknown): unknown[] {
  return ["lazuli", spec.name, args];
}

export type UseLazuliQueryOptions<Args, Result> = Omit<
  UseQueryOptions<Result, Error, Result, unknown[]>,
  "queryKey" | "queryFn"
> & {
  client?: LazuliClient;
};

export function useLazuliQuery<Args, Result>(
  spec: QuerySpec<Args, Result>,
  args: Args,
  options: UseLazuliQueryOptions<Args, Result> = {},
): UseQueryResult<Result, Error> {
  const { client: clientOverride, ...queryOptions } = options;
  const client = useLazuliClient(clientOverride);
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
  const client = useLazuliClient(clientOverride);
  const queryClient = useQueryClient();
  return useMutation<Output, Error, Input>({
    mutationFn: (input) => client.runCommand(spec, input),
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
