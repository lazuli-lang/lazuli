package auth

import (
	"context"
	"database/sql"
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

// sessionProbe describes one shape of the session SELECT. The resolver
// walks a ladder of probes from richest (org_id + revoked_at) to poorest
// (no org_id, no revoked_at), degrading past each OPTIONAL column that a
// pilot's hand-declared session table happens to omit. expires_at and the
// WHERE-clause `token_hash` + the `"user"` / `id` projection are REQUIRED
// in every variant; only org_id and revoked_at are optional.
type sessionProbe struct {
	hasOrg     bool
	hasRevoked bool
}

// columns returns the SELECT projection for this probe shape.
func (p sessionProbe) columns() string {
	switch {
	case p.hasOrg && p.hasRevoked:
		return `id, "user", org_id, expires_at, revoked_at`
	case p.hasOrg && !p.hasRevoked:
		return `id, "user", org_id, expires_at`
	case !p.hasOrg && p.hasRevoked:
		return `id, "user", expires_at, revoked_at`
	default:
		return `id, "user", expires_at`
	}
}

// sessionProbeLadder is the ordered degrade path. We try the widest shape
// first and fall to the next on a 42703 (undefined column) error, so the
// resolver accepts the widest set of session-table shapes a pilot may
// hand-declare without forcing every contract to enumerate its columns.
var sessionProbeLadder = []sessionProbe{
	{hasOrg: true, hasRevoked: true},   // canonical org tenancy emit
	{hasOrg: false, hasRevoked: true},  // global tenancy (no org_id)
	{hasOrg: true, hasRevoked: false},  // org table without revoked_at
	{hasOrg: false, hasRevoked: false}, // minimal: id + user + expires_at
}

// lookupSession issues the SELECT for one contract. Returns
// `found=false` for `pgx.ErrNoRows` or an expired/revoked row. Other
// errors bubble up so the resolver can log them.
//
// Robustness contract (hardened for hand-declared pilot session tables):
//   - A missing OPTIONAL column (org_id and/or revoked_at) is tolerated:
//     the probe ladder degrades to a narrower SELECT.
//   - A NULL org_id scans into a nullable dest and is treated as org 0
//     (no tenant) — it never crashes the scan.
//   - A row with no revoked_at column is treated as never-revoked; when
//     the column IS present, a non-NULL revoked_at still invalidates.
//   - expires_at is REQUIRED and enforced in EVERY probe variant.
//   - If EVERY probe fails with undefined-column (a genuinely wrong table
//     missing a REQUIRED column such as token_hash or "user"), we emit a
//     LOUD one-time error naming the table instead of silently returning
//     anonymous, and return not-found.
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

	var lastUndefined error
	for _, probe := range sessionProbeLadder {
		uid, oid, sid, expires, revoked, ok, qErr := selectSession(ctx, table, tokenHash, probe)
		if qErr != nil {
			if isUndefinedColumn(qErr) {
				// This shape references an OPTIONAL column the table
				// lacks — degrade to the next, narrower probe.
				lastUndefined = qErr
				continue
			}
			// A genuine DB fault (connection, permission, syntax on a
			// REQUIRED column) — bubble up so the resolver logs it.
			return 0, 0, 0, false, qErr
		}
		if !ok {
			// Probe succeeded (table shape matched) but no row had this
			// token_hash → not found. Stop; further probes would race
			// against the same missing row.
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

	// Every probe shape errored with undefined-column. The optional
	// columns can't ALL be the cause (the poorest probe references only
	// id, "user", expires_at, token_hash) — so a REQUIRED column is
	// missing and this session table is genuinely misconfigured. Make it
	// LOUD (one-time per table) rather than silently authenticating no
	// one → opaque 403s.
	warnBrokenSessionTable(table, lastUndefined)
	return 0, 0, 0, false, nil
}

// selectSession runs a single probe shape. org_id is scanned into a
// nullable dest so a NULL tenant column never crashes the scan; a
// NULL/absent org resolves to 0 (no tenant). revoked_at is scanned into a
// *time.Time only when the probe carries it; absent ⇒ never-revoked.
func selectSession(
	ctx context.Context,
	table, tokenHash string,
	probe sessionProbe,
) (userID, orgID, sessionID lazuli.ID, expiresAt time.Time, revokedAt *time.Time, ok bool, err error) {
	query := fmt.Sprintf(
		`SELECT %s FROM %s WHERE token_hash = $1 LIMIT 1`,
		probe.columns(),
		quoteTableIdent(table),
	)

	var org sql.NullInt64
	var revoked *time.Time

	// Build the scan dests in the same order as columns().
	dests := []any{&sessionID, &userID}
	if probe.hasOrg {
		dests = append(dests, &org)
	}
	dests = append(dests, &expiresAt)
	if probe.hasRevoked {
		dests = append(dests, &revoked)
	}

	row := sessionDBProvider().QueryRow(ctx, query, tokenHash)
	err = row.Scan(dests...)
	if errors.Is(err, pgx.ErrNoRows) {
		return 0, 0, 0, time.Time{}, nil, false, nil
	}
	if err != nil {
		return 0, 0, 0, time.Time{}, nil, false, err
	}
	if org.Valid {
		orgID = lazuli.ID(org.Int64)
	}
	return userID, orgID, sessionID, expiresAt, revoked, true, nil
}

// brokenSessionTables records tables we've already warned about so the
// loud error fires at most once per table even under a request storm.
var (
	brokenSessionTablesMu sync.Mutex
	brokenSessionTables   = map[string]struct{}{}
)

// warnBrokenSessionTable emits a one-time slog.Error when a session table
// is missing a REQUIRED column (token_hash / "user" / id / expires_at), so
// an operator gets a clear signal instead of a silent stream of 403s.
func warnBrokenSessionTable(table string, cause error) {
	brokenSessionTablesMu.Lock()
	_, seen := brokenSessionTables[table]
	if !seen {
		brokenSessionTables[table] = struct{}{}
	}
	brokenSessionTablesMu.Unlock()
	if seen {
		return
	}
	detail := ""
	if cause != nil {
		detail = cause.Error()
	}
	slog.Error(
		"auth: session table is missing a REQUIRED column; every request will fall back to anonymous (403). "+
			"The resolver tolerates absent org_id/revoked_at, but requires id, \"user\", token_hash, and expires_at.",
		"table", table,
		"required", `id, "user", token_hash, expires_at`,
		"optional", "org_id, revoked_at",
		"detail", detail,
	)
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
