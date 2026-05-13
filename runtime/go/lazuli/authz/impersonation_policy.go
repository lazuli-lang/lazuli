package authz

import (
	"fmt"
	"strconv"
	"strings"
	"time"
)

// ImpersonationReason classifies an impersonation policy decision.
type ImpersonationReason string

const (
	// ImpersonationReasonAllowed means every configured policy constraint
	// matched.
	ImpersonationReasonAllowed ImpersonationReason = "allowed"

	// ImpersonationReasonSelfDenied means the actor and subject are the same
	// non-zero principal and self-impersonation is not explicitly allowed.
	ImpersonationReasonSelfDenied ImpersonationReason = "self_denied"

	// ImpersonationReasonOrgMismatch means RequireSameOrg was set and actor and
	// subject org ids do not match.
	ImpersonationReasonOrgMismatch ImpersonationReason = "org_mismatch"

	// ImpersonationReasonReasonRequired means RequireReason was set and the
	// request reason is empty.
	ImpersonationReasonReasonRequired ImpersonationReason = "reason_required"

	// ImpersonationReasonDurationRequired means MaxDuration was set but the
	// request does not carry a positive RequestedAt..ExpiresAt duration.
	ImpersonationReasonDurationRequired ImpersonationReason = "duration_required"

	// ImpersonationReasonDurationExceeded means the requested impersonation
	// duration is above MaxDuration.
	ImpersonationReasonDurationExceeded ImpersonationReason = "duration_exceeded"

	// ImpersonationReasonActorDenied means the actor failed its configured
	// principal constraint.
	ImpersonationReasonActorDenied ImpersonationReason = "actor_denied"

	// ImpersonationReasonSubjectDenied means the subject failed its configured
	// principal constraint.
	ImpersonationReasonSubjectDenied ImpersonationReason = "subject_denied"

	// ImpersonationReasonRoleDenied means a subject role is outside
	// AllowedSubjectRoles.
	ImpersonationReasonRoleDenied ImpersonationReason = "role_denied"

	// ImpersonationReasonScopeDenied means a subject scope is outside
	// AllowedSubjectScopes.
	ImpersonationReasonScopeDenied ImpersonationReason = "scope_denied"
)

// String returns the stable reason token.
func (r ImpersonationReason) String() string {
	if r == "" {
		return string(ImpersonationReasonAllowed)
	}
	return string(r)
}

// ImpersonationPrincipal is the provider-neutral identity shape evaluated by
// ImpersonationPolicy. It intentionally uses any for ids so callers can pass
// the same id type they already use at auth/session boundaries.
type ImpersonationPrincipal struct {
	Kind   string
	ID     any
	OrgID  any
	Roles  []string
	Scopes []string
}

// Normalize returns principal with trimmed kind/id strings and deduplicated
// role/scope lists.
func (p ImpersonationPrincipal) Normalize() ImpersonationPrincipal {
	p.Kind = strings.TrimSpace(p.Kind)
	p.ID = normalizeID(p.ID)
	p.OrgID = normalizeID(p.OrgID)
	p.Roles = cleanRoleNames(p.Roles)
	p.Scopes = cleanImpersonationScopes(p.Scopes)
	return p
}

// ImpersonationPrincipalConstraint restricts which actors or subjects may
// participate in an impersonation request. Empty fields do not restrict that
// dimension. Roles match when the principal has any listed role, honoring the
// policy RoleGraph for inherited roles. Scopes are required capabilities; every
// listed scope must be covered by the principal's scopes.
type ImpersonationPrincipalConstraint struct {
	Kinds  []string
	IDs    []any
	OrgIDs []any
	Roles  []string
	Scopes []string
}

// ImpersonationRequest is the authorization question evaluated by
// ImpersonationPolicy.
type ImpersonationRequest struct {
	Actor   ImpersonationPrincipal
	Subject ImpersonationPrincipal

	Reason      string
	RequestedAt time.Time
	ExpiresAt   time.Time
}

// Normalize returns request with normalized actor and subject principals.
func (r ImpersonationRequest) Normalize() ImpersonationRequest {
	r.Actor = r.Actor.Normalize()
	r.Subject = r.Subject.Normalize()
	r.Reason = strings.TrimSpace(r.Reason)
	r.RequestedAt = normalizeImpersonationTime(r.RequestedAt)
	r.ExpiresAt = normalizeImpersonationTime(r.ExpiresAt)
	return r
}

// Duration returns the requested impersonation lifetime. A zero value means the
// request does not carry enough timing information to derive a positive
// duration.
func (r ImpersonationRequest) Duration() time.Duration {
	r = r.Normalize()
	if r.RequestedAt.IsZero() || r.ExpiresAt.IsZero() {
		return 0
	}
	duration := r.ExpiresAt.Sub(r.RequestedAt)
	if duration <= 0 {
		return 0
	}
	return duration
}

