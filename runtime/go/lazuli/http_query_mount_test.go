package lazuli

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// TestQueryMountPathDropsQueryInfix locks the wire contract that
// cell B1 established: queries are mounted at
// `POST /api/v1/q/<feature>.<query_name>` with NO `.query.` infix.
//
// This is a runtime-only test — it does not exercise codegen. It
// directly constructs a `Query[A, R]` with `Name:
// "account.lookup_my_user"`, registers it, and probes the resulting
// `Mux()` for both the new and the legacy path shapes.
//
// The legacy path (`/api/v1/q/account.query.lookup_my_user`) MUST
// return 404, proving the framework no longer mounts that historical
// shape and any client still sending it gets a clean break.
func TestQueryMountPathDropsQueryInfix(t *testing.T) {
	installZeroAuthoringFixture(t)
	t.Cleanup(clearRegistriesForTest())

	// A minimal policy-denied query. The dispatcher refuses the
	// anonymous request at the policy layer (before RunList) so we
	// never need a real SQL backend. All we care about is the route
	// being mounted: the response code is 403 (mounted + denied),
	// NOT 404 (unmounted).
	query := &Query[struct{}, struct{}]{
		Name: "account.lookup_my_user",
		Kind: QueryLookup,
		Policy: Policy{
			Name: "@policy.authenticated",
			Atoms: []PolicyAtom{
				{Namespace: "scope", Name: "authenticated"},
			},
		},
		WithSource: func(ctx context.Context) context.Context {
			return WithSource(ctx, SourceTag{
				Capsule: "demo",
				Feature: "account",
				Kind:    "query",
				Op:      "lookup_my_user",
			})
		},
	}
	Register(query)

	handler := Mux()

	// Probe 1: NEW shape — must be mounted.
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/q/account.lookup_my_user", strings.NewReader("{}"))
	req.Header.Set("Content-Type", "application/json")
	handler.ServeHTTP(rec, req)

	if rec.Code == http.StatusNotFound {
		t.Fatalf("new query path was NOT mounted: POST /api/v1/q/account.lookup_my_user returned 404; body: %s", rec.Body.String())
	}
	// Any non-404 status is acceptable here: this cell proves the
	// route is wired, not what the route returns. Policy denial (403)
	// is the expected outcome for an anonymous probe, but a future
	// dispatcher change that returns 4xx/2xx for other reasons still
	// satisfies the mount contract.
	if rec.Code < 200 || rec.Code >= 500 {
		t.Fatalf("new query path mounted but returned unexpected status %d; body: %s", rec.Code, rec.Body.String())
	}

	// Probe 2: LEGACY shape — must be 404. This is the load-bearing
	// half of the test. If a future codegen refactor reintroduces
	// the `.query.` infix, the registered Name becomes
	// `account.query.lookup_my_user` and this assertion silently
	// passes. To guard against that drift we ALSO assert the new
	// path works above; the pair together fails fast on either
	// regression direction.
	rec = httptest.NewRecorder()
	req = httptest.NewRequest(http.MethodPost, "/api/v1/q/account.query.lookup_my_user", strings.NewReader("{}"))
	req.Header.Set("Content-Type", "application/json")
	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusNotFound {
		t.Fatalf("legacy query path with `.query.` infix is still mounted: POST /api/v1/q/account.query.lookup_my_user returned %d (want 404); body: %s", rec.Code, rec.Body.String())
	}
}

// TestQueryRegisterPanicsOnDuplicateName locks the registry's
// existing collision guard (`register.go:101`). Two queries with the
// same Name must panic at register time, not silently overwrite. If a
// future change weakens this guard, codegen regressions (e.g. two
// features both lowering to `account.lookup_my_user`) would corrupt
// the wire contract without warning.
func TestQueryRegisterPanicsOnDuplicateName(t *testing.T) {
	t.Cleanup(clearRegistriesForTest())

	first := &Query[struct{}, struct{}]{
		Name: "duplicate_probe.same_name",
		Kind: QueryList,
	}
	second := &Query[struct{}, struct{}]{
		Name: "duplicate_probe.same_name",
		Kind: QueryList,
	}

	Register(first)

	defer func() {
		r := recover()
		if r == nil {
			t.Fatal("expected panic on duplicate query registration, got nil")
		}
		msg, ok := r.(string)
		if !ok {
			t.Fatalf("expected string panic value, got %T: %v", r, r)
		}
		if !strings.Contains(msg, "duplicate_probe.same_name") {
			t.Fatalf("panic message does not name the offending query: %q", msg)
		}
		if !strings.Contains(msg, "registered twice") {
			t.Fatalf("panic message is not the expected collision shape: %q", msg)
		}
	}()
	Register(second)
}
