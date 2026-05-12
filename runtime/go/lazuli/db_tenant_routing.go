package lazuli

import (
	"context"
	"errors"
	"fmt"
)

var (
	// ErrInvalidDBTenantRoute is returned when a tenant database route has no
	// routing target or contains invalid identifier components.
	ErrInvalidDBTenantRoute = errors.New("lazuli: invalid db tenant route")

	// ErrInvalidDBTenantIdentifier is returned when a database, schema, or SQL
	// identifier falls outside Lazuli's strict generated identifier subset.
	ErrInvalidDBTenantIdentifier = errors.New("lazuli: invalid db tenant identifier")
)

// DBTenantRoute identifies the physical database and/or schema selected for a
// tenant. The zero value is intentionally invalid; omit the context binding for
// global operations that do not use tenant database routing.
type DBTenantRoute struct {
	// Database is the optional physical database name selected before executing
	// generated SQL. Lazuli only validates and carries this value; connection
	// switching remains owned by the database adapter.
	Database string

	// Schema is the optional SQL schema qualifier for generated identifiers.
	Schema string
}

// DBTenantRouteResolver resolves the database/schema route for a tenant.
type DBTenantRouteResolver interface {
	ResolveDBTenantRoute(context.Context, Tenant) (DBTenantRoute, error)
}

// DBTenantRouteResolverFunc adapts a function into DBTenantRouteResolver.
type DBTenantRouteResolverFunc func(context.Context, Tenant) (DBTenantRoute, error)

// ResolveDBTenantRoute calls f with ctx and tenant.
func (f DBTenantRouteResolverFunc) ResolveDBTenantRoute(ctx context.Context, tenant Tenant) (DBTenantRoute, error) {
	if f == nil {
		return DBTenantRoute{}, errors.New("lazuli: nil db tenant route resolver")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	return f(ctx, tenant)
}

type dbTenantRouteContextKey struct{}

// WithDBTenantRoute returns a child context carrying route as the resolved
// tenant database/schema route.
func WithDBTenantRoute(ctx context.Context, route DBTenantRoute) context.Context {
	if ctx == nil {
		ctx = context.Background()
	}
	return context.WithValue(ctx, dbTenantRouteContextKey{}, route)
}

// DBTenantRouteFromContext reads the active tenant database/schema route from
// ctx.
func DBTenantRouteFromContext(ctx context.Context) (DBTenantRoute, bool) {
	if ctx == nil {
		return DBTenantRoute{}, false
	}
	route, ok := ctx.Value(dbTenantRouteContextKey{}).(DBTenantRoute)
	return route, ok
}

// ValidateDBTenantRoute validates the route's database and schema components.
// A valid route must select at least one routing target.
func ValidateDBTenantRoute(route DBTenantRoute) error {
	if route.Database == "" && route.Schema == "" {
		return fmt.Errorf("%w: database or schema required", ErrInvalidDBTenantRoute)
	}
	if err := validateOptionalDBTenantIdentifier("database", route.Database); err != nil {
		return fmt.Errorf("%w: %w", ErrInvalidDBTenantRoute, err)
	}
	if err := validateOptionalDBTenantIdentifier("schema", route.Schema); err != nil {
		return fmt.Errorf("%w: %w", ErrInvalidDBTenantRoute, err)
	}
	return nil
}

// BuildDBTenantQualifiedIdentifier quotes identifier and qualifies it with
// route.Schema when a schema route is present. The route and identifier are
// strictly validated before SQL is returned.
func BuildDBTenantQualifiedIdentifier(route DBTenantRoute, identifier string) (string, error) {
	if err := ValidateDBTenantRoute(route); err != nil {
		return "", err
	}

	quotedIdentifier, err := quoteDBTenantIdentifier("identifier", identifier)
	if err != nil {
		return "", err
	}
	if route.Schema == "" {
		return quotedIdentifier, nil
	}

	quotedSchema, err := quoteDBTenantIdentifier("schema", route.Schema)
	if err != nil {
		return "", err
	}
	return quotedSchema + "." + quotedIdentifier, nil
}

// BuildDBSchemaQualifiedIdentifier quotes schema and identifier as a
// schema-qualified SQL identifier. Each component must be a strict generated
// identifier; dotted or pre-quoted input is rejected.
func BuildDBSchemaQualifiedIdentifier(schema, identifier string) (string, error) {
	quotedSchema, err := quoteDBTenantIdentifier("schema", schema)
	if err != nil {
		return "", err
	}
	quotedIdentifier, err := quoteDBTenantIdentifier("identifier", identifier)
	if err != nil {
		return "", err
	}
	return quotedSchema + "." + quotedIdentifier, nil
}

func validateOptionalDBTenantIdentifier(kind, identifier string) error {
	if identifier == "" {
		return nil
	}
	_, err := quoteDBTenantIdentifier(kind, identifier)
	return err
}

func quoteDBTenantIdentifier(kind, identifier string) (string, error) {
	if !validDBTenantIdentifier(identifier) {
		return "", fmt.Errorf("%w: %s %q", ErrInvalidDBTenantIdentifier, kind, identifier)
	}
	return `"` + identifier + `"`, nil
}

func validDBTenantIdentifier(identifier string) bool {
	if identifier == "" {
		return false
	}
	for i := 0; i < len(identifier); i++ {
		c := identifier[i]
		if i == 0 {
			if !isDBTenantIdentifierLetter(c) && c != '_' {
				return false
			}
			continue
		}
		if !isDBTenantIdentifierLetter(c) && !isDBTenantIdentifierDigit(c) && c != '_' {
			return false
		}
	}
	return true
}

func isDBTenantIdentifierLetter(c byte) bool {
	return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')
}

func isDBTenantIdentifierDigit(c byte) bool {
	return c >= '0' && c <= '9'
}
