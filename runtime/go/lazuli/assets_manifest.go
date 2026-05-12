package lazuli

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"path"
	"sort"
	"strings"
	"unicode"
)

// ErrInvalidAssetManifest is returned when an asset manifest cannot be parsed
// or contains unsafe asset paths.
var ErrInvalidAssetManifest = errors.New("lazuli: invalid asset manifest")

// AssetManifest maps stable logical asset names to generated asset file names.
//
// Manifest entries are safe relative asset paths after normalization. Values
// are expected to name fingerprinted build outputs such as
// "assets/app.a1b2c3d4.js".
type AssetManifest struct {
	assets map[string]string
}

// NewAssetManifest returns a validated manifest from logical asset names to
// generated asset file names. The input map is copied.
func NewAssetManifest(entries map[string]string) (AssetManifest, error) {
	if len(entries) == 0 {
		return AssetManifest{assets: map[string]string{}}, nil
	}

	keys := make([]string, 0, len(entries))
	for key := range entries {
		keys = append(keys, key)
	}
	sort.Strings(keys)

	assets := make(map[string]string, len(entries))
	originalNames := make(map[string]string, len(entries))
	for _, key := range keys {
		logical, err := cleanManifestAssetPath(key)
		if err != nil {
			return AssetManifest{}, invalidAssetManifest("logical asset %q: %v", key, err)
		}
		target, err := cleanManifestAssetPath(entries[key])
		if err != nil {
			return AssetManifest{}, invalidAssetManifest("asset %q target %q: %v", key, entries[key], err)
		}
		if previous, ok := originalNames[logical]; ok {
			return AssetManifest{}, invalidAssetManifest("logical asset %q normalizes to %q already used by %q", key, logical, previous)
		}
		originalNames[logical] = key
		assets[logical] = target
	}

	return AssetManifest{assets: assets}, nil
}

// LoadAssetManifest reads a JSON object mapping logical asset names to
// generated asset file names and returns a validated manifest.
func LoadAssetManifest(r io.Reader) (AssetManifest, error) {
	if r == nil {
		return AssetManifest{}, invalidAssetManifest("reader is nil")
	}

	decoder := json.NewDecoder(r)
	token, err := decoder.Token()
	if err != nil {
		if errors.Is(err, io.EOF) {
			return AssetManifest{}, invalidAssetManifest("empty input")
		}
		return AssetManifest{}, invalidAssetManifest("decode: %v", err)
	}
	delim, ok := token.(json.Delim)
	if !ok || delim != '{' {
		return AssetManifest{}, invalidAssetManifest("expected JSON object")
	}

	entries := map[string]string{}
	for decoder.More() {
		token, err := decoder.Token()
		if err != nil {
			return AssetManifest{}, invalidAssetManifest("decode object key: %v", err)
		}
		key, ok := token.(string)
		if !ok {
			return AssetManifest{}, invalidAssetManifest("expected string object key")
		}
		if _, exists := entries[key]; exists {
			return AssetManifest{}, invalidAssetManifest("duplicate logical asset %q", key)
		}

		var value string
		if err := decoder.Decode(&value); err != nil {
			return AssetManifest{}, invalidAssetManifest("asset %q target: %v", key, err)
		}
		entries[key] = value
	}

	token, err = decoder.Token()
	if err != nil {
		return AssetManifest{}, invalidAssetManifest("decode object end: %v", err)
	}
	delim, ok = token.(json.Delim)
	if !ok || delim != '}' {
		return AssetManifest{}, invalidAssetManifest("expected JSON object end")
	}

	if err := rejectTrailingJSON(decoder); err != nil {
		return AssetManifest{}, err
	}
	return NewAssetManifest(entries)
}

// Lookup returns the generated asset path for logicalName. The lookup name is
// normalized with the same path safety rules used while loading the manifest.
func (m AssetManifest) Lookup(logicalName string) (string, bool) {
	name, err := cleanManifestAssetPath(logicalName)
	if err != nil {
		return "", false
	}
	target, ok := m.assets[name]
	return target, ok
}

// Entries returns a copy of the manifest entries keyed by normalized logical
// asset name.
func (m AssetManifest) Entries() map[string]string {
	entries := make(map[string]string, len(m.assets))
	for key, value := range m.assets {
		entries[key] = value
	}
	return entries
}

// StaticFilesWithManifest returns a StaticFiles handler that redirects logical
// manifest asset requests to their generated asset URL before serving static
// files. Redirects are temporary because fingerprinted filenames change across
// builds.
func StaticFilesWithManifest(config StaticFileConfig, manifest AssetManifest) http.Handler {
	static := StaticFiles(config)
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method == http.MethodGet || r.Method == http.MethodHead {
			name, ok := cleanStaticRequestPath(r)
			if ok {
				if target, found := manifest.Lookup(name); found && target != name {
					http.Redirect(w, r, assetRedirectURL(target, r.URL.RawQuery), http.StatusTemporaryRedirect)
					return
				}
			}
		}
		static.ServeHTTP(w, r)
	})
}

func rejectTrailingJSON(decoder *json.Decoder) error {
	var extra any
	err := decoder.Decode(&extra)
	if errors.Is(err, io.EOF) {
		return nil
	}
	if err != nil {
		return invalidAssetManifest("decode trailing data: %v", err)
	}
	return invalidAssetManifest("unexpected trailing JSON value")
}

func cleanManifestAssetPath(name string) (string, error) {
	if strings.HasPrefix(name, "//") || strings.Contains(name, "://") {
		return "", errors.New("absolute URLs are not allowed")
	}
	if strings.ContainsAny(name, "?#") {
		return "", errors.New("query strings and fragments are not allowed")
	}
	for _, r := range name {
		if unicode.IsControl(r) {
			return "", errors.New("control characters are not allowed")
		}
	}

	cleaned, ok := cleanStaticAssetPath(name)
	if !ok || cleaned == "" || cleaned == "." {
		return "", errors.New("path must be a safe file path")
	}
	return cleaned, nil
}

func assetRedirectURL(target, rawQuery string) string {
	u := url.URL{
		Path: path.Join("/", target),
	}
	if rawQuery != "" {
		u.RawQuery = rawQuery
	}
	return u.String()
}

func invalidAssetManifest(format string, args ...any) error {
	return fmt.Errorf("%w: %s", ErrInvalidAssetManifest, fmt.Sprintf(format, args...))
}
