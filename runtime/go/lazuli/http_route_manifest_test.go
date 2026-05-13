package lazuli

import (
	"errors"
	"reflect"
	"testing"
)

func TestNewHTTPRouteManifestSortsNormalizesAndCopiesRoutes(t *testing.T) {
	source := []HTTPRoute{
		{
			Method:     "post",
			Path:       "api/v1/q/customers.search",
			Name:       "customers.search",
			Middleware: []string{"auth.session", "audit.log"},
		},
		{
			Method:     "GET",
			Path:       "customers/:id",
			Name:       "customers.show",
			Middleware: []string{" auth.session "},
		},
		{
			Method: "GET",
			Path:   "/",
			Name:   "home.index",
		},
	}

	manifest, err := NewHTTPRouteManifest(source)
	if err != nil {
		t.Fatalf("NewHTTPRouteManifest returned error: %v", err)
	}

	want := []HTTPRoute{
		{
			Method: "GET",
			Path:   "/",
			Name:   "home.index",
		},
		{
			Method:     "GET",
			Path:       "/customers/{id}",
			Name:       "customers.show",
			Middleware: []string{"auth.session"},
		},
		{
			Method:     "POST",
			Path:       "/api/v1/q/customers.search",
			Name:       "customers.search",
			Middleware: []string{"auth.session", "audit.log"},
		},
	}
	if !reflect.DeepEqual(manifest.Routes, want) {
		t.Fatalf("Routes = %#v, want %#v", manifest.Routes, want)
	}

	source[0].Middleware[0] = "changed"
	if got := manifest.Routes[2].Middleware[0]; got != "auth.session" {
		t.Fatalf("manifest middleware = %q, want copied value", got)
	}
}

func TestNewHTTPRouteManifestIsDeterministic(t *testing.T) {
	left, err := NewHTTPRouteManifest([]HTTPRoute{
		{Method: "POST", Path: "/customers/search", Name: "customers.search"},
		{Method: "GET", Path: "/customers/{id}", Name: "customers.show"},
		{Method: "GET", Path: "/", Name: "home.index"},
	})
	if err != nil {
		t.Fatalf("NewHTTPRouteManifest(left) returned error: %v", err)
	}
	right, err := NewHTTPRouteManifest([]HTTPRoute{
		{Method: "GET", Path: "/", Name: "home.index"},
		{Method: "GET", Path: "/customers/{id}", Name: "customers.show"},
		{Method: "POST", Path: "/customers/search", Name: "customers.search"},
	})
	if err != nil {
		t.Fatalf("NewHTTPRouteManifest(right) returned error: %v", err)
	}

	if !reflect.DeepEqual(left.Routes, right.Routes) {
		t.Fatalf("left routes = %#v, right routes = %#v", left.Routes, right.Routes)
	}
}

func TestHTTPRouteManifestLookup(t *testing.T) {
	manifest, err := NewHTTPRouteManifest([]HTTPRoute{
		{
			Method:     "GET",
			Path:       "/widgets/:id",
			Name:       "widgets.show",
			Middleware: []string{"auth.session"},
		},
		{
			Method: "POST",
			Path:   "/widgets",
			Name:   "widgets.create",
		},
	})
	if err != nil {
		t.Fatalf("NewHTTPRouteManifest returned error: %v", err)
	}

	route, ok := manifest.Lookup(" get ", "widgets/{id}")
	if !ok {
		t.Fatal("Lookup did not find widgets.show")
	}
	if route.Name != "widgets.show" || route.Method != "GET" || route.Path != "/widgets/{id}" {
		t.Fatalf("Lookup route = %#v, want widgets.show GET /widgets/{id}", route)
	}

	route.Middleware[0] = "changed"
	route, ok = manifest.LookupName(" widgets.show ")
	if !ok {
		t.Fatal("LookupName did not find widgets.show")
	}
	if !reflect.DeepEqual(route.Middleware, []string{"auth.session"}) {
		t.Fatalf("LookupName middleware = %#v, want copied middleware", route.Middleware)
	}

	if _, ok := manifest.Lookup("GET", "/missing"); ok {
		t.Fatal("Lookup found missing route")
	}
	if _, ok := manifest.LookupName("widgets.missing"); ok {
		t.Fatal("LookupName found missing route")
	}
}

func TestHTTPRouteManifestRejectsDuplicates(t *testing.T) {
	tests := []struct {
		name   string
		routes []HTTPRoute
	}{
		{
			name: "duplicate method path after normalization",
			routes: []HTTPRoute{
				{Method: "GET", Path: "widgets/:id", Name: "widgets.show"},
				{Method: "get", Path: "/widgets/{id}", Name: "widgets.show.alt"},
			},
		},
		{
			name: "duplicate name",
			routes: []HTTPRoute{
				{Method: "GET", Path: "/widgets/{id}", Name: "widgets.show"},
				{Method: "POST", Path: "/widgets/search", Name: "widgets.show"},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := NewHTTPRouteManifest(tt.routes)
			if !errors.Is(err, ErrInvalidHTTPRouteManifest) {
				t.Fatalf("NewHTTPRouteManifest error = %v, want ErrInvalidHTTPRouteManifest", err)
			}
		})
	}
}

func TestHTTPRouteManifestRejectsInvalidRoutes(t *testing.T) {
	tests := []struct {
		name  string
		route HTTPRoute
	}{
		{
			name:  "empty method",
			route: HTTPRoute{Path: "/widgets", Name: "widgets.list"},
		},
		{
			name:  "invalid method token",
			route: HTTPRoute{Method: "GET POST", Path: "/widgets", Name: "widgets.list"},
		},
		{
			name:  "empty path",
			route: HTTPRoute{Method: "GET", Name: "widgets.list"},
		},
		{
			name:  "absolute URL path",
			route: HTTPRoute{Method: "GET", Path: "https://example.test/widgets", Name: "widgets.list"},
		},
		{
			name:  "query string path",
			route: HTTPRoute{Method: "GET", Path: "/widgets?debug=1", Name: "widgets.list"},
		},
		{
			name:  "empty name",
			route: HTTPRoute{Method: "GET", Path: "/widgets"},
		},
		{
			name:  "name with whitespace",
			route: HTTPRoute{Method: "GET", Path: "/widgets", Name: "widgets list"},
		},
		{
			name:  "empty middleware",
			route: HTTPRoute{Method: "GET", Path: "/widgets", Name: "widgets.list", Middleware: []string{"auth", ""}},
		},
		{
			name:  "middleware with whitespace",
			route: HTTPRoute{Method: "GET", Path: "/widgets", Name: "widgets.list", Middleware: []string{"auth session"}},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := NewHTTPRouteManifest([]HTTPRoute{tt.route})
			if !errors.Is(err, ErrInvalidHTTPRouteManifest) {
				t.Fatalf("NewHTTPRouteManifest error = %v, want ErrInvalidHTTPRouteManifest", err)
			}
		})
	}
}

func TestHTTPRouteManifestValidateDoesNotMutate(t *testing.T) {
	manifest := HTTPRouteManifest{
		Routes: []HTTPRoute{
			{Method: "get", Path: "widgets/:id", Name: "widgets.show"},
		},
	}

	if err := manifest.Validate(); err != nil {
		t.Fatalf("Validate returned error: %v", err)
	}

	got := manifest.Routes[0]
	if got.Method != "get" || got.Path != "widgets/:id" {
		t.Fatalf("Validate mutated route = %#v", got)
	}
}
