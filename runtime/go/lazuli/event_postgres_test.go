package lazuli

import (
	"context"
	"errors"
	"io"
	"reflect"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/pgtype"
	"github.com/jackc/pgx/v5/pgxpool"
)

var (
	_ PostgresEventQuerier = pgx.Tx(nil)
	_ PostgresEventQuerier = (*pgxpool.Pool)(nil)
)

func TestPostgresEventStoreAppendUsesDeterministicSQL(t *testing.T) {
	ctx := context.WithValue(context.Background(), postgresEventTestContextKey{}, "ctx")
	now := time.Date(2026, 5, 12, 12, 30, 0, 0, time.UTC)
	userID := ID(42)
	event := Event{
		Name:       "customer_created",
		Trace:      true,
		Tenant:     &Tenant{OrgID: 7},
		Actor:      ActorUser,
		UserID:     &userID,
		Payload:    map[string]any{"id": ID(100), "meta": map[string]any{"tier": "gold"}, "tags": []any{"new", ID(2)}},
		OccurredAt: now,
	}
	db := &fakePostgresEventDB{
		row: fakePostgresEventRow{values: fakePostgresEventValues(42, event)},
	}

	stored, err := NewPostgresEventStore(db).Append(ctx, event)
	if err != nil {
		t.Fatalf("Append() error = %v", err)
	}

	if db.queryRowCtx != ctx {
		t.Fatal("Append() used a different context")
	}
	if db.queryRowSQL != postgresEventInsertSQL {
		t.Fatalf("Append() SQL = %q, want %q", db.queryRowSQL, postgresEventInsertSQL)
	}
	if len(db.queryRowArgs) != 7 {
		t.Fatalf("Append() args len = %d, want 7", len(db.queryRowArgs))
	}
	if db.queryRowArgs[0] != "customer_created" || db.queryRowArgs[1] != true || db.queryRowArgs[2] != ID(7) ||
		db.queryRowArgs[3] != "user" || db.queryRowArgs[4] != ID(42) || db.queryRowArgs[6] != now {
		t.Fatalf("Append() args = %#v, want event fields", db.queryRowArgs)
	}
	if got := db.queryRowArgs[5].(string); got != `{"id":100,"meta":{"tier":"gold"},"tags":["new",2]}` {
		t.Fatalf("Append() payload JSON = %s", got)
	}
	if stored.Sequence != 42 {
		t.Fatalf("Append() Sequence = %d, want 42", stored.Sequence)
	}
	assertStoredEventFields(t, stored, event)
}

func TestPostgresEventStoreListBuildsFiltersAndScansRows(t *testing.T) {
	now := time.Date(2026, 5, 12, 13, 0, 0, 0, time.UTC)
	events := []Event{
		{Name: "customer_created", Tenant: &Tenant{OrgID: 3}, Actor: ActorSystem, Payload: map[string]any{"id": ID(1)}, OccurredAt: now},
		{Name: "customer_created", Tenant: &Tenant{OrgID: 3}, Actor: ActorSystem, Payload: map[string]any{"id": ID(2)}, OccurredAt: now.Add(time.Second)},
	}
	rows := &fakePostgresEventRows{
		values: [][]any{
			fakePostgresEventValues(10, events[0]),
			fakePostgresEventValues(11, events[1]),
		},
	}
	db := &fakePostgresEventDB{rows: rows}

	got, err := NewPostgresEventStore(db).List(context.Background(), EventListFilter{
		Name:          "customer_created",
		Tenant:        &Tenant{OrgID: 3},
		SinceSequence: 9,
	})
	if err != nil {
		t.Fatalf("List() error = %v", err)
	}

	wantSQL := "SELECT sequence, name, trace, tenant_org_id, actor, user_id, payload, occurred_at FROM lazuli_events WHERE sequence > $1 AND name = $2 AND tenant_org_id = $3 ORDER BY sequence ASC"
	if db.querySQL != wantSQL {
		t.Fatalf("List() SQL = %q, want %q", db.querySQL, wantSQL)
	}
	if !reflect.DeepEqual(db.queryArgs, []any{int64(9), "customer_created", ID(3)}) {
		t.Fatalf("List() args = %#v", db.queryArgs)
	}
	if !rows.closed {
		t.Fatal("List() did not close rows")
	}
	assertSequences(t, got, []uint64{10, 11})
	assertStoredEventFields(t, got[0], events[0])
	assertStoredEventFields(t, got[1], events[1])
}

