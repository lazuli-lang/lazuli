package openapi

import (
	"bytes"
	"html/template"
	"net/http"
	"net/url"
	"strconv"
	"strings"
)

const (
	// DefaultUITitle is the title used by UIHandler when UIConfig.Title is empty.
	DefaultUITitle = "OpenAPI"

	// ContentTypeHTML is the response Content-Type for the OpenAPI UI helper.
	ContentTypeHTML = "text/html; charset=utf-8"
)

const defaultUIContentSecurityPolicy = "default-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"

// UIConfig controls an OpenAPI UI handler.
type UIConfig struct {
	// SpecURL is the public URL of the OpenAPI artifact. It must be either a
	// safe root-relative path or an absolute http(s) URL.
	SpecURL string

	// Title is the optional HTML document title and heading. When empty,
	// DefaultUITitle is used.
	Title string

	// CacheControl is written as the Cache-Control header when non-empty.
	CacheControl string
}

// UIHandler returns a minimal HTML handler that links to an OpenAPI artifact.
//
// The handler serves only GET and HEAD requests and writes CSP-friendly HTML:
// no inline scripts, no inline styles, and no vendored Swagger/Redoc assets.
// UIHandler panics when SpecURL is empty or unsafe.
func UIHandler(config UIConfig) http.Handler {
	specURL, ok := cleanSpecURL(config.SpecURL)
	if !ok {
		panic("lazuli/openapi: UIConfig.SpecURL must be a safe root-relative or http(s) URL")
	}

	title := config.Title
	if title == "" {
		title = DefaultUITitle
	}

	body, err := renderUIHTML(uiTemplateData{
		Title:   title,
		SpecURL: specURL,
	})
	if err != nil {
		panic("lazuli/openapi: failed to render UI HTML: " + err.Error())
	}

	return &uiHandler{
		body:         body,
		cacheControl: config.CacheControl,
	}
}

type uiHandler struct {
	body         []byte
	cacheControl string
}

func (h *uiHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet && r.Method != http.MethodHead {
		w.Header().Set("Allow", "GET, HEAD")
		http.Error(w, http.StatusText(http.StatusMethodNotAllowed), http.StatusMethodNotAllowed)
		return
	}

	header := w.Header()
	header.Set("Content-Type", ContentTypeHTML)
	header.Set("Content-Length", strconv.Itoa(len(h.body)))
	header.Set("Content-Security-Policy", defaultUIContentSecurityPolicy)
	header.Set("X-Content-Type-Options", "nosniff")
	if h.cacheControl != "" {
		header.Set("Cache-Control", h.cacheControl)
	}

	w.WriteHeader(http.StatusOK)
	if r.Method == http.MethodHead {
		return
	}
	_, _ = w.Write(h.body)
}

type uiTemplateData struct {
	Title   string
	SpecURL string
}

var uiTemplate = template.Must(template.New("openapi-ui").Parse(`<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="openapi-spec-url" content="{{ .SpecURL }}">
<title>{{ .Title }}</title>
</head>
<body>
<main>
<h1>{{ .Title }}</h1>
<p>This service publishes an OpenAPI description.</p>
<p><a href="{{ .SpecURL }}" rel="noopener noreferrer">Open the OpenAPI document</a></p>
</main>
</body>
</html>
`))

func renderUIHTML(data uiTemplateData) ([]byte, error) {
	var body bytes.Buffer
	if err := uiTemplate.Execute(&body, data); err != nil {
		return nil, err
	}
	return body.Bytes(), nil
}

func cleanSpecURL(raw string) (string, bool) {
	if raw == "" || strings.TrimSpace(raw) != raw || strings.ContainsAny(raw, "\x00\\") {
		return "", false
	}

	parsed, err := url.Parse(raw)
	if err != nil {
		return "", false
	}
	if parsed.Fragment != "" || parsed.Opaque != "" || parsed.User != nil {
		return "", false
	}

	switch {
	case parsed.Scheme == "" && parsed.Host == "":
		return cleanRelativeSpecURL(parsed)
	case parsed.Scheme == "http" || parsed.Scheme == "https":
		return cleanAbsoluteSpecURL(parsed)
	default:
		return "", false
	}
}

func cleanRelativeSpecURL(parsed *url.URL) (string, bool) {
	if parsed.Path == "" || !strings.HasPrefix(parsed.Path, "/") {
		return "", false
	}
	if _, ok := cleanSpecURLPath(parsed); !ok {
		return "", false
	}
	return parsed.String(), true
}

func cleanAbsoluteSpecURL(parsed *url.URL) (string, bool) {
	if parsed.Host == "" {
		return "", false
	}
	if _, ok := cleanSpecURLPath(parsed); !ok {
		return "", false
	}
	return parsed.String(), true
}

func cleanSpecURLPath(parsed *url.URL) (string, bool) {
	pathURL := parsed.EscapedPath()
	if pathURL == "" {
		pathURL = parsed.Path
	}
	decoded, err := url.PathUnescape(pathURL)
	if err != nil {
		return "", false
	}
	return cleanServePath(decoded)
}
