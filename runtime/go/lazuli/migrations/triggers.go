package migrations

import (
	"errors"
	"fmt"
	"strings"
)

var (
	// ErrInvalidSQLExtensionName is returned when an extension helper receives
	// an extension name outside Lazuli's safe quoted extension-name subset.
	ErrInvalidSQLExtensionName = errors.New("migrations: invalid SQL extension name")
	// ErrInvalidTriggerTiming is returned when a trigger helper receives an
	// unknown CREATE TRIGGER timing.
	ErrInvalidTriggerTiming = errors.New("migrations: invalid trigger timing")
	// ErrNoTriggerEvents is returned when a trigger helper receives no events.
	ErrNoTriggerEvents = errors.New("migrations: trigger requires at least one event")
	// ErrInvalidTriggerEvent is returned when a trigger helper receives an
	// unknown or duplicate CREATE TRIGGER event.
	ErrInvalidTriggerEvent = errors.New("migrations: invalid trigger event")
	// ErrInvalidTriggerBody is returned when a trigger helper receives an unsafe
	// or incomplete CREATE TRIGGER body fragment.
	ErrInvalidTriggerBody = errors.New("migrations: invalid trigger body")
)

// TriggerTiming selects the CREATE TRIGGER timing clause.
type TriggerTiming string

const (
	// TriggerTimingBefore emits BEFORE.
	TriggerTimingBefore TriggerTiming = "before"
	// TriggerTimingAfter emits AFTER.
	TriggerTimingAfter TriggerTiming = "after"
	// TriggerTimingInsteadOf emits INSTEAD OF.
	TriggerTimingInsteadOf TriggerTiming = "instead_of"
)

// TriggerEvent names a CREATE TRIGGER event.
type TriggerEvent string

const (
	// TriggerEventInsert emits INSERT.
	TriggerEventInsert TriggerEvent = "insert"
	// TriggerEventUpdate emits UPDATE.
	TriggerEventUpdate TriggerEvent = "update"
	// TriggerEventDelete emits DELETE.
	TriggerEventDelete TriggerEvent = "delete"
	// TriggerEventTruncate emits TRUNCATE.
	TriggerEventTruncate TriggerEvent = "truncate"
)

// CreateTriggerOptions configures BuildCreateTriggerSQL.
type CreateTriggerOptions struct {
	// Name is the trigger name.
	Name string
	// Table is the schema-qualified or unqualified trigger target.
	Table TableName
	// Timing is BEFORE, AFTER, or INSTEAD OF.
	Timing TriggerTiming
	// Events is canonicalized to INSERT, UPDATE, DELETE, TRUNCATE order.
	Events []TriggerEvent
	// Body is the CREATE TRIGGER tail after ON <table>, without a trailing
	// semicolon. It must contain EXECUTE FUNCTION or EXECUTE PROCEDURE.
	Body string
}

// BuildCreateExtensionSQL returns a PostgreSQL CREATE EXTENSION IF NOT EXISTS
// statement. It only builds SQL; callers remain responsible for choosing the
// connection and execution policy.
func BuildCreateExtensionSQL(name string) (string, error) {
	extension, err := quoteSQLExtensionName(name)
	if err != nil {
		return "", err
	}
	return "CREATE EXTENSION IF NOT EXISTS " + extension + ";", nil
}

// BuildCreateTriggerSQL returns a PostgreSQL CREATE TRIGGER statement with
// quoted trigger/table identifiers and deterministic event ordering.
func BuildCreateTriggerSQL(opts CreateTriggerOptions) (string, error) {
	name, err := quoteSQLIdentifier("trigger name", opts.Name)
	if err != nil {
		return "", err
	}
	table, err := quoteTableName(opts.Table)
	if err != nil {
		return "", err
	}
	timing, err := triggerTimingSQL(opts.Timing)
	if err != nil {
		return "", err
	}
	events, eventSet, err := triggerEventsSQL(opts.Events)
	if err != nil {
		return "", err
	}
	body, err := triggerBodySQL(opts.Body, opts.Timing, eventSet)
	if err != nil {
		return "", err
	}

	return "CREATE TRIGGER " + name + " " + timing + " " + events + " ON " + table + " " + body + ";", nil
}

