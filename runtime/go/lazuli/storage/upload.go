package storage

import (
	"context"
	"errors"
	"io"
	"os"
	"path/filepath"
	"sync"
	"time"
)

// ObjectStore is the adapter contract for the `object_storage`
// capability. Adapter packages (`@runtime/local`, `@runtime/s3`,
// `@plugin/.../gcs`) implement this interface; Lazuli core never
// names any of them.
type ObjectStore interface {
	// Put streams the body into the adapter's backing store
	// under `key` and tags it with `contentType`. Implementations
	// must be idempotent on `key`.
	Put(ctx context.Context, key Key, body io.Reader, contentType string) error

	// Get returns a readable stream for the stored object.
	// Returns `ErrFileNotFound` if the key is unknown.
	Get(ctx context.Context, key Key) (io.ReadCloser, error)

	// Sign returns an URL valid for `ttl` that grants raw fetch
	// access to the stored object. Adapter chooses the signing
	// scheme (HMAC for S3-compatible; token + TTL index for local).
	Sign(ctx context.Context, key Key, ttl time.Duration) (string, error)

	// Delete removes the stored object. Returns `ErrFileNotFound`
	// if the key is unknown.
	Delete(ctx context.Context, key Key) error
}

// Upload validates the body against `contract` and streams it
// through `store`. Returns an opaque `Key` that the calling
// command persists as the `@cap.File` field value.
//
// Implementation note: the real validation pipeline (multipart
// frame parsing, MIME sniffing, size accounting) is runtime-team
// work. This stub does the language-side typing (max-size check
// when `metadata.Size > 0`, MIME accept check when `metadata.
// ContentType` is set) and delegates the byte stream to the bound
// `ObjectStore`. The adapter owns durability and atomicity.
func Upload(
	ctx context.Context,
	contract FileContract,
	store ObjectStore,
	body io.Reader,
	metadata Metadata,
) (Key, error) {
	if metadata.Size > 0 && contract.MaxSize > 0 && metadata.Size > contract.MaxSize {
		return "", ErrFileSizeExceeded
	}
	if metadata.ContentType != "" && len(contract.Accept) > 0 {
		got := parseMime(metadata.ContentType)
		matched := false
		for _, accept := range contract.Accept {
			if accept.Matches(got) {
				matched = true
				break
			}
		}
		if !matched {
			return "", ErrFileMimeRejected
		}
	}

	key := mintKey(contract, metadata)
	if err := store.Put(ctx, key, body, metadata.ContentType); err != nil {
		return "", err
	}
	return key, nil
}

// LocalStore implements `ObjectStore` against the local filesystem.
// It is the canonical development adapter; production deployments
// bind `@runtime/s3` or a `@plugin/.../<provider>` adapter.
type LocalStore struct {
	// Root is the directory that all objects are written under.
	Root string

	// Clock returns the current time. Defaults to `time.Now` when
	// nil; tests inject `testing/synctest`-friendly clocks.
	Clock func() time.Time

	mu       sync.Mutex
	tokens   map[string]localSignedToken
	contents map[Key][]byte
}

type localSignedToken struct {
	key    Key
	expiry time.Time
}

// NewLocalStore returns a fresh in-memory + filesystem store. When
// `root` is empty the store stays memory-only — useful for tests.
func NewLocalStore(root string) *LocalStore {
	return &LocalStore{
		Root:     root,
		tokens:   make(map[string]localSignedToken),
		contents: make(map[Key][]byte),
	}
}

func (s *LocalStore) now() time.Time {
	if s.Clock != nil {
		return s.Clock()
	}
	return time.Now()
}

// Put writes the body under `key`. When `Root` is set, also
// persists the bytes to disk.
func (s *LocalStore) Put(_ context.Context, key Key, body io.Reader, _ string) error {
	bytes, err := io.ReadAll(body)
	if err != nil {
		return err
	}
	s.mu.Lock()
	s.contents[key] = bytes
	s.mu.Unlock()
	if s.Root != "" {
		path := filepath.Join(s.Root, string(key))
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			return err
		}
		if err := os.WriteFile(path, bytes, 0o600); err != nil {
			return err
		}
	}
	return nil
}

// Get returns a ReadCloser over the stored bytes.
func (s *LocalStore) Get(_ context.Context, key Key) (io.ReadCloser, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	bytes, ok := s.contents[key]
	if !ok {
		return nil, ErrFileNotFound
	}
	return io.NopCloser(newBytesReader(bytes)), nil
}

