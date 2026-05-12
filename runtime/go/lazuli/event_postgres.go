package lazuli

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"reflect"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgtype"
)

const (
	// PostgresEventStoreSchemaSQL is the minimal table shape expected by
	// PostgresEventStore. It is exposed so hosts can include it in their own
	// migration flow without the runtime opening connections at init time.
	PostgresEventStoreSchemaSQL = `CREATE TABLE IF NOT EXISTS lazuli_events (sequence BIGSERIAL PRIMARY KEY, name TEXT NOT NULL, trace BOOLEAN NOT NULL DEFAULT FALSE, tenant_org_id BIGINT, actor TEXT NOT NULL, user_id BIGINT, payload JSONB, occurred_at TIMESTAMPTZ NOT NULL)`

	postgresEventColumns     = "sequence, name, trace, tenant_org_id, actor, user_id, payload, occurred_at"
	postgresEventInsertSQL   = "INSERT INTO lazuli_events (name, trace, tenant_org_id, actor, user_id, payload, occurred_at) VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7) RETURNING " + postgresEventColumns
	postgresEventSelectSQL   = "SELECT " + postgresEventColumns + " FROM lazuli_events"
	postgresEventOrderSQL    = " ORDER BY sequence ASC"
	postgresEventMaxSequence = uint64(1<<63 - 1)
)

// ErrNilPostgresEventDB is returned when a PostgresEventStore has no pgx db.
var ErrNilPostgresEventDB = errors.New("lazuli: postgres event store db is nil")

// PostgresEventQuerier is the subset of pgx pool and transaction APIs required
// by PostgresEventStore.
type PostgresEventQuerier interface {
	Query(context.Context, string, ...any) (pgx.Rows, error)
	QueryRow(context.Context, string, ...any) pgx.Row
}

// PostgresEventStore persists runtime events in a pgx-backed Postgres table.
type PostgresEventStore struct {
	db PostgresEventQuerier
}

var (
	_ EventStore       = (*PostgresEventStore)(nil)
	_ EventReplayStore = (*PostgresEventStore)(nil)
)

// NewPostgresEventStore returns an EventStore backed by a pgx pool or
// transaction.
func NewPostgresEventStore(db PostgresEventQuerier) *PostgresEventStore {
	return &PostgresEventStore{db: db}
}

// Append stores event and returns the row assigned by Postgres.
func (s *PostgresEventStore) Append(ctx context.Context, event Event) (StoredEvent, error) {
	if err := ctx.Err(); err != nil {
		return StoredEvent{}, err
	}
	if s == nil || isNilPostgresEventQuerier(s.db) {
		return StoredEvent{}, ErrNilPostgresEventDB
	}

	payload, err := marshalPostgresEventPayload(event.Payload)
	if err != nil {
		return StoredEvent{}, err
	}

	row := s.db.QueryRow(ctx, postgresEventInsertSQL,
		event.Name,
		event.Trace,
		postgresEventTenantArg(event.Tenant),
		string(event.Actor),
		postgresEventUserIDArg(event.UserID),
		payload,
		event.OccurredAt,
	)
	stored, err := scanPostgresStoredEvent(row)
	if err != nil {
		return StoredEvent{}, fmt.Errorf("lazuli: append postgres event: %w", err)
	}
	return cloneStoredEvent(stored), nil
}

// List returns sequence-ordered events matching filter.
func (s *PostgresEventStore) List(ctx context.Context, filter EventListFilter) ([]StoredEvent, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	if s == nil || isNilPostgresEventQuerier(s.db) {
		return nil, ErrNilPostgresEventDB
	}
	if filter.SinceSequence > postgresEventMaxSequence {
		return []StoredEvent{}, nil
	}

	sql, args := buildPostgresEventListQuery(filter)
	rows, err := s.db.Query(ctx, sql, args...)
	if err != nil {
		return nil, fmt.Errorf("lazuli: list postgres events: %w", err)
	}
	defer rows.Close()

	events, err := collectPostgresStoredEvents(rows)
	if err != nil {
		return nil, fmt.Errorf("lazuli: list postgres events: %w", err)
	}
	return events, nil
}

// ReplayEvents streams matching events in sequence order.
func (s *PostgresEventStore) ReplayEvents(ctx context.Context, filter EventReplayFilter, yield func(Event) error) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	if s == nil || isNilPostgresEventQuerier(s.db) {
		return ErrNilPostgresEventDB
	}

	sql, args := buildPostgresEventReplayQuery(filter)
	rows, err := s.db.Query(ctx, sql, args...)
	if err != nil {
		return fmt.Errorf("lazuli: replay postgres events: %w", err)
	}
	defer rows.Close()

	for rows.Next() {
		if err := ctx.Err(); err != nil {
			return err
		}
		stored, err := scanPostgresStoredEvent(rows)
		if err != nil {
			return fmt.Errorf("lazuli: replay postgres events: %w", err)
		}
		if err := yield(cloneEvent(stored.Event)); err != nil {
			return err
		}
	}
	if err := rows.Err(); err != nil {
		return fmt.Errorf("lazuli: replay postgres events: %w", err)
	}
	return ctx.Err()
}

