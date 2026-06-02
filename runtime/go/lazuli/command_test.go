package lazuli

import (
	"context"
	"encoding/json"
	"errors"
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
)

func TestCommandTransitionMismatchReturnsTypedError(t *testing.T) {
	type input struct {
		ID int64
	}
	type output struct {
		ID int64
	}

	resource := &Resource[output]{Name: "LifecycleThing"}
	cmd := &Command[input, output]{
		Name: "thing.advance",
		Policy: Policy{Atoms: []PolicyAtom{
			{Namespace: "scope", Name: "public"},
		}},
		Effect: Updates(resource,
			Bindings{"id": FromInput("ID")},
			Bindings{"name": FromConst("changed")},
		),
		Transitions: []TransitionAdvance{{From: "A", To: "B"}},
	}

	tx := &commandTransitionTxStub{lifecycleState: "X"}
	previous := runCommandTx
	runCommandTx = func(ctx context.Context, fn func(pgx.Tx) error) error {
		return fn(tx)
	}
	t.Cleanup(func() { runCommandTx = previous })

	_, err := cmd.dispatch(&Ctx{Context: context.Background(), Actor: ActorAnonymous}, json.RawMessage(`{"ID":123}`))
	if err == nil {
		t.Fatal("dispatch error = nil, want lifecycle mismatch")
	}
	if !errors.Is(err, ErrLifecycleStateMismatch) {
		t.Fatalf("errors.Is(err, ErrLifecycleStateMismatch) = false; err = %v", err)
	}
	var mismatch *LifecycleStateMismatchError
	if !errors.As(err, &mismatch) {
		t.Fatalf("error type = %T, want *LifecycleStateMismatchError", err)
	}
	if mismatch.Expected != "A" || mismatch.Actual != "X" {
		t.Fatalf("mismatch = {Expected:%q Actual:%q}, want {A X}", mismatch.Expected, mismatch.Actual)
	}
	if tx.queryRowSQL != `SELECT "lifecycle_state" FROM "lifecycle_thing" WHERE "id" = $1 FOR UPDATE` {
		t.Fatalf("guard SQL = %q", tx.queryRowSQL)
	}
	if len(tx.queryRowArgs) != 1 || tx.queryRowArgs[0] != int64(123) {
		t.Fatalf("guard args = %#v, want [123]", tx.queryRowArgs)
	}
	if tx.queryCalls != 0 || tx.execCalls != 0 {
		t.Fatalf("effect ran after mismatch: Query calls=%d Exec calls=%d", tx.queryCalls, tx.execCalls)
	}
}

// TestMultiSourceTransitionGuardMembership proves GAP-R1: a transition
// declaring `from A, B` (fan-in) passes the pre-transition guard from
// BOTH A and B, and is rejected (409 lifecycle_state_mismatch) from C.
func TestMultiSourceTransitionGuardMembership(t *testing.T) {
	transitions := []TransitionAdvance{
		{From: "A", FromAny: []string{"A", "B"}, To: "C"},
	}
	target := lifecycleTransitionTarget{
		Resource: &resourceErased{Name: "LifecycleThing"},
		IDColumn: "id",
		IDValue:  int64(7),
	}

	for _, allowed := range []string{"A", "B"} {
		tx := &commandTransitionTxStub{lifecycleState: allowed}
		err := lockLifecycleTransition(
			&Ctx{Context: context.Background(), Actor: ActorAnonymous},
			tx, target, transitions,
		)
		if err != nil {
			t.Fatalf("guard from %q = %v, want nil (fan-in member must pass)", allowed, err)
		}
	}

	// Rejected from a non-member state.
	tx := &commandTransitionTxStub{lifecycleState: "C"}
	err := lockLifecycleTransition(
		&Ctx{Context: context.Background(), Actor: ActorAnonymous},
		tx, target, transitions,
	)
	if err == nil {
		t.Fatal("guard from \"C\" = nil, want lifecycle mismatch")
	}
	if !errors.Is(err, ErrLifecycleStateMismatch) {
		t.Fatalf("errors.Is(err, ErrLifecycleStateMismatch) = false; err = %v", err)
	}
	var mismatch *LifecycleStateMismatchError
	if !errors.As(err, &mismatch) {
		t.Fatalf("error type = %T, want *LifecycleStateMismatchError", err)
	}
	if mismatch.Actual != "C" {
		t.Fatalf("mismatch.Actual = %q, want C", mismatch.Actual)
	}
	if got, want := mismatch.expectedStates(), []string{"A", "B"}; !equalStrings(got, want) {
		t.Fatalf("mismatch.expectedStates() = %v, want %v", got, want)
	}
	// 409 envelope carries the full accepted set.
	var wireErr *Error
	if !errors.As(err, &wireErr) {
		t.Fatalf("errors.As(*Error) = false; err = %v", err)
	}
	if wireErr.Status != 409 {
		t.Fatalf("status = %d, want 409", wireErr.Status)
	}
	data, ok := wireErr.Data.(map[string]any)
	if !ok {
		t.Fatalf("envelope Data type = %T, want map[string]any", wireErr.Data)
	}
	if states, ok := data["expected_states"].([]string); !ok || !equalStrings(states, []string{"A", "B"}) {
		t.Fatalf("envelope expected_states = %#v, want [A B]", data["expected_states"])
	}
}

