import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { useController, type UseFormProps } from "react-hook-form";
import type { ZodSchema } from "zod";
import { describe, expect, it, vi } from "vitest";

import { LazuliClient } from "./client.js";
import { LazuliProvider } from "./react.web.js";
import { useLazuliFormRHF } from "./react-rhf.js";
import { defineCommand, defineQuery } from "./spec.js";

type ProfileValues = {
  fullName: string;
  email: string;
  age: number;
};

const defaults: ProfileValues = { fullName: "", email: "", age: 0 };
const hydrateProfile = defineQuery<Record<string, never>, Partial<ProfileValues>>("profile.lookup");
const submitProfile = defineCommand<ProfileValues, { ok: boolean }>("profile.update");

type ProfileFormOptions = {
  hydrateFrom?: typeof hydrateProfile;
  mapValuesToInput?: (values: ProfileValues) => ProfileValues;
  schema?: ZodSchema<ProfileValues>;
  rhfOptions?: Omit<UseFormProps<ProfileValues>, "defaultValues" | "resolver">;
  onSuccess?: (out: { ok: boolean }, values: ProfileValues) => void;
  onError?: (err: unknown, values: ProfileValues) => void;
};

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

function useProfileForm(options: ProfileFormOptions = {}) {
  const form = useLazuliFormRHF({
    defaults,
    submit: submitProfile,
    mapValuesToInput: options.mapValuesToInput ?? ((values) => values),
    ...(options.hydrateFrom !== undefined ? { hydrateFrom: options.hydrateFrom } : {}),
    ...(options.schema !== undefined ? { schema: options.schema } : {}),
    ...(options.rhfOptions !== undefined ? { rhfOptions: options.rhfOptions } : {}),
    ...(options.onSuccess !== undefined ? { onSuccess: options.onSuccess } : {}),
    ...(options.onError !== undefined ? { onError: options.onError } : {}),
  });
  const fullName = useController<ProfileValues, "fullName">({
    control: form.control,
    name: "fullName",
  });
  const email = useController<ProfileValues, "email">({
    control: form.control,
    name: "email",
  });
  const age = useController<ProfileValues, "age">({
    control: form.control,
    name: "age",
  });
  const errors = form.formState.errors;
  const isDirty = form.formState.isDirty;
  return { form, fullName, email, age, errors, isDirty };
}

function okFetch(body: unknown): typeof globalThis.fetch {
  return async () => jsonResponse(body);
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status });
}

