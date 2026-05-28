package lazuli

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"os"
	"reflect"
	"strconv"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
)

type commandTxRunner func(context.Context, func(pgx.Tx) error) error

var runCommandTx commandTxRunner = withTx

// rateLimitGloballyDisabled reports whether
// `LAZULI_RATE_LIMIT_DISABLE` is set to a truthy value. Cheap
// per-request env read — same pattern as `RateLimit.Resolve()`'s
// `LAZULI_ENV` lookup. Recognised truthy literals: `1`, `true`,
// `yes`, `on` (case-insensitive). Anything else (incl. unset) is
// false → limits apply normally.
//
// PRODUCTION DEPLOYMENTS MUST NOT SET THIS — the limit is part
// of the auth / abuse defense. Intended for local dev + e2e test
// runs only.
func rateLimitGloballyDisabled() bool {
	switch strings.ToLower(strings.TrimSpace(os.Getenv("LAZULI_RATE_LIMIT_DISABLE"))) {
	case "1", "true", "yes", "on":
		return true
	}
	return false
}

// Handle executes the command transactionally. The runtime calls this from
// the HTTP dispatcher (and later from job triggers and webhook callbacks).
//
// Pipeline:
//  1. enforce policy
//  2. run validators (placeholder; Phase F)
//  3. open transaction for SQL effects
//  4. apply effect (insert / update / delete / return)
//  5. write guaranteed outbox and audit rows
//  6. publish events (post-commit, best-effort)
//  7. invalidate caches (placeholder; Phase H)
//
// The placeholders exist so the type signatures stay stable; later cuts
// fill them in without changing the generated dist code.
func (c *Command[I, O]) Handle(ctx *Ctx, input I) (O, error) {
	var zero O

	// 1. policy — pass the typed input so GAP-09 input-value-predicate
	// atoms (`@policy.x when input.y = "z"`) can test their `When` guard.
	if err := EvalPolicyInput(ctx, c.Policy, input); err != nil {
		return zero, err
	}

	// SECURITY (SEC-H3): enforce Command.RateLimit if declared. The IR
	// stores the env-qualified spec (default + by_env entries); resolve
	// once per request against `LAZULI_ENV` and apply the active limit.
	// Failing the check returns 429 to the client.
	//
	// `ir-rate-limit-env-aware` Cell 2: `c.RateLimit.Resolve()` is a
	// linear scan over `ByEnv` (at most 5 entries). Empty resolved
	// limit == no throttle — short-circuit before parsing.
	//
	// Operator escape hatch: `LAZULI_RATE_LIMIT_DISABLE=true` skips
	// enforcement entirely for ALL commands. Intended for local dev +
	// e2e test runs where the limit is noise (e.g. registerUser hit
	// 429s after a handful of test users). PRODUCTION DEPLOYMENTS
	// MUST NOT SET THIS — the limit is part of the auth / abuse
	// defense. The variable is read per request (no boot caching) so
	// tests can toggle it mid-suite.
	resolvedLimit := c.RateLimit.Resolve()
	if rateLimitGloballyDisabled() {
		resolvedLimit = ""
	}
	if strings.TrimSpace(resolvedLimit) != "" {
		spec, err := parseRateLimitSpec(resolvedLimit)
		if err != nil {
			slog.Error("lazuli: malformed rate_limit literal", "command", c.Name, "spec", resolvedLimit, "err", err)
			// Continue without enforcement — fail-open on parse error because
			// malformed specs are config bugs, not attacks.
		} else {
			key := buildRateLimitKey(ctx, c.Name, spec)
			storeCtx := context.Background()
			if ctx != nil && ctx.Context != nil {
				storeCtx = ctx
			}
			count, err := activeStore.Inc(storeCtx, key, spec.Window)
			if err != nil {
				slog.Error("lazuli: rate_limit store error", "err", err)
				// Fail-open on store error — don't lock out users on infra blip.
			} else if count > spec.Limit {
				return zero, ErrRateLimited
			}
		}
	}

	// 2. validators — run in declaration order, abort on first failure.
	for _, ref := range c.Validators {
		fn := LookupValidator(ref.Canonical())
		if fn == nil {
			return zero, &Error{Status: 500, Code: CodeInternal,
				Message: "validator not registered: " + ref.Canonical()}
		}
		if err := fn(ctx, input); err != nil {
			// Wrap non-Lazuli errors so the response carries a typed
			// validation_failed envelope.
			if _, ok := err.(*Error); ok {
				return zero, err
			}
			return zero, &Error{Status: 400, Code: CodeValidationFailed,
				Message: err.Error()}
		}
	}

	var output O
	if eff, ok := c.Effect.(ReturnsEffect); ok {
		if len(c.Transitions) > 0 {
			return zero, &Error{Status: 500, Code: CodeInternal,
				Message: "lifecycle transitions require an updates effect"}
		}
		out, err := applyReturns[I, O](ctx, eff, input)
		if err != nil {
			return zero, err
		}
		output = out
	} else {
		// 3-4. SQL effects run inside a transaction.
		err := runCommandTx(ctx, func(tx pgx.Tx) error {
			var lifecycleTarget *lifecycleTransitionTarget
			if len(c.Transitions) > 0 {
				target, err := resolveLifecycleTransitionTarget(ctx, c.Effect, input)
				if err != nil {
					return err
				}
				if err := lockLifecycleTransition(ctx, tx, target, c.Transitions); err != nil {
					return err
				}
				lifecycleTarget = &target
			}

			out, err := applyEffect[I, O](ctx, tx, c.Effect, input)
			if err != nil {
				return err
			}
			output = out
			if lifecycleTarget != nil {
				if err := advanceLifecycleTransition(ctx, tx, *lifecycleTarget, c.Transitions); err != nil {
					return err
				}
			}
			// EVENT-OUTBOX §3.3 — write outbox rows for guaranteed
			// emits in the same tx as the resource mutation. Failure
			// here rolls back the resource write, preserving the
			// atomicity invariant.
			if err := writeGuaranteedOutboxRows(ctx, tx, c.Emits, c.Effect, input, out); err != nil {
				return err
			}
			if c.Audit != nil {
				if err := writeAuditRow(ctx, tx, c.Name, "allowed"); err != nil {
					return err
				}
			}
			return nil
		})
		if err != nil {
			return zero, err
		}
	}

	// 5. emits (post-commit, best-effort). Guaranteed emits are
	// dispatched by the outbox pump (see runtime/lazuli/events), so
	// publishEmits skips them.
	publishEmits(ctx, c.Emits, c.EmitsTrace, c.Effect, input, output)

	// 7. invalidate caches whose results are now stale.
	if len(c.Invalidates) > 0 {
		queryCache.invalidateQueries(c.Invalidates)
	}

	return output, nil
}

