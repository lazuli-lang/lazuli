# dist/go — Lazuli runtime spike

Hand-written demonstration of the **target shape** that
`crates/lazuli_codegen_go` will produce from the IR. Once the runtime API
locks, codegen reproduces files like these mechanically.

## Layout

```
dist/go/
├── customer/customer.gen.go    # Resource + Command declarations (the "thin" generated code)
├── main.go                     # Process entrypoint (boot runtime + serve HTTP)
├── migrations/001_customer.sql # SQL DDL (later derived from IR)
├── docker-compose.yml          # Postgres for local dev
├── go.mod                      # imports lazuli.dev/runtime via replace directive
└── README.md
```

## What this spike proves

The DSL block

```lazuli
command create
  input
    name: Text required
    email: @semantic.Email @pii.contact required
  policy @policy.create
  rate_limit "30 per hour per ip"
  creates Customer
    name = input.name
    email = input.email
  emits customer_created from creates
  invalidates
    query.list
    query.global_search
```

becomes:

```go
type CreateCustomerInput struct {
    Name  string `json:"name"`
    Email string `json:"email"`
}

var createCustomer = lazuli.Command[CreateCustomerInput, Customer]{
    Name:      "customer.create",
    Resource:  &customerResource,
    Policy:    lazuli.Policy{Name: "@policy.create", Atoms: []lazuli.PolicyAtom{...}},
    RateLimit: "30 per hour per ip",
    Audit:     lazuli.AuditDefault,
    Effect: lazuli.Creates(&customerResource, lazuli.Bindings{
        "name":  lazuli.FromInput("name"),
        "email": lazuli.FromInput("email"),
    }),
    Emits: []lazuli.EventEmit{
        {Name: "customer_created", From: lazuli.FromCreates},
    },
    Invalidates: []string{"customer.query.list", "customer.query.global_search"},
}

func init() { lazuli.Register(&customerResource, &createCustomer) }
```

That's the entire generated Go for one command — about 20 lines of
declarative wiring. All execution logic (policy enforcement, validators,
transaction lifecycle, INSERT building, audit, event publish, cache
invalidation) lives in `runtime/go/lazuli/` and runs the same way for
every command.

## Run locally

```sh
# 1. start Postgres + apply schema (host port 55432 to avoid clashing with a
#    Postgres already listening on 5432)
docker compose up -d
docker compose exec postgres pg_isready

# 2. point the runtime at the Postgres container
export LAZULI_DB="postgres://lazuli:lazuli@localhost:55432/lazuli?sslmode=disable"
export LAZULI_ADDR=":8088"   # any free port works

# 3. boot the server
go run .

# 4. create a customer (command)
curl -X POST http://localhost:8088/api/v1/c/customer.create \
  -H 'Content-Type: application/json' \
  -d '{"name":"Acme Co","email":"hello@acme.example"}'
# expected: {"id":1,"org_id":0,"name":"","email":""}

# 5. list customers (query)
curl -X POST http://localhost:8088/api/v1/q/customer.query.list \
  -H 'Content-Type: application/json' -d '{}'
# expected: [{"id":1,"org_id":0,"name":"Acme Co",...}, ...]

# 6. lookup by id (query)
curl -X POST http://localhost:8088/api/v1/q/customer.query.by_id \
  -H 'Content-Type: application/json' -d '{"id":1}'
# expected: {"id":1,"org_id":0,"name":"Acme Co",...}

# 7. lookup not found
curl -X POST http://localhost:8088/api/v1/q/customer.query.by_id \
  -H 'Content-Type: application/json' -d '{"id":999}'
# expected: {"code":"not_found","message":"no row matches lookup keys"}

# 8. verify rows landed
docker compose exec postgres psql -U lazuli -d lazuli \
  -c "SELECT id, name, email FROM customer;"
```

`org_id` is 0 in the response because the spike has no auth wired yet —
`ctx.Tenant` is nil, so the migration's `org_id BIGINT NOT NULL DEFAULT 0`
takes over. `name` and `email` come back empty because `Handle` only
populates the returned struct's `ID` field; row materialisation arrives
with the query layer in Phase B.

## Phase B added

Queries now work end-to-end. The runtime grew `Query[A, R]` with three
kinds (list / lookup / sql), a SELECT builder that injects `deleted_at IS
NULL` and tenancy scoping automatically, optional filters, ordering, and
limit. HTTP routes mount as `POST /api/v1/q/<query-name>`.

The DSL block

```lazuli
query.list list
  paginate 50

query.lookup by_id by id: ID
```

becomes:

```go
var listCustomers = lazuli.Query[ListCustomersArgs, Customer]{
    Name:     "customer.query.list",
    Resource: &customerResource,
    Kind:     lazuli.QueryList,
    Policy:   lazuli.Policy{Name: "@policy.read", Atoms: []lazuli.PolicyAtom{...}},
    Paginate: 50,
}

var customerByID = lazuli.Query[CustomerByIDArgs, Customer]{
    Name:     "customer.query.by_id",
    Resource: &customerResource,
    Kind:     lazuli.QueryLookup,
    Policy:   lazuli.Policy{...},
    LookupBy: []lazuli.LookupKey{
        {Column: "id", Source: lazuli.FromInput("ID")},
    },
}
```

Empty registration; the runtime executes the SELECT.

## What's missing (post-Phase-B)

The runtime spike implements the **happy path for `creates`**. These are
explicit placeholders, not bugs:

- **Policy**: `enforcePolicy` accepts everything. The auth cut wires real
  RBAC against `ctx.Actor` / `ctx.User` / `ctx.Tenant`.
- **Validators**: `Validators` field is read but not invoked. The
  validator cut runs them in declaration order, with `let`/`requires` semantics.
- **Audit**: `Audit` field is read but no record is written. The audit cut
  wires the audit log table + writer.
- **Event publishing**: `Emits` and `EmitsTrace` are read but not delivered.
  The event cut adds an in-process bus + Postgres-backed durable queue
  (river).
- **Cache invalidation**: `Invalidates` is read but no signal is sent.
  Phase B adds the query layer + cache invalidation hooks.
- **Tenancy resolution**: `Ctx.Tenant` is always nil (no auth). Once
  auth lands, `WHERE org_id = ctx.tenant.org_id` is enforced
  automatically via `Resource.Tenancy`.
- **Updates / deletes effects**: only `creates` is wired in this phase.
  Phase C adds the rest.
- **Soft-delete + retention**: `SoftDelete` and `Retention` are stored
  but inert. Phase C makes `DELETE` set `deleted_at`; retention runs as
  a scheduled job.
- **Output materialisation**: `Handle` returns a Customer struct with
  only `ID` populated. Once the query layer lands, the runtime can
  re-select the inserted row.
