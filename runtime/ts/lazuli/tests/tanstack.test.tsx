import { beforeEach, describe, expect, it, vi } from "vitest";

import type { LazuliClient } from "../src/client.js";
import type { LazuliActor } from "../src/route-guard.js";
import type { PolicyAtom } from "../src/spec.js";

const tanstackRouter = vi.hoisted(() => ({
  load: vi.fn(),
  redirect: vi.fn((opts: { to: string }) => ({ redirect: opts.to })),
}));

vi.mock("@tanstack/react-router", () => {
  tanstackRouter.load();
  return { redirect: tanstackRouter.redirect };
});

const policy = (atoms: readonly PolicyAtom[]) => ({ atoms });
const authenticated = policy([{ namespace: "scope", name: "authenticated" }]);
const hostOnly = policy([{ namespace: "role", name: "host" }]);

function clientWithActor(actor: LazuliActor | null): LazuliClient {
  return {
    resolveActor: vi.fn().mockResolvedValue(actor),
  } as unknown as LazuliClient;
}

async function loadAdapter() {
  return import("@lazuli/runtime/react/tanstack");
}

describe("tanstackBeforeLoadGuard", () => {
  beforeEach(() => {
    vi.resetModules();
    tanstackRouter.load.mockClear();
    tanstackRouter.redirect.mockClear();
  });

  it("does not load the TanStack Router peer from the default React entrypoint", async () => {
    const react = await import("@lazuli/runtime/react");

    expect("tanstackBeforeLoadGuard" in react).toBe(false);
    expect(tanstackRouter.load).not.toHaveBeenCalled();
  });

  it("redirects unauthenticated actors to /sign-in", async () => {
    const { tanstackBeforeLoadGuard } = await loadAdapter();
    const guard = tanstackBeforeLoadGuard(clientWithActor(null), {
      policy: authenticated,
    });

    await expect(guard()).rejects.toEqual({ redirect: "/sign-in" });
    expect(tanstackRouter.redirect).toHaveBeenCalledTimes(1);
    expect(tanstackRouter.redirect).toHaveBeenCalledWith({ to: "/sign-in" });
  });

  it("redirects authenticated actors without the required role to /forbidden", async () => {
    const { tanstackBeforeLoadGuard } = await loadAdapter();
    const guard = tanstackBeforeLoadGuard(clientWithActor({ role: "traveler" }), {
      policy: hostOnly,
    });

    await expect(guard()).rejects.toEqual({ redirect: "/forbidden" });
    expect(tanstackRouter.redirect).toHaveBeenCalledTimes(1);
    expect(tanstackRouter.redirect).toHaveBeenCalledWith({ to: "/forbidden" });
  });

  it("allows actors with the required role through", async () => {
    const { tanstackBeforeLoadGuard } = await loadAdapter();
    const guard = tanstackBeforeLoadGuard(clientWithActor({ role: "host" }), {
      policy: hostOnly,
    });

    await expect(guard()).resolves.toBeUndefined();
    expect(tanstackRouter.redirect).not.toHaveBeenCalled();
  });
});
