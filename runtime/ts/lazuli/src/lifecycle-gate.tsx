import { createElement, useEffect, useState, type ComponentType } from "react";

import { useRouteGuardSkeleton } from "./route-guard-component.js";
import { useLazuliClient } from "./use-actor.js";

export type LifecycleVerdict =
  | { kind: "pass" }
  | { kind: "redirect"; to: string };

export type LifecycleGateMetadata = {
  readonly resource: string;
  readonly state: string;
  readonly substep?: string;
  readonly resume: string;
};

export type LifecycleGateEvaluator = (
  client: import("./client.js").LazuliClient,
  expectedState: string,
  expectedSubstep?: string,
) => Promise<LifecycleVerdict>;

export function withLifecycleGate<P extends object>(
  Component: ComponentType<P>,
  metadata: LifecycleGateMetadata,
  evaluator: LifecycleGateEvaluator,
): ComponentType<P> {
  return function LifecycleGatedComponent(props: P) {
    const skeleton = useRouteGuardSkeleton();
    const { verdict } = useLifecycleGate(metadata, evaluator);
    if (verdict === "hydrating") return skeleton;
    if (verdict.kind === "redirect") return null;
    return createElement(Component, props);
  };
}

export function useLifecycleGate(
  metadata: LifecycleGateMetadata,
  evaluator: LifecycleGateEvaluator,
): { verdict: LifecycleVerdict | "hydrating" } {
  const client = useLazuliClient();
  const router = client.router;
  const [verdict, setVerdict] = useState<LifecycleVerdict | "hydrating">("hydrating");

  useEffect(() => {
    let cancelled = false;
    const result =
      metadata.substep === undefined
        ? evaluator(client, metadata.state)
        : evaluator(client, metadata.state, metadata.substep);
    result
      .then((next) => {
        if (cancelled) return;
        setVerdict(next);
        if (next.kind === "redirect") void router?.navigate({ to: next.to, replace: true });
      })
      .catch(() => {
        if (!cancelled) setVerdict({ kind: "pass" });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return { verdict };
}
