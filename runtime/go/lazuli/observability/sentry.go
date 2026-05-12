package observability

import (
	"context"
	"fmt"
	"runtime/pprof"
	"strings"

	"github.com/getsentry/sentry-go"
)

// SentryConfig configures Lazuli's optional Sentry panic/error reporter.
//
// An empty DSN disables the reporter without error so runtime boot can keep
// Sentry wiring present while leaving deployment secrets optional.
type SentryConfig struct {
	// DSN is the Sentry project DSN. Empty means disabled.
	DSN string
	// Environment is copied to Sentry events.
	Environment string
	// Release is copied to Sentry events.
	Release string
	// ServerName is copied to Sentry events.
	ServerName string
	// Debug enables sentry-go SDK debug output.
	Debug bool
	// SampleRate controls Sentry event sampling. Leave zero for sentry-go's default.
	SampleRate float64
	// Tags are default Sentry tags applied by the SDK.
	Tags map[string]string
	// BeforeSend is passed through to sentry-go for final event filtering.
	BeforeSend func(event *sentry.Event, hint *sentry.EventHint) *sentry.Event
	// Transport overrides sentry-go delivery. Tests should use this to avoid
	// network calls.
	Transport sentry.Transport
	// PanicEnvelopeOptions controls the Lazuli panic context attached to Sentry
	// panic events. StackMode defaults to PanicStackOmit.
	PanicEnvelopeOptions PanicEnvelopeOptions
}

// SentryReporter sends recovered panics and explicit errors to Sentry.
type SentryReporter struct {
	client  *sentry.Client
	options PanicEnvelopeOptions
}

var _ PanicReporter = (*SentryReporter)(nil)

// ConfigureSentry creates a Sentry reporter and installs it as the process-wide
// PanicReporter. When config.DSN is empty, panic reporting is disabled without
// returning an error.
func ConfigureSentry(config SentryConfig) (*SentryReporter, error) {
	reporter, err := NewSentryReporter(config)
	if err != nil {
		return nil, err
	}
	if reporter.Enabled() {
		SetPanicReporter(reporter)
	} else {
		ResetPanicReporter()
	}
	return reporter, nil
}

// NewSentryReporter creates a Sentry-backed reporter. Empty DSN returns a
// disabled reporter and a nil error.
func NewSentryReporter(config SentryConfig) (*SentryReporter, error) {
	if strings.TrimSpace(config.DSN) == "" {
		return &SentryReporter{options: config.PanicEnvelopeOptions}, nil
	}

	client, err := sentry.NewClient(sentry.ClientOptions{
		Dsn:              strings.TrimSpace(config.DSN),
		Environment:      strings.TrimSpace(config.Environment),
		Release:          strings.TrimSpace(config.Release),
		ServerName:       strings.TrimSpace(config.ServerName),
		Debug:            config.Debug,
		SampleRate:       config.SampleRate,
		AttachStacktrace: true,
		Tags:             sentryCopyTags(config.Tags),
		BeforeSend:       config.BeforeSend,
		Transport:        config.Transport,
	})
	if err != nil {
		return nil, err
	}

	return &SentryReporter{
		client:  client,
		options: config.PanicEnvelopeOptions,
	}, nil
}

// Enabled reports whether this reporter has a configured Sentry client.
func (r *SentryReporter) Enabled() bool {
	return r != nil && r.client != nil
}

// ReportPanic implements PanicReporter.
func (r *SentryReporter) ReportPanic(ctx context.Context, report PanicReport) {
	_ = r.CapturePanic(ctx, report)
}

// CapturePanic sends a recovered panic report to Sentry and returns the event
// id when sent. Disabled reporters return nil.
func (r *SentryReporter) CapturePanic(ctx context.Context, report PanicReport) *sentry.EventID {
	if !r.Enabled() {
		return nil
	}
	if ctx == nil {
		ctx = context.Background()
	}

	event := r.panicEvent(ctx, report)
	return r.client.CaptureEvent(event, &sentry.EventHint{
		Context:            ctx,
		RecoveredException: report.Recovered,
	}, sentry.NewScope())
}

