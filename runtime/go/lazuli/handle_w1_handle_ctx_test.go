package lazuli

import (
	"context"
	"errors"
	"strings"
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
)

// advanceCaptureTxStub captures the advance UPDATE's SQL + args. The shared
// commandTransitionTxStub deliberately errors on Exec (its tests assert the
// effect never runs); the advance-scope test needs a successful Exec capture.
type advanceCaptureTxStub struct {
	execSQL  string
	execArgs []any
}

func (tx *advanceCaptureTxStub) Begin(context.Context) (pgx.Tx, error) { return tx, nil }
func (tx *advanceCaptureTxStub) Commit(context.Context) error          { return nil }
func (tx *advanceCaptureTxStub) Rollback(context.Context) error        { return nil }
func (tx *advanceCaptureTxStub) CopyFrom(context.Context, pgx.Identifier, []string, pgx.CopyFromSource) (int64, error) {
	return 0, errors.New("unexpected CopyFrom")
}
func (tx *advanceCaptureTxStub) SendBatch(context.Context, *pgx.Batch) pgx.BatchResults {
	panic("unexpected SendBatch")
}
func (tx *advanceCaptureTxStub) LargeObjects() pgx.LargeObjects { panic("unexpected LargeObjects") }
func (tx *advanceCaptureTxStub) Prepare(context.Context, string, string) (*pgconn.StatementDescription, error) {
	return nil, errors.New("unexpected Prepare")
}
func (tx *advanceCaptureTxStub) Exec(_ context.Context, sql string, args ...any) (pgconn.CommandTag, error) {
	tx.execSQL = sql
	tx.execArgs = append([]any(nil), args...)
	return pgconn.CommandTag{}, nil
}
func (tx *advanceCaptureTxStub) Query(context.Context, string, ...any) (pgx.Rows, error) {
	return nil, errors.New("unexpected Query")
}
func (tx *advanceCaptureTxStub) QueryRow(context.Context, string, ...any) pgx.Row {
	panic("unexpected QueryRow")
}
func (tx *advanceCaptureTxStub) Conn() *pgx.Conn { return nil }

// --- BUG 1 (W4-4) — readCtx resolves actor.tenant_id ----------------------

// TestReadCtxActorTenantIDResolves proves the W4-4 fix: `ctx.actor.tenant_id`
// (lowered to FromCtx("actor.tenant_id")) is recognised by readCtx and
// resolves to the actor's tenant/org identifier, instead of 500'ing with
// "unknown ctx path: actor.tenant_id". Pilots (pauta `user_by_id`,
// `users_in_tenant`) bind their `tenant_id` filter to this path.
func TestReadCtxActorTenantIDResolves(t *testing.T) {
	ctx := &Ctx{User: &User{ID: 42, OrgID: 7}, Tenant: &Tenant{OrgID: 7}}
	got, err := readCtx(ctx, "actor.tenant_id")
	if err != nil {
		t.Fatalf("readCtx(actor.tenant_id) returned error: %v", err)
	}
	if got != ID(7) {
		t.Fatalf("readCtx(actor.tenant_id) = %v, want 7 (actor tenant id)", got)
	}

	// Parity with actor.org_id — both resolve from the same source.
	org, err := readCtx(ctx, "actor.org_id")
	if err != nil {
		t.Fatalf("readCtx(actor.org_id) returned error: %v", err)
	}
	if org != got {
		t.Fatalf("actor.tenant_id (%v) and actor.org_id (%v) must resolve identically", got, org)
	}
}

// TestReadCtxActorTenantIDFallsBackToTenant proves the path still resolves
// when only the Tenant scope (no User) is attached.
func TestReadCtxActorTenantIDFallsBackToTenant(t *testing.T) {
	ctx := &Ctx{Tenant: &Tenant{OrgID: 99}}
	got, err := readCtx(ctx, "actor.tenant_id")
	if err != nil {
		t.Fatalf("readCtx(actor.tenant_id) returned error: %v", err)
	}
	if got != ID(99) {
		t.Fatalf("readCtx(actor.tenant_id) = %v, want 99 (tenant scope fallback)", got)
	}
}

// TestReadCtxActorTenantIDNoContextErrors proves it fails closed (not a 500
// "unknown ctx path") when there is no actor/tenant at all.
func TestReadCtxActorTenantIDNoContextErrors(t *testing.T) {
	_, err := readCtx(&Ctx{}, "actor.tenant_id")
	if err == nil {
		t.Fatal("readCtx(actor.tenant_id) with no actor/tenant should error")
	}
	le, ok := err.(*Error)
	if !ok {
		t.Fatalf("error type = %T, want *Error", err)
	}
	if strings.HasPrefix(le.Message, "unknown ctx path") {
		t.Fatalf("actor.tenant_id must be a RECOGNISED path; got %q", le.Message)
	}
	if le.Status != 400 {
		t.Fatalf("status = %d, want 400 (tenant mismatch), msg=%q", le.Status, le.Message)
	}
}

