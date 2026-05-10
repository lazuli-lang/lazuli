// Hand-written extensions for the customer feature. NOT generated.
//
// Mirrors `dist/go/customer/extensions.go`: the `.gen.ts` sibling holds
// typed contracts (commands, queries, types) and this file is the only
// place an author writes TS for the feature — UI helpers, derived
// selectors, formatters, etc. Generated code never collides with this
// file (`.gen.ts` suffix is reserved for codegen output).

import type { Customer } from "./customer.gen.js";

// Display the customer's name with their email in parentheses, used in
// search-result rows and breadcrumbs. The `extensions` block in the DSL
// could later expose a `display` slot that resolves to this function via
// the typed extension registry.
export function customerLabel(c: Customer): string {
  return `${c.name} (${c.email})`;
}
