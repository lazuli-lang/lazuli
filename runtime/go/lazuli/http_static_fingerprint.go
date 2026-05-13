package lazuli

import (
	"crypto/sha256"
	"encoding/hex"
	"io"
	"sort"
	"strings"
)

const (
	// StaticAssetDigestSHA256Prefix prefixes static asset content digests.
	StaticAssetDigestSHA256Prefix = "sha256:"
)

// StaticAssetFingerprintEntry describes one immutable generated static asset.
// LogicalName is the stable source name callers use in templates, while Path is
// the generated fingerprinted asset path served by StaticFiles.
type StaticAssetFingerprintEntry struct {
	LogicalName string `json:"logicalName"`
	Path        string `json:"path"`
	Digest      string `json:"digest,omitempty"`
}

// ETag returns the strongest stable entity tag available for entry.
func (e StaticAssetFingerprintEntry) ETag() string {
	if etag := StaticAssetDigestETag(e.Digest); etag != "" {
		return etag
	}

	path, err := cleanManifestAssetPath(e.Path)
	if err != nil {
		return ""
	}
	return ETag("asset:" + path)
}

// CacheMetadata returns immutable cache metadata for entry.
func (e StaticAssetFingerprintEntry) CacheMetadata() StaticAssetCacheMetadata {
	path, err := cleanManifestAssetPath(e.Path)
	if err != nil || !isFingerprintedStaticAsset(path) {
		return StaticAssetCacheMetadata{}
	}
	return StaticAssetImmutableCacheMetadata(e.ETag())
}

// StaticAssetCacheMetadata is the cache policy for a resolved static asset.
type StaticAssetCacheMetadata struct {
	CacheControl string
	ETag         string
	Immutable    bool
}

// Headers converts metadata to CacheHeaders for HTTP responses.
func (m StaticAssetCacheMetadata) Headers() CacheHeaders {
	return CacheHeaders{
		CacheControl: m.CacheControl,
		ETag:         m.ETag,
	}
}

// StaticAssetImmutableCacheMetadata returns the default immutable static asset
// cache metadata. The etag value may be raw or already formatted.
func StaticAssetImmutableCacheMetadata(etag string) StaticAssetCacheMetadata {
	return StaticAssetCacheMetadata{
		CacheControl: immutableStaticCacheControl,
		ETag:         ETag(etag),
		Immutable:    true,
	}
}

// StaticAssetFingerprintManifest indexes fingerprinted static assets by
// logical name and generated path.
type StaticAssetFingerprintManifest struct {
	entriesByLogical map[string]StaticAssetFingerprintEntry
	entriesByPath    map[string]StaticAssetFingerprintEntry
}

