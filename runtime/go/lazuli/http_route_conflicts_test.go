package lazuli

import (
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestNormalizeRouteBindingCanonicalizesGeneratedMetadata(t *testing.T) {
	got, err := NormalizeRouteBinding(RouteBinding{
		Method: " get ",
		Path:   "api//users/",
		Name:   " users.index ",
	})
	if err != nil {
		t.Fatalf("NormalizeRouteBinding() error = %v", err)
	}

	want := RouteBinding{
		Method: "GET",
		Path:   "/api/users/",
		Name:   "users.index",
	}
	if got != want {
		t.Fatalf("NormalizeRouteBinding() = %#v, want %#v", got, want)
	}
}

func TestNormalizeRouteBindingRejectsInvalidMetadata(t *testing.T) {
	tests := []struct {
		name    string
		binding RouteBinding
	}{
		{
			name: "empty method",
			binding: RouteBinding{
				Path: "/users",
				Name: "users.index",
			},
		},
		{
			name: "method with space",
			binding: RouteBinding{
				Method: "GET POST",
				Path:   "/users",
				Name:   "users.index",
			},
		},
		{
			name: "path traversal",
			binding: RouteBinding{
				Method: "GET",
				Path:   "/../users",
				Name:   "users.index",
			},
		},
		{
			name: "path query",
			binding: RouteBinding{
				Method: "GET",
				Path:   "/users?q=1",
				Name:   "users.index",
			},
		},
		{
			name: "empty name",
			binding: RouteBinding{
				Method: "GET",
				Path:   "/users",
			},
		},
		{
			name: "name control",
			binding: RouteBinding{
				Method: "GET",
				Path:   "/users",
				Name:   "users\nindex",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := NormalizeRouteBinding(tt.binding)
			if !errors.Is(err, ErrInvalidRouteBinding) {
				t.Fatalf("NormalizeRouteBinding() error = %v, want ErrInvalidRouteBinding", err)
			}
		})
	}
}

func TestDetectRouteConflictsFindsMethodPathAndNameConflicts(t *testing.T) {
	report, err := DetectRouteConflicts([]RouteBinding{
		{Method: "post", Path: "/orders", Name: "orders.create"},
		{Method: "GET", Path: "/users", Name: "users.index"},
		{Method: "get", Path: "users", Name: "users.duplicate.path"},
		{Method: "DELETE", Path: "/sessions", Name: "users.index"},
	})
	if !errors.Is(err, ErrRouteConflict) {
		t.Fatalf("DetectRouteConflicts() error = %v, want ErrRouteConflict", err)
	}
	if report.OK() {
		t.Fatal("report.OK() = true, want false")
	}

	gotSummaries := report.Summaries()
	wantSummaries := []string{
		`method/path "GET /users" is bound by GET /users (users.duplicate.path), GET /users (users.index)`,
		`name "users.index" is bound by DELETE /sessions (users.index), GET /users (users.index)`,
	}
	if !reflect.DeepEqual(gotSummaries, wantSummaries) {
		t.Fatalf("Summaries() = %#v, want %#v", gotSummaries, wantSummaries)
	}
	if !strings.Contains(err.Error(), wantSummaries[0]) || !strings.Contains(err.Error(), wantSummaries[1]) {
		t.Fatalf("error = %q, want conflict summaries", err.Error())
	}
}

func TestDetectRouteConflictsBuildsMethodNotAllowedMetadata(t *testing.T) {
	report, err := DetectRouteConflicts([]RouteBinding{
		{Method: "POST", Path: "/users", Name: "users.create"},
		{Method: "get", Path: "/users", Name: "users.index"},
		{Method: "PATCH", Path: "/users/{id}", Name: "users.update"},
	})
	if err != nil {
		t.Fatalf("DetectRouteConflicts() error = %v", err)
	}
	if !report.OK() {
		t.Fatal("report.OK() = false, want true")
	}

	wantBindings := []RouteBinding{
		{Method: "GET", Path: "/users", Name: "users.index"},
		{Method: "POST", Path: "/users", Name: "users.create"},
		{Method: "PATCH", Path: "/users/{id}", Name: "users.update"},
	}
	if !reflect.DeepEqual(report.Bindings, wantBindings) {
		t.Fatalf("Bindings = %#v, want %#v", report.Bindings, wantBindings)
	}

	wantMetadata := []RouteMethodNotAllowedMetadata{
		{
			Path:            "/users",
			Methods:         []string{"GET", "HEAD", "POST"},
			Allow:           "GET, HEAD, POST",
			RouteNames:      []string{"users.create", "users.index"},
			HasImplicitHEAD: true,
		},
		{
			Path:       "/users/{id}",
			Methods:    []string{"PATCH"},
			Allow:      "PATCH",
			RouteNames: []string{"users.update"},
		},
	}
	if !reflect.DeepEqual(report.MethodNotAllowed, wantMetadata) {
		t.Fatalf("MethodNotAllowed = %#v, want %#v", report.MethodNotAllowed, wantMetadata)
	}
}

func TestDetectRouteConflictsPreservesExplicitHeadMetadata(t *testing.T) {
	report, err := DetectRouteConflicts([]RouteBinding{
		{Method: "GET", Path: "/", Name: "home.show"},
		{Method: "HEAD", Path: "/", Name: "home.head"},
	})
	if err != nil {
		t.Fatalf("DetectRouteConflicts() error = %v", err)
	}

	if got := report.MethodNotAllowed[0].Allow; got != "GET, HEAD" {
		t.Fatalf("Allow = %q, want GET, HEAD", got)
	}
	if report.MethodNotAllowed[0].HasImplicitHEAD {
		t.Fatal("HasImplicitHEAD = true, want false for explicit HEAD")
	}
}

func TestDetectRouteConflictsRejectsInvalidBindingWithIndex(t *testing.T) {
	_, err := DetectRouteConflicts([]RouteBinding{
		{Method: "GET", Path: "/ok", Name: "ok"},
		{Method: "GET", Path: "/bad\npath", Name: "bad"},
	})
	if !errors.Is(err, ErrInvalidRouteBinding) {
		t.Fatalf("DetectRouteConflicts() error = %v, want ErrInvalidRouteBinding", err)
	}
	if !strings.Contains(err.Error(), "route 1") {
		t.Fatalf("DetectRouteConflicts() error = %v, want route index", err)
	}
}
