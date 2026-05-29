package auth

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"sync"
	"time"

	"github.com/jackc/pgx/v5"

	"lazuli.dev/runtime/lazuli"
)

// SessionContractRegistry is the process-global list of session
// contracts the production middleware should consult when resolving
// an opaque session token. Codegen calls `RegisterSessionContract`
// from per-feature `auth.gen.go` init() blocks; the runtime walks
// the registered contracts until one returns a row.
//
// Today every Lazuli app has exactly one session contract (the
// `User` feature's `UserSession`), but the registry shape is plural
// so multi-actor apps (host portal + admin portal sharing one
// runtime, each with its own session resource) can opt in later.
var (
	sessionContractsMu sync.RWMutex
	sessionContracts   []SessionsContract
)

// RegisterSessionContract appends a SessionsContract to the
// production-resolver registry. Codegen calls this once per feature
// that declares `auth sessions`. Re-registering the same Resource
// replaces the prior entry so test harnesses can call this multiple
// times safely.
//
// Side effect: the first registration installs the runtime-side
// `lazuli.SessionResolver`, wiring the production session middleware.
func RegisterSessionContract(contract SessionsContract) {
	if contract.Resource == "" {
		return
	}
	sessionContractsMu.Lock()
	replaced := false
	for i := range sessionContracts {
		if sessionContracts[i].Resource == contract.Resource {
			sessionContracts[i] = contract
			replaced = true
			break
		}
	}
	if !replaced {
		sessionContracts = append(sessionContracts, contract)
	}
	first := !sessionResolverInstalled
	sessionResolverInstalled = true
	sessionContractsMu.Unlock()
	if first {
		lazuli.RegisterSessionResolver(&runtimeResolver{})
	}
}

var sessionResolverInstalled bool

// runtimeResolver implements `lazuli.SessionResolver` by walking the
// registered contracts and issuing one SELECT per resource until a
// row matches the hashed token. Stops at the first hit.
type runtimeResolver struct{}

func (runtimeResolver) Resolve(
	ctx context.Context,
	token string,
) (userID, orgID lazuli.ID, sessionID lazuli.ID, found bool, err error) {
	tokenHash, hashErr := HashSessionToken(token)
	if hashErr != nil {
		// Bad token shape — treat as not-found (anonymous) without
		// surfacing the malformed-token error to the boundary. The
		// resolver contract reserves `err` for genuine DB faults.
		return 0, 0, 0, false, nil
	}

	sessionContractsMu.RLock()
	contracts := append([]SessionsContract(nil), sessionContracts...)
	sessionContractsMu.RUnlock()
	if len(contracts) == 0 {
		return 0, 0, 0, false, nil
	}

	now := time.Now()
	for _, contract := range contracts {
		uid, oid, sid, ok, qErr := lookupSession(ctx, contract, tokenHash, now)
		if qErr != nil {
			slog.Warn("auth: session lookup error", "resource", contract.Resource, "error", qErr)
			return 0, 0, 0, false, qErr
		}
		if ok {
			return uid, oid, sid, true, nil
		}
	}
	return 0, 0, 0, false, nil
}

// lookupSession issues the SELECT for one contract. Returns
// `found=false` for `pgx.ErrNoRows` or an expired row. Other errors
// bubble up so the resolver can log them.
func lookupSession(
	ctx context.Context,
	contract SessionsContract,
	tokenHash string,
	now time.Time,
) (userID, orgID, sessionID lazuli.ID, found bool, err error) {
	table := sessionResourceTable(contract.Resource)
	if err := guardSessionIdent(contract.Resource); err != nil {
		return 0, 0, 0, false, err
	}

	// SELECT shape: id + user + (optional) org_id + expires_at +
	// revoked_at. The Lazuli emitter ships `org_id` on every
	// tenancy=org feature; for global features the column is absent
	// and we fall back to a four-column SELECT.
	//
	// We probe `org_id` opportunistically — try the 5-column SELECT
	// first; on a `column does not exist` PgError fall through to
	// the 4-column variant. This keeps the resolver source-of-truth
	// in one place without forcing every contract to declare its
	// columns explicitly.
	uid, oid, sid, expires, revoked, ok, err := selectWithOrg(ctx, table, tokenHash)
	if err != nil {
		if isUndefinedColumn(err) {
			uid, sid, expires, revoked, ok, err = selectWithoutOrg(ctx, table, tokenHash)
		}
	}
	if err != nil {
		return 0, 0, 0, false, err
	}
	if !ok {
		return 0, 0, 0, false, nil
	}
	if revoked != nil {
		return 0, 0, 0, false, nil
	}
	if !expires.After(now) {
		return 0, 0, 0, false, nil
	}
	return uid, oid, sid, true, nil
}

