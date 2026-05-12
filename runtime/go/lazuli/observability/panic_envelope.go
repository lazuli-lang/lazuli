package observability

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"path"
	"reflect"
	"runtime/pprof"
	"strings"
	"time"
)

const (
	panicEnvelopeCodeInternalPanic = "internal_panic"

	panicEnvelopeOriginUserDSL        = "user_dsl"
	panicEnvelopeOriginLibInternal    = "lib_internal"
	panicEnvelopeOriginCodegenBug     = "codegen_bug"
	panicEnvelopeOriginAdapterRuntime = "adapter_runtime"

	panicEnvelopeDefaultMaxStackBytes = 64 * 1024
	panicEnvelopeMaxErrorDepth        = 64
)

// PanicStackMode controls whether a panic envelope carries a stack trace.
//
// EXPERIMENTAL: subject to change before 1.0.
type PanicStackMode int

const (
	// PanicStackOmit leaves the stack out of the envelope.
	PanicStackOmit PanicStackMode = iota
	// PanicStackRedacted includes the stack with local file paths reduced to basenames.
	PanicStackRedacted
	// PanicStackRaw includes the captured stack without path redaction.
	PanicStackRaw
)

// PanicEnvelopeOptions controls debug fields copied into panic envelopes.
//
// EXPERIMENTAL: subject to change before 1.0.
type PanicEnvelopeOptions struct {
	// OmitSource strips Lazuli source metadata from the error envelope.
	OmitSource bool
	// StackMode controls whether Stack is omitted, redacted, or raw.
	StackMode PanicStackMode
	// MaxStackBytes caps Stack. Values <= 0 use the package default.
	MaxStackBytes int
	// RequestID overrides request_id metadata inferred from ctx.
	RequestID string
	// TraceID overrides trace_id metadata inferred from ctx.
	TraceID string
}

// PanicEnvelope is the JSON-friendly debug/error envelope for a recovered panic.
//
// EXPERIMENTAL: subject to change before 1.0.
type PanicEnvelope struct {
	Error PanicEnvelopeError `json:"error"`
	Debug PanicEnvelopeDebug `json:"debug"`
}

// PanicEnvelopeError is the typed error portion of a panic envelope.
//
// EXPERIMENTAL: subject to change before 1.0.
type PanicEnvelopeError struct {
	Code    string `json:"code"`
	Status  int    `json:"status"`
	Message string `json:"message,omitempty"`
	Origin  string `json:"origin,omitempty"`
	Feature string `json:"feature,omitempty"`
	Kind    string `json:"kind,omitempty"`
	Op      string `json:"op,omitempty"`
	Source  string `json:"source,omitempty"`
}

// PanicEnvelopeDebug is the debug metadata portion of a panic envelope.
//
// EXPERIMENTAL: subject to change before 1.0.
type PanicEnvelopeDebug struct {
	Scope          string                `json:"scope"`
	Recovered      string                `json:"recovered,omitempty"`
	RecoveredType  string                `json:"recovered_type,omitempty"`
	Request        *PanicEnvelopeRequest `json:"request,omitempty"`
	Stack          string                `json:"stack,omitempty"`
	StackRedacted  bool                  `json:"stack_redacted,omitempty"`
	StackTruncated bool                  `json:"stack_truncated,omitempty"`
	RecoveredAt    *time.Time            `json:"recovered_at,omitempty"`
}

// PanicEnvelopeRequest carries request correlation metadata for a panic envelope.
//
// EXPERIMENTAL: subject to change before 1.0.
type PanicEnvelopeRequest struct {
	Method    string `json:"method,omitempty"`
	Path      string `json:"path,omitempty"`
	RequestID string `json:"request_id,omitempty"`
	TraceID   string `json:"trace_id,omitempty"`
}

