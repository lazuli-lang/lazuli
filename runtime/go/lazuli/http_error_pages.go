package lazuli

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"strconv"
	"strings"
)

const (
	contentTypeProblemJSON = "application/problem+json"
	contentTypePlainText   = "text/plain; charset=utf-8"
	contentTypeHTML        = "text/html; charset=utf-8"
)

var errNilHTTPErrorPageRenderer = errors.New("nil HTTP error page renderer")

// HTTPErrorPage is a rendered custom error page response body.
//
// ContentType defaults to text/html; charset=utf-8 when empty.
type HTTPErrorPage struct {
	ContentType string
	Body        []byte
}

// HTTPErrorPageRenderer renders a custom error page for browser-facing
// requests. The runtime writes the status code and falls back to problem JSON
// or plain text when the renderer fails.
type HTTPErrorPageRenderer interface {
	RenderHTTPErrorPage(r *http.Request, problem Problem) (HTTPErrorPage, error)
}

// HTTPErrorPageRendererFunc adapts a function to HTTPErrorPageRenderer.
type HTTPErrorPageRendererFunc func(r *http.Request, problem Problem) (HTTPErrorPage, error)

// RenderHTTPErrorPage implements HTTPErrorPageRenderer.
func (f HTTPErrorPageRendererFunc) RenderHTTPErrorPage(r *http.Request, problem Problem) (HTTPErrorPage, error) {
	if f == nil {
		return HTTPErrorPage{}, errNilHTTPErrorPageRenderer
	}
	return f(r, problem)
}

// WriteHTTPStatusError writes status as a negotiated error response. When the
// request accepts HTML and renderer is non-nil, renderer is used for the body.
// Otherwise the response falls back to RFC 9457 problem JSON or plain text.
func WriteHTTPStatusError(w http.ResponseWriter, r *http.Request, status int, renderer HTTPErrorPageRenderer) {
	WriteHTTPProblem(w, r, Problem{Status: status}, renderer)
}

// WriteHTTPProblem writes problem as a negotiated error response. Renderer
// failures are logged and fall back to the built-in problem JSON/text writers.
func WriteHTTPProblem(w http.ResponseWriter, r *http.Request, problem Problem, renderer HTTPErrorPageRenderer) {
	if w == nil {
		return
	}

	problem = normalizeProblem(problem)
	addVaryHeader(w.Header(), "Accept")

	if negotiateHTTPErrorFormat(r, renderer != nil) == httpErrorFormatHTML {
		page, err := renderHTTPErrorPage(renderer, r, problem)
		if err == nil {
			writeHTTPErrorPage(w, problem.Status, page)
			return
		}
		slog.Error("lazuli: failed to render HTTP error page",
			"error", err,
			"status", problem.Status,
		)
	}

	writeHTTPErrorFallback(w, r, problem)
}

type httpErrorFormat uint8

const (
	httpErrorFormatProblemJSON httpErrorFormat = iota
	httpErrorFormatHTML
	httpErrorFormatText
)

func renderHTTPErrorPage(renderer HTTPErrorPageRenderer, r *http.Request, problem Problem) (page HTTPErrorPage, err error) {
	defer func() {
		if rec := recover(); rec != nil {
			err = fmt.Errorf("panic rendering HTTP error page: %v", rec)
		}
	}()
	return renderer.RenderHTTPErrorPage(r, problem)
}

func writeHTTPErrorPage(w http.ResponseWriter, status int, page HTTPErrorPage) {
	contentType := strings.TrimSpace(page.ContentType)
	if contentType == "" {
		contentType = contentTypeHTML
	}
	w.Header().Set("Content-Type", contentType)
	w.WriteHeader(status)
	if statusMustNotHaveBody(status) {
		return
	}
	if _, err := w.Write(page.Body); err != nil {
		slog.Error("lazuli: failed to write HTTP error page", "error", err)
	}
}

func writeHTTPErrorFallback(w http.ResponseWriter, r *http.Request, problem Problem) {
	switch negotiateHTTPErrorFormat(r, false) {
	case httpErrorFormatText:
		writeProblemText(w, problem)
	default:
		writeProblemJSON(w, problem)
	}
}