// TestLifecycleGuardUsesCustomStateColumn proves the lifecycle
// transition pre-guard SELECTs the resource's authored discriminator
// column (e.g. `registration_step`) when target.StateColumn is set,
// instead of the legacy hardcoded `lifecycle_state`. Before this fix the
// guard 500'd with PG 42703 for every pilot whose discriminator isn't
// literally named `lifecycle_state` (e.g. pauta's onboarding
// `registration_step`).
func TestLifecycleGuardUsesCustomStateColumn(t *testing.T) {
	transitions := []TransitionAdvance{{From: "profile_setup", To: "agency_setup"}}
	target := lifecycleTransitionTarget{
		Resource:    &resourceErased{Name: "User"},
		IDColumn:    "id",
		IDValue:     int64(11),
		StateColumn: "registration_step",
	}
	// Current state is a member of the from-set → guard passes, and we
	// can inspect the SQL it issued.
	tx := &commandTransitionTxStub{lifecycleState: "profile_setup"}
	if err := lockLifecycleTransition(
		&Ctx{Context: context.Background(), Actor: ActorAnonymous},
		tx, target, transitions,
	); err != nil {
		t.Fatalf("guard = %v, want nil", err)
	}
	want := `SELECT "registration_step" FROM "user" WHERE "id" = $1 FOR UPDATE`
	if tx.queryRowSQL != want {
		t.Fatalf("guard SQL = %q, want %q", tx.queryRowSQL, want)
	}
	// Empty StateColumn still defaults to lifecycle_state (back-compat).
	target.StateColumn = ""
	tx2 := &commandTransitionTxStub{lifecycleState: "profile_setup"}
	_ = lockLifecycleTransition(
		&Ctx{Context: context.Background(), Actor: ActorAnonymous},
		tx2, target, transitions,
	)
	if got, want := tx2.queryRowSQL, `SELECT "lifecycle_state" FROM "user" WHERE "id" = $1 FOR UPDATE`; got != want {
		t.Fatalf("default guard SQL = %q, want %q", got, want)
	}
}

func equalStrings(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

type commandTransitionTxStub struct {
	lifecycleState string
	queryRowSQL    string
	queryRowArgs   []any
	queryCalls     int
	execCalls      int
}

func (tx *commandTransitionTxStub) Begin(context.Context) (pgx.Tx, error) { return tx, nil }
func (tx *commandTransitionTxStub) Commit(context.Context) error          { return nil }
func (tx *commandTransitionTxStub) Rollback(context.Context) error        { return nil }
func (tx *commandTransitionTxStub) CopyFrom(context.Context, pgx.Identifier, []string, pgx.CopyFromSource) (int64, error) {
	return 0, errors.New("unexpected CopyFrom")
}
func (tx *commandTransitionTxStub) SendBatch(context.Context, *pgx.Batch) pgx.BatchResults {
	panic("unexpected SendBatch")
}
func (tx *commandTransitionTxStub) LargeObjects() pgx.LargeObjects { panic("unexpected LargeObjects") }
func (tx *commandTransitionTxStub) Prepare(context.Context, string, string) (*pgconn.StatementDescription, error) {
	return nil, errors.New("unexpected Prepare")
}
func (tx *commandTransitionTxStub) Exec(context.Context, string, ...any) (pgconn.CommandTag, error) {
	tx.execCalls++
	return pgconn.CommandTag{}, errors.New("unexpected Exec")
}
func (tx *commandTransitionTxStub) Query(context.Context, string, ...any) (pgx.Rows, error) {
	tx.queryCalls++
	return nil, errors.New("unexpected Query")
}
func (tx *commandTransitionTxStub) QueryRow(_ context.Context, sql string, args ...any) pgx.Row {
	tx.queryRowSQL = sql
	tx.queryRowArgs = append([]any(nil), args...)
	return commandTransitionRowStub{state: tx.lifecycleState}
}
func (tx *commandTransitionTxStub) Conn() *pgx.Conn { return nil }

type commandTransitionRowStub struct {
	state string
}

func (row commandTransitionRowStub) Scan(dest ...any) error {
	*dest[0].(*string) = row.state
	return nil
}
