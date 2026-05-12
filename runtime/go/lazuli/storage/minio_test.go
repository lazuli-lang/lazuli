package storage

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/minio/minio-go/v7"
	"github.com/minio/minio-go/v7/pkg/credentials"
)

type fakeMinIOServer struct {
	mu      sync.Mutex
	objects map[string]fakeMinIOObject
}

type fakeMinIOObject struct {
	body        []byte
	contentType string
}

func newFakeMinIOServer() *fakeMinIOServer {
	return &fakeMinIOServer{objects: make(map[string]fakeMinIOObject)}
}

func (f *fakeMinIOServer) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	if r.Method == http.MethodGet && r.URL.Query().Has("location") {
		w.Header().Set("Content-Type", "application/xml")
		io.WriteString(w, `<LocationConstraint xmlns="http://s3.amazonaws.com/doc/2006-03-01/">us-east-1</LocationConstraint>`)
		return
	}

	bucket, key, ok := splitMinIOPath(r.URL.Path)
	if !ok || bucket != "bucket" {
		writeMinIOError(w, http.StatusNotFound, "NoSuchBucket")
		return
	}

	objectID := bucket + "/" + key
	switch r.Method {
	case http.MethodPut:
		body, err := io.ReadAll(r.Body)
		if err != nil {
			writeMinIOError(w, http.StatusInternalServerError, "InternalError")
			return
		}
		f.mu.Lock()
		f.objects[objectID] = fakeMinIOObject{body: body, contentType: r.Header.Get("Content-Type")}
		f.mu.Unlock()
		w.Header().Set("ETag", `"fake-etag"`)
		w.WriteHeader(http.StatusOK)
	case http.MethodHead:
		obj, ok := f.object(objectID)
		if !ok {
			writeMinIOError(w, http.StatusNotFound, "NoSuchKey")
			return
		}
		writeMinIOObjectHeaders(w, obj)
		w.WriteHeader(http.StatusOK)
	case http.MethodGet:
		obj, ok := f.object(objectID)
		if !ok {
			writeMinIOError(w, http.StatusNotFound, "NoSuchKey")
			return
		}
		writeMinIOObjectHeaders(w, obj)
		w.WriteHeader(http.StatusOK)
		w.Write(obj.body)
	case http.MethodDelete:
		f.mu.Lock()
		delete(f.objects, objectID)
		f.mu.Unlock()
		w.WriteHeader(http.StatusNoContent)
	default:
		writeMinIOError(w, http.StatusMethodNotAllowed, "MethodNotAllowed")
	}
}

func (f *fakeMinIOServer) object(id string) (fakeMinIOObject, bool) {
	f.mu.Lock()
	defer f.mu.Unlock()
	obj, ok := f.objects[id]
	return obj, ok
}

func splitMinIOPath(path string) (string, string, bool) {
	trimmed := strings.TrimPrefix(path, "/")
	parts := strings.SplitN(trimmed, "/", 2)
	if len(parts) != 2 || parts[0] == "" || parts[1] == "" {
		return "", "", false
	}
	return parts[0], parts[1], true
}

func writeMinIOObjectHeaders(w http.ResponseWriter, obj fakeMinIOObject) {
	w.Header().Set("Content-Length", fmt.Sprint(len(obj.body)))
	w.Header().Set("Content-Type", obj.contentType)
	w.Header().Set("ETag", `"fake-etag"`)
	w.Header().Set("Last-Modified", time.Unix(0, 0).UTC().Format(http.TimeFormat))
}

func writeMinIOError(w http.ResponseWriter, status int, code string) {
	w.Header().Set("Content-Type", "application/xml")
	w.WriteHeader(status)
	fmt.Fprintf(w, `<Error><Code>%s</Code><Message>%s</Message></Error>`, code, code)
}

func newTestMinIOStore(t *testing.T, srv *httptest.Server) *MinIOStore {
	t.Helper()

	endpoint := strings.TrimPrefix(srv.URL, "https://")
	client, err := minio.New(endpoint, &minio.Options{
		Creds:     credentials.NewStaticV4("minio", "miniostorage", ""),
		Secure:    true,
		Transport: srv.Client().Transport,
	})
	if err != nil {
		t.Fatalf("minio.New: %v", err)
	}
	return &MinIOStore{Client: client, Bucket: "bucket"}
}

func TestNewMinIOStore(t *testing.T) {
	t.Parallel()

	store, err := NewMinIOStore("localhost:9000", "minio", "miniostorage", "bucket", false)
	if err != nil {
		t.Fatalf("NewMinIOStore: %v", err)
	}
	if store.Client == nil {
		t.Fatal("NewMinIOStore returned nil client")
	}
	if store.Bucket != "bucket" {
		t.Fatalf("Bucket = %q, want bucket", store.Bucket)
	}
}

func TestMinIOStorePutGetDeleteRoundTrip(t *testing.T) {
	t.Parallel()

	fake := newFakeMinIOServer()
	srv := httptest.NewTLSServer(fake)
	defer srv.Close()
	store := newTestMinIOStore(t, srv)

	ctx := context.Background()
	payload := "hello,minio\n"
	if err := store.Put(ctx, Key("import/file"), strings.NewReader(payload), "text/csv"); err != nil {
		t.Fatalf("Put: %v", err)
	}

	obj, ok := fake.object("bucket/import/file")
	if !ok {
		t.Fatal("Put did not store object")
	}
	if obj.contentType != "text/csv" {
		t.Fatalf("Content-Type = %q, want text/csv", obj.contentType)
	}

	rc, err := store.Get(ctx, Key("import/file"))
	if err != nil {
		t.Fatalf("Get: %v", err)
	}
	got, err := io.ReadAll(rc)
	if err != nil {
		t.Fatalf("ReadAll: %v", err)
	}
	rc.Close()
	if string(got) != payload {
		t.Fatalf("Get bytes = %q, want %q", got, payload)
	}

	if err := store.Delete(ctx, Key("import/file")); err != nil {
		t.Fatalf("Delete: %v", err)
	}
	if _, err := store.Get(ctx, Key("import/file")); !errors.Is(err, ErrFileNotFound) {
		t.Fatalf("Get after Delete: want ErrFileNotFound, got %v", err)
	}
	if err := store.Delete(ctx, Key("import/file")); !errors.Is(err, ErrFileNotFound) {
		t.Fatalf("Delete missing: want ErrFileNotFound, got %v", err)
	}
}

func TestMinIOStoreSign(t *testing.T) {
	t.Parallel()

	fake := newFakeMinIOServer()
	srv := httptest.NewTLSServer(fake)
	defer srv.Close()
	store := newTestMinIOStore(t, srv)

	signed, err := store.Sign(context.Background(), Key("import/file"), time.Minute)
	if err != nil {
		t.Fatalf("Sign: %v", err)
	}
	if !strings.HasPrefix(signed, srv.URL+"/bucket/import/file?") {
		t.Fatalf("signed URL = %q, want server bucket/key prefix", signed)
	}
	if !strings.Contains(signed, "X-Amz-Expires=60") {
		t.Fatalf("signed URL = %q, want 60 second expiry", signed)
	}
	if !strings.Contains(signed, "X-Amz-Signature=") {
		t.Fatalf("signed URL = %q, want signature", signed)
	}

	if _, err := store.Sign(context.Background(), Key("import/file"), 0); !errors.Is(err, ErrVisibilityMismatch) {
		t.Fatalf("Sign ttl=0: want ErrVisibilityMismatch, got %v", err)
	}
}