func writeProblemJSON(w http.ResponseWriter, problem Problem) {
	w.Header().Set("Content-Type", contentTypeProblemJSON)
	w.WriteHeader(problem.Status)
	if statusMustNotHaveBody(problem.Status) {
		return
	}
	if err := json.NewEncoder(w).Encode(problem); err != nil {
		slog.Error("lazuli: failed to encode problem response", "error", err)
	}
}

func writeProblemText(w http.ResponseWriter, problem Problem) {
	w.Header().Set("Content-Type", contentTypePlainText)
	w.WriteHeader(problem.Status)
	if statusMustNotHaveBody(problem.Status) {
		return
	}
	if _, err := io.WriteString(w, problemText(problem)); err != nil {
		slog.Error("lazuli: failed to write problem text response", "error", err)
	}
}

func problemText(problem Problem) string {
	if problem.Detail != "" {
		return problem.Detail + "\n"
	}
	if problem.Title != "" {
		return problem.Title + "\n"
	}
	return http.StatusText(problem.Status) + "\n"
}

func negotiateHTTPErrorFormat(r *http.Request, hasRenderer bool) httpErrorFormat {
	if r == nil {
		return httpErrorFormatProblemJSON
	}
	accept := strings.TrimSpace(r.Header.Get("Accept"))
	if accept == "" {
		return httpErrorFormatProblemJSON
	}

	best := httpErrorFormatProblemJSON
	bestQ := -1.0
	bestOrder := len(strings.Split(accept, ",")) + 1
	for order, part := range strings.Split(accept, ",") {
		mediaType, q, ok := parseAcceptPart(part)
		if !ok || q <= 0 {
			continue
		}
		format, ok := httpErrorFormatForMediaType(mediaType, hasRenderer)
		if !ok {
			continue
		}
		if q > bestQ || (q == bestQ && order < bestOrder) {
			best = format
			bestQ = q
			bestOrder = order
		}
	}
	if bestQ >= 0 {
		return best
	}
	if acceptsOnlyHTML(accept) {
		return httpErrorFormatText
	}
	return httpErrorFormatProblemJSON
}

func parseAcceptPart(part string) (string, float64, bool) {
	part = strings.TrimSpace(part)
	if part == "" {
		return "", 0, false
	}

	mediaType, params, _ := strings.Cut(part, ";")
	mediaType = strings.ToLower(strings.TrimSpace(mediaType))
	if mediaType == "" {
		return "", 0, false
	}

	q := 1.0
	for params != "" {
		var param string
		param, params, _ = strings.Cut(params, ";")
		name, value, ok := strings.Cut(strings.TrimSpace(param), "=")
		if !ok || !strings.EqualFold(strings.TrimSpace(name), "q") {
			continue
		}
		parsed, err := strconv.ParseFloat(strings.Trim(strings.TrimSpace(value), `"`), 64)
		if err != nil {
			continue
		}
		q = parsed
	}
	return mediaType, q, true
}

func httpErrorFormatForMediaType(mediaType string, hasRenderer bool) (httpErrorFormat, bool) {
	switch {
	case mediaType == "text/html":
		if hasRenderer {
			return httpErrorFormatHTML, true
		}
		return 0, false
	case mediaType == "text/plain":
		return httpErrorFormatText, true
	case mediaType == contentTypeProblemJSON || mediaType == "application/json" || strings.HasSuffix(mediaType, "+json"):
		return httpErrorFormatProblemJSON, true
	case mediaType == "application/*":
		return httpErrorFormatProblemJSON, true
	case mediaType == "text/*":
		if hasRenderer {
			return httpErrorFormatHTML, true
		}
		return httpErrorFormatText, true
	case mediaType == "*/*":
		return httpErrorFormatProblemJSON, true
	default:
		return 0, false
	}
}

func acceptsOnlyHTML(accept string) bool {
	for _, part := range strings.Split(accept, ",") {
		mediaType, q, ok := parseAcceptPart(part)
		if !ok || q <= 0 {
			continue
		}
		if mediaType == "text/html" || mediaType == "text/*" {
			return true
		}
	}
	return false
}
