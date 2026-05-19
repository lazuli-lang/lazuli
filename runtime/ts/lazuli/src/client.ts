// LazuliClient is the typed transport that hits the Go runtime's command
// and query routes. Every typed call goes through `runCommand` / `runQuery`
// so headers, error decoding, and base URL handling live in one place.
//
//   const client = new LazuliClient({ baseUrl: "http://localhost:8080" });
//   const customers = await client.runQuery(listCustomers, {});
//   const created = await client.runCommand(createCustomer, { name, email });
//
// The HTTP contract is the same one the Go runtime serves:
//   POST /api/v1/c/<command-name>   body: input    -> output
//   POST /api/v1/q/<query-name>     body: args     -> result

import { camelToSnakeDeep, snakeToCamelDeep } from "./case-mapper.js";
import { LazuliError, type LazuliErrorEnvelope } from "./error.js";
import type { LazuliActor } from "./route-guard.js";
import type { CommandSpec, QuerySpec } from "./spec.js";

export interface LazuliRouter {
  navigate(to: string | { to: string; replace?: boolean }): unknown;
}

export interface LazuliClientOptions {
  // Base URL of the Go runtime. Trailing slashes are tolerated.
  baseUrl: string;

  // Optional fetch override (Node test harness, MSW, vitest jsdom).
  fetch?: typeof globalThis.fetch;

  // Per-request headers merged with the per-call ones. Useful for
  // auth tokens or `X-Lazuli-Org-ID` in dev sessions.
  headers?: HeadersInit;

  // Optional app-level actor query generated from `app.actor_query`.
  actorQuery?: QuerySpec<unknown, LazuliActor | null>;
  actorQueryArgs?: unknown;

  // Optional route adapter used by <RouteGuard>; callers can wrap any router.
  router?: LazuliRouter;
}

export class LazuliClient {
  readonly baseUrl: string;
  private readonly fetchImpl: typeof globalThis.fetch;
  private readonly defaultHeaders: Headers;
  private readonly actorQuery: QuerySpec<unknown, LazuliActor | null> | null;
  private readonly actorQueryArgs: unknown;
  readonly router: LazuliRouter | null;
  private authToken: string | null = null;

  constructor(options: LazuliClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/+$/, "");
    this.fetchImpl = options.fetch ?? globalThis.fetch.bind(globalThis);
    this.defaultHeaders = new Headers(options.headers ?? {});
    this.actorQuery = options.actorQuery ?? null;
    this.actorQueryArgs = options.actorQueryArgs ?? {};
    this.router = options.router ?? null;
  }

  /**
   * Set the bearer token applied to every subsequent request as
   * `Authorization: Bearer <token>`. Pass `null` to clear (logout).
   *
   * Typical use: after `loginAccount` returns the session token,
   * call `client.setAuthToken(token)` so `me`/queries/commands behind
   * `@policy.authenticated` work without each call threading the
   * header through.
   *
   * Storage is in-memory; consumers wanting cross-tab/persistent auth
   * should mirror the token to `localStorage` themselves and rehydrate
   * with `client.setAuthToken(stored)` at app boot.
   */
  setAuthToken(token: string | null): void {
    this.authToken = token;
  }

  /** Read the currently-set bearer token (or null if none). */
  getAuthToken(): string | null {
    return this.authToken;
  }

  async runCommand<Input, Output>(
    spec: CommandSpec<Input, Output>,
    input: Input,
    init?: RequestInit,
  ): Promise<Output> {
    return this.post<Output>(`/api/v1/c/${spec.name}`, input, init);
  }

  async runQuery<Args, Result>(
    spec: QuerySpec<Args, Result>,
    args: Args,
    init?: RequestInit,
  ): Promise<Result> {
    return this.post<Result>(`/api/v1/q/${spec.name}`, args, init);
  }

  async resolveActor(): Promise<LazuliActor | null> {
    if (!this.actorQuery) return null;
    return this.runQuery(this.actorQuery, this.actorQueryArgs);
  }

  private async post<T>(path: string, body: unknown, init?: RequestInit): Promise<T> {
    const headers = new Headers(this.defaultHeaders);
    if (this.authToken) {
      headers.set("Authorization", `Bearer ${this.authToken}`);
    }
    if (init?.headers) {
      new Headers(init.headers).forEach((value: string, key: string) => headers.set(key, value));
    }
    headers.set("Content-Type", "application/json");

    // Generated SDK interfaces are camelCase; the Go runtime's JSON
    // contract is snake_case. We translate at this single boundary —
    // see case-mapper.ts for the scope (plain objects only;
    // strings/numbers/arrays pass through).
    const wireBody = body === undefined ? "{}" : JSON.stringify(camelToSnakeDeep(body));

    const response = await this.fetchImpl(`${this.baseUrl}${path}`, {
      ...init,
      method: "POST",
      headers,
      body: wireBody,
    });

    const text = await response.text();
    if (!response.ok) {
      throw decodeError(response.status, text);
    }
    if (!text) {
      return undefined as T;
    }
    return snakeToCamelDeep(JSON.parse(text)) as T;
  }
}

function decodeError(status: number, raw: string): LazuliError {
  if (!raw) {
    return new LazuliError(status, {
      code: "internal",
      message: `lazuli: empty error body (status ${status})`,
    });
  }
  try {
    const parsed = JSON.parse(raw) as Partial<LazuliErrorEnvelope>;
    return new LazuliError(status, {
      code: parsed.code ?? "internal",
      message: parsed.message ?? raw,
      data: parsed.data,
    });
  } catch {
    return new LazuliError(status, {
      code: "internal",
      message: raw,
    });
  }
}
