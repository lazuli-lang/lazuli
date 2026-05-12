package lazuli

import (
	"context"
	"errors"
	"testing"

	"lazuli.dev/runtime/lazuli/observability"
)

func TestPingDBReturnsPingResult(t *testing.T) {
	wantErr := errors.New("database unavailable")
	db := &fakeDBPinger{err: wantErr}

	if err := PingDB(context.Background(), db); !errors.Is(err, wantErr) {
		t.Fatalf("PingDB error = %v, want %v", err, wantErr)
	}

	if db.calls != 1 {
		t.Fatalf("Ping calls = %d, want 1", db.calls)
	}
}

func TestPingDBReturnsNilDBError(t *testing.T) {
	if err := PingDB(context.Background(), nil); !errors.Is(err, ErrNilDB) {
		t.Fatalf("PingDB error = %v, want %v", err, ErrNilDB)
	}
}

func TestPingDBReturnsNilDBErrorForTypedNil(t *testing.T) {
	var db *fakePointerDBPinger

	if err := PingDB(context.Background(), db); !errors.Is(err, ErrNilDB) {
		t.Fatalf("PingDB error = %v, want %v", err, ErrNilDB)
	}
}

func TestPingDBRespectsCanceledContext(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	db := &fakeDBPinger{}

	if err := PingDB(ctx, db); !errors.Is(err, context.Canceled) {
		t.Fatalf("PingDB error = %v, want %v", err, context.Canceled)
	}

	if db.calls != 0 {
		t.Fatalf("Ping calls = %d, want 0", db.calls)
	}
}

func TestDBHealthCheck(t *testing.T) {
	wantErr := errors.New("database unavailable")
	db := &fakeDBPinger{err: wantErr}
	check := DBHealthCheck("primary", db)

	var _ observability.ReadinessProbe = check

	if got := check.Name(); got != "primary" {
		t.Fatalf("Name() = %q, want primary", got)
	}
	if err := check.Check(context.Background()); !errors.Is(err, wantErr) {
		t.Fatalf("Check error = %v, want %v", err, wantErr)
	}
	if db.calls != 1 {
		t.Fatalf("Ping calls = %d, want 1", db.calls)
	}
}

func TestDBHealthCheckDefaultsName(t *testing.T) {
	check := DBHealthCheck("", &fakeDBPinger{})

	if got := check.Name(); got != "db" {
		t.Fatalf("Name() = %q, want db", got)
	}
}

type fakeDBPinger struct {
	calls int
	err   error
}

func (db *fakeDBPinger) Ping(context.Context) error {
	db.calls++
	return db.err
}

type fakePointerDBPinger struct{}

func (*fakePointerDBPinger) Ping(context.Context) error {
	return nil
}
