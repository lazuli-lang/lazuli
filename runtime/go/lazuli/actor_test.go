package lazuli

import (
	"errors"
	"testing"
)

func TestRequireActor_ReturnsErrorWhenNoUser(t *testing.T) {
	ctx := &Ctx{}
	user, err := RequireActor(ctx)
	if err == nil {
		t.Fatal("expected error, got nil")
	}
	if user != nil {
		t.Errorf("expected nil user, got %+v", user)
	}
	if !errors.Is(err, ErrNotAuthenticated) {
		t.Errorf("expected errors.Is(err, ErrNotAuthenticated), got %v", err)
	}
	var envelope *Error
	if !errors.As(err, &envelope) {
		t.Fatalf("expected error to wrap *Error, got %T", err)
	}
	if envelope.Status != 401 {
		t.Errorf("expected status 401, got %d", envelope.Status)
	}
	if envelope.Code != CodeNotAuthenticated {
		t.Errorf("expected code %q, got %q", CodeNotAuthenticated, envelope.Code)
	}
}

func TestRequireActor_NilContext(t *testing.T) {
	user, err := RequireActor(nil)
	if err == nil {
		t.Fatal("expected error, got nil")
	}
	if user != nil {
		t.Errorf("expected nil user, got %+v", user)
	}
}

func TestRequireActor_ReturnsUser(t *testing.T) {
	want := &User{ID: 42, OrgID: 7, Email: "x@y.z", Roles: []string{"host"}}
	ctx := &Ctx{User: want}
	got, err := RequireActor(ctx)
	if err != nil {
		t.Fatalf("expected nil error, got %v", err)
	}
	if got != want {
		t.Errorf("expected same pointer, got %p vs %p", got, want)
	}
}

func TestRequireRole_PassesWhenMatch(t *testing.T) {
	user := &User{ID: 1, OrgID: 1, Roles: []string{"host", "operator"}}
	ctx := &Ctx{User: user}
	got, err := RequireRole(ctx, "host")
	if err != nil {
		t.Fatalf("expected nil, got %v", err)
	}
	if got != user {
		t.Errorf("expected same user pointer")
	}
}

func TestRequireRole_FailsWhenMissing(t *testing.T) {
	user := &User{ID: 1, OrgID: 1, Roles: []string{"traveler"}}
	ctx := &Ctx{User: user}
	got, err := RequireRole(ctx, "host")
	if err == nil {
		t.Fatal("expected error, got nil")
	}
	if got != nil {
		t.Errorf("expected nil user on failure, got %+v", got)
	}
	if !errors.Is(err, ErrRoleRequired) {
		t.Errorf("expected errors.Is(err, ErrRoleRequired), got %v", err)
	}
	var envelope *Error
	if !errors.As(err, &envelope) {
		t.Fatalf("expected *Error wrap, got %T", err)
	}
	if envelope.Status != 403 {
		t.Errorf("expected status 403, got %d", envelope.Status)
	}
	if envelope.Code != CodePolicyDenied {
		t.Errorf("expected code %q, got %q", CodePolicyDenied, envelope.Code)
	}
}

func TestRequireRole_PropagatesNotAuthenticated(t *testing.T) {
	got, err := RequireRole(&Ctx{}, "host")
	if err == nil {
		t.Fatal("expected error, got nil")
	}
	if got != nil {
		t.Errorf("expected nil user, got %+v", got)
	}
	if !errors.Is(err, ErrNotAuthenticated) {
		t.Errorf("expected NotAuthenticated to propagate, got %v", err)
	}
}

func TestHasRole(t *testing.T) {
	cases := []struct {
		name string
		ctx  *Ctx
		role string
		want bool
	}{
		{"nil ctx", nil, "host", false},
		{"no user", &Ctx{}, "host", false},
		{"empty roles", &Ctx{User: &User{}}, "host", false},
		{"match", &Ctx{User: &User{Roles: []string{"host"}}}, "host", true},
		{"no match", &Ctx{User: &User{Roles: []string{"traveler"}}}, "host", false},
		{"multi role match", &Ctx{User: &User{Roles: []string{"traveler", "host", "operator"}}}, "operator", true},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := HasRole(tc.ctx, tc.role)
			if got != tc.want {
				t.Errorf("got %v want %v", got, tc.want)
			}
		})
	}
}