type lifecycleTransitionTarget struct {
	Resource *resourceErased
	IDColumn string
	IDValue  any
}

func resolveLifecycleTransitionTarget[I any](ctx *Ctx, effect Effect, input I) (lifecycleTransitionTarget, error) {
	eff, ok := effect.(UpdatesEffect)
	if !ok {
		return lifecycleTransitionTarget{}, &Error{Status: 500, Code: CodeInternal,
			Message: "lifecycle transitions require an updates effect"}
	}
	if eff.Resource == nil {
		return lifecycleTransitionTarget{}, &Error{Status: 500, Code: CodeInternal,
			Message: "lifecycle transition updates effect has no resource"}
	}
	col, src, ok := lifecycleIDBinding(eff.Where)
	if !ok {
		return lifecycleTransitionTarget{}, &Error{Status: 500, Code: CodeInternal,
			Message: "lifecycle transition updates effect requires an id where binding"}
	}
	id, err := resolveSource(ctx, src, input)
	if err != nil {
		return lifecycleTransitionTarget{}, err
	}
	return lifecycleTransitionTarget{
		Resource: eff.Resource,
		IDColumn: col,
		IDValue:  id,
	}, nil
}

func lifecycleIDBinding(where Bindings) (string, Source, bool) {
	if src, ok := where["id"]; ok {
		return "id", src, true
	}
	if len(where) != 1 {
		return "", Source{}, false
	}
	for col, src := range where {
		return col, src, true
	}
	return "", Source{}, false
}

func lockLifecycleTransition(
	ctx *Ctx,
	tx pgx.Tx,
	target lifecycleTransitionTarget,
	transitions []TransitionAdvance,
) error {
	sql := fmt.Sprintf(
		`SELECT lifecycle_state FROM %s WHERE %s = $1 FOR UPDATE`,
		quoteResourceTable(target.Resource.Name),
		quoteIdent(target.IDColumn),
	)
	var actual string
	if err := tx.QueryRow(ctx, sql, target.IDValue).Scan(&actual); err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			return &Error{Status: 404, Code: CodeNotFound,
				Message: "no row matches lifecycle transition guard"}
		}
		return classifyDBError("lifecycle transition guard", err)
	}
	expected := transitions[0].From
	if actual != expected {
		return &LifecycleStateMismatchError{
			Expected:    expected,
			Actual:      actual,
			Transitions: transitionChainNames(transitions),
		}
	}
	return nil
}

func advanceLifecycleTransition(
	ctx *Ctx,
	tx pgx.Tx,
	target lifecycleTransitionTarget,
	transitions []TransitionAdvance,
) error {
	next := transitions[len(transitions)-1].To
	sql := fmt.Sprintf(
		`UPDATE %s SET "lifecycle_state" = $1 WHERE %s = $2`,
		quoteResourceTable(target.Resource.Name),
		quoteIdent(target.IDColumn),
	)
	if _, err := tx.Exec(ctx, sql, next, target.IDValue); err != nil {
		return classifyDBError("lifecycle transition advance", err)
	}
	return nil
}

func transitionChainNames(transitions []TransitionAdvance) []string {
	names := make([]string, 0, len(transitions))
	for _, t := range transitions {
		names = append(names, t.From+"->"+t.To)
	}
	return names
}

// publishEmits derives event payloads from the producing command's effect
// (or from explicit Bind maps) and publishes them through the in-process
// event bus. Errors are logged but never propagated — emits are post-commit.
//
// EVENT-OUTBOX §3.3 — emits with `Outbox: OutboxGuaranteed` are written
// to `lazuli_outbox` inside the producing tx; the outbox pump dispatches
// them. We skip them here to avoid double-delivery.
func publishEmits[I, O any](ctx *Ctx, emits []EventEmit, traces []EventTraceEmit, effect Effect, input I, output O) {
	for _, emit := range emits {
		if emit.Outbox == OutboxGuaranteed {
			continue
		}
		payload, err := buildEmitPayload(ctx, emit, effect, input, output)
		if err != nil {
			continue
		}
		Publish(ctx, eventFromCtx(ctx, emit.Name, false, payload))
	}
	for _, t := range traces {
		payload, err := resolveBindings(ctx, t.Bind, input)
		if err != nil {
			continue
		}
		Publish(ctx, eventFromCtx(ctx, t.Name, true, payload))
	}
}

