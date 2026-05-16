// LazuliError mirrors the Go runtime `*lazuli.Error` envelope. The HTTP
// layer encodes failures as JSON:
//
//   { code: "policy_denied", message: "...", data: {...} }
//
// Every typed client method throws an instance of this class on non-2xx
// responses so callers can branch on `err.code` instead of parsing strings.
//
// The canonical code list lives alongside the Go runtime in
// `runtime/go/lazuli/error.go`. We mirror it here as a string-literal union
// to keep autocomplete useful without coupling at compile time.

import type { Json } from "./types.js";

export type LazuliErrorCode =
  | "bad_request"
  | "unauthenticated"
  | "policy_denied"
  | "rate_limited"
  | "validation_failed"
  | "not_found"
  | "tenant_mismatch"
  | "conflict"
  | "internal";

export interface LazuliErrorEnvelope {
  code: LazuliErrorCode | string;
  message: string;
  data?: Json | undefined;
}

export class LazuliError extends Error {
  readonly code: LazuliErrorCode | string;
  readonly status: number;
  readonly data: Json | undefined;

  constructor(status: number, envelope: LazuliErrorEnvelope) {
    super(envelope.message);
    this.name = "LazuliError";
    this.code = envelope.code;
    this.status = status;
    this.data = envelope.data;
  }
}

export function isLazuliError(err: unknown): err is LazuliError {
  return err instanceof LazuliError;
}
