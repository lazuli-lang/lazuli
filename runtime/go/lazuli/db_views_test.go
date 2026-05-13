package lazuli

import (
	"errors"
	"reflect"
	"testing"
)

func TestBuildDBCreateViewSQLBuildsRegularView(t *testing.T) {
	got, err := BuildDBCreateViewSQL(DBViewCreateOptions{
		Name:        "app.active_users",
		Columns:     []string{"id", "email"},
		Query:       ` SELECT id, email FROM "app"."users" WHERE deleted_at IS NULL `,
		OrReplace:   true,
		CheckOption: DBViewLocalCheckOption,
	})
	if err != nil {
		t.Fatalf("BuildDBCreateViewSQL returned error: %v", err)
	}

	want := `CREATE OR REPLACE VIEW "app"."active_users" ("id", "email") AS SELECT id, email FROM "app"."users" WHERE deleted_at IS NULL WITH LOCAL CHECK OPTION`
	if got != want {
		t.Fatalf("SQL = %q, want %q", got, want)
	}
}

func TestBuildDBCreateViewSQLBuildsMaterializedView(t *testing.T) {
	got, err := BuildDBCreateViewSQL(DBViewCreateOptions{
		Name:        "reports.daily_sales",
		Kind:        DBViewMaterialized,
		Columns:     []string{"day", "total"},
		Query:       "WITH sales AS (SELECT created_at::date AS day, total FROM orders) SELECT day, sum(total) AS total FROM sales GROUP BY day",
		IfNotExists: true,
		WithNoData:  true,
	})
	if err != nil {
		t.Fatalf("BuildDBCreateViewSQL returned error: %v", err)
	}

	want := `CREATE MATERIALIZED VIEW IF NOT EXISTS "reports"."daily_sales" ("day", "total") AS WITH sales AS (SELECT created_at::date AS day, total FROM orders) SELECT day, sum(total) AS total FROM sales GROUP BY day WITH NO DATA`
	if got != want {
		t.Fatalf("SQL = %q, want %q", got, want)
	}
}

