package lazuli

import (
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
)

func TestNewPoolConfigDefaults(t *testing.T) {
	cfg, err := newPoolConfig("postgres://lazuli:secret@localhost:5432/app?sslmode=disable")
	if err != nil {
		t.Fatalf("newPoolConfig returned error: %v", err)
	}

	if cfg.MaxConns != 25 {
		t.Fatalf("MaxConns = %d, want 25", cfg.MaxConns)
	}
	if cfg.MinConns != 2 {
		t.Fatalf("MinConns = %d, want 2", cfg.MinConns)
	}
	if cfg.MaxConnLifetime != 30*time.Minute {
		t.Fatalf("MaxConnLifetime = %s, want 30m", cfg.MaxConnLifetime)
	}
	if cfg.MaxConnIdleTime != 5*time.Minute {
		t.Fatalf("MaxConnIdleTime = %s, want 5m", cfg.MaxConnIdleTime)
	}
	if cfg.HealthCheckPeriod != time.Minute {
		t.Fatalf("HealthCheckPeriod = %s, want 1m", cfg.HealthCheckPeriod)
	}
	if cfg.ConnConfig.DefaultQueryExecMode != pgx.QueryExecModeSimpleProtocol {
		t.Fatalf("DefaultQueryExecMode = %v, want simple protocol", cfg.ConnConfig.DefaultQueryExecMode)
	}
}

func TestNewPoolConfigParseError(t *testing.T) {
	if _, err := newPoolConfig("postgres://%zz"); err == nil {
		t.Fatal("newPoolConfig returned nil error for malformed URL")
	}
}