// writeGuaranteedOutboxRows inserts a `lazuli_outbox` row for each
// emit with `Outbox: OutboxGuaranteed`. Runs inside the same pgx tx
// as the resource mutation (EVENT-OUTBOX §3.3) so the resource row
// and the outbox row commit together — or roll back together if the
// outbox INSERT fails.
func writeGuaranteedOutboxRows[I, O any](
	ctx *Ctx,
	tx pgx.Tx,
	emits []EventEmit,
	effect Effect,
	input I,
	output O,
) error {
	for _, emit := range emits {
		if emit.Outbox != OutboxGuaranteed {
			continue
		}
		payload, err := buildEmitPayload(ctx, emit, effect, input, output)
		if err != nil {
			return err
		}
		raw, err := json.Marshal(payload)
		if err != nil {
			return err
		}
		envelopeID, err := newEnvelopeID()
		if err != nil {
			return err
		}
		if _, err := tx.Exec(ctx,
			`INSERT INTO lazuli_outbox (envelope_id, event_name, payload)
			 VALUES ($1, $2, $3)`,
			envelopeID, emit.Name, raw,
		); err != nil {
			return err
		}
	}
	return nil
}

// writeAuditRow inserts a row into lazuli_audit inside the supplied
// tx so audit emission is transactional with the command effect.
// Best-effort serialization of tenant id; failures bubble so the
// command rolls back rather than committing without trail.
func writeAuditRow(ctx *Ctx, tx pgx.Tx, command, decision string) error {
	actor := string(ctx.Actor)
	var tenant *string
	if ctx.Tenant != nil {
		s := strconv.FormatInt(int64(ctx.Tenant.OrgID), 10)
		tenant = &s
	}
	_, err := tx.Exec(ctx, `INSERT INTO lazuli_audit (command, actor, tenant, decision) VALUES ($1, $2, $3, $4)`,
		command, actor, tenant, decision)
	return err
}

// newEnvelopeID returns a fresh 128-bit hex identifier for the outbox
// envelope. Wire-thin: crypto/rand is stdlib and sufficient for the
// uniqueness the outbox dedup table requires.
func newEnvelopeID() (string, error) {
	var b [16]byte
	if _, err := rand.Read(b[:]); err != nil {
		return "", err
	}
	return hex.EncodeToString(b[:]), nil
}

// eventFromCtx builds the common Event envelope from the active request ctx.
func eventFromCtx(ctx *Ctx, name string, trace bool, payload map[string]any) Event {
	e := Event{
		Name:       name,
		Trace:      trace,
		Tenant:     ctx.Tenant,
		Actor:      ctx.Actor,
		Payload:    payload,
		OccurredAt: ctx.Now,
	}
	if ctx.User != nil {
		uid := ctx.User.ID
		e.UserID = &uid
	}
	return e
}

// buildEmitPayload constructs the Payload map for one emit. Derived emits
// (from creates/updates/deletes) carry the producing row in O — the runtime
// reflects O into a map so subscribers see the exact persisted state.
// Explicit emits resolve their own Bind map against input/ctx.
func buildEmitPayload[I, O any](ctx *Ctx, emit EventEmit, effect Effect, input I, output O) (map[string]any, error) {
	if emit.From == FromExplicit {
		return resolveBindings(ctx, emit.Bind, input)
	}
	// Derived: payload is the producing row. Subscribers can read any
	// persisted field via the resulting map.
	return rowToMap(output), nil
}

// resolveBindings resolves a Bindings map of `column -> Source` to a
// `column -> value` map for explicit event payloads.
func resolveBindings[I any](ctx *Ctx, bind Bindings, input I) (map[string]any, error) {
	out := make(map[string]any, len(bind))
	for col, src := range bind {
		val, err := resolveSource(ctx, src, input)
		if err != nil {
			return nil, err
		}
		out[col] = val
	}
	return out, nil
}

// rowToMap reflects an exported struct into a {column: value} map keyed by
// each field's `db` tag (preferred) or `json` tag (fallback) or lowercased
// name (last resort). The output is what subscribers consume.
func rowToMap(v any) map[string]any {
	out := map[string]any{}
	rv := reflect.ValueOf(v)
	if rv.Kind() == reflect.Pointer {
		rv = rv.Elem()
	}
	if rv.Kind() != reflect.Struct {
		return out
	}
	rt := rv.Type()
	for i := 0; i < rv.NumField(); i++ {
		ft := rt.Field(i)
		if !ft.IsExported() {
			continue
		}
		name := ft.Tag.Get("db")
		if name == "" {
			name = strings.SplitN(ft.Tag.Get("json"), ",", 2)[0]
		}
		if name == "" {
			name = strings.ToLower(ft.Name)
		}
		if name == "-" {
			continue
		}
		out[name] = rv.Field(i).Interface()
	}
	return out
}