func TestPostgresEventStoreReplayBuildsFiltersAndStopsOnYieldError(t *testing.T) {
	since := time.Date(2026, 5, 12, 14, 0, 0, 0, time.UTC)
	until := since.Add(time.Hour)
	events := []Event{
		{Name: "customer_created", Tenant: &Tenant{OrgID: 5}, Actor: ActorSystem, Payload: map[string]any{"id": ID(1)}, OccurredAt: since},
		{Name: "customer_updated", Tenant: &Tenant{OrgID: 5}, Actor: ActorSystem, Payload: map[string]any{"id": ID(1)}, OccurredAt: since.Add(time.Minute)},
	}
	rows := &fakePostgresEventRows{
		values: [][]any{
			fakePostgresEventValues(1, events[0]),
			fakePostgresEventValues(2, events[1]),
		},
	}
	db := &fakePostgresEventDB{rows: rows}
	wantErr := errors.New("projection failed")

	var got []Event
	err := NewPostgresEventStore(db).ReplayEvents(context.Background(), EventReplayFilter{
		Names:  []string{"customer_created", "customer_updated"},
		Tenant: &Tenant{OrgID: 5},
		Since:  since,
		Until:  until,
	}, func(event Event) error {
		got = append(got, event)
		if len(got) == 2 {
			return wantErr
		}
		return nil
	})
	if !errors.Is(err, wantErr) {
		t.Fatalf("ReplayEvents() error = %v, want %v", err, wantErr)
	}

	wantSQL := "SELECT sequence, name, trace, tenant_org_id, actor, user_id, payload, occurred_at FROM lazuli_events WHERE tenant_org_id = $1 AND name = ANY($2::text[]) AND occurred_at >= $3 AND occurred_at < $4 ORDER BY sequence ASC"
	if db.querySQL != wantSQL {
		t.Fatalf("ReplayEvents() SQL = %q, want %q", db.querySQL, wantSQL)
	}
	wantArgs := []any{ID(5), []string{"customer_created", "customer_updated"}, since, until}
	if !reflect.DeepEqual(db.queryArgs, wantArgs) {
		t.Fatalf("ReplayEvents() args = %#v, want %#v", db.queryArgs, wantArgs)
	}
	if !rows.closed {
		t.Fatal("ReplayEvents() did not close rows")
	}
	if len(got) != 2 {
		t.Fatalf("ReplayEvents() yielded %d events, want 2", len(got))
	}
	assertEventFields(t, got[0], events[0])
	assertEventFields(t, got[1], events[1])
}

func TestPostgresEventStoreReturnsContextAndNilDBErrors(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	store := NewPostgresEventStore(&fakePostgresEventDB{})

	if _, err := store.Append(ctx, Event{Name: "customer_created"}); !errors.Is(err, context.Canceled) {
		t.Fatalf("Append() canceled error = %v, want context.Canceled", err)
	}
	if _, err := store.List(ctx, EventListFilter{}); !errors.Is(err, context.Canceled) {
		t.Fatalf("List() canceled error = %v, want context.Canceled", err)
	}
	if err := store.ReplayEvents(ctx, EventReplayFilter{}, func(Event) error { return nil }); !errors.Is(err, context.Canceled) {
		t.Fatalf("ReplayEvents() canceled error = %v, want context.Canceled", err)
	}

	nilStore := NewPostgresEventStore(nil)
	if _, err := nilStore.Append(context.Background(), Event{Name: "customer_created"}); !errors.Is(err, ErrNilPostgresEventDB) {
		t.Fatalf("Append() nil db error = %v, want ErrNilPostgresEventDB", err)
	}
	if _, err := nilStore.List(context.Background(), EventListFilter{}); !errors.Is(err, ErrNilPostgresEventDB) {
		t.Fatalf("List() nil db error = %v, want ErrNilPostgresEventDB", err)
	}
	if err := nilStore.ReplayEvents(context.Background(), EventReplayFilter{}, func(Event) error { return nil }); !errors.Is(err, ErrNilPostgresEventDB) {
		t.Fatalf("ReplayEvents() nil db error = %v, want ErrNilPostgresEventDB", err)
	}
}

func TestPostgresEventStoreListSkipsQueryWhenSequenceExceedsPostgresRange(t *testing.T) {
	db := &fakePostgresEventDB{}
	got, err := NewPostgresEventStore(db).List(context.Background(), EventListFilter{
		SinceSequence: postgresEventMaxSequence + 1,
	})
	if err != nil {
		t.Fatalf("List() error = %v", err)
	}
	if len(got) != 0 {
		t.Fatalf("List() len = %d, want 0", len(got))
	}
	if db.querySQL != "" {
		t.Fatalf("List() queried SQL %q, want no query", db.querySQL)
	}
}

type postgresEventTestContextKey struct{}

