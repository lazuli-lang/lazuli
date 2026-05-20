package lazuli

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"log/slog"
	"net/http"
	"os"
	"time"

	"lazuli.dev/runtime/lazuli/i18n"
	"lazuli.dev/runtime/lazuli/report"
	"lazuli.dev/runtime/lazuli/webhooks"
)

const (
	defaultMaxBodyBytes = 1 << 20 // 1 MiB
	defaultMuxRateLimit = RateLimit("600 per minute per ip")
)

// init wires the eventbus publisher into the webhooks package so the
// receiver can fire `Emits` on successful dispatch without taking a
// direct lazuli import (which would create a cycle: lazuli ↔ webhooks).
func init() {
	webhooks.RegisterEventPublisher(func(
		ctx context.Context,
		name string,
		payload map[string]any,
		occurredAt time.Time,
	) {
		Publish(ctx, Event{
			Name:       name,
			Payload:    payload,
			OccurredAt: occurredAt,
		})
	})
}

// Mux returns an http.Handler that exposes every registered command and
// query as a typed endpoint. Routes:
//
//	POST /api/v1/c/<command-name>      -> command.Handle
//	POST /api/v1/q/<query-name>        -> query.Run* (kind dispatched)
//	GET  /healthz                      -> liveness
//
// Queries use POST + JSON body for v0; query strings encode complex args
// awkwardly and forms/typed clients prefer JSON. A future cut may add
// GET-with-query-params for cache-friendly URLs once we have caching.
//
// Generated code does not configure routes; it only registers commands and
// queries, and the runtime mounts them.
func Mux() http.Handler {
	mux := http.NewServeMux()

	mux.HandleFunc("GET /healthz", func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
	})

	mux.HandleFunc("GET /debug/cache", func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusOK, Stats())
	})

	for _, cmd := range Commands() {
		cmd := cmd
		path := "POST /api/v1/c/" + cmd.Name
		mux.HandleFunc(path, func(w http.ResponseWriter, r *http.Request) {
			handleCommandRequest(w, r, cmd)
		})
	}

	for _, q := range Queries() {
		q := q
		path := "POST /api/v1/q/" + q.Name
		mux.HandleFunc(path, func(w http.ResponseWriter, r *http.Request) {
			handleQueryRequest(w, r, q)
		})
	}

	// API auto-mount — apiRegistration now carries Method + Dispatch
	// closure populated by `RegisterApi[I, O]`. Mount each as
	// `<METHOD> <PATH>` bound to its typed dispatcher.
	for _, api := range GlobalRegistry.Apis() {
		api := api
		if api.Dispatch == nil || api.Method == "" {
			continue
		}
		if api.HandlerChecker != nil && !api.HandlerChecker() {
			continue
		}
		path := string(api.Method) + " " + api.Path
		mux.HandleFunc(path, func(w http.ResponseWriter, r *http.Request) {
			handleApiRequest(w, r, api)
		})
	}

	// Webhook auto-mount — closes WAR-RUNTIME-WEBHOOK-MUX-01.
	// `webhooks.Register(&contract, handler)` from per-feature init
	// blocks populates the global registry; we walk it here and bind
	// `POST <Route>` for each. The framework's `webhooks.Mount` helper
	// handles the verification + idempotency + dispatch pipeline.
	if webhookContracts, webhookHandlers := webhooks.Registered(); len(webhookContracts) > 0 {
		_ = webhooks.Mount(&webhookRouterAdapter{mux: mux}, webhookContracts, webhookHandlers)
	}

	// Report auto-mount — `GET /api/reports/<name>.<format>` per
	// (contract × format) declared in any feature. Walks the
	// process-global registry populated by generated `reports.gen.go`
	// `init()` blocks. See `runtime/go/lazuli/report/mount.go` +
	// `docs/proposals/report-vocab.md` §Open questions
	// "Auto-mount HTTP endpoints".
	report.Mount(mux)

	// SECURITY (SEC-D2): every route flows through the standard stack.
	// Order (outermost first): recover catches all inner panics; CORS handles
	// OPTIONS before downstream checks; logging observes rate/CSRF denials;
	// rate-limit cheaply rejects before CSRF; CSRF is closest to the mux.
	handler := http.Handler(mux)
	handler = CSRFMiddleware(NewCSRFGuard(activeCSRFAllowedOrigins()))(handler)
	handler = RateLimitMiddleware(defaultMuxRateLimit, handler)
	handler = loggingMiddleware(handler)
	handler = CorsMiddleware(handler)
	handler = RecoverMiddleware(handler)
	return handler
}

