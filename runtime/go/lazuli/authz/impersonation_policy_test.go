package authz

import (
	"testing"
	"time"
)

func TestImpersonationPolicyAllowsMatchingRequestWithExplanation(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 15, 0, 0, 0, time.UTC)
	graph := mustRoleGraph(t,
		Role{Name: "support"},
		Role{Name: "admin", Inherits: []string{"support"}},
	)
	policy := ImpersonationPolicy{
		Roles: graph,
		Actor: ImpersonationPrincipalConstraint{
			Kinds:  []string{"user"},
			Roles:  []string{"support"},
			Scopes: []string{"admin:impersonate"},
		},
		Subject: ImpersonationPrincipalConstraint{
			Kinds:  []string{"user"},
			OrgIDs: []any{"org-1"},
		},
		RequireSameOrg:       true,
		RequireReason:        true,
		MaxDuration:          time.Hour,
		AllowedSubjectRoles:  []string{"customer"},
		AllowedSubjectScopes: []string{"customers:*", "tickets:read"},
	}
	request := ImpersonationRequest{
		Actor: ImpersonationPrincipal{
			Kind:   " user ",
			ID:     " actor-1 ",
			OrgID:  " org-1 ",
			Roles:  []string{" admin "},
			Scopes: []string{"admin:*"},
		},
		Subject: ImpersonationPrincipal{
			Kind:   "user",
			ID:     "subject-1",
			OrgID:  "org-1",
			Roles:  []string{" customer "},
			Scopes: []string{"customers:read", "tickets:read"},
		},
		Reason:      " support ticket 123 ",
		RequestedAt: now,
		ExpiresAt:   now.Add(30 * time.Minute),
	}

	evaluation := policy.Evaluate(request)
	if !evaluation.Allowed {
		t.Fatalf("Evaluate() allowed = false, want true: %#v", evaluation)
	}
	if evaluation.Reason != ImpersonationReasonAllowed {
		t.Fatalf("Evaluate() reason = %q, want allowed", evaluation.Reason)
	}
	if !policy.Allow(request) {
		t.Fatal("Allow() = false, want true")
	}
	if explained := policy.Explain(request); !explained.Allowed {
		t.Fatalf("Explain() allowed = false, want true: %#v", explained)
	}

	want := `allowed=true reason=allowed actor_kind="user" actor_id="actor-1" actor_org="org-1" subject_kind="user" subject_id="subject-1" subject_org="org-1" duration=30m0s max_duration=1h0m0s role="" scope=""`
	if got := evaluation.Explanation(); got != want {
		t.Fatalf("Explanation() = %q, want %q", got, want)
	}
}

func TestImpersonationPolicyAppliesActorAndSubjectConstraints(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 15, 0, 0, 0, time.UTC)
	base := ImpersonationRequest{
		Actor: ImpersonationPrincipal{
			Kind:   "user",
			ID:     "actor-1",
			OrgID:  "org-1",
			Roles:  []string{"support"},
			Scopes: []string{"admin:impersonate"},
		},
		Subject: ImpersonationPrincipal{
			Kind:  "user",
			ID:    "subject-1",
			OrgID: "org-1",
		},
		RequestedAt: now,
		ExpiresAt:   now.Add(15 * time.Minute),
	}

	tests := []struct {
		name   string
		policy ImpersonationPolicy
		mutate func(*ImpersonationRequest)
		want   ImpersonationReason
		wantOK bool
	}{
		{
			name: "actor role",
			policy: ImpersonationPolicy{
				Actor: ImpersonationPrincipalConstraint{Roles: []string{"support"}},
			},
			mutate: func(request *ImpersonationRequest) {
				request.Actor.Roles = []string{"viewer"}
			},
			want: ImpersonationReasonActorDenied,
		},
		{
			name: "actor scope",
			policy: ImpersonationPolicy{
				Actor: ImpersonationPrincipalConstraint{Scopes: []string{"admin:impersonate"}},
			},
			mutate: func(request *ImpersonationRequest) {
				request.Actor.Scopes = []string{"admin:read"}
			},
			want: ImpersonationReasonActorDenied,
		},
		{
			name: "subject kind",
			policy: ImpersonationPolicy{
				Subject: ImpersonationPrincipalConstraint{Kinds: []string{"customer"}},
			},
			want: ImpersonationReasonSubjectDenied,
		},
		{
			name: "subject org id",
			policy: ImpersonationPolicy{
				Subject: ImpersonationPrincipalConstraint{OrgIDs: []any{"org-2"}},
			},
			want: ImpersonationReasonSubjectDenied,
		},
		{
			name: "matching id",
			policy: ImpersonationPolicy{
				Subject: ImpersonationPrincipalConstraint{IDs: []any{"subject-1"}},
			},
			want:   ImpersonationReasonAllowed,
			wantOK: true,
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			request := base
			if tt.mutate != nil {
				tt.mutate(&request)
			}

			evaluation := tt.policy.Evaluate(request)
			if evaluation.Allowed != tt.wantOK {
				t.Fatalf("Evaluate() allowed = %v, want %v: %#v", evaluation.Allowed, tt.wantOK, evaluation)
			}
			if evaluation.Reason != tt.want {
				t.Fatalf("Evaluate() reason = %q, want %q", evaluation.Reason, tt.want)
			}
		})
	}
}

