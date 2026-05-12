package auth

import (
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"time"

	"lazuli.dev/runtime/lazuli"
)

// ImpersonationAuditKind names impersonation lifecycle events recorded in
// audit_log.
type ImpersonationAuditKind string

const (
	ImpersonationAuditStarted ImpersonationAuditKind = "auth.impersonation.started"
	ImpersonationAuditEnded   ImpersonationAuditKind = "auth.impersonation.ended"
)

// Typed errors returned by the impersonation helpers. They are intentionally
// structural and adapter-neutral; product policy failures should come from
// ImpersonationPolicyHook implementations.
var (
	ErrImpersonationActorRequired   = errors.New("auth: impersonation actor required")
	ErrImpersonationSubjectRequired = errors.New("auth: impersonation subject required")
	ErrImpersonationReasonRequired  = errors.New("auth: impersonation reason required")
	ErrImpersonationExpiryRequired  = errors.New("auth: impersonation expiry required")
	ErrImpersonationExpired         = errors.New("auth: impersonation expired")
	ErrImpersonationSelf            = errors.New("auth: impersonation self")
)

var errImpersonationAuditKindMissing = errors.New("auth: impersonation audit kind missing")

// ImpersonationIdentity is the non-secret identity metadata recorded for both
// the real actor and the temporary subject. Metadata is reserved for
// adapter-specific facts such as directory ids; callers must not put secrets in
// it because it is eligible for audit payloads.
type ImpersonationIdentity struct {
	Kind     string
	ID       lazuli.ID
	OrgID    lazuli.ID
	Email    string
	Roles    []string
	Metadata map[string]any
}

// ImpersonationRequest describes a requested impersonation grant before any
// adapter/session store persists it.
type ImpersonationRequest struct {
	Actor       ImpersonationIdentity
	Subject     ImpersonationIdentity
	Reason      string
	RequestedAt time.Time
	ExpiresAt   time.Time
	Details     map[string]any
}

// ImpersonationAuditEvent wraps a validated request in an audit lifecycle
// event. ErrorCode marks the resulting audit row as failed.
type ImpersonationAuditEvent struct {
	Kind      ImpersonationAuditKind
	Request   ImpersonationRequest
	ErrorCode string
}

// ImpersonationPolicyHook is the extension point for product-specific
// impersonation rules, such as role allow-lists, same-org checks, or maximum
// durations.
type ImpersonationPolicyHook interface {
	ValidateImpersonation(ctx *lazuli.Ctx, request ImpersonationRequest) error
}

// ImpersonationPolicyFunc adapts a function to ImpersonationPolicyHook.
type ImpersonationPolicyFunc func(ctx *lazuli.Ctx, request ImpersonationRequest) error

// ValidateImpersonation implements ImpersonationPolicyHook.
func (fn ImpersonationPolicyFunc) ValidateImpersonation(ctx *lazuli.Ctx, request ImpersonationRequest) error {
	if fn == nil {
		return nil
	}
	return fn(ctx, request)
}

// ImpersonationIdentityFromUser converts a Lazuli user into impersonation
// identity metadata.
func ImpersonationIdentityFromUser(user *lazuli.User) ImpersonationIdentity {
	if user == nil {
		return ImpersonationIdentity{}
	}
	return ImpersonationIdentity{
		Kind:  AuditActorUser,
		ID:    user.ID,
		OrgID: user.OrgID,
		Email: user.Email,
		Roles: append([]string(nil), user.Roles...),
	}
}

// ImpersonationActorFromCtx returns the real actor metadata from the active
// Lazuli context.
func ImpersonationActorFromCtx(ctx *lazuli.Ctx) ImpersonationIdentity {
	if ctx == nil {
		return ImpersonationIdentity{}
	}
	if ctx.User != nil {
		identity := ImpersonationIdentityFromUser(ctx.User)
		if ctx.Actor != "" {
			identity.Kind = string(ctx.Actor)
		}
		return identity
	}
	if ctx.Actor != "" {
		return ImpersonationIdentity{Kind: string(ctx.Actor)}
	}
	return ImpersonationIdentity{}
}

// Validate enforces the framework-level impersonation invariants: the real
// actor and subject must be explicit, a reason is required, and the grant must
// expire in the future. Product-specific policy belongs in hooks passed to
// ValidateImpersonation.
func (r ImpersonationRequest) Validate(now time.Time) error {
	if err := r.validateRequired(); err != nil {
		return err
	}
	if now.IsZero() {
		now = time.Now()
	}
	if !r.ExpiresAt.After(now) {
		return fmt.Errorf("%w: expires_at must be after now", ErrImpersonationExpired)
	}
	return nil
}

// ValidateImpersonation validates the structural request and then runs
// product-supplied policy hooks in declaration order.
func ValidateImpersonation(
	ctx *lazuli.Ctx,
	request ImpersonationRequest,
	hooks ...ImpersonationPolicyHook,
) error {
	if err := sessionStoreContextErr(ctxOrBackground(ctx)); err != nil {
		return err
	}
	if err := request.Validate(impersonationNow(ctx)); err != nil {
		return err
	}
	for _, hook := range hooks {
		if hook == nil {
			continue
		}
		if err := hook.ValidateImpersonation(ctx, request); err != nil {
			return err
		}
	}
	return nil
}