// NewStaticAssetFingerprintManifest returns a validated fingerprint manifest.
// Entry paths must use the same safe relative path rules as StaticFiles and
// must contain a fingerprint token such as app.a1b2c3d4.css.
func NewStaticAssetFingerprintManifest(entries []StaticAssetFingerprintEntry) (StaticAssetFingerprintManifest, error) {
	if len(entries) == 0 {
		return StaticAssetFingerprintManifest{
			entriesByLogical: map[string]StaticAssetFingerprintEntry{},
			entriesByPath:    map[string]StaticAssetFingerprintEntry{},
		}, nil
	}

	normalized := make([]StaticAssetFingerprintEntry, 0, len(entries))
	logicalOwners := make(map[string]string, len(entries))
	pathOwners := make(map[string]string, len(entries))

	for i, entry := range entries {
		normalizedEntry, err := normalizeStaticAssetFingerprintEntry(entry, i)
		if err != nil {
			return StaticAssetFingerprintManifest{}, err
		}

		if previous, ok := logicalOwners[normalizedEntry.LogicalName]; ok {
			return StaticAssetFingerprintManifest{}, invalidAssetManifest(
				"fingerprint entry[%d] logicalName %q normalizes to %q already used by %q",
				i,
				entry.LogicalName,
				normalizedEntry.LogicalName,
				previous,
			)
		}
		if previous, ok := pathOwners[normalizedEntry.Path]; ok {
			return StaticAssetFingerprintManifest{}, invalidAssetManifest(
				"fingerprint entry[%d] path %q normalizes to %q already used by %q",
				i,
				entry.Path,
				normalizedEntry.Path,
				previous,
			)
		}

		logicalOwners[normalizedEntry.LogicalName] = entry.LogicalName
		pathOwners[normalizedEntry.Path] = entry.LogicalName
		normalized = append(normalized, normalizedEntry)
	}

	for i, entry := range normalized {
		if owner, ok := pathOwners[entry.LogicalName]; ok && owner != entries[i].LogicalName {
			return StaticAssetFingerprintManifest{}, invalidAssetManifest(
				"fingerprint entry[%d] logicalName %q conflicts with generated path owned by %q",
				i,
				entries[i].LogicalName,
				owner,
			)
		}
	}

	byLogical := make(map[string]StaticAssetFingerprintEntry, len(normalized))
	byPath := make(map[string]StaticAssetFingerprintEntry, len(normalized))
	for _, entry := range normalized {
		byLogical[entry.LogicalName] = entry
		byPath[entry.Path] = entry
	}

	return StaticAssetFingerprintManifest{
		entriesByLogical: byLogical,
		entriesByPath:    byPath,
	}, nil
}

// NewStaticAssetFingerprintManifestFromAssetManifest converts the existing
// logical-name manifest shape into a fingerprint manifest.
func NewStaticAssetFingerprintManifestFromAssetManifest(manifest AssetManifest) (StaticAssetFingerprintManifest, error) {
	entries := manifest.Entries()
	keys := make([]string, 0, len(entries))
	for key := range entries {
		keys = append(keys, key)
	}
	sort.Strings(keys)

	fingerprints := make([]StaticAssetFingerprintEntry, 0, len(keys))
	for _, key := range keys {
		fingerprints = append(fingerprints, StaticAssetFingerprintEntry{
			LogicalName: key,
			Path:        entries[key],
		})
	}
	return NewStaticAssetFingerprintManifest(fingerprints)
}

// Lookup returns the manifest entry for logicalName after safe path
// normalization.
func (m StaticAssetFingerprintManifest) Lookup(logicalName string) (StaticAssetFingerprintEntry, bool) {
	name, err := cleanManifestAssetPath(logicalName)
	if err != nil {
		return StaticAssetFingerprintEntry{}, false
	}
	entry, ok := m.entriesByLogical[name]
	return entry, ok
}

// AssetPath returns the generated asset path for logicalName.
func (m StaticAssetFingerprintManifest) AssetPath(logicalName string) (string, bool) {
	entry, ok := m.Lookup(logicalName)
	if !ok {
		return "", false
	}
	return entry.Path, true
}

// Resolve returns the manifest entry for a safe logical name or generated path.
// It never resolves unknown assets by path cleaning alone.
func (m StaticAssetFingerprintManifest) Resolve(name string) (StaticAssetFingerprintEntry, bool) {
	cleaned, err := cleanManifestAssetPath(name)
	if err != nil {
		return StaticAssetFingerprintEntry{}, false
	}
	if entry, ok := m.entriesByLogical[cleaned]; ok {
		return entry, true
	}
	entry, ok := m.entriesByPath[cleaned]
	return entry, ok
}

// ResolvePath returns the generated asset path for a safe logical name or
// generated path known to the manifest.
func (m StaticAssetFingerprintManifest) ResolvePath(name string) (string, bool) {
	entry, ok := m.Resolve(name)
	if !ok {
		return "", false
	}
	return entry.Path, true
}

// CacheMetadata returns immutable cache metadata for a resolved manifest entry.
func (m StaticAssetFingerprintManifest) CacheMetadata(name string) (StaticAssetCacheMetadata, bool) {
	entry, ok := m.Resolve(name)
	if !ok {
		return StaticAssetCacheMetadata{}, false
	}
	return entry.CacheMetadata(), true
}

