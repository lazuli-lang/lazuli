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
  | "lifecycle_state_mismatch"
  | "conflict"
  | "internal";

export interface LazuliErrorEnvelope {
  code: LazuliErrorCode | string;
  message: string;
  data?: Json | undefined;
  /**
   * Stable identifier for the error message in the app's i18n catalog
   * (e.g. `traveler_authenticated_signin_required`). Emitted by the Go
   * runtime's `writeLazuliError` when the feature contract opts to
   * expose `message_key` (see `i18n.ShouldExpose`). The wire form is
   * snake_case (`message_key`); we surface it camelCased on the
   * runtime instance, matching the rest of `case-mapper`'s convention.
   *
   * Frontend i18n layers should branch on `messageKey` first (most
   * specific) and fall back to `code` when the key is absent or the
   * client catalog doesn't know it. Undefined for legacy callers that
   * predate the Error-Vocab proposal.
   */
  message_key?: string | undefined;
}

export class LazuliError extends Error {
  readonly code: LazuliErrorCode | string;
  readonly status: number;
  readonly data: Json | undefined;
  /** See `LazuliErrorEnvelope.message_key`. */
  readonly messageKey: string | undefined;

  constructor(status: number, envelope: LazuliErrorEnvelope) {
    super(envelope.message);
    this.name = "LazuliError";
    this.code = envelope.code;
    this.status = status;
    this.data = envelope.data;
    this.messageKey = envelope.message_key;
  }
}

export function isLazuliError(err: unknown): err is LazuliError {
  return err instanceof LazuliError;
}

/**
 * Server rejected a command because the resource's lifecycle state
 * doesn't match what the bound transition chain requires. Frontend
 * should refetch the resource and either retry or surface a UX message.
 *
 * Emitted by commands with `triggers transition <...>` declared.
 */
export class LifecycleStateMismatchError extends LazuliError {
  readonly expectedState: string;
  readonly actualState: string;
  readonly transitions: ReadonlyArray<string>;

  constructor(opts: {
    expectedState: string;
    actualState: string;
    transitions: ReadonlyArray<string>;
    message?: string;
  }) {
    super(409, {
      code: "lifecycle_state_mismatch",
      message: opts.message ?? `expected state '${opts.expectedState}', got '${opts.actualState}'`,
      data: {
        expected_state: opts.expectedState,
        actual_state: opts.actualState,
        transitions: [...opts.transitions],
      },
    });
    this.expectedState = opts.expectedState;
    this.actualState = opts.actualState;
    this.transitions = opts.transitions;
  }
}

export function isLifecycleStateMismatchError(err: unknown): err is LifecycleStateMismatchError {
  return err instanceof LifecycleStateMismatchError;
}