// webhookRouterAdapter bridges `*http.ServeMux` to the
// `webhooks.Router` interface (which expects a chi-style
// `Method(method, pattern, handler)` shape).
type webhookRouterAdapter struct {
	mux *http.ServeMux
}

func (w *webhookRouterAdapter) Method(method, pattern string, handler http.HandlerFunc) {
	w.mux.HandleFunc(method+" "+pattern, handler)
}

// handleCommandRequest is the per-command HTTP handler. It builds the
// request Ctx, decodes input, dispatches to the typed Command[I, O], and
// writes JSON output.
func handleCommandRequest(w http.ResponseWriter, r *http.Request, cmd *commandErased) {
	handler := lookupCommandHandler(cmd.Name)
	if handler == nil {
		writeError(w, r, &Error{Status: 500, Code: CodeInternal,
			Message: "command registered without typed handler: " + cmd.Name})
		return
	}

	body, err := readRequestBody(w, r)
	if err != nil {
		writeError(w, r, err)
		return
	}

	// DB-INTEGRITY-CATALOG-EXT (2026-05-19): stamp the source tag onto
	// the http.Request context BEFORE dispatch so writeError can read
	// `SourceTag.Feature` and walk the L2 feature-level catch-all in the
	// resolver (proposal §2.E step 3). Without this propagation, the
	// source tag stays trapped inside `*Ctx.Context` and the L2 layer
	// is dark — feature-level overrides like `errors unique_violation
	// message @translation.account_email_already_registered` would
	// silently fall through to the builtin floor.
	if cmd.WithSource != nil {
		r = r.WithContext(cmd.WithSource(r.Context()))
	}

	ctx := newRequestCtx(r)
	ctx.withResponseWriter(w)
	if err := sessionExpiredError(ctx); err != nil {
		writeError(w, r, err)
		return
	}
	out, err := handler.dispatch(ctx, body)
	if err != nil {
		// Wave 3.5 — stamp the per-command MessageKey override on the
		// error envelope before the resolver runs. Codegen sets
		// `Command.ErrorKeys` for every command authoring
		// `policy ... when_denied @translation.<key>` (and, post-pilot,
		// other per-code overrides); without this stamp the resolver's
		// L1 layer would miss every per-command authored key because
		// `EvalPolicy` / validators write the bare code (e.g.
		// `policy_denied`) into `Error.MessageKey`, not the qualified
		// override key. See proposal §2.E step 1.
		stampPerCommandMessageKey(err, cmd.ErrorKeys)
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, out)
}

