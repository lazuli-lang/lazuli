package lazuli

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
)

type fakeResolver struct {
	wantToken string
	userID    ID
	orgID     ID
	sessionID ID
	found     bool
	err       error
	seen      string
}

func (f *fakeResolver) Resolve(_ context.Context, token string) (ID, ID, ID, bool, error) {
	f.seen = token
	if f.err != nil {
		return 0, 0, 0, false, f.err
	}
	if !f.found || token != f.wantToken {
		return 0, 0, 0, false, nil
	}
	return f.userID, f.orgID, f.sessionID, true, nil
}

func withFakeResolver(t *testing.T, f *fakeResolver) {
	t.Helper()
	prev := sessionResolver
	sessionResolver = f
	t.Cleanup(func() { sessionResolver = prev })
}

func TestPopulateProductionSession_NoResolver_NoOp(t *testing.T) {
	prev := sessionResolver
	sessionResolver = nil
	t.Cleanup(func() { sessionResolver = prev })

	req := httptest.NewRequest(http.MethodPost, "/x", nil)
	req.Header.Set("Authorization", "Bearer abc")
	ctx := &Ctx{Actor: ActorAnonymous}
	populateProductionSession(req, ctx)

	if ctx.Actor != ActorAnonymous || ctx.User != nil {
		t.Fatalf("expected anonymous, got actor=%q user=%+v", ctx.Actor, ctx.User)
	}
}

func TestPopulateProductionSession_BearerHeader(t *testing.T) {
	f := &fakeResolver{wantToken: "tok-1", userID: 7, orgID: 42, sessionID: 99, found: true}
	withFakeResolver(t, f)

	req := httptest.NewRequest(http.MethodPost, "/x", nil)
	req.Header.Set("Authorization", "Bearer tok-1")
	ctx := &Ctx{Actor: ActorAnonymous}
	populateProductionSession(req, ctx)

	if ctx.Actor != ActorUser || ctx.User == nil || ctx.User.ID != 7 || ctx.User.OrgID != 42 {
		t.Fatalf("unexpected ctx state: %+v / user=%+v", ctx, ctx.User)
	}
	if ctx.Tenant == nil || ctx.Tenant.OrgID != 42 {
		t.Fatalf("tenant not populated: %+v", ctx.Tenant)
	}
	if ctx.SessionID != 99 || ctx.SessionToken != "tok-1" {
		t.Fatalf("session id/token not set: id=%d tok=%q", ctx.SessionID, ctx.SessionToken)
	}
}

func TestPopulateProductionSession_CookieWinsOverHeader(t *testing.T) {
	f := &fakeResolver{wantToken: "cookie-tok", userID: 1, orgID: 2, sessionID: 3, found: true}
	withFakeResolver(t, f)

	req := httptest.NewRequest(http.MethodPost, "/x", nil)
	req.AddCookie(&http.Cookie{Name: ProductionSessionCookieName, Value: "cookie-tok"})
	req.Header.Set("Authorization", "Bearer header-tok")
	ctx := &Ctx{Actor: ActorAnonymous}
	populateProductionSession(req, ctx)

	if f.seen != "cookie-tok" {
		t.Fatalf("expected resolver to see cookie token, got %q", f.seen)
	}
	if ctx.User == nil || ctx.User.ID != 1 {
		t.Fatalf("unexpected user: %+v", ctx.User)
	}
}

func TestPopulateProductionSession_UnknownToken_LeavesAnonymous(t *testing.T) {
	f := &fakeResolver{wantToken: "real-tok", found: false}
	withFakeResolver(t, f)

	req := httptest.NewRequest(http.MethodPost, "/x", nil)
	req.Header.Set("Authorization", "Bearer bogus")
	ctx := &Ctx{Actor: ActorAnonymous}
	populateProductionSession(req, ctx)

	if ctx.Actor != ActorAnonymous || ctx.User != nil {
		t.Fatalf("expected anonymous on miss, got actor=%q user=%+v", ctx.Actor, ctx.User)
	}
}

func TestExtractSessionToken_BearerMixedCase(t *testing.T) {
	req := httptest.NewRequest(http.MethodPost, "/x", nil)
	req.Header.Set("Authorization", "bEaReR  spaced-tok  ")
	if got := extractSessionToken(req); got != "spaced-tok" {
		t.Fatalf("expected trimmed token, got %q", got)
	}
}

func TestExtractSessionToken_NoAuth(t *testing.T) {
	req := httptest.NewRequest(http.MethodPost, "/x", nil)
	if got := extractSessionToken(req); got != "" {
		t.Fatalf("expected empty, got %q", got)
	}
}

func TestRegisterSessionResolver_ReplacesPrior(t *testing.T) {
	prev := sessionResolver
	t.Cleanup(func() { sessionResolver = prev })

	first := &fakeResolver{wantToken: "a"}
	RegisterSessionResolver(first)
	if sessionResolver != first {
		t.Fatalf("first registration not applied")
	}
	second := &fakeResolver{wantToken: "b"}
	RegisterSessionResolver(second)
	if sessionResolver != second {
		t.Fatalf("second registration did not replace first")
	}
}
