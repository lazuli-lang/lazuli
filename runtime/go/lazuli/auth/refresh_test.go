package auth

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"

	"lazuli.dev/runtime/lazuli"
)

type refreshDB struct {
	rows map[lazuli.ID]*refreshDBRow
	next lazuli.ID
}

type refreshDBRow struct {
	id, user, parent       lazuli.ID
	tokenHash, refreshHash string
	expires, refreshExp    time.Time
	created                time.Time
	revoked, theft         *time.Time
}

func newRefreshDB(t *testing.T, now time.Time) (*refreshDB, *lazuli.Ctx, string) {
	t.Helper()
	db := &refreshDB{rows: map[lazuli.ID]*refreshDBRow{}, next: 2}
	prevDB, prevRefresh, prevSessions, prevInstalled := sessionDBProvider, refreshContracts, sessionContracts, sessionResolverInstalled
	sessionDBProvider = func() sessionDB { return db }
	refreshContracts, sessionContracts, sessionResolverInstalled = nil, nil, false
	t.Cleanup(func() {
		sessionDBProvider, refreshContracts, sessionContracts, sessionResolverInstalled = prevDB, prevRefresh, prevSessions, prevInstalled
	})
	_, accessHash, err := newSessionToken()
	if err != nil {
		t.Fatal(err)
	}
	refresh, refreshHash, err := newSessionToken()
	if err != nil {
		t.Fatal(err)
	}
	db.rows[1] = &refreshDBRow{id: 1, user: 42, tokenHash: accessHash, refreshHash: refreshHash, expires: now.Add(time.Hour), refreshExp: now.Add(24 * time.Hour), created: now}
	RegisterRefreshContract(SessionsContract{Resource: "session", AccessTTL: time.Minute, RefreshTTL: 24 * time.Hour, RotationGrace: 30 * time.Second})
	return db, &lazuli.Ctx{Context: context.Background(), Now: now}, refresh
}

func (db *refreshDB) QueryRow(_ context.Context, sql string, args ...any) pgx.Row {
	switch {
	case strings.Contains(sql, "org_id"):
		return refreshRowResult{err: &pgconn.PgError{Code: "42703", Message: "column does not exist"}}
	case strings.HasPrefix(sql, `SELECT id, "user", refresh_expires_at`):
		hash := args[0].(string)
		for _, r := range db.rows {
			if r.refreshHash == hash {
				return refreshRowResult{row: r}
			}
		}
		return refreshRowResult{err: pgx.ErrNoRows}
	case strings.HasPrefix(sql, `WITH marked AS`):
		parentID, now := args[5].(lazuli.ID), args[6].(time.Time)
		parent := db.rows[parentID]
		if parent == nil || parent.revoked != nil {
			return refreshRowResult{err: pgx.ErrNoRows}
		}
		parent.revoked = &now
		return refreshRowResult{id: db.insert(args[0].(lazuli.ID), args[1].(string), args[2].(string), args[4].(time.Time), args[3].(time.Time), now, parentID)}
	case strings.HasPrefix(sql, `INSERT INTO "session"`):
		return refreshRowResult{id: db.insert(args[0].(lazuli.ID), args[1].(string), args[2].(string), args[4].(time.Time), args[3].(time.Time), args[5].(time.Time), args[6].(lazuli.ID))}
	default:
		return refreshRowResult{err: fmt.Errorf("unexpected query SQL: %s", sql)}
	}
}

func (db *refreshDB) Exec(_ context.Context, sql string, args ...any) (pgconn.CommandTag, error) {
	now := args[0].(time.Time)
	switch {
	case strings.HasPrefix(sql, `WITH RECURSIVE family`):
		db.revokeFamily(args[1].(lazuli.ID), now)
	case strings.Contains(sql, `WHERE "user"=$2`):
		user := args[1].(lazuli.ID)
		for _, r := range db.rows {
			if r.user == user {
				r.revoked, r.theft = &now, &now
			}
		}
	default:
		return pgconn.CommandTag{}, fmt.Errorf("unexpected exec SQL: %s", sql)
	}
	return pgconn.NewCommandTag("UPDATE 1"), nil
}

func (db *refreshDB) insert(user lazuli.ID, accessHash, refreshHash string, accessExp, refreshExp, created time.Time, parent lazuli.ID) lazuli.ID {
	id := db.next
	db.next++
	db.rows[id] = &refreshDBRow{id: id, user: user, parent: parent, tokenHash: accessHash, refreshHash: refreshHash, expires: accessExp, refreshExp: refreshExp, created: created}
	return id
}

