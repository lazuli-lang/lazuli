// Package audit defines the Sink contract for shipping audit rows
// (written by SEC-C2 to lazuli_audit table) to external SIEMs.
//
// Boundary discipline: this package never names a concrete sink.
// Adapters (Splunk HEC, Datadog Logs, Elastic, S3+Athena) live in
// @lazuli/plugin-audit-sink, selected at boot via registry.lzi bindings.
package audit

import (
	"context"
	"errors"
	"time"
)

// Sink ships an audit row to an external destination. Implementations
// MUST be safe for concurrent use. Failures should NOT block the
// command tx (which already committed); use background retry.
type Sink interface {
	Ship(ctx context.Context, row Row) error
	Close() error
}

// Row is the canonical audit-row shape. Mirrors the lazuli_audit
// table columns from SEC-C2's migration.
type Row struct {
	ID         int64
	Command    string
	Actor      string
	Tenant     *string
	Decision   string // "allowed" | "denied"
	Fields     map[string]any
	RecordedAt time.Time
}

var (
	ErrSinkUnavailable = errors.New("lazuli/audit: sink unavailable")
	ErrSinkRateLimited = errors.New("lazuli/audit: sink rate-limited")
)

// NoopSink is the default. Audit rows are written to lazuli_audit
// but NOT fanned out externally. Pilots SHOULD bind a real sink
// (Splunk HEC, Datadog, S3+Athena) for tamper-resistance - the
// app process can DELETE FROM lazuli_audit, but it cannot delete
// rows already shipped to an external SIEM.
type NoopSink struct{}

func (NoopSink) Ship(ctx context.Context, row Row) error { return nil }
func (NoopSink) Close() error                            { return nil }
