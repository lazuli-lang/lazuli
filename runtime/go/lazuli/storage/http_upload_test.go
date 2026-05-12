package storage_test

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

type httpUploadStore struct {
	key         storage.Key
	contentType string
	ttl         time.Duration
	signErr     error
}

func (s *httpUploadStore) Put(context.Context, storage.Key, io.Reader, string) error {
	return errors.New("Put should not be called for direct upload tickets")
}

func (s *httpUploadStore) Get(context.Context, storage.Key) (io.ReadCloser, error) {
	return io.NopCloser(strings.NewReader("")), nil
}

func (s *httpUploadStore) Sign(context.Context, storage.Key, time.Duration) (string, error) {
	return "", errors.New("Sign should not be called for direct upload tickets")
}

func (s *httpUploadStore) Delete(context.Context, storage.Key) error {
	return nil
}

func (s *httpUploadStore) SignPut(_ context.Context, key storage.Key, contentType string, ttl time.Duration) (string, error) {
	s.key = key
	s.contentType = contentType
	s.ttl = ttl
	if s.signErr != nil {
		return "", s.signErr
	}
	return "https://uploads.test/" + string(key), nil
}

func TestDirectUploadHTTPHandlerReturnsJSONTicket(t *testing.T) {
	t.Parallel()

	contract := storage.Public("Profile", "avatar", 5<<20, storage.ImageAny())
	store := &httpUploadStore{}
	handler := storage.DirectUploadHTTPHandler(contract, store, 2*time.Minute)
	req := httptest.NewRequest(http.MethodPost, "/uploads/profile/avatar", strings.NewReader(`{
		"filename": "me.png",
		"content_type": "image/png",
		"size": 4096
	}`))
	rec := httptest.NewRecorder()

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, body = %s", rec.Code, rec.Body.String())
	}
	if got := rec.Header().Get("Content-Type"); got != "application/json" {
		t.Fatalf("Content-Type = %q, want application/json", got)
	}

	var body storage.DirectUploadHTTPResponse
	if err := json.NewDecoder(rec.Body).Decode(&body); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if body.Key != storage.Key("Profile/avatar/me.png") {
		t.Fatalf("key = %q, want Profile/avatar/me.png", body.Key)
	}
	if body.UploadURL != "https://uploads.test/Profile/avatar/me.png" {
		t.Fatalf("upload_url = %q", body.UploadURL)
	}
	if body.Headers["Content-Type"] != "image/png" {
		t.Fatalf("headers = %v, want Content-Type image/png", body.Headers)
	}

	if store.key != body.Key {
		t.Fatalf("SignPut key = %q, want %q", store.key, body.Key)
	}
	if store.contentType != "image/png" {
		t.Fatalf("SignPut contentType = %q, want image/png", store.contentType)
	}
	if store.ttl != 2*time.Minute {
		t.Fatalf("SignPut ttl = %v, want 2m", store.ttl)
	}
}

func TestDirectUploadHTTPHandlerRejectsInvalidRequests(t *testing.T) {
	t.Parallel()

	contract := storage.Public("Profile", "avatar", 1024, storage.ImageAny())
	store := &httpUploadStore{}
	handler := storage.DirectUploadHTTPHandler(contract, store, time.Minute)

	t.Run("method", func(t *testing.T) {
		t.Parallel()

		rec := httptest.NewRecorder()
		handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/uploads/profile/avatar", nil))

		assertHTTPUploadError(t, rec, http.StatusMethodNotAllowed, "method_not_allowed")
		if got := rec.Header().Get("Allow"); got != http.MethodPost {
			t.Fatalf("Allow = %q, want POST", got)
		}
	})

	t.Run("json", func(t *testing.T) {
		t.Parallel()

		rec := httptest.NewRecorder()
		req := httptest.NewRequest(http.MethodPost, "/uploads/profile/avatar", strings.NewReader(`{"filename":`))
		handler.ServeHTTP(rec, req)

		assertHTTPUploadError(t, rec, http.StatusBadRequest, "bad_request")
	})

	t.Run("unknown field", func(t *testing.T) {
		t.Parallel()

		rec := httptest.NewRecorder()
		req := httptest.NewRequest(http.MethodPost, "/uploads/profile/avatar", strings.NewReader(`{"filename":"me.png","content_type":"image/png","extra":true}`))
		handler.ServeHTTP(rec, req)

		assertHTTPUploadError(t, rec, http.StatusBadRequest, "bad_request")
	})
}

func TestDirectUploadHTTPHandlerMapsStorageErrors(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		contract storage.FileContract
		body     string
		ttl      time.Duration
		status   int
		code     string
	}{
		{
			name:     "size",
			contract: storage.Private("ImportBatch", "file", 1024, storage.TextMime("csv")),
			body:     `{"filename":"people.csv","content_type":"text/csv","size":2048}`,
			ttl:      time.Minute,
			status:   http.StatusRequestEntityTooLarge,
			code:     "storage.file_size_exceeded",
		},
		{
			name:     "mime",
			contract: storage.Private("ImportBatch", "file", 1024, storage.TextMime("csv")),
			body:     `{"filename":"people.pdf","content_type":"application/pdf","size":512}`,
			ttl:      time.Minute,
			status:   http.StatusUnsupportedMediaType,
			code:     "storage.file_mime_rejected",
		},
		{
			name:     "visibility",
			contract: storage.Public("Profile", "avatar", 1024, storage.ImageAny()),
			body:     `{"filename":"me.png","content_type":"image/png","size":512}`,
			ttl:      0,
			status:   http.StatusInternalServerError,
			code:     "storage.visibility_mismatch",
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			rec := httptest.NewRecorder()
			req := httptest.NewRequest(http.MethodPost, "/uploads", strings.NewReader(tt.body))
			storage.DirectUploadHTTPHandler(tt.contract, &httpUploadStore{}, tt.ttl).ServeHTTP(rec, req)

			assertHTTPUploadError(t, rec, tt.status, tt.code)
		})
	}
}

func TestDirectUploadHTTPHandlerReturnsStoreErrorsAsJSON(t *testing.T) {
	t.Parallel()

	contract := storage.Public("Profile", "avatar", 1024, storage.ImageAny())
	store := &httpUploadStore{signErr: errors.New("signer unavailable")}
	handler := storage.DirectUploadHTTPHandler(contract, store, time.Minute)
	req := httptest.NewRequest(http.MethodPost, "/uploads/profile/avatar", strings.NewReader(`{
		"filename": "me.png",
		"content_type": "image/png",
		"size": 512
	}`))
	rec := httptest.NewRecorder()

	handler.ServeHTTP(rec, req)

	assertHTTPUploadError(t, rec, http.StatusInternalServerError, "storage.direct_upload_failed")
}

func assertHTTPUploadError(t *testing.T, rec *httptest.ResponseRecorder, status int, code string) {
	t.Helper()

	if rec.Code != status {
		t.Fatalf("status = %d, body = %s", rec.Code, rec.Body.String())
	}
	if got := rec.Header().Get("Content-Type"); got != "application/json" {
		t.Fatalf("Content-Type = %q, want application/json", got)
	}

	var body storage.DirectUploadHTTPError
	if err := json.NewDecoder(rec.Body).Decode(&body); err != nil {
		t.Fatalf("decode error response: %v", err)
	}
	if body.Code != code {
		t.Fatalf("code = %q, want %q", body.Code, code)
	}
	if body.Error == "" {
		t.Fatal("error message is empty")
	}
}
