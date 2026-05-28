// Request-time report input params (W5 GAP-REPORT-01).
//
// A `report input { … }` block declares request-time parameters
// (`period_start`, `period_end`, `format`, ...) that thread to the
// report's `source` query. The auto-mount route in mount.go parses them
// off the request query string, validates required-ness against the
// contract's declared `Inputs`, and stashes the result into the request
// context with WithParams. The bound `SourceFn` reads them back with
// ParamsFromContext to build the filtered cursor — without changing the
// SourceFn / Runner signatures (the params ride the context).
//
// Wire-thin: parsing is `net/url.Values`, validation is "required ⇒
// present and non-empty". Type coercion stays in the SourceFn, which
// knows the source query's column types.

package report

import "context"

// Params holds the validated request-time report input values, keyed by
// declared param name. Values are the verbatim query-string strings; the
// SourceFn coerces them to the column types it needs.
type Params map[string]string

// Get returns the value for the named param (empty string when absent).
func (p Params) Get(name string) string {
	if p == nil {
		return ""
	}
	return p[name]
}

// Has reports whether the named param was supplied (and non-empty).
func (p Params) Has(name string) bool {
	if p == nil {
		return false
	}
	v, ok := p[name]
	return ok && v != ""
}

type paramsCtxKey struct{}

// WithParams returns a child context carrying the parsed report params.
// The auto-mount route calls this before invoking the runner; the
// SourceFn recovers the values via ParamsFromContext.
func WithParams(ctx context.Context, p Params) context.Context {
	return context.WithValue(ctx, paramsCtxKey{}, p)
}

// ParamsFromContext recovers the report params stashed by WithParams.
// Returns an empty (non-nil) Params when none were set, so callers can
// chain `.Get` / `.Has` without a nil check.
func ParamsFromContext(ctx context.Context) Params {
	if p, ok := ctx.Value(paramsCtxKey{}).(Params); ok && p != nil {
		return p
	}
	return Params{}
}

// MissingInputError is returned by ParseInputs when a required param is
// absent or empty. The auto-mount route surfaces it as HTTP 400.
type MissingInputError struct {
	Param string
}

func (e *MissingInputError) Error() string {
	return "report: required input param missing or empty: " + e.Param
}

// ParseInputs reads the declared `inputs` from the request query values
// `q` (typically `r.URL.Query()`), validating that every `Required`
// param is present and non-empty. Returns a `*MissingInputError` for the
// first offending required param. Undeclared query keys are ignored (the
// declared `Inputs` is the closed contract). Optional params absent from
// the request are simply omitted from the result.
func ParseInputs(inputs []Input, q map[string][]string) (Params, error) {
	out := make(Params, len(inputs))
	for _, in := range inputs {
		var val string
		if vs, ok := q[in.Name]; ok && len(vs) > 0 {
			val = vs[0]
		}
		if in.Required && val == "" {
			return nil, &MissingInputError{Param: in.Name}
		}
		if val != "" {
			out[in.Name] = val
		}
	}
	return out, nil
}
