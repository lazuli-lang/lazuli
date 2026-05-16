// Common scalar types shared between every typed feature client.
//
// `ID` and `Time` mirror the Go runtime's `lazuli.ID` (int64 marshalled as
// JSON number) and `lazuli.Time` (RFC 3339 string). Generated types prefer
// these aliases to keep the wire contract obvious in editor tooltips.

export type ID = number;
export type Time = string;

// `toID` coerces URL params (always string) into the wire ID shape
// (number). Closes WAR-VOCAB-HOSTPROPDETAIL-03 by giving route
// components a typed alternative to scattered `Number(params.id)` casts
// — non-numeric input throws synchronously so the call site catches
// the bug at the boundary rather than producing `NaN` payloads.
export function toID(value: string | number | undefined | null): ID {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string") {
    const trimmed = value.trim();
    if (/^\d+$/.test(trimmed)) return Number(trimmed);
  }
  throw new Error(
    `lazuli: invalid ID value ${JSON.stringify(value)} — expected a non-negative integer string or number`,
  );
}

// `tryID` is the non-throwing variant for paths that may legitimately
// receive non-numeric placeholders (storybook fixture ids like
// `"pousada"`). Returns `null` when the value is not coercible.
export function tryID(value: string | number | undefined | null): ID | null {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string") {
    const trimmed = value.trim();
    if (/^\d+$/.test(trimmed)) return Number(trimmed);
  }
  return null;
}

// JSON-shaped value, used as a fallback for free-form payloads (audit data,
// event payloads, query.sql results).
export type Json =
  | null
  | string
  | number
  | boolean
  | Json[]
  | { [key: string]: Json };
