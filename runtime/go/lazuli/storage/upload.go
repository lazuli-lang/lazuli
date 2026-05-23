package storage

import (
	"context"
	"errors"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/service/s3"
	s3types "github.com/aws/aws-sdk-go-v2/service/s3/types"
	"github.com/aws/smithy-go"
)

// ObjectStore is the adapter contract for the `object_storage`
// capability. Adapter packages (`@runtime/local`, `@runtime/s3`,
// `@lazuli/plugin-.../gcs`) implement this interface; Lazuli core never
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
	var limited *uploadSizeLimitReader
	if contract.MaxSize > 0 {
		// SECURITY (SEC-H7): metadata.Size is client-declared. Cap
		// the actual stream at the write boundary so a client cannot
		// claim a small payload and stream past FileContract.MaxSize.
		limited = newUploadSizeLimitReader(body, contract.MaxSize)
		body = limited
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

	key, err := mintKey(contract, metadata)
	if err != nil {
		return "", err
	}
	if err := store.Put(ctx, key, body, metadata.ContentType); err != nil {
		return "", err
	}
	if limited != nil && limited.BytesRead() > contract.MaxSize {
		_ = store.Delete(ctx, key)
		return "", ErrFileSizeExceeded
	}
	return key, nil
}

type uploadSizeLimitReader struct {
	r    io.Reader
	max  int64
	read int64
}

func newUploadSizeLimitReader(r io.Reader, max int64) *uploadSizeLimitReader {
	return &uploadSizeLimitReader{r: io.LimitReader(r, max+1), max: max}
}

func (r *uploadSizeLimitReader) Read(p []byte) (int, error) {
	n, err := r.r.Read(p)
	r.read += int64(n)
	if r.read > r.max {
		return n, ErrFileSizeExceeded
	}
	return n, err
}

func (r *uploadSizeLimitReader) BytesRead() int64 {
	return r.read
}

// LocalStore implements `ObjectStore` against the local filesystem.
// It is the canonical development adapter; production deployments
// bind `@runtime/s3` or a `@lazuli/plugin-.../<provider>` adapter.
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
	var path string
	if s.Root != "" {
		var err error
		abs, err := filepath.Abs(filepath.Join(s.Root, string(key)))
		if err != nil {
			return err
		}
		rootAbs, err := filepath.Abs(s.Root)
		if err != nil {
			return err
		}
		if !strings.HasPrefix(abs, rootAbs+string(filepath.Separator)) && abs != rootAbs {
			return errors.New("storage: resolved path escaped Root")
		}
		path = abs
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			return err
		}
		if err := os.WriteFile(path, bytes, 0o600); err != nil {
			return err
		}
	}
	s.mu.Lock()
	s.contents[key] = bytes
	s.mu.Unlock()
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

// s3API is the narrow surface of `*s3.Client` that S3Store consumes.
// Tests substitute a mock; production wires the live SDK client via
// `NewS3Store`.
type s3API interface {
	PutObject(ctx context.Context, in *s3.PutObjectInput, optFns ...func(*s3.Options)) (*s3.PutObjectOutput, error)
	GetObject(ctx context.Context, in *s3.GetObjectInput, optFns ...func(*s3.Options)) (*s3.GetObjectOutput, error)
	DeleteObject(ctx context.Context, in *s3.DeleteObjectInput, optFns ...func(*s3.Options)) (*s3.DeleteObjectOutput, error)
}

// S3Store wires the `@runtime/s3` adapter to `aws-sdk-go-v2`. The
// concrete client is built lazily by `NewS3Store` and stored in
// `client`; tests may swap `client` for a mock that satisfies the
// `s3API` interface.
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

	// client is the live SDK client. Populated by NewS3Store; tests
	// can override via WithS3Client to inject a fake.
	client s3API

	// presigner is the SDK presign client. Populated by NewS3Store
	// alongside `client`.
	presigner *s3.PresignClient
}

// NewS3Store builds an S3Store wired to a live `*s3.Client` and
// `*s3.PresignClient`. `cfg` is the SDK config (region, credentials,
// retries) the adapter resolved at boot from environment / AWS
// profile / `@lazuli/plugin-...` secret manager.
func NewS3Store(cfg aws.Config, bucket, region, prefix, endpoint string) *S3Store {
	opts := []func(*s3.Options){}
	if endpoint != "" {
		ep := endpoint
		opts = append(opts, func(o *s3.Options) {
			o.BaseEndpoint = aws.String(ep)
			o.UsePathStyle = true // safer default for non-AWS S3-compatible.
		})
	}
	client := s3.NewFromConfig(cfg, opts...)
	return &S3Store{
		Bucket:    bucket,
		Region:    region,
		Prefix:    prefix,
		Endpoint:  endpoint,
		client:    client,
		presigner: s3.NewPresignClient(client),
	}
}

// WithS3Client returns a copy of the store with `client` swapped.
// Test helper — production code uses `NewS3Store` and never touches
// the SDK client directly.
func (s *S3Store) WithS3Client(client s3API) *S3Store {
	cp := *s
	cp.client = client
	return &cp
}

// fullKey applies the bucket prefix if configured.
func (s *S3Store) fullKey(key Key) string {
	if s.Prefix == "" {
		return string(key)
	}
	return s.Prefix + "/" + string(key)
}

