import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render, renderHook, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { LazuliClient } from "../src/client.js";
import {
  LazuliProvider,
  useLifecycleGate,
  withLifecycleGate,
  withRouteGuard,
  type LifecycleGateEvaluator,
  type LifecycleGateMetadata,
} from "../src/react.web.js";
import type { LazuliActor } from "../src/route-guard.js";
import type { PolicyAtom } from "../src/spec.js";

const metadata: LifecycleGateMetadata = {
  resource: "Host",
  state: "complete",
  resume: "host_onboarding",
};
const authenticated = { atoms: [{ namespace: "scope", name: "authenticated" } satisfies PolicyAtom] };

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("lifecycle route gates", () => {
  it("renders the child on a pass verdict", async () => {
    const { client } = mockClient();
    const evaluator = vi.fn<LifecycleGateEvaluator>().mockResolvedValue({ kind: "pass" });
    const Gated = withLifecycleGate(Child, metadata, evaluator);

    renderWithClient(<Gated />, client);

    expect(await screen.findByTestId("child")).toBeTruthy();
    expect(evaluator).toHaveBeenCalledOnce();
    expect(evaluator).toHaveBeenCalledWith(client, "complete");
  });

  it("navigates and hides the child on a redirect verdict", async () => {
    const { client, router } = mockClient();
    const evaluator = vi
      .fn<LifecycleGateEvaluator>()
      .mockResolvedValue({ kind: "redirect", to: "/onboarding/host" });
    const Gated = withLifecycleGate(Child, metadata, evaluator);

    renderWithClient(<Gated />, client);

    await waitFor(() =>
      expect(router.navigate).toHaveBeenCalledWith({ to: "/onboarding/host", replace: true }),
    );
    expect(screen.queryByTestId("child")).toBeNull();
  });

  it("renders the route-guard skeleton while hydrating", () => {
    const { client } = mockClient();
    const pending = deferred();
    const evaluator = vi.fn<LifecycleGateEvaluator>().mockReturnValue(pending.promise);
    const Gated = withLifecycleGate(Child, metadata, evaluator);

    const { container } = renderWithClient(<Gated />, client);

    expect(container.childElementCount).toBe(0);
    expect(screen.queryByTestId("child")).toBeNull();
  });

  it("does not set state after unmounting during a pending evaluator", async () => {
    const { client } = mockClient();
    const pending = deferred();
    const evaluator = vi.fn<LifecycleGateEvaluator>().mockReturnValue(pending.promise);
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const Gated = withLifecycleGate(Child, metadata, evaluator);

    const { unmount } = renderWithClient(<Gated />, client);
    unmount();
    await act(async () => {
      pending.resolve({ kind: "pass" });
      await pending.promise;
    });

    expect(consoleError).not.toHaveBeenCalled();
  });

  it("returns verdicts from useLifecycleGate", async () => {
    const { client } = mockClient();
    const evaluator = vi.fn<LifecycleGateEvaluator>().mockResolvedValue({ kind: "pass" });
    const wrapper = wrapperFor(client);

    const { result } = renderHook(() => useLifecycleGate(metadata, evaluator), { wrapper });

    expect(result.current.verdict).toBe("hydrating");
    await waitFor(() => expect(result.current.verdict).toEqual({ kind: "pass" }));
  });

  it("passes optional substep metadata to the lifecycle evaluator", async () => {
    const { client } = mockClient();
    const evaluator = vi.fn<LifecycleGateEvaluator>().mockResolvedValue({ kind: "pass" });
    const wrapper = wrapperFor(client);
    const substepMetadata: LifecycleGateMetadata = {
      ...metadata,
      state: "basic_details_pending",
      substep: "phone_verification",
    };

    renderHook(() => useLifecycleGate(substepMetadata, evaluator), { wrapper });

    await waitFor(() =>
      expect(evaluator).toHaveBeenCalledWith(
        client,
        "basic_details_pending",
        "phone_verification",
      ),
    );
  });

  it("does not call the lifecycle evaluator when withRouteGuard rejects the actor", async () => {
    const { client, router } = mockClient(null);
    const evaluator = vi.fn<LifecycleGateEvaluator>().mockResolvedValue({ kind: "pass" });
    const LifecycleGated = withLifecycleGate(Child, metadata, evaluator);
    const Guarded = withRouteGuard(LifecycleGated, {
      policy: authenticated,
      onUnauthenticated: "/sign-in",
    });

    renderWithClient(<Guarded />, client);

    await waitFor(() =>
      expect(router.navigate).toHaveBeenCalledWith({ to: "/sign-in", replace: true }),
    );
    expect(evaluator).not.toHaveBeenCalled();
    expect(screen.queryByTestId("child")).toBeNull();
  });
});

function Child() {
  return <div data-testid="child" />;
}

function mockClient(actor: LazuliActor | null = { role: "host" }) {
  const router = { navigate: vi.fn() };
  const client = {
    router,
    resolveActor: vi.fn().mockResolvedValue(actor),
  } as unknown as LazuliClient;
  return { client, router };
}

function wrapperFor(client: LazuliClient) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <LazuliProvider client={client}>{children}</LazuliProvider>
      </QueryClientProvider>
    );
  };
}

function renderWithClient(ui: ReactNode, client: LazuliClient) {
  return render(ui, { wrapper: wrapperFor(client) });
}

function deferred() {
  let resolve!: (value: Awaited<ReturnType<LifecycleGateEvaluator>>) => void;
  const promise = new Promise<Awaited<ReturnType<LifecycleGateEvaluator>>>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}