// --- BUG 2 (W4-6) — struct-tag required validation ------------------------

type w46RequiredInput struct {
	LegalName string  `json:"legal_name" validate:"required"`
	TradeName string  `json:"trade_name" validate:"required"`
	Note      *string `json:"note,omitempty"`
}

type w46AllOptionalInput struct {
	Note  *string `json:"note,omitempty"`
	Count *int    `json:"count,omitempty"`
}

type w46NoFieldsInput struct{}

// TestValidateInputTagsRejectsMissingRequired proves W4-6: a command input
// with empty required fields is rejected with a 400 validation_failed
// envelope listing the offending fields (by wire name).
func TestValidateInputTagsRejectsMissingRequired(t *testing.T) {
	err := validateInputTags(w46RequiredInput{})
	if err == nil {
		t.Fatal("validateInputTags on empty required input = nil, want validation_failed")
	}
	le, ok := err.(*Error)
	if !ok {
		t.Fatalf("error type = %T, want *Error", err)
	}
	if le.Status != 400 || le.Code != CodeValidationFailed {
		t.Fatalf("envelope = {Status:%d Code:%q}, want {400 validation_failed}", le.Status, le.Code)
	}
	data, ok := le.Data.(map[string]any)
	if !ok {
		t.Fatalf("Data type = %T, want map[string]any", le.Data)
	}
	fields, ok := data["fields"].([]string)
	if !ok {
		t.Fatalf("Data[fields] type = %T, want []string", data["fields"])
	}
	wantSet := map[string]bool{"legal_name": true, "trade_name": true}
	if len(fields) != 2 {
		t.Fatalf("missing fields = %v, want legal_name+trade_name", fields)
	}
	for _, f := range fields {
		if !wantSet[f] {
			t.Fatalf("unexpected field in missing set: %q (got %v)", f, fields)
		}
	}
}

// TestValidateInputTagsPassesWhenRequiredPresent proves the happy path: a
// fully-populated required input passes (and the optional field staying nil
// does not trip the check).
func TestValidateInputTagsPassesWhenRequiredPresent(t *testing.T) {
	if err := validateInputTags(w46RequiredInput{LegalName: "Acme", TradeName: "Acme Co"}); err != nil {
		t.Fatalf("validateInputTags on populated required input = %v, want nil", err)
	}
}

// TestValidateInputTagsAllOptionalPasses proves commands with all-optional
// inputs are unaffected — an empty body is valid.
func TestValidateInputTagsAllOptionalPasses(t *testing.T) {
	if err := validateInputTags(w46AllOptionalInput{}); err != nil {
		t.Fatalf("validateInputTags on all-optional input = %v, want nil", err)
	}
}

// TestValidateInputTagsNoFieldsPasses proves no-input commands pass.
func TestValidateInputTagsNoFieldsPasses(t *testing.T) {
	if err := validateInputTags(w46NoFieldsInput{}); err != nil {
		t.Fatalf("validateInputTags on no-field input = %v, want nil", err)
	}
}

// TestValidateInputTagsPartialMissing proves only the empty required field is
// flagged when some required fields are present.
func TestValidateInputTagsPartialMissing(t *testing.T) {
	err := validateInputTags(w46RequiredInput{LegalName: "Acme"})
	le, ok := err.(*Error)
	if !ok {
		t.Fatalf("error type = %T, want *Error", err)
	}
	data := le.Data.(map[string]any)
	fields := data["fields"].([]string)
	if len(fields) != 1 || fields[0] != "trade_name" {
		t.Fatalf("missing fields = %v, want [trade_name]", fields)
	}
}

// --- BUG 3 (W1-1) — lifecycle transition tenant scoping -------------------

// TestLifecycleLockScopesTenancyOrg proves the W1-1 fix on the pre-guard
// SELECT: a TenancyOrg resource's lock SELECT now carries the org_id scope
// predicate (bound to the active tenant), so a cross-tenant id finds no row.
func TestLifecycleLockScopesTenancyOrg(t *testing.T) {
	transitions := []TransitionAdvance{{From: "profile_setup", To: "agency_setup"}}
	target := lifecycleTransitionTarget{
		Resource:    &resourceErased{Name: "Onboarding", Tenancy: TenancyOrg},
		IDColumn:    "id",
		IDValue:     int64(11),
		StateColumn: "registration_step",
	}
	ctx := &Ctx{Context: context.Background(), Actor: ActorUser,
		User: &User{ID: 1, OrgID: 7}, Tenant: &Tenant{OrgID: 7}}
	tx := &commandTransitionTxStub{lifecycleState: "profile_setup"}
	if err := lockLifecycleTransition(ctx, tx, target, transitions); err != nil {
		t.Fatalf("guard = %v, want nil", err)
	}
	if !strings.Contains(tx.queryRowSQL, `org_id = $2`) {
		t.Fatalf("lock SELECT must carry tenant scope predicate; got SQL: %q", tx.queryRowSQL)
	}
	if !strings.Contains(tx.queryRowSQL, `"id" = $1`) {
		t.Fatalf("lock SELECT must still key on id; got SQL: %q", tx.queryRowSQL)
	}
	// id at $1, org_id at $2.
	if len(tx.queryRowArgs) != 2 || tx.queryRowArgs[0] != int64(11) || tx.queryRowArgs[1] != ID(7) {
		t.Fatalf("lock args = %#v, want [11 7]", tx.queryRowArgs)
	}
}