// atomMatches reports whether the active context satisfies one leaf
// policy atom. The full policy walker (`EvalPolicy` in policy.go)
// composes these results across `and` / `or` / `not` combinators when
// the atom slice carries a structured expression.
//
// Atom semantics:
//
//	@actor.user        → ctx.Actor == ActorUser
//	@actor.system      → ctx.Actor == ActorSystem
//	@actor.anonymous   → ctx.Actor == ActorAnonymous
//	@role.<name>       → ctx.User != nil AND <name> in ctx.User.Roles
//	@scope.public      → always
//	@scope.authenticated → ctx.User != nil
//	@scope.same_org    → ctx.Tenant != nil
//	@scope.self/owner  → returns false until target loading lands
//	rbac.role.<n>      → any of ctx.User.Roles satisfies HasRole(role, n)
//	rbac.permission.X  → any of ctx.User.Roles satisfies HasPermission(role, X)
//	predicate.authenticated → ctx.User != nil (combinator leaf)
func atomMatches(ctx *Ctx, atom PolicyAtom) bool {
	switch atom.Namespace {
	case "actor":
		switch atom.Name {
		case "user":
			return ctx.Actor == ActorUser
		case "system":
			return ctx.Actor == ActorSystem
		case "anonymous":
			return ctx.Actor == ActorAnonymous
		}
	case "role":
		if ctx.User == nil {
			return false
		}
		for _, r := range ctx.User.Roles {
			if r == atom.Name {
				return true
			}
		}
		return false
	case "scope":
		switch atom.Name {
		case "public":
			return true
		case "authenticated":
			return ctx.User != nil
		case "same_org":
			return ctx.Tenant != nil
		case "self", "owner":
			// These require a loaded target row to compare ownership.
			// Target binding is deferred; these atoms always fail closed
			// for now.
			return false
		}
	case "rbac.role":
		if ctx.User == nil {
			return false
		}
		for _, r := range ctx.User.Roles {
			if rbacHasRole(r, atom.Name) {
				return true
			}
		}
		return false
	case "rbac.permission":
		if ctx.User == nil {
			return false
		}
		for _, r := range ctx.User.Roles {
			if rbacHasPermission(r, atom.Name) {
				return true
			}
		}
		return false
	case "predicate":
		if atom.Name == "authenticated" {
			return ctx.User != nil
		}
	}
	return false
}

// applyEffect runs the registered effect against the given transaction. The
// runtime translates the typed effect (Creates/Updates/Deletes) into pgx
// queries.
func applyEffect[I, O any](ctx *Ctx, tx pgx.Tx, effect Effect, input I) (O, error) {
	var zero O
	if effect == nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "command has no effect"}
	}
	switch eff := effect.(type) {
	case CreatesEffect:
		return applyCreates[I, O](ctx, tx, eff, input)
	case UpdatesEffect:
		return applyUpdates[I, O](ctx, tx, eff, input)
	case DeletesEffect:
		return applyDeletes[I, O](ctx, tx, eff, input)
	case ReturnsEffect:
		return applyReturns[I, O](ctx, eff, input)
	case ReorderEffect:
		return applyReorder[I, O](ctx, tx, eff, input)
	default:
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: fmt.Sprintf("unknown effect kind: %T", effect)}
	}
}

// applyReorder runs a `reorder <Resource> by <position>` command (W4
// GAP-REORDER-01). It reads the ordered id list from the request input
// (field "ids" — a slice of ids in the desired order) and emits a single
// batch UPDATE that sets the position column to each id's index in the list
// via a `VALUES (id, position)` join. Wire-thin: one statement, no
// per-row round trips, no homegrown ordering. Returns the zero output value
// (reorder commands are side-effecting; O is typically `struct{}`).
func applyReorder[I, O any](ctx *Ctx, tx pgx.Tx, eff ReorderEffect, input I) (O, error) {
	var zero O
	idsRaw, err := readPath(reflect.ValueOf(input), "ids")
	if err != nil {
		return zero, &Error{Status: 400, Code: CodeBadRequest,
			Message: "reorder command requires an ordered `ids` list"}
	}
	ids, ok := toAnySlice(idsRaw)
	if !ok {
		return zero, &Error{Status: 400, Code: CodeBadRequest,
			Message: "reorder `ids` must be a list"}
	}
	if len(ids) == 0 {
		return zero, nil
	}

	// Build a single `UPDATE <table> SET <pos> = data.position
	// FROM (VALUES ($1,0),($2,1),...) AS data(id, position) WHERE
	// <table>.id = data.id` statement. The VALUES rows pair each id with
	// its 0-based ordinal in the supplied order.
	values := make([]any, 0, len(ids))
	tuples := make([]string, 0, len(ids))
	for i, id := range ids {
		values = append(values, id)
		tuples = append(tuples, fmt.Sprintf("($%d, %d)", len(values), i))
	}
	sql := fmt.Sprintf(
		`UPDATE %s SET %s = data.position FROM (VALUES %s) AS data(id, position) WHERE %s.id = data.id`,
		quoteResourceTable(eff.Resource.Name),
		quoteIdent(eff.PositionColumn),
		strings.Join(tuples, ", "),
		quoteResourceTable(eff.Resource.Name),
	)
	if _, err := tx.Exec(ctx, sql, values...); err != nil {
		return zero, classifyDBError("reorder", err)
	}
	return zero, nil
}

// toAnySlice coerces a `readPath` result into a `[]any` so the reorder
// effect can iterate the ordered ids regardless of the concrete slice type
// the JSON decoder produced.
func toAnySlice(v any) ([]any, bool) {
	switch s := v.(type) {
	case []any:
		return s, true
	case []string:
		out := make([]any, len(s))
		for i, e := range s {
			out[i] = e
		}
		return out, true
	case []int64:
		out := make([]any, len(s))
		for i, e := range s {
			out[i] = e
		}
		return out, true
	default:
		return nil, false
	}
}

// applyReturns calls the user-authored pure handler without entering the SQL
// effect pipeline. Policy, validators, rate limiting, and audit hooks live
// above this dispatch point in Command.Handle.
func applyReturns[I, O any](ctx *Ctx, eff ReturnsEffect, input I) (O, error) {
	var zero O
	if eff.Handler == nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "returns effect requires Handler"}
	}
	raw, err := eff.Handler(ctx, input)
	if err != nil {
		return zero, err
	}
	out, ok := raw.(O)
	if !ok {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "returns handler produced output of wrong type"}
	}
	return out, nil
}

