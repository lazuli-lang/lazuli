import { useQuery } from "@tanstack/react-query";
import { createContext, useContext } from "react";
import type { LazuliClient } from "./client.js";
import type { LazuliActor } from "./route-guard.js";

export const LazuliClientContext = createContext<LazuliClient | null>(null);

export type UseActorResult = {
  readonly data: LazuliActor | null;
  readonly isLoading: boolean;
  readonly isError: boolean;
};

export function useLazuliClient(override?: LazuliClient): LazuliClient {
  if (override) return override;
  const ctx = useContext(LazuliClientContext);
  if (!ctx) throw new Error("useLazuliClient: missing LazuliProvider client.");
  return ctx;
}

export function useActor(): UseActorResult {
  const client = useLazuliClient();
  const query = useQuery({
    queryKey: ["lazuli", "actor"],
    queryFn: () => client.resolveActor(),
    retry: false,
    staleTime: Infinity,
  });
  return { data: query.data ?? null, isLoading: query.isLoading, isError: query.isError };
}
