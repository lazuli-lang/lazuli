package storage

import (
	"context"
	"errors"
	"io"
	"iter"
	"strings"
	"testing"
	"time"
)

// fakeProvider implements `Provider` against an in-memory map. Used
// by the runtime smoke tests for the `Provider` contract and by
// `lazuli.ObjectStore("<name>")` resolution tests in the parent
// `lazuli` package (see `runtime/go/lazuli/object_store_test.go`).
type fakeProvider struct {
	objects map[string][]byte
	mimes   map[string]string
}

func newFakeProvider() *fakeProvider {
	return &fakeProvider{
		objects: make(map[string][]byte),
		mimes:   make(map[string]string),
	}
}

func (f *fakeProvider) bucketKey(bucket, key string) string {
	return bucket + "/" + key
}

func (f *fakeProvider) PutObject(ctx context.Context, bucket, key, contentType string, body io.Reader) error {
	_ = ctx
	bytes, err := io.ReadAll(body)
	if err != nil {
		return err
	}
	bk := f.bucketKey(bucket, key)
	f.objects[bk] = bytes
	f.mimes[bk] = contentType
	return nil
}

func (f *fakeProvider) GetObject(ctx context.Context, bucket, key string) (io.ReadCloser, string, error) {
	_ = ctx
	bk := f.bucketKey(bucket, key)
	bytes, ok := f.objects[bk]
	if !ok {
		return nil, "", ErrFileNotFound
	}
	return io.NopCloser(strings.NewReader(string(bytes))), f.mimes[bk], nil
}

func (f *fakeProvider) DeleteObject(ctx context.Context, bucket, key string) error {
	_ = ctx
	bk := f.bucketKey(bucket, key)
	if _, ok := f.objects[bk]; !ok {
		return ErrFileNotFound
	}
	delete(f.objects, bk)
	delete(f.mimes, bk)
	return nil
}

func (f *fakeProvider) ListPrefix(ctx context.Context, bucket, prefix string) iter.Seq2[ObjectMeta, error] {
	_ = ctx
	return func(yield func(ObjectMeta, error) bool) {
		bp := bucket + "/" + prefix
		for bk, bytes := range f.objects {
			if !strings.HasPrefix(bk, bp) {
				continue
			}
			key := strings.TrimPrefix(bk, bucket+"/")
			meta := ObjectMeta{
				Key:         key,
				Size:        int64(len(bytes)),
				ContentType: f.mimes[bk],
			}
			if !yield(meta, nil) {
				return
			}
		}
	}
}

func (f *fakeProvider) PresignedURL(ctx context.Context, bucket, key string, ttl time.Duration, method string) (string, error) {
	_ = ctx
	if ttl <= 0 {
		return "", ErrVisibilityMismatch
	}
	bk := f.bucketKey(bucket, key)
	return method + " fake://" + bk + "?ttl=" + ttl.String(), nil
}

func TestProviderRoundTrip(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	p := newFakeProvider()

	// Put + Get
	if err := p.PutObject(ctx, "media", "photo/a.jpg", "image/jpeg", strings.NewReader("bytes")); err != nil {
		t.Fatalf("PutObject: %v", err)
	}
	body, mime, err := p.GetObject(ctx, "media", "photo/a.jpg")
	if err != nil {
		t.Fatalf("GetObject: %v", err)
	}
	got, _ := io.ReadAll(body)
	body.Close()
	if string(got) != "bytes" {
		t.Errorf("GetObject body: want 'bytes', got %q", string(got))
	}
	if mime != "image/jpeg" {
		t.Errorf("GetObject mime: want 'image/jpeg', got %q", mime)
	}

	// PresignedURL
	url, err := p.PresignedURL(ctx, "media", "photo/a.jpg", time.Hour, "PUT")
	if err != nil {
		t.Fatalf("PresignedURL: %v", err)
	}
	if !strings.HasPrefix(url, "PUT fake://media/photo/a.jpg") {
		t.Errorf("PresignedURL url: %q", url)
	}

	// Delete
	if err := p.DeleteObject(ctx, "media", "photo/a.jpg"); err != nil {
		t.Fatalf("DeleteObject: %v", err)
	}
	if _, _, err := p.GetObject(ctx, "media", "photo/a.jpg"); !errors.Is(err, ErrFileNotFound) {
		t.Errorf("GetObject after Delete: want ErrFileNotFound, got %v", err)
	}
}

func TestProviderPresignedURLZeroTTL(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	p := newFakeProvider()
	_, err := p.PresignedURL(ctx, "media", "k", 0, "GET")
	if !errors.Is(err, ErrVisibilityMismatch) {
		t.Errorf("zero ttl: want ErrVisibilityMismatch, got %v", err)
	}
}

func TestProviderListPrefix(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	p := newFakeProvider()
	_ = p.PutObject(ctx, "media", "photo/a.jpg", "image/jpeg", strings.NewReader("a"))
	_ = p.PutObject(ctx, "media", "photo/b.jpg", "image/jpeg", strings.NewReader("bb"))
	_ = p.PutObject(ctx, "media", "doc/c.pdf", "application/pdf", strings.NewReader("ccc"))

	var seen []string
	for meta, err := range p.ListPrefix(ctx, "media", "photo/") {
		if err != nil {
			t.Fatalf("ListPrefix: %v", err)
		}
		seen = append(seen, meta.Key)
	}
	if len(seen) != 2 {
		t.Errorf("ListPrefix: want 2 photo entries, got %d (%v)", len(seen), seen)
	}
}
