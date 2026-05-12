package lazuli

import (
	"errors"
	"testing"
)

func TestBuildDBPessimisticLockClauseBuildsModes(t *testing.T) {
	tests := []struct {
		name string
		opts DBPessimisticLockOptions
		want string
	}{
		{
			name: "none",
			opts: DBPessimisticLockOptions{},
			want: "",
		},
		{
			name: "update",
			opts: DBPessimisticLockOptions{Mode: DBPessimisticLockUpdate},
			want: "FOR UPDATE",
		},
		{
			name: "share",
			opts: DBPessimisticLockOptions{Mode: DBPessimisticLockShare},
			want: "FOR SHARE",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := BuildDBPessimisticLockClause(tt.opts)
			if err != nil {
				t.Fatalf("BuildDBPessimisticLockClause returned error: %v", err)
			}
			if got != tt.want {
				t.Fatalf("clause = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestBuildDBPessimisticLockClauseBuildsOptions(t *testing.T) {
	tests := []struct {
		name string
		opts DBPessimisticLockOptions
		want string
	}{
		{
			name: "of and nowait",
			opts: DBPessimisticLockOptions{
				Mode:   DBPessimisticLockUpdate,
				Of:     []string{"accounts", "tenant_1.OrderEvents"},
				NoWait: true,
			},
			want: `FOR UPDATE OF "accounts", "tenant_1"."OrderEvents" NOWAIT`,
		},
		{
			name: "skip locked",
			opts: DBPessimisticLockOptions{
				Mode:       DBPessimisticLockShare,
				SkipLocked: true,
			},
			want: "FOR SHARE SKIP LOCKED",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := BuildDBPessimisticLockClause(tt.opts)
			if err != nil {
				t.Fatalf("BuildDBPessimisticLockClause returned error: %v", err)
			}
			if got != tt.want {
				t.Fatalf("clause = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestAppendDBPessimisticLockClausePlacesClauseAfterOrderAndLimit(t *testing.T) {
	sql := `SELECT * FROM "jobs" WHERE status = $1 ORDER BY priority DESC, id ASC LIMIT 10`

	got, err := AppendDBPessimisticLockClause(sql, DBPessimisticLockOptions{
		Mode:       DBPessimisticLockUpdate,
		Of:         []string{"jobs"},
		SkipLocked: true,
	})
	if err != nil {
		t.Fatalf("AppendDBPessimisticLockClause returned error: %v", err)
	}

	want := `SELECT * FROM "jobs" WHERE status = $1 ORDER BY priority DESC, id ASC LIMIT 10 FOR UPDATE OF "jobs" SKIP LOCKED`
	if got != want {
		t.Fatalf("SQL = %q, want %q", got, want)
	}
}

func TestAppendDBPessimisticLockClauseIgnoresLockWordsInsideLiterals(t *testing.T) {
	sql := `SELECT * FROM accounts WHERE note = 'FOR UPDATE'`

	got, err := AppendDBPessimisticLockClause(sql, DBPessimisticLockOptions{
		Mode:   DBPessimisticLockShare,
		NoWait: true,
	})
	if err != nil {
		t.Fatalf("AppendDBPessimisticLockClause returned error: %v", err)
	}

	want := `SELECT * FROM accounts WHERE note = 'FOR UPDATE' FOR SHARE NOWAIT`
	if got != want {
		t.Fatalf("SQL = %q, want %q", got, want)
	}
}

func TestAppendDBPessimisticLockClauseReturnsTrimmedSQLForNoLock(t *testing.T) {
	got, err := AppendDBPessimisticLockClause("  SELECT * FROM accounts  ", DBPessimisticLockOptions{})
	if err != nil {
		t.Fatalf("AppendDBPessimisticLockClause returned error: %v", err)
	}
	if got != "SELECT * FROM accounts" {
		t.Fatalf("SQL = %q, want trimmed SELECT", got)
	}
}

func TestBuildDBPessimisticLockClauseRejectsInvalidModesAndOptions(t *testing.T) {
	tests := []struct {
		name    string
		opts    DBPessimisticLockOptions
		wantErr error
	}{
		{
			name:    "invalid mode",
			opts:    DBPessimisticLockOptions{Mode: DBPessimisticLockMode(99)},
			wantErr: errInvalidDBPessimisticLockMode,
		},
		{
			name: "nowait with no lock",
			opts: DBPessimisticLockOptions{
				NoWait: true,
			},
			wantErr: errInvalidDBPessimisticLockOptions,
		},
		{
			name: "of with no lock",
			opts: DBPessimisticLockOptions{
				Of: []string{"accounts"},
			},
			wantErr: errInvalidDBPessimisticLockOptions,
		},
		{
			name: "conflicting wait options",
			opts: DBPessimisticLockOptions{
				Mode:       DBPessimisticLockUpdate,
				NoWait:     true,
				SkipLocked: true,
			},
			wantErr: errInvalidDBPessimisticLockOptions,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if clause, err := BuildDBPessimisticLockClause(tt.opts); !errors.Is(err, tt.wantErr) {
				t.Fatalf("BuildDBPessimisticLockClause = %q, %v; want error %v", clause, err, tt.wantErr)
			}
		})
	}
}

func TestBuildDBPessimisticLockClauseRejectsInvalidIdentifiers(t *testing.T) {
	invalidIdentifiers := []string{
		"",
		" accounts",
		"accounts ",
		"1accounts",
		"tenant.1accounts",
		"tenant..accounts",
		"tenant.accounts.extra",
		"accounts;drop",
		`accounts"`,
		"accounts-name",
		"accounts name",
		"accounts\nname",
		"contasé",
	}

	for _, identifier := range invalidIdentifiers {
		t.Run(identifier, func(t *testing.T) {
			_, err := BuildDBPessimisticLockClause(DBPessimisticLockOptions{
				Mode: DBPessimisticLockUpdate,
				Of:   []string{"accounts", identifier},
			})
			if !errors.Is(err, errInvalidDBPessimisticLockIdentifier) {
				t.Fatalf("BuildDBPessimisticLockClause error = %v, want invalid identifier", err)
			}
		})
	}
}

func TestBuildDBPessimisticLockClauseAllowsStrictIdentifiers(t *testing.T) {
	got, err := BuildDBPessimisticLockClause(DBPessimisticLockOptions{
		Mode: DBPessimisticLockUpdate,
		Of:   []string{"_", "_events", "Users_2", "app_1._jobs"},
	})
	if err != nil {
		t.Fatalf("BuildDBPessimisticLockClause returned error: %v", err)
	}

	want := `FOR UPDATE OF "_", "_events", "Users_2", "app_1"."_jobs"`
	if got != want {
		t.Fatalf("clause = %q, want %q", got, want)
	}
}

func TestAppendDBPessimisticLockClauseRejectsInvalidSQLOrder(t *testing.T) {
	invalidSQL := []string{
		"",
		"UPDATE accounts SET locked = true",
		"SELECT * FROM accounts;",
		"SELECT * FROM accounts FOR UPDATE",
		"SELECT * FROM accounts FOR SHARE NOWAIT",
		"SELECT * FROM accounts FOR NO KEY UPDATE",
		"SELECT * FROM accounts FOR KEY SHARE",
		"SELECT * FROM accounts NOWAIT",
		"SELECT * FROM accounts SKIP LOCKED",
	}

	for _, sql := range invalidSQL {
		t.Run(sql, func(t *testing.T) {
			_, err := AppendDBPessimisticLockClause(sql, DBPessimisticLockOptions{
				Mode: DBPessimisticLockUpdate,
			})
			if !errors.Is(err, errInvalidDBPessimisticLockOrder) {
				t.Fatalf("AppendDBPessimisticLockClause error = %v, want invalid order", err)
			}
		})
	}
}
