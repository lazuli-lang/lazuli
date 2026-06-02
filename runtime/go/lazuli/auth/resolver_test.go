package auth

import (
	"bytes"
	"context"
	"database/sql"
	"log/slog"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"

	"lazuli.dev/runtime/lazuli"
)

// TestQuoteTableIdentEscapesEmbeddedQuote is a regression guard for the
// former naive `"` + table + `"` implementation, which would have
// emitted a syntactically broken / injection-prone identifier when the
// table name itself contained a double-quote. quoteTableIdent now
// delegates to pgx.Identifier.Sanitize(), so an embedded `"` MUST be
// doubled (the SQL escape), not passed through.
func TestQuoteTableIdentEscapesEmbeddedQuote(t *testing.T) {
	cases := []struct {
		name  string
		table string
	}{
		{name: "simple", table: "user_session"},
		{name: "single embedded quote", table: `ab"cd`},
		{name: "injection attempt", table: `x"; DROP TABLE users; --`},
		{name: "trailing quote", table: `tbl"`},
		{name: "only quote", table: `"`},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := quoteTableIdent(tc.table)

			// Must match the library-correct sanitizer exactly (wire,
			// not a reimplementation).
			want := pgx.Identifier{tc.table}.Sanitize()
			if got != want {
				t.Fatalf("quoteTableIdent(%q) = %q, want %q (pgx.Identifier.Sanitize)", tc.table, got, want)
			}

			// Result is always wrapped in double-quotes.
			if !strings.HasPrefix(got, `"`) || !strings.HasSuffix(got, `"`) {
				t.Fatalf("quoteTableIdent(%q) = %q, expected double-quoted identifier", tc.table, got)
			}

			// Crucially, the naive wrapper would have produced
			// `"` + table + `"` verbatim. Assert we do NOT do that
			// whenever the name carries a double-quote: every embedded
			// `"` in the raw name must appear doubled in the output, so
			// the naive form is strictly shorter and unequal.
			if strings.Contains(tc.table, `"`) {
				naive := `"` + tc.table + `"`
				if got == naive {
					t.Fatalf("quoteTableIdent(%q) = %q is the naive (unescaped) wrapping; embedded quote not sanitized", tc.table, got)
				}

				rawQuotes := strings.Count(tc.table, `"`)
				// Sanitize escapes each inner `"` as `""`, then adds the
				// two surrounding quotes: total quotes = rawQuotes*2 + 2.
				wantQuotes := rawQuotes*2 + 2
				if gotQuotes := strings.Count(got, `"`); gotQuotes != wantQuotes {
					t.Fatalf("quoteTableIdent(%q) = %q has %d double-quotes, want %d (each embedded quote doubled)", tc.table, got, gotQuotes, wantQuotes)
				}
			}
		})
	}
}

// --- resolver probe-ladder harness -------------------------------------
//
// resolverMockDB simulates a Postgres session table whose set of columns
// is configurable. A SELECT that references a column the table lacks is
// rejected with a 42703 (undefined_column) PgError-shaped error, exactly
// as Postgres would, so we can exercise the resolver's degrade ladder and
// its loud-on-required-missing path without a live database.

type fakePgError struct{ state string }

func (e fakePgError) Error() string    { return "ERROR: column does not exist (SQLSTATE " + e.state + ")" }
func (e fakePgError) SQLState() string { return e.state }

type resolverMockRow struct {
	dests   []func() any
	scanErr error
}

type resolverMockDB struct {
	// columns present in the simulated table.
	hasUser      bool
	hasTokenHash bool
	hasID        bool
	hasExpires   bool
	hasOrg       bool
	hasRevoked   bool

	// the single stored row (matched by token hash).
	tokenHash string
	sessionID lazuli.ID
	userID    lazuli.ID
	orgID     *int64 // nil ⇒ NULL org_id
	expiresAt time.Time
	revokedAt *time.Time
}

func (db *resolverMockDB) Exec(context.Context, string, ...any) (pgconn.CommandTag, error) {
	return pgconn.CommandTag{}, nil
}

// referencedColumns returns the bare column names the projection touches
// plus token_hash (always in the WHERE clause).
func selectMentions(sql, col string) bool {
	// match the column as a standalone token in the projection/predicate.
	return strings.Contains(sql, col)
}