// applyCreates resolves the bindings, builds an `INSERT ... RETURNING *`,
// and scans the inserted row into O. Tenancy auto-injection adds `org_id`
// when the resource declares `TenancyOrg` and the request has a tenant.
func applyCreates[I, O any](ctx *Ctx, tx pgx.Tx, eff CreatesEffect, input I) (O, error) {
	var zero O

	cols := make([]string, 0, len(eff.Bind))
	values := make([]any, 0, len(eff.Bind))
	placeholders := make([]string, 0, len(eff.Bind))
	for col, src := range eff.Bind {
		val, err := resolveSource(ctx, src, input)
		if err != nil {
			return zero, err
		}
		// `conventions [crud]` synth `create_<resource>` emits
		// `FromInputOptional` for every non-required input slot — when
		// the wire payload omits the field, skip the column so the DB
		// default (or NULL) takes over. `val == nil` only matches the
		// nil-pointer case from `readPath`; a wire `"col": 0` / `""`
		// still resolves to a non-nil zero value and is preserved.
		if src.kind == sourceInputOptional && val == nil {
			continue
		}
		cols = append(cols, quoteIdent(col))
		values = append(values, val)
		placeholders = append(placeholders, fmt.Sprintf("$%d", len(values)))
	}

	// WAR-RUNTIME-MULTITENANT-01 closure: under `@policy.public`, the
	// session-bound tenant is nil, but the resource still requires
	// `org_id`. If the host app has registered a default-tenant
	// resolver via `lazuli.WithDefaultTenant`, invoke it now and pin
	// the resolved tenant for the remainder of the request. Calls
	// outside this codepath (authed commands with session-bound
	// tenant) are unaffected — the resolver runs only when ctx.Tenant
	// is currently nil.
	if eff.Resource.Tenancy == TenancyOrg && ctx.Tenant == nil {
		resolved, err := resolveDefaultTenant(ctx)
		if err != nil {
			return zero, internalServerError(err, "default tenant resolver failed")
		}
		if resolved != nil {
			ctx.Tenant = resolved
		}
	}

	if eff.Resource.Tenancy == TenancyOrg && ctx.Tenant != nil {
		// The TenancyOrg migration emit produces TWO columns:
		//   * org_id — auto-tenancy column injected by the DDL emitter
		//   * org    — FK column derived from the `org: Org required` field
		// Both are NOT NULL, so both must be populated. Authors don't bind
		// either column explicitly — the runtime mirrors the session.go
		// INSERT pattern (`(org_id, org, ...) VALUES ($1, $1, ...)`) so a
		// single tenant value satisfies both NOT NULL constraints.
		// Skip whichever column the bindings have already filled.
		for _, col := range [...]string{"org_id", "org"} {
			if _, bound := eff.Bind[col]; bound {
				continue
			}
			cols = append(cols, quoteIdent(col))
			values = append(values, ctx.Tenant.OrgID)
			placeholders = append(placeholders, fmt.Sprintf("$%d", len(values)))
		}
	}

	// Auto-populate `updated_at = now()` on INSERT when the resource
	// declares the column and the bindings don't already set it.
	// Mirrors the UPDATE-path bump at line ~818 / ~947 so authors don't
	// have to repeat `updated_at = ctx.now` in every `creates` block.
	// Resources without an `updated_at` column (e.g. ones declared with
	// `conventions [timestamps off]`) are unaffected — `HasColumn`
	// returns false and the branch is a no-op. Symmetry: the runtime
	// keeps responsibility for the audit-timestamp pair (created_at +
	// updated_at), authors only bind business fields.
	if eff.Resource.HasColumn("updated_at") {
		if _, bound := eff.Bind["updated_at"]; !bound {
			cols = append(cols, quoteIdent("updated_at"))
			values = append(values, time.Now().UTC())
			placeholders = append(placeholders, fmt.Sprintf("$%d", len(values)))
		}
	}

	// Encrypt @cap.Encrypted / @cap.E2ee bound columns before they
	// reach the driver. The runtime walks `Resource.EncryptedColumns`
	// (populated by codegen) and replaces each plaintext value with
	// AES-256-GCM ciphertext via `encryption.ForCtx`. No-op when the
	// resource has no encrypted fields.
	if err := encryptColumnValues(ctx, eff.Resource, cols, values); err != nil {
		return zero, internalServerError(err, "insert encrypt failed")
	}

	// `ir-resource-conventions-owner-scope.md` §8.5.A —
	// CTE-prefixed INSERT for `@owner_axis` resources. The CTE
	// verifies the actor owns the FK target row; when it returns
	// zero rows, the INSERT fires zero times and the downstream
	// `CollectOneRow` returns `ErrNoRows`, which we map to a 403
	// `not_owner` envelope below. Tenant-only INSERTs (nil
	// OwnerCheck) skip this branch entirely.
	var sql string
	if eff.OwnerCheck != nil {
		oc := eff.OwnerCheck
		// Find the placeholder index for the FK column in the
		// already-resolved `cols`/`values` arrays. The codegen
		// populates the binding via the standard `Bind` map so the
		// FK value lands among the row values; the CTE just
		// references the same `$N` slot.
		fkPlaceholder := 0
		for i, col := range cols {
			if unquoteIdent(col) == oc.FKColumn {
				fkPlaceholder = i + 1
				break
			}
		}
		if fkPlaceholder == 0 {
			return zero, &Error{Status: 500, Code: CodeInternal,
				Message: "owner-check FK column not bound: " + oc.FKColumn}
		}
		// Append the actor's user.ID as the last placeholder
		// (`$<N+1>`) for the CTE's `<through> = $<actor>` clause.
		actor, err := readCtx(ctx, "user.id")
		if err != nil {
			return zero, err
		}
		values = append(values, actor)
		actorPlaceholder := len(values)
		sql = fmt.Sprintf(
			`WITH owner_check AS (SELECT 1 FROM %s WHERE id = $%d AND %s = $%d) `+
				`INSERT INTO %s (%s) SELECT %s FROM owner_check RETURNING *`,
			quoteIdent(oc.RelatedTable),
			fkPlaceholder,
			quoteIdent(oc.ThroughColumn),
			actorPlaceholder,
			quoteResourceTable(eff.Resource.Name),
			strings.Join(cols, ", "),
			strings.Join(placeholders, ", "),
		)
	} else {
		sql = fmt.Sprintf(
			`INSERT INTO %s (%s) VALUES (%s) RETURNING *`,
			quoteResourceTable(eff.Resource.Name),
			strings.Join(cols, ", "),
			strings.Join(placeholders, ", "),
		)
	}

	rows, err := tx.Query(ctx, sql, values...)
	if err != nil {
		return zero, classifyDBError("insert", err)
	}
	defer rows.Close()

	out, err := pgx.CollectOneRow(rows, pgx.RowToStructByName[O])
	if errors.Is(err, pgx.ErrNoRows) && eff.OwnerCheck != nil {
		// Zero-row CTE → zero-row INSERT → ErrNoRows. Map to the
		// canonical `not_owner` envelope. Tenant-only INSERT errors
		// continue through `classifyDBError`.
		return zero, &Error{Status: 403, Code: CodePolicyDenied,
			Message: "actor does not own the referenced row"}
	}
	if err != nil {
		return zero, classifyDBError("insert scan", err)
	}
	// Decrypt server-readable encrypted fields on the returned row so
	// downstream code (events, response bodies, audit) sees plaintext.
	if err := decryptScannedRow(ctx, eff.Resource, &out); err != nil {
		return zero, internalServerError(err, "insert decrypt failed")
	}
	return out, nil
}

