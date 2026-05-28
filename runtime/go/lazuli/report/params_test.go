package report

import (
	"context"
	"net/http"
	"net/http/httptest"
	"net/url"
	"testing"

	"lazuli.dev/runtime/lazuli/storage"
)

// W5 GAP-REPORT-01 — request-time report input params.

func TestParseInputsRequiredPresent(t *testing.T) {
	inputs := []Input{
		{Name: "period_start", Type: "Date", Required: true},
		{Name: "period_end", Type: "Date", Required: true},
		{Name: "format", Type: "CSV", Required: false},
	}
	q := url.Values{"period_start": {"2026-01-01"}, "period_end": {"2026-01-31"}}
	params, err := ParseInputs(inputs, q)
	if err != nil {
		t.Fatalf("ParseInputs: unexpected error %v", err)
	}
	if params.Get("period_start") != "2026-01-01" {
		t.Fatalf("period_start = %q, want 2026-01-01", params.Get("period_start"))
	}
	if params.Has("format") {
		t.Fatalf("optional absent param should not be present")
	}
}

func TestParseInputsMissingRequiredErrors(t *testing.T) {
	inputs := []Input{{Name: "period_start", Type: "Date", Required: true}}
	_, err := ParseInputs(inputs, url.Values{})
	if err == nil {
		t.Fatalf("expected MissingInputError for absent required param")
	}
	if me, ok := err.(*MissingInputError); !ok || me.Param != "period_start" {
		t.Fatalf("error = %v, want MissingInputError{period_start}", err)
	}
}

func TestParseInputsRequiredEmptyStringErrors(t *testing.T) {
	inputs := []Input{{Name: "period_start", Type: "Date", Required: true}}
	// Present but empty value still fails the required check.
	_, err := ParseInputs(inputs, url.Values{"period_start": {""}})
	if err == nil {
		t.Fatalf("expected error for empty required param")
	}
}

func TestParamsFromContextRoundTrip(t *testing.T) {
	ctx := WithParams(context.Background(), Params{"period_start": "2026-01-01"})
	got := ParamsFromContext(ctx)
	if got.Get("period_start") != "2026-01-01" {
		t.Fatalf("round-trip lost value: %q", got.Get("period_start"))
	}
	// Unset context yields a non-nil empty Params (no nil-panic on Get).
	empty := ParamsFromContext(context.Background())
	if empty.Has("anything") {
		t.Fatalf("empty params should report nothing present")
	}
}

func TestMountRejectsMissingRequiredInputWith400(t *testing.T) {
	resetRegistryForTest(t)
	Register(
		Contract{
			Name:       "billing_summary",
			Formats:    []Format{CSV},
			Visibility: storage.VisibilityPublic,
			Inputs:     []Input{{Name: "period_start", Type: "Date", Required: true}},
		},
		func(_ context.Context, _ Format) (string, error) {
			return "https://cdn.example.test/billing_summary.csv", nil
		},
	)

	mux := http.NewServeMux()
	Mount(mux)
	srv := httptest.NewServer(mux)
	defer srv.Close()

	// No period_start → 400 before the runner runs.
	resp, err := http.Get(srv.URL + "/api/reports/billing_summary.csv")
	if err != nil {
		t.Fatalf("GET: %v", err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("status = %d, want 400 for missing required input", resp.StatusCode)
	}
}

func TestMountThreadsInputsToRunnerViaContext(t *testing.T) {
	resetRegistryForTest(t)
	var seen string
	Register(
		Contract{
			Name:       "billing_summary",
			Formats:    []Format{CSV},
			Visibility: storage.VisibilityPublic,
			Inputs:     []Input{{Name: "period_start", Type: "Date", Required: true}},
		},
		func(ctx context.Context, _ Format) (string, error) {
			// The SourceFn (or any runner) reads the parsed params off
			// the context the auto-mount route stashed them on.
			seen = ParamsFromContext(ctx).Get("period_start")
			return "https://cdn.example.test/billing_summary.csv", nil
		},
	)

	mux := http.NewServeMux()
	Mount(mux)
	srv := httptest.NewServer(mux)
	defer srv.Close()

	resp, err := http.Get(srv.URL + "/api/reports/billing_summary.csv?period_start=2026-01-01")
	if err != nil {
		t.Fatalf("GET: %v", err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status = %d, want 200", resp.StatusCode)
	}
	if seen != "2026-01-01" {
		t.Fatalf("runner saw period_start = %q, want 2026-01-01", seen)
	}
}