// stampPerCommandMessageKey overrides `Error.MessageKey` with the
// qualified per-command key for the matching code (proposal §2.E
// step 1). No-op when the command has no ErrorKeys, the error is not
// a typed `*Error`, or the matching field is zero.
//
// The function intentionally clobbers any pre-existing MessageKey:
// `EvalPolicy` writes the bare `"policy_denied"` literal at
// `policy.go:147`, and we want the qualified override (e.g.
// `account.account_me_signin_required`) to win whenever the command
// authored one. The resolver's L1 layer then hits the catalog entry
// instead of falling through to L3 builtin.
func stampPerCommandMessageKey(err error, keys *i18n.ErrorKeys) {
	if keys == nil {
		return
	}
	var le *Error
	if !errors.As(err, &le) {
		return
	}
	var ref i18n.MessageRef
	switch le.Code {
	case CodePolicyDenied:
		ref = keys.PolicyDenied
	case CodeValidationFailed:
		ref = keys.ValidationFailed
	case CodeTenantMismatch:
		ref = keys.TenantMismatch
	case CodeNotFound:
		ref = keys.NotFound
	case CodeRateLimited:
		ref = keys.RateLimited
	case CodeBadRequest:
		ref = keys.BadRequest
	case CodeMethodNotAllowed:
		ref = keys.MethodNotAllowed
	case CodeIntegrationError:
		ref = keys.IntegrationError
	case CodeUniqueViolation:
		ref = keys.UniqueViolation
	case CodeForeignKeyViolation:
		ref = keys.ForeignKeyViolation
	case CodeNotNullViolation:
		ref = keys.NotNullViolation
	case CodeCheckViolation:
		ref = keys.CheckViolation
	default:
		return
	}
	if ref.IsZero() {
		return
	}
	le.MessageKey = ref.Qualified()
}

// handleApiRequest is the per-Api HTTP handler. It builds the request
// Ctx, reads the body, dispatches via the registration's typed
// closure, and writes JSON.
func handleApiRequest(w http.ResponseWriter, r *http.Request, api apiRegistration) {
	body, err := readRequestBody(w, r)
	if err != nil {
		writeError(w, r, err)
		return
	}

	ctx := newRequestCtx(r)
	ctx.withResponseWriter(w)
	if err := sessionExpiredError(ctx); err != nil {
		writeError(w, r, err)
		return
	}
	out, err := api.Dispatch(ctx, body)
	if err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, out)
}

// handleQueryRequest is the per-query HTTP handler. It builds the request
// Ctx, decodes args, dispatches to the typed Query[A, R], and writes JSON.
func handleQueryRequest(w http.ResponseWriter, r *http.Request, q *queryErased) {
	handler := lookupQueryHandler(q.Name)
	if handler == nil {
		writeError(w, r, &Error{Status: 500, Code: CodeInternal,
			Message: "query registered without typed handler: " + q.Name})
		return
	}

	body, err := readRequestBody(w, r)
	if err != nil {
		writeError(w, r, err)
		return
	}

	if q.WithSource != nil {
		r = r.WithContext(q.WithSource(r.Context()))
	}

	ctx := newRequestCtx(r)
	ctx.withResponseWriter(w)
	if err := sessionExpiredError(ctx); err != nil {
		writeError(w, r, err)
		return
	}
	out, err := handler.dispatch(ctx, body)
	if err != nil {
		stampPerCommandMessageKey(err, q.ErrorKeys)
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, out)
}

// readRequestBody returns the raw JSON body, or an empty raw message if the
// request has no body (GET / DELETE / etc.).
//
// It accepts the ResponseWriter so http.MaxBytesReader can enforce the cap at
// the shared request-decoding boundary; only command/query/api entry handlers
// call this helper, so this keeps call-site churn small.
func readRequestBody(w http.ResponseWriter, r *http.Request) (json.RawMessage, error) {
	if r.Body == nil {
		return nil, nil
	}
	r.Body = http.MaxBytesReader(w, r.Body, defaultMaxBodyBytes)
	defer r.Body.Close()
	buf, err := io.ReadAll(r.Body)
	if err != nil {
		var maxErr *http.MaxBytesError
		if errors.As(err, &maxErr) {
			return nil, &Error{Status: http.StatusRequestEntityTooLarge, Code: CodeBadRequest,
				Message: "request body too large", MessageKey: CodeBadRequest}
		}
		return nil, &Error{Status: 400, Code: CodeBadRequest,
			Message: "failed to read body: " + err.Error()}
	}
	return json.RawMessage(buf), nil
}

