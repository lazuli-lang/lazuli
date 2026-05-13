package migrations

import (
	"errors"
	"testing"
)

func TestBuildCreateExtensionSQL(t *testing.T) {
	tests := []struct {
		name string
		want string
	}{
		{name: "pg_trgm", want: `CREATE EXTENSION IF NOT EXISTS "pg_trgm";`},
		{name: "uuid-ossp", want: `CREATE EXTENSION IF NOT EXISTS "uuid-ossp";`},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			statement, err := BuildCreateExtensionSQL(tt.name)
			if err != nil {
				t.Fatalf("BuildCreateExtensionSQL returned %v", err)
			}
			if statement != tt.want {
				t.Fatalf("statement = %q, want %q", statement, tt.want)
			}
		})
	}
}

func TestBuildCreateTriggerSQL(t *testing.T) {
	statement, err := BuildCreateTriggerSQL(CreateTriggerOptions{
		Name:   "users_set_updated_at",
		Table:  TableName{Schema: "app", Name: "users"},
		Timing: TriggerTimingBefore,
		Events: []TriggerEvent{
			TriggerEventUpdate,
			TriggerEventInsert,
		},
		Body: `FOR EACH ROW EXECUTE FUNCTION app.set_updated_at()`,
	})
	if err != nil {
		t.Fatalf("BuildCreateTriggerSQL returned %v", err)
	}

	want := `CREATE TRIGGER "users_set_updated_at" BEFORE INSERT OR UPDATE ON "app"."users" FOR EACH ROW EXECUTE FUNCTION app.set_updated_at();`
	if statement != want {
		t.Fatalf("statement = %q, want %q", statement, want)
	}
}

func TestBuildCreateTriggerSQLAllowsStatementTriggers(t *testing.T) {
	statement, err := BuildCreateTriggerSQL(CreateTriggerOptions{
		Name:   "accounts_audit_truncate",
		Table:  TableName{Name: "accounts"},
		Timing: TriggerTimingAfter,
		Events: []TriggerEvent{
			TriggerEventTruncate,
			TriggerEventDelete,
		},
		Body: `FOR EACH STATEMENT EXECUTE FUNCTION audit_accounts()`,
	})
	if err != nil {
		t.Fatalf("BuildCreateTriggerSQL returned %v", err)
	}

	want := `CREATE TRIGGER "accounts_audit_truncate" AFTER DELETE OR TRUNCATE ON "accounts" FOR EACH STATEMENT EXECUTE FUNCTION audit_accounts();`
	if statement != want {
		t.Fatalf("statement = %q, want %q", statement, want)
	}
}

func TestBuildCreateTriggerSQLAllowsInsteadOfRowTriggers(t *testing.T) {
	statement, err := BuildCreateTriggerSQL(CreateTriggerOptions{
		Name:   "view_write",
		Table:  TableName{Name: "active_users"},
		Timing: TriggerTimingInsteadOf,
		Events: []TriggerEvent{
			TriggerEventDelete,
			TriggerEventUpdate,
			TriggerEventInsert,
		},
		Body: `FOR EACH ROW EXECUTE FUNCTION write_active_users()`,
	})
	if err != nil {
		t.Fatalf("BuildCreateTriggerSQL returned %v", err)
	}

	want := `CREATE TRIGGER "view_write" INSTEAD OF INSERT OR UPDATE OR DELETE ON "active_users" FOR EACH ROW EXECUTE FUNCTION write_active_users();`
	if statement != want {
		t.Fatalf("statement = %q, want %q", statement, want)
	}
}

