package auth

import (
	"errors"
	"fmt"
	"time"

	"lazuli.dev/runtime/lazuli"
)

type TheftAction string

const (
	TheftRevokeSessionFamily TheftAction = "revoke_session_family"
	TheftRevokeUser          TheftAction = "revoke_user"
)

var (
	ErrRefreshInvalid   = errors.New("auth: refresh invalid")
	ErrRefreshRevoked   = errors.New("auth: refresh revoked")
	ErrTheftDetected    = errors.New("auth: theft detected")
	errConcurrentRotate = errors.New("auth: concurrent refresh rotation")
	refreshContracts    []SessionsContract
)

type refreshRow struct {
	ID, UserID, OrgID lazuli.ID
	HasOrg            bool
	RefreshExpiresAt  time.Time
	RevokedAt         *time.Time
}

func RegisterRefreshContract(c SessionsContract) {
	if c.Resource == "" {
		return
	}
	c.Refresh, c.Rotation = true, true
	sessionContractsMu.Lock()
	for i := range refreshContracts {
		if refreshContracts[i].Resource == c.Resource {
			refreshContracts[i] = c
			sessionContractsMu.Unlock()
			RegisterSessionContract(c)
			return
		}
	}
	refreshContracts = append(refreshContracts, c)
	sessionContractsMu.Unlock()
	RegisterSessionContract(c)
}

func registeredRefreshContracts() []SessionsContract {
	sessionContractsMu.RLock()
	defer sessionContractsMu.RUnlock()
	return append([]SessionsContract(nil), refreshContracts...)
}

func RotateSession(ctx *lazuli.Ctx, refreshToken string) (string, string, error) {
	h, err := hashSessionToken(refreshToken)
	if err != nil {
		return "", "", refreshError(lazuli.CodeRefreshInvalid, ErrRefreshInvalid)
	}
	now := sessionNow(ctx)
	for _, c := range registeredRefreshContracts() {
		r, ok, err := findRefreshRow(ctx, c, h)
		if err != nil {
			return "", "", err
		}
		if !ok {
			continue
		}
		if !r.RefreshExpiresAt.After(now) {
			return "", "", refreshError(lazuli.CodeRefreshInvalid, ErrRefreshInvalid)
		}
		if r.RevokedAt != nil {
			if now.Sub(*r.RevokedAt) > rotationGrace(c) {
				if err := applyTheftAction(ctx, c, r, now); err != nil {
					return "", "", err
				}
				return "", "", refreshError(lazuli.CodeRefreshRevoked, errors.Join(ErrRefreshRevoked, ErrTheftDetected))
			}
			return issueChild(ctx, c, r, now, false)
		}
		a, f, err := issueChild(ctx, c, r, now, true)
		if errors.Is(err, errConcurrentRotate) {
			return RotateSession(ctx, refreshToken)
		}
		return a, f, err
	}
	return "", "", refreshError(lazuli.CodeRefreshInvalid, ErrRefreshInvalid)
}

func applyTheftAction(ctx *lazuli.Ctx, c SessionsContract, r refreshRow, now time.Time) error {
	t, args := quoteSessionIdent(c.Resource), []any{now, r.ID}
	q := fmt.Sprintf(`WITH RECURSIVE family AS (SELECT id, parent_session_id FROM %s WHERE id=$2 UNION SELECT s.id, s.parent_session_id FROM %s s JOIN family f ON s.id=f.parent_session_id UNION SELECT s.id, s.parent_session_id FROM %s s JOIN family f ON s.parent_session_id=f.id) UPDATE %s SET revoked_at=$1, theft_detected_at=$1 WHERE id IN (SELECT id FROM family)`, t, t, t, t)
	if c.TheftDetectionAction == TheftRevokeUser {
		q, args = fmt.Sprintf(`UPDATE %s SET revoked_at=$1, theft_detected_at=$1 WHERE "user"=$2`, t), []any{now, r.UserID}
	}
	if _, err := sessionDBProvider().Exec(ctxOrBackground(ctx), q, args...); err != nil {
		return lazuli.ClassifyDBError("refresh theft", err)
	}
	return nil
}
