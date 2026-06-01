package lazuli

import "context"

// DefaultTenantResolverFn returns a Tenant when the request carries no
// session-bound tenant. Closes WAR-RUNTIME-MULTITENANT-01 — public-policy
// commands that need to write tenant-scoped data (e.g. user sign-up
// creating a User row in an Org) can resolve the tenant via a user-
// authored fn registered once at boot via [WithDefaultTenant].
//
// Single-tenant apps register a resolver that returns the same Tenant
// row every call (idempotent SELECT / INSERT against a `default` slug
// or environment-pinned org id). Multi-tenant apps either rely on
// session-bound tenant (so this resolver is never invoked) or read a
// tenant key from request context — Host header, subdomain, query
// param — and resolve from there.
//
// Returning (nil, nil) leaves ctx.Tenant unset; a subsequent `creates`
// against a tenancy-org resource will then fail at INSERT time (same
// behaviour as before this hook existed).
//
// The resolver runs INSIDE the command pipeline, after policy + rate
// limit + validators pass, before the typed effect (CreatesEffect /
// UpdatesEffect / DeletesEffect) is applied. The resolver SHOULD NOT
// open its own transaction — the active tx is reachable via the same
// patterns user-authored handlers use (`lazuli.DB()`).
type DefaultTenantResolverFn func(ctx context.Context) (*Tenant, error)

// defaultTenantResolver is the singleton registered via [WithDefaultTenant].
// Package-private; not exposed in the public surface.
var defaultTenantResolver DefaultTenantResolverFn

// WithDefaultTenant registers a default-tenant resolver for the
// runtime. Call once at boot, before [Mux] is built. Idempotent — the
// last registration wins.
//
// Example (single-tenant Acme shape):
//
//	func main() {
//	    lazuli.WithDefaultTenant(func(ctx context.Context) (*lazuli.Tenant, error) {
//	        var id lazuli.ID
//	        err := lazuli.DB().QueryRow(ctx,
//	            `SELECT id FROM "org" WHERE slug = 'default' LIMIT 1`,
//	        ).Scan(&id)
//	        if errors.Is(err, pgx.ErrNoRows) {
//	            err = lazuli.DB().QueryRow(ctx,
//	                `INSERT INTO "org" (name, slug) VALUES ('Acme Co', 'default') RETURNING id`,
//	            ).Scan(&id)
//	        }
//	        if err != nil {
//	            return nil, err
//	        }
//	        return &lazuli.Tenant{OrgID: id}, nil
//	    })
//	    lazuli.Boot(lazuli.Mux())
//	}
func WithDefaultTenant(fn DefaultTenantResolverFn) {
	defaultTenantResolver = fn
}

// TenantOrgID returns the active tenant's OrgID, or the zero ID when the
// context carries no tenant (e.g. `scope global` operations or an unresolved
// anonymous request). Generated referential-guard call sites (Spec 0014) read
// the tenant id through this nil-safe accessor so the emitted delete-command
// wiring never panics on a nil `ctx.Tenant`; the guard's `EXISTS` simply
// matches against org 0, which no live row carries.
func TenantOrgID(ctx *Ctx) ID {
	if ctx == nil || ctx.Tenant == nil {
		return 0
	}
	return ctx.Tenant.OrgID
}

// resolveDefaultTenant runs the registered resolver, if any. Used by
// the runtime command pipeline before applying a `creates` effect on
// a tenancy-org resource when ctx.Tenant is nil. Returns (nil, nil)
// when no resolver is registered.
func resolveDefaultTenant(ctx context.Context) (*Tenant, error) {
	if defaultTenantResolver == nil {
		return nil, nil
	}
	return defaultTenantResolver(ctx)
}
