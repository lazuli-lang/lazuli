package lazuli

import (
	"encoding/json"
	"errors"
	"io"
	"log/slog"
	"net/http"
	"time"
)

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

	return loggingMiddleware(mux)
}

// handleCommandRequest is the per-command HTTP handler. It builds the
// request Ctx, decodes input, dispatches to the typed Command[I, O], and
// writes JSON output.
func handleCommandRequest(w http.ResponseWriter, r *http.Request, cmd *commandErased) {
	handler := lookupCommandHandler(cmd.Name)
	if handler == nil {
		writeError(w, &Error{Status: 500, Code: CodeInternal,
			Message: "command registered without typed handler: " + cmd.Name})
		return
	}

	body, err := readRequestBody(r)
	if err != nil {
		writeError(w, err)
		return
	}

	ctx := newRequestCtx(r)
	out, err := handler.dispatch(ctx, body)
	if err != nil {
		writeError(w, err)
		return
	}
	writeJSON(w, http.StatusOK, out)
}

// handleQueryRequest is the per-query HTTP handler. It builds the request
// Ctx, decodes args, dispatches to the typed Query[A, R], and writes JSON.
func handleQueryRequest(w http.ResponseWriter, r *http.Request, q *queryErased) {
	handler := lookupQueryHandler(q.Name)
	if handler == nil {
		writeError(w, &Error{Status: 500, Code: CodeInternal,
			Message: "query registered without typed handler: " + q.Name})
		return
	}

	body, err := readRequestBody(r)
	if err != nil {
		writeError(w, err)
		return
	}

	ctx := newRequestCtx(r)
	out, err := handler.dispatch(ctx, body)
	if err != nil {
		writeError(w, err)
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

// newRequestCtx builds the Ctx for an inbound HTTP request. v0 spike: no
// auth wiring; everything is anonymous. The auth cut populates Actor /
// User / Tenant from session/JWT/HMAC.
func newRequestCtx(r *http.Request) *Ctx {
	return &Ctx{
		Context:   r.Context(),
		Actor:     ActorAnonymous,
		User:      nil,
		Tenant:    nil,
		RequestID: r.Header.Get("X-Request-ID"),
		TraceID:   r.Header.Get("X-Trace-ID"),
		Now:       time.Now(),
	}
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
func writeError(w http.ResponseWriter, err error) {
	var le *Error
	if errors.As(err, &le) {
		status := le.Status
		if status == 0 {
			status = http.StatusInternalServerError
		}
		writeJSON(w, status, map[string]any{
			"code":    le.Code,
			"message": le.Message,
			"data":    le.Data,
		})
		return
	}
	writeJSON(w, http.StatusInternalServerError, map[string]any{
		"code":    CodeInternal,
		"message": err.Error(),
	})
}

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
