package auth

import (
	"context"
	"encoding/json"
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli"
)

func TestImpersonationActorFromCtxCopiesUserMetadata(t *testing.T) {
	ctx := &lazuli.Ctx{
		Actor: lazuli.ActorUser,
		User: &lazuli.User{
			ID:    10,
			OrgID: 20,
			Email: "admin@example.test",
			Roles: []string{"support", "admin"},
		},
	}

	actor := ImpersonationActorFromCtx(ctx)

	if actor.Kind != AuditActorUser {
		t.Fatalf("Kind = %q, want %q", actor.Kind, AuditActorUser)
	}
	if actor.ID != 10 {
		t.Fatalf("ID = %d, want 10", actor.ID)
	}
	if actor.OrgID != 20 {
		t.Fatalf("OrgID = %d, want 20", actor.OrgID)
	}
	if actor.Email != "admin@example.test" {
		t.Fatalf("Email = %q, want admin@example.test", actor.Email)
	}
	if len(actor.Roles) != 2 || actor.Roles[0] != "support" || actor.Roles[1] != "admin" {
		t.Fatalf("Roles = %#v, want copied user roles", actor.Roles)
	}

	actor.Roles[0] = "changed"
	if ctx.User.Roles[0] != "support" {
		t.Fatalf("source roles mutated to %#v", ctx.User.Roles)
	}
}

func TestImpersonationRequestValidateRequiresActorSubjectReasonAndExpiry(t *testing.T) {
	now := time.Date(2026, 5, 12, 15, 0, 0, 0, time.UTC)
	valid := testImpersonationRequest(now)

	if err := valid.Validate(now); err != nil {
		t.Fatalf("Validate returned error for valid request: %v", err)
	}

	tests := []struct {
		name   string
		mutate func(*ImpersonationRequest)
		want   error
	}{
		{
			name: "actor",
			mutate: func(request *ImpersonationRequest) {
				request.Actor = ImpersonationIdentity{}
			},
			want: ErrImpersonationActorRequired,
		},
		{
			name: "subject",
			mutate: func(request *ImpersonationRequest) {
				request.Subject.ID = 0
			},
			want: ErrImpersonationSubjectRequired,
		},
		{
			name: "reason",
			mutate: func(request *ImpersonationRequest) {
				request.Reason = "  "
			},
			want: ErrImpersonationReasonRequired,
		},
		{
			name: "expiry",
			mutate: func(request *ImpersonationRequest) {
				request.ExpiresAt = time.Time{}
			},
			want: ErrImpersonationExpiryRequired,
		},
		{
			name: "expired",
			mutate: func(request *ImpersonationRequest) {
				request.ExpiresAt = now
			},
			want: ErrImpersonationExpired,
		},
		{
			name: "self",
			mutate: func(request *ImpersonationRequest) {
				request.Subject = request.Actor
			},
			want: ErrImpersonationSelf,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			request := valid
			tc.mutate(&request)

			err := request.Validate(now)
			if !errors.Is(err, tc.want) {
				t.Fatalf("Validate error = %v, want %v", err, tc.want)
			}
		})
	}
}

func TestValidateImpersonationRunsPolicyHooks(t *testing.T) {
	now := time.Date(2026, 5, 12, 15, 0, 0, 0, time.UTC)
	request := testImpersonationRequest(now)
	ctx := &lazuli.Ctx{Context: context.Background(), Now: now}
	policyErr := errors.New("policy denied")
	var calls int

	err := ValidateImpersonation(ctx, request, ImpersonationPolicyFunc(
		func(gotCtx *lazuli.Ctx, got ImpersonationRequest) error {
			calls++
			if gotCtx != ctx {
				t.Fatalf("hook ctx = %#v, want original ctx", gotCtx)
			}
			if got.Subject.ID != request.Subject.ID {
				t.Fatalf("hook Subject.ID = %d, want %d", got.Subject.ID, request.Subject.ID)
			}
			return policyErr
		},
	))

	if !errors.Is(err, policyErr) {
		t.Fatalf("ValidateImpersonation error = %v, want policy error", err)
	}
	if calls != 1 {
		t.Fatalf("hook calls = %d, want 1", calls)
	}
}

func TestValidateImpersonationSkipsHooksAfterStructuralFailure(t *testing.T) {
	now := time.Date(2026, 5, 12, 15, 0, 0, 0, time.UTC)
	request := testImpersonationRequest(now)
	request.Reason = ""
	var calls int

	err := ValidateImpersonation(
		&lazuli.Ctx{Context: context.Background(), Now: now},
		request,
		ImpersonationPolicyFunc(func(*lazuli.Ctx, ImpersonationRequest) error {
			calls++
			return nil
		}),
	)

	if !errors.Is(err, ErrImpersonationReasonRequired) {
		t.Fatalf("ValidateImpersonation error = %v, want reason required", err)
	}
	if calls != 0 {
		t.Fatalf("hook calls = %d, want 0 after structural failure", calls)
	}
}