// PanicEnvelopeFromReport converts a recovered PanicReport into a typed
// debug/error envelope. Source metadata comes from the recovered typed error
// first, then from active operation labels in ctx when available.
//
// EXPERIMENTAL: subject to change before 1.0.
func PanicEnvelopeFromReport(ctx context.Context, report PanicReport, options PanicEnvelopeOptions) PanicEnvelope {
	metadata := panicEnvelopeMetadataFromRecovered(report.Recovered)
	metadata.merge(panicEnvelopeMetadataFromContext(ctx))
	if options.OmitSource {
		metadata.Source = ""
	}

	stack, stackRedacted, stackTruncated := panicEnvelopeStack(report.Stack, options)
	return PanicEnvelope{
		Error: panicEnvelopeError(metadata),
		Debug: PanicEnvelopeDebug{
			Scope:          panicEnvelopeScopeName(report.Scope),
			Recovered:      panicEnvelopeRecoveredString(report.Recovered),
			RecoveredType:  panicEnvelopeRecoveredType(report.Recovered),
			Request:        panicEnvelopeRequest(ctx, report, options),
			Stack:          stack,
			StackRedacted:  stackRedacted,
			StackTruncated: stackTruncated,
			RecoveredAt:    panicEnvelopeRecoveredAt(report.Time),
		},
	}
}

// PanicEnvelopeFromPanicError converts a PanicError into a typed debug/error
// envelope. The request method and path are empty because PanicError is used
// outside the HTTP recovery boundary.
//
// EXPERIMENTAL: subject to change before 1.0.
func PanicEnvelopeFromPanicError(ctx context.Context, panicErr *PanicError, options PanicEnvelopeOptions) PanicEnvelope {
	if panicErr == nil {
		return PanicEnvelopeFromReport(ctx, PanicReport{}, options)
	}
	return PanicEnvelopeFromReport(ctx, PanicReport{
		Recovered: panicErr.Recovered,
		Stack:     panicErr.Stack,
		Scope:     panicErr.Scope,
	}, options)
}

// PanicEnvelopeFromError extracts a PanicError from err and converts it into a
// typed debug/error envelope. The boolean is false when err does not wrap a
// PanicError.
//
// EXPERIMENTAL: subject to change before 1.0.
func PanicEnvelopeFromError(ctx context.Context, err error, options PanicEnvelopeOptions) (PanicEnvelope, bool) {
	var panicErr *PanicError
	if !errors.As(err, &panicErr) || panicErr == nil {
		return PanicEnvelope{}, false
	}
	return PanicEnvelopeFromPanicError(ctx, panicErr, options), true
}

type panicEnvelopeMetadata struct {
	Code    string
	Status  int
	Message string
	Origin  string
	Feature string
	Kind    string
	Op      string
	Source  string
}

func (m *panicEnvelopeMetadata) merge(next panicEnvelopeMetadata) {
	if m.Code == "" {
		m.Code = next.Code
	}
	if m.Status == 0 {
		m.Status = next.Status
	}
	if m.Message == "" {
		m.Message = next.Message
	}
	if m.Origin == "" {
		m.Origin = next.Origin
	}
	if m.Feature == "" {
		m.Feature = next.Feature
	}
	if m.Kind == "" {
		m.Kind = next.Kind
	}
	if m.Op == "" {
		m.Op = next.Op
	}
	if m.Source == "" {
		m.Source = next.Source
	}
}

func panicEnvelopeError(metadata panicEnvelopeMetadata) PanicEnvelopeError {
	if metadata.Code == "" {
		metadata.Code = panicEnvelopeCodeInternalPanic
	}
	if !panicEnvelopeValidHTTPStatus(metadata.Status) {
		metadata.Status = http.StatusInternalServerError
	}
	if metadata.Message == "" {
		metadata.Message = "internal server error"
	}
	if metadata.Origin == "" {
		metadata.Origin = panicEnvelopeOriginForStatus(metadata.Status)
		if metadata.Code == panicEnvelopeCodeInternalPanic {
			metadata.Origin = panicEnvelopeOriginLibInternal
		}
	}

	return PanicEnvelopeError{
		Code:    metadata.Code,
		Status:  metadata.Status,
		Message: metadata.Message,
		Origin:  metadata.Origin,
		Feature: metadata.Feature,
		Kind:    metadata.Kind,
		Op:      metadata.Op,
		Source:  metadata.Source,
	}
}

func panicEnvelopeMetadataFromRecovered(recovered any) panicEnvelopeMetadata {
	err, ok := recovered.(error)
	if !ok || err == nil {
		return panicEnvelopeMetadata{}
	}
	metadata, _ := panicEnvelopeMetadataFromError(err, 0)
	return metadata
}

