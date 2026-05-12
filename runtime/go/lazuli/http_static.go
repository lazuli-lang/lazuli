package lazuli

import (
	"errors"
	"io/fs"
	"net/http"
	"net/url"
	"path"
	"strings"
)

const immutableStaticCacheControl = "public, max-age=31536000, immutable"

// StaticFileConfig controls StaticFiles.
type StaticFileConfig struct {
	// FS is an io/fs filesystem containing static assets. Exactly one of FS or
	// FileSystem must be set.
	FS fs.FS

	// FileSystem is an HTTP filesystem containing static assets. Exactly one of
	// FS or FileSystem must be set.
	FileSystem http.FileSystem

	// IndexFallback is the asset path served when a request does not resolve to
	// a file, for example "index.html" for browser-routed applications. Empty
	// disables fallback behavior.
	IndexFallback string

	// NotFound handles unresolved paths when IndexFallback is disabled or the
	// fallback asset is unavailable. When nil, http.NotFound is used.
	NotFound http.Handler

	// ImmutableCache enables long-lived immutable Cache-Control headers for
	// files whose names contain a fingerprint token such as app.abc12345.js.
	ImmutableCache bool
}

// StaticFiles returns an HTTP handler that serves static assets from config.
// It serves only GET and HEAD requests, rejects traversal attempts before path
// cleaning, never lists directories, and optionally serves an index fallback
// for unresolved application routes.
//
// StaticFiles panics when config does not set exactly one filesystem, or when
// IndexFallback is not a safe file path.
func StaticFiles(config StaticFileConfig) http.Handler {
	fileSystem := staticHTTPFileSystem(config)

	var indexFallback string
	if config.IndexFallback != "" {
		var ok bool
		indexFallback, ok = cleanStaticAssetPath(config.IndexFallback)
		if !ok || indexFallback == "" {
			panic("lazuli: StaticFileConfig.IndexFallback must be a safe file path")
		}
	}

	return &staticFileHandler{
		fileSystem:     fileSystem,
		indexFallback:  indexFallback,
		notFound:       config.NotFound,
		immutableCache: config.ImmutableCache,
	}
}

type staticFileHandler struct {
	fileSystem     http.FileSystem
	indexFallback  string
	notFound       http.Handler
	immutableCache bool
}

func (h *staticFileHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet && r.Method != http.MethodHead {
		w.Header().Set("Allow", "GET, HEAD")
		http.Error(w, http.StatusText(http.StatusMethodNotAllowed), http.StatusMethodNotAllowed)
		return
	}

	name, ok := cleanStaticRequestPath(r)
	if !ok {
		h.serveNotFound(w, r)
		return
	}

	if name != "" {
		switch h.serveFile(w, r, name) {
		case staticFileServed, staticFileFailed:
			return
		case staticFileMissing:
		}
	}

	if h.indexFallback != "" {
		switch h.serveFile(w, r, h.indexFallback) {
		case staticFileServed, staticFileFailed:
			return
		case staticFileMissing:
		}
	}

	h.serveNotFound(w, r)
}

func (h *staticFileHandler) serveNotFound(w http.ResponseWriter, r *http.Request) {
	if h.notFound != nil {
		h.notFound.ServeHTTP(w, r)
		return
	}
	http.NotFound(w, r)
}

type staticFileResult int

const (
	staticFileServed staticFileResult = iota
	staticFileMissing
	staticFileFailed
)

func (h *staticFileHandler) serveFile(w http.ResponseWriter, r *http.Request, name string) staticFileResult {
	file, err := h.fileSystem.Open("/" + name)
	if err != nil {
		switch {
		case errors.Is(err, fs.ErrNotExist):
			return staticFileMissing
		case errors.Is(err, fs.ErrInvalid), errors.Is(err, fs.ErrPermission):
			http.Error(w, http.StatusText(http.StatusForbidden), http.StatusForbidden)
			return staticFileFailed
		default:
			http.Error(w, http.StatusText(http.StatusInternalServerError), http.StatusInternalServerError)
			return staticFileFailed
		}
	}
	defer file.Close()

	info, err := file.Stat()
	if err != nil {
		if errors.Is(err, fs.ErrNotExist) {
			return staticFileMissing
		}
		http.Error(w, http.StatusText(http.StatusInternalServerError), http.StatusInternalServerError)
		return staticFileFailed
	}
	if info.IsDir() {
		return staticFileMissing
	}

	if h.immutableCache && isFingerprintedStaticAsset(name) {
		w.Header().Set("Cache-Control", immutableStaticCacheControl)
	}

	http.ServeContent(w, r, path.Base(name), info.ModTime(), file)
	return staticFileServed
}

func staticHTTPFileSystem(config StaticFileConfig) http.FileSystem {
	switch {
	case config.FS != nil && config.FileSystem != nil:
		panic("lazuli: StaticFileConfig must set FS or FileSystem, not both")
	case config.FS != nil:
		return http.FS(config.FS)
	case config.FileSystem != nil:
		return config.FileSystem
	default:
		panic("lazuli: StaticFileConfig requires FS or FileSystem")
	}
}

func cleanStaticRequestPath(r *http.Request) (string, bool) {
	raw := r.URL.EscapedPath()
	if raw == "" {
		raw = r.URL.Path
	}

	decoded, err := url.PathUnescape(raw)
	if err != nil {
		return "", false
	}
	return cleanStaticAssetPath(decoded)
}

func cleanStaticAssetPath(name string) (string, bool) {
	if name == "" {
		name = "/"
	}
	if strings.ContainsAny(name, "\x00\\") {
		return "", false
	}
	for _, segment := range strings.Split(name, "/") {
		if segment == ".." {
			return "", false
		}
	}
	return strings.TrimPrefix(path.Clean("/"+name), "/"), true
}

func isFingerprintedStaticAsset(name string) bool {
	base := path.Base(name)
	if path.Ext(base) == "" {
		return false
	}
	stem := strings.TrimSuffix(base, path.Ext(base))
	for _, part := range strings.FieldsFunc(stem, func(r rune) bool {
		return r == '.' || r == '-' || r == '_'
	}) {
		if isStaticFingerprintToken(part) {
			return true
		}
	}
	return false
}

func isStaticFingerprintToken(token string) bool {
	if len(token) < 8 {
		return false
	}

	hasLetter := false
	hasDigit := false
	for _, r := range token {
		switch {
		case 'a' <= r && r <= 'z':
			hasLetter = true
		case 'A' <= r && r <= 'Z':
			hasLetter = true
		case '0' <= r && r <= '9':
			hasDigit = true
		default:
			return false
		}
	}
	return hasLetter && hasDigit
}
