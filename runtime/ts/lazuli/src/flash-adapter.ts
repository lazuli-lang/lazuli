import { createContext, createElement, type ReactNode } from "react";

export type FlashKind = "success" | "error" | "info";

export type FlashAdapter = {
  show: (kind: FlashKind, messageKey: string) => void;
};

const noopFlashAdapter: FlashAdapter = {
  show: () => undefined,
};

export const LazuliFlashAdapterContext = createContext<FlashAdapter>(noopFlashAdapter);

export interface LazuliFlashAdapterProviderProps {
  value: FlashAdapter;
  children: ReactNode;
}

export function LazuliFlashAdapterProvider({
  value,
  children,
}: LazuliFlashAdapterProviderProps): ReactNode {
  return createElement(LazuliFlashAdapterContext.Provider, { value }, children);
}
