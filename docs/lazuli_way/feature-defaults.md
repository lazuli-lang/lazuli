# Feature defaults

## Reach for this

When every (or nearly every) command in a feature spells the **same**
`rate_limit "<spec>"` or the same `audit default`, declare it **once** in the
feature-level `defaults` block and let each command inherit it — instead of
copy-pasting the line onto every command and hoping each copy stays in sync.

This is the same inheritance rule `defaults` already gives `tenancy`,
`timestamps`, `soft_delete`, and `policy_for`: the feature default fills the
gap; a per-command value always wins. `rate_limit` stays a string spec (the
string→struct redesign is a separate, deferred change) and `audit` keeps its
keyword shape — this is purely about *where you declare it*, not *what it
means*.

The hoist is a **pure refactor**: the emitted Go is byte-identical to the
fully-explicit per-command form, because the analyzer bakes the inherited value
onto each command at lowering (a command with no own `rate_limit`/`audit` ends
up with exactly the IR it would have had if you typed the line by hand). The
pilot migrations were validated by diffing `generate go` output before/after —
empty diff in every feature.

## Before (hand-rolled) / After (idiomatic)

The pilot audit found ~445 lines of this across pauta + hostpoint with
near-zero variation: `rate_limit` repeated ~230× (pauta `media_price_tables`
×17, `customer_management` ×17), `audit default` repeated ~215× (hostpoint
`operations` 8×, `catalog` 18×, `messaging`/`trust` 7× each).

**Before** — hostpoint `operations.lzi`: every command re-types the same two
lines (excerpt, `app/features/operations/operations.lzi`):

```
feature operations
  defaults
    tenancy org

  command request_service          # operations.lzi:231
    rate_limit "10 per 10 minutes per ip"
    audit default
    ...
  command send_counter_proposal    # operations.lzi:241
    rate_limit "10 per 10 minutes per ip"
    audit default
    ...
  command accept_proposal          # operations.lzi:250
    rate_limit "10 per 10 minutes per ip"
    audit default
    ...
```

**After** — hoist the uniform value into `defaults`; every command inherits:

```
feature operations
  defaults
    tenancy org
    audit default                  # hoisted: all 8 commands inherit

  command request_service
    rate_limit "10 per 10 minutes per ip"   # kept: only the 5 write
    ...                                       #   commands carry a rate_limit;
  command send_counter_proposal               #   the 3 read commands do not,
    rate_limit "10 per 10 minutes per ip"     #   so rate_limit is NOT uniform
    ...                                        #   and stays per-command.
```

When the value **is** fully uniform across every command, hoist both — pauta
`media_price_tables` collapsed 17 identical `rate_limit "60 per minute per
user"` + 17 `audit default` (34 lines) into a 2-line `defaults` block:

```
feature media_price_tables
  defaults
    rate_limit "60 per minute per user"
    audit default

  command create_media_price_table   # inherits both
    ...
```

### Inheritance, override, opt-out

- **Inherit** — a command with no own `rate_limit` / `audit` picks up the
  feature default.
- **Override** — a per-command `rate_limit "<spec>"` or `audit <subjects>`
  wins over the feature default (e.g. hostpoint `account`'s auth/OTP commands
  keep their own stricter limits).
- **Opt out** — `audit none` on a command clears the inherited audit default
  for that command.

### When *not* to hoist (keep it explicit)

Inheritance must reproduce each command's exact effective value. Do **not**
hoist when:

- **The value genuinely varies.** pauta `account` keeps `rate_limit`
  per-command — its commands split `"5 per 10 minutes per ip"` vs `"20 per
  minute per user"`; there is no single default worth hoisting.
- **A read command must stay un-audited.** Hoisting `audit default` would give
  every command an audit it may not want (e.g. an `account.me` read that emits
  no audit today). `audit none` does *not* reproduce "no audit line" in the
  emitted command struct, so a feature with a deliberately un-audited command
  is left explicit.
- **`audit default` carries `audit data_subject <field>` children.** Those
  LGPD/GDPR children (hostpoint `account` / `host` / `traveler`) depend on the
  command-local `audit` line; hoisting would orphan them. Left explicit.

## Grammar

```
defaults
  rate_limit "<spec>"     # hoisted to every command; per-command rate_limit wins
  audit default           # hoisted to every command; per-command audit / audit none wins
```

Both keys sit alongside the existing `tenancy` / `timestamps` / `policy_for`
defaults. See `docs/grammar.lzi.md` §4 (`defaults_block`).

## Enforced by

- `defaults_hoist_rate_limit_hint` / `defaults_hoist_audit_hint`
  (`crates/lazuli_doctor_run/src/doctor/aggregators/audit/policy_hints.rs`,
  `defaults_hoist_hints`) — `lazuli doctor` emits a hint when a feature repeats
  an identical `rate_limit` or `audit default` on **≥3** commands while *not*
  already hoisting it into `defaults`. The hint names the `defaults` line to
  add and deep-links this doc. It stays silent below 3 commands, on any
  variation, and once the value is hoisted (the `defaults_rate_limit` /
  `defaults_audit` guard, since the inheritance pass bakes the value onto every
  command's IR).
- `command-rate-limit` (`crates/lazuli_lsp/src/diagnostics/policy/rate_limit.rs`)
  — the security check that requires public/mutating commands to declare a
  `rate_limit` now recognizes a feature-level `defaults rate_limit` as
  satisfying the contract, so the hoist doesn't trip it.

The scaffold seed `note` feature
(`lazurite/templates/default/app/features/note/note.lzi`) demonstrates the
idiom: it declares `defaults rate_limit` + `defaults audit default` and its
commands inherit.

See also: the spec at `.specs/changes/0004-defaults-hoist/` and
[crud-by-convention.md](crud-by-convention.md) (the other axis of "declare the
shared shape once").
