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
}

export class LazuliClient {
  readonly baseUrl: string;
  private readonly fetchImpl: typeof globalThis.fetch;
  private readonly defaultHeaders: HeadersInit;

  constructor(options: LazuliClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/+$/, "");
    this.fetchImpl = options.fetch ?? globalThis.fetch.bind(globalThis);
    this.defaultHeaders = options.headers ?? {};
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
    if (init?.headers) {
      new Headers(init.headers).forEach((value: string, key: string) => headers.set(key, value));
    }
    headers.set("Content-Type", "application/json");

    const response = await this.fetchImpl(`${this.baseUrl}${path}`, {
      ...init,
      method: "POST",
      headers,
      body: body === undefined ? "{}" : JSON.stringify(body),
    });

    const text = await response.text();
    if (!response.ok) {
      throw decodeError(response.status, text);
    }
    if (!text) {
      return undefined as T;
    }
    return JSON.parse(text) as T;
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