// ImpersonationPolicy evaluates provider-neutral impersonation constraints.
// Empty allow-lists and constraints do not restrict their dimension. By
// default, self-impersonation is denied when actor and subject have the same
// non-zero kind/id pair; set AllowSelf to true to permit it.
type ImpersonationPolicy struct {
	// Roles enables inherited role checks for Actor.Roles and Subject.Roles
	// constraints. AllowedSubjectRoles remains an exact allow-list.
	Roles *RoleGraph

	Actor   ImpersonationPrincipalConstraint
	Subject ImpersonationPrincipalConstraint

	// RequireSameOrg requires non-zero actor and subject org ids to match.
	RequireSameOrg bool

	// AllowSelf permits actor and subject to be the same non-zero kind/id pair.
	AllowSelf bool

	// RequireReason requires a non-empty request reason.
	RequireReason bool

	// MaxDuration caps ExpiresAt-RequestedAt. Zero means no duration cap.
	MaxDuration time.Duration

	// AllowedSubjectRoles caps roles carried by the impersonated subject. Every
	// subject role must appear exactly in this allow-list when it is non-empty.
	AllowedSubjectRoles []string

	// AllowedSubjectScopes caps scopes carried by the impersonated subject. Each
	// subject scope must be covered by this allow-list when it is non-empty.
	// Allow-list entries may use '*' as a wildcard.
	AllowedSubjectScopes []string
}

// ImpersonationEvaluation is the structured decision returned by
// ImpersonationPolicy.
type ImpersonationEvaluation struct {
	Allowed bool
	Reason  ImpersonationReason

	Actor   ImpersonationPrincipal
	Subject ImpersonationPrincipal

	Duration    time.Duration
	MaxDuration time.Duration

	// Role or Scope identifies the first denied subject role/scope when the
	// decision reason is role_denied or scope_denied.
	Role  string
	Scope string
}

// Explanation renders the evaluation in a stable field order for audit logs,
// tests, and generated problem details.
func (e ImpersonationEvaluation) Explanation() string {
	actor := e.Actor.Normalize()
	subject := e.Subject.Normalize()
	parts := []string{
		"allowed=" + strconv.FormatBool(e.Allowed),
		"reason=" + e.Reason.String(),
		"actor_kind=" + impersonationExplanationToken(actor.Kind),
		"actor_id=" + impersonationExplanationToken(formatImpersonationID(actor.ID)),
		"actor_org=" + impersonationExplanationToken(formatImpersonationID(actor.OrgID)),
		"subject_kind=" + impersonationExplanationToken(subject.Kind),
		"subject_id=" + impersonationExplanationToken(formatImpersonationID(subject.ID)),
		"subject_org=" + impersonationExplanationToken(formatImpersonationID(subject.OrgID)),
		"duration=" + e.Duration.String(),
		"max_duration=" + e.MaxDuration.String(),
		"role=" + impersonationExplanationToken(e.Role),
		"scope=" + impersonationExplanationToken(e.Scope),
	}
	return strings.Join(parts, " ")
}

// Evaluate returns the impersonation authorization decision for request.
func (p ImpersonationPolicy) Evaluate(request ImpersonationRequest) ImpersonationEvaluation {
	request = request.Normalize()
	evaluation := ImpersonationEvaluation{
		Allowed:     true,
		Reason:      ImpersonationReasonAllowed,
		Actor:       request.Actor,
		Subject:     request.Subject,
		Duration:    request.Duration(),
		MaxDuration: p.MaxDuration,
	}

	if !p.AllowSelf && sameImpersonationPrincipal(request.Actor, request.Subject) {
		return evaluation.deny(ImpersonationReasonSelfDenied)
	}
	if p.RequireSameOrg && !sameNonZeroID(request.Actor.OrgID, request.Subject.OrgID) {
		return evaluation.deny(ImpersonationReasonOrgMismatch)
	}
	if p.RequireReason && request.Reason == "" {
		return evaluation.deny(ImpersonationReasonReasonRequired)
	}
	if p.MaxDuration > 0 {
		if evaluation.Duration <= 0 {
			return evaluation.deny(ImpersonationReasonDurationRequired)
		}
		if evaluation.Duration > p.MaxDuration {
			return evaluation.deny(ImpersonationReasonDurationExceeded)
		}
	}
	if !p.principalMatches(request.Actor, p.Actor) {
		return evaluation.deny(ImpersonationReasonActorDenied)
	}
	if !p.principalMatches(request.Subject, p.Subject) {
		return evaluation.deny(ImpersonationReasonSubjectDenied)
	}
	if role, ok := firstDeniedImpersonationRole(request.Subject.Roles, p.AllowedSubjectRoles); ok {
		evaluation.Role = role
		return evaluation.deny(ImpersonationReasonRoleDenied)
	}
	if scope, ok := firstDeniedImpersonationScope(request.Subject.Scopes, p.AllowedSubjectScopes); ok {
		evaluation.Scope = scope
		return evaluation.deny(ImpersonationReasonScopeDenied)
	}
	return evaluation
}

// Explain is an alias for Evaluate for call sites that treat the structured
// decision as diagnostic output.
func (p ImpersonationPolicy) Explain(request ImpersonationRequest) ImpersonationEvaluation {
	return p.Evaluate(request)
}

