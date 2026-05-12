package lazuli

import (
	"bytes"
	"encoding/json"
	"io"
	"log/slog"
	"net/http"
	"reflect"
	"strconv"
	"strings"
	"sync"
	"time"
)

// Mux returns an http.Handler that exposes every registered command and
// query as a typed endpoint. Routes:
//
//	POST /api/v1/c/<command-name>      -> command.Handle
//	POST /api/v1/q/<query-name>        -> query.Run* (kind dispatched)
//	<method> <api path>                -> api.Handler (when mounted directly)
//	GET  /healthz                      -> liveness
//
// Queries use POST + JSON body for v0; query strings encode complex args
// awkwardly and forms/typed clients prefer JSON. A future cut may add
// GET-with-query-params for cache-friendly URLs once we have caching.
//
// Generated code does not configure routes; it registers commands, queries,
// and API metadata, and the runtime mounts them.
func Mux() *http.ServeMux {
	mux := http.NewServeMux()

	handleFunc(mux, "GET /healthz", func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
	})

	handleFunc(mux, "GET /debug/cache", func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusOK, Stats())
	})

	MountAll(mux)
	return mux
}

var mountedHTTPMuxes sync.Map

// MountAll walks the process registries and attaches every registered command,
// query, and API metadata route to mux. It is safe to call MountAll on the
// same mux more than once; generated main packages do this after Mux() so
// applications can also pass a custom mux when needed.
func MountAll(mux *http.ServeMux) {
	if mux == nil {
		panic("lazuli: MountAll called with nil mux")
	}
	if _, loaded := mountedHTTPMuxes.LoadOrStore(mux, struct{}{}); loaded {
		return
	}

	for _, cmd := range Commands() {
		cmd := cmd
		path := "POST /api/v1/c/" + cmd.Name
		handleFunc(mux, path, func(w http.ResponseWriter, r *http.Request) {
			handleCommandRequest(w, r, cmd)
		})
	}

	for _, q := range Queries() {
		q := q
		path := "POST /api/v1/q/" + q.Name
		handleFunc(mux, path, func(w http.ResponseWriter, r *http.Request) {
			handleQueryRequest(w, r, q)
		})
	}

	for _, api := range GlobalRegistry.Apis() {
		api := api
		path := normalizeHTTPPathPattern(api.Path)
		handleFunc(mux, path, func(w http.ResponseWriter, _ *http.Request) {
			writeError(w, &Error{Status: http.StatusNotImplemented, Code: CodeInternal,
				Message: "api registered without typed handler: " + api.Name})
		})
	}
}

