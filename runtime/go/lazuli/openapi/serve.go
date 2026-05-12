// Package openapi serves generated OpenAPI artifacts from the Go runtime.
package openapi

import (
	"errors"
	"io/fs"
	"net/http"
	"net/url"
	"path"
	"strconv"
	"strings"
)

const (
	// DefaultPath is the default public path for a YAML OpenAPI artifact.
	DefaultPath = "/openapi.yaml"

	// ContentTypeJSON is the response Content-Type for JSON OpenAPI artifacts.
	ContentTypeJSON = "application/json"

	// ContentTypeYAML is the response Content-Type for YAML OpenAPI artifacts.
	ContentTypeYAML = "application/yaml"
)

// Config controls an OpenAPI artifact handler.
type Config struct {
	// Path is the exact public path that serves the artifact. When empty,
	// DefaultPath is used.
	Path string

	// UIPath is an optional exact path that redirects to Path. It is a
	// placeholder for downstream Swagger UI, Redoc, or Stoplight adapters.
	UIPath string

	// FS contains the generated artifact. Exactly one of FS or Bytes must be
	// set.
	FS fs.FS

	// File is the artifact path inside FS. It is required when FS is set.
	File string

	// Bytes contains the generated artifact. Exactly one of FS or Bytes must be
	// set. The handler copies the bytes during construction.
	Bytes []byte

	// ContentType overrides automatic JSON/YAML content type selection when
	// non-empty.
	ContentType string

	// CacheControl is written as the Cache-Control header when non-empty.
	CacheControl string

	// NotFound handles requests for paths other than Path or UIPath. When nil,
	// http.NotFound is used.
	NotFound http.Handler
}

// Handler returns an HTTP handler that serves one generated OpenAPI artifact.
//
// The handler serves only GET and HEAD requests to Config.Path. Unsupported
// methods on the artifact path return 405 with Allow: GET, HEAD. Requests for
// any other path use Config.NotFound or http.NotFound. Handler panics when the
// source or configured paths are invalid, or when an FS artifact cannot be read.
func Handler(config Config) http.Handler {
	return newHandler(config)
}

// ServeMux returns a new ServeMux with the configured artifact path mounted.
// UIPath is mounted too when set. When Config.NotFound is set, the mux uses it
// as a catch-all fallback for paths not handled by the artifact routes.
func ServeMux(config Config) *http.ServeMux {
	handler := newHandler(config)
	mux := http.NewServeMux()
	mux.Handle(handler.path, handler)
	if handler.uiPath != "" {
		mux.Handle(handler.uiPath, handler)
	}
	if handler.notFound != nil {
		mux.Handle("/", handler.notFound)
	}
	return mux
}

type artifactHandler struct {
	path         string
	uiPath       string
	body         []byte
	contentType  string
	cacheControl string
	notFound     http.Handler
}

func newHandler(config Config) *artifactHandler {
	publicPath := config.Path
	if publicPath == "" {
		publicPath = DefaultPath
	}

	servePath, ok := cleanServePath(publicPath)
	if !ok {
		panic("lazuli/openapi: Config.Path must be a safe absolute path")
	}

	var uiPath string
	if config.UIPath != "" {
		var ok bool
		uiPath, ok = cleanServePath(config.UIPath)
		if !ok {
			panic("lazuli/openapi: Config.UIPath must be a safe absolute path")
		}
		if uiPath == servePath {
			panic("lazuli/openapi: Config.UIPath must differ from Config.Path")
		}
	}

	body, file := loadArtifact(config)
	return &artifactHandler{
		path:         servePath,
		uiPath:       uiPath,
		body:         body,
		contentType:  artifactContentType(config.ContentType, servePath, file),
		cacheControl: config.CacheControl,
		notFound:     config.NotFound,
	}
}

func (h *artifactHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	requestPath, ok := cleanRequestPath(r)
	if !ok {
		h.serveNotFound(w, r)
		return
	}

	switch requestPath {
	case h.path:
		h.serveArtifact(w, r)
	case h.uiPath:
		h.redirectUI(w, r)
	default:
		h.serveNotFound(w, r)
	}
}