func quoteSQLExtensionName(name string) (string, error) {
	if !validSQLExtensionName(name) {
		return "", fmt.Errorf("%w: extension name %q", ErrInvalidSQLExtensionName, name)
	}
	return `"` + name + `"`, nil
}

func validSQLExtensionName(name string) bool {
	if name == "" {
		return false
	}
	for i := 0; i < len(name); i++ {
		c := name[i]
		if i == 0 {
			if !isSQLIdentifierLetter(c) && c != '_' {
				return false
			}
			continue
		}
		if !isSQLIdentifierLetter(c) && !isSQLIdentifierDigit(c) && c != '_' && c != '-' {
			return false
		}
	}
	return name[len(name)-1] != '-'
}

func triggerTimingSQL(timing TriggerTiming) (string, error) {
	switch timing {
	case TriggerTimingBefore:
		return "BEFORE", nil
	case TriggerTimingAfter:
		return "AFTER", nil
	case TriggerTimingInsteadOf:
		return "INSTEAD OF", nil
	default:
		return "", fmt.Errorf("%w %q", ErrInvalidTriggerTiming, timing)
	}
}

func triggerEventsSQL(events []TriggerEvent) (string, map[TriggerEvent]bool, error) {
	if len(events) == 0 {
		return "", nil, ErrNoTriggerEvents
	}

	seen := make(map[TriggerEvent]bool, len(events))
	for _, event := range events {
		if _, ok := triggerEventSQL(event); !ok {
			return "", nil, fmt.Errorf("%w %q", ErrInvalidTriggerEvent, event)
		}
		if seen[event] {
			return "", nil, fmt.Errorf("%w: duplicate %q", ErrInvalidTriggerEvent, event)
		}
		seen[event] = true
	}

	ordered := make([]string, 0, len(seen))
	for _, event := range []TriggerEvent{
		TriggerEventInsert,
		TriggerEventUpdate,
		TriggerEventDelete,
		TriggerEventTruncate,
	} {
		if seen[event] {
			part, _ := triggerEventSQL(event)
			ordered = append(ordered, part)
		}
	}
	return strings.Join(ordered, " OR "), seen, nil
}

func triggerEventSQL(event TriggerEvent) (string, bool) {
	switch event {
	case TriggerEventInsert:
		return "INSERT", true
	case TriggerEventUpdate:
		return "UPDATE", true
	case TriggerEventDelete:
		return "DELETE", true
	case TriggerEventTruncate:
		return "TRUNCATE", true
	default:
		return "", false
	}
}

func triggerBodySQL(body string, timing TriggerTiming, events map[TriggerEvent]bool) (string, error) {
	body = strings.TrimSpace(body)
	if body == "" {
		return "", ErrInvalidTriggerBody
	}
	if strings.ContainsAny(body, ";\x00") || strings.Contains(body, "--") || strings.Contains(body, "/*") || strings.Contains(body, "*/") {
		return "", fmt.Errorf("%w: body must be a single uncommented SQL fragment", ErrInvalidTriggerBody)
	}
	if !triggerBodyHasExecute(body) {
		return "", fmt.Errorf("%w: body must contain EXECUTE FUNCTION or EXECUTE PROCEDURE", ErrInvalidTriggerBody)
	}
	upper := strings.ToUpper(body)
	if timing == TriggerTimingInsteadOf {
		if events[TriggerEventTruncate] {
			return "", fmt.Errorf("%w: INSTEAD OF triggers cannot use TRUNCATE", ErrInvalidTriggerEvent)
		}
		if !strings.Contains(upper, "FOR EACH ROW") {
			return "", fmt.Errorf("%w: INSTEAD OF triggers must be FOR EACH ROW", ErrInvalidTriggerBody)
		}
	}
	if events[TriggerEventTruncate] && strings.Contains(upper, "FOR EACH ROW") {
		return "", fmt.Errorf("%w: TRUNCATE triggers cannot be FOR EACH ROW", ErrInvalidTriggerBody)
	}
	return body, nil
}

func triggerBodyHasExecute(body string) bool {
	fields := strings.Fields(strings.ToUpper(body))
	for i := 0; i+1 < len(fields); i++ {
		if fields[i] == "EXECUTE" && (fields[i+1] == "FUNCTION" || fields[i+1] == "PROCEDURE") {
			return true
		}
	}
	return false
}
