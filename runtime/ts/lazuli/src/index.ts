// `@lazuli/runtime` — typed transport for generated `dist/web/<feature>`
// packages. The React entrypoint at `@lazuli/runtime/react` adds TanStack
// Query bindings without forcing React on non-UI consumers (Node scripts,
// tests, edge workers).

export { LazuliClient, type LazuliClientOptions, type LazuliRouter } from "./client.js";
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
export { toID, tryID } from "./types.js";
export type { ID, Json, Money, Time } from "./types.js";
export { messageForLazuliError, type LazuliErrorMessage } from "./message-for-error.js";
