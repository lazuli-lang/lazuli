package lazuli

import (
	"context"
	"errors"
	"testing"
)

func TestWithDBTenantRouteStoresRouteInContext(t *testing.T) {
	want := DBTenantRoute{
		Database: "tenant_db_42",
		Schema:   "tenant_42",
	}

	ctx := WithDBTenantRoute(t.Context(), want)
	got, ok := DBTenantRouteFromContext(ctx)
	if !ok {
		t.Fatal("DBTenantRouteFromContext(ctx) ok = false, want true")
	}
	if got != want {
		t.Fatalf("DBTenantRouteFromContext(ctx) = %#v, want %#v", got, want)
	}
}

func TestDBTenantRouteFromContextReturnsFalseWhenAbsent(t *testing.T) {
	if got, ok := DBTenantRouteFromContext(t.Context()); ok || got != (DBTenantRoute{}) {
		t.Fatalf("DBTenantRouteFromContext(empty) = %#v, %v; want zero, false", got, ok)
	}
	if got, ok := DBTenantRouteFromContext(nil); ok || got != (DBTenantRoute{}) {
		t.Fatalf("DBTenantRouteFromContext(nil) = %#v, %v; want zero, false", got, ok)
	}
}

func TestDBTenantRouteResolverFuncDelegates(t *testing.T) {
	ctx := context.WithValue(t.Context(), dbTenantRouteTestContextKey{}, "ctx")
	tenant := Tenant{OrgID: 42}
	want := DBTenantRoute{Schema: "tenant_42"}

	var gotCtx context.Context
	var gotTenant Tenant
	resolver := DBTenantRouteResolverFunc(func(ctx context.Context, tenant Tenant) (DBTenantRoute, error) {
		gotCtx = ctx
		gotTenant = tenant
		return want, nil
	})

	got, err := resolver.ResolveDBTenantRoute(ctx, tenant)
	if err != nil {
		t.Fatalf("ResolveDBTenantRoute returned error: %v", err)
	}
	if got != want {
		t.Fatalf("ResolveDBTenantRoute returned %#v, want %#v", got, want)
	}
	if gotCtx != ctx {
		t.Fatal("resolver received different context")
	}
	if gotTenant != tenant {
		t.Fatalf("resolver tenant = %#v, want %#v", gotTenant, tenant)
	}
}

func TestDBTenantRouteResolverFuncRejectsNil(t *testing.T) {
	var resolver DBTenantRouteResolverFunc

	if route, err := resolver.ResolveDBTenantRoute(t.Context(), Tenant{OrgID: 1}); err == nil {
		t.Fatalf("ResolveDBTenantRoute returned nil error with route %#v", route)
	}
}

func TestValidateDBTenantRoute(t *testing.T) {
	for _, route := range []DBTenantRoute{
		{Database: "tenant_db_42"},
		{Schema: "tenant_42"},
		{Database: "TenantDB_42", Schema: "_tenant_42"},
	} {
		if err := ValidateDBTenantRoute(route); err != nil {
			t.Fatalf("ValidateDBTenantRoute(%#v) returned %v", route, err)
		}
	}
}

func TestValidateDBTenantRouteRejectsInvalidRoutes(t *testing.T) {
	tests := []struct {
		name  string
		route DBTenantRoute
	}{
		{name: "empty", route: DBTenantRoute{}},
		{name: "bad database", route: DBTenantRoute{Database: "tenant-db"}},
		{name: "bad schema", route: DBTenantRoute{Schema: "1tenant"}},
		{name: "dotted schema", route: DBTenantRoute{Schema: "app.tenant"}},
		{name: "quoted schema", route: DBTenantRoute{Schema: `"tenant"`}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateDBTenantRoute(tt.route)
			if !errors.Is(err, ErrInvalidDBTenantRoute) {
				t.Fatalf("ValidateDBTenantRoute(%#v) error = %v, want ErrInvalidDBTenantRoute", tt.route, err)
			}
		})
	}
}

func TestBuildDBTenantQualifiedIdentifier(t *testing.T) {
	tests := []struct {
		name       string
		route      DBTenantRoute
		identifier string
		want       string
	}{
		{
			name:       "schema route",
			route:      DBTenantRoute{Schema: "tenant_42"},
			identifier: "orders",
			want:       `"tenant_42"."orders"`,
		},
		{
			name:       "database route",
			route:      DBTenantRoute{Database: "tenant_db_42"},
			identifier: "orders",
			want:       `"orders"`,
		},
		{
			name:       "database and schema route",
			route:      DBTenantRoute{Database: "tenant_db_42", Schema: "tenant_42"},
			identifier: "_events_2",
			want:       `"tenant_42"."_events_2"`,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := BuildDBTenantQualifiedIdentifier(tt.route, tt.identifier)
			if err != nil {
				t.Fatalf("BuildDBTenantQualifiedIdentifier returned error: %v", err)
			}
			if got != tt.want {
				t.Fatalf("BuildDBTenantQualifiedIdentifier = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestBuildDBSchemaQualifiedIdentifier(t *testing.T) {
	got, err := BuildDBSchemaQualifiedIdentifier("tenant_42", "orders")
	if err != nil {
		t.Fatalf("BuildDBSchemaQualifiedIdentifier returned error: %v", err)
	}
	if want := `"tenant_42"."orders"`; got != want {
		t.Fatalf("BuildDBSchemaQualifiedIdentifier = %q, want %q", got, want)
	}
}

func TestDBTenantIdentifierBuildersRejectInvalidInput(t *testing.T) {
	tests := []struct {
		name string
		run  func() error
		want error
	}{
		{
			name: "empty route",
			run: func() error {
				_, err := BuildDBTenantQualifiedIdentifier(DBTenantRoute{}, "orders")
				return err
			},
			want: ErrInvalidDBTenantRoute,
		},
		{
			name: "invalid route database",
			run: func() error {
				_, err := BuildDBTenantQualifiedIdentifier(DBTenantRoute{Database: "tenant-db", Schema: "tenant_42"}, "orders")
				return err
			},
			want: ErrInvalidDBTenantIdentifier,
		},
		{
			name: "dotted identifier",
			run: func() error {
				_, err := BuildDBTenantQualifiedIdentifier(DBTenantRoute{Schema: "tenant_42"}, "public.orders")
				return err
			},
			want: ErrInvalidDBTenantIdentifier,
		},
		{
			name: "identifier starts with digit",
			run: func() error {
				_, err := BuildDBSchemaQualifiedIdentifier("tenant_42", "1orders")
				return err
			},
			want: ErrInvalidDBTenantIdentifier,
		},
		{
			name: "schema punctuation",
			run: func() error {
				_, err := BuildDBSchemaQualifiedIdentifier("tenant-42", "orders")
				return err
			},
			want: ErrInvalidDBTenantIdentifier,
		},
		{
			name: "unicode schema",
			run: func() error {
				_, err := BuildDBSchemaQualifiedIdentifier("tenánt", "orders")
				return err
			},
			want: ErrInvalidDBTenantIdentifier,
		},
		{
			name: "injection",
			run: func() error {
				_, err := BuildDBSchemaQualifiedIdentifier(`tenant"; DROP SCHEMA public; --`, "orders")
				return err
			},
			want: ErrInvalidDBTenantIdentifier,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := tt.run(); !errors.Is(err, tt.want) {
				t.Fatalf("%s error = %v, want %v", tt.name, err, tt.want)
			}
		})
	}
}

type dbTenantRouteTestContextKey struct{}
