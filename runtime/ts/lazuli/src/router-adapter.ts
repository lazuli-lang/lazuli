import { createContext, createElement, type ReactNode } from "react";

export type RouterAdapter = {
  navigate: (to: string, opts?: { replace?: boolean }) => void;
  back: () => void;
};

const noopRouterAdapter: RouterAdapter = {
  navigate: () => undefined,
  back: () => undefined,
};

export const LazuliRouterAdapterContext =
  createContext<RouterAdapter>(noopRouterAdapter);

export interface LazuliRouterAdapterProviderProps {
  value: RouterAdapter;
  children: ReactNode;
}

export function LazuliRouterAdapterProvider({
  value,
  children,
}: LazuliRouterAdapterProviderProps): ReactNode {
  return createElement(LazuliRouterAdapterContext.Provider, { value }, children);
}
