import { renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";

import { LazuliClient } from "./client.js";
import {
  LazuliProvider,
  LazuliRouteParamsError,
  useLazuliRouteParams,
} from "./react.web.js";

describe("useLazuliRouteParams", () => {
  it("returns parsed params for valid raw route params", () => {
    const parser = (raw: Record<string, string>) => {
      const id = Number.parseInt(raw.id ?? "", 10);
      return Number.isNaN(id) ? null : { id };
    };

    const { result } = renderHook(() =>
      useLazuliRouteParams("host_detail", parser, { id: "42" }),
    );

    expect(result.current).toEqual({ id: 42 });
  });

  it("throws a typed error for invalid params without redirect", () => {
    const parser = () => null;

    expect(() =>
      renderHook(() => useLazuliRouteParams("host_detail", parser, { id: "nope" })),
    ).toThrow(LazuliRouteParamsError);
  });

  it("redirects through the injected router adapter for invalid params", () => {
    const navigate = vi.fn();
    const client = new LazuliClient({
      baseUrl: "http://runtime.test",
      fetch: vi.fn(),
      router: { navigate },
    });
    const wrapper = ({ children }: { children: ReactNode }) => (
      <LazuliProvider client={client}>{children}</LazuliProvider>
    );

    expect(() =>
      renderHook(
        () =>
          useLazuliRouteParams("host_detail", () => null, { id: "nope" }, {
            redirectOnInvalid: "/host",
          }),
        { wrapper },
      ),
    ).toThrow(LazuliRouteParamsError);
    expect(navigate).toHaveBeenCalledWith({ to: "/host", replace: true });
  });
});
