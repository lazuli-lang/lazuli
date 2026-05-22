import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";

import { LazuliClient } from "./client.js";
import {
  LazuliFlashAdapterProvider,
  LazuliProvider,
  LazuliRouterAdapterProvider,
  useLazuliAction,
  type FlashAdapter,
  type RouterAdapter,
} from "./react.web.js";
import { defineCommand } from "./spec.js";

type Input = { name: string };
type Output = { id: string; name: string };

const okOutput: Output = { id: "cmd_1", name: "Ada" };

describe("useLazuliAction", () => {
  it("invalidates merged query keys before the caller onSuccess", async () => {
    const order: string[] = [];
    const { spec, wrapper, queryClient } = makeHarness({
      invalidates: ["feat.lookup"],
    });
    vi.spyOn(queryClient, "invalidateQueries").mockImplementation(async (filters) => {
      order.push(`invalidate:${String(filters?.queryKey?.[1])}`);
    });
    const onSuccess = vi.fn(() => {
      order.push("onSuccess");
    });

    const { result } = renderHook(
      () => useLazuliAction(spec, { invalidates: ["feat.list"], onSuccess }),
      { wrapper },
    );

    await act(async () => {
      await result.current.mutateAsync({ name: "Ada" });
    });

    expect(order).toEqual([
      "invalidate:feat.lookup",
      "invalidate:feat.list",
      "onSuccess",
    ]);
    expect(onSuccess).toHaveBeenCalledWith(okOutput, { name: "Ada" });
  });

  it("calls onError and swallows errors by default", async () => {
    const onError = vi.fn();
    const { spec, wrapper } = makeHarness({ fail: true });
    const { result } = renderHook(() => useLazuliAction(spec, { onError }), { wrapper });
    let resolved: Output | undefined;

    await act(async () => {
      resolved = await result.current.mutateAsync({ name: "bad" });
    });

    expect(resolved).toBeUndefined();
    expect(onError).toHaveBeenCalledTimes(1);
    expect(onError.mock.calls[0]?.[1]).toEqual({ name: "bad" });
  });

  it("rethrows errors when throwOnError is true", async () => {
    const onError = vi.fn();
    const { spec, wrapper } = makeHarness({ fail: true });
    const { result } = renderHook(
      () => useLazuliAction(spec, { onError, throwOnError: true }),
      { wrapper },
    );
    let thrown: unknown;

    await act(async () => {
      try {
        await result.current.mutateAsync({ name: "bad" });
      } catch (err) {
        thrown = err;
      }
    });

    expect(thrown).toMatchObject({ status: 400 });
    expect(onError).toHaveBeenCalledTimes(1);
  });

  it("navigates to the resolved redirect on success", async () => {
    const router: RouterAdapter = { navigate: vi.fn(), back: vi.fn() };
    const { spec, wrapper } = makeHarness({ router });
    const { result } = renderHook(
      () =>
        useLazuliAction(spec, {
          redirect: (out) => `/commands/${out.id}`,
          replace: true,
        }),
      { wrapper },
    );

    await act(async () => {
      await result.current.mutateAsync({ name: "Ada" });
    });

    expect(router.navigate).toHaveBeenCalledWith("/commands/cmd_1", { replace: true });
    expect(router.back).not.toHaveBeenCalled();
  });

  it("goes back on success when back is set", async () => {
    const router: RouterAdapter = { navigate: vi.fn(), back: vi.fn() };
    const { spec, wrapper } = makeHarness({ router });
    const { result } = renderHook(() => useLazuliAction(spec, { back: true }), {
      wrapper,
    });

    await act(async () => {
      await result.current.mutateAsync({ name: "Ada" });
    });

    expect(router.back).toHaveBeenCalledTimes(1);
    expect(router.navigate).not.toHaveBeenCalled();
  });

  it("shows the configured flash on success", async () => {
    const flash: FlashAdapter = { show: vi.fn() };
    const { spec, wrapper } = makeHarness({ flash });
    const { result } = renderHook(
      () =>
        useLazuliAction(spec, {
          flash: { kind: "success", messageKey: "feature.saved" },
        }),
      { wrapper },
    );

    await act(async () => {
      await result.current.mutateAsync({ name: "Ada" });
    });

    expect(flash.show).toHaveBeenCalledWith("success", "feature.saved");
  });

  it("maps input before the command runs", async () => {
    const { spec, wrapper, bodies } = makeHarness();
    const { result } = renderHook(
      () =>
        useLazuliAction(spec, {
          mapInput: (input) => ({ ...input, name: input.name.trim() }),
        }),
      { wrapper },
    );
    let out: Output | undefined;

    await act(async () => {
      out = await result.current.mutate({ name: "  Ada  " });
    });

    expect(out).toEqual(okOutput);
    expect(bodies).toEqual([{ name: "Ada" }]);
  });

  it("does not import TanStack Router from the headless action source", () => {
    const here = dirname(fileURLToPath(import.meta.url));
    const source = readFileSync(join(here, "use-lazuli-action.ts"), "utf8");

    expect(source).not.toContain("@tanstack/react-router");
  });
});

function makeHarness(options: {
  invalidates?: readonly string[];
  fail?: boolean;
  router?: RouterAdapter;
  flash?: FlashAdapter;
} = {}) {
  const spec = defineCommand<Input, Output>("feat.save", {
    invalidates: options.invalidates ?? [],
  });
  const bodies: unknown[] = [];
  const fetchImpl: typeof globalThis.fetch = async (_url, init) => {
    bodies.push(JSON.parse(String(init?.body ?? "{}")));
    if (options.fail) {
      return new Response(JSON.stringify({ code: "bad_request", message: "Nope" }), {
        status: 400,
      });
    }
    return new Response(JSON.stringify(okOutput), { status: 200 });
  };
  const client = new LazuliClient({ baseUrl: "http://runtime.test", fetch: fetchImpl });
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const router = options.router ?? { navigate: () => undefined, back: () => undefined };
  const flash = options.flash ?? { show: () => undefined };
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>
      <LazuliProvider client={client}>
        <LazuliRouterAdapterProvider value={router}>
          <LazuliFlashAdapterProvider value={flash}>{children}</LazuliFlashAdapterProvider>
        </LazuliRouterAdapterProvider>
      </LazuliProvider>
    </QueryClientProvider>
  );

  return { spec, wrapper, queryClient, bodies };
}
