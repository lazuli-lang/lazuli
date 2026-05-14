// Smoke tests for the L0 #6 view-helpers (C.4b). Cover the pure helpers
// (`canonicalizeSearch`, `parseSegments`) deterministically and the stateful
// hooks (`useMultiSelection`, `useLocalSetting`) via RTL's `renderHook`.
// `useDrawerSubView` / `useFilterState` integration belongs to the C.4a
// codegen tests — this file only validates the shapes shipped by C.4b.

import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import {
  canonicalizeSearch,
  parseSegments,
  useLocalSetting,
  useMultiSelection,
} from "./view-helpers.js";

describe("canonicalizeSearch", () => {
  it("emits empty string for empty state", () => {
    expect(canonicalizeSearch({}, "")).toBe("");
  });

  it("orders keys alphabetically", () => {
    const out = canonicalizeSearch({ type: "doc", slug: "welcome" }, "");
    expect(out).toBe("slug:welcome type:doc");
  });

  it("flattens multi-value keys as repeated segments", () => {
    const out = canonicalizeSearch({ tag: ["a", "b"], type: "doc" }, "onboarding");
    expect(out).toBe("tag:a tag:b type:doc onboarding");
  });

  it("drops null / empty entries", () => {
    const out = canonicalizeSearch({ slug: null, type: "doc", tag: [] }, "  ");
    expect(out).toBe("type:doc");
  });

  it("appends free text last with trim", () => {
    const out = canonicalizeSearch({ type: "doc" }, "  some free text  ");
    expect(out).toBe("type:doc some free text");
  });
});

describe("parseSegments", () => {
  it("parses key:value tokens and free text", () => {
    const segs = parseSegments(
      "slug:foo tag:a tag:b free text",
      ["slug", "tag"] as const,
      ["tag"] as const,
    );
    expect(segs).toEqual([
      { kind: "filter", field: "slug", value: "foo" },
      { kind: "filter", field: "tag", value: "a" },
      { kind: "filter", field: "tag", value: "b" },
      { kind: "free", value: "free text" },
    ]);
  });

  it("returns only free text when no keyword matches", () => {
    const segs = parseSegments("just typing here", ["slug", "tag"] as const, ["tag"] as const);
    expect(segs).toEqual([{ kind: "free", value: "just typing here" }]);
  });

  it("returns empty array for empty input", () => {
    expect(parseSegments("", ["slug"] as const, [] as const)).toEqual([]);
  });
});

describe("useMultiSelection", () => {
  const items = [
    { id: "a" as const },
    { id: "b" as const },
    { id: "c" as const },
    { id: "d" as const },
  ];

  it("toggles ids in and out of the Set", () => {
    const { result } = renderHook(() => useMultiSelection<"a" | "b" | "c" | "d">(items));
    act(() => result.current.toggle("a"));
    expect(result.current.ids.has("a")).toBe(true);
    act(() => result.current.toggle("a"));
    expect(result.current.ids.has("a")).toBe(false);
  });

  it("selectRange selects inclusive range against snapshot order", () => {
    const { result } = renderHook(() => useMultiSelection<"a" | "b" | "c" | "d">(items));
    act(() => result.current.selectRange("b", "d"));
    expect([...result.current.ids].sort()).toEqual(["b", "c", "d"]);
  });

  it("shift-toggle extends from last selected id via selectRange", () => {
    const { result } = renderHook(() => useMultiSelection<"a" | "b" | "c" | "d">(items));
    act(() => result.current.toggle("a")); // last = a
    act(() => result.current.toggle("c", true)); // shift → range a..c
    expect([...result.current.ids].sort()).toEqual(["a", "b", "c"]);
  });

  it("clear empties the selection", () => {
    const { result } = renderHook(() => useMultiSelection<"a" | "b" | "c" | "d">(items));
    act(() => result.current.toggle("a"));
    act(() => result.current.toggle("b"));
    act(() => result.current.clear());
    expect(result.current.ids.size).toBe(0);
  });
});

describe("useLocalSetting", () => {
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
    const { result } = renderHook(() => useLocalSetting<{ gridSize: string }>("test:key", { gridSize: "sm" }));
    act(() => result.current[1]({ gridSize: "lg" }));
    expect(result.current[0]).toEqual({ gridSize: "lg" });
    expect(JSON.parse(window.localStorage.getItem("test:key") ?? "null")).toEqual({ gridSize: "lg" });
  });

  it("reads pre-existing storage value on first render", () => {
    window.localStorage.setItem("test:key", JSON.stringify({ gridSize: "md" }));
    const { result } = renderHook(() => useLocalSetting<{ gridSize: string }>("test:key", { gridSize: "sm" }));
    expect(result.current[0]).toEqual({ gridSize: "md" });
  });
});