func TestBuildImpersonationAuditPayloadIncludesActorSubjectReasonAndExpiry(t *testing.T) {
	now := time.Date(2026, 5, 12, 15, 0, 0, 0, time.UTC)
	request := testImpersonationRequest(now)
	request.Actor.Metadata = map[string]any{"directory_id": "okta-actor"}
	request.Subject.Metadata = map[string]any{"directory_id": "okta-subject"}
	request.Reason = "  support ticket 123  "
	request.Details = map[string]any{"ticket_id": "T-123"}

	raw, err := BuildImpersonationAuditPayload(request)
	if err != nil {
		t.Fatalf("BuildImpersonationAuditPayload returned error: %v", err)
	}
	payload := decodeImpersonationPayload(t, raw)

	if payload["reason"] != "support ticket 123" {
		t.Fatalf("payload reason = %v, want trimmed reason", payload["reason"])
	}
	if payload["requested_at"] != request.RequestedAt.Format(time.RFC3339Nano) {
		t.Fatalf("payload requested_at = %v, want %s", payload["requested_at"], request.RequestedAt.Format(time.RFC3339Nano))
	}
	if payload["expires_at"] != request.ExpiresAt.Format(time.RFC3339Nano) {
		t.Fatalf("payload expires_at = %v, want %s", payload["expires_at"], request.ExpiresAt.Format(time.RFC3339Nano))
	}

	actor := payloadMap(t, payload, "actor")
	if actor["kind"] != AuditActorUser {
		t.Fatalf("actor kind = %v, want %s", actor["kind"], AuditActorUser)
	}
	if actor["id"] != float64(10) {
		t.Fatalf("actor id = %v, want 10", actor["id"])
	}
	if actor["org_id"] != float64(20) {
		t.Fatalf("actor org_id = %v, want 20", actor["org_id"])
	}
	metadata := payloadMap(t, actor, "metadata")
	if metadata["directory_id"] != "okta-actor" {
		t.Fatalf("actor metadata directory_id = %v, want okta-actor", metadata["directory_id"])
	}

	subject := payloadMap(t, payload, "subject")
	if subject["id"] != float64(30) {
		t.Fatalf("subject id = %v, want 30", subject["id"])
	}
	if subject["email"] != "customer@example.test" {
		t.Fatalf("subject email = %v, want customer@example.test", subject["email"])
	}
	details := payloadMap(t, payload, "details")
	if details["ticket_id"] != "T-123" {
		t.Fatalf("details ticket_id = %v, want T-123", details["ticket_id"])
	}
}

func TestBuildImpersonationAuditEntryUsesRealActorAndSubjectTarget(t *testing.T) {
	now := time.Date(2026, 5, 12, 15, 0, 0, 0, time.UTC)
	request := testImpersonationRequest(now)
	ctx := &lazuli.Ctx{
		Context:   context.Background(),
		RequestID: "req-impersonation",
		Now:       now,
	}

	entry, err := BuildImpersonationAuditEntry(ctx, ImpersonationAuditEvent{
		Kind:    ImpersonationAuditStarted,
		Request: request,
	})
	if err != nil {
		t.Fatalf("BuildImpersonationAuditEntry returned error: %v", err)
	}

	if entry.CommandName != string(ImpersonationAuditStarted) {
		t.Fatalf("CommandName = %q, want %q", entry.CommandName, ImpersonationAuditStarted)
	}
	if entry.ActorKind != AuditActorUser {
		t.Fatalf("ActorKind = %q, want %q", entry.ActorKind, AuditActorUser)
	}
	if got := impersonationAuditPtrValue(t, entry.ActorID, "ActorID"); got != 10 {
		t.Fatalf("ActorID = %d, want 10", got)
	}
	if got := impersonationAuditPtrValue(t, entry.OrgID, "OrgID"); got != 20 {
		t.Fatalf("OrgID = %d, want 20", got)
	}
	if entry.TargetResource != "Impersonation" {
		t.Fatalf("TargetResource = %q, want Impersonation", entry.TargetResource)
	}
	if got := impersonationAuditPtrValue(t, entry.TargetID, "TargetID"); got != 30 {
		t.Fatalf("TargetID = %d, want 30", got)
	}
	if entry.ResultStatus != AuditResultOK {
		t.Fatalf("ResultStatus = %q, want %q", entry.ResultStatus, AuditResultOK)
	}
	if entry.CorrelationID != "req-impersonation" {
		t.Fatalf("CorrelationID = %q, want req-impersonation", entry.CorrelationID)
	}
	if len(entry.Payload) == 0 {
		t.Fatal("Payload is empty")
	}
}

func testImpersonationRequest(now time.Time) ImpersonationRequest {
	return ImpersonationRequest{
		Actor: ImpersonationIdentity{
			Kind:  AuditActorUser,
			ID:    10,
			OrgID: 20,
			Email: "admin@example.test",
			Roles: []string{"support"},
		},
		Subject: ImpersonationIdentity{
			Kind:  AuditActorUser,
			ID:    30,
			OrgID: 20,
			Email: "customer@example.test",
		},
		Reason:      "support ticket 123",
		RequestedAt: now.Add(-time.Minute),
		ExpiresAt:   now.Add(30 * time.Minute),
	}
}

func decodeImpersonationPayload(t *testing.T, raw []byte) map[string]any {
	t.Helper()
	var payload map[string]any
	if err := json.Unmarshal(raw, &payload); err != nil {
		t.Fatalf("payload JSON decode error: %v", err)
	}
	return payload
}

func payloadMap(t *testing.T, parent map[string]any, key string) map[string]any {
	t.Helper()
	child, ok := parent[key].(map[string]any)
	if !ok {
		t.Fatalf("payload %s = %#v, want object", key, parent[key])
	}
	return child
}

func impersonationAuditPtrValue(t *testing.T, got *int64, name string) int64 {
	t.Helper()
	if got == nil {
		t.Fatalf("%s = nil, want pointer", name)
	}
	return *got
}