func selectWithOrg(
	ctx context.Context,
	table, tokenHash string,
) (userID, orgID, sessionID lazuli.ID, expiresAt time.Time, revokedAt *time.Time, ok bool, err error) {
	sql := fmt.Sprintf(
		`SELECT id, "user", org_id, expires_at, revoked_at FROM %s WHERE token_hash = $1 LIMIT 1`,
		quoteTableIdent(table),
	)
	row := sessionDBProvider().QueryRow(ctx, sql, tokenHash)
	err = row.Scan(&sessionID, &userID, &orgID, &expiresAt, &revokedAt)
	if errors.Is(err, pgx.ErrNoRows) {
		return 0, 0, 0, time.Time{}, nil, false, nil
	}
	if err != nil {
		return 0, 0, 0, time.Time{}, nil, false, err
	}
	return userID, orgID, sessionID, expiresAt, revokedAt, true, nil
}

func selectWithoutOrg(
	ctx context.Context,
	table, tokenHash string,
) (userID, sessionID lazuli.ID, expiresAt time.Time, revokedAt *time.Time, ok bool, err error) {
	sql := fmt.Sprintf(
		`SELECT id, "user", expires_at, revoked_at FROM %s WHERE token_hash = $1 LIMIT 1`,
		quoteTableIdent(table),
	)
	row := sessionDBProvider().QueryRow(ctx, sql, tokenHash)
	err = row.Scan(&sessionID, &userID, &expiresAt, &revokedAt)
	if errors.Is(err, pgx.ErrNoRows) {
		return 0, 0, time.Time{}, nil, false, nil
	}
	if err != nil {
		return 0, 0, time.Time{}, nil, false, err
	}
	return userID, sessionID, expiresAt, revokedAt, true, nil
}

// ResolveRoles fetches the actor's role for policy `@role.*` atom
// evaluation. Hardcodes the `"user"` table + `"role"` column for v1 —
// matches the canonical Lazuli auth identity shape where every app
// declares a `User` resource with a `role` field. Apps that name their
// identity differently can override by registering a custom resolver.
func (runtimeResolver) ResolveRoles(ctx context.Context, userID lazuli.ID) ([]string, error) {
	var role string
	err := sessionDBProvider().QueryRow(ctx, `SELECT COALESCE(role::text, '') FROM "user" WHERE id = $1`, int64(userID)).Scan(&role)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	if role == "" {
		return nil, nil
	}
	return []string{role}, nil
}

// quoteTableIdent quotes a table identifier for safe interpolation into
// a SQL string. It delegates to pgx's library-correct sanitizer (which
// double-quotes and escapes embedded double-quotes) rather than naively
// wrapping the raw name, matching `lazuli/retention.go`'s quoteIdentifier.
// Self-guarding: a name containing a double-quote is escaped, not passed
// through unaltered.
func quoteTableIdent(table string) string { return pgx.Identifier{table}.Sanitize() }

func guardSessionIdent(name string) error {
	for _, c := range name {
		ok := (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
			(c >= '0' && c <= '9') || c == '_'
		if !ok {
			return fmt.Errorf("auth: suspicious session resource %q", name)
		}
	}
	return nil
}

// isUndefinedColumn returns true when the pgx error is a "column does
// not exist" PgError (SQLSTATE 42703). Used by the resolver to fall
// back from the 5-column SELECT (with org_id) to the 4-column SELECT
// (without org_id) on `tenancy global` features.
func isUndefinedColumn(err error) bool {
	if err == nil {
		return false
	}
	type pgError interface {
		SQLState() string
	}
	var pe pgError
	if errors.As(err, &pe) {
		return pe.SQLState() == "42703"
	}
	return false
}