// newRequestCtx builds the Ctx for an inbound HTTP request. Two
// auth-population paths run in order:
//
//  1. populateProductionSession: cookie or `Authorization: Bearer` →
//     `SessionResolver.Resolve` → Ctx.User / Tenant / SessionID /
//     SessionToken. No-op when no resolver is registered or the
//     request carries no token.
//  2. populateDevSession: `X-Lazuli-*` headers. Wins over the
//     production path so test scaffolding stays green even when a
//     real session cookie happens to be present.
//
// Either path may leave the Ctx anonymous; the policy layer decides
// whether to deny.
func newRequestCtx(r *http.Request) *Ctx {
	ctx := &Ctx{
		Context:   r.Context(),
		Actor:     ActorAnonymous,
		User:      nil,
		Tenant:    nil,
		RequestID: r.Header.Get("X-Request-ID"),
		TraceID:   r.Header.Get("X-Trace-ID"),
		Now:       time.Now(),
	}
	populateProductionSession(r, ctx)
	populateDevSession(r, ctx)
	return ctx
}

// writeJSON marshals v to JSON with the given status. Encoding errors are
// logged but the request still completes.
func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	if v == nil {
		return
	}
	if err := json.NewEncoder(w).Encode(v); err != nil {
		slog.Error("lazuli: failed to encode response", "error", err)
	}
}

// writeError encodes a runtime error envelope. Non-Lazuli errors map to 500.
//
// When `r` is non-nil, the boundary also:
//   - negotiates locale from `Accept-Language` against the installed locale
//     contract (`AppLocaleContract()`),
//   - asks `i18n.Default()` to resolve the wire message via the four-layer
//     chain (proposal §2.E),
//   - gates `message_key` / `data` exposure via the feature's
//     `FeatureErrorContract` (proposal §2.G).
//
// Callers without an `*http.Request` (e.g. middleware that fires before
// request decoding) pass `nil`; the boundary then writes the legacy
// envelope (Message/Code/Data) without resolution.
func writeError(w http.ResponseWriter, r *http.Request, err error) {
	var le *Error
	if errors.As(err, &le) {
		writeLazuliError(w, r, le)
		return
	}
	reqID := ""
	if r != nil {
		reqID = r.Header.Get("X-Request-ID")
	}
	errText := "<nil>"
	if err != nil {
		errText = err.Error()
	}
	// SECURITY: do not serialize err.Error(); it may include stack strings,
	// paths, SQL wrappers, or provider details. Log detail server-side only.
	slog.Error("lazuli: unmapped runtime error", "err", errText, "request_id", reqID)
	writeLazuliError(w, r, &Error{Status: http.StatusInternalServerError, Code: CodeInternal,
		Message: "internal server error", MessageKey: CodeInternal})
}

func activeCSRFAllowedOrigins() []string {
	contract := currentCors.Load()
	if contract == nil || len(contract.AllowOrigins) == 0 {
		return nil
	}
	env := os.Getenv("LAZULI_ENV")
	if env == "" {
		env = "local"
	}
	return contract.AllowOrigins[env]
}

