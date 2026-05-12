package lazuli

import (
	"context"
	"errors"
	"reflect"
	"sync"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
)

var (
	_ PreparedStatementPreparer    = (*pgx.Conn)(nil)
	_ PreparedStatementPreparer    = pgx.Tx(nil)
	_ PreparedStatementDeallocator = (*pgx.Conn)(nil)
)

func TestPreparedStatementCachePreparesOnceAndReusesDescription(t *testing.T) {
	ctx := context.Background()
	cache := NewPreparedStatementCache()
	db := &preparedStatementDBFake{}

	first, err := cache.Prepare(ctx, db, "find_customer", "select * from customers where id = $1")
	if err != nil {
		t.Fatalf("Prepare returned error: %v", err)
	}
	second, err := cache.Prepare(ctx, db, "find_customer", "select * from customers where id = $1")
	if err != nil {
		t.Fatalf("second Prepare returned error: %v", err)
	}

	if first != second {
		t.Fatal("Prepare did not return cached statement description")
	}
	assertPreparedStatementPrepareCalls(t, db.prepareCallsSnapshot(), []preparedStatementPrepareCall{
		{name: "find_customer", sql: "select * from customers where id = $1"},
	})
}

func TestPreparedStatementCacheRejectsNameSQLMismatch(t *testing.T) {
	ctx := context.Background()
	cache := NewPreparedStatementCache()
	db := &preparedStatementDBFake{}

	if _, err := cache.Prepare(ctx, db, "find_customer", "select * from customers where id = $1"); err != nil {
		t.Fatalf("Prepare returned error: %v", err)
	}
	_, err := cache.Prepare(ctx, db, "find_customer", "select * from customers where email = $1")

	if err == nil {
		t.Fatal("Prepare returned nil error for mismatched SQL")
	}
	assertPreparedStatementPrepareCalls(t, db.prepareCallsSnapshot(), []preparedStatementPrepareCall{
		{name: "find_customer", sql: "select * from customers where id = $1"},
	})
}

func TestPreparedStatementCacheRetriesAfterPrepareError(t *testing.T) {
	ctx := context.Background()
	wantErr := errors.New("prepare failed")
	cache := NewPreparedStatementCache()
	db := &preparedStatementDBFake{prepareErrs: []error{wantErr, nil}}

	if _, err := cache.Prepare(ctx, db, "find_customer", "select 1"); !errors.Is(err, wantErr) {
		t.Fatalf("Prepare error = %v, want %v", err, wantErr)
	}
	if _, err := cache.Prepare(ctx, db, "find_customer", "select 1"); err != nil {
		t.Fatalf("second Prepare returned error: %v", err)
	}

	assertPreparedStatementPrepareCalls(t, db.prepareCallsSnapshot(), []preparedStatementPrepareCall{
		{name: "find_customer", sql: "select 1"},
		{name: "find_customer", sql: "select 1"},
	})
}

func TestPreparedStatementCacheCloseDeallocatesPreparedStatements(t *testing.T) {
	ctx := context.Background()
	cache := NewPreparedStatementCache()
	db := &preparedStatementDBFake{}

	if _, err := cache.Prepare(ctx, db, "b_stmt", "select 2"); err != nil {
		t.Fatalf("Prepare b_stmt returned error: %v", err)
	}
	if _, err := cache.Prepare(ctx, db, "a_stmt", "select 1"); err != nil {
		t.Fatalf("Prepare a_stmt returned error: %v", err)
	}

	if err := cache.Close(ctx, db); err != nil {
		t.Fatalf("Close returned error: %v", err)
	}

	assertPreparedStatementDeallocateCalls(t, db.deallocateCallsSnapshot(), []string{"a_stmt", "b_stmt"})

	if _, err := cache.Prepare(ctx, db, "a_stmt", "select 1"); err != nil {
		t.Fatalf("Prepare after Close returned error: %v", err)
	}
	assertPreparedStatementPrepareCalls(t, db.prepareCallsSnapshot(), []preparedStatementPrepareCall{
		{name: "b_stmt", sql: "select 2"},
		{name: "a_stmt", sql: "select 1"},
		{name: "a_stmt", sql: "select 1"},
	})
}

func TestPreparedStatementCacheCloseClearsLocalCacheWhenDeallocateUnsupported(t *testing.T) {
	ctx := context.Background()
	cache := NewPreparedStatementCache()
	db := &preparedStatementDBFake{}

	if _, err := cache.Prepare(ctx, db, "find_customer", "select 1"); err != nil {
		t.Fatalf("Prepare returned error: %v", err)
	}
	if err := cache.Close(ctx, struct{}{}); err != nil {
		t.Fatalf("Close returned error: %v", err)
	}
	if _, err := cache.Prepare(ctx, db, "find_customer", "select 1"); err != nil {
		t.Fatalf("Prepare after unsupported Close returned error: %v", err)
	}

	assertPreparedStatementPrepareCalls(t, db.prepareCallsSnapshot(), []preparedStatementPrepareCall{
		{name: "find_customer", sql: "select 1"},
		{name: "find_customer", sql: "select 1"},
	})
}

func TestPreparedStatementCacheCloseKeepsFailedDeallocations(t *testing.T) {
	ctx := context.Background()
	wantErr := errors.New("deallocate failed")
	cache := NewPreparedStatementCache()
	db := &preparedStatementDBFake{
		deallocateErrByName: map[string]error{"a_stmt": wantErr},
	}

	if _, err := cache.Prepare(ctx, db, "a_stmt", "select 1"); err != nil {
		t.Fatalf("Prepare a_stmt returned error: %v", err)
	}
	if _, err := cache.Prepare(ctx, db, "b_stmt", "select 2"); err != nil {
		t.Fatalf("Prepare b_stmt returned error: %v", err)
	}

	if err := cache.Close(ctx, db); !errors.Is(err, wantErr) {
		t.Fatalf("Close error = %v, want %v", err, wantErr)
	}
	assertPreparedStatementDeallocateCalls(t, db.deallocateCallsSnapshot(), []string{"a_stmt", "b_stmt"})

	db.clearDeallocateErrors()
	if err := cache.Close(ctx, db); err != nil {
		t.Fatalf("second Close returned error: %v", err)
	}
	assertPreparedStatementDeallocateCalls(t, db.deallocateCallsSnapshot(), []string{"a_stmt", "b_stmt", "a_stmt"})
}

