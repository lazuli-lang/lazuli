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

// Alias used by generated route-param parsers. Kept as a thin wrapper
// around the established non-throwing coercion helper.
export const parseId = tryID;

// `Money` is the currency-aware semantic value emitted by codegen for
// `@semantic.Money` / bare `Money` field declarations. Per proposal
// `docs/proposals/semantic-types-money-brazilian.md` v0.3:
//   - `amount` is a decimal string (e.g. "19.90") — JS `number` loses
//     precision above 2^53, so the wire layer stays in string form.
//   - `currency` is an ISO 4217 3-letter uppercase code.
//
//   const total: Money = { amount: "19.90", currency: "BRL" };
//   formatMoney(total); // -> "R$ 19,90"
export interface Money {
  amount: string;
  currency: string;
}

// Render a Money value via `Intl.NumberFormat` — handles minor-unit
// count per ISO 4217 automatically (BRL/USD with 2 decimals, JPY/KRW
// with 0, BHD/JOD with 3). Default locale is `pt-BR` because Lazuli's
// first pilot is Brazilian; pass an explicit locale for other UIs.
export function formatMoney(m: Money, locale = "pt-BR"): string {
  const numeric = Number(m.amount);
  if (!Number.isFinite(numeric)) {
    return `${m.currency} ${m.amount}`;
  }
  try {
    return new Intl.NumberFormat(locale, {
      style: "currency",
      currency: m.currency,
    }).format(numeric);
  } catch {
    return `${m.currency} ${m.amount}`;
  }
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