func (db *resolverMockDB) QueryRow(_ context.Context, sql string, args ...any) pgx.Row {
	// token_hash is always required (WHERE clause).
	if !db.hasTokenHash && selectMentions(sql, "token_hash") {
		return resolverMockRow{scanErr: fakePgError{state: "42703"}}
	}
	if !db.hasID && selectMentions(sql, "id,") {
		return resolverMockRow{scanErr: fakePgError{state: "42703"}}
	}
	if !db.hasUser && selectMentions(sql, `"user"`) {
		return resolverMockRow{scanErr: fakePgError{state: "42703"}}
	}
	if !db.hasExpires && selectMentions(sql, "expires_at") {
		return resolverMockRow{scanErr: fakePgError{state: "42703"}}
	}
	if !db.hasOrg && selectMentions(sql, "org_id") {
		return resolverMockRow{scanErr: fakePgError{state: "42703"}}
	}
	if !db.hasRevoked && selectMentions(sql, "revoked_at") {
		return resolverMockRow{scanErr: fakePgError{state: "42703"}}
	}

	// All referenced columns exist. Match the stored row by token hash.
	wantHash, _ := args[0].(string)
	if wantHash != db.tokenHash {
		return resolverMockRow{scanErr: pgx.ErrNoRows}
	}

	// Build the value producers in projection order: id, "user",
	// [org_id], expires_at, [revoked_at].
	dests := []func() any{
		func() any { return db.sessionID },
		func() any { return db.userID },
	}
	if selectMentions(sql, "org_id") {
		dests = append(dests, func() any {
			if db.orgID == nil {
				return (*int64)(nil) // NULL
			}
			return *db.orgID
		})
	}
	dests = append(dests, func() any { return db.expiresAt })
	if selectMentions(sql, "revoked_at") {
		dests = append(dests, func() any { return db.revokedAt })
	}
	return resolverMockRow{dests: dests}
}

func (r resolverMockRow) Scan(out ...any) error {
	if r.scanErr != nil {
		return r.scanErr
	}
	if len(out) != len(r.dests) {
		return fakePgError{state: "00000"} // shape mismatch shouldn't happen
	}
	for i, d := range out {
		v := r.dests[i]()
		switch dst := d.(type) {
		case *lazuli.ID:
			*dst = v.(lazuli.ID)
		case *sql.NullInt64:
			if iv, ok := v.(int64); ok {
				*dst = sql.NullInt64{Int64: iv, Valid: true}
			} else {
				*dst = sql.NullInt64{Valid: false} // NULL org_id
			}
		case *time.Time:
			*dst = v.(time.Time)
		case **time.Time:
			*dst = v.(*time.Time)
		default:
			return fakePgError{state: "00001"}
		}
	}
	return nil
}

func withResolverMockDB(t *testing.T, db *resolverMockDB) {
	t.Helper()
	prev := sessionDBProvider
	sessionDBProvider = func() sessionDB { return db }
	t.Cleanup(func() { sessionDBProvider = prev })
}

func validResolverToken(t *testing.T) (token, hash string) {
	t.Helper()
	tok, h, err := newSessionToken()
	if err != nil {
		t.Fatalf("newSessionToken: %v", err)
	}
	return tok, h
}

// (a) A session table WITHOUT revoked_at resolves a valid token.
func TestResolverToleratesMissingRevokedAt(t *testing.T) {
	tok, h := validResolverToken(t)
	org := int64(7)
	db := &resolverMockDB{
		hasUser: true, hasTokenHash: true, hasID: true, hasExpires: true,
		hasOrg: true, hasRevoked: false, // no revoked_at column
		tokenHash: h, sessionID: 100, userID: 42, orgID: &org,
		expiresAt: time.Now().Add(time.Hour),
	}
	withResolverMockDB(t, db)

	uid, oid, sid, found, err := lookupSession(context.Background(),
		SessionsContract{Resource: "user_session"}, h, time.Now())
	if err != nil {
		t.Fatalf("lookupSession err = %v, want nil", err)
	}
	if !found {
		t.Fatalf("found = false, want true (missing revoked_at must still resolve)")
	}
	if uid != 42 || oid != 7 || sid != 100 {
		t.Fatalf("got uid=%d oid=%d sid=%d, want 42/7/100", uid, oid, sid)
	}
	_ = tok
}