// whereConditionFragment renders a single WHERE-clause condition for
// a (column, source) pair, honoring the `sourceCtxOwnedVia` shape
// that expands to `<col> IN (SELECT id FROM <related> WHERE
// <owner_col> = $N)`. All other Source kinds collapse to the
// scalar `<col> = $N` form. The caller has already appended the
// resolved value to `values`, so `placeholderIdx == len(values)`
// is the 1-based position for the `$N` reference.
//
// Closes the relation-traversal arm of `@scope.owner` per the
// the canonical pilot Phase 4 capability audit (2026-05-17).
func whereConditionFragment(col string, src Source, placeholderIdx int) string {
	if src.kind == sourceCtxOwnedVia && src.subquery != nil {
		return fmt.Sprintf(
			"%s IN (SELECT id FROM %s WHERE %s = $%d)",
			quoteIdent(col),
			quoteIdent(src.subquery.relatedTable),
			quoteIdent(src.subquery.ownerColumn),
			placeholderIdx,
		)
	}
	return fmt.Sprintf("%s = $%d", quoteIdent(col), placeholderIdx)
}

// applyUpdates resolves the where + bind sources, builds an `UPDATE ... SET
// ... WHERE ... RETURNING *`, and scans the updated row into O. Adds
// tenancy + soft-delete scoping to the WHERE clause.
func applyUpdates[I, O any](ctx *Ctx, tx pgx.Tx, eff UpdatesEffect, input I) (O, error) {
	var zero O
	if len(eff.Where) == 0 {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "updates effect requires Where bindings"}
	}
	if len(eff.Bind) == 0 {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "updates effect requires Bind bindings"}
	}

	values := make([]any, 0, len(eff.Bind)+len(eff.Where))
	sets := make([]string, 0, len(eff.Bind))
	// Track the bind-side column names parallel to `values[0..n-1]` so
	// `encryptColumnValues` can match each value to its `EncryptedColumns`
	// scope. The WHERE-side bindings appended later are intentionally
	// excluded — encrypted columns are never WHERE-keys (the cipher
	// nonce makes equality lookups impossible).
	bindCols := make([]string, 0, len(eff.Bind))
	for col, src := range eff.Bind {
		val, err := resolveSource(ctx, src, input)
		if err != nil {
			return zero, err
		}
		// `conventions [crud]` synth `update_<resource>` emits
		// `FromInputOptional` for every input slot — when the wire
		// payload omits the field, skip the SET clause so the existing
		// column value stays put (partial-update semantics). `val ==
		// nil` only matches the nil-pointer case from `readPath`; a
		// wire `"col": 0` / `""` still resolves to a non-nil zero
		// value and is written.
		if src.kind == sourceInputOptional && val == nil {
			continue
		}
		values = append(values, val)
		bindCols = append(bindCols, col)
		sets = append(sets, fmt.Sprintf("%s = $%d", quoteIdent(col), len(values)))
	}
	// Encrypt @cap.Encrypted / @cap.E2ee bound columns before the
	// WHERE values are appended. Only the SET-side bindings are
	// candidates.
	if err := encryptColumnValues(ctx, eff.Resource, bindCols, values[:len(bindCols)]); err != nil {
		return zero, internalServerError(err, "update encrypt failed")
	}
	// Bump `updated_at = now()` only when the resource declares the
	// column. Resources without `timestamps` (DSL `defaults.timestamps
	// = false` or per-resource `timestamps off`) have no `updated_at`
	// column; appending the SET clause unconditionally would raise
	// PG 42703 (undefined_column) at execute time.
	if eff.Resource.HasColumn("updated_at") {
		sets = append(sets, `"updated_at" = now()`)
	}
	// Partial-update no-op: every Bind binding was `sourceInputOptional`
	// with a nil value AND the resource has no `updated_at` column, so
	// there is nothing to SET. Composing `UPDATE foo SET WHERE …` is
	// invalid SQL — instead, SELECT the row that the WHERE would have
	// matched and return it unchanged, mirroring what the UPDATE …
	// RETURNING * would have produced if a noop SET were legal.
	if len(sets) == 0 {
		return selectByEffectWhere[I, O](ctx, tx, eff, input)
	}

	conds, condValues, err := baseScopeConditions(ctx, eff.Resource, len(values)+1)
	if err != nil {
		return zero, err
	}
	values = append(values, condValues...)
	for col, src := range eff.Where {
		val, err := resolveSource(ctx, src, input)
		if err != nil {
			return zero, err
		}
		values = append(values, val)
		conds = append(conds, whereConditionFragment(col, src, len(values)))
	}

	sql := fmt.Sprintf(
		`UPDATE %s SET %s WHERE %s RETURNING *`,
		quoteResourceTable(eff.Resource.Name),
		strings.Join(sets, ", "),
		strings.Join(conds, " AND "),
	)

	rows, err := tx.Query(ctx, sql, values...)
	if err != nil {
		return zero, classifyDBError("update", err)
	}
	defer rows.Close()

	out, err := pgx.CollectOneRow(rows, pgx.RowToStructByName[O])
	if errors.Is(err, pgx.ErrNoRows) {
		return zero, &Error{Status: 404, Code: CodeNotFound,
			Message: "no row matches update where clause"}
	}
	if err != nil {
		return zero, classifyDBError("update scan", err)
	}
	if err := decryptScannedRow(ctx, eff.Resource, &out); err != nil {
		return zero, internalServerError(err, "update decrypt failed")
	}
	return out, nil
}