// Put uploads the body to S3 under bucket+key, tagging it with the
// supplied content-type. Wires `s3.PutObject` straight through.
func (s *S3Store) Put(ctx context.Context, key Key, body io.Reader, contentType string) error {
	if s.client == nil {
		return errors.New("lazuli/storage: S3Store has no client (call NewS3Store)")
	}
	in := &s3.PutObjectInput{
		Bucket: aws.String(s.Bucket),
		Key:    aws.String(s.fullKey(key)),
		Body:   body,
	}
	if contentType != "" {
		in.ContentType = aws.String(contentType)
	}
	_, err := s.client.PutObject(ctx, in)
	return err
}

// Get fetches the object body. Maps the S3 NoSuchKey API error to
// `ErrFileNotFound` so the rest of the runtime can branch on the
// typed sentinel.
func (s *S3Store) Get(ctx context.Context, key Key) (io.ReadCloser, error) {
	if s.client == nil {
		return nil, errors.New("lazuli/storage: S3Store has no client (call NewS3Store)")
	}
	out, err := s.client.GetObject(ctx, &s3.GetObjectInput{
		Bucket: aws.String(s.Bucket),
		Key:    aws.String(s.fullKey(key)),
	})
	if err != nil {
		var nsk *s3types.NoSuchKey
		if errors.As(err, &nsk) {
			return nil, ErrFileNotFound
		}
		var apiErr smithy.APIError
		if errors.As(err, &apiErr) && apiErr.ErrorCode() == "NotFound" {
			return nil, ErrFileNotFound
		}
		return nil, err
	}
	return out.Body, nil
}

// Sign generates a presigned GET URL valid for `ttl`. Wires
// `s3.PresignClient.PresignGetObject`.
func (s *S3Store) Sign(ctx context.Context, key Key, ttl time.Duration) (string, error) {
	if ttl <= 0 {
		return "", ErrVisibilityMismatch
	}
	if s.presigner == nil {
		return "", errors.New("lazuli/storage: S3Store has no presigner (call NewS3Store)")
	}
	signed, err := s.presigner.PresignGetObject(ctx, &s3.GetObjectInput{
		Bucket: aws.String(s.Bucket),
		Key:    aws.String(s.fullKey(key)),
	}, s3.WithPresignExpires(ttl))
	if err != nil {
		return "", err
	}
	return signed.URL, nil
}

// SignPut generates a presigned PUT URL valid for `ttl`. Lets the
// browser upload directly to S3 without proxying bytes through the
// app server.
func (s *S3Store) SignPut(ctx context.Context, key Key, contentType string, ttl time.Duration) (string, error) {
	if ttl <= 0 {
		return "", ErrVisibilityMismatch
	}
	if s.presigner == nil {
		return "", errors.New("lazuli/storage: S3Store has no presigner (call NewS3Store)")
	}
	in := &s3.PutObjectInput{
		Bucket: aws.String(s.Bucket),
		Key:    aws.String(s.fullKey(key)),
	}
	if contentType != "" {
		in.ContentType = aws.String(contentType)
	}
	signed, err := s.presigner.PresignPutObject(ctx, in, s3.WithPresignExpires(ttl))
	if err != nil {
		return "", err
	}
	return signed.URL, nil
}

// Delete removes the stored object.
func (s *S3Store) Delete(ctx context.Context, key Key) error {
	if s.client == nil {
		return errors.New("lazuli/storage: S3Store has no client (call NewS3Store)")
	}
	_, err := s.client.DeleteObject(ctx, &s3.DeleteObjectInput{
		Bucket: aws.String(s.Bucket),
		Key:    aws.String(s.fullKey(key)),
	})
	if err != nil {
		var nsk *s3types.NoSuchKey
		if errors.As(err, &nsk) {
			return ErrFileNotFound
		}
	}
	return err
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

func mintKey(contract FileContract, metadata Metadata) (Key, error) {
	stem := contract.Resource + "/" + contract.Field
	if stem == "/" {
		stem = "api/" + contract.API
	}
	if metadata.Filename != "" {
		name, err := sanitizeFilename(metadata.Filename)
		if err != nil {
			return "", err
		}
		return Key(stem + "/" + name), nil
	}
	return Key(stem), nil
}

// sanitizeFilename rejects path-traversal, leading slash, control chars,
// non-ASCII, and any character that's unsafe as a filename segment.
// SECURITY (SEC-M-2): LocalStore.Put resolves keys through filepath.Join
// which collapses `..` -- unsanitized filenames write outside Root.
func sanitizeFilename(name string) (string, error) {
	if name == "" {
		return "default", nil
	}
	if strings.ContainsAny(name, "/\\") {
		return "", errors.New("storage: filename contains path separator")
	}
	if strings.ContainsAny(name, `<>:"|?*`) {
		return "", errors.New("storage: filename contains unsafe char")
	}
	if strings.Contains(name, "..") {
		return "", errors.New("storage: filename contains parent traversal")
	}
	for _, r := range name {
		if r < 32 || r == 127 || r > 127 {
			return "", errors.New("storage: filename contains control or non-ASCII char")
		}
	}
	return name, nil
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
