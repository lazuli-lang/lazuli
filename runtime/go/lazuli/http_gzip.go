package lazuli

import (
	"compress/gzip"
	"net/http"
	"strconv"
	"strings"
)

const gzipContentEncoding = "gzip"

// GzipMiddleware compresses eligible HTTP responses when the request's
// Accept-Encoding header allows gzip. It leaves responses untouched when they
// already declare Content-Encoding, opt out with Cache-Control: no-transform,
// or use a status code that must not carry a response body.
func GzipMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gw := &gzipResponseWriter{
			ResponseWriter: w,
			acceptsGzip:    acceptsGzipEncoding(r.Header.Values("Accept-Encoding")),
		}
		defer gw.close()

		next.ServeHTTP(gw, r)
	})
}

type gzipResponseWriter struct {
	http.ResponseWriter
	acceptsGzip bool
	gzipWriter  *gzip.Writer
	wroteHeader bool
	compressing bool
}

func (w *gzipResponseWriter) WriteHeader(code int) {
	if code >= 100 && code < 200 && code != http.StatusSwitchingProtocols {
		w.ResponseWriter.WriteHeader(code)
		return
	}
	if w.wroteHeader {
		return
	}
	w.wroteHeader = true

	if w.canTransform(code) {
		addVaryHeader(w.Header(), "Accept-Encoding")
		if w.acceptsGzip {
			w.Header().Set("Content-Encoding", gzipContentEncoding)
			w.Header().Del("Content-Length")
			w.gzipWriter = gzip.NewWriter(w.ResponseWriter)
			w.compressing = true
		}
	}

	w.ResponseWriter.WriteHeader(code)
}

func (w *gzipResponseWriter) Write(p []byte) (int, error) {
	if !w.wroteHeader {
		if w.Header().Get("Content-Type") == "" && len(p) > 0 {
			w.Header().Set("Content-Type", http.DetectContentType(p))
		}
		w.WriteHeader(http.StatusOK)
	}
	if !w.compressing {
		return w.ResponseWriter.Write(p)
	}
	return w.gzipWriter.Write(p)
}

func (w *gzipResponseWriter) Unwrap() http.ResponseWriter {
	return w.ResponseWriter
}

func (w *gzipResponseWriter) close() {
	if w.gzipWriter != nil {
		_ = w.gzipWriter.Close()
	}
}

func (w *gzipResponseWriter) canTransform(status int) bool {
	h := w.Header()
	return !statusMustNotHaveBody(status) &&
		strings.TrimSpace(h.Get("Content-Encoding")) == "" &&
		!cacheControlNoTransform(h.Values("Cache-Control"))
}

func acceptsGzipEncoding(values []string) bool {
	for _, header := range values {
		for _, part := range strings.Split(header, ",") {
			token, params, _ := strings.Cut(strings.TrimSpace(part), ";")
			if !strings.EqualFold(strings.TrimSpace(token), gzipContentEncoding) {
				continue
			}
			return encodingQualityAllows(params)
		}
	}
	return false
}

func encodingQualityAllows(params string) bool {
	for _, part := range strings.Split(params, ";") {
		name, value, ok := strings.Cut(strings.TrimSpace(part), "=")
		if !ok || !strings.EqualFold(strings.TrimSpace(name), "q") {
			continue
		}
		q, err := strconv.ParseFloat(strings.TrimSpace(value), 64)
		return err != nil || q > 0
	}
	return true
}

func cacheControlNoTransform(values []string) bool {
	for _, header := range values {
		for _, part := range strings.Split(header, ",") {
			directive, _, _ := strings.Cut(strings.TrimSpace(part), "=")
			if strings.EqualFold(strings.TrimSpace(directive), "no-transform") {
				return true
			}
		}
	}
	return false
}

func statusMustNotHaveBody(status int) bool {
	return (status >= 100 && status < 200) ||
		status == http.StatusNoContent ||
		status == http.StatusNotModified
}

func addVaryHeader(h http.Header, value string) {
	values := h.Values("Vary")
	if len(values) == 0 {
		h.Set("Vary", value)
		return
	}

	for _, headerValue := range values {
		for _, part := range strings.Split(headerValue, ",") {
			part = strings.TrimSpace(part)
			if part == "*" || strings.EqualFold(part, value) {
				return
			}
		}
	}

	combined := strings.TrimSpace(strings.Join(values, ", "))
	if combined == "" {
		h.Set("Vary", value)
		return
	}
	h.Set("Vary", combined+", "+value)
}