func panicEnvelopeMetadataFromError(err error, depth int) (panicEnvelopeMetadata, bool) {
	if err == nil || depth >= panicEnvelopeMaxErrorDepth {
		return panicEnvelopeMetadata{}, false
	}

	if metadata, ok := panicEnvelopeMetadataFromValue(reflect.ValueOf(err)); ok {
		return metadata, true
	}

	type unwrapMany interface {
		Unwrap() []error
	}
	type unwrapOne interface {
		Unwrap() error
	}
	if many, ok := err.(unwrapMany); ok {
		for _, child := range many.Unwrap() {
			if metadata, ok := panicEnvelopeMetadataFromError(child, depth+1); ok {
				return metadata, true
			}
		}
		return panicEnvelopeMetadata{}, false
	}
	if one, ok := err.(unwrapOne); ok {
		return panicEnvelopeMetadataFromError(one.Unwrap(), depth+1)
	}
	return panicEnvelopeMetadata{}, false
}

func panicEnvelopeMetadataFromValue(value reflect.Value) (panicEnvelopeMetadata, bool) {
	value = panicEnvelopeDeref(value)
	if !value.IsValid() || value.Kind() != reflect.Struct {
		return panicEnvelopeMetadata{}, false
	}

	if base := panicEnvelopeDeref(value.FieldByName("Base")); base.IsValid() && base.Kind() == reflect.Struct {
		if metadata, ok := panicEnvelopeMetadataFromStruct(base); ok {
			return metadata, true
		}
	}
	return panicEnvelopeMetadataFromStruct(value)
}

func panicEnvelopeMetadataFromStruct(value reflect.Value) (panicEnvelopeMetadata, bool) {
	metadata := panicEnvelopeMetadata{
		Code:    panicEnvelopeStringField(value, "Code"),
		Status:  panicEnvelopeIntField(value, "Status"),
		Message: panicEnvelopeStringField(value, "Message"),
		Origin:  panicEnvelopeOriginField(value.FieldByName("Origin")),
		Feature: panicEnvelopeStringField(value, "Feature"),
		Kind:    panicEnvelopeStringField(value, "Kind"),
		Op:      panicEnvelopeStringField(value, "Op"),
		Source:  panicEnvelopeStringField(value, "Source"),
	}
	return metadata, metadata != (panicEnvelopeMetadata{})
}

func panicEnvelopeMetadataFromContext(ctx context.Context) panicEnvelopeMetadata {
	if ctx == nil {
		return panicEnvelopeMetadata{}
	}

	metadata := panicEnvelopeMetadata{}
	if feature, ok := pprof.Label(ctx, opLabelFeatureKey); ok {
		metadata.Feature = strings.TrimSpace(feature)
	}
	if kind, ok := pprof.Label(ctx, opLabelKindKey); ok {
		metadata.Kind = strings.TrimSpace(kind)
	}
	if op, ok := pprof.Label(ctx, opLabelNameKey); ok {
		metadata.Op = strings.TrimSpace(op)
	}
	if metadata.Op == "" {
		if op, ok := pprof.Label(ctx, ProfileLabelOp); ok {
			metadata.Op = strings.TrimSpace(op)
		}
	}
	if source, ok := pprof.Label(ctx, opLabelSourceKey); ok {
		metadata.Source = strings.TrimSpace(source)
	}
	return metadata
}

func panicEnvelopeRequest(ctx context.Context, report PanicReport, options PanicEnvelopeOptions) *PanicEnvelopeRequest {
	request := PanicEnvelopeRequest{
		Method:    strings.TrimSpace(report.RequestMethod),
		Path:      strings.TrimSpace(report.RequestPath),
		RequestID: strings.TrimSpace(options.RequestID),
		TraceID:   strings.TrimSpace(options.TraceID),
	}
	if request.RequestID == "" {
		request.RequestID = panicEnvelopeContextStringField(ctx, "RequestID")
	}
	if request.TraceID == "" {
		request.TraceID = panicEnvelopeContextStringField(ctx, "TraceID")
	}
	if request == (PanicEnvelopeRequest{}) {
		return nil
	}
	return &request
}

func panicEnvelopeStack(stack []byte, options PanicEnvelopeOptions) (string, bool, bool) {
	if len(stack) == 0 || options.StackMode == PanicStackOmit {
		return "", false, false
	}

	out := string(stack)
	redacted := options.StackMode == PanicStackRedacted
	if redacted {
		out = panicEnvelopeRedactStack(out)
	}

	maxBytes := options.MaxStackBytes
	if maxBytes <= 0 {
		maxBytes = panicEnvelopeDefaultMaxStackBytes
	}
	if len(out) <= maxBytes {
		return out, redacted, false
	}
	return out[:maxBytes] + "\n... [truncated]", redacted, true
}

