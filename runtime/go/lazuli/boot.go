package lazuli

import (
	"context"

	"github.com/jackc/pgx/v5/pgxpool"
)

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