func TestBuildCreateTriggerSQLRejectsInvalidInput(t *testing.T) {
	tests := []struct {
		name string
		err  error
		run  func() error
	}{
		{
			name: "trigger name",
			err:  ErrInvalidSQLIdentifier,
			run: func() error {
				_, err := BuildCreateTriggerSQL(CreateTriggerOptions{
					Name:   "bad-trigger",
					Table:  TableName{Name: "users"},
					Timing: TriggerTimingBefore,
					Events: []TriggerEvent{TriggerEventUpdate},
					Body:   `FOR EACH ROW EXECUTE FUNCTION touch_user()`,
				})
				return err
			},
		},
		{
			name: "table name",
			err:  ErrInvalidSQLIdentifier,
			run: func() error {
				_, err := BuildCreateTriggerSQL(CreateTriggerOptions{
					Name:   "touch_user",
					Table:  TableName{Name: "users;drop"},
					Timing: TriggerTimingBefore,
					Events: []TriggerEvent{TriggerEventUpdate},
					Body:   `FOR EACH ROW EXECUTE FUNCTION touch_user()`,
				})
				return err
			},
		},
		{
			name: "timing",
			err:  ErrInvalidTriggerTiming,
			run: func() error {
				_, err := BuildCreateTriggerSQL(CreateTriggerOptions{
					Name:   "touch_user",
					Table:  TableName{Name: "users"},
					Timing: TriggerTiming("during"),
					Events: []TriggerEvent{TriggerEventUpdate},
					Body:   `FOR EACH ROW EXECUTE FUNCTION touch_user()`,
				})
				return err
			},
		},
		{
			name: "no events",
			err:  ErrNoTriggerEvents,
			run: func() error {
				_, err := BuildCreateTriggerSQL(CreateTriggerOptions{
					Name:   "touch_user",
					Table:  TableName{Name: "users"},
					Timing: TriggerTimingBefore,
					Body:   `FOR EACH ROW EXECUTE FUNCTION touch_user()`,
				})
				return err
			},
		},
		{
			name: "unknown event",
			err:  ErrInvalidTriggerEvent,
			run: func() error {
				_, err := BuildCreateTriggerSQL(CreateTriggerOptions{
					Name:   "touch_user",
					Table:  TableName{Name: "users"},
					Timing: TriggerTimingBefore,
					Events: []TriggerEvent{TriggerEvent("merge")},
					Body:   `FOR EACH ROW EXECUTE FUNCTION touch_user()`,
				})
				return err
			},
		},
		{
			name: "duplicate event",
			err:  ErrInvalidTriggerEvent,
			run: func() error {
				_, err := BuildCreateTriggerSQL(CreateTriggerOptions{
					Name:   "touch_user",
					Table:  TableName{Name: "users"},
					Timing: TriggerTimingBefore,
					Events: []TriggerEvent{TriggerEventUpdate, TriggerEventUpdate},
					Body:   `FOR EACH ROW EXECUTE FUNCTION touch_user()`,
				})
				return err
			},
		},
		{
			name: "empty body",
			err:  ErrInvalidTriggerBody,
			run: func() error {
				_, err := BuildCreateTriggerSQL(CreateTriggerOptions{
					Name:   "touch_user",
					Table:  TableName{Name: "users"},
					Timing: TriggerTimingBefore,
					Events: []TriggerEvent{TriggerEventUpdate},
				})
				return err
			},
		},
		{
			name: "semicolon body",
			err:  ErrInvalidTriggerBody,
			run: func() error {
				_, err := BuildCreateTriggerSQL(CreateTriggerOptions{
					Name:   "touch_user",
					Table:  TableName{Name: "users"},
					Timing: TriggerTimingBefore,
					Events: []TriggerEvent{TriggerEventUpdate},
					Body:   `FOR EACH ROW EXECUTE FUNCTION touch_user();`,
				})
				return err
			},
		},
		{
			name: "incomplete body",
			err:  ErrInvalidTriggerBody,
			run: func() error {
				_, err := BuildCreateTriggerSQL(CreateTriggerOptions{
					Name:   "touch_user",
					Table:  TableName{Name: "users"},
					Timing: TriggerTimingBefore,
					Events: []TriggerEvent{TriggerEventUpdate},
					Body:   `FOR EACH ROW`,
				})
				return err
			},
		},
		{
			name: "truncate row body",
			err:  ErrInvalidTriggerBody,
			run: func() error {
				_, err := BuildCreateTriggerSQL(CreateTriggerOptions{
					Name:   "touch_user",
					Table:  TableName{Name: "users"},
					Timing: TriggerTimingAfter,
					Events: []TriggerEvent{TriggerEventTruncate},
					Body:   `FOR EACH ROW EXECUTE FUNCTION touch_user()`,
				})
				return err
			},
		},
		{
			name: "instead of truncate",
			err:  ErrInvalidTriggerEvent,
			run: func() error {
				_, err := BuildCreateTriggerSQL(CreateTriggerOptions{
					Name:   "touch_user",
					Table:  TableName{Name: "users"},
					Timing: TriggerTimingInsteadOf,
					Events: []TriggerEvent{TriggerEventTruncate},
					Body:   `FOR EACH ROW EXECUTE FUNCTION touch_user()`,
				})
				return err
			},
		},
		{
			name: "instead of statement body",
			err:  ErrInvalidTriggerBody,
			run: func() error {
				_, err := BuildCreateTriggerSQL(CreateTriggerOptions{
					Name:   "touch_user",
					Table:  TableName{Name: "users"},
					Timing: TriggerTimingInsteadOf,
					Events: []TriggerEvent{TriggerEventUpdate},
					Body:   `FOR EACH STATEMENT EXECUTE FUNCTION touch_user()`,
				})
				return err
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := tt.run(); !errors.Is(err, tt.err) {
				t.Fatalf("error = %v, want %v", err, tt.err)
			}
		})
	}
}

func TestBuildCreateExtensionSQLRejectsInvalidNames(t *testing.T) {
	for _, name := range []string{"", "1pgcrypto", "uuid-ossp-", `bad";drop`, "bad.schema"} {
		t.Run(name, func(t *testing.T) {
			if statement, err := BuildCreateExtensionSQL(name); !errors.Is(err, ErrInvalidSQLExtensionName) {
				t.Fatalf("BuildCreateExtensionSQL returned statement %q and error %v, want %v", statement, err, ErrInvalidSQLExtensionName)
			}
		})
	}
}
