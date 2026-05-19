package auth

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5"

	"lazuli.dev/runtime/lazuli"
)

type ResolvedSession struct {
	ExpiresAt time.Time
}

func (runtimeResolver) ResolveAccessWithExpiry(ctx context.Context, token string) (time.Time, bool, error) {
	for _, contract := range registeredRefreshContracts() {
		resolved, err := ResolveAccessWithExpiry(ctx, contract, token)
		if errors.Is(err, ErrSessionExpired) {
			return resolved.ExpiresAt, true, nil
		}
		if errors.Is(err, ErrSessionNotFound) || errors.Is(err, ErrTokenInvalid) || err == nil {
			continue
		}
		return time.Time{}, false, err
	}
	return time.Time{}, false, nil
}

func ResolveAccessWithExpiry(ctx context.Context, contract SessionsContract, accessToken string) (ResolvedSession, error) {
	tokenHash, err := hashSessionToken(accessToken)
	if err != nil {
		return ResolvedSession{}, ErrTokenInvalid
	}
	q := fmt.Sprintf(`SELECT expires_at, revoked_at, refresh_token_hash FROM %s WHERE token_hash = $1 LIMIT 1`, quoteSessionIdent(contract.Resource))
	var exp time.Time
	var revoked *time.Time
	var refreshHash string
	err = sessionDBProvider().QueryRow(ctx, q, tokenHash).Scan(&exp, &revoked, &refreshHash)
	if errors.Is(err, pgx.ErrNoRows) {
		return ResolvedSession{}, ErrSessionNotFound
	}
	if err != nil {
		return ResolvedSession{}, lazuli.ClassifyDBError("access lookup", err)
	}
	if revoked != nil || refreshHash == "" {
		return ResolvedSession{}, ErrSessionNotFound
	}
	if !exp.After(time.Now()) {
		return ResolvedSession{ExpiresAt: exp}, ErrSessionExpired
	}
	return ResolvedSession{}, nil
}

func PopulateExpiredSignal(ctx *lazuli.Ctx, exp time.Time) {
	if ctx != nil {
		ctx.SessionExpiredAt = &exp
	}
}
