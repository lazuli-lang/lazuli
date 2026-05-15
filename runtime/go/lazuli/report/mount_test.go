package report

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"lazuli.dev/runtime/lazuli/storage"
)

func resetRegistryForTest(t *testing.T) {
	t.Helper()
	Reset()
	t.Cleanup(Reset)
}

func TestMountSignedReportRedirects(t *testing.T) {
	resetRegistryForTest(t)

	Register(
		Contract{Name: "monthly_audit", Formats: []Format{CSV, XLSX}, Visibility: storage.VisibilitySigned},
		func(_ context.Context, format Format) (string, error) {
			return "https://signed.example.test/" + format.Token() + ".bin", nil
		},
	)

	mux := http.NewServeMux()
	Mount(mux)
	srv := httptest.NewServer(mux)
	defer srv.Close()

	client := &http.Client{CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}

	for _, fmt := range []string{"csv", "xlsx"} {
		resp, err := client.Get(srv.URL + "/api/reports/monthly_audit." + fmt)
		if err != nil {
			t.Fatalf("GET %s: %v", fmt, err)
		}
		if resp.StatusCode != http.StatusFound {
			t.Fatalf("%s: status = %d, want 302", fmt, resp.StatusCode)
		}
		loc := resp.Header.Get("Location")
		want := "https://signed.example.test/" + fmt + ".bin"
		if loc != want {
			t.Fatalf("%s: Location = %q, want %q", fmt, loc, want)
		}
		_ = resp.Body.Close()
	}
}

func TestMountPublicReportReturnsJSON(t *testing.T) {
	resetRegistryForTest(t)

	Register(
		Contract{Name: "open_data", Formats: []Format{CSV}, Visibility: storage.VisibilityPublic},
		func(_ context.Context, _ Format) (string, error) {
			return "https://cdn.example.test/open_data.csv", nil
		},
	)

	mux := http.NewServeMux()
	Mount(mux)
	srv := httptest.NewServer(mux)
	defer srv.Close()

	resp, err := http.Get(srv.URL + "/api/reports/open_data.csv")
	if err != nil {
		t.Fatalf("GET: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status = %d, want 200", resp.StatusCode)
	}
	var payload map[string]string
	if err := json.NewDecoder(resp.Body).Decode(&payload); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if payload["url"] != "https://cdn.example.test/open_data.csv" {
		t.Fatalf("payload[url] = %q, want CDN URL", payload["url"])
	}
}

