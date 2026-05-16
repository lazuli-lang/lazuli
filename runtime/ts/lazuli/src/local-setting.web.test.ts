// Web-specific tests for `useLocalSetting`. Asserts the documented
// first-render contract: returns the persisted value synchronously
// (via `useSyncExternalStore` over `localStorage`). The native
// counterpart lives next door under jest-expo (per
// `docs/proposals/mobile-target.md` §3.1.1).

import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { useLocalSetting } from "./local-setting.web.js";

describe("useLocalSetting (web)", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  afterEach(() => {
    window.localStorage.clear();
  });

  it("returns the default value when storage is empty", () => {
    const { result } = renderHook(() => useLocalSetting("test:key", { gridSize: "sm" as const }));
    expect(result.current[0]).toEqual({ gridSize: "sm" });
  });

  it("writes and reads through localStorage", () => {
    const { result } = renderHook(() =>
      useLocalSetting<{ gridSize: string }>("test:key", { gridSize: "sm" }),
    );
    act(() => result.current[1]({ gridSize: "lg" }));
    expect(result.current[0]).toEqual({ gridSize: "lg" });
    expect(JSON.parse(window.localStorage.getItem("test:key") ?? "null")).toEqual({
      gridSize: "lg",
    });
  });

  it("reads pre-existing storage value on first render", () => {
    window.localStorage.setItem("test:key", JSON.stringify({ gridSize: "md" }));
    const { result } = renderHook(() =>
      useLocalSetting<{ gridSize: string }>("test:key", { gridSize: "sm" }),
    );
    expect(result.current[0]).toEqual({ gridSize: "md" });
  });
});