// MountApi attaches a concrete API handler to mux using the method and path
// declared on api. Path parameters in "{name}" or ":name" segments are copied
// into matching JSON-tagged fields on the generated args struct before the
// handler runs.
func MountApi[I, O any](mux *http.ServeMux, api *Api[I, O]) {
	if mux == nil {
		panic("lazuli: MountApi called with nil mux")
	}
	if api == nil {
		panic("lazuli: MountApi called with nil api")
	}
	method := strings.TrimSpace(string(api.Method))
	if method == "" {
		panic("lazuli: api " + api.Name + " has empty method")
	}
	path := normalizeHTTPPathPattern(api.Path)
	pattern := method + " " + path
	handleFunc(mux, pattern, func(w http.ResponseWriter, r *http.Request) {
		handleAPIRequest(w, r, api)
	})
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

// handleAPIRequest is the per-API HTTP handler. It builds the request Ctx,
// decodes JSON input plus path parameters, dispatches to the typed handler,
// and writes JSON output.
func handleAPIRequest[I, O any](w http.ResponseWriter, r *http.Request, api *Api[I, O]) {
	if api.Handler == nil {
		writeError(w, &Error{Status: http.StatusNotImplemented, Code: CodeInternal,
			Message: "api registered without typed handler: " + api.Name})
		return
	}

	input, err := decodeAPIInput[I](r, api.Path)
	if err != nil {
		writeError(w, err)
		return
	}

	ctx := newRequestCtx(r)
	if err := enforcePolicy(ctx, api.Policy); err != nil {
		writeError(w, err)
		return
	}

	out, err := api.Handler(ctx, input)
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

func decodeAPIInput[I any](r *http.Request, path string) (I, error) {
	var input I
	body, err := readRequestBody(r)
	if err != nil {
		return input, err
	}
	if len(bytes.TrimSpace(body)) > 0 {
		if err := json.Unmarshal(body, &input); err != nil {
			return input, &Error{Status: http.StatusBadRequest, Code: CodeBadRequest,
				Message: "invalid JSON body: " + err.Error()}
		}
	}
	if err := applyPathValues(&input, routePathParamNames(path), r); err != nil {
		return input, err
	}
	return input, nil
}

func applyPathValues(input any, names []string, r *http.Request) error {
	if len(names) == 0 {
		return nil
	}
	rv := reflect.ValueOf(input)
	if rv.Kind() != reflect.Pointer || rv.IsNil() {
		return &Error{Status: http.StatusBadRequest, Code: CodeBadRequest,
			Message: "api path parameters require pointer input"}
	}
	rv = rv.Elem()
	if rv.Kind() != reflect.Struct {
		return &Error{Status: http.StatusBadRequest, Code: CodeBadRequest,
			Message: "api path parameters require struct input"}
	}

	for _, name := range names {
		raw := r.PathValue(name)
		if raw == "" {
			continue
		}
		field := fieldByJSONName(rv, name)
		if !field.IsValid() || !field.CanSet() {
			return &Error{Status: http.StatusBadRequest, Code: CodeBadRequest,
				Message: "api path parameter has no input field: " + name}
		}
		if err := setPathField(field, raw); err != nil {
			return err
		}
	}
	return nil
}

func fieldByJSONName(rv reflect.Value, name string) reflect.Value {
	rt := rv.Type()
	for i := 0; i < rt.NumField(); i++ {
		field := rt.Field(i)
		if !field.IsExported() {
			continue
		}
		tagName := strings.SplitN(field.Tag.Get("json"), ",", 2)[0]
		if tagName == name || (tagName == "" && strings.EqualFold(field.Name, name)) {
			return rv.Field(i)
		}
	}
	return reflect.Value{}
}

func setPathField(field reflect.Value, raw string) error {
	if field.Kind() == reflect.Pointer {
		if field.IsNil() {
			field.Set(reflect.New(field.Type().Elem()))
		}
		field = field.Elem()
	}

	switch field.Kind() {
	case reflect.String:
		field.SetString(raw)
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		value, err := strconv.ParseInt(raw, 10, field.Type().Bits())
		if err != nil {
			return &Error{Status: http.StatusBadRequest, Code: CodeBadRequest,
				Message: "invalid integer path parameter: " + raw}
		}
		field.SetInt(value)
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64:
		value, err := strconv.ParseUint(raw, 10, field.Type().Bits())
		if err != nil {
			return &Error{Status: http.StatusBadRequest, Code: CodeBadRequest,
				Message: "invalid unsigned integer path parameter: " + raw}
		}
		field.SetUint(value)
	case reflect.Bool:
		value, err := strconv.ParseBool(raw)
		if err != nil {
			return &Error{Status: http.StatusBadRequest, Code: CodeBadRequest,
				Message: "invalid boolean path parameter: " + raw}
		}
		field.SetBool(value)
	default:
		return &Error{Status: http.StatusBadRequest, Code: CodeBadRequest,
			Message: "unsupported path parameter field type: " + field.Type().String()}
	}
	return nil
}

func normalizeHTTPPathPattern(path string) string {
	path = strings.TrimSpace(path)
	if path == "" {
		return "/"
	}
	if !strings.HasPrefix(path, "/") {
		path = "/" + path
	}

	segments := strings.Split(path, "/")
	for i, segment := range segments {
		if len(segment) <= 1 || segment[0] != ':' {
			continue
		}
		name := segment[1:]
		if !isHTTPPathParamName(name) {
			continue
		}
		segments[i] = "{" + name + "}"
	}
	return strings.Join(segments, "/")
}

func routePathParamNames(path string) []string {
	path = normalizeHTTPPathPattern(path)
	segments := strings.Split(path, "/")
	names := make([]string, 0)
	seen := map[string]struct{}{}
	for _, segment := range segments {
		if len(segment) >= 3 && segment[0] == '{' && segment[len(segment)-1] == '}' {
			name := strings.TrimSuffix(segment[1:len(segment)-1], "...")
			if name != "" {
				if _, ok := seen[name]; !ok {
					seen[name] = struct{}{}
					names = append(names, name)
				}
			}
		}
	}
	return names
}

func isHTTPPathParamName(name string) bool {
	if name == "" {
		return false
	}
	for _, ch := range name {
		if (ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') ||
			(ch >= '0' && ch <= '9') || ch == '_' {
			continue
		}
		return false
	}
	return true
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

// writeError encodes a runtime error as problem details. Non-Lazuli errors map
// to 500.
func writeError(w http.ResponseWriter, err error) {
	WriteProblem(w, ProblemFromError(err))
}

func handleFunc(mux *http.ServeMux, pattern string, handler func(http.ResponseWriter, *http.Request)) {
	mux.Handle(pattern, loggingMiddleware(http.HandlerFunc(handler)))
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