func TestBuildDBRefreshMaterializedViewSQLBuildsPolicies(t *testing.T) {
	tests := []struct {
		name string
		opts DBMaterializedViewRefreshOptions
		want string
	}{
		{
			name: "default",
			want: `REFRESH MATERIALIZED VIEW "reports"."daily_sales"`,
		},
		{
			name: "concurrently with data",
			opts: DBMaterializedViewRefreshOptions{
				Concurrently: true,
				Policy:       DBViewRefreshWithData,
			},
			want: `REFRESH MATERIALIZED VIEW CONCURRENTLY "reports"."daily_sales" WITH DATA`,
		},
		{
			name: "with no data",
			opts: DBMaterializedViewRefreshOptions{
				Policy: DBViewRefreshWithNoData,
			},
			want: `REFRESH MATERIALIZED VIEW "reports"."daily_sales" WITH NO DATA`,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := BuildDBRefreshMaterializedViewSQL("reports.daily_sales", tt.opts)
			if err != nil {
				t.Fatalf("BuildDBRefreshMaterializedViewSQL returned error: %v", err)
			}
			if got != tt.want {
				t.Fatalf("SQL = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestBuildDBDropViewSQLBuildsDropStatements(t *testing.T) {
	tests := []struct {
		name string
		view string
		opts DBViewDropOptions
		want string
	}{
		{
			name: "regular cascade",
			view: "app.active_users",
			opts: DBViewDropOptions{
				IfExists: true,
				Behavior: DBViewDropCascade,
			},
			want: `DROP VIEW IF EXISTS "app"."active_users" CASCADE`,
		},
		{
			name: "materialized restrict",
			view: "reports.daily_sales",
			opts: DBViewDropOptions{
				Kind:     DBViewMaterialized,
				Behavior: DBViewDropRestrict,
			},
			want: `DROP MATERIALIZED VIEW "reports"."daily_sales" RESTRICT`,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := BuildDBDropViewSQL(tt.view, tt.opts)
			if err != nil {
				t.Fatalf("BuildDBDropViewSQL returned error: %v", err)
			}
			if got != tt.want {
				t.Fatalf("SQL = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestBuildDBViewDependencyNotes(t *testing.T) {
	refresh := DBMaterializedViewRefreshOptions{
		Concurrently: true,
		Policy:       DBViewRefreshWithData,
	}
	notes, err := BuildDBViewDependencyNotes(DBViewDependencyNotesOptions{
		View:         "reports.daily_sales",
		Kind:         DBViewMaterialized,
		Dependencies: []string{"public.orders", "customers"},
		Refresh:      &refresh,
	})
	if err != nil {
		t.Fatalf("BuildDBViewDependencyNotes returned error: %v", err)
	}

	want := []string{
		`-- lazuli: materialized view "reports"."daily_sales" depends on "public"."orders", "customers"`,
		`-- lazuli: materialized view "reports"."daily_sales" refresh concurrently with data`,
	}
	if !reflect.DeepEqual(notes, want) {
		t.Fatalf("notes = %#v, want %#v", notes, want)
	}
}

func TestBuildDBCreateViewSQLRejectsInvalidInput(t *testing.T) {
	tests := []struct {
		name    string
		opts    DBViewCreateOptions
		wantErr error
	}{
		{
			name: "invalid view name",
			opts: DBViewCreateOptions{
				Name:  "reports.daily.sales",
				Query: "SELECT 1",
			},
			wantErr: errInvalidDBViewIdentifier,
		},
		{
			name: "invalid column",
			opts: DBViewCreateOptions{
				Name:    "active_users",
				Columns: []string{"email-address"},
				Query:   "SELECT email FROM users",
			},
			wantErr: errInvalidDBViewIdentifier,
		},
		{
			name: "empty query",
			opts: DBViewCreateOptions{
				Name: "active_users",
			},
			wantErr: errInvalidDBViewQuery,
		},
		{
			name: "query with terminator",
			opts: DBViewCreateOptions{
				Name:  "active_users",
				Query: "SELECT * FROM users; DROP TABLE users",
			},
			wantErr: errInvalidDBViewQuery,
		},
		{
			name: "query must be select shaped",
			opts: DBViewCreateOptions{
				Name:  "active_users",
				Query: "UPDATE users SET active = true",
			},
			wantErr: errInvalidDBViewQuery,
		},
		{
			name: "if not exists on regular view",
			opts: DBViewCreateOptions{
				Name:        "active_users",
				Query:       "SELECT * FROM users",
				IfNotExists: true,
			},
			wantErr: errInvalidDBViewOptions,
		},
		{
			name: "or replace on materialized view",
			opts: DBViewCreateOptions{
				Name:      "daily_sales",
				Kind:      DBViewMaterialized,
				Query:     "SELECT * FROM sales",
				OrReplace: true,
			},
			wantErr: errInvalidDBViewOptions,
		},
		{
			name: "check option on materialized view",
			opts: DBViewCreateOptions{
				Name:        "daily_sales",
				Kind:        DBViewMaterialized,
				Query:       "SELECT * FROM sales",
				CheckOption: DBViewCascadedCheckOption,
			},
			wantErr: errInvalidDBViewOptions,
		},
		{
			name: "invalid check option",
			opts: DBViewCreateOptions{
				Name:        "active_users",
				Query:       "SELECT * FROM users",
				CheckOption: DBViewCheckOption(99),
			},
			wantErr: errInvalidDBViewOptions,
		},
		{
			name: "invalid kind",
			opts: DBViewCreateOptions{
				Name:  "active_users",
				Kind:  DBViewKind(99),
				Query: "SELECT * FROM users",
			},
			wantErr: errInvalidDBViewOptions,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if sql, err := BuildDBCreateViewSQL(tt.opts); !errors.Is(err, tt.wantErr) {
				t.Fatalf("BuildDBCreateViewSQL = %q, %v; want error %v", sql, err, tt.wantErr)
			}
		})
	}
}

func TestDBViewHelpersRejectInvalidRefreshDropAndNotes(t *testing.T) {
	tests := []struct {
		name    string
		run     func() error
		wantErr error
	}{
		{
			name: "refresh invalid identifier",
			run: func() error {
				_, err := BuildDBRefreshMaterializedViewSQL("reports.daily-sales", DBMaterializedViewRefreshOptions{})
				return err
			},
			wantErr: errInvalidDBViewIdentifier,
		},
		{
			name: "refresh invalid policy",
			run: func() error {
				_, err := BuildDBRefreshMaterializedViewSQL("reports.daily_sales", DBMaterializedViewRefreshOptions{
					Policy: DBViewRefreshPolicy(99),
				})
				return err
			},
			wantErr: errInvalidDBViewRefreshPolicy,
		},
		{
			name: "refresh concurrently with no data",
			run: func() error {
				_, err := BuildDBRefreshMaterializedViewSQL("reports.daily_sales", DBMaterializedViewRefreshOptions{
					Concurrently: true,
					Policy:       DBViewRefreshWithNoData,
				})
				return err
			},
			wantErr: errInvalidDBViewRefreshPolicy,
		},
		{
			name: "drop invalid behavior",
			run: func() error {
				_, err := BuildDBDropViewSQL("active_users", DBViewDropOptions{
					Behavior: DBViewDropBehavior("force"),
				})
				return err
			},
			wantErr: errInvalidDBViewDropBehavior,
		},
		{
			name: "drop invalid identifier",
			run: func() error {
				_, err := BuildDBDropViewSQL("1active_users", DBViewDropOptions{})
				return err
			},
			wantErr: errInvalidDBViewIdentifier,
		},
		{
			name: "dependency invalid identifier",
			run: func() error {
				_, err := BuildDBViewDependencyNotes(DBViewDependencyNotesOptions{
					View:         "daily_sales",
					Dependencies: []string{"public.orders", "public.bad-name"},
				})
				return err
			},
			wantErr: errInvalidDBViewIdentifier,
		},
		{
			name: "refresh note on regular view",
			run: func() error {
				refresh := DBMaterializedViewRefreshOptions{Policy: DBViewRefreshWithData}
				_, err := BuildDBViewDependencyNotes(DBViewDependencyNotesOptions{
					View:    "active_users",
					Refresh: &refresh,
				})
				return err
			},
			wantErr: errInvalidDBViewOptions,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := tt.run(); !errors.Is(err, tt.wantErr) {
				t.Fatalf("error = %v, want %v", err, tt.wantErr)
			}
		})
	}
}