// selectByEffectWhere resolves the WHERE side of an UpdatesEffect and
// returns the matching row unchanged. Used as the no-op fallback when
// every Bind binding was a `sourceInputOptional` with a nil value AND
// the resource has no `updated_at` column to bump — in that case
// composing `UPDATE … SET … WHERE …` would yield invalid SQL with an
// empty SET. Mirrors the SELECT shape of `applyUpdates` so the caller
// sees the same row payload UPDATE … RETURNING * would have produced.
func selectByEffectWhere[I, O any](ctx *Ctx, tx pgx.Tx, eff UpdatesEffect, input I) (O, error) {
	var zero O
	conds, values, err := baseScopeConditions(ctx, eff.Resource, 1)
	if err != nil {
		return zero, err
	}
	for col, src := range eff.Where {
		val, err := resolveSource(ctx, src, input)
		if err != nil {
			return zero, err
		}
		values = append(values, val)
		conds = append(conds, whereConditionFragment(col, src, len(values)))
	}
	sql := fmt.Sprintf(
		`SELECT * FROM %s WHERE %s LIMIT 1`,
		quoteResourceTable(eff.Resource.Name),
		strings.Join(conds, " AND "),
	)
	rows, err := tx.Query(ctx, sql, values...)
	if err != nil {
		return zero, classifyDBError("update noop select", err)
	}
	defer rows.Close()
	out, err := pgx.CollectOneRow(rows, pgx.RowToStructByName[O])
	if errors.Is(err, pgx.ErrNoRows) {
		return zero, &Error{Status: 404, Code: CodeNotFound,
			Message: "no row matches update where clause"}
	}
	if err != nil {
		return zero, classifyDBError("update noop scan", err)
	}
	if err := decryptScannedRow(ctx, eff.Resource, &out); err != nil {
		return zero, internalServerError(err, "update noop decrypt failed")
	}
	return out, nil
}

// applyDeletes resolves the where bindings and either soft-deletes or
// hard-deletes the matching row, returning the affected row in O. Soft
// delete is the canonical path when `Resource.SoftDelete` is true.
func applyDeletes[I, O any](ctx *Ctx, tx pgx.Tx, eff DeletesEffect, input I) (O, error) {
	var zero O
	if len(eff.Where) == 0 {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "deletes effect requires Where bindings"}
	}

	conds, values, err := baseScopeConditions(ctx, eff.Resource, 1)
	if err != nil {
		return zero, err
	}
	for col, src := range eff.Where {
		val, err := resolveSource(ctx, src, input)
		if err != nil {
			return zero, err
		}
		values = append(values, val)
		conds = append(conds, whereConditionFragment(col, src, len(values)))
	}

	var sql string
	if eff.Resource.SoftDelete {
		// `deleted_at` always exists on soft-delete resources. The
		// `updated_at` bump is only appended when the resource also
		// declares timestamps — otherwise the SET clause references
		// an undefined column (PG 42703).
		softSets := []string{`"deleted_at" = now()`}
		if eff.Resource.HasColumn("updated_at") {
			softSets = append(softSets, `"updated_at" = now()`)
		}
		sql = fmt.Sprintf(
			`UPDATE %s SET %s WHERE %s RETURNING *`,
			quoteResourceTable(eff.Resource.Name),
			strings.Join(softSets, ", "),
			strings.Join(conds, " AND "),
		)
	} else {
		sql = fmt.Sprintf(
			`DELETE FROM %s WHERE %s RETURNING *`,
			quoteResourceTable(eff.Resource.Name),
			strings.Join(conds, " AND "),
		)
	}

	rows, err := tx.Query(ctx, sql, values...)
	if err != nil {
		return zero, classifyDBError("delete", err)
	}
	defer rows.Close()

	out, err := pgx.CollectOneRow(rows, pgx.RowToStructByName[O])
	if errors.Is(err, pgx.ErrNoRows) {
		return zero, &Error{Status: 404, Code: CodeNotFound,
			Message: "no row matches delete where clause"}
	}
	if err != nil {
		return zero, classifyDBError("delete scan", err)
	}
	if err := decryptScannedRow(ctx, eff.Resource, &out); err != nil {
		return zero, internalServerError(err, "delete decrypt failed")
	}
	return out, nil
}

