// `@lazuli/runtime` — typed transport for generated `dist/web/<feature>`
// packages. The React entrypoint at `@lazuli/runtime/react` adds TanStack
// Query bindings without forcing React on non-UI consumers (Node scripts,
// tests, edge workers).

export { LazuliClient, type LazuliClientOptions } from "./client.js";
export {
  LazuliError,
  isLazuliError,
  type LazuliErrorCode,
  type LazuliErrorEnvelope,
} from "./error.js";
export {
  defineCommand,
  defineQuery,
  type CommandSpec,
  type DefineCommandOptions,
  type QuerySpec,
} from "./spec.js";
export type { ID, Json, Time } from "./types.js";
