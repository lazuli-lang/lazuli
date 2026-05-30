# M.2 RHF Adapter Status

Implemented `@lazuli/runtime/react/rhf` with `useLazuliFormRHF`, an RHF-compatible adapter around Lazuli query hydration and command submit.

## HostPersonal.tsx Migration Sketch

```tsx
import { Controller } from "react-hook-form";
import { useLazuliFormRHF } from "@lazuli/runtime/react/rhf";
import { lookupMyHost, updateHostPersonal } from "../../dist/ts-web/host/host.gen";
import { hostPersonalSchema } from "../../dist/ts-web/host/host.zod";

export function HostPersonal() {
  const form = useLazuliFormRHF({
    defaults: { firstName: "", lastName: "", birthDate: null, gender: "" },
    hydrateFrom: lookupMyHost,
    submit: updateHostPersonal,
    schema: hostPersonalSchema.schema,
    mapValuesToInput: (values) => ({
      firstName: values.firstName.trim(),
      lastName: values.lastName.trim(),
      birthDate: values.birthDate,
      gender: values.gender || null,
    }),
  });

  return (
    <form onSubmit={(event) => void (event.preventDefault(), form.onSubmit())}>
      <Controller
        control={form.control}
        name="firstName"
        render={({ field, fieldState }) => (
          <Input value={field.value} onChange={field.onChange} error={fieldState.error?.message} />
        )}
      />
      <Controller
        control={form.control}
        name="lastName"
        render={({ field, fieldState }) => (
          <Input value={field.value} onChange={field.onChange} error={fieldState.error?.message} />
        )}
      />
      <Controller
        control={form.control}
        name="birthDate"
        render={({ field, fieldState }) => (
          <DatePicker value={field.value} onChange={field.onChange} error={fieldState.error?.message} />
        )}
      />
      <Button type="submit" disabled={form.isSubmitting}>Save</Button>
    </form>
  );
}
```

The panel keeps existing `<Controller>` UI bindings while deleting the local `useForm`, `zodResolver`, hydrate effect, save mutation, and field-error mapping boilerplate. The expected shape is about 50 lines for the common settings panels once repeated layout code is left in shared panel primitives.
