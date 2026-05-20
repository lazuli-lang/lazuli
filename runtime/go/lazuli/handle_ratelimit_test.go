package lazuli

import (
	"context"
	"errors"
	"testing"
	"time"
)

func TestParseRateLimitSpec(t *testing.T) {
	cases := []struct {
		in        string
		wantLim   int
		wantSec   int
		wantScope string
	}{
		{"30 per 10 minutes per ip", 30, 600, "ip"},
		{"5 per minute per user", 5, 60, "user"},
		{"100 per hour per actor", 100, 3600, "actor"},
	}
	for _, c := range cases {
		spec, err := parseRateLimitSpec(c.in)
		if err != nil {
			t.Fatalf("%q: %v", c.in, err)
		}
		if spec.Limit != c.wantLim {
			t.Errorf("%q: Limit = %d, want %d", c.in, spec.Limit, c.wantLim)
		}
		if spec.Window != time.Duration(c.wantSec)*time.Second {
			t.Errorf("%q: Window = %s, want %ds", c.in, spec.Window, c.wantSec)
		}
		if spec.Scope != c.wantScope {
			t.Errorf("%q: Scope = %q, want %q", c.in, spec.Scope, c.wantScope)
		}
	}
}

func TestRateLimitEnforcedInHandle(t *testing.T) {
	previous := activeStore
	activeStore = newInMemoryStore()
	t.Cleanup(func() { activeStore = previous })

	calls := 0
	cmd := Command[struct{}, struct{}]{
		Name:      "account.login",
		Policy:    Policy{Atoms: []PolicyAtom{{Namespace: "scope", Name: "public"}}},
		RateLimit: "1 per hour per actor",
		Effect: Returns(func(ctx *Ctx, input struct{}) (struct{}, error) {
			calls++
			return struct{}{}, nil
		}),
	}
	ctx := &Ctx{Context: context.Background(), Actor: ActorAnonymous}

	if _, err := cmd.Handle(ctx, struct{}{}); err != nil {
		t.Fatalf("first Handle error = %v", err)
	}
	if _, err := cmd.Handle(ctx, struct{}{}); !errors.Is(err, ErrRateLimited) {
		t.Fatalf("second Handle error = %v, want ErrRateLimited", err)
	}
	if calls != 1 {
		t.Fatalf("handler calls = %d, want 1", calls)
	}
}
