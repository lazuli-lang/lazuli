# Command-Triggered Lifecycle Transitions

## TL;DR

Use `triggers transition` when a command's successful write should advance a
resource lifecycle.
The backend checks the resource is still in the transition's `from` state before
the command effect runs.
The state advance is emitted in the same transaction as the command mutation.

## Single transition

Bind a command to one declared lifecycle transition:

```lazuli
command save_host_basic_details
  policy @policy.host_only
  triggers transition fill_basic_details
  updates Host
    full_name = input.full_name
```

`triggers transition fill_basic_details` means the command owns the write and the
lifecycle owns the state edge. Generated backend code gates execution on
`Host.lifecycle_state == fill_basic_details.from`.

If the pre-check passes, the command effect runs and the same transaction writes
`Host.lifecycle_state = fill_basic_details.to`. If the row is already somewhere
else, the command returns `409 lifecycle_state_mismatch`. The command body does
not assign `lifecycle_state` directly.

## Chains (multi-step shortcut)

Use a comma-separated list when one command intentionally skips through a
contiguous onboarding chain:

```lazuli
command complete_host_onboarding
  policy @policy.host_only
  triggers transition fill_basic_details, fill_address, fill_languages
  updates Host
    is_active = true
```

The backend still performs one pre-check, on the first transition's source:
`Host.lifecycle_state == fill_basic_details.from`.

The command mutation runs once. On success, the backend writes the final target:
`Host.lifecycle_state = fill_languages.to`.

Chains must be contiguous: `T[i].to == T[i+1].from`. The analyzer enforces that
relation before codegen. Do not use chains as a general workflow language; they
are a shortcut for a command that validly collapses adjacent lifecycle steps.

## Backend semantics

Generated handlers keep the state check, command effect, and transition update
inside one database transaction:

```go
tx, err := db.BeginTx(ctx, nil)
if err != nil { return err }

var state string
err = tx.QueryRowContext(ctx, `SELECT lifecycle_state FROM host WHERE id = $1 FOR UPDATE`, id).Scan(&state)
if err != nil { tx.Rollback(); return err }

if state != fillBasicDetails.From {
  tx.Rollback()
  return LifecycleStateMismatchError{Code: "lifecycle_state_mismatch"}
}
// run command effect
lastTo := fillLanguages.To
_, err = tx.ExecContext(ctx, `UPDATE host SET lifecycle_state = $1 WHERE id = $2`, lastTo, id)
if err != nil { tx.Rollback(); return err }
return tx.Commit()
```

For a single transition, `<last.to>` is that transition's `to`. For a chain,
`<last.to>` is the final transition's `to`.

## Frontend handling

The TypeScript SDK exposes lifecycle conflicts as a typed error. Refetch the
resource before showing another command surface:

```ts
try {
  await client.runCommand(saveHostBasicDetails, input);
} catch (err) {
  if (isLifecycleStateMismatchError(err)) {
    // refetch resource + show user-facing message
    toast.error(`Estado mudou — atualize a página.`);
  }
}
```

Treat this like an optimistic-concurrency miss. The user may have another tab
open, or another actor may have advanced the same resource.

## Diagnostics

| Code | Description |
|---|---|
| `LIFECYCLE-TRANSITION-001` | Command references a transition name that does not exist on the target resource lifecycle. |
| `LIFECYCLE-TRANSITION-002` | Command has no single lifecycle-bearing target resource for the transition binding. |
| `LIFECYCLE-TRANSITION-003` | Command binds a transition from a lifecycle on a different resource than the command updates. |
| `LIFECYCLE-TRANSITION-004` | Transition chain is not contiguous; one transition's `to` does not match the next transition's `from`. |
| `LIFECYCLE-TRANSITION-005` | Command body writes the lifecycle field directly while also using `triggers transition`. |
| `LIFECYCLE-TRANSITION-006` | Transition binding crosses a feature boundary that is not supported by command-triggered lifecycle transitions. |

## Non-goals

- No multi-resource transitions.
- No implicit or convention-based binding from command names.
- Cross-feature lifecycle transitions are deferred.

## Related

- [Lifecycle grammar](grammar.lzi.md)
- IR command-transition binding (operational proposal archive)