// Allow reports whether Evaluate permits request.
func (p ImpersonationPolicy) Allow(request ImpersonationRequest) bool {
	return p.Evaluate(request).Allowed
}

func (e ImpersonationEvaluation) deny(reason ImpersonationReason) ImpersonationEvaluation {
	e.Allowed = false
	e.Reason = reason
	return e
}

func (p ImpersonationPolicy) principalMatches(principal ImpersonationPrincipal, constraint ImpersonationPrincipalConstraint) bool {
	principal = principal.Normalize()
	if !stringConstraintMatches(principal.Kind, constraint.Kinds) {
		return false
	}
	if !idConstraintMatches(principal.ID, constraint.IDs) {
		return false
	}
	if !idConstraintMatches(principal.OrgID, constraint.OrgIDs) {
		return false
	}
	roles := cleanRoleNames(constraint.Roles)
	if len(roles) > 0 && !(Policy{Roles: p.Roles}).hasAnyRole(principal.Roles, roles) {
		return false
	}
	scopes := cleanImpersonationScopes(constraint.Scopes)
	return hasAllImpersonationScopes(principal.Scopes, scopes)
}

func stringConstraintMatches(value string, allowed []string) bool {
	allowed = cleanImpersonationScopes(allowed)
	if len(allowed) == 0 {
		return true
	}
	value = strings.TrimSpace(value)
	for _, candidate := range allowed {
		if value == candidate {
			return true
		}
	}
	return false
}

func idConstraintMatches(value any, allowed []any) bool {
	if len(allowed) == 0 {
		return true
	}
	for _, candidate := range allowed {
		if sameNonZeroID(value, candidate) {
			return true
		}
	}
	return false
}

func firstDeniedImpersonationRole(roles, allowed []string) (string, bool) {
	allowed = cleanRoleNames(allowed)
	if len(allowed) == 0 {
		return "", false
	}
	allowedSet := make(map[string]struct{}, len(allowed))
	for _, role := range allowed {
		allowedSet[role] = struct{}{}
	}
	for _, role := range cleanRoleNames(roles) {
		if _, ok := allowedSet[role]; !ok {
			return role, true
		}
	}
	return "", false
}

func firstDeniedImpersonationScope(scopes, allowed []string) (string, bool) {
	allowed = cleanImpersonationScopes(allowed)
	if len(allowed) == 0 {
		return "", false
	}
	for _, scope := range cleanImpersonationScopes(scopes) {
		if !impersonationScopeAllowed(allowed, scope) {
			return scope, true
		}
	}
	return "", false
}

func hasAllImpersonationScopes(grants, required []string) bool {
	required = cleanImpersonationScopes(required)
	if len(required) == 0 {
		return true
	}
	grants = cleanImpersonationScopes(grants)
	for _, scope := range required {
		if !impersonationScopeAllowed(grants, scope) {
			return false
		}
	}
	return true
}

func impersonationScopeAllowed(grants []string, required string) bool {
	required = strings.TrimSpace(required)
	if required == "" {
		return false
	}
	for _, grant := range grants {
		if matchImpersonationScope(grant, required) {
			return true
		}
	}
	return false
}

func matchImpersonationScope(grant, required string) bool {
	grant = strings.TrimSpace(grant)
	required = strings.TrimSpace(required)
	if grant == required && grant != "" {
		return true
	}
	if grant == "" || required == "" || strings.Contains(required, "*") {
		return false
	}
	if grant == "*" {
		return true
	}
	if !strings.Contains(grant, "*") {
		return false
	}

	parts := strings.Split(grant, "*")
	pos := 0
	if parts[0] != "" {
		if !strings.HasPrefix(required, parts[0]) {
			return false
		}
		pos = len(parts[0])
	}
	for i := 1; i < len(parts); i++ {
		part := parts[i]
		if part == "" {
			continue
		}
		found := strings.Index(required[pos:], part)
		if found < 0 {
			return false
		}
		pos += found + len(part)
	}
	last := parts[len(parts)-1]
	return last == "" || strings.HasSuffix(required, last)
}

func cleanImpersonationScopes(scopes []string) []string {
	seen := map[string]struct{}{}
	out := make([]string, 0, len(scopes))
	for _, scope := range scopes {
		scope = strings.TrimSpace(scope)
		if scope == "" {
			continue
		}
		if _, ok := seen[scope]; ok {
			continue
		}
		seen[scope] = struct{}{}
		out = append(out, scope)
	}
	return out
}

func sameImpersonationPrincipal(actor, subject ImpersonationPrincipal) bool {
	actor = actor.Normalize()
	subject = subject.Normalize()
	return actor.Kind != "" && actor.Kind == subject.Kind && sameNonZeroID(actor.ID, subject.ID)
}

func normalizeImpersonationTime(t time.Time) time.Time {
	if t.IsZero() {
		return time.Time{}
	}
	return t.Round(0).UTC()
}

func formatImpersonationID(value any) string {
	value = normalizeID(value)
	if isZeroID(value) {
		return ""
	}
	return fmt.Sprint(value)
}

func impersonationExplanationToken(value string) string {
	if value == "" {
		return `""`
	}
	return strconv.Quote(value)
}
