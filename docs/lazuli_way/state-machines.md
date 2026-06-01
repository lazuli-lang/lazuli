# State machines

## Reach for this

When a resource's status moves through a fixed set of stages, declare a
`lifecycle <field>` block whose `state` members are a **named, closed set** and
whose `transition`s bind their `from`/`to` to members of that set. Do NOT leave
the status lattice as an "enum-by-command" shape — a bare `status:` field whose
legal transitions live only in prose comments and are implied by which command
ran. A declared closed `state` set gives the compiler a typed lattice to
membership-check every transition against, and gives agents the status set from
the type instead of a comment.

The closed `state` set IS the named type: the `lifecycle <field>` block lowers
its `state` list into a closed enum (`<Resource><Field>`, e.g.
`JobStepStatus`) + the discriminator field, and every `transition.from` /
`transition.to` must be a member of that set. There is no separate
hand-declared `enum` to keep in sync.

## Before (hand-rolled) / After (idiomatic)

**Before** — enum-by-command: the status lattice documented in a comment, with
no closed `state` set the transitions bind to. The legal moves are *implied* by
which command ran, so nothing membership-checks them:

```
# pauta-web app/features/attachments/attachments.lzi:46
# Upload status discriminator (enum-by-command, see note above). Created pending.
status: AttachmentStatus = pending
```

```
# pauta-web app/features/job_steps_activities/job_steps_activities.lzi:60-65
# Native lifecycle (GAP-11). The block auto-owns the `status` discriminator
# field + its enum (pending | in_progress | completed); no sibling `status:`
# field, no separate `enum JobStepStatus`. Transitions are named DISTINCTLY
# ...
#   begin_step : pending -> in_progress   (start_step, after the order guard)
#   finish_step: in_progress -> completed (complete_step, after no-open-acts)
```

**After** — a closed `state` set with `initial`/`terminal` markers; each
`transition` binds its `from`/`to` to members of the set, so the lattice is
typed and introspectable (no prose, no command-implied status):

```
# pauta-web app/features/job_steps_activities/job_steps_activities.lzi:68
lifecycle status
  state pending initial
  state in_progress
  state completed terminal

  transition begin_step
    from pending
    to in_progress

  transition finish_step
    from in_progress
    to completed
```

The state set is named (`JobStepStatus`, generated from
`<Resource><Field>`), closed (only the three declared members), and
referenceable — every transition's `from`/`to` resolves against it.

## Enforced by

- `LIFECYCLE-STATE-SET-UNDECLARED-001`
  ([crates/lazuli_doctor/src/lifecycle/state_set_undeclared_001.rs](../../crates/lazuli_doctor/src/lifecycle/state_set_undeclared_001.rs))
  — fires on the enum-by-command shape: a lifecycle/transition machine that
  carries `transition`s but declares no closed `state` set for them to bind to.
  Silent once the closed set is declared.
- `LIFECYCLE-TRANSITION-FROM-UNDECLARED` /
  `LIFECYCLE-TRANSITION-TO-UNDECLARED`
  ([crates/lazuli_doctor/src/lifecycle/transition_from_undeclared.rs](../../crates/lazuli_doctor/src/lifecycle/transition_from_undeclared.rs),
  [transition_to_undeclared.rs](../../crates/lazuli_doctor/src/lifecycle/transition_to_undeclared.rs))
  — closed-set membership: a transition whose `from`/`to` names a state outside
  the declared set fires.
- `LIFECYCLE-NO-INITIAL-STATE` / `LIFECYCLE-INITIAL-AMBIGUOUS` — exactly one
  `initial` member on the closed set.
- `LIFECYCLE-ENUM-DUPLICATE` / `LIFECYCLE-FIELD-DOUBLE-DECLARED` consult the
  synth origin (span match), so the auto-emitted closed enum + discriminator
  field are never flagged against their own synthesis — the declared `state`
  set lowers through the existing lifecycle synth without double-emit.