// resolveSource turns a binding Source into a concrete value at execution
// time. Reflection-based for the v0 spike; later cuts will let codegen emit
// typed accessors.
func resolveSource[I any](ctx *Ctx, src Source, input I) (any, error) {
	switch src.kind {
	case sourceConst:
		return src.value, nil
	case sourceInput, sourceInputOptional:
		return readPath(reflect.ValueOf(input), src.path)
	case sourceCtx, sourceCtxOwnedVia:
		return readCtx(ctx, src.path)
	case sourceTarget:
		return nil, &Error{Status: 501, Code: CodeInternal,
			Message: "target binding not yet implemented in runtime spike"}
	case sourceFn:
		// WAR-VOCAB-CREATES-FN-CALL-01 closure: `@fn.<name>(<arg>...)`
		// resolves each arg source first, then invokes the registered
		// BindingFn with the resolved args. Fail-closed: unknown fn
		// names abort the command rather than emit a confusing
		// downstream type error.
		fn, ok := lookupBindingFn(src.path)
		if !ok {
			return nil, &Error{Status: 500, Code: CodeInternal,
				Message: "binding fn not registered: @fn." + src.path}
		}
		argSources, _ := src.value.([]Source)
		args := make([]any, 0, len(argSources))
		for _, arg := range argSources {
			v, err := resolveSource(ctx, arg, input)
			if err != nil {
				return nil, err
			}
			args = append(args, v)
		}
		return fn(ctx, args...)
	default:
		return nil, &Error{Status: 500, Code: CodeInternal,
			Message: fmt.Sprintf("unknown source kind: %d", src.kind)}
	}
}

// readPath looks up `path` (e.g. "name", "owner.id") on the given reflect
// Value. Tolerates struct field names that differ in case from the DSL by
// trying PascalCase variants.
//
// Single-level pointer fields are auto-dereferenced so downstream logic
// (filter equality, search pattern building, JSON encoding) sees the
// underlying value. A nil pointer field returns nil so `isNilOrZero`
// correctly skips optional filters.
func readPath(v reflect.Value, path string) (any, error) {
	if v.Kind() == reflect.Pointer {
		v = v.Elem()
	}
	for _, part := range strings.Split(path, ".") {
		if v.Kind() != reflect.Struct {
			return nil, &Error{Status: 500, Code: CodeInternal,
				Message: "input path " + path + " hit non-struct value"}
		}
		field := v.FieldByName(pascalCase(part))
		if !field.IsValid() {
			field = v.FieldByName(part)
		}
		if !field.IsValid() {
			return nil, &Error{Status: 400, Code: CodeBadRequest,
				Message: "input field not found: " + path}
		}
		v = field
	}
	if v.Kind() == reflect.Pointer {
		if v.IsNil() {
			return nil, nil
		}
		v = v.Elem()
	}
	return v.Interface(), nil
}

// readCtx pulls a value from the request context by canonical DSL path.
func readCtx(ctx *Ctx, path string) (any, error) {
	switch path {
	case "user", "user.id":
		if ctx.User == nil {
			return nil, &Error{Status: 401, Code: CodePolicyDenied,
				Message: "no authenticated user in context"}
		}
		return ctx.User.ID, nil
	case "user.org_id", "user.org":
		if ctx.User == nil {
			return nil, &Error{Status: 401, Code: CodePolicyDenied,
				Message: "no authenticated user in context"}
		}
		return ctx.User.OrgID, nil
	case "actor.user_id":
		if ctx.User == nil {
			return nil, &Error{Status: 401, Code: CodePolicyDenied,
				Message: "no authenticated user in context"}
		}
		return ctx.User.ID, nil
	case "actor.org_id":
		if ctx.User != nil {
			return ctx.User.OrgID, nil
		}
		if ctx.Tenant != nil {
			return ctx.Tenant.OrgID, nil
		}
		return nil, &Error{Status: 400, Code: CodeTenantMismatch,
			Message: "no actor org in context"}
	case "tenant.org_id":
		if ctx.Tenant == nil {
			return nil, &Error{Status: 400, Code: CodeTenantMismatch,
				Message: "no tenant in context"}
		}
		return ctx.Tenant.OrgID, nil
	case "now":
		if !ctx.Now.IsZero() {
			return ctx.Now, nil
		}
		return time.Now(), nil
	default:
		return nil, &Error{Status: 500, Code: CodeInternal,
			Message: "unknown ctx path: " + path}
	}
}

// pascalCase converts snake_case to PascalCase ("first_name" -> "FirstName").
func pascalCase(s string) string {
	parts := strings.Split(s, "_")
	for i, p := range parts {
		if len(p) == 0 {
			continue
		}
		parts[i] = strings.ToUpper(p[:1]) + p[1:]
	}
	return strings.Join(parts, "")
}

// Boot wires the runtime: opens the DB pool. Call once at process startup
// before the HTTP server begins serving.
func Boot(ctx context.Context, dbURL string) error {
	pool, err := connectDB(ctx, dbURL)
	if err != nil {
		return err
	}
	SetDB(pool)
	return nil
}