// (b) A session row with NULL org_id resolves (org 0), no crash.
func TestResolverToleratesNullOrgID(t *testing.T) {
	_, h := validResolverToken(t)
	db := &resolverMockDB{
		hasUser: true, hasTokenHash: true, hasID: true, hasExpires: true,
		hasOrg: true, hasRevoked: true,
		tokenHash: h, sessionID: 9, userID: 5, orgID: nil, // NULL
		expiresAt: time.Now().Add(time.Hour),
	}
	withResolverMockDB(t, db)

	uid, oid, sid, found, err := lookupSession(context.Background(),
		SessionsContract{Resource: "user_session"}, h, time.Now())
	if err != nil {
		t.Fatalf("lookupSession err = %v, want nil (NULL org_id must not crash scan)", err)
	}
	if !found {
		t.Fatalf("found = false, want true")
	}
	if oid != 0 {
		t.Fatalf("oid = %d, want 0 for NULL org_id", oid)
	}
	if uid != 5 || sid != 9 {
		t.Fatalf("got uid=%d sid=%d, want 5/9", uid, sid)
	}
}

// (c) A session table missing a REQUIRED column (token_hash) logs a loud
// error and returns not-found.
func TestResolverLoudOnMissingRequiredColumn(t *testing.T) {
	_, h := validResolverToken(t)
	db := &resolverMockDB{
		hasUser: true, hasTokenHash: false, // REQUIRED column absent
		hasID: true, hasExpires: true, hasOrg: false, hasRevoked: false,
		tokenHash: h, sessionID: 1, userID: 1,
		expiresAt: time.Now().Add(time.Hour),
	}
	withResolverMockDB(t, db)

	// Reset the one-time warn dedupe for the table under test.
	brokenSessionTablesMu.Lock()
	delete(brokenSessionTables, "broken_session")
	brokenSessionTablesMu.Unlock()

	var buf bytes.Buffer
	prevLogger := slog.Default()
	slog.SetDefault(slog.New(slog.NewTextHandler(&buf, &slog.HandlerOptions{Level: slog.LevelError})))
	t.Cleanup(func() { slog.SetDefault(prevLogger) })

	_, _, _, found, err := lookupSession(context.Background(),
		SessionsContract{Resource: "BrokenSession"}, h, time.Now())
	if err != nil {
		t.Fatalf("lookupSession err = %v, want nil (loud-but-not-fatal)", err)
	}
	if found {
		t.Fatalf("found = true, want false for a genuinely broken table")
	}
	logged := buf.String()
	if !strings.Contains(logged, "REQUIRED column") {
		t.Fatalf("expected loud error mentioning REQUIRED column, got: %q", logged)
	}
	if !strings.Contains(logged, "broken_session") {
		t.Fatalf("expected loud error to name the table broken_session, got: %q", logged)
	}
}

// (d) A present non-NULL revoked_at still invalidates the session.
func TestResolverRevokedAtInvalidates(t *testing.T) {
	_, h := validResolverToken(t)
	revoked := time.Now().Add(-time.Minute)
	db := &resolverMockDB{
		hasUser: true, hasTokenHash: true, hasID: true, hasExpires: true,
		hasOrg: true, hasRevoked: true,
		tokenHash: h, sessionID: 3, userID: 8,
		expiresAt: time.Now().Add(time.Hour),
		revokedAt: &revoked, // non-NULL ⇒ invalid
	}
	withResolverMockDB(t, db)

	_, _, _, found, err := lookupSession(context.Background(),
		SessionsContract{Resource: "user_session"}, h, time.Now())
	if err != nil {
		t.Fatalf("lookupSession err = %v, want nil", err)
	}
	if found {
		t.Fatalf("found = true, want false for a revoked session")
	}
}

// (e) expires_at is enforced in EVERY probe variant — including the
// minimal (no org_id, no revoked_at) shape. An expired row in a
// revoked_at-less table must NOT resolve.
func TestResolverExpiryEnforcedInMinimalProbe(t *testing.T) {
	_, h := validResolverToken(t)
	db := &resolverMockDB{
		hasUser: true, hasTokenHash: true, hasID: true, hasExpires: true,
		hasOrg: false, hasRevoked: false, // minimal shape
		tokenHash: h, sessionID: 2, userID: 4,
		expiresAt: time.Now().Add(-time.Hour), // already expired
	}
	withResolverMockDB(t, db)

	_, _, _, found, err := lookupSession(context.Background(),
		SessionsContract{Resource: "user_session"}, h, time.Now())
	if err != nil {
		t.Fatalf("lookupSession err = %v, want nil", err)
	}
	if found {
		t.Fatalf("found = true, want false (expired session must not resolve even without revoked_at)")
	}
}
