package auth

import "time"

// SessionRotationReason names a rule that selected a session for token
// rotation. The values are stable strings callers can use for audit payloads.
type SessionRotationReason string

const (
	SessionRotationReasonAbsoluteAge               SessionRotationReason = "absolute_age"
	SessionRotationReasonIdleAge                   SessionRotationReason = "idle_age"
	SessionRotationReasonCredentialSensitiveAction SessionRotationReason = "credential_sensitive_action"
	SessionRotationReasonLogin                     SessionRotationReason = "login"
)

// SessionRotationPolicy controls storage-agnostic session token rotation.
type SessionRotationPolicy struct {
	// MaxAbsoluteAge rotates a session at or after IssuedAt plus this duration.
	// Non-positive values disable absolute-age rotation.
	MaxAbsoluteAge time.Duration
	// MaxIdleAge rotates a session at or after LastSeenAt plus this duration.
	// If LastSeenAt is unavailable, IssuedAt is used as the idle anchor.
	// Non-positive values disable idle-age rotation.
	MaxIdleAge time.Duration
	// RotateOnLogin rotates the current session when a login event completes.
	RotateOnLogin bool
}

// SessionRotationSnapshot is the adapter-neutral state needed to evaluate
// rotation rules for the current session token.
type SessionRotationSnapshot struct {
	// IssuedAt is when the current session token was created.
	IssuedAt time.Time
	// LastSeenAt is the latest authenticated activity observed for the session.
	LastSeenAt time.Time
}

// SessionRotationEvent describes the auth event currently being handled.
type SessionRotationEvent struct {
	// CredentialSensitiveAction marks credential, MFA, recovery, or similar
	// sensitive account changes. These always rotate the active session.
	CredentialSensitiveAction bool
	// Login marks a completed login flow. Rotation depends on policy.
	Login bool
}

// SessionRotationPlan is a dry-run decision concrete adapters can apply by
// issuing a replacement token and invalidating the previous one.
type SessionRotationPlan struct {
	GeneratedAt time.Time
	Rotate      bool
	Reasons     []SessionRotationReason
}

// ShouldRotateSession reports whether the current session token should rotate.
func ShouldRotateSession(
	now time.Time,
	session SessionRotationSnapshot,
	event SessionRotationEvent,
	policy SessionRotationPolicy,
) bool {
	return len(sessionRotationReasons(normalizeSessionRotationTime(now), session, event, policy)) > 0
}

// PlanSessionRotation returns a storage-agnostic rotation decision and all
// matching reasons. It does not mutate or require a session store.
func PlanSessionRotation(
	now time.Time,
	session SessionRotationSnapshot,
	event SessionRotationEvent,
	policy SessionRotationPolicy,
) SessionRotationPlan {
	now = normalizeSessionRotationTime(now)
	reasons := sessionRotationReasons(now, session, event, policy)
	return SessionRotationPlan{
		GeneratedAt: now,
		Rotate:      len(reasons) > 0,
		Reasons:     reasons,
	}
}

func sessionRotationReasons(
	now time.Time,
	session SessionRotationSnapshot,
	event SessionRotationEvent,
	policy SessionRotationPolicy,
) []SessionRotationReason {
	reasons := make([]SessionRotationReason, 0, 4)
	if sessionRotationAgeExceeded(now, session.IssuedAt, policy.MaxAbsoluteAge) {
		reasons = append(reasons, SessionRotationReasonAbsoluteAge)
	}
	if sessionRotationAgeExceeded(now, sessionRotationIdleAnchor(session), policy.MaxIdleAge) {
		reasons = append(reasons, SessionRotationReasonIdleAge)
	}
	if event.CredentialSensitiveAction {
		reasons = append(reasons, SessionRotationReasonCredentialSensitiveAction)
	}
	if event.Login && policy.RotateOnLogin {
		reasons = append(reasons, SessionRotationReasonLogin)
	}
	return reasons
}

func sessionRotationAgeExceeded(now, anchor time.Time, maxAge time.Duration) bool {
	if anchor.IsZero() || maxAge <= 0 {
		return false
	}
	return !anchor.Add(maxAge).After(now)
}

func sessionRotationIdleAnchor(session SessionRotationSnapshot) time.Time {
	if !session.LastSeenAt.IsZero() {
		return session.LastSeenAt
	}
	return session.IssuedAt
}

func normalizeSessionRotationTime(t time.Time) time.Time {
	if t.IsZero() {
		return time.Now().UTC()
	}
	return t.UTC()
}