func TestPreparedStatementCachePrepareConcurrentSameStatementOnce(t *testing.T) {
	cache := NewPreparedStatementCache()
	db := &preparedStatementDBFake{prepareDelay: 10 * time.Millisecond}

	const callers = 32
	start := make(chan struct{})
	errCh := make(chan error, callers)
	descCh := make(chan *pgconn.StatementDescription, callers)

	var wg sync.WaitGroup
	for i := 0; i < callers; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			<-start

			desc, err := cache.Prepare(context.Background(), db, "find_customer", "select 1")
			if err != nil {
				errCh <- err
				return
			}
			descCh <- desc
		}()
	}

	close(start)
	wg.Wait()
	close(errCh)
	close(descCh)

	for err := range errCh {
		t.Fatalf("Prepare returned error: %v", err)
	}

	var first *pgconn.StatementDescription
	for desc := range descCh {
		if desc == nil {
			t.Fatal("Prepare returned nil description")
		}
		if first == nil {
			first = desc
			continue
		}
		if desc != first {
			t.Fatal("concurrent Prepare did not return the cached description")
		}
	}

	assertPreparedStatementPrepareCalls(t, db.prepareCallsSnapshot(), []preparedStatementPrepareCall{
		{name: "find_customer", sql: "select 1"},
	})
}

func TestPreparedStatementCacheNilInputs(t *testing.T) {
	ctx := context.Background()
	cache := NewPreparedStatementCache()

	if _, err := (*PreparedStatementCache)(nil).Prepare(ctx, &preparedStatementDBFake{}, "stmt", "select 1"); !errors.Is(err, errNilPreparedStatementCache) {
		t.Fatalf("nil cache Prepare error = %v, want %v", err, errNilPreparedStatementCache)
	}
	if err := (*PreparedStatementCache)(nil).Close(ctx, &preparedStatementDBFake{}); !errors.Is(err, errNilPreparedStatementCache) {
		t.Fatalf("nil cache Close error = %v, want %v", err, errNilPreparedStatementCache)
	}
	if _, err := cache.Prepare(ctx, nil, "stmt", "select 1"); !errors.Is(err, errNilPreparedStatementPreparer) {
		t.Fatalf("nil preparer error = %v, want %v", err, errNilPreparedStatementPreparer)
	}

	var typedNil *preparedStatementDBFake
	if _, err := cache.Prepare(ctx, typedNil, "stmt", "select 1"); !errors.Is(err, errNilPreparedStatementPreparer) {
		t.Fatalf("typed nil preparer error = %v, want %v", err, errNilPreparedStatementPreparer)
	}
}

type preparedStatementPrepareCall struct {
	name string
	sql  string
}

type preparedStatementDBFake struct {
	mu                  sync.Mutex
	prepareCalls        []preparedStatementPrepareCall
	prepareErrs         []error
	prepareDelay        time.Duration
	deallocateCalls     []string
	deallocateErrByName map[string]error
}

func (db *preparedStatementDBFake) Prepare(ctx context.Context, name, sql string) (*pgconn.StatementDescription, error) {
	if db.prepareDelay > 0 {
		timer := time.NewTimer(db.prepareDelay)
		defer timer.Stop()

		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-timer.C:
		}
	}

	db.mu.Lock()
	defer db.mu.Unlock()

	db.prepareCalls = append(db.prepareCalls, preparedStatementPrepareCall{name: name, sql: sql})
	if len(db.prepareErrs) > 0 {
		err := db.prepareErrs[0]
		db.prepareErrs = db.prepareErrs[1:]
		if err != nil {
			return nil, err
		}
	}

	return &pgconn.StatementDescription{Name: name, SQL: sql}, nil
}

func (db *preparedStatementDBFake) Deallocate(ctx context.Context, name string) error {
	if err := ctx.Err(); err != nil {
		return err
	}

	db.mu.Lock()
	defer db.mu.Unlock()

	db.deallocateCalls = append(db.deallocateCalls, name)
	return db.deallocateErrByName[name]
}

func (db *preparedStatementDBFake) prepareCallsSnapshot() []preparedStatementPrepareCall {
	db.mu.Lock()
	defer db.mu.Unlock()

	return append([]preparedStatementPrepareCall(nil), db.prepareCalls...)
}

func (db *preparedStatementDBFake) deallocateCallsSnapshot() []string {
	db.mu.Lock()
	defer db.mu.Unlock()

	return append([]string(nil), db.deallocateCalls...)
}

func (db *preparedStatementDBFake) clearDeallocateErrors() {
	db.mu.Lock()
	defer db.mu.Unlock()

	db.deallocateErrByName = nil
}

func assertPreparedStatementPrepareCalls(t *testing.T, got, want []preparedStatementPrepareCall) {
	t.Helper()

	if !reflect.DeepEqual(got, want) {
		t.Fatalf("prepare calls = %#v, want %#v", got, want)
	}
}

func assertPreparedStatementDeallocateCalls(t *testing.T, got, want []string) {
	t.Helper()

	if !reflect.DeepEqual(got, want) {
		t.Fatalf("deallocate calls = %#v, want %#v", got, want)
	}
}
