// Typed spec objects that generated `dist/web/<feature>/<feature>.gen.ts`
// produces, one per command and query. The runtime client and React hooks
// consume these specs to know the canonical name + which queries to
// invalidate post-mutation, while keeping input/output types attached.
//
// `Input` and `Output` are phantom-typed: they only exist at compile time
// so callers can't pass the wrong shape. At runtime the spec is a plain
// JSON-serialisable record.

// Operational metadata projected onto every CommandSpec / QuerySpec.
// Generated client code populates these fields from the DSL so apps can
// build smart UX (rate-limit-aware backoff, policy-aware button gating,
// audit-aware error reporting) WITHOUT calling a separate metadata RPC.
// The server is still the source of truth — these are hints, not
// enforcement.

export interface PolicySpec {
  /// Canonical policy name as written in the DSL — e.g. `@policy.create`.
  readonly name: string;
  /// Resolved policy atoms — pairs like `{ namespace: "role", name: "admin" }`.
  /// Clients can compare against their session principal to disable
  /// affordances proactively rather than waiting for a 403.
  readonly atoms: readonly PolicyAtom[];
}

export interface PolicyAtom {
  readonly namespace: string;
  readonly name: string;
}

/// Rate-limit envelope. Format mirrors the DSL string ("30 per hour per
/// ip"). Clients can parse for backoff hints; the server enforces.
export type RateLimitSpec = string | null;

/// Audit declaration. `"default"` opts into the framework audit;
/// `"none"` opts out; a string array names specific fields.
export type AuditSpec = "default" | "none" | readonly string[];

/// Outbound wire-key map: idiomatic camelCase SDK key → the exact Go
/// `json:"…"` tag the runtime decoder expects. The Go emitter writes the
/// DSL field name VERBATIM as the json tag, so for a field whose DSL name
/// is already camelCase (`registrationStep`) the tag is `registrationStep`
/// too — but the SDK key folds identically, so the heuristic
/// `camelToSnakeDeep` would emit `registration_step` and the value would
/// be silently dropped on decode. This map pins the correct wire key for
/// exactly those fields where the heuristic round-trip is lossy; snake_case
/// fields (`tenant_id` ↔ `tenantId`) round-trip cleanly and are omitted.
export type WireKeyMap = Readonly<Record<string, string>>;

export interface CommandSpec<Input, Output> {
  readonly kind: "command";
  readonly name: string;
  readonly invalidates: readonly string[];
  readonly policy?: PolicySpec;
  readonly rateLimit?: RateLimitSpec;
  readonly audit?: AuditSpec;
  /// Per-field wire-key overrides for the outbound body (see WireKeyMap).
  /// Absent when every input field round-trips through the default caser.
  readonly fields?: WireKeyMap;
  // brand keeps the type parameters from being erased to `unknown`.
  readonly _input?: Input;
  readonly _output?: Output;
}

export interface QuerySpec<Args, Result> {
  readonly kind: "query";
  readonly name: string;
  readonly policy?: PolicySpec;
  readonly rateLimit?: RateLimitSpec;
  /// Per-field wire-key overrides for the outbound args (see WireKeyMap).
  readonly fields?: WireKeyMap;
  readonly _args?: Args;
  readonly _result?: Result;
}

export interface DefineCommandOptions {
  readonly invalidates?: readonly string[];
  readonly policy?: PolicySpec;
  readonly rateLimit?: RateLimitSpec;
  readonly audit?: AuditSpec;
  readonly fields?: WireKeyMap;
}

export interface DefineQueryOptions {
  readonly policy?: PolicySpec;
  readonly rateLimit?: RateLimitSpec;
  readonly fields?: WireKeyMap;
}

export function defineCommand<Input, Output>(
  name: string,
  options: DefineCommandOptions = {},
): CommandSpec<Input, Output> {
  return {
    kind: "command",
    name,
    invalidates: options.invalidates ?? [],
    // Spread-when-defined keeps `exactOptionalPropertyTypes` happy:
    // omitting an optional key entirely differs from assigning
    // `undefined`. Generated code passes only the fields the author
    // declared in the DSL, so unused metadata stays absent rather
    // than visibly `undefined`.
    ...(options.policy !== undefined ? { policy: options.policy } : {}),
    ...(options.rateLimit !== undefined ? { rateLimit: options.rateLimit } : {}),
    ...(options.audit !== undefined ? { audit: options.audit } : {}),
    ...(options.fields !== undefined ? { fields: options.fields } : {}),
  };
}

export function defineQuery<Args, Result>(
  name: string,
  options: DefineQueryOptions = {},
): QuerySpec<Args, Result> {
  return {
    kind: "query",
    name,
    ...(options.policy !== undefined ? { policy: options.policy } : {}),
    ...(options.rateLimit !== undefined ? { rateLimit: options.rateLimit } : {}),
    ...(options.fields !== undefined ? { fields: options.fields } : {}),
  };
}
