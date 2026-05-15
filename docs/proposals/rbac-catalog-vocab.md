# RBAC Catalog Vocabulary — `role` + `permission` declarations in `.lzi`

**Status**: v0.1 design proposal. L0 vocabulary draft. Builds on
existing `policies` / `@role.*` / `@scope.*` / `@actor.*` surface
(`docs/invariants.md:270-287`). Not yet graded.

**Audience**: language team (Lazuli core), Lazuli Go runtime team,
downstream product authors who today hardcode permission catalogs in
TypeScript/Go.

**Date**: 2026-05-14.

**Pilot bucket**: production-grade readiness cell #8
(`docs/proposals/production-readiness.md:39`). Same wave as
`encryption-vocab`, `report-vocab`, `poller-vocab`, `plan-and-gate-vocab`.

**Companions / non-overlap**:

- `docs/proposals/auth-session-tenant-pin.md` — codegen gap for
  multi-tenant session columns. Surface-free. **No overlap**: that
  proposal threads the tenant axis through `IssueSession`; this
  proposal declares which permissions a role grants. Distinct slots.
- `docs/proposals/auth-lowering-scope.md` — chose Route A for the
  `auth` block lowering and listed `service_account` / `passkeys` /
  `impersonation` etc. as speculative until pilot exercise. RBAC
  catalog is **not** in that speculative list; it is a separate
  primitive that lives outside the `auth` block (catalog is package-
  scoped, not feature-scoped). **No overlap**.
- `docs/proposals/bucket-auth-cycle.md` — design pass for the
  existing `auth` block (`identity` / `password` / `oauth` / `mfa` /
  `sessions`). **Adjacent, not overlapping**: that proposal tightens
  the existing auth surface; this proposal introduces a new top-level
  surface for the catalog. The two land independently.
