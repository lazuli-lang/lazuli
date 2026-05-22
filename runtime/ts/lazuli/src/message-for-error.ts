import { isLazuliError } from "./error.js";

export type LazuliErrorMessage = {
  key: string;
  fallback: string;
  params: Record<string, string | number>;
  code: string;
};

function scalarParams(data: unknown): Record<string, string | number> {
  const params: Record<string, string | number> = {};

  if (data === null || typeof data !== "object" || Array.isArray(data)) {
    return params;
  }

  for (const [key, value] of Object.entries(data as Record<string, unknown>)) {
    if (typeof value === "string" || typeof value === "number") {
      params[key] = value;
    }
  }

  return params;
}

export function messageForLazuliError(err: unknown): LazuliErrorMessage {
  if (isLazuliError(err)) {
    return {
      key: err.messageKey ?? err.code,
      fallback: err.message,
      params: scalarParams(err.data),
      code: err.code,
    };
  }

  if (err instanceof Error && /network|fetch|offline/i.test(err.message)) {
    return { key: "errors.network", fallback: err.message, params: {}, code: "network" };
  }

  return { key: "errors.unknown", fallback: "An unknown error occurred", params: {}, code: "unknown" };
}
