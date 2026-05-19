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
import type { CommandSpec, QuerySpec } from "./spec.js";

export interface LazuliClientOptions {
  // Base URL of the Go runtime. Trailing slashes are tolerated.
  baseUrl: string;

  // Optional fetch override (Node test harness, MSW, vitest jsdom).
  fetch?: typeof globalThis.fetch;

  // Per-request headers merged with the per-call ones. Useful for
  // auth tokens or `X-Lazuli-Org-ID` in dev sessions.
  headers?: HeadersInit;

  enableAutoRefresh?: boolean;
  refreshUrl?: string;
  onRefresh?: (newToken: string) => void;
  onRefreshFailure?: (err: Error) => void;
}

export class LazuliClient {
  readonly baseUrl: string;
  private readonly fetchImpl: typeof globalThis.fetch;
  private readonly defaultHeaders: Headers;
  private readonly refreshOptions: LazuliClientOptions;
  private authToken: string | null = null;
  private refreshPromise: Promise<string> | null = null;

  constructor(options: LazuliClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/+$/, "");
    this.fetchImpl = options.fetch ?? globalThis.fetch.bind(globalThis);
    this.defaultHeaders = new Headers(options.headers ?? {});
    this.refreshOptions = { ...options, enableAutoRefresh: options.enableAutoRefresh ?? false, refreshUrl: options.refreshUrl ?? "/api/v1/c/auth.refresh" };
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

    const url = `${this.baseUrl}${path}`;
    const request: RequestInit = {
      ...init,
      method: "POST",
      headers,
      body: wireBody,
    };

    const response = await this.fetchImpl(url, request);
    const text = await response.text();
    const error = response.ok ? null : decodeError(response.status, text);
    if (this.refreshOptions.enableAutoRefresh && error?.status === 401 && error.code === "token_expired") {
      const originalError = error;
      try { await this.refreshAuthToken(); } catch { throw originalError; }
      const retryHeaders = new Headers(headers);
      if (this.authToken) retryHeaders.set("Authorization", `Bearer ${this.authToken}`);
      const retry = await this.fetchImpl(url, { ...request, headers: retryHeaders });
      const retryText = await retry.text();
      if (!retry.ok) throw retry.status === 401 ? originalError : decodeError(retry.status, retryText);
      return parseOk<T>(retryText);
    }
    if (error) throw error;
    return parseOk<T>(text);
  }

  private refreshAuthToken(): Promise<string> {
    if (!this.refreshPromise) {
      this.refreshPromise = this.runRefresh().finally(() => { this.refreshPromise = null; });
    }
    return this.refreshPromise;
  }

  private async runRefresh(): Promise<string> {
    try {
      const headers = new Headers(this.defaultHeaders);
      if (this.authToken) headers.set("Authorization", `Bearer ${this.authToken}`);
      headers.set("Content-Type", "application/json");
      const response = await this.fetchImpl(`${this.baseUrl}${this.refreshOptions.refreshUrl}`, {
        method: "POST",
        headers,
        body: "{}",
      });
      if (!response.ok) throw new Error(`lazuli: refresh failed (status ${response.status})`);
      const body = snakeToCamelDeep(await response.json()) as Record<string, unknown>;
      const token = typeof body.access === "string"
        ? body.access
        : typeof body.accessToken === "string" ? body.accessToken : typeof body.token === "string" ? body.token : undefined;
      if (!token) throw new Error("lazuli: refresh response did not include an access token");
      this.setAuthToken(token);
      this.refreshOptions.onRefresh?.(token);
      return token;
    } catch (err) {
      const error = err instanceof Error ? err : new Error("lazuli: refresh failed");
      this.refreshOptions.onRefreshFailure?.(error);
      throw error;
    }
  }
}

function parseOk<T>(text: string): T {
  return text ? (snakeToCamelDeep(JSON.parse(text)) as T) : (undefined as T);
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