// BuildImpersonationAuditPayload returns the JSON payload used for
// impersonation audit rows.
func BuildImpersonationAuditPayload(request ImpersonationRequest) ([]byte, error) {
	if err := request.validateRequired(); err != nil {
		return nil, err
	}

	payload := map[string]any{
		"actor":      impersonationIdentityPayload(request.Actor),
		"subject":    impersonationIdentityPayload(request.Subject),
		"reason":     strings.TrimSpace(request.Reason),
		"expires_at": request.ExpiresAt.UTC().Format(time.RFC3339Nano),
	}
	if !request.RequestedAt.IsZero() {
		payload["requested_at"] = request.RequestedAt.UTC().Format(time.RFC3339Nano)
	}
	if len(request.Details) > 0 {
		payload["details"] = cloneSessionAttrs(request.Details)
	}
	return json.Marshal(payload)
}

// BuildImpersonationAuditEntry converts an impersonation lifecycle event into
// the generic audit_log row shape.
func BuildImpersonationAuditEntry(ctx *lazuli.Ctx, event ImpersonationAuditEvent) (AuditEntry, error) {
	if event.Kind == "" {
		return AuditEntry{}, errImpersonationAuditKindMissing
	}
	payload, err := BuildImpersonationAuditPayload(event.Request)
	if err != nil {
		return AuditEntry{}, err
	}

	entry := AuditFromCtx(ctx).
		WithCommand(string(event.Kind)).
		WithTarget("Impersonation", event.Request.Subject.ID).
		WithPayload(payload).
		Succeeded()
	if event.ErrorCode != "" {
		entry = entry.Failed(event.ErrorCode)
	}
	applyImpersonationAuditActor(&entry, event.Request.Actor)
	applyImpersonationAuditOrg(&entry, event.Request)
	return entry, nil
}

func (r ImpersonationRequest) validateRequired() error {
	if err := validateImpersonationActor(r.Actor); err != nil {
		return err
	}
	if err := validateImpersonationSubject(r.Subject); err != nil {
		return err
	}
	if strings.TrimSpace(r.Reason) == "" {
		return ErrImpersonationReasonRequired
	}
	if r.ExpiresAt.IsZero() {
		return ErrImpersonationExpiryRequired
	}
	if sameImpersonationIdentity(r.Actor, r.Subject) {
		return ErrImpersonationSelf
	}
	return nil
}

func validateImpersonationActor(identity ImpersonationIdentity) error {
	kind := strings.TrimSpace(identity.Kind)
	if kind == "" {
		return ErrImpersonationActorRequired
	}
	if identity.ID == 0 && kind != AuditActorSystem {
		return ErrImpersonationActorRequired
	}
	return nil
}

func validateImpersonationSubject(identity ImpersonationIdentity) error {
	if strings.TrimSpace(identity.Kind) == "" || identity.ID == 0 {
		return ErrImpersonationSubjectRequired
	}
	return nil
}

func sameImpersonationIdentity(actor, subject ImpersonationIdentity) bool {
	if actor.ID == 0 || subject.ID == 0 {
		return false
	}
	return strings.TrimSpace(actor.Kind) == strings.TrimSpace(subject.Kind) && actor.ID == subject.ID
}

func impersonationIdentityPayload(identity ImpersonationIdentity) map[string]any {
	payload := map[string]any{
		"kind": strings.TrimSpace(identity.Kind),
	}
	if identity.ID != 0 {
		payload["id"] = int64(identity.ID)
	}
	if identity.OrgID != 0 {
		payload["org_id"] = int64(identity.OrgID)
	}
	if identity.Email != "" {
		payload["email"] = identity.Email
	}
	if len(identity.Roles) > 0 {
		payload["roles"] = append([]string(nil), identity.Roles...)
	}
	if len(identity.Metadata) > 0 {
		payload["metadata"] = cloneSessionAttrs(identity.Metadata)
	}
	return payload
}

func applyImpersonationAuditActor(entry *AuditEntry, actor ImpersonationIdentity) {
	entry.ActorKind = strings.TrimSpace(actor.Kind)
	if actor.ID != 0 {
		entry.ActorID = auditIDPtr(actor.ID)
	}
}

func applyImpersonationAuditOrg(entry *AuditEntry, request ImpersonationRequest) {
	if entry.OrgID != nil {
		return
	}
	if request.Subject.OrgID != 0 {
		entry.OrgID = auditIDPtr(request.Subject.OrgID)
		return
	}
	actor := request.Actor
	if actor.OrgID != 0 {
		entry.OrgID = auditIDPtr(actor.OrgID)
	}
}

func impersonationNow(ctx *lazuli.Ctx) time.Time {
	if ctx != nil && !ctx.Now.IsZero() {
		return ctx.Now
	}
	return time.Now()
}
