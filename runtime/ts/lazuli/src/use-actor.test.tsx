import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it } from "vitest";

import { LazuliClient } from "./client.js";
import { LazuliProvider, useActor } from "./react.web.js";
import { defineQuery } from "./spec.js";
import type { LazuliActor } from "./route-guard.js";

describe("useActor", () => {
  it("returns actor data from the configured actor query", async () => {
    const actorQuery = defineQuery<Record<string, never>, LazuliActor>("account.query.me");
    const fetchImpl: typeof globalThis.fetch = async (input, init) => {
      expect(String(input)).toBe("http://runtime.test/api/v1/q/account.query.me");
      expect(init?.method).toBe("POST");
      return new Response(JSON.stringify({ id: "usr_1", role: "host" }), { status: 200 });
    };
    const client = new LazuliClient({ baseUrl: "http://runtime.test", fetch: fetchImpl, actorQuery });
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>
        <LazuliProvider client={client}>{children}</LazuliProvider>
      </QueryClientProvider>
    );

    const { result } = renderHook(() => useActor(), { wrapper });

    expect(result.current.isLoading).toBe(true);
    await waitFor(() => expect(result.current.data).toEqual({ id: "usr_1", role: "host" }));
    expect(result.current.isError).toBe(false);
  });
});
