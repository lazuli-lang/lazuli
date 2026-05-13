package lazuli

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
)

func TestRunRetentionScanEmptyRegistryNoop(t *testing.T) {
	restore := clearRegistriesForTest()
	defer restore()

	if err := RunRetentionScan(context.Background(), nil, time.Unix(100, 0)); err != nil {
		t.Fatalf("RunRetentionScan returned %v, want nil", err)
	}
}

func TestRetentionScanSkipsResourceWithoutRetention(t *testing.T) {
	db := &retentionDBFake{}
	err := runRetentionScan(context.Background(), db, time.Unix(100, 0), []*resourceErased{
		{Name: "Customer", SoftDelete: true},
	})
	if err != nil {
		t.Fatalf("runRetentionScan returned %v, want nil", err)
	}
	if db.beginCount != 0 {
		t.Fatalf("beginCount = %d, want 0", db.beginCount)
	}
}

func TestRetentionDeleteEmitsExpectedSQL(t *testing.T) {
	now := time.Unix(100, 0).UTC()
	db := &retentionDBFake{}

	err := runRetentionScan(context.Background(), db, now, []*resourceErased{
		{
			Name:       "Customer",
			SoftDelete: true,
			Retention:  &RetentionSpec{Window: Duration("7d"), Then: RetentionDelete},
		},
	})
	if err != nil {
		t.Fatalf("runRetentionScan returned %v, want nil", err)
	}

	tx := db.lastTx()
	if tx.execSQL != `DELETE FROM "customer" WHERE deleted_at IS NOT NULL AND deleted_at + INTERVAL '7 days' <= $1` {
		t.Fatalf("Exec SQL = %q", tx.execSQL)
	}
	if len(tx.execArgs) != 1 || !tx.execArgs[0].(time.Time).Equal(now) {
		t.Fatalf("Exec args = %#v, want now", tx.execArgs)
	}
	if !tx.committed {
		t.Fatal("transaction was not committed")
	}
}

func TestRetentionAnonymizeWithPIIFieldsEmitsExpectedSQL(t *testing.T) {
	now := time.Unix(100, 0).UTC()
	db := &retentionDBFake{}

	err := runRetentionScan(context.Background(), db, now, []*resourceErased{
		{
			Name:       "CustomerProfile",
			SoftDelete: true,
			Retention:  &RetentionSpec{Window: Duration("24h"), Then: RetentionAnonymize},
			PIIFields:  []string{"email", "phone_number"},
		},
	})
	if err != nil {
		t.Fatalf("runRetentionScan returned %v, want nil", err)
	}

	tx := db.lastTx()
	want := `UPDATE "customer_profile" SET "email" = NULL, "phone_number" = NULL WHERE deleted_at IS NOT NULL AND deleted_at + INTERVAL '86400 seconds' <= $1 AND ("email" IS NOT NULL OR "phone_number" IS NOT NULL)`
	if tx.execSQL != want {
		t.Fatalf("Exec SQL = %q, want %q", tx.execSQL, want)
	}
	if len(tx.execArgs) != 1 || !tx.execArgs[0].(time.Time).Equal(now) {
		t.Fatalf("Exec args = %#v, want now", tx.execArgs)
	}
}

func TestRetentionAnonymizeEmptyPIIFieldsNoSQL(t *testing.T) {
	db := &retentionDBFake{}

	err := runRetentionScan(context.Background(), db, time.Unix(100, 0), []*resourceErased{
		{
			Name:       "Customer",
			SoftDelete: true,
			Retention:  &RetentionSpec{Window: Duration("7d"), Then: RetentionAnonymize},
		},
	})
	if err != nil {
		t.Fatalf("runRetentionScan returned %v, want nil", err)
	}

	tx := db.lastTx()
	if tx.execSQL != "" {
		t.Fatalf("Exec SQL = %q, want empty", tx.execSQL)
	}
	if !tx.committed {
		t.Fatal("transaction was not committed")
	}
}

func TestRetentionArchiveReturnsSentinel(t *testing.T) {
	db := &retentionDBFake{}

	err := runRetentionScan(context.Background(), db, time.Unix(100, 0), []*resourceErased{
		{
			Name:       "Customer",
			SoftDelete: true,
			Retention:  &RetentionSpec{Window: Duration("7d"), Then: RetentionArchive},
		},
	})
	if !errors.Is(err, ErrRetentionArchiveNotImplemented) {
		t.Fatalf("runRetentionScan error = %v, want ErrRetentionArchiveNotImplemented", err)
	}

	tx := db.lastTx()
	if tx.committed {
		t.Fatal("archive transaction committed, want rollback-only")
	}
	if !tx.rolledBack {
		t.Fatal("archive transaction was not rolled back")
	}
}