func (db *refreshDB) revokeFamily(id lazuli.ID, now time.Time) {
	seen := map[lazuli.ID]bool{}
	var walk func(lazuli.ID)
	walk = func(cur lazuli.ID) {
		if seen[cur] || db.rows[cur] == nil {
			return
		}
		seen[cur] = true
		if p := db.rows[cur].parent; p != 0 {
			walk(p)
		}
		for _, r := range db.rows {
			if r.parent == cur {
				walk(r.id)
			}
		}
	}
	walk(id)
	for id := range seen {
		db.rows[id].revoked, db.rows[id].theft = &now, &now
	}
}

type refreshRowResult struct {
	row *refreshDBRow
	id  lazuli.ID
	err error
}

func (r refreshRowResult) Scan(dest ...any) error {
	if r.err != nil {
		return r.err
	}
	if len(dest) == 1 {
		*dest[0].(*lazuli.ID) = r.id
		return nil
	}
	*(dest[0].(*lazuli.ID)) = r.row.id
	*(dest[1].(*lazuli.ID)) = r.row.user
	*(dest[2].(*time.Time)) = r.row.refreshExp
	*(dest[3].(**time.Time)) = r.row.revoked
	return nil
}

func assertCode(t *testing.T, err error, code string) {
	t.Helper()
	var le *lazuli.Error
	if !errors.As(err, &le) || le.Code != code {
		t.Fatalf("error code = %v (%T), want %s", err, err, code)
	}
}

func TestRotateSessionHappy(t *testing.T) {
	now := time.Date(2026, 5, 19, 12, 0, 0, 0, time.UTC)
	db, ctx, refresh := newRefreshDB(t, now)
	access, nextRefresh, err := RotateSession(ctx, refresh)
	if err != nil {
		t.Fatalf("RotateSession: %v", err)
	}
	if access == "" || nextRefresh == "" || db.rows[1].revoked == nil || db.rows[2].parent != 1 {
		t.Fatalf("rotation did not revoke parent and insert child: %#v", db.rows)
	}
}

func TestRotateSessionRevokedWithinGrace(t *testing.T) {
	now := time.Date(2026, 5, 19, 12, 0, 0, 0, time.UTC)
	db, ctx, refresh := newRefreshDB(t, now)
	revoked := now.Add(-5 * time.Second)
	db.rows[1].revoked = &revoked
	if _, _, err := RotateSession(ctx, refresh); err != nil {
		t.Fatalf("RotateSession within grace: %v", err)
	}
	if db.rows[2].parent != 1 {
		t.Fatalf("within-grace refresh did not create child")
	}
}

func TestRotateSessionRevokedPastGraceRevokesFamily(t *testing.T) {
	now := time.Date(2026, 5, 19, 12, 0, 0, 0, time.UTC)
	db, ctx, refresh := newRefreshDB(t, now)
	revoked := now.Add(-time.Minute)
	db.rows[1].revoked = &revoked
	db.insert(42, "a", "b", now.Add(time.Minute), now.Add(time.Hour), now, 1)
	_, _, err := RotateSession(ctx, refresh)
	assertCode(t, err, lazuli.CodeRefreshRevoked)
	if db.rows[1].theft == nil || db.rows[2].theft == nil {
		t.Fatalf("family theft markers missing: %#v", db.rows)
	}
}

func TestRotateSessionRefreshExpired(t *testing.T) {
	now := time.Date(2026, 5, 19, 12, 0, 0, 0, time.UTC)
	db, ctx, refresh := newRefreshDB(t, now)
	db.rows[1].refreshExp = now.Add(-time.Second)
	_, _, err := RotateSession(ctx, refresh)
	assertCode(t, err, lazuli.CodeRefreshInvalid)
}

func TestRotateSessionRefreshNotFound(t *testing.T) {
	now := time.Date(2026, 5, 19, 12, 0, 0, 0, time.UTC)
	_, ctx, _ := newRefreshDB(t, now)
	missing, _, _ := newSessionToken()
	_, _, err := RotateSession(ctx, missing)
	assertCode(t, err, lazuli.CodeRefreshInvalid)
}

func TestRotateSessionSecondRotationChainLengthTwo(t *testing.T) {
	now := time.Date(2026, 5, 19, 12, 0, 0, 0, time.UTC)
	db, ctx, refresh := newRefreshDB(t, now)
	_, refresh2, err := RotateSession(ctx, refresh)
	if err != nil {
		t.Fatalf("first rotation: %v", err)
	}
	ctx.Now = now.Add(time.Second)
	_, _, err = RotateSession(ctx, refresh2)
	if err != nil {
		t.Fatalf("second rotation: %v", err)
	}
	if db.rows[2].parent != 1 || db.rows[3].parent != 2 || db.rows[2].revoked == nil {
		t.Fatalf("chain mismatch: %#v", db.rows)
	}
}
