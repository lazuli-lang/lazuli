import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";

import { LazuliClient } from "./client.js";
import { LazuliProvider, useLazuliForm } from "./react.web.js";
import { defineCommand, defineQuery } from "./spec.js";

type ProfileValues = {
  fullName: string;
  email: string;
  age: number;
};

const defaults: ProfileValues = { fullName: "", email: "", age: 0 };
const hydrateProfile = defineQuery<Record<string, never>, Partial<ProfileValues>>("profile.lookup");
const submitProfile = defineCommand<ProfileValues, { ok: boolean }>("profile.update");

function makeHarness(fetchImpl?: typeof globalThis.fetch) {
  const client = new LazuliClient({
    baseUrl: "http://runtime.test",
    fetch: fetchImpl ?? okFetch({ ok: true }),
  });
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>
      <LazuliProvider client={client}>{children}</LazuliProvider>
    </QueryClientProvider>
  );
  return { queryClient, wrapper };
}

function okFetch(body: unknown): typeof globalThis.fetch {
  return async () => jsonResponse(body);
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status });
}

describe("useLazuliForm", () => {
  it("uses defaults as initial values", () => {
    const { wrapper } = makeHarness();

    const { result } = renderHook(
      () =>
        useLazuliForm({
          defaults,
          submit: submitProfile,
          mapValuesToInput: (values) => values,
        }),
      { wrapper },
    );

    expect(result.current.values).toEqual(defaults);
    expect(result.current.isDirty).toBe(false);
    expect(result.current.serverErrors).toEqual({});
  });

  it("merges hydrateFrom query data into the form once loaded", async () => {
    const { wrapper } = makeHarness(async (input) => {
      expect(String(input)).toContain("/api/v1/q/profile.lookup");
      return jsonResponse({ full_name: "Ada Lovelace", email: "ada@example.com" });
    });

    const { result } = renderHook(
      () =>
        useLazuliForm({
          defaults,
          hydrateFrom: hydrateProfile,
          submit: submitProfile,
          mapValuesToInput: (values) => values,
        }),
      { wrapper },
    );

    await waitFor(() => expect(result.current.values.fullName).toBe("Ada Lovelace"));
    expect(result.current.values).toEqual({
      fullName: "Ada Lovelace",
      email: "ada@example.com",
      age: 0,
    });
  });

  it("hydrates only once so refetches do not overwrite user edits", async () => {
    let queryCalls = 0;
    const { queryClient, wrapper } = makeHarness(async () => {
      queryCalls += 1;
      return jsonResponse({
        full_name: queryCalls === 1 ? "Ada Lovelace" : "Grace Hopper",
      });
    });

    const { result } = renderHook(
      () =>
        useLazuliForm({
          defaults,
          hydrateFrom: hydrateProfile,
          submit: submitProfile,
          mapValuesToInput: (values) => values,
        }),
      { wrapper },
    );

    await waitFor(() => expect(result.current.values.fullName).toBe("Ada Lovelace"));

    act(() => result.current.setValue("fullName", "Typed Name"));
    await act(async () => {
      await queryClient.invalidateQueries({ queryKey: ["lazuli", "profile.lookup"] });
    });

    await waitFor(() => expect(queryCalls).toBeGreaterThanOrEqual(2));
    expect(result.current.values.fullName).toBe("Typed Name");
  });

  it("updates fields and marks the form dirty", () => {
    const { wrapper } = makeHarness();

    const { result } = renderHook(
      () =>
        useLazuliForm({
          defaults,
          submit: submitProfile,
          mapValuesToInput: (values) => values,
        }),
      { wrapper },
    );

    act(() => result.current.setValue("fullName", "Ada Lovelace"));
    expect(result.current.values.fullName).toBe("Ada Lovelace");
    expect(result.current.isDirty).toBe(true);

    act(() => result.current.setValues({ email: "ada@example.com", age: 36 }));
    expect(result.current.values).toEqual({
      fullName: "Ada Lovelace",
      email: "ada@example.com",
      age: 36,
    });
  });

  it("submits mapped values through the command hook", async () => {
    const requests: unknown[] = [];
    const onSuccess = vi.fn();
    const { wrapper } = makeHarness(async (_input, init) => {
      requests.push(JSON.parse(String(init?.body)));
      return jsonResponse({ ok: true });
    });

    const { result } = renderHook(
      () =>
        useLazuliForm({
          defaults,
          submit: submitProfile,
          mapValuesToInput: (values) => ({
            fullName: values.fullName.trim(),
            email: values.email,
            age: values.age,
          }),
          onSuccess,
        }),
      { wrapper },
    );

    act(() => result.current.setValues({ fullName: "  Ada Lovelace  ", age: 36 }));
    await act(async () => {
      await result.current.handleSubmit();
    });

    expect(requests).toEqual([{ full_name: "Ada Lovelace", email: "", age: 36 }]);
    expect(onSuccess).toHaveBeenCalledWith(
      { ok: true },
      { fullName: "  Ada Lovelace  ", email: "", age: 36 },
    );
  });

  it("surfaces validation_failed field errors from the server", async () => {
    const onError = vi.fn();
    const { wrapper } = makeHarness(async () =>
      jsonResponse(
        {
          code: "validation_failed",
          message: "invalid profile",
          data: {
            fields: {
              full_name: "too_short",
              email: { message: "invalid_format" },
            },
          },
        },
        400,
      ),
    );

    const { result } = renderHook(
      () =>
        useLazuliForm({
          defaults,
          submit: submitProfile,
          mapValuesToInput: (values) => values,
          onError,
        }),
      { wrapper },
    );

    await act(async () => {
      await result.current.handleSubmit();
    });

    await waitFor(() =>
      expect(result.current.serverErrors).toEqual({
        fullName: "too_short",
        email: "invalid_format",
      }),
    );
    expect(result.current.error).toMatchObject({ code: "validation_failed" });
    expect(onError).toHaveBeenCalledTimes(1);
  });

  it("resets values to defaults and clears dirty state plus server errors", async () => {
    const { wrapper } = makeHarness(async () =>
      jsonResponse(
        {
          code: "validation_failed",
          message: "invalid profile",
          data: { fields: { full_name: "too_short" } },
        },
        400,
      ),
    );

    const { result } = renderHook(
      () =>
        useLazuliForm({
          defaults,
          submit: submitProfile,
          mapValuesToInput: (values) => values,
        }),
      { wrapper },
    );

    act(() => result.current.setValue("fullName", "Ada Lovelace"));
    await act(async () => {
      await result.current.handleSubmit();
    });
    await waitFor(() => expect(result.current.serverErrors.fullName).toBe("too_short"));

    act(() => result.current.reset());

    expect(result.current.values).toEqual(defaults);
    expect(result.current.isDirty).toBe(false);
    expect(result.current.serverErrors).toEqual({});
  });

  it("preserves the generic field shape at compile time", () => {
    function TypeFixture() {
      const form = useLazuliForm({
        defaults,
        submit: submitProfile,
        mapValuesToInput: (values) => values,
      });

      form.setValue("age", 1);
      form.setValues({ fullName: "Ada Lovelace" });
      // @ts-expect-error age requires a number.
      form.setValue("age", "1");
      // @ts-expect-error field must exist on the form values.
      form.setValue("missing", true);

      return form.values.age;
    }

    expect(TypeFixture).toBeTypeOf("function");
  });
});