func clearRegistriesForTest() func() {
	registry.Lock()
	oldResources := registry.resources
	oldCommands := registry.commands
	oldCommandHandlers := registry.commandHandlers
	oldQueries := registry.queries
	oldQueryHandlers := registry.queryHandlers
	registry.resources = map[string]*resourceErased{}
	registry.commands = map[string]*commandErased{}
	registry.commandHandlers = map[string]commandHandler{}
	registry.queries = map[string]*queryErased{}
	registry.queryHandlers = map[string]queryHandler{}
	registry.Unlock()

	GlobalRegistry.mu.Lock()
	oldGlobalResources := GlobalRegistry.resources
	oldGlobalCommands := GlobalRegistry.commands
	oldGlobalQueries := GlobalRegistry.queries
	oldGlobalApis := GlobalRegistry.apis
	GlobalRegistry.resources = map[string]*resourceErased{}
	GlobalRegistry.commands = map[string]commandRegistration{}
	GlobalRegistry.queries = map[string]queryRegistration{}
	GlobalRegistry.apis = map[string]apiRegistration{}
	GlobalRegistry.mu.Unlock()

	return func() {
		registry.Lock()
		registry.resources = oldResources
		registry.commands = oldCommands
		registry.commandHandlers = oldCommandHandlers
		registry.queries = oldQueries
		registry.queryHandlers = oldQueryHandlers
		registry.Unlock()

		GlobalRegistry.mu.Lock()
		GlobalRegistry.resources = oldGlobalResources
		GlobalRegistry.commands = oldGlobalCommands
		GlobalRegistry.queries = oldGlobalQueries
		GlobalRegistry.apis = oldGlobalApis
		GlobalRegistry.mu.Unlock()
	}
}

type retentionDBFake struct {
	beginCount int
	txs        []*retentionTxFake
}

func (db *retentionDBFake) Begin(context.Context) (pgx.Tx, error) {
	db.beginCount++
	tx := &retentionTxFake{}
	db.txs = append(db.txs, tx)
	return tx, nil
}

func (db *retentionDBFake) lastTx() *retentionTxFake {
	if len(db.txs) == 0 {
		panic("no fake transaction was started")
	}
	return db.txs[len(db.txs)-1]
}

type retentionTxFake struct {
	execSQL    string
	execArgs   []any
	committed  bool
	rolledBack bool
}

func (tx *retentionTxFake) Begin(context.Context) (pgx.Tx, error) { return tx, nil }
func (tx *retentionTxFake) Commit(context.Context) error {
	tx.committed = true
	return nil
}
func (tx *retentionTxFake) Rollback(context.Context) error {
	tx.rolledBack = true
	return nil
}
func (tx *retentionTxFake) CopyFrom(context.Context, pgx.Identifier, []string, pgx.CopyFromSource) (int64, error) {
	return 0, errors.New("unexpected CopyFrom")
}
func (tx *retentionTxFake) SendBatch(context.Context, *pgx.Batch) pgx.BatchResults {
	panic("unexpected SendBatch")
}
func (tx *retentionTxFake) LargeObjects() pgx.LargeObjects { panic("unexpected LargeObjects") }
func (tx *retentionTxFake) Prepare(context.Context, string, string) (*pgconn.StatementDescription, error) {
	return nil, errors.New("unexpected Prepare")
}
func (tx *retentionTxFake) Exec(_ context.Context, sql string, arguments ...any) (pgconn.CommandTag, error) {
	if strings.TrimSpace(sql) == "" {
		return pgconn.CommandTag{}, errors.New("empty SQL")
	}
	tx.execSQL = sql
	tx.execArgs = arguments
	return pgconn.CommandTag{}, nil
}
func (tx *retentionTxFake) Query(context.Context, string, ...any) (pgx.Rows, error) {
	return nil, errors.New("unexpected Query")
}
func (tx *retentionTxFake) QueryRow(context.Context, string, ...any) pgx.Row {
	panic("unexpected QueryRow")
}
func (tx *retentionTxFake) Conn() *pgx.Conn { return nil }
