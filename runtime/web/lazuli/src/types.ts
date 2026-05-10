// Common scalar types shared between every typed feature client.
//
// `ID` and `Time` mirror the Go runtime's `lazuli.ID` (int64 marshalled as
// JSON number) and `lazuli.Time` (RFC 3339 string). Generated types prefer
// these aliases to keep the wire contract obvious in editor tooltips.

export type ID = number;
export type Time = string;

// JSON-shaped value, used as a fallback for free-form payloads (audit data,
// event payloads, query.sql results).
export type Json =
  | null
  | string
  | number
  | boolean
  | Json[]
  | { [key: string]: Json };
