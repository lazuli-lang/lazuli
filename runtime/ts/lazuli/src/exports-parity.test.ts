// Exports parity invariant for `@lazuli/runtime/react`.
//
// The type contract (`react.ts`) declares the public surface that both
// concrete entrypoints (`react.web.ts`, `react.native.ts`) must satisfy.
// Type-level satisfaction is enforced by TypeScript at compile time
// (when each `.web`/`.native` re-export bumps into the contract's
// declared signatures). This test guards the runtime-export half:
// every contracted runtime name must be present on the web entrypoint.
// (The native entrypoint is validated by Cell E.2's smoke test, which
// runs with RN installed; Vitest's web jsdom environment can't load
// `react-native` modules.)
//
// See `docs/proposals/mobile-target.md` §3.5 + §11.1 cell A.5.

import { describe, expect, it } from "vitest";

import * as web from "./react.web.js";

/**
 * Every name `@lazuli/runtime/react` is contracted to export. Sourced
 * from `react.ts`. When this list drifts from `react.ts`, the parity
 * test fails — the fix is to update both in lockstep.
 *
 * `export declare ...` in `react.ts` produces zero runtime exports, so
 * we can't introspect `react.ts` directly. The contract is duplicated
 * here as the runtime-side mirror.
 */
const CONTRACTED_RUNTIME_EXPORTS = [
  // Universal client/query/command hooks
  "LazuliProvider",
  "queryKeyFor",
  "useLazuliClient",
  "useLazuliQuery",
  "useLazuliCommand",
  "useLazuliAction",
  "useLazuliForm",
  "LazuliRouterAdapterContext",
  "LazuliRouterAdapterProvider",
  "LazuliFlashAdapterContext",
  "LazuliFlashAdapterProvider",
  "evaluatePolicy",
  "withTanStackGuard",
  "RouteGuard",
  "useRouteGuardSkeleton",
  "withRouteGuard",
  "useActor",
  "useLifecycleGate",
  "withLifecycleGate",
  // Universal view-helper hooks
  "canonicalizeSearch",
  "parseSegments",
  "useFilterState",
  "useMultiSelection",
  // Platform-split hooks
  "useLocalSetting",
  "useDrawerSubView",
] as const;

describe("@lazuli/runtime/react — exports parity", () => {
  const webNames = new Set(Object.keys(web));

  it("react.web.ts exports every name declared in react.ts", () => {
    const missing: string[] = [];
    for (const name of CONTRACTED_RUNTIME_EXPORTS) {
      if (!webNames.has(name)) {
        missing.push(name);
      }
    }
    expect(missing).toEqual([]);
  });

  it("react.web.ts does not add un-contracted runtime exports", () => {
    // Additive surface area is a soft violation: anything in `.web.ts`
    // that is not in the contract will work for web consumers but
    // surprise native consumers. Fail loudly.
    const contracted = new Set<string>(CONTRACTED_RUNTIME_EXPORTS);
    const extra: string[] = [];
    for (const name of webNames) {
      if (!contracted.has(name)) {
        extra.push(name);
      }
    }
    expect(extra).toEqual([]);
  });
});