func TestStubRunnerReturns501(t *testing.T) {
	resetRegistryForTest(t)

	Register(
		Contract{Name: "unwired", Formats: []Format{CSV}, Visibility: storage.VisibilitySigned},
		nil,
	)

	mux := http.NewServeMux()
	Mount(mux)
	srv := httptest.NewServer(mux)
	defer srv.Close()

	resp, err := http.Get(srv.URL + "/api/reports/unwired.csv")
	if err != nil {
		t.Fatalf("GET: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusNotImplemented {
		t.Fatalf("status = %d, want 501", resp.StatusCode)
	}
}

func TestSetRunnerOverridesStub(t *testing.T) {
	resetRegistryForTest(t)

	Register(
		Contract{Name: "monthly_audit", Formats: []Format{CSV}, Visibility: storage.VisibilitySigned},
		nil,
	)

	if !SetRunner("monthly_audit", func(context.Context, Format) (string, error) {
		return "https://signed.example.test/monthly_audit.csv", nil
	}) {
		t.Fatalf("SetRunner returned false for registered name")
	}

	if SetRunner("missing", nil) {
		t.Fatalf("SetRunner returned true for unregistered name")
	}

	mux := http.NewServeMux()
	Mount(mux)
	srv := httptest.NewServer(mux)
	defer srv.Close()

	client := &http.Client{CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}
	resp, err := client.Get(srv.URL + "/api/reports/monthly_audit.csv")
	if err != nil {
		t.Fatalf("GET: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusFound {
		t.Fatalf("status = %d, want 302 after SetRunner", resp.StatusCode)
	}
}

func TestDuplicateRegistrationPanics(t *testing.T) {
	resetRegistryForTest(t)

	Register(Contract{Name: "dup"}, nil)
	defer func() {
		if recover() == nil {
			t.Fatalf("expected panic on duplicate Register")
		}
	}()
	Register(Contract{Name: "dup"}, nil)
}

// R.C.4 — auto-mount enforces Contract.Atoms via the installed
// PolicyChecker hook. The lazuli package wires the real evaluator in
// `report_bridge.go`; tests install a stub to keep the report package
// dependency-free.

func withPolicyChecker(t *testing.T, fn PolicyChecker) {
	t.Helper()
	prev := policyChecker
	t.Cleanup(func() { SetPolicyChecker(prev) })
	SetPolicyChecker(fn)
}

func withRateLimiter(t *testing.T, fn RateLimiter) {
	t.Helper()
	prev := rateLimiter
	t.Cleanup(func() { SetRateLimiter(prev) })
	SetRateLimiter(fn)
}

func TestMountDeniesReportWhenPolicyCheckerFails(t *testing.T) {
	resetRegistryForTest(t)
	withPolicyChecker(t, func(_ *http.Request, atoms []PolicyAtom) error {
		// Stub mimics "must carry rbac.role manager" — deny by default.
		for _, a := range atoms {
			if a.Namespace == "rbac.role" && a.Name == "manager" {
				return &policyDeniedError{"insufficient role"}
			}
		}
		return nil
	})

	Register(
		Contract{
			Name:       "monthly_audit",
			Formats:    []Format{CSV},
			Visibility: storage.VisibilitySigned,
			Atoms: []PolicyAtom{
				{Namespace: "rbac.role", Name: "manager"},
			},
		},
		func(_ context.Context, _ Format) (string, error) {
			return "https://signed.example.test/monthly_audit.csv", nil
		},
	)

	mux := http.NewServeMux()
	Mount(mux)
	srv := httptest.NewServer(mux)
	defer srv.Close()

	resp, err := http.Get(srv.URL + "/api/reports/monthly_audit.csv")
	if err != nil {
		t.Fatalf("GET: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusForbidden {
		t.Fatalf("status = %d, want 403", resp.StatusCode)
	}
}

func TestMountAllowsReportWhenPolicyCheckerPasses(t *testing.T) {
	resetRegistryForTest(t)
	withPolicyChecker(t, func(_ *http.Request, _ []PolicyAtom) error { return nil })

	Register(
		Contract{
			Name:       "monthly_audit",
			Formats:    []Format{CSV},
			Visibility: storage.VisibilitySigned,
			Atoms: []PolicyAtom{
				{Namespace: "rbac.role", Name: "manager"},
			},
		},
		func(_ context.Context, _ Format) (string, error) {
			return "https://signed.example.test/monthly_audit.csv", nil
		},
	)

	mux := http.NewServeMux()
	Mount(mux)
	srv := httptest.NewServer(mux)
	defer srv.Close()

	client := &http.Client{CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}
	resp, err := client.Get(srv.URL + "/api/reports/monthly_audit.csv")
	if err != nil {
		t.Fatalf("GET: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusFound {
		t.Fatalf("status = %d, want 302", resp.StatusCode)
	}
}

func TestMountFailsClosedWhenPolicyCheckerUnset(t *testing.T) {
	resetRegistryForTest(t)
	withPolicyChecker(t, nil) // explicit clear

	Register(
		Contract{
			Name:       "monthly_audit",
			Formats:    []Format{CSV},
			Visibility: storage.VisibilitySigned,
			Atoms: []PolicyAtom{
				{Namespace: "rbac.role", Name: "manager"},
			},
		},
		func(_ context.Context, _ Format) (string, error) {
			return "https://signed.example.test/monthly_audit.csv", nil
		},
	)

	mux := http.NewServeMux()
	Mount(mux)
	srv := httptest.NewServer(mux)
	defer srv.Close()

	resp, err := http.Get(srv.URL + "/api/reports/monthly_audit.csv")
	if err != nil {
		t.Fatalf("GET: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusForbidden {
		t.Fatalf("status = %d, want 403 with no checker installed", resp.StatusCode)
	}
}

func TestMountAppliesRateLimiterWhenDeclared(t *testing.T) {
	resetRegistryForTest(t)
	withPolicyChecker(t, func(*http.Request, []PolicyAtom) error { return nil })

	calls := 0
	withRateLimiter(t, func(limit string, next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			calls++
			if limit != "10 per hour per user" {
				t.Errorf("rate-limit literal = %q, want %q", limit, "10 per hour per user")
			}
			next.ServeHTTP(w, r)
		})
	})

	Register(
		Contract{
			Name:       "open_data",
			Formats:    []Format{CSV},
			Visibility: storage.VisibilityPublic,
			RateLimit:  "10 per hour per user",
		},
		func(_ context.Context, _ Format) (string, error) {
			return "https://cdn.example.test/open_data.csv", nil
		},
	)

	mux := http.NewServeMux()
	Mount(mux)
	srv := httptest.NewServer(mux)
	defer srv.Close()

	resp, err := http.Get(srv.URL + "/api/reports/open_data.csv")
	if err != nil {
		t.Fatalf("GET: %v", err)
	}
	resp.Body.Close()

	if calls != 1 {
		t.Fatalf("rate-limit wrapper invocation count = %d, want 1", calls)
	}
}

type policyDeniedError struct{ msg string }

func (e *policyDeniedError) Error() string { return e.msg }
