package lazuli

import "context"

// SourceTag identifies the authored Lazuli operation currently running.
// Codegen attaches it to handler contexts so runtime diagnostics can map
// Go frames and typed errors back to .lzi semantics.
type SourceTag struct {
	Capsule string
	Feature string
	Kind    string
	Op      string
}

type sourceTagKey struct{}

// WithSource returns a child context carrying the authored Lazuli source tag.
func WithSource(ctx context.Context, tag SourceTag) context.Context {
	return context.WithValue(ctx, sourceTagKey{}, tag)
}

// SourceTagFromContext returns the Lazuli source tag attached to ctx, if any.
func SourceTagFromContext(ctx context.Context) SourceTag {
	tag, _ := ctx.Value(sourceTagKey{}).(SourceTag)
	return tag
}