// writeLazuliError handles the typed `*Error` path: resolver +
// exposure gating + JSON write.
func writeLazuliError(w http.ResponseWriter, r *http.Request, le *Error) {
	status := le.Status
	if status == 0 {
		status = http.StatusInternalServerError
	}

	// Resolve the message via the four-layer chain. Skip entirely when
	// the caller had no request (no locale, no source tag).
	var (
		feature   string
		locale    string
		hasCtx    = r != nil
		resolver  = i18n.Default()
		firedKey  string
		resolved  string
		hasResult bool
	)
	if hasCtx {
		ctx := r.Context()
		tag := SourceTagFromContext(ctx)
		feature = tag.Feature
		locale = negotiateErrorLocale(ctx, r.Header.Get("Accept-Language"))
		resolved, firedKey = resolver.Resolve(i18n.ErrorRequest{
			Code:       le.Code,
			MessageKey: le.MessageKey,
			Feature:    feature,
			Locale:     locale,
			Source:     le,
		})
		hasResult = resolved != ""
	}
	if hasResult {
		le.Message = resolved
		if le.MessageKey == "" {
			le.MessageKey = firedKey
		}
	}

	// Exposure gating. When no feature contract is registered, default
	// to the legacy behaviour (message + code + data exposed) so the
	// pre-Error-Vocab callers don't lose information.
	var (
		contract      i18n.FeatureErrorContract
		contractKnown bool
	)
	if hasCtx && feature != "" {
		contract, contractKnown = appErrorRegistry.Features[feature]
	}

	payload := map[string]any{
		"code": le.Code,
	}
	if !contractKnown || i18n.ShouldExpose(contract, status, "message") {
		payload["message"] = le.Message
	}
	if !contractKnown || i18n.ShouldExpose(contract, status, "data") {
		if le.Data != nil {
			payload["data"] = le.Data
		} else if !contractKnown {
			// Preserve legacy shape: pre-contract callers always wrote
			// the "data" key (often null). Keep that until codegen
			// ships a contract.
			payload["data"] = le.Data
		}
	}
	if le.MessageKey != "" && (!contractKnown || i18n.ShouldExpose(contract, status, "message_key")) {
		payload["message_key"] = le.MessageKey
	}

	writeJSON(w, status, payload)
}

// negotiateErrorLocale resolves the locale used for error message
// rendering. Order: request-context locale (set by `i18n.Middleware`)
// → Accept-Language best-match against the installed locale contract
// → installed-contract default → "en-US".
func negotiateErrorLocale(ctx context.Context, acceptLanguage string) string {
	if tag := i18n.LocaleFrom(ctx); tag != "" {
		return tag
	}
	contract := AppLocaleContract()
	if len(contract.Supported) > 0 {
		return i18n.NegotiateAcceptLanguage(contract, acceptLanguage)
	}
	// No contract installed: ship a sensible default so the resolver
	// has something to look up. The proposal's catalog ships en-US +
	// pt-BR at minimum (RUNTIME-2), so en-US is a safe floor.
	return "en-US"
}

// appErrorRegistry is the process-global error-resolution registry,
// installed at boot by codegen-emitted `RegisterAppErrorResolver`
// (proposal §4.1.3). Defaults to a zero registry — the boundary then
// falls back to legacy envelope behaviour.
var appErrorRegistry i18n.AppErrorResolverRegistry

// RegisterAppErrorResolver installs the app-level error-resolution
// registry. Codegen calls this once at boot from
// `dist/go/app/error_resolution.gen.go`. Passing a zero value reinstalls
// the empty registry (legacy envelope behaviour).
func RegisterAppErrorResolver(reg i18n.AppErrorResolverRegistry) {
	appErrorRegistry = reg
}

// appLocaleContract is the process-global locale contract. Codegen's
// `app.locale` lowering installs it; absent that, `AppLocaleContract`
// returns the zero value and the negotiator falls back to `en-US`.
var appLocaleContract i18n.LocaleContract

// AppLocaleContract returns the installed locale contract. Used by
// the error boundary for `Accept-Language` negotiation.
func AppLocaleContract() i18n.LocaleContract { return appLocaleContract }

// RegisterAppLocaleContract installs the app-level locale contract.
// Codegen calls this once at boot from the lowered `app.locale` block.
func RegisterAppLocaleContract(c i18n.LocaleContract) { appLocaleContract = c }

// loggingMiddleware logs every request with method, path, status, duration.
func loggingMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		start := time.Now()
		rec := &statusRecorder{ResponseWriter: w, status: http.StatusOK}
		next.ServeHTTP(rec, r)
		slog.Info("lazuli http",
			"method", r.Method,
			"path", r.URL.Path,
			"status", rec.status,
			"duration_ms", time.Since(start).Milliseconds(),
		)
	})
}

type statusRecorder struct {
	http.ResponseWriter
	status int
}

func (s *statusRecorder) WriteHeader(code int) {
	s.status = code
	s.ResponseWriter.WriteHeader(code)
}
