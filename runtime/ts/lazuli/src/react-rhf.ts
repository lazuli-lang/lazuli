import { zodResolver } from "@hookform/resolvers/zod";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  useForm,
  type DefaultValues,
  type FieldValues,
  type Path,
  type Resolver,
  type UseFormProps,
  type UseFormReturn,
} from "react-hook-form";
import type { ZodSchema } from "zod";

import { isLazuliError } from "./error.js";
import { useLazuliCommand, useLazuliQuery } from "./react.web.js";
import type { CommandSpec, QuerySpec } from "./spec.js";

type EmptyArgs = Record<string, never>;

const emptyArgs: EmptyArgs = {};
const disabledHydrateQuery: QuerySpec<EmptyArgs, Partial<object>> = {
  kind: "query",
  name: "__lazuli.form_rhf.disabled",
};

export type UseLazuliFormRHFOptions<
  Values extends FieldValues,
  SubmitInput,
  SubmitOutput,
> = {
  defaults: Values;
  hydrateFrom?: QuerySpec<unknown, Partial<Values>>;
  submit: CommandSpec<SubmitInput, SubmitOutput>;
  mapValuesToInput: (values: Values) => SubmitInput;
  schema?: ZodSchema<Values>;
  rhfOptions?: Omit<UseFormProps<Values>, "defaultValues" | "resolver">;
  onSuccess?: (out: SubmitOutput, values: Values) => void;
  onError?: (err: unknown, values: Values) => void;
};

export type UseLazuliFormRHFResult<Values extends FieldValues> = Omit<
  UseFormReturn<Values>,
  "reset"
> & {
  /** Wired submit handler that runs RHF validation, then the Lazuli command. */
  onSubmit: () => Promise<void>;
  isSubmitting: boolean;
  /** Raw error from the last submit, usually LazuliError. */
  submitError: unknown;
  /** Reset to defaults, or to defaults merged with a partial override. */
  reset: (next?: Partial<Values>) => void;
};

export function useLazuliFormRHF<Values extends FieldValues, Input, Output>(
  opts: UseLazuliFormRHFOptions<Values, Input, Output>,
): UseLazuliFormRHFResult<Values> {
  const hydratedRef = useRef(false);
  const [submitError, setSubmitError] = useState<unknown>(null);

  const form = useForm<Values>({
    ...opts.rhfOptions,
    defaultValues: opts.defaults as DefaultValues<Values>,
    ...(opts.schema !== undefined
      ? { resolver: zodResolver(opts.schema) as Resolver<Values> }
      : {}),
  });
  const command = useLazuliCommand(opts.submit);
  const hydrateSpec = (opts.hydrateFrom ?? disabledHydrateQuery) as QuerySpec<
    EmptyArgs,
    Partial<Values>
  >;
  const hydrate = useLazuliQuery(hydrateSpec, emptyArgs, {
    enabled: opts.hydrateFrom !== undefined,
  });

  const reset = useCallback(
    (next?: Partial<Values>) => {
      form.reset(mergeDefaults(opts.defaults, next));
      form.clearErrors();
      command.reset();
      setSubmitError(null);
    },
    [command, form, opts.defaults],
  );

  useEffect(() => {
    if (hydratedRef.current || hydrate.data === undefined) return;
    hydratedRef.current = true;
    form.reset(mergeDefaults(opts.defaults, hydrate.data));
  }, [form, hydrate.data, opts.defaults]);

  const onSubmit = useCallback(async () => {
    await form.handleSubmit(async (values) => {
      form.clearErrors();
      setSubmitError(null);
      try {
        const out = await command.mutateAsync(opts.mapValuesToInput(values));
        opts.onSuccess?.(out, values);
      } catch (err) {
        setSubmitError(err);
        const usedFieldErrors = setServerFieldErrors(err, values, form.setError);
        if (!usedFieldErrors) opts.onError?.(err, values);
      }
    })();
  }, [command, form, opts]);

  return {
    ...form,
    onSubmit,
    isSubmitting: form.formState.isSubmitting || command.isPending,
    submitError,
    reset,
  };
}

function mergeDefaults<Values extends FieldValues>(
  defaults: Values,
  next?: Partial<Values>,
): Values {
  return { ...defaults, ...(next ?? {}) } as Values;
}

/**
 * Route a Lazuli `validation_failed` envelope's per-field errors into a
 * react-hook-form's `setError`. Returns `true` when at least one field
 * error was applied (caller can then skip a top-level form banner —
 * the inline messages cover it).
 *
 * Backend envelope shape (emitted by codegen-go's semantic-scalar
 * validation block):
 * ```json
 * {
 *   "code": "validation_failed",
 *   "data": {
 *     "fields":             { "cpf": "cpf_invalid" },
 *     "field_message_keys": { "cpf": "validation.cpf_invalid" }
 *   }
 * }
 * ```
 *
 * Message priority per field: `field_message_keys[field]` (i18n catalog
 * key) wins over `fields[field]` (stable error code). Screens that
 * translate `validation.*` keys at render time get localised strings;
 * those that pass the message through verbatim get the stable code
 * fallback.
 *
 * Exposed for screens that use raw `useForm` + `useLazuliCommand`
 * (instead of the all-in-one `useLazuliFormRHF`) so they can adopt
 * inline field errors without refactoring the whole form harness.
 */
export function setServerFieldErrors<Values extends FieldValues>(
  err: unknown,
  values: Values,
  setError: UseFormReturn<Values>["setError"],
): boolean {
  if (!isLazuliError(err) || err.code !== "validation_failed") return false;
  const data = asRecord(err.data);
  const fields = asRecord(data.fields);
  const messageKeys = asRecord(data.field_message_keys);
  let used = false;
  for (const [rawField, rawMessage] of Object.entries(fields)) {
    const message = fieldMessage(messageKeys[rawField] ?? rawMessage);
    if (!message) continue;
    setError(normalizeFieldName(rawField, values), { type: "server", message });
    used = true;
  }
  return used;
}

function fieldMessage(value: unknown): string | null {
  if (typeof value === "string") return value;
  if (Array.isArray(value)) {
    for (const item of value) {
      const nested = fieldMessage(item);
      if (nested) return nested;
    }
    return null;
  }
  const record = asRecord(value);
  for (const key of ["message", "reason", "code"] as const) {
    const candidate = record[key];
    if (typeof candidate === "string") return candidate;
  }
  return null;
}

function normalizeFieldName<Values extends FieldValues>(
  raw: string,
  values: Values,
): Path<Values> {
  const record = values as Record<PropertyKey, unknown>;
  if (raw in record) return raw as Path<Values>;
  const camel = raw.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase());
  return (camel in record ? camel : raw) as Path<Values>;
}

function asRecord(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}