func TestImpersonationPolicyDeniesSelfAndOrgMismatch(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 15, 0, 0, 0, time.UTC)
	self := ImpersonationRequest{
		Actor:       ImpersonationPrincipal{Kind: "user", ID: int64(10), OrgID: int64(20)},
		Subject:     ImpersonationPrincipal{Kind: "user", ID: int64(10), OrgID: int64(20)},
		RequestedAt: now,
		ExpiresAt:   now.Add(10 * time.Minute),
	}
	if evaluation := (ImpersonationPolicy{}).Evaluate(self); evaluation.Reason != ImpersonationReasonSelfDenied {
		t.Fatalf("self Evaluate() = %#v, want self_denied", evaluation)
	}
	if evaluation := (ImpersonationPolicy{AllowSelf: true}).Evaluate(self); !evaluation.Allowed {
		t.Fatalf("allow-self Evaluate() allowed = false, want true: %#v", evaluation)
	}

	orgMismatch := self
	orgMismatch.Actor.ID = int64(11)
	orgMismatch.Subject.OrgID = int64(21)
	evaluation := (ImpersonationPolicy{RequireSameOrg: true}).Evaluate(orgMismatch)
	if evaluation.Allowed {
		t.Fatalf("org mismatch Evaluate() allowed = true, want false: %#v", evaluation)
	}
	if evaluation.Reason != ImpersonationReasonOrgMismatch {
		t.Fatalf("org mismatch reason = %q, want %q", evaluation.Reason, ImpersonationReasonOrgMismatch)
	}
}

func TestImpersonationPolicyRequiresReasonAndDurationWithinMax(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 15, 0, 0, 0, time.UTC)
	base := ImpersonationRequest{
		Actor:       ImpersonationPrincipal{Kind: "user", ID: "actor"},
		Subject:     ImpersonationPrincipal{Kind: "user", ID: "subject"},
		Reason:      "ticket",
		RequestedAt: now,
		ExpiresAt:   now.Add(30 * time.Minute),
	}

	noReason := base
	noReason.Reason = "  "
	evaluation := (ImpersonationPolicy{RequireReason: true}).Evaluate(noReason)
	if evaluation.Reason != ImpersonationReasonReasonRequired {
		t.Fatalf("empty reason Evaluate() = %#v, want reason_required", evaluation)
	}

	noDuration := base
	noDuration.RequestedAt = time.Time{}
	evaluation = (ImpersonationPolicy{MaxDuration: time.Hour}).Evaluate(noDuration)
	if evaluation.Reason != ImpersonationReasonDurationRequired {
		t.Fatalf("missing duration Evaluate() = %#v, want duration_required", evaluation)
	}

	evaluation = (ImpersonationPolicy{MaxDuration: 15 * time.Minute}).Evaluate(base)
	if evaluation.Reason != ImpersonationReasonDurationExceeded {
		t.Fatalf("long duration Evaluate() = %#v, want duration_exceeded", evaluation)
	}
	if evaluation.Duration != 30*time.Minute {
		t.Fatalf("long duration = %s, want 30m", evaluation.Duration)
	}
}

func TestImpersonationPolicyEnforcesAllowedSubjectRolesAndScopes(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 15, 0, 0, 0, time.UTC)
	request := ImpersonationRequest{
		Actor: ImpersonationPrincipal{
			Kind: "user",
			ID:   "actor",
		},
		Subject: ImpersonationPrincipal{
			Kind:   "user",
			ID:     "subject",
			Roles:  []string{"customer", "admin"},
			Scopes: []string{"customers:read", "admin:write"},
		},
		RequestedAt: now,
		ExpiresAt:   now.Add(10 * time.Minute),
	}

	roleEvaluation := (ImpersonationPolicy{
		AllowedSubjectRoles: []string{"customer"},
	}).Evaluate(request)
	if roleEvaluation.Reason != ImpersonationReasonRoleDenied {
		t.Fatalf("role Evaluate() = %#v, want role_denied", roleEvaluation)
	}
	if roleEvaluation.Role != "admin" {
		t.Fatalf("denied role = %q, want admin", roleEvaluation.Role)
	}

	scopeRequest := request
	scopeRequest.Subject.Roles = []string{"customer"}
	scopeEvaluation := (ImpersonationPolicy{
		AllowedSubjectRoles:  []string{"customer"},
		AllowedSubjectScopes: []string{"customers:*"},
	}).Evaluate(scopeRequest)
	if scopeEvaluation.Reason != ImpersonationReasonScopeDenied {
		t.Fatalf("scope Evaluate() = %#v, want scope_denied", scopeEvaluation)
	}
	if scopeEvaluation.Scope != "admin:write" {
		t.Fatalf("denied scope = %q, want admin:write", scopeEvaluation.Scope)
	}
}
