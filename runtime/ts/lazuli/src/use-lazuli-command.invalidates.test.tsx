import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";

import { LazuliClient } from "./client.js";
import { LazuliProvider, useLazuliCommand } from "./react.web.js";
import { defineCommand } from "./spec.js";

// Verifies that `useLazuliCommand`'s `onSuccess` handler iterates over EVERY
// entry in `spec.invalidates` and fires one `queryClient.invalidateQueries`
// call per entry, with `queryKey: ["lazuli", <entry>]`. Historically every
// emitted SDK had `invalidates: []`, so this path had never been exercised
// with a non-empty array — Cell A7 of the codegen-correctness cycle.

interface InvalidateCall {
  queryKey: readonly unknown[];
}

function makeHarness(invalidates: readonly string[]) {
  const spec = defineCommand<Record<string, never>, { ok: boolean }>("feat.do_thing", {
    invalidates,
  });

  const fetchImpl: typeof globalThis.fetch = async () =>
    new Response(JSON.stringify({ ok: true }), { status: 200 });
  const client = new LazuliClient({ baseUrl: "http://runtime.test", fetch: fetchImpl });

  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const calls: InvalidateCall[] = [];
  const originalInvalidate = queryClient.invalidateQueries.bind(queryClient);
  vi.spyOn(queryClient, "invalidateQueries").mockImplementation(async (filters) => {
    calls.push({ queryKey: (filters?.queryKey ?? []) as readonly unknown[] });
    return originalInvalidate(filters);
  });

  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>
      <LazuliProvider client={client}>{children}</LazuliProvider>
    </QueryClientProvider>
  );

  return { spec, wrapper, calls };
}

describe("useLazuliCommand — invalidates", () => {
  it("fires one invalidateQueries call per entry when invalidates has many", async () => {
    const invalidates = ["feat.lookup_my_x", "feat.list_my_xs"] as const;
    const { spec, wrapper, calls } = makeHarness(invalidates);

    const { result } = renderHook(() => useLazuliCommand(spec), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({});
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(calls).toHaveLength(invalidates.length);
    expect(calls.map((c) => c.queryKey)).toEqual([
      ["lazuli", "feat.lookup_my_x"],
      ["lazuli", "feat.list_my_xs"],
    ]);
  });

  it("fires exactly one invalidateQueries call for a single-entry invalidates", async () => {
    const { spec, wrapper, calls } = makeHarness(["feat.only_one"]);

    const { result } = renderHook(() => useLazuliCommand(spec), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({});
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(calls).toEqual([{ queryKey: ["lazuli", "feat.only_one"] }]);
  });

  it("does not call invalidateQueries when invalidates is empty", async () => {
    const { spec, wrapper, calls } = makeHarness([]);

    const { result } = renderHook(() => useLazuliCommand(spec), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({});
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(calls).toEqual([]);
  });

  it("still invokes the caller's onSuccess after invalidations resolve", async () => {
    const { spec, wrapper, calls } = makeHarness(["feat.a", "feat.b"]);
    const onSuccess = vi.fn();

    const { result } = renderHook(() => useLazuliCommand(spec, { onSuccess }), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({});
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(calls).toHaveLength(2);
    expect(onSuccess).toHaveBeenCalledTimes(1);
  });
});