// CaptureError sends err to Sentry and returns the event id when sent. Nil
// errors and disabled reporters return nil.
func (r *SentryReporter) CaptureError(ctx context.Context, err error) *sentry.EventID {
	if !r.Enabled() || err == nil {
		return nil
	}
	if ctx == nil {
		ctx = context.Background()
	}

	event := r.client.EventFromException(err, sentry.LevelError)
	sentryDecorateErrorEvent(ctx, event, err)
	return r.client.CaptureEvent(event, &sentry.EventHint{
		Context:           ctx,
		OriginalException: err,
	}, sentry.NewScope())
}

// Flush waits for queued events to be delivered. Disabled reporters return true.
func (r *SentryReporter) Flush(ctx context.Context) bool {
	if !r.Enabled() {
		return true
	}
	if ctx == nil {
		ctx = context.Background()
	}
	return r.client.FlushWithContext(ctx)
}

// Close releases Sentry transport resources. Call Flush first when shutting
// down if queued events must be delivered.
func (r *SentryReporter) Close() {
	if r.Enabled() {
		r.client.Close()
	}
}

func (r *SentryReporter) panicEvent(ctx context.Context, report PanicReport) *sentry.Event {
	switch recovered := report.Recovered.(type) {
	case error:
		event := r.client.EventFromException(recovered, sentry.LevelFatal)
		sentryDecoratePanicEvent(ctx, event, report, r.options)
		return event
	case string:
		event := r.client.EventFromMessage(recovered, sentry.LevelFatal)
		sentryDecoratePanicEvent(ctx, event, report, r.options)
		return event
	default:
		event := r.client.EventFromMessage(fmt.Sprintf("%#v", recovered), sentry.LevelFatal)
		sentryDecoratePanicEvent(ctx, event, report, r.options)
		return event
	}
}

func sentryDecoratePanicEvent(ctx context.Context, event *sentry.Event, report PanicReport, options PanicEnvelopeOptions) {
	if event == nil {
		return
	}

	envelope := PanicEnvelopeFromReport(ctx, report, options)
	event.Logger = "lazuli/observability"
	event.Transaction = sentryPanicTransactionName(envelope.Error.Feature, envelope.Error.Kind, envelope.Error.Op, report)
	sentryEnsurePanicStacktrace(event)
	if envelope.Debug.Request != nil {
		event.Request = &sentry.Request{
			Method: envelope.Debug.Request.Method,
			URL:    envelope.Debug.Request.Path,
		}
	}

	tags := sentryTags(event.Tags)
	sentrySetTag(tags, "lazuli.scope", envelope.Debug.Scope)
	sentrySetTag(tags, "lazuli.error_code", envelope.Error.Code)
	sentrySetTag(tags, "lazuli.origin", envelope.Error.Origin)
	sentrySetTag(tags, "lazuli.feature", envelope.Error.Feature)
	sentrySetTag(tags, "lazuli.kind", envelope.Error.Kind)
	sentrySetTag(tags, "lazuli.op", envelope.Error.Op)
	sentrySetTag(tags, "lazuli.source", envelope.Error.Source)
	sentryAddPatternTags(ctx, tags)
	event.Tags = tags

	contexts := sentryContexts(event.Contexts)
	contexts["lazuli_panic"] = sentryPanicContext(envelope)
	event.Contexts = contexts
}

func sentryDecorateErrorEvent(ctx context.Context, event *sentry.Event, err error) {
	if event == nil {
		return
	}

	metadata, _ := panicEnvelopeMetadataFromError(err, 0)
	metadata.merge(panicEnvelopeMetadataFromContext(ctx))

	event.Logger = "lazuli/observability"
	event.Transaction = sentryOperationName(metadata.Feature, metadata.Kind, metadata.Op)

	tags := sentryTags(event.Tags)
	sentrySetTag(tags, "lazuli.error_code", metadata.Code)
	sentrySetTag(tags, "lazuli.origin", metadata.Origin)
	sentrySetTag(tags, "lazuli.feature", metadata.Feature)
	sentrySetTag(tags, "lazuli.kind", metadata.Kind)
	sentrySetTag(tags, "lazuli.op", metadata.Op)
	sentrySetTag(tags, "lazuli.source", metadata.Source)
	sentryAddPatternTags(ctx, tags)
	event.Tags = tags

	if envelope, ok := PanicEnvelopeFromError(ctx, err, PanicEnvelopeOptions{}); ok {
		contexts := sentryContexts(event.Contexts)
		contexts["lazuli_panic"] = sentryPanicContext(envelope)
		event.Contexts = contexts
	}
}

