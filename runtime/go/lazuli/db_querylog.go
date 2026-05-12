package lazuli

import (
	"context"
	"errors"
	"log/slog"
	"time"

	"github.com/jackc/pgx/v5/pgconn"
)

// QueryEvent describes a completed database query for structured logging.
// Query arguments are intentionally represented only as a count so logs do not
// capture sensitive bound values.
type QueryEvent struct {
	// Operation names the SQL operation or logical query being logged.
	Operation string

	// Duration is the elapsed time spent running the query.
	Duration time.Duration

	// Rows is the number of rows returned or affected, when known.
	Rows int64

	// ArgCount is the number of bound query arguments. Raw argument values are
	// never logged by LogQuery.
	ArgCount int

	// Err is the query error, when the query failed.
	Err error
}

// LogQuery writes a structured database query log entry.
//
// Successful queries are logged at info level and failed queries at error
// level. The entry includes duration_ms, operation, rows, arg_count, request_id
// when available, and error_code/error_message for failures.
func LogQuery(ctx context.Context, logger *slog.Logger, event QueryEvent) {
	if logger == nil {
		logger = slog.Default()
	}

	argCount := event.ArgCount
	if argCount < 0 {
		argCount = 0
	}

	attrs := []any{
		"operation", event.Operation,
		"duration_ms", event.Duration.Milliseconds(),
		"rows", event.Rows,
		"arg_count", argCount,
	}
	if requestID := queryLogRequestID(ctx); requestID != "" {
		attrs = append(attrs, "request_id", requestID)
	}

	logCtx := queryLogContext(ctx)
	if event.Err == nil {
		logger.InfoContext(logCtx, "lazuli db query", attrs...)
		return
	}

	code, message := queryLogError(event.Err)
	attrs = append(attrs,
		"error_code", code,
		"error_message", message,
	)
	logger.ErrorContext(logCtx, "lazuli db query", attrs...)
}

func queryLogRequestID(ctx context.Context) string {
	if ctx == nil {
		return ""
	}
	if c, ok := ctx.(*Ctx); ok {
		if c == nil {
			return ""
		}
		if c.Context != nil {
			if id := RequestID(c.Context); id != "" {
				return id
			}
		}
		return c.RequestID
	}
	return RequestID(ctx)
}

func queryLogContext(ctx context.Context) context.Context {
	if ctx == nil {
		return context.Background()
	}
	if c, ok := ctx.(*Ctx); ok {
		if c == nil || c.Context == nil {
			return context.Background()
		}
		return c.Context
	}
	return ctx
}

func queryLogError(err error) (code, message string) {
	if err == nil {
		return "", ""
	}

	var pgErr *pgconn.PgError
	if errors.As(err, &pgErr) {
		return pgErr.Code, pgErr.Message
	}
	return "", err.Error()
}
