package lazuli

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"log/slog"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgconn"
)

func TestLogQueryLogsSafeMetadataAndRequestID(t *testing.T) {
	var buf bytes.Buffer
	logger := slog.New(slog.NewJSONHandler(&buf, nil))
	ctx := context.WithValue(t.Context(), requestIDKey{}, "req-123")

	LogQuery(ctx, logger, QueryEvent{
		Operation: "select customer",
		Duration:  25 * time.Millisecond,
		Rows:      3,
		ArgCount:  2,
	})

	record := decodeQueryLogRecord(t, &buf)
	assertQueryLogString(t, record, "level", "INFO")
	assertQueryLogString(t, record, "msg", "lazuli db query")
	assertQueryLogString(t, record, "operation", "select customer")
	assertQueryLogNumber(t, record, "duration_ms", 25)
	assertQueryLogNumber(t, record, "rows", 3)
	assertQueryLogNumber(t, record, "arg_count", 2)
	assertQueryLogString(t, record, "request_id", "req-123")

	if _, ok := record["args"]; ok {
		t.Fatal("LogQuery logged raw args")
	}
}

func TestLogQueryLogsPostgresErrorDetails(t *testing.T) {
	var buf bytes.Buffer
	logger := slog.New(slog.NewJSONHandler(&buf, nil))

	LogQuery(nil, logger, QueryEvent{
		Operation: "insert customer",
		Duration:  10 * time.Millisecond,
		Rows:      0,
		ArgCount:  1,
		Err: &pgconn.PgError{
			Code:    "23505",
			Message: "duplicate key value violates unique constraint",
		},
	})

	record := decodeQueryLogRecord(t, &buf)
	assertQueryLogString(t, record, "level", "ERROR")
	assertQueryLogString(t, record, "error_code", "23505")
	assertQueryLogString(t, record, "error_message", "duplicate key value violates unique constraint")
}

func TestLogQueryLogsGenericErrorMessage(t *testing.T) {
	var buf bytes.Buffer
	logger := slog.New(slog.NewJSONHandler(&buf, nil))

	LogQuery(t.Context(), logger, QueryEvent{
		Operation: "select customer",
		Err:       errors.New("connection refused"),
	})

	record := decodeQueryLogRecord(t, &buf)
	assertQueryLogString(t, record, "level", "ERROR")
	assertQueryLogString(t, record, "error_code", "")
	assertQueryLogString(t, record, "error_message", "connection refused")
}

func TestLogQueryReadsRequestIDFromCtxField(t *testing.T) {
	var buf bytes.Buffer
	logger := slog.New(slog.NewJSONHandler(&buf, nil))

	LogQuery(&Ctx{Context: t.Context(), RequestID: "req-field"}, logger, QueryEvent{
		Operation: "select customer",
	})

	record := decodeQueryLogRecord(t, &buf)
	assertQueryLogString(t, record, "request_id", "req-field")
}

func decodeQueryLogRecord(t *testing.T, buf *bytes.Buffer) map[string]any {
	t.Helper()

	var record map[string]any
	raw := strings.TrimSpace(buf.String())
	if raw == "" {
		t.Fatal("expected log record, got empty output")
	}
	if err := json.Unmarshal([]byte(raw), &record); err != nil {
		t.Fatalf("failed to decode log record %q: %v", raw, err)
	}
	return record
}

func assertQueryLogString(t *testing.T, record map[string]any, key, want string) {
	t.Helper()

	got, ok := record[key].(string)
	if !ok {
		t.Fatalf("log %s = %#v, want string %q", key, record[key], want)
	}
	if got != want {
		t.Fatalf("log %s = %q, want %q", key, got, want)
	}
}

func assertQueryLogNumber(t *testing.T, record map[string]any, key string, want float64) {
	t.Helper()

	got, ok := record[key].(float64)
	if !ok {
		t.Fatalf("log %s = %#v, want number %v", key, record[key], want)
	}
	if got != want {
		t.Fatalf("log %s = %v, want %v", key, got, want)
	}
}