func buildPostgresEventListQuery(filter EventListFilter) (string, []any) {
	args := []any{int64(filter.SinceSequence)}
	clauses := []string{"sequence > $1"}
	if filter.Name != "" {
		args = append(args, filter.Name)
		clauses = append(clauses, fmt.Sprintf("name = $%d", len(args)))
	}
	if filter.Tenant != nil {
		args = append(args, filter.Tenant.OrgID)
		clauses = append(clauses, fmt.Sprintf("tenant_org_id = $%d", len(args)))
	}

	return postgresEventSelectSQL + " WHERE " + strings.Join(clauses, " AND ") + postgresEventOrderSQL, args
}

func buildPostgresEventReplayQuery(filter EventReplayFilter) (string, []any) {
	var args []any
	var clauses []string
	if filter.Tenant != nil {
		args = append(args, filter.Tenant.OrgID)
		clauses = append(clauses, fmt.Sprintf("tenant_org_id = $%d", len(args)))
	}
	if len(filter.Names) > 0 {
		names := append([]string(nil), filter.Names...)
		args = append(args, names)
		clauses = append(clauses, fmt.Sprintf("name = ANY($%d::text[])", len(args)))
	}
	if !filter.Since.IsZero() {
		args = append(args, filter.Since)
		clauses = append(clauses, fmt.Sprintf("occurred_at >= $%d", len(args)))
	}
	if !filter.Until.IsZero() {
		args = append(args, filter.Until)
		clauses = append(clauses, fmt.Sprintf("occurred_at < $%d", len(args)))
	}

	sql := postgresEventSelectSQL
	if len(clauses) > 0 {
		sql += " WHERE " + strings.Join(clauses, " AND ")
	}
	return sql + postgresEventOrderSQL, args
}

func collectPostgresStoredEvents(rows pgx.Rows) ([]StoredEvent, error) {
	events := make([]StoredEvent, 0)
	for rows.Next() {
		stored, err := scanPostgresStoredEvent(rows)
		if err != nil {
			return nil, err
		}
		events = append(events, cloneStoredEvent(stored))
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	return events, nil
}

type postgresEventScanner interface {
	Scan(...any) error
}

func scanPostgresStoredEvent(scanner postgresEventScanner) (StoredEvent, error) {
	var sequence int64
	var name string
	var trace bool
	var tenantOrgID pgtype.Int8
	var actor string
	var userID pgtype.Int8
	var payload []byte
	var occurredAt time.Time

	if err := scanner.Scan(&sequence, &name, &trace, &tenantOrgID, &actor, &userID, &payload, &occurredAt); err != nil {
		return StoredEvent{}, err
	}
	if sequence < 0 {
		return StoredEvent{}, fmt.Errorf("lazuli: postgres event sequence %d is invalid", sequence)
	}

	eventPayload, err := unmarshalPostgresEventPayload(payload)
	if err != nil {
		return StoredEvent{}, err
	}
	event := Event{
		Name:       name,
		Trace:      trace,
		Actor:      Actor(actor),
		Payload:    eventPayload,
		OccurredAt: occurredAt,
	}
	if tenantOrgID.Valid {
		event.Tenant = &Tenant{OrgID: ID(tenantOrgID.Int64)}
	}
	if userID.Valid {
		id := ID(userID.Int64)
		event.UserID = &id
	}

	return StoredEvent{
		Sequence: uint64(sequence),
		Event:    event,
	}, nil
}

func marshalPostgresEventPayload(payload map[string]any) (any, error) {
	if payload == nil {
		return nil, nil
	}
	raw, err := json.Marshal(payload)
	if err != nil {
		return nil, fmt.Errorf("lazuli: encode postgres event payload: %w", err)
	}
	return string(raw), nil
}

func unmarshalPostgresEventPayload(raw []byte) (map[string]any, error) {
	if len(raw) == 0 {
		return nil, nil
	}

	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.UseNumber()

	var payload map[string]any
	if err := decoder.Decode(&payload); err != nil {
		return nil, fmt.Errorf("lazuli: decode postgres event payload: %w", err)
	}
	if payload == nil {
		return nil, nil
	}
	return normalizePostgresEventMap(payload), nil
}

func normalizePostgresEventMap(payload map[string]any) map[string]any {
	out := make(map[string]any, len(payload))
	for key, value := range payload {
		out[key] = normalizePostgresEventValue(value)
	}
	return out
}

func normalizePostgresEventValue(value any) any {
	switch v := value.(type) {
	case map[string]any:
		return normalizePostgresEventMap(v)
	case []any:
		out := make([]any, len(v))
		for i := range v {
			out[i] = normalizePostgresEventValue(v[i])
		}
		return out
	case json.Number:
		if i, err := v.Int64(); err == nil {
			return i
		}
		if f, err := v.Float64(); err == nil {
			return f
		}
		return v.String()
	default:
		return v
	}
}

func postgresEventTenantArg(tenant *Tenant) any {
	if tenant == nil {
		return nil
	}
	return tenant.OrgID
}

func postgresEventUserIDArg(userID *ID) any {
	if userID == nil {
		return nil
	}
	return *userID
}

func isNilPostgresEventQuerier(db PostgresEventQuerier) bool {
	if db == nil {
		return true
	}

	value := reflect.ValueOf(db)
	switch value.Kind() {
	case reflect.Chan, reflect.Func, reflect.Interface, reflect.Map, reflect.Ptr, reflect.Slice:
		return value.IsNil()
	default:
		return false
	}
}
