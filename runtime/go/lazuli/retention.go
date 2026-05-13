package lazuli

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"strconv"
	"strings"
	"time"
	"unicode"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

var (
	ErrRetentionArchiveNotImplemented = errors.New("lazuli: retention archive action not yet implemented")
)

type retentionDB interface {
	Begin(context.Context) (pgx.Tx, error)
}

// RunRetentionScan walks every registered resource that has a Retention spec
// and applies its terminal action to rows whose deleted_at + window <= now.
//
// Returns a multi-error if any per-resource sweep fails; partial progress
// is committed (each resource is its own transaction).
//
// The scanner assumes:
//   - Resource has SoftDelete enabled (rows have a deleted_at column).
//   - Table name is snake_case of Resource.Name (canonical convention).
//   - For Anonymize, PIIFields lists snake_case column names to set NULL.
//     Empty list = no-op anonymize (logs an info; updates nothing).
func RunRetentionScan(ctx context.Context, pool *pgxpool.Pool, now time.Time) error {
	return runRetentionScan(ctx, pool, now, Resources())
}

// StartRetentionWorker launches a goroutine that calls RunRetentionScan
// every `interval` until ctx is cancelled. Errors are logged via slog.
// Returns immediately.
func StartRetentionWorker(ctx context.Context, pool *pgxpool.Pool, interval time.Duration) {
	if interval <= 0 {
		slog.Error("lazuli: retention worker interval must be positive", "interval", interval)
		return
	}

	go func() {
		ticker := time.NewTicker(interval)
		defer ticker.Stop()

		for {
			select {
			case <-ctx.Done():
				return
			case now := <-ticker.C:
				if err := RunRetentionScan(ctx, pool, now); err != nil {
					slog.Error("lazuli: retention scan failed", "error", err)
				}
			}
		}
	}()
}

func runRetentionScan(ctx context.Context, db retentionDB, now time.Time, resources []*resourceErased) error {
	var errs []error
	for _, resource := range resources {
		if resource == nil || resource.Retention == nil {
			continue
		}
		if err := runRetentionSweep(ctx, db, now, resource); err != nil {
			errs = append(errs, fmt.Errorf("retention sweep %s: %w", resource.Name, err))
		}
	}
	return errors.Join(errs...)
}

func runRetentionSweep(ctx context.Context, db retentionDB, now time.Time, resource *resourceErased) error {
	if db == nil {
		return errors.New("lazuli: retention scan requires a database pool")
	}

	tx, err := db.Begin(ctx)
	if err != nil {
		return err
	}
	defer func() { _ = tx.Rollback(ctx) }()

	if err := applyRetentionAction(ctx, tx, now, resource); err != nil {
		return err
	}
	return tx.Commit(ctx)
}

func applyRetentionAction(ctx context.Context, tx pgx.Tx, now time.Time, resource *resourceErased) error {
	switch resource.Retention.Then {
	case RetentionDelete:
		return applyRetentionDelete(ctx, tx, now, resource)
	case RetentionAnonymize:
		return applyRetentionAnonymize(ctx, tx, now, resource)
	case RetentionArchive:
		return ErrRetentionArchiveNotImplemented
	default:
		return fmt.Errorf("lazuli: unknown retention action %d", resource.Retention.Then)
	}
}

func applyRetentionDelete(ctx context.Context, tx pgx.Tx, now time.Time, resource *resourceErased) error {
	interval, err := retentionIntervalLiteral(resource.Retention.Window)
	if err != nil {
		return err
	}

	sql := fmt.Sprintf(
		"DELETE FROM %s WHERE deleted_at IS NOT NULL AND deleted_at + INTERVAL %s <= $1",
		quoteIdentifier(snakeCase(resource.Name)),
		interval,
	)
	_, err = tx.Exec(ctx, sql, now)
	return err
}

func applyRetentionAnonymize(ctx context.Context, tx pgx.Tx, now time.Time, resource *resourceErased) error {
	if len(resource.PIIFields) == 0 {
		slog.Info("lazuli: retention anonymize skipped; no PIIFields configured", "resource", resource.Name)
		return nil
	}

	interval, err := retentionIntervalLiteral(resource.Retention.Window)
	if err != nil {
		return err
	}

	sets := make([]string, 0, len(resource.PIIFields))
	notNull := make([]string, 0, len(resource.PIIFields))
	for _, field := range resource.PIIFields {
		if field == "" {
			return errors.New("lazuli: retention PIIFields cannot contain empty names")
		}
		column := quoteIdentifier(snakeCase(field))
		sets = append(sets, column+" = NULL")
		notNull = append(notNull, column+" IS NOT NULL")
	}

	sql := fmt.Sprintf(
		"UPDATE %s SET %s WHERE deleted_at IS NOT NULL AND deleted_at + INTERVAL %s <= $1 AND (%s)",
		quoteIdentifier(snakeCase(resource.Name)),
		strings.Join(sets, ", "),
		interval,
		strings.Join(notNull, " OR "),
	)
	_, err = tx.Exec(ctx, sql, now)
	return err
}

func retentionIntervalLiteral(window Duration) (string, error) {
	raw := strings.TrimSpace(string(window))
	if raw == "" {
		return "", errors.New("lazuli: retention window cannot be empty")
	}

	if amount, unit, ok := splitDurationLiteral(raw); ok {
		switch unit {
		case "d", "day", "days":
			return quoteLiteral(amount + " days"), nil
		case "w", "week", "weeks":
			return quoteLiteral(amount + " weeks"), nil
		case "y", "year", "years":
			return quoteLiteral(amount + " years"), nil
		}
	}

	parsed, err := time.ParseDuration(raw)
	if err != nil {
		return "", fmt.Errorf("lazuli: invalid retention window %q: %w", raw, err)
	}
	return quoteLiteral(strconv.FormatFloat(parsed.Seconds(), 'f', -1, 64) + " seconds"), nil
}

func splitDurationLiteral(raw string) (amount string, unit string, ok bool) {
	i := 0
	for i < len(raw) && (raw[i] >= '0' && raw[i] <= '9') {
		i++
	}
	if i == 0 || i == len(raw) {
		return "", "", false
	}
	return raw[:i], strings.ToLower(strings.TrimSpace(raw[i:])), true
}

func quoteIdentifier(name string) string {
	return pgx.Identifier{name}.Sanitize()
}

func quoteLiteral(value string) string {
	return "'" + strings.ReplaceAll(value, "'", "''") + "'"
}

func snakeCase(name string) string {
	var out []rune
	var prev rune
	for i, r := range name {
		if r == '-' || r == ' ' || r == '.' {
			if len(out) > 0 && out[len(out)-1] != '_' {
				out = append(out, '_')
			}
			prev = '_'
			continue
		}
		if r == '_' {
			if len(out) > 0 && out[len(out)-1] != '_' {
				out = append(out, '_')
			}
			prev = r
			continue
		}
		if unicode.IsUpper(r) && i > 0 && prev != '_' {
			out = append(out, '_')
		}
		out = append(out, unicode.ToLower(r))
		prev = r
	}
	return strings.Trim(string(out), "_")
}
