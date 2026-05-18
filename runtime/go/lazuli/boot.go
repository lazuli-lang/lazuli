package lazuli

import (
	"context"

	"github.com/jackc/pgx/v5/pgxpool"

	"lazuli.dev/runtime/lazuli/i18n"
)

// init wires the framework's built-in error catalog (Cell RUNTIME-2,
// proposal §11) as the process-global error resolver. The `i18n`
// package's own init already installs `NewDefaultResolver` (built-in
// PT-BR + en-US on); this call is the *explicit* boot-side
// declaration so the wiring is discoverable from the root package's
// boot surface — the natural place an operator greps when chasing
// "where does the resolver come from?".
//
// Idempotent: calling `i18n.SetDefaultResolver` with the same
// pre-wired resolver is a no-op for behaviour. Tests that need the
// un-wired path call `i18n.SetDefaultResolver(nil)` directly.
func init() {
	i18n.SetDefaultResolver(i18n.NewDefaultResolver())
}

type envContextKey struct{}
type errorSourceContextKey struct{}
type panicRecoverContextKey struct{}

// SetEnvironment installs the active environment name (for example
// "dev"). Panic recovery reads it to decide whether Source is returned
// in HTTP error envelopes.
func SetEnvironment(ctx context.Context, env string) context.Context {
	return context.WithValue(ctx, envContextKey{}, env)
}

// EnvironmentFromContext returns the active Lazuli environment.
func EnvironmentFromContext(ctx context.Context) string {
	env, _ := ctx.Value(envContextKey{}).(string)
	return env
}

// SetObservabilityPolicy installs app.observability projection policy
// onto a context. The HTTP server wire derives request contexts from
// this base.
func SetObservabilityPolicy(ctx context.Context, errorSources []string) context.Context {
	copied := append([]string(nil), errorSources...)
	return context.WithValue(ctx, errorSourceContextKey{}, copied)
}

// ObservabilityErrorSourcesFromContext returns the environments where
// lazuli.ErrorBase.Source may be projected to response bodies.
func ObservabilityErrorSourcesFromContext(ctx context.Context) []string {
	allowed, _ := ctx.Value(errorSourceContextKey{}).([]string)
	return allowed
}

// SetPanicRecoverPolicy installs whether runtime panic guards should
// swallow panics. The default is true when unset.
func SetPanicRecoverPolicy(ctx context.Context, recoverPanics bool) context.Context {
	return context.WithValue(ctx, panicRecoverContextKey{}, recoverPanics)
}

// PanicRecoverFromContext returns whether panic guards should recover.
func PanicRecoverFromContext(ctx context.Context) bool {
	recoverPanics, ok := ctx.Value(panicRecoverContextKey{}).(bool)
	if !ok {
		return true
	}
	return recoverPanics
}

// RegisterErrorRendererEscape installs a process-global, locale-aware
// error-renderer that runs BEFORE the four-layer resolver chain
// (proposal §9.4 + Cell RUNTIME-3). If `fn` returns a non-empty
// string, that text wins on the wire; if it returns empty, the chain
// proceeds as normal.
//
// Reserved for white-label SaaS hosts and OEM rebrand scenarios that
// need full control over every byte of the error payload (legal
// compliance, brand voice enforcement, per-tenant string overrides).
// Normal apps should NOT use this — the declarative .lzi `errors`
// block + `when_denied @translation.<key>` surface is the
// cold-readable path and the one doctor diagnostics audit. See
// `docs/anti-patterns/error-renderer-escape.md` for the [do | don't]
// table and the list of doctor diagnostics that go silent when the
// escape is installed.
//
// Idempotent: the last registration wins. Pass nil to clear and
// resume the normal chain.
func RegisterErrorRendererEscape(fn func(*Error, string) string) {
	if fn == nil {
		i18n.SetEscapeRenderer(nil)
		return
	}
	i18n.SetEscapeRenderer(func(req i18n.ErrorRequest) string {
		le, _ := req.Source.(*Error)
		return fn(le, req.Locale)
	})
}

// connectDB opens a Postgres pool against the given URL.
func connectDB(ctx context.Context, dbURL string) (*pgxpool.Pool, error) {
	cfg, err := pgxpool.ParseConfig(dbURL)
	if err != nil {
		return nil, err
	}
	pool, err := pgxpool.NewWithConfig(ctx, cfg)
	if err != nil {
		return nil, err
	}
	if err := pool.Ping(ctx); err != nil {
		pool.Close()
		return nil, err
	}
	return pool, nil
}