func (h *artifactHandler) serveArtifact(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet && r.Method != http.MethodHead {
		w.Header().Set("Allow", "GET, HEAD")
		http.Error(w, http.StatusText(http.StatusMethodNotAllowed), http.StatusMethodNotAllowed)
		return
	}

	header := w.Header()
	header.Set("Content-Type", h.contentType)
	header.Set("Content-Length", strconv.Itoa(len(h.body)))
	if h.cacheControl != "" {
		header.Set("Cache-Control", h.cacheControl)
	}

	w.WriteHeader(http.StatusOK)
	if r.Method == http.MethodHead {
		return
	}
	_, _ = w.Write(h.body)
}

func (h *artifactHandler) redirectUI(w http.ResponseWriter, r *http.Request) {
	if h.uiPath == "" {
		h.serveNotFound(w, r)
		return
	}
	if r.Method != http.MethodGet && r.Method != http.MethodHead {
		w.Header().Set("Allow", "GET, HEAD")
		http.Error(w, http.StatusText(http.StatusMethodNotAllowed), http.StatusMethodNotAllowed)
		return
	}

	w.Header().Set("Location", h.path)
	w.WriteHeader(http.StatusTemporaryRedirect)
}

func (h *artifactHandler) serveNotFound(w http.ResponseWriter, r *http.Request) {
	if h.notFound != nil {
		h.notFound.ServeHTTP(w, r)
		return
	}
	http.NotFound(w, r)
}

func loadArtifact(config Config) ([]byte, string) {
	hasFS := config.FS != nil
	hasBytes := config.Bytes != nil

	switch {
	case hasFS && hasBytes:
		panic("lazuli/openapi: Config must set FS or Bytes, not both")
	case hasFS:
		if config.File == "" {
			panic("lazuli/openapi: Config.File is required when FS is set")
		}
		file, ok := cleanFSPath(config.File)
		if !ok {
			panic("lazuli/openapi: Config.File must be a safe fs path")
		}

		body, err := fs.ReadFile(config.FS, file)
		if err != nil {
			if errors.Is(err, fs.ErrNotExist) {
				panic("lazuli/openapi: Config.File does not exist")
			}
			panic("lazuli/openapi: failed to read Config.File: " + err.Error())
		}
		return body, file
	case hasBytes:
		return append([]byte(nil), config.Bytes...), config.File
	default:
		panic("lazuli/openapi: Config requires FS or Bytes")
	}
}

func artifactContentType(override, publicPath, file string) string {
	if override != "" {
		return override
	}

	for _, name := range []string{publicPath, file} {
		switch strings.ToLower(path.Ext(name)) {
		case ".json":
			return ContentTypeJSON
		case ".yaml", ".yml":
			return ContentTypeYAML
		}
	}
	return ContentTypeYAML
}

func cleanRequestPath(r *http.Request) (string, bool) {
	raw := r.URL.EscapedPath()
	if raw == "" {
		raw = r.URL.Path
	}

	decoded, err := url.PathUnescape(raw)
	if err != nil {
		return "", false
	}
	return cleanServePath(decoded)
}

func cleanServePath(name string) (string, bool) {
	if name == "" || !strings.HasPrefix(name, "/") {
		return "", false
	}
	if strings.ContainsAny(name, "\x00\\{}") {
		return "", false
	}
	for _, segment := range strings.Split(name, "/") {
		if segment == ".." {
			return "", false
		}
	}
	cleaned := path.Clean(name)
	if cleaned == "." || cleaned == "/" {
		return "", false
	}
	return cleaned, true
}

func cleanFSPath(name string) (string, bool) {
	if name == "" || strings.HasPrefix(name, "/") {
		return "", false
	}
	if strings.ContainsAny(name, "\x00\\") {
		return "", false
	}
	for _, segment := range strings.Split(name, "/") {
		if segment == ".." {
			return "", false
		}
	}
	cleaned := path.Clean(name)
	return cleaned, cleaned != "." && fs.ValidPath(cleaned)
}
