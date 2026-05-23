// `@lazuli/runtime` — typed transport for generated `dist/web/<feature>`
// packages. The React entrypoint at `@lazuli/runtime/react` adds TanStack
// Query bindings without forcing React on non-UI consumers (Node scripts,
// tests, edge workers).

export {
  LazuliClient,
  type LazuliClientOptions,
  type LazuliFlash,
  type LazuliRouter,
} from "./client.js";
export {
  LifecycleStateMismatchError,
  LazuliError,
  isLifecycleStateMismatchError,
  isLazuliError,
  type LazuliErrorCode,
  type LazuliErrorEnvelope,
} from "./error.js";
export {
  evaluatePolicy,
  type LazuliActor,
  type LazuliRouteGuardPolicy,
  type RouteGuardVerdict,
} from "./route-guard.js";
export {
  defineCommand,
  defineQuery,
  type AuditSpec,
  type CommandSpec,
  type DefineCommandOptions,
  type DefineQueryOptions,
  type PolicyAtom,
  type PolicySpec,
  type QuerySpec,
  type RateLimitSpec,
} from "./spec.js";
export type { ScalarFixtureProvider, ScalarFixtures } from "./scalars.js";
export {
  formatBrazilianCNPJ,
  formatBrazilianCPF,
  formatBrazilianPhone,
  formatCurrency,
  formatDate,
  formatMoney,
  formatPhone,
  parseBrazilianCNPJInput,
  parseBrazilianCPFInput,
  parseBrazilianPhoneInput,
  parseCurrencyInput,
  parseDateInput,
  parseMoneyInput,
  parsePhoneInput,
} from "./formatters.js";
// `parseId` added by Wave A.7 (useLazuliRouteParams typed parsers).
// c-3 owns `formatMoney` canonically; a-7's types.js variant (if any)
// is shadowed by the formatters.js re-export above. Build will catch
// any name collision.
export { toID, tryID, parseId } from "./types.js";
export type { ID, Json, Money, Time } from "./types.js";
export { messageForLazuliError, type LazuliErrorMessage } from "./message-for-error.js";
