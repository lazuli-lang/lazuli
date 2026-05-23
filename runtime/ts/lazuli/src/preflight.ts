// Client-side preflight registry. Each command spec name (e.g.
// `host.save_host_basic_details`) can have a preflight function
// registered that runs before `useLazuliCommand`'s mutationFn ships
// the request. If the preflight reports field errors, the mutation
// rejects with a `validation_failed`-shaped error — same shape the
// server produces — so the consumer's error path is identical
// whether validation fails locally or at the server.
//
// LAZ-SEMANTIC-AUTO-VALIDATE — codegen emits per-feature
// `preflight.gen.ts` files that call `registerPreflight` at import
// time, populating the registry for every command whose input has at
// least one `@semantic.X` field with a plugin TS validator.

export type PreflightFieldErrors = Record<string, string>;

export type PreflightResult =
  | { ok: true }
  | { ok: false; fields: PreflightFieldErrors; fieldMessageKeys?: Record<string, string> };

export type PreflightFn = (input: unknown) => PreflightResult;

const REGISTRY = new Map<string, PreflightFn>();

export function registerPreflight(commandName: string, fn: PreflightFn): void {
  REGISTRY.set(commandName, fn);
}

export function lookupPreflight(commandName: string): PreflightFn | undefined {
  return REGISTRY.get(commandName);
}

/** Test/reset helper — clears every registered preflight. */
export function __resetPreflightRegistry(): void {
  REGISTRY.clear();
}
