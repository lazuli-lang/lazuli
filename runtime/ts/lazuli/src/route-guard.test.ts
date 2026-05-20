import { describe, expect, it } from "vitest";

import {
  evaluatePolicy,
  evaluatePolicyList,
  type LazuliActor,
  type LazuliRouteGuardPolicyAtomSet,
} from "./route-guard.js";
import type { PolicyAtom } from "./spec.js";

const policy = (atoms: readonly PolicyAtom[]): LazuliRouteGuardPolicyAtomSet => ({ atoms });
const host: LazuliActor = { id: "usr_1", roles: ["host"] };
const traveler: LazuliActor = { id: "usr_2", role: "traveler" };

describe("evaluatePolicy", () => {
  it("authorizes public policies without an actor", () => {
    expect(evaluatePolicy(null, policy([{ namespace: "scope", name: "public" }]))).toBe(
      "authorized",
    );
  });

  it("requires an actor for authenticated policies", () => {
    const p = policy([{ namespace: "scope", name: "authenticated" }]);
    expect(evaluatePolicy(null, p)).toBe("unauthenticated");
    expect(evaluatePolicy(host, p)).toBe("authorized");
  });

  it("authorizes matching roles", () => {
    expect(evaluatePolicy(host, policy([{ namespace: "role", name: "host" }]))).toBe(
      "authorized",
    );
  });

  it("rejects mismatched roles for authenticated actors", () => {
    expect(evaluatePolicy(traveler, policy([{ namespace: "role", name: "host" }]))).toBe(
      "unauthorized",
    );
  });

  it("authorizes when any policy in a policy list matches", () => {
    expect(
      evaluatePolicyList(traveler, [
        policy([{ namespace: "role", name: "host" }]),
        policy([{ namespace: "role", name: "traveler" }]),
      ]),
    ).toBe("authorized");
  });
});