func sentryEnsurePanicStacktrace(event *sentry.Event) {
	if len(event.Threads) > 0 {
		return
	}
	for _, exception := range event.Exception {
		if exception.Stacktrace != nil {
			return
		}
	}
	event.Threads = []sentry.Thread{{
		Stacktrace: sentry.NewStacktrace(),
		Current:    true,
	}}
}

func sentryPanicContext(envelope PanicEnvelope) sentry.Context {
	ctx := sentry.Context{
		"scope":          envelope.Debug.Scope,
		"recovered":      envelope.Debug.Recovered,
		"recovered_type": envelope.Debug.RecoveredType,
		"code":           envelope.Error.Code,
		"status":         envelope.Error.Status,
		"origin":         envelope.Error.Origin,
	}
	if envelope.Debug.Request != nil {
		ctx["request"] = map[string]string{
			"method":     envelope.Debug.Request.Method,
			"path":       envelope.Debug.Request.Path,
			"request_id": envelope.Debug.Request.RequestID,
			"trace_id":   envelope.Debug.Request.TraceID,
		}
	}
	if envelope.Debug.Stack != "" {
		ctx["stack"] = envelope.Debug.Stack
		ctx["stack_redacted"] = envelope.Debug.StackRedacted
		ctx["stack_truncated"] = envelope.Debug.StackTruncated
	}
	if envelope.Debug.RecoveredAt != nil {
		ctx["recovered_at"] = envelope.Debug.RecoveredAt.Format("2006-01-02T15:04:05.999999999Z07:00")
	}
	return ctx
}

func sentryAddPatternTags(ctx context.Context, tags map[string]string) {
	if ctx == nil {
		return
	}
	if patternID, ok := pprof.Label(ctx, opLabelPatternIDKey); ok {
		sentrySetTag(tags, "lazuli.pattern_id", patternID)
	}
	if patternVersion, ok := pprof.Label(ctx, opLabelPatternVersionKey); ok {
		sentrySetTag(tags, "lazuli.pattern_version", patternVersion)
	}
}

func sentryOperationName(feature, kind, op string) string {
	parts := make([]string, 0, 3)
	if feature = strings.TrimSpace(feature); feature != "" {
		parts = append(parts, feature)
	}
	if kind = strings.TrimSpace(kind); kind != "" {
		parts = append(parts, kind)
	}
	if op = strings.TrimSpace(op); op != "" {
		parts = append(parts, op)
	}
	return strings.Join(parts, ".")
}

func sentryTransactionNameFromRequest(report PanicReport) string {
	method := strings.TrimSpace(report.RequestMethod)
	path := strings.TrimSpace(report.RequestPath)
	if method == "" {
		return path
	}
	if path == "" {
		return method
	}
	return method + " " + path
}

func sentryPanicTransactionName(feature, kind, op string, report PanicReport) string {
	if name := sentryOperationName(feature, kind, op); name != "" {
		return name
	}
	return sentryTransactionNameFromRequest(report)
}

func sentryTags(tags map[string]string) map[string]string {
	if tags == nil {
		return map[string]string{}
	}
	return tags
}

func sentryContexts(contexts map[string]sentry.Context) map[string]sentry.Context {
	if contexts == nil {
		return map[string]sentry.Context{}
	}
	return contexts
}

func sentrySetTag(tags map[string]string, key, value string) {
	value = strings.TrimSpace(value)
	if value == "" {
		return
	}
	tags[key] = value
}

func sentryCopyTags(tags map[string]string) map[string]string {
	if len(tags) == 0 {
		return nil
	}
	out := make(map[string]string, len(tags))
	for key, value := range tags {
		key = strings.TrimSpace(key)
		value = strings.TrimSpace(value)
		if key != "" && value != "" {
			out[key] = value
		}
	}
	return out
}
