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
			allowed, _, err := activeStore.Allow(storeCtx, key, spec.Limit, spec.Window)
			if err != nil {
				slog.Error("lazuli: rate_limit store error", "err", err)
				// Fail-open on store error — don't lock out users on infra blip.
			} else if !allowed {
				return zero, ErrRateLimited
			}
		}
	}

	// 2a. struct-tag input validation — W4-6. The codegen stamps
	// `validate:"required"` on every required input field; without this
	// pass a create with an empty body (`create_agency {}`) sailed through
	// to the INSERT and returned a 200 all-zero row. Enforce the declared
	// `required` rule (and other future rules) on the decoded input BEFORE
	// any effect runs, returning a 400 validation_failed envelope listing
	// the offending field(s). Commands with all-optional inputs or no
	// inputs declare no `required` tags, so they are unaffected.
	if err := validateInputTags(input); err != nil {
		return zero, err
	}

	// 2b. validators — run in declaration order, abort on first failure.
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
				// The lifecycle discriminator column (e.g.
				// `registration_step`) is authored on the resource; default
				// to `lifecycle_state` only when codegen predates the
				// LifecycleColumn field. Hardcoding `lifecycle_state` broke
				// every pilot whose discriminator has a different name
				// (PG 42703 undefined column at the transition pre-guard).
				target.StateColumn = c.LifecycleColumn
				if target.StateColumn == "" {
					target.StateColumn = "lifecycle_state"
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
				// GAP-AUDIT-01 — `audit materialize @feature.<f>.<R>`.
				// Write the SAME assembled audit record into the declared
				// append_only OperationLog, in the same tx (one record,
				// two sinks). Doctor guarantees the table is append_only.
				if c.Audit.MaterializeTable != "" {
					if err := writeAuditMaterializeRow(ctx, tx, c.Audit.MaterializeTable, c.Name, "allowed"); err != nil {
						return err
					}
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
	// StateColumn is the lifecycle discriminator column SELECTed by the
	// pre-guard and written by the advance. Set by the caller from
	// Command.LifecycleColumn (default `lifecycle_state`).
	StateColumn string
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
	stateCol := target.StateColumn
	if stateCol == "" {
		stateCol = "lifecycle_state"
	}
	// W1-1 (SEC-LIFECYCLE-NOTENANT) — the pre-guard SELECT must carry the
	// same tenant/org scope predicate every other read/write gets, or an
	// attacker can drive ANOTHER tenant's row through a lifecycle transition
	// by guessing its id (cross-tenant IDOR). The id is bound at $1; tenant
	// scope placeholders start at $2.
	values := []any{target.IDValue}
	conds := []string{quoteIdent(target.IDColumn) + " = $1"}
	scopeConds, scopeValues, err := baseScopeConditions(ctx, target.Resource, len(values)+1)
	if err != nil {
		return err
	}
	conds = append(conds, scopeConds...)
	values = append(values, scopeValues...)
	sql := fmt.Sprintf(
		`SELECT %s FROM %s WHERE %s FOR UPDATE`,
		quoteIdent(stateCol),
		quoteResourceTable(target.Resource.Name),
		strings.Join(conds, " AND "),
	)
	var actual string
	if err := tx.QueryRow(ctx, sql, values...).Scan(&actual); err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			return &Error{Status: 404, Code: CodeNotFound,
				Message: "no row matches lifecycle transition guard"}
		}
		return classifyDBError("lifecycle transition guard", err)
	}
	// MULTI-SOURCE (fan-in) guard: the resource's current lifecycle_state
	// must be a MEMBER of the entry transition's from-set, not equal to a
	// single state. A single-`from` transition is a 1-element set, so this
	// stays back-compatible.
	if !transitions[0].allowsFrom(actual) {
		return &LifecycleStateMismatchError{
			Expected:    transitions[0].From,
			ExpectedAny: transitions[0].froms(),
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
	stateCol := target.StateColumn
	if stateCol == "" {
		stateCol = "lifecycle_state"
	}
	// W1-1 (SEC-LIFECYCLE-NOTENANT) — the advance UPDATE must carry the same
	// tenant/org scope predicate as the lock SELECT, or a cross-tenant id
	// silently mutates another tenant's row. State value is bound at $1, the
	// id at $2; tenant scope placeholders start at $3.
	values := []any{next, target.IDValue}
	conds := []string{quoteIdent(target.IDColumn) + " = $2"}
	scopeConds, scopeValues, err := baseScopeConditions(ctx, target.Resource, len(values)+1)
	if err != nil {
		return err
	}
	conds = append(conds, scopeConds...)
	values = append(values, scopeValues...)
	sql := fmt.Sprintf(
		`UPDATE %s SET %s = $1 WHERE %s`,
		quoteResourceTable(target.Resource.Name),
		quoteIdent(stateCol),
		strings.Join(conds, " AND "),
	)
	if _, err := tx.Exec(ctx, sql, values...); err != nil {
		return classifyDBError("lifecycle transition advance", err)
	}
	return nil
}

func transitionChainNames(transitions []TransitionAdvance) []string {
	names := make([]string, 0, len(transitions))
	for _, t := range transitions {
		// Render the full from-set for fan-in transitions
		// (`{A|B}->C`); single-from stays `A->C`.
		from := strings.Join(t.froms(), "|")
		if len(t.froms()) > 1 {
			from = "{" + from + "}"
		}
		names = append(names, from+"->"+t.To)
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

// auditRecord is the ctx-derived audit row assembled once per command and
// written into the canonical `audit_log` table (and, when declared, the
// materialize OperationLog sink). Its columns map 1:1 onto the codegen-emitted
// `audit_log` DDL (crates/lazuli_codegen_go/src/emitter/audit.rs), reconciling
// the two faces of the audit subsystem (codegen DDL vs runtime INSERT) onto one
// schema. Pointer fields are NULL when the ctx does not carry the value.
type auditRecord struct {
	OrgID         *int64 // audit_log.org_id     — NULL when no tenant
	ActorID       *int64 // audit_log.actor_id   — NULL for system/anonymous
	ActorKind     string // audit_log.actor_kind — 'user' | 'system' | 'anonymous'
	CommandName   string // audit_log.command_name — qualified, e.g. "account.signup"
	ResultStatus  string // audit_log.result_status — 'ok' | 'error'
	ErrorCode     *string
	CorrelationID *string
}

// auditDecisionToResultStatus maps the runtime's coarse `decision` token onto
// the `audit_log.result_status` domain ('ok' | 'error'). Allow/commit outcomes
// are successes; deny/error outcomes are failures. The single in-tree caller
// passes "allowed" (the audit row is written only on the post-effect success
// path), but the mapping stays total so a future deny/error caller records the
// right status instead of leaking the raw token.
func auditDecisionToResultStatus(decision string) string {
	switch strings.ToLower(strings.TrimSpace(decision)) {
	case "allow", "allowed", "commit", "committed", "ok", "success":
		return "ok"
	default:
		return "error"
	}
}

// assembleAuditRecord derives the audit_log row from the request ctx. actor_id
// is the authenticated user's id (NULL for system/anonymous); actor_kind is
// always set (the column is NOT NULL). org_id comes from the active tenant.
func assembleAuditRecord(ctx *Ctx, command, decision string) auditRecord {
	rec := auditRecord{
		CommandName:  command,
		ResultStatus: auditDecisionToResultStatus(decision),
	}
	if ctx == nil {
		rec.ActorKind = string(ActorSystem)
		return rec
	}
	if ctx.Tenant != nil {
		org := int64(ctx.Tenant.OrgID)
		rec.OrgID = &org
	}
	// actor_kind defaults to the ctx Actor token; for a user actor also stamp
	// actor_id. An empty Actor with a populated User is treated as a user.
	switch {
	case ctx.Actor == ActorUser || (ctx.Actor == "" && ctx.User != nil):
		rec.ActorKind = string(ActorUser)
		if ctx.User != nil {
			id := int64(ctx.User.ID)
			rec.ActorID = &id
		}
	case ctx.Actor == "":
		rec.ActorKind = string(ActorSystem)
	default:
		rec.ActorKind = string(ctx.Actor)
	}
	if rec.ResultStatus == "error" {
		code := classifyAuditErrorCode(decision)
		rec.ErrorCode = &code
	}
	if cid := auditCorrelationID(ctx); cid != "" {
		rec.CorrelationID = &cid
	}
	return rec
}

// auditCorrelationID recovers the request correlation id from the ctx, falling
// back to the request-id middleware value stashed on the underlying context.
func auditCorrelationID(ctx *Ctx) string {
	if ctx.RequestID != "" {
		return ctx.RequestID
	}
	if ctx.Context != nil {
		return RequestID(ctx.Context)
	}
	return ""
}

// classifyAuditErrorCode derives a stable error_code token for a non-ok audit
// row. The runtime only ever passes a coarse decision string here, so the code
// echoes the (deny/error) token rather than inventing a richer taxonomy.
func classifyAuditErrorCode(decision string) string {
	d := strings.ToLower(strings.TrimSpace(decision))
	if d == "" {
		return "error"
	}
	return d
}

// writeAuditRow inserts a row into the canonical `audit_log` table inside the
// supplied tx so audit emission is transactional with the command effect.
// Columns map onto the codegen-emitted DDL; happened_at relies on the DDL's
// DEFAULT NOW(). Failures bubble so the command rolls back rather than
// committing without a trail.
func writeAuditRow(ctx *Ctx, tx pgx.Tx, command, decision string) error {
	rec := assembleAuditRecord(ctx, command, decision)
	_, err := tx.Exec(ctx,
		`INSERT INTO audit_log (org_id, actor_id, actor_kind, command_name, result_status, error_code, correlation_id) `+
			`VALUES ($1, $2, $3, $4, $5, $6, $7)`,
		rec.OrgID, rec.ActorID, rec.ActorKind, rec.CommandName, rec.ResultStatus, rec.ErrorCode, rec.CorrelationID)
	return err
}

// writeAuditMaterializeRow inserts the SAME assembled audit record into a
// declared append_only OperationLog table (GAP-AUDIT-01 `audit materialize
// @feature.<f>.<R>`). It runs inside the command tx so materialization is
// atomic with the effect + the audit_log row — one record, two sinks.
// Wire-thin: reuses the exact ctx-derived record (org_id, actor_id, actor_kind,
// command_name, result_status, error_code, correlation_id) that writeAuditRow
// assembles; no new audit logic. The table is a compile-time literal
// (snake-cased target resource emitted by codegen) verified append_only by
// doctor, so the identifier interpolation carries no untrusted input. The
// OperationLog columns mirror audit_log so the same record shape lands in both
// sinks; `recorded_at` is the append_only ledger's own write timestamp.
func writeAuditMaterializeRow(ctx *Ctx, tx pgx.Tx, table, command, decision string) error {
	rec := assembleAuditRecord(ctx, command, decision)
	recordedAt := time.Now().UTC()
	if ctx != nil && !ctx.Now.IsZero() {
		recordedAt = ctx.Now
	}
	// %s is a codegen-emitted snake_case table literal, never user input;
	// pgx.Identifier.Sanitize quotes it (defense-in-depth on the identifier slot).
	sql := fmt.Sprintf(
		`INSERT INTO %s (org_id, actor_id, actor_kind, command_name, result_status, error_code, correlation_id, recorded_at) `+
			`VALUES ($1, $2, $3, $4, $5, $6, $7, $8)`,
		pgx.Identifier{table}.Sanitize(),
	)
	_, err := tx.Exec(ctx, sql,
		rec.OrgID, rec.ActorID, rec.ActorKind, rec.CommandName, rec.ResultStatus, rec.ErrorCode, rec.CorrelationID, recordedAt)
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
		switch atom.Name {
		case "authenticated":
			return ctx.User != nil
		case "deny":
			// SECURITY (POLICY-REF-UNRESOLVED): codegen emits a lone
			// `predicate.deny` atom when an author-declared policy reference
			// could not be resolved to its atom list (cross-feature
			// `PolicyRef::External`, or an unknown `@policy.<name>`). It must
			// NEVER be satisfiable — the command/query is denied (403) until
			// the reference is fixed. Fail closed, never open.
			return false
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

	// Sanitize `validate sanitize_html(<profile>)` bound columns before
	// they reach the driver (and before encryption, so the plaintext is
	// scrubbed first). The runtime walks `Resource.SanitizeColumns`
	// (populated by codegen) and rewrites each bound string value through
	// the matching bluemonday policy. No-op when the resource has no
	// sanitized fields. This is the patch that turns the previously-no-op
	// `sanitize_html` constraint into a real stored-XSS guard.
	if err := sanitizeColumnValues(eff.Resource, cols, values); err != nil {
		return zero, internalServerError(err, "insert sanitize failed")
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
	// Sanitize `validate sanitize_html(<profile>)` bound columns before
	// the WHERE values are appended (and before encryption). Only the
	// SET-side bindings are candidates. No-op when the resource declares
	// no sanitized fields.
	if err := sanitizeColumnValues(eff.Resource, bindCols, values[:len(bindCols)]); err != nil {
		return zero, internalServerError(err, "update sanitize failed")
	}
	// Encrypt @cap.Encrypted / @cap.E2ee bound columns before the
	// WHERE values are appended. Only the SET-side bindings are
	// candidates.
	if err := encryptColumnValues(ctx, eff.Resource, bindCols, values[:len(bindCols)]); err != nil {
		return zero, internalServerError(err, "update encrypt failed")
	}
	// Bump `updated_at = now()` only when the resource declares the
	// column AND the author did not already bind it in the SET map.
	// Resources without `timestamps` (DSL `defaults.timestamps = false`
	// or per-resource `timestamps off`) have no `updated_at` column;
	// appending the SET clause unconditionally would raise PG 42703
	// (undefined_column). And when the author wrote an explicit
	// `updated_at = ctx.now` (already in `eff.Bind`, emitted above),
	// appending a second `updated_at = now()` raises PG 42601
	// ("multiple assignments to same column") — mirror the INSERT path's
	// `if _, bound := eff.Bind["updated_at"]; !bound` guard.
	if _, bound := eff.Bind["updated_at"]; !bound && eff.Resource.HasColumn("updated_at") {
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
		// Spec 0015 — `soft_delete by` stamps the deleting actor into
		// `deleted_by` (mirroring `deleted_at = now()`). The value comes
		// from `ctx.actor` (the authenticated user's id); a system /
		// anonymous actor with no user resolves to NULL — the column is
		// nullable, matching the hand-rolled `deleted_by: ID optional`
		// pairs this trait folds in. Bound as a positional placeholder so
		// the id is never string-interpolated into SQL.
		if eff.Resource.SoftDeleteActor {
			var actorID any
			if ctx != nil && ctx.User != nil {
				actorID = ctx.User.ID
			}
			values = append(values, actorID)
			softSets = append(softSets, fmt.Sprintf(`"deleted_by" = $%d`, len(values)))
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
		// resolves each arg source first, then invokes the resolved
		// BindingFn with the resolved args. resolveBindingFn bridges the
		// explicit binding-fn registry AND the handler registry that
		// pilots actually populate via `RegisterFn("<feature>.<name>")`,
		// reconciling the bare-vs-qualified naming. Fail-closed: a fn in
		// NEITHER registry (or an ambiguous suffix match) aborts the
		// command rather than emit a confusing downstream type error.
		fn, ok := resolveBindingFn(src.path)
		if !ok {
			return nil, &Error{Status: 500, Code: CodeInternal,
				Message: "binding fn not registered in either registry: @fn." + src.path}
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
		// Thread the live *Ctx through as the context arg so a
		// handler-registry fn (signature `func(*Ctx, I) (O, error)`)
		// can recover it; explicit BindingFns just see a context.Context.
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
		field := resolveField(v, part)
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
	case "actor.user_id", "actor.id":
		// BUG #17 — `ctx.actor.id` lowers to FromCtx("actor.id"); it is an
		// alias of `actor.user_id` (the acting user's row id). Without
		// this arm, `where id = ctx.actor.id` 500'd with "unknown ctx
		// path: actor.id".
		if ctx.User == nil {
			return nil, &Error{Status: 401, Code: CodePolicyDenied,
				Message: "no authenticated user in context"}
		}
		return ctx.User.ID, nil
	case "actor.org_id", "actor.tenant_id":
		// W4-4 — `ctx.actor.tenant_id` lowers to FromCtx("actor.tenant_id").
		// The framework carries one tenant identifier per actor: the org/
		// tenant scope on the session (`User.OrgID`, falling back to the
		// active `Tenant.OrgID`). Pilots that model multi-tenancy via an
		// app-level `tenant_id` column (e.g. pauta's `User.tenant_id`) seed
		// that value as the actor's org-id at the transport boundary, so the
		// two paths resolve to the same identifier. Without this arm, any
		// filter binding `ctx.actor.tenant_id` (pauta `user_by_id`,
		// `users_in_tenant`) 500'd with "unknown ctx path: actor.tenant_id".
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

// validateInputTags enforces the declared struct-tag validation rules on a
// decoded command input BEFORE any effect runs (W4-6). The codegen stamps
// `validate:"required"` on every required input field; this is the runtime
// half that actually enforces it. Today the only rule honoured is `required`
// (the field's value must not be the type's zero value); unknown rules are
// ignored so the contract can grow new rules without breaking old binaries.
//
// Returns a 400 `validation_failed` *Error listing every offending field's
// wire (json) name, or nil when the input satisfies every declared rule.
// Inputs with no `validate` tags (all-optional or no-input commands) always
// pass.
func validateInputTags[I any](input I) error {
	v := reflect.ValueOf(input)
	for v.Kind() == reflect.Pointer {
		if v.IsNil() {
			return nil
		}
		v = v.Elem()
	}
	if v.Kind() != reflect.Struct {
		return nil
	}
	missing := collectMissingRequired(v)
	if len(missing) == 0 {
		return nil
	}
	msg := "missing required field"
	if len(missing) > 1 {
		msg = "missing required fields"
	}
	return &Error{
		Status:     400,
		Code:       CodeValidationFailed,
		Message:    msg + ": " + strings.Join(missing, ", "),
		MessageKey: CodeValidationFailed,
		Data:       map[string]any{"fields": missing},
		Base: ErrorBase{
			Status:  400,
			Code:    CodeValidationFailed,
			Message: msg + ": " + strings.Join(missing, ", "),
			Surface: SurfaceUserDSL,
		},
	}
}

// collectMissingRequired walks a struct value and returns the wire (json)
// names of every field tagged `validate:"required"` whose value is the zero
// value. Anonymous (embedded) structs are recursed into so promoted fields
// are checked too. A nil pointer or empty string/slice/map counts as missing.
func collectMissingRequired(v reflect.Value) []string {
	var missing []string
	t := v.Type()
	for i := 0; i < t.NumField(); i++ {
		field := t.Field(i)
		if field.PkgPath != "" {
			continue // unexported
		}
		fv := v.Field(i)
		if field.Anonymous {
			ev := fv
			for ev.Kind() == reflect.Pointer && !ev.IsNil() {
				ev = ev.Elem()
			}
			if ev.Kind() == reflect.Struct {
				missing = append(missing, collectMissingRequired(ev)...)
			}
			continue
		}
		if !tagHasRule(field.Tag.Get("validate"), "required") {
			continue
		}
		if isZeroForValidation(fv) {
			name := jsonTagName(field.Tag.Get("json"))
			if name == "" {
				name = field.Name
			}
			missing = append(missing, name)
		}
	}
	return missing
}

// tagHasRule reports whether a `validate` tag's comma-separated rule list
// contains the named rule (e.g. "required" in "required,email").
func tagHasRule(tag, rule string) bool {
	if tag == "" || tag == "-" {
		return false
	}
	for _, part := range strings.Split(tag, ",") {
		if strings.TrimSpace(part) == rule {
			return true
		}
	}
	return false
}

// isZeroForValidation reports whether a value should be treated as "missing"
// for a `required` check. Pointers are missing when nil; strings/slices/maps
// when empty; everything else when it equals the type's zero value. This
// mirrors go-playground/validator's `required` semantics closely enough for
// the framework's needs without vendoring it.
func isZeroForValidation(v reflect.Value) bool {
	switch v.Kind() {
	case reflect.Pointer, reflect.Interface:
		return v.IsNil()
	case reflect.Slice, reflect.Map:
		return v.Len() == 0
	default:
		return v.IsZero()
	}
}

// resolveField finds the struct field on v that a DSL binding path part
// names. Command bindings pass the wire key (snake_case, e.g. "agency_id");
// query bindings pass the Go field name (e.g. "AgencyID"). Both must resolve
// to the same field, which the codegen emits with an acronym-aware caser
// ("agency_id" -> AgencyID) plus a `json:"agency_id"` tag.
//
// The lookup tries, in order:
//  1. exact field name (covers query-style PascalCase and DSL-exact parts);
//  2. acronym-aware PascalCase (covers command-style snake_case via the same
//     algorithm the emitter uses, so AgencyID/ID/HTMLBody all hit);
//  3. the `json` struct tag (drift-proof: it's the wire key the emitter
//     wrote, so it matches the binding regardless of casing rules);
//  4. case-insensitive field name (belt-and-suspenders before erroring).
//
// Returns an invalid reflect.Value if nothing matches.
func resolveField(v reflect.Value, part string) reflect.Value {
	if f := v.FieldByName(part); f.IsValid() {
		return f
	}
	if f := v.FieldByName(pascalCase(part)); f.IsValid() {
		return f
	}
	t := v.Type()
	for i := 0; i < t.NumField(); i++ {
		if jsonTagName(t.Field(i).Tag.Get("json")) == part {
			return v.Field(i)
		}
	}
	for i := 0; i < t.NumField(); i++ {
		if strings.EqualFold(t.Field(i).Name, part) {
			return v.Field(i)
		}
	}
	return reflect.Value{}
}

// jsonTagName returns the field name portion of a `json` struct tag,
// stripping any options like ",omitempty". An empty or "-" tag yields "".
func jsonTagName(tag string) string {
	if tag == "" || tag == "-" {
		return ""
	}
	if comma := strings.IndexByte(tag, ','); comma >= 0 {
		return tag[:comma]
	}
	return tag
}

// pascalCase converts a DSL identifier to the Go PascalCase field name the
// codegen emits ("first_name" -> "FirstName"). Acronyms are uppercased
// wholesale ("agency_id" -> "AgencyID", "html_body" -> "HTMLBody"); the
// acronym set MUST stay in sync with the emitter's
// `lazuli_codegen_go::emitter::casing::is_acronym` table, otherwise binding
// reflection drifts from the struct fields it resolves against.
func pascalCase(s string) string {
	var b strings.Builder
	b.Grow(len(s))
	for _, word := range splitWords(s) {
		if word == "" {
			continue
		}
		if isAcronym(word) {
			b.WriteString(strings.ToUpper(word))
			continue
		}
		b.WriteString(strings.ToUpper(word[:1]))
		b.WriteString(word[1:])
	}
	return b.String()
}

// recognisedBootEnvs lists every `LAZULI_ENV` value the runtime knows how to
// interpret. Boot warns loudly when the configured env is none of these so a
// typo (`prod`, `prodution`, …) is caught at startup instead of silently
// degrading a security posture (CORS origins, rate limits, dev-session).
var recognisedBootEnvs = map[string]struct{}{
	"local":      {},
	"dev":        {},
	"test":       {},
	"staging":    {},
	"production": {},
}

// warnUnknownBootEnv (BOOT-ENV-UNSET-001) emits a loud startup warning when
// `LAZULI_ENV` is unset or set to a value the runtime does not recognise.
// It never aborts boot (a refusal would be a deploy footgun), but it makes
// the misconfiguration impossible to miss in logs — and critically, the
// dev-session header path stays OFF for any unknown env (see
// `devSessionEnabled`), so an unrecognised env is fail-closed, not fail-open.
func warnUnknownBootEnv() {
	raw := os.Getenv("LAZULI_ENV")
	env := normalizeEnv(raw)
	if env == "" {
		slog.Warn("lazuli: LAZULI_ENV is UNSET — treating as unknown; dev-session header auth is DISABLED, env-scoped CORS/rate-limit fall back to defaults. Set LAZULI_ENV to one of local|dev|test|staging|production.")
		return
	}
	if _, ok := recognisedBootEnvs[env]; !ok {
		slog.Warn("lazuli: LAZULI_ENV has an UNRECOGNISED value — likely a typo; dev-session header auth is DISABLED (fail-closed). Set LAZULI_ENV to one of local|dev|test|staging|production.",
			"lazuli_env", env)
	}
}

// isAcronym mirrors the emitter's closed acronym catalog. Keep in sync with
// `lazuli_codegen_go::emitter::casing::is_acronym`.
func isAcronym(word string) bool {
	switch strings.ToLower(word) {
	case "id", "url", "uri", "api", "html", "json", "sql", "ttl", "uuid":
		return true
	default:
		return false
	}
}

// splitWords splits a mixed-case identifier into words on `_`/`-` separators
// and on case transitions, mirroring the emitter's `split_words`. This is
// what lets "agency_id" -> [agency, id] and "HTMLBody" -> [HTML, Body].
func splitWords(s string) []string {
	var words []string
	var current strings.Builder
	chars := []rune(s)
	flush := func() {
		if current.Len() > 0 {
			words = append(words, current.String())
			current.Reset()
		}
	}
	for i, ch := range chars {
		if ch == '_' || ch == '-' {
			flush()
			continue
		}
		if ch >= 'A' && ch <= 'Z' {
			prevLower := i > 0 && (isLower(chars[i-1]) || isDigit(chars[i-1]))
			nextLower := i+1 < len(chars) && isLower(chars[i+1])
			if current.Len() > 0 && (prevLower || nextLower) {
				if !nextLower {
					// Mid-uppercase run (e.g. "HT" in "HTML"): keep going.
					current.WriteRune(ch)
					continue
				}
				flush()
			}
		}
		current.WriteRune(ch)
	}
	flush()
	return words
}

func isLower(r rune) bool { return r >= 'a' && r <= 'z' }
func isDigit(r rune) bool { return r >= '0' && r <= '9' }

// Boot wires the runtime: opens the DB pool. Call once at process startup
// before the HTTP server begins serving.
func Boot(ctx context.Context, dbURL string) error {
	warnUnknownBootEnv()
	pool, err := connectDB(ctx, dbURL)
	if err != nil {
		return err
	}
	SetDB(pool)
	return nil
}
