import { describe, expect, it } from "vitest";

import { LazuliClient, LifecycleStateMismatchError, isLifecycleStateMismatchError } from "./index.js";
import { defineCommand, defineQuery } from "./spec.js";

const me = defineQuery<Record<string, never>, { ok: boolean }>("me");
const publish = defineCommand<Record<string, never>, { ok: boolean }>("publish");

interface FetchCall {
  url: string;
  init?: RequestInit;
}

function mockFetch(
  handler: (url: string, init?: RequestInit) => Response | Promise<Response>,
): typeof fetch & { calls: FetchCall[] } {
  const calls: FetchCall[] = [];
  const fetcher = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.toString() : input.url;
    calls.push(init === undefined ? { url } : { url, init });
    return handler(url, init);
  }) as typeof fetch & { calls: FetchCall[] };
  fetcher.calls = calls;
  return fetcher;
}

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function tokenExpired(message = "expired"): Response {
  return json({ code: "token_expired", message }, 401);
}

function authorization(init?: RequestInit): string | null {
  return init?.headers === undefined ? null : new Headers(init.headers).get("Authorization");
}

async function waitUntil(done: () => boolean): Promise<void> {
  for (let i = 0; i < 50; i++) {
    if (done()) return;
    await new Promise((resolve) => setTimeout(resolve, 1));
  }
  throw new Error("timed out waiting for concurrent refresh setup");
}

describe("LazuliClient error decoding", () => {
  it("decodes lifecycle_state_mismatch envelopes as LifecycleStateMismatchError", async () => {
    const fetcher = mockFetch(() => json({
      code: "lifecycle_state_mismatch",
      message: "cannot publish from archived",
      data: {
        expected_state: "draft",
        actual_state: "archived",
        transitions: ["submit", "publish"],
      },
    }, 409));
    const client = new LazuliClient({ baseUrl: "https://api.example.test", fetch: fetcher });

    let caught: unknown;
    try {
      await client.runCommand(publish, {});
    } catch (err) {
      caught = err;
    }

    expect(caught).toBeInstanceOf(LifecycleStateMismatchError);
    expect(isLifecycleStateMismatchError(caught)).toBe(true);
    if (!isLifecycleStateMismatchError(caught)) throw new Error("expected lifecycle mismatch error");
    expect(caught.status).toBe(409);
    expect(caught.code).toBe("lifecycle_state_mismatch");
    expect(caught.message).toBe("cannot publish from archived");
    expect(caught.expectedState).toBe("draft");
    expect(caught.actualState).toBe("archived");
    expect(caught.transitions).toEqual(["submit", "publish"]);
  });
});

describe("LazuliClient auto refresh", () => {
  it("refreshes on token_expired, stores the new access token, and retries once", async () => {
    const queryAuth: string[] = [];
    const refreshed: string[] = [];
    const fetcher = mockFetch((url, init) => {
      if (url.endsWith("/api/v1/c/auth.refresh")) {
        expect(authorization(init)).toBe("Bearer old-access");
        return json({ access: "new-access", refresh: "new-refresh" });
      }
      queryAuth.push(authorization(init) ?? "");
      return queryAuth.length === 1 ? tokenExpired() : json({ ok: true });
    });
    const client = new LazuliClient({
      baseUrl: "https://api.example.test",
      fetch: fetcher,
      enableAutoRefresh: true,
      onRefresh: (token) => refreshed.push(token),
    });
    client.setAuthToken("old-access");

    await expect(client.runQuery(me, {})).resolves.toEqual({ ok: true });

    expect(refreshed).toEqual(["new-access"]);
    expect(client.getAuthToken()).toBe("new-access");
    expect(queryAuth).toEqual(["Bearer old-access", "Bearer new-access"]);
    expect(fetcher.calls.filter((call) => call.url.endsWith("/api/v1/c/auth.refresh"))).toHaveLength(1);
  });

  it("single-flights concurrent token_expired responses", async () => {
    let staleCalls = 0;
    let refreshCalls = 0;
    let releaseInitial!: () => void;
    let releaseRefresh!: () => void;
    const initialGate = new Promise<void>((resolve) => { releaseInitial = resolve; });
    const refreshGate = new Promise<void>((resolve) => { releaseRefresh = resolve; });
    const fetcher = mockFetch(async (url, init) => {
      if (url.endsWith("/api/v1/c/auth.refresh")) {
        refreshCalls++;
        await refreshGate;
        return json({ access_token: "fresh-access", refresh_token: "fresh-refresh" });
      }
      if (authorization(init) === "Bearer stale-access") {
        staleCalls++;
        if (staleCalls === 10) releaseInitial();
        await initialGate;
        return tokenExpired();
      }
      return json({ ok: true });
    });
    const client = new LazuliClient({
      baseUrl: "https://api.example.test",
      fetch: fetcher,
      enableAutoRefresh: true,
    });
    client.setAuthToken("stale-access");

    const pending = Array.from({ length: 10 }, () => client.runQuery(me, {}));
    await waitUntil(() => staleCalls === 10 && refreshCalls === 1);
    await Promise.resolve();
    await Promise.resolve();
    releaseRefresh();

    await expect(Promise.all(pending)).resolves.toEqual(
      Array.from({ length: 10 }, () => ({ ok: true })),
    );
    expect(refreshCalls).toBe(1);
    expect(fetcher.calls.filter((call) => authorization(call.init) === "Bearer fresh-access")).toHaveLength(10);
  });

  it("propagates the original 401 and reports refresh failure", async () => {
    const failures: Error[] = [];
    const fetcher = mockFetch((url) => {
      if (url.endsWith("/api/v1/c/auth.refresh")) return json({ code: "refresh_invalid", message: "bad refresh" }, 401);
      return tokenExpired("original expired");
    });
    const client = new LazuliClient({
      baseUrl: "https://api.example.test",
      fetch: fetcher,
      enableAutoRefresh: true,
      onRefreshFailure: (err) => failures.push(err),
    });
    client.setAuthToken("old-access");

    await expect(client.runQuery(me, {})).rejects.toMatchObject({
      status: 401,
      code: "token_expired",
      message: "original expired",
    });
    expect(failures).toHaveLength(1);
  });

  it("does not refresh when auto-refresh is disabled", async () => {
    let refreshCalls = 0;
    const fetcher = mockFetch((url) => {
      if (url.endsWith("/api/v1/c/auth.refresh")) {
        refreshCalls++;
        return json({ access: "unexpected" });
      }
      return tokenExpired();
    });
    const client = new LazuliClient({ baseUrl: "https://api.example.test", fetch: fetcher });
    client.setAuthToken("old-access");

    await expect(client.runQuery(me, {})).rejects.toMatchObject({
      status: 401,
      code: "token_expired",
    });
    expect(refreshCalls).toBe(0);
  });

  it("propagates the original 401 when the retried request is still unauthorized", async () => {
    let queryCalls = 0;
    const fetcher = mockFetch((url) => {
      if (url.endsWith("/api/v1/c/auth.refresh")) return json({ access: "new-access", refresh: "new-refresh" });
      queryCalls++;
      return queryCalls === 1 ? tokenExpired("original expired") : tokenExpired("retry expired");
    });
    const client = new LazuliClient({
      baseUrl: "https://api.example.test",
      fetch: fetcher,
      enableAutoRefresh: true,
    });
    client.setAuthToken("old-access");

    await expect(client.runQuery(me, {})).rejects.toMatchObject({
      status: 401,
      code: "token_expired",
      message: "original expired",
    });
  });
});
