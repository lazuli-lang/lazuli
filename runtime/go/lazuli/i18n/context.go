package i18n

import "context"

// localeContextKey is the typed key used to stash the negotiated
// locale in the request context. Defined as a private type so other
// packages cannot accidentally collide.
type localeContextKey struct{}

// WithLocale returns a new context carrying the negotiated locale.
// Generated handlers call `i18n.LocaleFrom(ctx)` to read it.
func WithLocale(ctx context.Context, tag string) context.Context {
	return context.WithValue(ctx, localeContextKey{}, tag)
}

// LocaleFrom reads the negotiated locale from the context. Returns
// an empty string when no locale has been negotiated (the middleware
// was not mounted).
func LocaleFrom(ctx context.Context) string {
	if v, ok := ctx.Value(localeContextKey{}).(string); ok {
		return v
	}
	return ""
}