// TestLifecycleLockTenancyNoneStaysUnscoped proves back-compat: a TenancyNone
// resource (no org_id column) gets no scope predicate, so existing pilots are
// unaffected and the SQL shape matches the pre-fix form.
func TestLifecycleLockTenancyNoneStaysUnscoped(t *testing.T) {
	transitions := []TransitionAdvance{{From: "A", To: "B"}}
	target := lifecycleTransitionTarget{
		Resource: &resourceErased{Name: "Thing", Tenancy: TenancyNone},
		IDColumn: "id",
		IDValue:  int64(5),
	}
	ctx := &Ctx{Context: context.Background(), Actor: ActorAnonymous}
	tx := &commandTransitionTxStub{lifecycleState: "A"}
	if err := lockLifecycleTransition(ctx, tx, target, transitions); err != nil {
		t.Fatalf("guard = %v, want nil", err)
	}
	want := `SELECT "lifecycle_state" FROM "thing" WHERE "id" = $1 FOR UPDATE`
	if tx.queryRowSQL != want {
		t.Fatalf("TenancyNone lock SQL = %q, want %q", tx.queryRowSQL, want)
	}
}

// TestLifecycleLockTenancyOrgWithoutTenantFailsClosed proves the lock fails
// closed (tenant required) for a TenancyOrg resource with no tenant on ctx,
// rather than silently dropping the scope predicate.
func TestLifecycleLockTenancyOrgWithoutTenantFailsClosed(t *testing.T) {
	transitions := []TransitionAdvance{{From: "A", To: "B"}}
	target := lifecycleTransitionTarget{
		Resource: &resourceErased{Name: "Onboarding", Tenancy: TenancyOrg},
		IDColumn: "id",
		IDValue:  int64(11),
	}
	ctx := &Ctx{Context: context.Background(), Actor: ActorAnonymous}
	tx := &commandTransitionTxStub{lifecycleState: "A"}
	err := lockLifecycleTransition(ctx, tx, target, transitions)
	if err == nil {
		t.Fatal("lock on TenancyOrg resource without tenant = nil, want tenant-required error")
	}
	if errors.Is(err, ErrLifecycleStateMismatch) {
		t.Fatalf("expected tenant-required error, got lifecycle mismatch: %v", err)
	}
}

// TestLifecycleAdvanceScopesTenancyOrg proves the W1-1 fix on the advance
// UPDATE: it carries the org_id scope predicate, so the UPDATE WHERE cannot
// mutate another tenant's row (cross-tenant IDOR). State at $1, id at $2,
// org_id at $3.
func TestLifecycleAdvanceScopesTenancyOrg(t *testing.T) {
	transitions := []TransitionAdvance{{From: "profile_setup", To: "agency_setup"}}
	target := lifecycleTransitionTarget{
		Resource:    &resourceErased{Name: "Onboarding", Tenancy: TenancyOrg},
		IDColumn:    "id",
		IDValue:     int64(11),
		StateColumn: "registration_step",
	}
	ctx := &Ctx{Context: context.Background(), Actor: ActorUser,
		User: &User{ID: 1, OrgID: 7}, Tenant: &Tenant{OrgID: 7}}
	tx := &advanceCaptureTxStub{}
	if err := advanceLifecycleTransition(ctx, tx, target, transitions); err != nil {
		t.Fatalf("advance = %v, want nil", err)
	}
	if !strings.Contains(tx.execSQL, `org_id = $3`) {
		t.Fatalf("advance UPDATE must carry tenant scope predicate; got SQL: %q", tx.execSQL)
	}
	if !strings.Contains(tx.execSQL, `"id" = $2`) {
		t.Fatalf("advance UPDATE must still key on id; got SQL: %q", tx.execSQL)
	}
	if !strings.Contains(tx.execSQL, `SET "registration_step" = $1`) {
		t.Fatalf("advance UPDATE must SET the discriminator at $1; got SQL: %q", tx.execSQL)
	}
	if len(tx.execArgs) != 3 ||
		tx.execArgs[0] != "agency_setup" ||
		tx.execArgs[1] != int64(11) ||
		tx.execArgs[2] != ID(7) {
		t.Fatalf("advance args = %#v, want [agency_setup 11 7]", tx.execArgs)
	}
}