type fakePostgresEventDB struct {
	queryRowCtx  context.Context
	queryRowSQL  string
	queryRowArgs []any
	row          fakePostgresEventRow

	queryCtx  context.Context
	querySQL  string
	queryArgs []any
	rows      *fakePostgresEventRows
	queryErr  error
}

func (db *fakePostgresEventDB) QueryRow(ctx context.Context, sql string, args ...any) pgx.Row {
	db.queryRowCtx = ctx
	db.queryRowSQL = sql
	db.queryRowArgs = append([]any(nil), args...)
	return db.row
}

func (db *fakePostgresEventDB) Query(ctx context.Context, sql string, args ...any) (pgx.Rows, error) {
	db.queryCtx = ctx
	db.querySQL = sql
	db.queryArgs = append([]any(nil), args...)
	if db.queryErr != nil {
		return nil, db.queryErr
	}
	if db.rows == nil {
		db.rows = &fakePostgresEventRows{}
	}
	return db.rows, nil
}

type fakePostgresEventRow struct {
	values []any
	err    error
}

func (row fakePostgresEventRow) Scan(dest ...any) error {
	if row.err != nil {
		return row.err
	}
	return scanFakePostgresEventValues(dest, row.values)
}

type fakePostgresEventRows struct {
	values [][]any
	index  int
	closed bool
	err    error
}

func (rows *fakePostgresEventRows) Close() {
	rows.closed = true
}

func (rows *fakePostgresEventRows) Err() error {
	return rows.err
}

func (rows *fakePostgresEventRows) CommandTag() pgconn.CommandTag {
	return pgconn.CommandTag{}
}

func (rows *fakePostgresEventRows) FieldDescriptions() []pgconn.FieldDescription {
	return nil
}

func (rows *fakePostgresEventRows) Next() bool {
	if rows.index >= len(rows.values) {
		return false
	}
	rows.index++
	return true
}

func (rows *fakePostgresEventRows) Scan(dest ...any) error {
	if rows.index == 0 || rows.index > len(rows.values) {
		return io.ErrUnexpectedEOF
	}
	return scanFakePostgresEventValues(dest, rows.values[rows.index-1])
}

func (rows *fakePostgresEventRows) Values() ([]any, error) {
	if rows.index == 0 || rows.index > len(rows.values) {
		return nil, io.ErrUnexpectedEOF
	}
	return append([]any(nil), rows.values[rows.index-1]...), nil
}

func (rows *fakePostgresEventRows) RawValues() [][]byte {
	return nil
}

func (rows *fakePostgresEventRows) Conn() *pgx.Conn {
	return nil
}

func fakePostgresEventValues(sequence int64, event Event) []any {
	var tenantOrgID any
	if event.Tenant != nil {
		tenantOrgID = event.Tenant.OrgID
	}
	var userID any
	if event.UserID != nil {
		userID = *event.UserID
	}
	payload, err := marshalPostgresEventPayload(event.Payload)
	if err != nil {
		panic(err)
	}
	return []any{
		sequence,
		event.Name,
		event.Trace,
		tenantOrgID,
		string(event.Actor),
		userID,
		fakePostgresEventPayloadValue(payload),
		event.OccurredAt,
	}
}

func fakePostgresEventPayloadValue(payload any) any {
	if payload == nil {
		return nil
	}
	return []byte(payload.(string))
}

func scanFakePostgresEventValues(dest []any, values []any) error {
	if len(dest) != len(values) {
		return errors.New("fake postgres event scan destination count mismatch")
	}
	for i := range dest {
		if err := assignFakePostgresEventValue(dest[i], values[i]); err != nil {
			return err
		}
	}
	return nil
}

func assignFakePostgresEventValue(dest any, value any) error {
	switch d := dest.(type) {
	case *int64:
		*d = value.(int64)
	case *string:
		*d = value.(string)
	case *bool:
		*d = value.(bool)
	case *pgtype.Int8:
		if value == nil {
			*d = pgtype.Int8{}
			return nil
		}
		*d = pgtype.Int8{Int64: int64(value.(ID)), Valid: true}
	case *[]byte:
		if value == nil {
			*d = nil
			return nil
		}
		switch v := value.(type) {
		case []byte:
			*d = append([]byte(nil), v...)
		case string:
			*d = []byte(v)
		default:
			return errors.New("fake postgres event unsupported payload value")
		}
	case *time.Time:
		*d = value.(time.Time)
	default:
		return errors.New("fake postgres event unsupported scan destination")
	}
	return nil
}

func assertEventFields(t *testing.T, got Event, want Event) {
	t.Helper()

	assertStoredEventFields(t, StoredEvent{Event: got}, want)
}