- `docs/proposals/bucket-admin-cycle.md` — defines `admin_resource` /
  `admin_dashboard` / `admin_action` kinds, **pilot-gated**. Touches
  `@role.admin` in its policy-narrowing diagnostics
  (`ADMIN-POLICY-001`) but does **not** declare the role catalog. The
  reconciliation is explicit (see §"Reconciliation with
  `bucket-admin-cycle.md`" below): `admin_resource` consumes the role
  catalog produced by this proposal as its narrowing-check ground
  truth. The two land orthogonally; admin cuts only after both this
  proposal and a pilot SaaS exercise admin chrome.

**First consumers**:

- Pleiades v2 (Phase C of the strategic pivot
  `project_strategic_pivot_2026-05-13.md`) — declares per-user
  permissions for skill-tree authoring and review actions.
- Hostpoint Phase 1 Auth port (Phase D, downstream) — multi-tenant
  marketplace with role-per-tenant assignments.
- production-grade apps generally (`admin / supervisor / vendedor /
  operacional` per `c:/Users/lucas/dev-trabalho/production-grade/apps/api/src/
  building-blocks/auth/permissions.ts`). The TS file is the negative
  reference: hardcoded `ROLE_PERMISSIONS` map ≈ 90 LOC of pure data
  that should be DSL.

---

## Problem

Today an author declares roles **implicitly** by mentioning
`@role.admin` / `@role.sales` / `@role.viewer` inside `policies`
blocks (`examples/full-capsule/full-capsule.lzi:253-270`). Doctor
harvests the set by **text-walking every `policies` dictionary** and
extracting `@role.<name>` tokens
(`crates/lazuli_cli/src/doctor.rs:8121-8170` — `collect_known_roles`).
That set then gates `command.approval by @role.<name>` references
(`crates/lazuli_cli/src/doctor.rs:8003-8060`). There is no `permission`
concept at all — every check is "actor has role X for effect Y".

Three concrete shortcomings:

1. **No closed permission catalog.** A real product has 10-30 named
   verbs (`proposals:read`, `users:create`, `multi_bank:consult`,
   `report:repasse:mark`, ...). Lazuli has no DSL surface for them;
   they live in handler files, registry strings, or are smuggled in
   as policy atom names that pretend to be roles. The
   production-grade `Actions` enum
   (`c:/Users/lucas/dev-trabalho/production-grade/apps/api/src/building-blocks/auth/permissions.ts:36-65`)
   is the canonical negative example: 17 named verbs hand-rolled in
   TS, with a separate `ROLE_PERMISSIONS` map at L75-92 mapping each
   role to a literal list of verbs.

2. **Role definition is implicit and order-dependent.** `@role.admin`
   "exists" only because some `policies` block somewhere typed it.
   Add a typo (`@role.admin_` instead of `@role.admin`) in an out-of-
   the-way feature and doctor accepts it silently — the typo becomes
   a new role with zero permissions and silently fails closed at
   runtime. There is no `KNOWN-ROLE-001` diagnostic because the
   knowledge set is reflective.

3. **No closure semantics.** Even ignoring (1), the question "what
   does `@role.sales_manager` allow?" is not answerable from the
   source. A reader has to grep every `policies` block in every
   feature, dedupe by hand, and assemble the closure. An LLM cold-
   reading the fixture cannot summarize the role contract; today it
   can only summarize *one feature's view* of the role.

The production-grade file `permissions.ts` is the textbook miniature of this
problem: roles + actions + role-permission map + helper functions =
**90 LOC of pure data** that has zero business logic but is also
zero-decidable from `.lzi` source. Lazuli should own this contract.

## Goal

Two new top-level kinds in `.lzi`:

- `permission <resource>:<action>` — declares one entry in the closed
  permission catalog.
- `role <name>` block — declares one role plus the permissions it
  grants (directly or via `inherits`).

Plus two policy-side extensions:

- `has_permission <resource>:<action>` — closed predicate inside
  `policy` expressions.
- `has_role <name>` — closed predicate inside `policy` expressions.

Plus five doctor diagnostics that turn the catalog into invariants.

The surface stays **wire-thin**: the catalog produces a compile-time
static Go table; the runtime owns role assignment (DB-backed
`user_role` membership), but the **catalog itself** lives in source.

## Non-goals (out of scope for v0.1)

- **Attribute-based access control (ABAC)**. No `condition` /
  `when` / `with attributes` clauses. Strictly role × permission.
  ABAC is a separate proposal if/when pilot pressure surfaces.
- **DB-driven permission catalogs**. Casbin-style runtime mutability
  of which permission a role grants is **out of scope** —
  `@plugin/casbin` or similar may exist later. The Lazuli catalog is
  compile-time. Role *assignment* per user is DB-driven (the user's
  current role lives on a membership resource); the catalog is not.
- **Multi-parent role inheritance**. Single-parent only for v0.1; see
  §"Inheritance" for the decision rationale.
- **Permission groups / bundles** (`permission_group billing_admin =
  [...]`). Until two features have repeated identical multi-
  permission lists, the bundle abstraction is premature.
- **Cross-tenant role hierarchies** (e.g., "tenant A's admin is
  globally tenant B's viewer"). Out of scope; role is scoped per
  tenant, period.
- **Replacing `@role.*` policy atoms with `has_role`**. The two
  coexist: `@role.*` is the canonical *policy atom* used inside
  `policies` dictionaries (existing surface, unchanged); `has_role`
  is the closed *predicate* used inside `policy` expressions when an
  author wants explicit role-check semantics in a compound boolean.
  See §"Composition with `policy` block" for the exact rule.
- **Reusing `@role.*` as the catalog entry**. The catalog uses a bare
  `role <name>` block at top level; the `@role.<name>` callsite
  resolves against the catalog. We don't invent `@role` as a *kind
  keyword* — `@role` stays a namespace per `docs/invariants.md:250-
  255`.
- **Workspace-scoped catalogs**. v0.1 catalog is package-scoped (one
  catalog per Lazuli package = one `app.lzi` root). Future workspace
  fan-out is a separate decision.

## Surface design

### Permission declaration

Top-level in `app.lzi` (small apps) or in `features/<auth-or-shared>/<file>.lzi`
(packages with a dedicated auth/shared feature):

```lzi
permission users:read
permission users:create
permission users:delete
permission proposals:read
permission proposals:approve
permission proposals:update_own
permission multi_bank:consult
permission report:repasse:mark
```

**Grammar invariants**:

- One permission per line, top-level only.
- Identifier shape: `[a-z][a-z0-9_]*` segments separated by colons,
  minimum 2 segments, maximum 4 (`resource:action`,
  `resource:action:scope`, `resource:action:scope:qualifier`).
  Empty segments rejected. Colon-prefix or colon-suffix rejected.
- Convention encouraged: `<resource>:<action>` for 2-segment;
  `<resource>:<action>:<scope>` for 3-segment when the same action
  splits by `own | team | company` style scopes (production-grade exemplar).
  v0.1 does **not** enforce semantic interpretation of segments —
  they are opaque identifiers — but the LSP can offer completions
  scoped by the most common prefix.

**Why colon-separated** (not dot):

- `.` is already heavily overloaded (`@role.admin`,
  `customer.query.list`, `Customer.email`). Adding `proposals.read`
  to the same lexical pool maximises confusion.
- `:` is unused in Lazuli `.lzi` for anything but type binding
  (`name: Text`), where it appears in field context only. A bare
  `users:read` token at policy-predicate position cannot collide.
- a production-grade app + every comparable codebase (Casbin, AWS IAM, GCP IAM,
  Auth0) uses `:` for permission strings. Aligning with the de-
  facto industry corpus reduces LLM confusion.

### Role declaration

```lzi
role viewer
  grants
    users:read
    proposals:read

role consultor
  inherits viewer
  grants
    multi_bank:consult

role manager
  inherits consultor
  grants
    users:create
    proposals:approve

role admin
  grants_all
```

**Grammar invariants** (closed children, indent 2):

| Slot | Required | Type | Closed catalog |
|---|---|---|---|
| `inherits <role>` | optional | role name | yes (other declared roles) |
| `grants` block | optional | multi-line list of permission refs | yes (declared permissions) |
| `grants_all` | optional | shorthand | n/a |

**Exactly one of `grants` / `grants_all` / "neither"** must hold:

- `grants` (block, indent 4) — list of permission refs, one per
  line. Permission refs are bare (e.g., `users:read`) at this
  position; they resolve against the catalog.
- `grants_all` — shorthand for "every declared permission". Doctor
  warns if the role does not also `inherits` from nothing, because
  a `grants_all` role with `inherits` is contradictory (parent
  already grants its subset; adding `grants_all` produces the full
  superset regardless). The contradiction is a `RBAC-ROLE-GRANTS-
  ALL-001` warning, not an error, because there is one valid reading
  ("I intend admin even if the parent later removes permissions").
- Neither — the role's grants come purely from its `inherits` chain.
  Valid for "marker" roles, e.g., a `support_lead` that has the
  same grants as `support` but exists to be named separately.

**Why a block, not a flat list**:

- The production-grade exemplar
  (`permissions.ts:75-92`) has 4 roles × up to 18 permissions each;
  a flat list `grants [users:read, proposals:read, ...]` is
  unreadable past ~5 entries. Indented one-per-line is the standard
  Lazuli shape for any "list of named atoms" (cf. `audit fields`,
  `cache invalidates`, `notification channel <list>`). Consistency
  wins.

**Why `grants_all` as a keyword, not `grants *`**:

- `*` as a wildcard is a *value* in the closed-catalog model and
  would invite "what about `grants foo:*`" wildcard slipping in. A
  named keyword (`grants_all`) is a deliberate semantic choice; it
  parses as a sibling of `grants`, not a wildcard inside it. Doctor
  enforces that `grants_all` and `grants` are mutually exclusive
  (`RBAC-ROLE-GRANTS-EXCLUSIVE-001`).

### Inheritance — single-parent decision

**Decision: single-parent for v0.1.** Multi-parent is deferred.

Rationale, in order of weight:

1. **Closure complexity scales linearly with single-parent; combinatorially with multi-parent.**
   Single-parent → role → permission closure is a simple chain walk
   with cycle detection. Multi-parent → diamond-inheritance question
   ("if A inherits from B and C, and both grant `users:read`, is
   there a `users:read_v2` revoke axis?") requires either an
   ordering rule (positional priority) or a closure semantics
   (last-write-wins, union, intersect). All are valid; none are
   obviously correct without pilot evidence. Picking blind is the
   same anti-pattern that produced the 2026-05-12 incident in a
   different bucket.

2. **The production-grade exemplar is single-parent.** `ROLE_PERMISSIONS`
   declares 4 roles, each independent — there is no inheritance in
   the TS source. If the user *wanted* inheritance, the natural
   shape is `supervisor` inherits-from `vendedor`, which is a chain,
   not a DAG. No pilot in flight exercises the multi-parent case.

3. **LLM cold-read clarity is higher with single-parent.** "Role A
   inherits from B" is a one-line summary; "role A inherits from B
   and C with diamond resolution rule X" is a paragraph. Lazuli's
   bar is "an LLM can summarize this without docs".

4. **Promotion path is forward-compatible.** When multi-parent is
   needed, the surface extends from `inherits <role>` to either
   `inherits <role>, <role>` (comma-list) or `inherits` block (indent
   4, one per line). Both are additive; single-parent code remains
   valid.

**Cycle detection**: `RBAC-ROLE-CYCLE-001` rejects `role A inherits B`
+ `role B inherits A`. Trivial DAG check post-parse; doctor cross-
file.

**Depth limit**: v0.1 imposes no depth limit. If pilots produce
chains > 5 deep, revisit. Practical depth in real systems is 2-3.

### Where the catalog lives

Two valid placements; doctor accepts both:

1. **`app.lzi` top level** (small apps, < ~10 roles total). Same
   level as `app`, `environments`, etc. Catalog blocks fold into the
   app declaration's top-level scope.
2. **`features/<auth-feature>/<feature>.lzi` top level** (packaged
   apps). The catalog is a top-level sibling of `feature <name>`,
   not nested *inside* it. Doctor enforces uniqueness across the
   package: if both `app.lzi` and a feature file declare
   `role admin`, that is `RBAC-ROLE-DUPLICATE-001` regardless of
   physical placement.

The catalog is **never** declared inside a `feature` block. Roles +
permissions are cross-feature axes, not feature-internal contracts.
Placing them inside one feature would imply a scope they do not have.

The recommended convention is "one catalog file" per package,
typically `features/auth/auth.lzi` or `features/rbac/rbac.lzi`. The
convention is not enforced; only uniqueness is.

### Composition with the existing `policy` block

The existing `policies` dictionary
(`examples/full-capsule/full-capsule.lzi:251-270`) stays unchanged:

```lzi
policies
  capture_lead: @scope.public
  create: @role.admin, @role.sales
  update: @role.admin, @role.sales
  delete: @role.admin
  read: @scope.same_org
```

Two extensions:

1. **`@role.<name>` atoms now resolve against the catalog**
   (instead of being implicitly defined by mention). The doctor's
   existing `collect_known_roles` text walk is *replaced* by a
   strict lookup against `RoleCatalog::roles`. References to roles
   not in the catalog produce `RBAC-ROLE-UNDECLARED-001`. This is
   the load-bearing diagnostic that closes the typo class.

2. **Inside `policy` expression-position, two new predicates**:
   - `has_role <name>` — true when the actor's current role is
     `<name>` *or* a role that transitively inherits `<name>`.
   - `has_permission <resource>:<action>` — true when the actor's
     current role grants `<resource>:<action>` via the closure.

   Both predicates are usable inside compound `policy` expressions:

   ```lzi
   command multi_bank_consult
     input
       bank_id: ID required
       cpf: @semantic.CPF required
     policy authenticated and has_permission multi_bank:consult
     rate_limit "30 per hour per actor"
   ```

   The combinator (`and` / `or` / `not`) syntax mirrors the existing
   closed policy predicate language. The two new tokens (`has_role`,
   `has_permission`) extend that closed catalog by exactly two
   entries. Documenting this is a one-line addition to
   `docs/invariants.md` §"Policies".

**Disambiguation rule** for authors:

- Use `@role.X` *inside `policies` dictionary entries*. That's the
  canonical form for "this category is gated to this role". The
  catalog of policy categories (`create`, `update`, `read`, etc.)
  remains the high-level shape.
- Use `has_role X` or `has_permission X:Y` *inside `policy`
  expressions on commands/queries/jobs*. That's the predicate form
  for explicit conditional logic.
- They are not interchangeable; the dictionary form is dictionary
  syntax (atom × category map), the predicate form is predicate
  syntax (boolean expression).

Doctor enforces: `@role.<X>` inside a *predicate* position is a
deprecation warning `RBAC-POLICY-PREDICATE-FORM-001`; use
`has_role X` instead. Conversely, `has_role` / `has_permission`
inside a *dictionary entry* position is `RBAC-POLICY-DICT-FORM-
001`. The split keeps each surface single-purpose.

### Multi-tenant role assignment

Role assignment lives on a membership resource. The catalog does not
declare *how* a user receives a role — that is application-domain
data, not language-level.

Recommended product shape (not enforced by Lazuli):

```lzi
feature account
  defaults
    tenancy org

  domain
    resource UserOrgMembership
      org: Org required
      user: User required
      role: Text required
      created_at: DateTime required

    query.list mine
      filters
        user_id = ctx.actor.id
```

The `role: Text` field uses `Text` because the catalog is compile-
time but the storage type is open-ended (free string). Doctor *can*
optionally enforce that the runtime-resolved value is in the
catalog at write time — that is a **runtime concern** (lookup +
reject), wired by codegen, not language vocabulary. The DSL stays
out of "which column shape stores this".

Pilot question for future evolution: is `role: @role required` a
useful shape that makes the field strongly typed against the
catalog? **Out of scope for v0.1** — see "Open questions" below.

### Surface example: production-shaped

The production-grade exemplar from `permissions.ts` lowered to v0.1 surface:

```lzi
# features/auth/auth.lzi (top level — siblings of `feature <name>`)

permission proposals:read:own
permission proposals:read:team
permission proposals:read:company
permission proposals:update:own
permission proposals:mark_repasse
permission proposals:recalc_commission

permission team:manage
permission team:read:own

permission credentials:create:own
permission credentials:manage:team
permission credentials:manage:company

permission cms_group:manage
permission bank_table:manage

permission report:repasse:read
permission report:repasse:mark
permission report:commissions:own

permission user:manage

role vendedor
  grants
    proposals:read:own
    proposals:update:own
    credentials:create:own
    report:commissions:own

role supervisor
  inherits vendedor
  grants
    proposals:read:team
    team:read:own

role operacional
  grants
    proposals:read:company

role admin
  grants_all
```

24 lines of catalog (vs the production-grade TS file's 92 lines), with full
doctor cross-checks and one canonical declaration. An LLM reading
cold can answer "what can `supervisor` do?" by walking
`supervisor` → `vendedor` and unioning the grants. No grep across N
feature policies.

## Lowering

### IR

In `crates/lazuli_ir/src/lib.rs`, two new top-level structs surfaced
on the package's top-level (a sibling of `Feature`):

```rust
pub struct RbacCatalog {
    pub permissions: Vec<PermissionEntry>,
    pub roles: Vec<RoleEntry>,
}

pub struct PermissionEntry {
    pub name: String,                       // "users:read"
    pub segments: Vec<String>,              // ["users", "read"]
    pub origin: SpanRef,
}

pub struct RoleEntry {
    pub name: String,                       // "admin"
    pub inherits: Option<String>,           // single-parent ref
    pub grants: RoleGrants,
    pub origin: SpanRef,
    // Analyzer-derived; not authored.
    pub closure: Vec<String>,               // flattened permission list
}

pub enum RoleGrants {
    Explicit(Vec<String>),                  // permission name refs
    All,                                    // grants_all
    InheritedOnly,                          // no grants block, inherits only
}
```

The `closure` is computed in the analyzer after lowering, before
codegen consumes the IR. Cycle detection runs there; the IR JSON
serialization includes the closure so downstream consumers (codegen,
inspect) don't recompute it.

Surfacing on the package: `Package.rbac: Option<RbacCatalog>`. None
when no `role` / `permission` declared (most fixtures, including the
existing `examples/full-capsule/` which doesn't yet exercise the
catalog). Doctor accepts `None` as "no catalog declared, use the
legacy `@role.*` text-walk for backwards compatibility".

**Backwards compatibility** for fixtures without a catalog:
`@role.<X>` references continue to work via the existing
`collect_known_roles` text walk. Doctor emits an *advisory* notice
`RBAC-CATALOG-MISSING-001` (info level, not warning) suggesting the
catalog migration when the implicit set is non-empty. The notice can
be silenced per-fixture by adding an empty `# rbac-catalog: legacy`
comment marker. No fixture breaks on day one.

### Analyzer

`crates/lazuli_analyzer/` gains a new pass:
`analyze_rbac_catalog(&PackageAst) -> AnalyzedRbacCatalog`.

Pass responsibilities:

1. Reject duplicate permission names (`RBAC-PERM-DUPLICATE-001`).
2. Reject duplicate role names (`RBAC-ROLE-DUPLICATE-001`).
3. Resolve `inherits` references to known roles
   (`RBAC-ROLE-INHERIT-UNKNOWN-001`).
4. DAG check — reject cycles (`RBAC-ROLE-CYCLE-001`).
5. Resolve every `grants` entry to a known permission
   (`RBAC-PERM-UNKNOWN-001`).
6. Reject the contradictory state where a role declares both
   `grants` and `grants_all` (`RBAC-ROLE-GRANTS-EXCLUSIVE-001`).
7. Compute the flattened closure per role.

The closure algorithm is depth-first chain walk with a visited set
for cycle detection. Linear in the inheritance chain length × number
of roles. Closure memoized in IR.

### Doctor

`crates/lazuli_cli/src/doctor.rs` gains five new cross-package
diagnostics (v0.1 doctor codes; numbered to allow expansion):

| Code | Severity | Trigger |
|---|---|---|
| `RBAC-PERM-UNDECLARED-001` | error | A `policy ... has_permission X:Y` references `X:Y` not in the catalog. |
| `RBAC-ROLE-UNDECLARED-001` | error | A `policy ... has_role X` or a `@role.X` atom references `X` not in the catalog. (Replaces the silent text-walk acceptance.) |
| `RBAC-CYCLE-001` | error | Role inheritance cycle (`A → B → A`). Surface origin is the *first* `inherits` edge that closes the cycle. |
| `RBAC-PERM-UNUSED-001` | warning | A declared permission is never referenced by any role's `grants` and not transitively granted by any `grants_all` role. (When the catalog has at least one `grants_all` role, every permission is reachable — this warning fires only when **all** roles are explicit.) |
| `RBAC-MISSING-POLICY-001` | warning | Inside one feature, ≥2 commands/queries declare `policy <expr>` and ≥1 sibling command/query has no `policy`. Suspicious gap; explicit `policy @scope.public` opts out. |

Additional advisory:

- `RBAC-CATALOG-MISSING-001` *(info)* — implicit-role-set non-empty
  + no catalog declared. Migration hint.
- `RBAC-ROLE-GRANTS-ALL-001` *(warning)* — `grants_all` paired with
  `inherits` (redundant or contradictory).
- `RBAC-POLICY-PREDICATE-FORM-001` *(warning)* — `@role.X` used in
  a `policy` expression context where `has_role X` is canonical.
- `RBAC-POLICY-DICT-FORM-001` *(warning)* — `has_role` / `has_permission`
  used in a `policies` dictionary entry where `@role.X` /
  `@scope.X` atoms are canonical.

**Existing doctor code reconciliation**: `collect_known_roles` at
`crates/lazuli_cli/src/doctor.rs:8121` becomes a fallback used only
when `Package.rbac` is `None`. When the catalog is present, doctor
uses `RbacCatalog::roles` as the authority and ignores text-walked
mentions. Migration is mechanical: as soon as a fixture declares
the catalog, every implicit role becomes a declared role; doctor
keeps both code paths during transition.

### LSP

`crates/lazuli_lsp/`:

- Hover on `role <name>` shows the resolved closure (permission
  list) inline.
- Hover on `@role.<name>` callsite shows the same closure.
- Hover on `has_permission <X>:<Y>` shows which roles transitively
  grant the permission.
- Completion at `has_permission <prefix>` proposes catalog entries
  matching the prefix (`users:` → `users:read`, `users:create`,
  `users:delete`).
- Completion at `inherits <prefix>` proposes role names.
- Diagnostic for `has_permission <unknown>` matches doctor
  `RBAC-PERM-UNDECLARED-001` for in-editor feedback.

### Codegen — Go

`crates/lazuli_codegen_go/`: a new emitter
`src/emitter/rbac_catalog.rs` produces a single static Go file per
package:

```
dist/go/<app>/auth/rbac.gen.go
```

Shape:

```go
// Code generated by lazuli; DO NOT EDIT.
// source: features/auth/auth.lzi (rbac catalog)

package auth_gen

import "lazuli.dev/runtime/lazuli/auth"

// Permission catalog (closed set).
const (
    PermUsersRead              auth.Permission = "users:read"
    PermUsersCreate            auth.Permission = "users:create"
    PermProposalsRead          auth.Permission = "proposals:read"
    // ... etc
)

// Roles + closure tables.
var (
    RoleViewer   = auth.Role{Name: "viewer",   Grants: []auth.Permission{PermUsersRead, PermProposalsRead}}
    RoleConsultor = auth.Role{Name: "consultor", Grants: []auth.Permission{PermUsersRead, PermProposalsRead, PermMultiBankConsult}}
    RoleAdmin    = auth.Role{Name: "admin",    Grants: auth.AllPermissions}
)

var AllRoles = []auth.Role{RoleViewer, RoleConsultor, RoleManager, RoleAdmin}

func init() {
    auth.RegisterCatalog(AllRoles)
}
```

**The closure is baked in**: `RoleConsultor.Grants` already includes
the permissions inherited from `viewer`. Runtime does no walking;
lookup is `O(len(grants))` linear scan or `O(1)` map (runtime picks
based on permission count).

The runtime side (`runtime/go/lazuli/auth/rbac.go`, ~80 LOC) declares
three exported symbols:

- `type Permission string`
- `type Role struct { Name string; Grants []Permission }`
- `func HasPermission(ctx *Ctx, perm Permission) bool` — resolves
  the actor's current role from `ctx.Role()`, looks up the role in
  the registered catalog, returns whether `perm` is in its grants.
- `func HasRole(ctx *Ctx, roleName string) bool` — true if
  `ctx.Role() == roleName` or any catalog-declared role
  transitively inheriting from `roleName` matches `ctx.Role()`.
- `var AllPermissions = []Permission{...}` — populated by codegen
  for `grants_all` resolution.
- `func RegisterCatalog([]Role)` — called from per-package init.

**Wire-thin gate**: `rbac.go` ≤ 120 LOC. No SQL, no transport, no
provider. Pure in-memory catalog lookup. The runtime never reads the
catalog from DB — that is a deliberate design choice (the catalog is
compile-time; mutability requires regen, which is the right blast
radius for "we added a new permission").

### Codegen — TypeScript

`crates/lazuli_codegen_ts/` emits a parallel file per frontend:

```
dist/ts-<frontend>/auth/rbac.gen.ts
```

```ts
// Code generated by lazuli; DO NOT EDIT.

export const Permissions = {
  UsersRead: "users:read",
  UsersCreate: "users:create",
  // ...
} as const;

export type Permission = (typeof Permissions)[keyof typeof Permissions];

export const Roles = {
  Viewer:    { name: "viewer",    grants: ["users:read", "proposals:read"] },
  Consultor: { name: "consultor", grants: ["users:read", "proposals:read", "multi_bank:consult"] },
  // ...
} as const;

export type Role = (typeof Roles)[keyof typeof Roles];

export function hasPermission(role: string, perm: Permission): boolean {
  return Roles[role as keyof typeof Roles]?.grants.includes(perm) ?? false;
}
```

The shape mirrors the Go side and gives the frontend a typed
catalog for conditional UI rendering (`{hasPermission(user.role,
Permissions.UsersDelete) && <DeleteButton />}`).

The TS emission is audience-scoped per the existing
`[frontends.*].audiences` rule (`docs/invariants.md:519-524`): the
catalog itself is identical across frontends (one closed set) but
the actor-resolution helpers can vary by audience. v0.1 ships the
full catalog to every frontend; audience-filtering is a follow-up if
pilots show admin-only permissions leaking to public bundles.

### Highlighting

`editors/vscode/syntaxes/lazuli.tmLanguage.json` gains four new
keyword scopes:

- `permission` at top-level position.
- `role` at top-level position.
- `inherits` at indent 2 inside a `role` block.
- `grants`, `grants_all` at indent 2 inside a `role` block.
- `has_role`, `has_permission` at predicate position.

Permission tokens (`users:read`) match the existing `string-like`
scope but with a `meta.lazuli.permission-ref` marker so themes can
style them distinctly from arbitrary strings.

## Reconciliation with `bucket-admin-cycle.md`

The admin bucket cycle (`docs/proposals/bucket-admin-cycle.md`)
proposes `admin_resource Customer policy @role.admin` and a doctor
diagnostic `ADMIN-POLICY-001` checking that the admin policy is a
non-empty subset of the resource's `commands.policy` union.

Reconciliation:

- The admin cycle's `@role.admin` reference becomes a lookup
  against this catalog (instead of the text-walk). Same shape, new
  authority.
- `ADMIN-POLICY-001`'s "non-empty subset" check is a subset check
  over **roles**, not permissions. The two proposals stack: the
  admin cycle's policy narrowing is at the role level; this
  proposal's `has_permission` gives the finer-grained alternative
  if the admin cycle later wants a permission-level narrowing
  check.
- `admin_resource` does **not** introduce a new role-or-permission
  primitive. It consumes the catalog this proposal produces.
- No overlap, no rename, no deprecation. The admin cycle's
  `bucket-admin-cycle.md` is pilot-gated; this proposal is not. When
  the admin cycle opens (Cut admin), the catalog already exists.

Explicit non-overlap with `bucket-admin-cycle.md:319`:
`ADMIN-ACTION-001` checks "command policy admits `@role.admin`".
The check resolves `@role.admin` via the catalog. No code change to
the admin proposal; just the resolution authority shifts.

## Cells

### S1 — Surface lowering + IR + analyzer

**Files**:
- `crates/lazuli_syntax/src/parser.rs` — extend
  `parse_package_skeleton` (the package-level canonical-indent slice)
  to recognise top-level `permission <ident>` and `role <name>`
  blocks. Same pattern as `parse_agent`.
- `crates/lazuli_syntax/src/ast.rs` — `PackageSkeleton.permissions:
  Vec<PermissionDecl>`, `PackageSkeleton.roles: Vec<RoleDecl>`.
- `crates/lazuli_ir/src/lib.rs` — `RbacCatalog`, `PermissionEntry`,
  `RoleEntry`, `RoleGrants` (above).
- `crates/lazuli_analyzer/src/rbac.rs` (new) — `analyze_rbac_catalog`
  pass + closure computation + cycle detection.

**Tests**: snapshot tests against a new fixture
`examples/rbac-catalog/` with 6 permissions, 4 roles, one
inheritance chain, one `grants_all`. Cycle injection test.

**Acceptance**:
- `lazuli check examples/rbac-catalog/` green.
- IR JSON serializes deterministically.
- Cycle injection produces `RBAC-CYCLE-001`.

**Commit message**: `lazuli_syntax/ir/analyzer: rbac catalog lowering`.

### S2 — Doctor + LSP diagnostics

**Files**:
- `crates/lazuli_cli/src/doctor.rs` — 5 new diagnostics (above) plus
  the 4 advisory codes.
- `crates/lazuli_lsp/src/lib.rs` — hover, completion, in-editor
  diagnostics for catalog references.

**Tests**: doctor snapshot tests for each diagnostic. LSP test for
hover output (closure rendering).

**Commit message**: `doctor/lsp: RBAC-* diagnostics + hover`.

### S3 — Codegen Go

**File**: `crates/lazuli_codegen_go/src/emitter/rbac_catalog.rs` (new)
+ wire-up in the existing package emitter.

**Tests**: snapshot test against `examples/rbac-catalog/` —
generated `rbac.gen.go` matches the shape in §"Codegen — Go".

**Wire-thin gate**: generated file ≤ 60 LOC for a 10-permission, 4-
role catalog (linear in catalog size).

**Commit message**: `codegen-go: rbac catalog emitter`.

### S4 — Runtime Go

**File**: `runtime/go/lazuli/auth/rbac.go` (new).

**Spec**: §"Codegen — Go" runtime side. `Permission` type, `Role`
struct, `HasPermission` / `HasRole`, `RegisterCatalog`,
`AllPermissions`. Strict bound ≤ 120 effective LOC.

**Tests**: `rbac_test.go` — table-driven lookups, multi-role
inheritance closure, `grants_all` resolution.

**Commit message**: `auth/rbac: catalog runtime lookup`.

### S5 — Codegen TS + frontend integration

**File**: `crates/lazuli_codegen_ts/src/emitter/rbac_catalog.rs` (new).

**Tests**: snapshot test against `examples/rbac-catalog/` — generated
`rbac.gen.ts` matches §"Codegen — TypeScript".

**Commit message**: `codegen-ts: rbac catalog emitter`.

### S6 — Policy predicate extension

**Files**:
- `crates/lazuli_syntax/src/parser.rs` — extend policy expression
  parser to accept `has_role <ident>` and `has_permission <perm-ref>`.
- `crates/lazuli_ir/src/lib.rs` — extend `PolicyExpr` enum.
- `crates/lazuli_codegen_go/` — lower `has_permission` to
  `auth.HasPermission(ctx, auth.PermXxx)` callsite.
- `crates/lazuli_codegen_ts/` — lower to `hasPermission(user.role,
  Permissions.Xxx)` callsite.

**Tests**: fixture-driven — a feature in `examples/rbac-catalog/`
declares a command with `policy authenticated and has_permission
multi_bank:consult` and codegen emits the expected Go + TS shape.

**Commit message**: `policy: has_role / has_permission predicates`.

### S7 — Fixture + invariants doc

**Files**:
- `examples/rbac-catalog/` — new minimal fixture (~40 lines).
- `docs/invariants.md` — add a §"RBAC Catalog" subsection (~12
  lines) declaring the closed-grammar invariants.
- `examples/full-capsule/` — optional follow-up: declare the
  catalog observed today implicitly through `policies` blocks. Not
  required for v0.1 because back-compat via `RBAC-CATALOG-MISSING-
  001` advisory keeps the fixture green.

**Acceptance**:
- `cargo test -q` green.
- `cargo run -q -p lazuli_cli -- doctor examples/full-capsule` green.
- `cargo run -q -p lazuli_cli -- doctor examples/rbac-catalog` green
  with zero warnings.

**Commit message**: `examples: rbac-catalog fixture + invariants`.

## Acceptance (cycle-level)

- All seven cells (S1-S7) land green.
- `examples/rbac-catalog/` lowers cleanly; `lazuli inspect --format=json`
  exposes the catalog with closures computed.
- Every existing fixture (`examples/full-capsule/`, `examples/auth-
  multi-tenant/`, `examples/smoke-hello/`, `examples/auth-roundtrip/`)
  remains green via the back-compat path
  (`RBAC-CATALOG-MISSING-001` advisory only).
- Runtime `runtime/go/lazuli/auth/rbac.go` ≤ 120 effective LOC.
- Generated `rbac.gen.go` ≤ 60 LOC per package for ≤ 20-permission,
  ≤ 8-role catalogs. Linear scaling thereafter.
- Doctor codes documented in `docs/invariants.md`.

## Risks

| Risk | Mitigation |
|---|---|
| Existing fixtures break because `RBAC-ROLE-UNDECLARED-001` fires on legacy `@role.*` mentions. | The diagnostic only fires when a catalog is **declared**; fixtures with no catalog stay on the legacy text-walk path (`RBAC-CATALOG-MISSING-001` advisory only). Migration is opt-in per package. |
| Author confusion between `@role.X` (dict form) and `has_role X` (predicate form). | The two `*-FORM-001` warnings tell the author exactly which form belongs in which surface. Pilot feedback from Pleiades v2 / Hostpoint will surface if the split is unclear; revisit at that point. |
| `grants_all` masks future permissions silently. | When a new permission is added to the catalog, every `grants_all` role automatically receives it. This is by design (admin "should have everything") but can surprise authors who forgot a role is `grants_all`. `RBAC-ROLE-GRANTS-ALL-001` already documents the trade-off; LSP hover surfaces the resolved closure for any role, so the new permission is visible. |
| Multi-parent pressure surfaces from a pilot before v0.2 ships. | Promotion path is additive (§"Inheritance"). Single-parent code remains valid; the comma-list or block extension is purely additive. No back-compat break. |
| Catalog file placement convention drifts (some packages use `app.lzi`, others use `features/auth/`). | Doctor enforces uniqueness, not placement. Convention documented in `docs/project-structure.md`; Lazurite scaffold places the catalog at `features/auth/auth.lzi` by default. |
| Generated Go file pulls in every package, causing import cycle. | The catalog file lives at a stable path (`dist/go/<app>/auth/rbac.gen.go`) and exports a single registration via `init()`. Other packages import `lazuli.dev/runtime/lazuli/auth` for the type definitions only. No cycle. |
| Runtime catalog mutability ask from pilots ("can we add a permission at runtime?"). | Out-of-scope. The DSL is compile-time; runtime adds require regen. A future `@plugin/casbin` could overlay dynamic policies on top of the static catalog if the pressure ever surfaces. The static catalog stays. |
| Tenant-scoped role assignment surface drift (the membership-resource recommendation isn't enforced). | Out of scope for v0.1 — see "Open questions". Pilot evidence drives whether the language should own that shape. |

## Out of scope (deferred)

- **Multi-parent inheritance**. See §"Inheritance".
- **Permission groups / bundles**. Pilot evidence required.
- **ABAC**. Separate proposal.
- **DB-driven mutable catalog**. Out of v0 entirely.
- **Workspace-scoped (cross-package) catalogs**. v0.1 is package-
  scoped; workspace fan-out waits on pilot pressure.
- **`role: @role required` field-type shape**. Strongly typed role
  columns. Useful but invasive (adds a new type-binding axis); pilot
  evidence required.
- **Per-tenant role overrides**. The catalog is global; tenant-
  specific role-permission mappings (e.g., "in tenant A,
  `consultor` also has `proposals:approve`") are explicitly
  rejected for v0.1 because they require ABAC machinery.
- **`has_any_role [A, B]` / `has_all_permissions [X, Y]`
  shorthands**. Wait for two pilots to exercise the pattern; until
  then `(has_role A) or (has_role B)` is readable enough.
- **Permission deprecation lifecycle** (`permission X deprecated
  since "v2"`). Pilot evidence required.

## Open questions

1. **Should `role: @role required` be a typed field shape?** The
   storage column would be type-checked against the catalog at write
   time. Trade-off: invasive type-system change vs runtime-only
   validation. **Recommendation: defer**, validate via runtime on
   write for v0.1, surface a `RBAC-FIELD-TYPE-001` advisory if pilot
   pressure surfaces.

2. **Should the catalog allow `permission X aliases Y` for
   migration?** When renaming a permission (e.g.,
   `proposals:approve` → `proposals:approve_v2`), is there a
   first-class migration shape? **Recommendation: defer to v0.2**,
   match the existing `previously migrated|alias` shape
   (`docs/invariants.md:208`) but only after one pilot needs it.

3. **Should `policy_for jobs, webhooks: @actor.system`
   (`full-capsule.lzi:23`) be the canonical "system actor bypasses
   catalog" mechanism, or should the catalog itself have a
   `role system grants_all` entry?** Today `@actor.system` is the
   non-role actor token for system-issued effects, which is a
   distinct category from "the catalog's all-grants role". The two
   should not merge — `@actor.system` is for **non-human caller
   identity**, not "the role of a human admin". The catalog should
   not declare `role system`. **Recommendation: no change**;
   document the distinction explicitly in `docs/invariants.md`.

4. **Biggest open question**: **does the catalog need a `denies`
   axis?** Some products want "everyone in role X can do Y *except*
   in case Z" — a deny-list overlay on top of grants. v0.1
   intentionally has no `denies` because every concrete pilot we
   have seen (production-grade, pleiades v2, hostpoint shape) models the same
   thing as a narrower scope (`proposals:update:own` vs
   `proposals:update:team`). The pilot test is whether two pilots
   independently produce a "narrower scope" workaround that is
   awkward enough to be evidence for adding `denies`. Until then,
   v0.1 stays grant-only and the §"Composition" rules keep
   the predicate language closed.

## Grade-then-fix gate

This proposal must reach **≥ 8.5/10 with no dimension below 7** via
`lazuli-language-architect` (per
`feedback_grade_before_commit.md`). Hard blockers, the proposal
fails fast if any survive:

- **Boundary leak**: any cell pushing role assignment, membership
  storage, or runtime mutability of the catalog into Lazuli core.
  Catalog is compile-time DSL; assignment is runtime data.
- **Vocabulary drift**: introducing a new `@-namespace` (e.g.,
  `@permission.X`). The catalog must use bare `permission` and
  `role` top-level kinds + the existing `@role.X` namespace at
  callsite, **not** a new `@permission.X` namespace. See
  `docs/invariants.md:250-255` (closed namespace catalog).
- **Wire violation**: runtime `rbac.go` growing past ~120 effective
  LOC or implementing closure walking (closure is baked at codegen
  time).
- **Coverage gap**: any new IR shape that doesn't appear in
  `lazuli inspect --format=json` or any new keyword that doesn't
  have an LSP scope.
- **Multi-parent inheritance creeping in**. v0.1 is single-parent;
  evidence-free generalization is the wrong move.
- **`denies` clause creeping in**. See open question (4); pilot
  evidence required.

If any blocker survives v0.1, the proposal blocks at design time and
cells S1-S7 do not launch.

## Companion docs to update (after cells land)

- `docs/invariants.md` — new §"RBAC Catalog" subsection with the
  closed-grammar rules and the eight doctor codes.
- `docs/architecture.md` — note the per-package `rbac.gen.go`
  pattern in the auth bucket inventory.
- `docs/project-structure.md` — recommend
  `features/auth/auth.lzi` as the canonical catalog placement.
- `docs/proposals/production-readiness.md` — flip gap #8 from
  ⬜ → 🟡 (proposal exists) → 🟢 (implemented after cells land).
- `docs/proposals/bucket-admin-cycle.md` — append a one-line note in
  §"Cross-checks" pointing at this proposal as the role/permission
  authority for `ADMIN-POLICY-001`.
