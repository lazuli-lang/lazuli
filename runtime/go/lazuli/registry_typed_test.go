package lazuli

import (
	"strings"
	"testing"
)

// freshRegistry returns an isolated Registry so the test doesn't leak
// state into GlobalRegistry (other tests assume it's empty at start of
// process). The registry is process-local; we don't use GlobalRegistry
// directly because that would persist across tests in this package.
func freshRegistry() *Registry {
	return &Registry{
		resources: map[string]*resourceErased{},
		commands:  map[string]commandRegistration{},
		queries:   map[string]queryRegistration{},
		apis:      map[string]apiRegistration{},
	}
}

func TestValidateApiHandlers_empty_registry_returns_nil(t *testing.T) {
	r := freshRegistry()
	if err := r.ValidateApiHandlers(); err != nil {
		t.Fatalf("empty registry must validate clean; got: %v", err)
	}
}

func TestValidateApiHandlers_reports_missing_handler(t *testing.T) {
	r := freshRegistry()
	api := &Api[struct{}, struct{}]{
		Name:    "customer.export",
		Feature: "customer",
		Path:    "/api/customer/export",
	}
	r.RegisterApi(apiRegistration{
		Name:           api.Name,
		Feature:        api.Feature,
		Path:           api.Path,
		HandlerChecker: func() bool { return api.Handler != nil },
	})

	err := r.ValidateApiHandlers()
	if err == nil {
		t.Fatalf("unwired api must surface as error; got nil")
	}
	if !strings.Contains(err.Error(), "customer.export") {
		t.Fatalf("error must name the missing endpoint; got: %v", err)
	}
	if !strings.Contains(err.Error(), "1 api endpoint") {
		t.Fatalf("error must count missing endpoints; got: %v", err)
	}
}

func TestValidateApiHandlers_passes_after_handler_assigned(t *testing.T) {
	r := freshRegistry()
	api := &Api[struct{}, struct{}]{
		Name:    "customer.export",
		Feature: "customer",
		Path:    "/api/customer/export",
	}
	r.RegisterApi(apiRegistration{
		Name:           api.Name,
		Feature:        api.Feature,
		Path:           api.Path,
		HandlerChecker: func() bool { return api.Handler != nil },
	})

	// Simulate user-code wiring the handler AFTER registration. The
	// closure reads the current value of api.Handler each invocation,
	// so registration order vs handler assignment order does not
	// matter — only the value at the time of ValidateApiHandlers().
	api.Handler = func(ctx *Ctx, in struct{}) (struct{}, error) {
		return struct{}{}, nil
	}

	if err := r.ValidateApiHandlers(); err != nil {
		t.Fatalf("wired api must validate clean; got: %v", err)
	}
}

func TestValidateApiHandlers_lists_all_missing_alphabetically(t *testing.T) {
	r := freshRegistry()
	zApi := &Api[struct{}, struct{}]{Name: "z.endpoint", Feature: "z", Path: "/z"}
	aApi := &Api[struct{}, struct{}]{Name: "a.endpoint", Feature: "a", Path: "/a"}
	mApi := &Api[struct{}, struct{}]{Name: "m.endpoint", Feature: "m", Path: "/m"}
	for _, api := range []*Api[struct{}, struct{}]{zApi, aApi, mApi} {
		captured := api
		r.RegisterApi(apiRegistration{
			Name:           captured.Name,
			Feature:        captured.Feature,
			Path:           captured.Path,
			HandlerChecker: func() bool { return captured.Handler != nil },
		})
	}

	err := r.ValidateApiHandlers()
	if err == nil {
		t.Fatalf("expected error listing 3 missing endpoints")
	}
	msg := err.Error()
	aIdx := strings.Index(msg, "a.endpoint")
	mIdx := strings.Index(msg, "m.endpoint")
	zIdx := strings.Index(msg, "z.endpoint")
	if aIdx < 0 || mIdx < 0 || zIdx < 0 {
		t.Fatalf("all three endpoints must appear in error; got: %s", msg)
	}
	if !(aIdx < mIdx && mIdx < zIdx) {
		t.Fatalf("error listing must be alphabetical; got: %s", msg)
	}
}

func TestValidateApiHandlers_skips_handlerless_legacy_registration(t *testing.T) {
	// Legacy direct RegisterApi(apiRegistration{...}) without a
	// HandlerChecker closure must not trigger the diagnostic — the
	// registry has no way to inspect the typed value, so it stays
	// opaque rather than reporting false-positives.
	r := freshRegistry()
	r.RegisterApi(apiRegistration{
		Name:    "legacy.api",
		Feature: "legacy",
		Path:    "/legacy",
	})

	if err := r.ValidateApiHandlers(); err != nil {
		t.Fatalf("legacy registrations must be opaque; got: %v", err)
	}
}

func TestRegisterApiAllowsSameShortNameWhenFeatureQualified(t *testing.T) {
	r := freshRegistry()
	r.RegisterApi(apiRegistration{
		Name:    "feat_a.list",
		Feature: "feat_a",
		Method:  MethodGet,
		Path:    "/a/list",
	})
	r.RegisterApi(apiRegistration{
		Name:    "feat_b.list",
		Feature: "feat_b",
		Method:  MethodGet,
		Path:    "/b/list",
	})

	apis := r.Apis()
	if len(apis) != 2 {
		t.Fatalf("expected two registered APIs, got %d: %#v", len(apis), apis)
	}
	if apis["feat_a.list"].Path != "/a/list" {
		t.Fatalf("feat_a path changed during registration: %#v", apis["feat_a.list"])
	}
	if apis["feat_b.list"].Path != "/b/list" {
		t.Fatalf("feat_b path changed during registration: %#v", apis["feat_b.list"])
	}
}

func TestRegisterApiPanicsOnDuplicateName(t *testing.T) {
	r := freshRegistry()
	r.RegisterApi(apiRegistration{
		Name:    "feat_a.list",
		Feature: "feat_a",
		Method:  MethodGet,
		Path:    "/a/list",
	})

	defer func() {
		rec := recover()
		if rec == nil {
			t.Fatal("expected panic on duplicate API registration, got nil")
		}
		msg, ok := rec.(string)
		if !ok {
			t.Fatalf("expected string panic value, got %T: %v", rec, rec)
		}
		if !strings.Contains(msg, "lazuli: api feat_a.list registered twice") {
			t.Fatalf("panic message does not name the duplicate API: %q", msg)
		}
	}()

	r.RegisterApi(apiRegistration{
		Name:    "feat_a.list",
		Feature: "feat_a",
		Method:  MethodPost,
		Path:    "/a/list-again",
	})
}
