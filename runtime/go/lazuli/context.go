package lazuli

import "context"

// SourceTag identifies the originating .lzi op for any error constructed
// during the lifetime of a request context. Codegen-emitted handlers stamp the
// tag on entry; runtime error constructors read it back to populate ErrorBase
// fields.
//
// Authors do not construct SourceTag manually; codegen owns this.
//
// EXPERIMENTAL: shape may add fields before 1.0.
type SourceTag struct {
	Capsule string // app capsule name, e.g. "crm"
	Feature string // feature name, e.g. "customer"
	Kind    string // "command" | "query" | "job" | "webhook" | "notification" | "auth"
	Op      string // op name within the kind, e.g. "create_customer"
	Source  string // ".lzi:line:col", e.g. "features/customer.lzi:42:1"
}

type sourceTagKey struct{}

// WithSource attaches a SourceTag to the context. Repeated calls overwrite the
// previous tag, so nested generated handlers inherit the innermost op context.
func WithSource(ctx context.Context, tag SourceTag) context.Context {
	return context.WithValue(ctx, sourceTagKey{}, tag)
}

// SourceTagFromContext returns the most recent SourceTag attached to ctx via
// WithSource, or a zero-value SourceTag if none was set.
func SourceTagFromContext(ctx context.Context) SourceTag {
	if ctx == nil {
		return SourceTag{}
	}
	if v, ok := ctx.Value(sourceTagKey{}).(SourceTag); ok {
		return v
	}
	return SourceTag{}
}
