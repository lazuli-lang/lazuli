import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { isLazuliError } from "./error.js";
import type { CommandSpec, QuerySpec } from "./spec.js";

type EmptyArgs = Record<string, never>;

const emptyArgs: EmptyArgs = {};
const disabledHydrateQuery: QuerySpec<EmptyArgs, Partial<object>> = {
  kind: "query",
  name: "__lazuli.form.disabled",
};

type QueryHook = <Result>(
  spec: QuerySpec<EmptyArgs, Result>,
  args: EmptyArgs,
  options?: { enabled?: boolean },
) => { data: Result | undefined };

type CommandHook = <Input, Output>(
  spec: CommandSpec<Input, Output>,
) => {
  mutateAsync: (input: Input) => Promise<Output>;
  isPending: boolean;
  error: unknown;
  reset: () => void;
};

export type UseLazuliFormOptions<Values extends object, SubmitInput, SubmitOutput> = {
  defaults: Values;
  hydrateFrom?: QuerySpec<EmptyArgs, Partial<Values>>;
  submit: CommandSpec<SubmitInput, SubmitOutput>;
  mapValuesToInput: (values: Values) => SubmitInput;
  onSuccess?: (out: SubmitOutput, values: Values) => void;
  onError?: (err: unknown, values: Values) => void;
};

export type UseLazuliFormResult<Values extends object> = {
  values: Values;
  setValue: <K extends keyof Values>(field: K, value: Values[K]) => void;
  setValues: (next: Partial<Values>) => void;
  reset: () => void;
  handleSubmit: () => Promise<void>;
  isDirty: boolean;
  isSubmitting: boolean;
  error: unknown;
  serverErrors: Partial<Record<keyof Values, string>>;
};

export function createUseLazuliForm(useQuery: QueryHook, useCommand: CommandHook) {
  return function useLazuliForm<Values extends object, SubmitInput, SubmitOutput>(
    opts: UseLazuliFormOptions<Values, SubmitInput, SubmitOutput>,
  ): UseLazuliFormResult<Values> {
    const [values, setFormValues] = useState<Values>(opts.defaults);
    const [serverErrors, setServerErrors] = useState<Partial<Record<keyof Values, string>>>({});
    const hydratedRef = useRef(false);

    const hydrateSpec = (opts.hydrateFrom ?? disabledHydrateQuery) as QuerySpec<
      EmptyArgs,
      Partial<Values>
    >;
    const hydrate = useQuery(hydrateSpec, emptyArgs, { enabled: opts.hydrateFrom !== undefined });
    const command = useCommand(opts.submit);

    useEffect(() => {
      if (hydratedRef.current || hydrate.data === undefined) return;
      hydratedRef.current = true;
      setFormValues((current) => ({ ...current, ...hydrate.data }) as Values);
    }, [hydrate.data]);

    const setValue = useCallback(
      <K extends keyof Values>(field: K, value: Values[K]) => {
        setFormValues((current) => ({ ...current, [field]: value }) as Values);
        setServerErrors((current) => omitServerErrors(current, [field]));
      },
      [],
    );

    const setValues = useCallback((next: Partial<Values>) => {
      setFormValues((current) => ({ ...current, ...next }) as Values);
      setServerErrors((current) =>
        omitServerErrors(current, Object.keys(next) as Array<keyof Values>),
      );
    }, []);

    const reset = useCallback(() => {
      setFormValues(opts.defaults);
      setServerErrors({});
      command.reset();
    }, [command, opts.defaults]);

    const handleSubmit = useCallback(async () => {
      setServerErrors({});
      const submittedValues = values;
      try {
        const out = await command.mutateAsync(opts.mapValuesToInput(submittedValues));
        opts.onSuccess?.(out, submittedValues);
      } catch (err) {
        setServerErrors(extractServerErrors(err, submittedValues));
        opts.onError?.(err, submittedValues);
      }
    }, [command, opts, values]);

    const isDirty = useMemo(
      () => JSON.stringify(values) !== JSON.stringify(opts.defaults),
      [opts.defaults, values],
    );

    return {
      values,
      setValue,
      setValues,
      reset,
      handleSubmit,
      isDirty,
      isSubmitting: command.isPending,
      error: command.error,
      serverErrors,
    };
  };
}

function omitServerErrors<Values extends object>(
  current: Partial<Record<keyof Values, string>>,
  fields: ReadonlyArray<keyof Values>,
): Partial<Record<keyof Values, string>> {
  let changed = false;
  const next = { ...current };
  for (const field of fields) {
    if (field in next) {
      delete next[field];
      changed = true;
    }
  }
  return changed ? next : current;
}

function extractServerErrors<Values extends object>(
  err: unknown,
  values: Values,
): Partial<Record<keyof Values, string>> {
  if (!isLazuliError(err) || err.code !== "validation_failed") return {};
  const fields = asRecord(asRecord(err.data).fields);
  const out: Partial<Record<keyof Values, string>> = {};
  for (const [rawField, rawMessage] of Object.entries(fields)) {
    const message = fieldMessage(rawMessage);
    if (message) out[normalizeFieldName(rawField, values)] = message;
  }
  return out;
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
    if (typeof record[key] === "string") return record[key];
  }
  return null;
}

function normalizeFieldName<Values extends object>(raw: string, values: Values): keyof Values {
  const record = values as Record<PropertyKey, unknown>;
  if (raw in record) return raw as keyof Values;
  const camel = raw.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase());
  return (camel in record ? camel : raw) as keyof Values;
}

function asRecord(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}
