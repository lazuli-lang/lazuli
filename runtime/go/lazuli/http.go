package lazuli

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"log/slog"
	"net/http"
	"time"

	"lazuli.dev/runtime/lazuli/i18n"
	"lazuli.dev/runtime/lazuli/report"
	"lazuli.dev/runtime/lazuli/webhooks"
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

	// Middleware order (outermost first):
	//   CORS  → handles OPTIONS preflight + sets headers BEFORE downstream
	//           middleware sees the request
	//   logging → records timing/status of every dispatched request
	//   mux   → typed command/query/api/webhook/healthz routing
	return CorsMiddleware(loggingMiddleware(mux))
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

	body, err := readRequestBody(r)
	if err != nil {
		writeError(w, r, err)
		return
	}

	ctx := newRequestCtx(r)
	out, err := handler.dispatch(ctx, body)
	if err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, out)
}

// handleApiRequest is the per-Api HTTP handler. It builds the request
// Ctx, reads the body, dispatches via the registration's typed
// closure, and writes JSON.
func handleApiRequest(w http.ResponseWriter, r *http.Request, api apiRegistration) {
	body, err := readRequestBody(r)
	if err != nil {
		writeError(w, r, err)
		return
	}

	ctx := newRequestCtx(r)
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

	body, err := readRequestBody(r)
	if err != nil {
		writeError(w, r, err)
		return
	}

	ctx := newRequestCtx(r)
	out, err := handler.dispatch(ctx, body)
	if err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, out)
}

// readRequestBody returns the raw JSON body, or an empty raw message if the
// request has no body (GET / DELETE / etc.).
func readRequestBody(r *http.Request) (json.RawMessage, error) {
	if r.Body == nil {
		return nil, nil
	}
	defer r.Body.Close()
	buf, err := io.ReadAll(r.Body)
	if err != nil {
		return nil, &Error{Status: 400, Code: CodeBadRequest,
			Message: "failed to read body: " + err.Error()}
	}
	return json.RawMessage(buf), nil
}

// newRequestCtx builds the Ctx for an inbound HTTP request. The dev-mode
// session reader (`populateDevSession`) reads `X-Lazuli-*` headers to set
// Actor / User / Tenant; the future auth cut replaces that helper with
// real cookie/JWT/HMAC sessions without changing this function's contract.
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
	writeJSON(w, http.StatusInternalServerError, map[string]any{
		"code":    CodeInternal,
		"message": err.Error(),
	})
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
		contract       i18n.FeatureErrorContract
		contractKnown  bool
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