// Entries returns a copy of the normalized manifest entries sorted by logical
// name.
func (m StaticAssetFingerprintManifest) Entries() []StaticAssetFingerprintEntry {
	entries := make([]StaticAssetFingerprintEntry, 0, len(m.entriesByLogical))
	for _, entry := range m.entriesByLogical {
		entries = append(entries, entry)
	}
	sort.Slice(entries, func(i, j int) bool {
		return entries[i].LogicalName < entries[j].LogicalName
	})
	return entries
}

// StaticAssetSHA256Digest returns the canonical SHA-256 digest for content.
func StaticAssetSHA256Digest(content []byte) string {
	sum := sha256.Sum256(content)
	return StaticAssetDigestSHA256Prefix + hex.EncodeToString(sum[:])
}

// StaticAssetSHA256DigestFromReader streams r into SHA-256 and returns the
// canonical static asset digest.
func StaticAssetSHA256DigestFromReader(r io.Reader) (string, error) {
	if r == nil {
		return "", invalidAssetManifest("static asset digest reader is nil")
	}

	hasher := sha256.New()
	if _, err := io.Copy(hasher, r); err != nil {
		return "", err
	}
	return StaticAssetDigestSHA256Prefix + hex.EncodeToString(hasher.Sum(nil)), nil
}

// StaticAssetDigestETag returns an HTTP entity tag for a canonical or raw
// SHA-256 digest. Invalid digest values return an empty string.
func StaticAssetDigestETag(digest string) string {
	canonical, ok := canonicalStaticAssetDigest(digest)
	if !ok {
		return ""
	}
	return ETag(canonical)
}

func normalizeStaticAssetFingerprintEntry(entry StaticAssetFingerprintEntry, index int) (StaticAssetFingerprintEntry, error) {
	logicalName, err := cleanManifestAssetPath(entry.LogicalName)
	if err != nil {
		return StaticAssetFingerprintEntry{}, invalidAssetManifest("fingerprint entry[%d] logicalName %q: %v", index, entry.LogicalName, err)
	}

	assetPath, err := cleanManifestAssetPath(entry.Path)
	if err != nil {
		return StaticAssetFingerprintEntry{}, invalidAssetManifest("fingerprint entry[%d] path %q: %v", index, entry.Path, err)
	}
	if !isFingerprintedStaticAsset(assetPath) {
		return StaticAssetFingerprintEntry{}, invalidAssetManifest("fingerprint entry[%d] path %q is not fingerprinted", index, entry.Path)
	}

	digest := strings.TrimSpace(entry.Digest)
	if digest != "" {
		var ok bool
		digest, ok = canonicalStaticAssetDigest(digest)
		if !ok {
			return StaticAssetFingerprintEntry{}, invalidAssetManifest("fingerprint entry[%d] digest %q is not a sha256 digest", index, entry.Digest)
		}
	}

	return StaticAssetFingerprintEntry{
		LogicalName: logicalName,
		Path:        assetPath,
		Digest:      digest,
	}, nil
}

func canonicalStaticAssetDigest(digest string) (string, bool) {
	digest = strings.TrimSpace(digest)
	if digest == "" {
		return "", false
	}

	if algorithm, value, ok := strings.Cut(digest, ":"); ok {
		if !strings.EqualFold(strings.TrimSpace(algorithm), "sha256") {
			return "", false
		}
		digest = strings.TrimSpace(value)
	}

	if len(digest) != sha256.Size*2 {
		return "", false
	}
	for _, r := range digest {
		if !isStaticAssetHexRune(r) {
			return "", false
		}
	}
	return StaticAssetDigestSHA256Prefix + strings.ToLower(digest), true
}

func isStaticAssetHexRune(r rune) bool {
	return ('0' <= r && r <= '9') ||
		('a' <= r && r <= 'f') ||
		('A' <= r && r <= 'F')
}
