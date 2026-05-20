import type { APIRequestContext } from '@playwright/test';

const DEFAULT_API_URL = 'http://localhost:8080';
const DEFAULT_COMMAND_NAMESPACE = 'account';

interface ProcessLike {
  readonly env?: Readonly<Record<string, string | undefined>>;
}

export interface ApiOptions {
  readonly apiUrl?: string;
  readonly commandNamespace?: string;
}

export interface FreshEmailOptions {
  readonly domain?: string;
}

export interface RegisterUserOptions extends ApiOptions {
  readonly email?: string;
  readonly password?: string;
  readonly slug?: string;
}

export type Role = string;

export interface RegisteredUser {
  readonly id: number;
  readonly email: string;
  readonly role: Role | null;
  readonly orgId: number;
}

function apiUrlFor(opts: ApiOptions): string {
  const runtime = globalThis as typeof globalThis & { readonly process?: ProcessLike };
  return opts.apiUrl ?? runtime.process?.env?.LAZULI_API_URL ?? DEFAULT_API_URL;
}

function namespaceFor(opts: ApiOptions): string {
  return opts.commandNamespace ?? DEFAULT_COMMAND_NAMESPACE;
}

export function freshEmail(slug: string, opts: FreshEmailOptions = {}): string {
  const domain = opts.domain ?? 'lazuli.test';
  return `e2e-${slug}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}@${domain}`;
}

/**
 * POST /api/v1/c/account.register. Returns the new user row.
 */
export async function registerUser(
  request: APIRequestContext,
  opts: RegisterUserOptions = {},
): Promise<{ user: RegisteredUser; password: string }> {
  const email = opts.email ?? freshEmail(opts.slug ?? 'user');
  const password = opts.password ?? 'pw-e2e-very-secure-123';
  const apiUrl = apiUrlFor(opts);
  const ns = namespaceFor(opts);
  const res = await request.post(`${apiUrl}/api/v1/c/${ns}.register`, {
    data: { email, password },
  });
  if (!res.ok()) {
    throw new Error(`registerUser failed: ${res.status()} ${await res.text()}`);
  }
  const body = (await res.json()) as {
    id: number;
    email: string;
    role: Role | null;
    org_id: number;
  };
  return {
    user: { id: body.id, email: body.email, role: body.role, orgId: body.org_id },
    password,
  };
}

/**
 * POST /api/v1/c/account.login. Returns the bearer token.
 */
export async function loginUser(
  request: APIRequestContext,
  email: string,
  password: string,
  opts: ApiOptions = {},
): Promise<string> {
  const apiUrl = apiUrlFor(opts);
  const ns = namespaceFor(opts);
  const res = await request.post(`${apiUrl}/api/v1/c/${ns}.login`, {
    data: { email, password },
  });
  if (!res.ok()) {
    throw new Error(`loginUser failed: ${res.status()} ${await res.text()}`);
  }
  return (await res.json()) as string;
}

/**
 * POST /api/v1/c/account.set_account_role.
 */
export async function setRole(
  request: APIRequestContext,
  userId: number,
  role: Role,
  opts: ApiOptions = {},
): Promise<void> {
  const apiUrl = apiUrlFor(opts);
  const ns = namespaceFor(opts);
  const res = await request.post(`${apiUrl}/api/v1/c/${ns}.set_account_role`, {
    headers: { 'X-Lazuli-User-Id': String(userId) },
    data: { role },
  });
  if (!res.ok()) {
    throw new Error(`setRole failed: ${res.status()} ${await res.text()}`);
  }
}

/**
 * Register + pick role in one call.
 */
export async function registerWithRole(
  request: APIRequestContext,
  role: Role,
  opts: RegisterUserOptions = {},
): Promise<{ user: RegisteredUser; password: string; token: string }> {
  const { user, password } = await registerUser(request, opts);
  await setRole(request, user.id, role, opts);
  const token = await loginUser(request, user.email, password, opts);
  return { user: { ...user, role }, password, token };
}