describe("useLazuliFormRHF", () => {
  it("uses defaults as RHF field values", () => {
    const { wrapper } = makeHarness();

    const { result } = renderHook(() => useProfileForm(), { wrapper });

    expect(result.current.fullName.field.value).toBe("");
    expect(result.current.email.field.value).toBe("");
    expect(result.current.age.field.value).toBe(0);
    expect(result.current.isDirty).toBe(false);
  });

  it("hydrates query data into RHF once loaded", async () => {
    const { wrapper } = makeHarness(async (input) => {
      expect(String(input)).toContain("/api/v1/q/profile.lookup");
      return jsonResponse({ full_name: "Ada Lovelace", email: "ada@example.com" });
    });

    const { result } = renderHook(() => useProfileForm({ hydrateFrom: hydrateProfile }), {
      wrapper,
    });

    await waitFor(() => expect(result.current.fullName.field.value).toBe("Ada Lovelace"));
    expect(result.current.email.field.value).toBe("ada@example.com");
    expect(result.current.age.field.value).toBe(0);
  });

  it("does not overwrite user edits on later hydrate refetches", async () => {
    let queryCalls = 0;
    const { queryClient, wrapper } = makeHarness(async () => {
      queryCalls += 1;
      return jsonResponse({
        full_name: queryCalls === 1 ? "Ada Lovelace" : "Grace Hopper",
      });
    });

    const { result } = renderHook(() => useProfileForm({ hydrateFrom: hydrateProfile }), {
      wrapper,
    });

    await waitFor(() => expect(result.current.fullName.field.value).toBe("Ada Lovelace"));
    await act(async () => {
      result.current.fullName.field.onChange("Typed Name");
    });
    await waitFor(() => expect(result.current.fullName.field.value).toBe("Typed Name"));

    await act(async () => {
      await queryClient.invalidateQueries({ queryKey: ["lazuli", "profile.lookup"] });
    });

    await waitFor(() => expect(queryCalls).toBeGreaterThanOrEqual(2));
    expect(result.current.fullName.field.value).toBe("Typed Name");
  });

  it("submits mapped RHF values through the Lazuli command", async () => {
    const requests: unknown[] = [];
    const { wrapper } = makeHarness(async (_input, init) => {
      requests.push(JSON.parse(String(init?.body)));
      return jsonResponse({ ok: true });
    });

    const { result } = renderHook(
      () =>
        useProfileForm({
          mapValuesToInput: (values) => ({
            fullName: values.fullName.trim(),
            email: values.email,
            age: values.age,
          }),
        }),
      { wrapper },
    );

    await act(async () => {
      result.current.fullName.field.onChange("  Ada Lovelace  ");
      result.current.age.field.onChange(36);
    });
    await act(async () => {
      await result.current.form.onSubmit();
    });

    expect(requests).toEqual([{ full_name: "Ada Lovelace", email: "", age: 36 }]);
  });

  it("fires onSuccess after a successful mutation", async () => {
    const events: string[] = [];
    const onSuccess = vi.fn((out: { ok: boolean }, values: ProfileValues) => {
      events.push(`success:${String(out.ok)}:${values.fullName}`);
    });
    const { wrapper } = makeHarness(async () => {
      events.push("mutation");
      return jsonResponse({ ok: true });
    });

    const { result } = renderHook(() => useProfileForm({ onSuccess }), { wrapper });

    await act(async () => {
      result.current.fullName.field.onChange("Ada Lovelace");
    });
    await act(async () => {
      await result.current.form.onSubmit();
    });

    expect(events).toEqual(["mutation", "success:true:Ada Lovelace"]);
    expect(onSuccess).toHaveBeenCalledWith(
      { ok: true },
      { fullName: "Ada Lovelace", email: "", age: 0 },
    );
  });

  it("maps validation_failed field errors into RHF formState errors", async () => {
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

    const { result } = renderHook(() => useProfileForm({ onError }), { wrapper });

    await act(async () => {
      await result.current.form.onSubmit();
    });

    await waitFor(() => expect(result.current.errors.fullName?.message).toBe("too_short"));
    expect(result.current.errors.fullName?.type).toBe("server");
    expect(result.current.errors.email?.message).toBe("invalid_format");
    expect(result.current.form.submitError).toMatchObject({ code: "validation_failed" });
    expect(onError).not.toHaveBeenCalled();
  });

  it("calls onError for non-field submit failures", async () => {
    const onError = vi.fn();
    const { wrapper } = makeHarness(async () =>
      jsonResponse({ code: "internal", message: "boom" }, 500),
    );

    const { result } = renderHook(() => useProfileForm({ onError }), { wrapper });

    await act(async () => {
      await result.current.form.onSubmit();
    });

    await waitFor(() => expect(result.current.form.submitError).toMatchObject({ code: "internal" }));
    expect(onError).toHaveBeenCalledWith(expect.objectContaining({ code: "internal" }), defaults);
  });

  it("resets to defaults merged with a partial override", async () => {
    const { wrapper } = makeHarness();

    const { result } = renderHook(() => useProfileForm(), { wrapper });

    await act(async () => {
      result.current.fullName.field.onChange("Ada Lovelace");
      result.current.age.field.onChange(36);
    });
    await act(async () => {
      result.current.form.reset({ age: 42 });
    });

    expect(result.current.fullName.field.value).toBe("");
    expect(result.current.email.field.value).toBe("");
    expect(result.current.age.field.value).toBe(42);
    expect(result.current.isDirty).toBe(false);
  });

  it("preserves the generic RHF field shape at compile time", () => {
    function TypeFixture() {
      const form = useLazuliFormRHF({
        defaults,
        submit: submitProfile,
        mapValuesToInput: (values) => values,
      });

      form.register("fullName");
      form.setValue("age", 1);
      // @ts-expect-error age requires a number.
      form.setValue("age", "1");
      // @ts-expect-error field must exist on the form values.
      form.register("missing");

      return form.watch("age");
    }

    expect(TypeFixture).toBeTypeOf("function");
  });
});