func panicEnvelopeRedactStack(stack string) string {
	lines := strings.Split(stack, "\n")
	for i, line := range lines {
		lines[i] = panicEnvelopeRedactStackLine(line)
	}
	return strings.Join(lines, "\n")
}

func panicEnvelopeRedactStackLine(line string) string {
	trimmed := strings.TrimLeft(line, "\t ")
	leading := line[:len(line)-len(trimmed)]
	fileEnd := strings.Index(trimmed, ".go:")
	if fileEnd < 0 {
		return line
	}

	filePath := strings.ReplaceAll(trimmed[:fileEnd+len(".go")], "\\", "/")
	fileName := path.Base(filePath)
	if fileName == "." || fileName == "/" || fileName == "" {
		return line
	}
	return leading + fileName + trimmed[fileEnd+len(".go"):]
}

func panicEnvelopeScopeName(scope PanicScope) string {
	switch scope {
	case ScopeHTTPCommand:
		return "http_command"
	case ScopeJobWorker:
		return "job_worker"
	case ScopeWebhookHandler:
		return "webhook_handler"
	default:
		return "unknown"
	}
}

func panicEnvelopeRecoveredString(recovered any) (message string) {
	if recovered == nil {
		return ""
	}
	defer func() {
		if recover() != nil {
			message = "<panic value unavailable>"
		}
	}()
	if err, ok := recovered.(error); ok {
		return err.Error()
	}
	if stringer, ok := recovered.(fmt.Stringer); ok {
		return stringer.String()
	}
	return fmt.Sprint(recovered)
}

func panicEnvelopeRecoveredType(recovered any) string {
	if recovered == nil {
		return ""
	}
	return reflect.TypeOf(recovered).String()
}

func panicEnvelopeRecoveredAt(recoveredAt time.Time) *time.Time {
	if recoveredAt.IsZero() {
		return nil
	}
	return &recoveredAt
}

func panicEnvelopeStringField(value reflect.Value, name string) string {
	field := value.FieldByName(name)
	if !field.IsValid() || field.Kind() != reflect.String {
		return ""
	}
	return strings.TrimSpace(field.String())
}

func panicEnvelopeIntField(value reflect.Value, name string) int {
	field := value.FieldByName(name)
	if !field.IsValid() {
		return 0
	}
	switch field.Kind() {
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		return int(field.Int())
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr:
		return int(field.Uint())
	default:
		return 0
	}
}

func panicEnvelopeOriginField(field reflect.Value) string {
	if !field.IsValid() {
		return ""
	}
	switch field.Kind() {
	case reflect.String:
		return strings.TrimSpace(field.String())
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		return panicEnvelopeOriginName(field.Int())
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr:
		return panicEnvelopeOriginName(int64(field.Uint()))
	default:
		return ""
	}
}

func panicEnvelopeOriginName(origin int64) string {
	switch origin {
	case 0:
		return panicEnvelopeOriginUserDSL
	case 1:
		return panicEnvelopeOriginLibInternal
	case 2:
		return panicEnvelopeOriginCodegenBug
	case 3:
		return panicEnvelopeOriginAdapterRuntime
	default:
		return ""
	}
}

func panicEnvelopeOriginForStatus(status int) string {
	if status >= http.StatusInternalServerError {
		return panicEnvelopeOriginLibInternal
	}
	return panicEnvelopeOriginUserDSL
}

func panicEnvelopeValidHTTPStatus(status int) bool {
	return status >= 100 && status <= 599
}

func panicEnvelopeContextStringField(ctx context.Context, name string) string {
	value := panicEnvelopeDeref(reflect.ValueOf(ctx))
	if !value.IsValid() || value.Kind() != reflect.Struct {
		return ""
	}
	return panicEnvelopeStringField(value, name)
}

func panicEnvelopeDeref(value reflect.Value) reflect.Value {
	for value.IsValid() && (value.Kind() == reflect.Interface || value.Kind() == reflect.Pointer) {
		if value.IsNil() {
			return reflect.Value{}
		}
		value = value.Elem()
	}
	return value
}