// Sign issues a memory-backed signed URL token. The local adapter
// does not produce a real HTTP URL — that's the gateway's
// responsibility — but the token is sufficient to test TTL semantics.
func (s *LocalStore) Sign(_ context.Context, key Key, ttl time.Duration) (string, error) {
	if ttl <= 0 {
		return "", ErrVisibilityMismatch
	}
	token := mintToken(key, s.now(), ttl)
	s.mu.Lock()
	s.tokens[token] = localSignedToken{
		key:    key,
		expiry: s.now().Add(ttl),
	}
	s.mu.Unlock()
	return token, nil
}

// Delete removes the stored object.
func (s *LocalStore) Delete(_ context.Context, key Key) error {
	s.mu.Lock()
	_, ok := s.contents[key]
	if ok {
		delete(s.contents, key)
	}
	s.mu.Unlock()
	if !ok {
		return ErrFileNotFound
	}
	return nil
}

// Resolve looks up `token` against the internal index and returns
// the underlying key if the token is unexpired. Test helper —
// real adapters resolve through an HTTP middleware.
func (s *LocalStore) Resolve(token string) (Key, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	entry, ok := s.tokens[token]
	if !ok {
		return "", ErrFileNotFound
	}
	if !s.now().Before(entry.expiry) {
		return "", ErrSignedURLExpired
	}
	return entry.key, nil
}

// S3Store is a stub for the `@runtime/s3` adapter. The concrete
// implementation belongs to the adapter package (it wraps
// `aws-sdk-go-v2`); this declaration only fixes the type surface
// so generated code can reference `*storage.S3Store` without
// compiling against the AWS SDK in core tests.
type S3Store struct {
	// Bucket is the target S3 bucket name. Required.
	Bucket string

	// Region is the AWS region. Required.
	Region string

	// Prefix is an optional key prefix prepended to every Put.
	Prefix string

	// Endpoint allows pointing at an S3-compatible store (R2,
	// MinIO). When empty, defaults to the public AWS endpoint
	// for `Region`.
	Endpoint string

	// SigningClock is the time source used for signed URLs.
	// Defaults to `time.Now`.
	SigningClock func() time.Time
}

// Put is a stub — the real implementation lives in the
// `@runtime/s3` adapter package.
func (s *S3Store) Put(_ context.Context, _ Key, _ io.Reader, _ string) error {
	return errors.New("lazuli/storage: S3Store.Put is implemented by the @runtime/s3 adapter")
}

// Get is a stub.
func (s *S3Store) Get(_ context.Context, _ Key) (io.ReadCloser, error) {
	return nil, errors.New("lazuli/storage: S3Store.Get is implemented by the @runtime/s3 adapter")
}

// Sign is a stub.
func (s *S3Store) Sign(_ context.Context, _ Key, _ time.Duration) (string, error) {
	return "", errors.New("lazuli/storage: S3Store.Sign is implemented by the @runtime/s3 adapter")
}

// Delete is a stub.
func (s *S3Store) Delete(_ context.Context, _ Key) error {
	return errors.New("lazuli/storage: S3Store.Delete is implemented by the @runtime/s3 adapter")
}

// --- internal helpers -------------------------------------------------------

func parseMime(raw string) MimeType {
	// Trim parameters (`;charset=utf-8`); the analyzer never
	// emits parameters in `accept:` so the simple split below
	// is safe.
	for i := 0; i < len(raw); i++ {
		if raw[i] == ';' {
			raw = raw[:i]
			break
		}
	}
	for i := 0; i < len(raw); i++ {
		if raw[i] == '/' {
			return MimeType{Family: raw[:i], Subtype: raw[i+1:]}
		}
	}
	return MimeType{Family: raw, Subtype: ""}
}

func mintKey(contract FileContract, metadata Metadata) Key {
	stem := contract.Resource + "/" + contract.Field
	if stem == "/" {
		stem = "api/" + contract.API
	}
	if metadata.Filename != "" {
		return Key(stem + "/" + metadata.Filename)
	}
	return Key(stem)
}

func mintToken(key Key, now time.Time, ttl time.Duration) string {
	// Deterministic token: keeps the test surface stable. Real
	// adapters use cryptographically signed URLs; this is the
	// local-dev fallback.
	return string(key) + "?expires=" + now.Add(ttl).Format(time.RFC3339Nano)
}

// bytesReader is a minimal `io.Reader` over a byte slice. We use
// it instead of `bytes.NewReader` to keep this file's import
// graph small (the rest of the package only needs stdlib).
type bytesReader struct {
	data []byte
	pos  int
}

func newBytesReader(data []byte) *bytesReader {
	return &bytesReader{data: data}
}

func (r *bytesReader) Read(p []byte) (int, error) {
	if r.pos >= len(r.data) {
		return 0, io.EOF
	}
	n := copy(p, r.data[r.pos:])
	r.pos += n
	return n, nil
}
