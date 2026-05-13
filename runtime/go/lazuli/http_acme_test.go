package lazuli

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"sync"
	"testing"
)

const acmeHTTPTestToken = "abcABC123_-abcABC123_-"

func TestValidateACMEHTTPChallengeToken(t *testing.T) {
	if err := ValidateACMEHTTPChallengeToken(acmeHTTPTestToken); err != nil {
		t.Fatalf("ValidateACMEHTTPChallengeToken(valid) error = %v", err)
	}

	tests := []string{
		"",
		"short",
		"abcABC123_-abcABC123_=",
		"abcABC123_-abcABC123_.",
		"abcABC123_-abcABC123_/",
		"abcABC123_-abcABC123_+",
		"abcABC123_-abcABC123_ ",
		"abcABC123_-abcABC123_\xff",
	}

	for _, token := range tests {
		t.Run(strconv.Quote(token), func(t *testing.T) {
			if err := ValidateACMEHTTPChallengeToken(token); !errors.Is(err, ErrACMEHTTPChallengeTokenInvalid) {
				t.Fatalf("ValidateACMEHTTPChallengeToken(%q) error = %v, want ErrACMEHTTPChallengeTokenInvalid", token, err)
			}
		})
	}
}

func TestMemoryACMEHTTPChallengeStorePutGetDelete(t *testing.T) {
	var store MemoryACMEHTTPChallengeStore

	if _, ok := store.Get(acmeHTTPTestToken); ok {
		t.Fatal("Get before Put returned ok")
	}

	if err := store.Put(acmeHTTPTestToken, acmeHTTPTestToken+".thumbprint"); err != nil {
		t.Fatalf("Put() error = %v", err)
	}

	keyAuthorization, ok := store.Get(acmeHTTPTestToken)
	if !ok {
		t.Fatal("Get after Put returned !ok")
	}
	if keyAuthorization != acmeHTTPTestToken+".thumbprint" {
		t.Fatalf("keyAuthorization = %q", keyAuthorization)
	}

	if err := store.Delete(acmeHTTPTestToken); err != nil {
		t.Fatalf("Delete() error = %v", err)
	}
	if _, ok := store.Get(acmeHTTPTestToken); ok {
		t.Fatal("Get after Delete returned ok")
	}
}

func TestMemoryACMEHTTPChallengeStoreRejectsInvalidInputs(t *testing.T) {
	store := NewMemoryACMEHTTPChallengeStore()

	if err := store.Put("short", "short.thumbprint"); !errors.Is(err, ErrACMEHTTPChallengeTokenInvalid) {
		t.Fatalf("Put invalid token error = %v, want ErrACMEHTTPChallengeTokenInvalid", err)
	}
	if err := store.Put(acmeHTTPTestToken, " "); !errors.Is(err, ErrACMEHTTPChallengeKeyAuthorizationInvalid) {
		t.Fatalf("Put empty key authorization error = %v, want ErrACMEHTTPChallengeKeyAuthorizationInvalid", err)
	}
	if err := store.Delete("short"); !errors.Is(err, ErrACMEHTTPChallengeTokenInvalid) {
		t.Fatalf("Delete invalid token error = %v, want ErrACMEHTTPChallengeTokenInvalid", err)
	}
	if _, ok := store.Get("short"); ok {
		t.Fatal("Get invalid token returned ok")
	}
}

func TestMemoryACMEHTTPChallengeStoreConcurrentAccess(t *testing.T) {
	store := NewMemoryACMEHTTPChallengeStore()
	tokens := []string{
		"abcABC123_-abcABC123_a",
		"abcABC123_-abcABC123_b",
		"abcABC123_-abcABC123_c",
	}

	var wg sync.WaitGroup
	for _, token := range tokens {
		token := token
		wg.Add(1)
		go func() {
			defer wg.Done()
			for i := 0; i < 100; i++ {
				if err := store.Put(token, token+".thumbprint"); err != nil {
					t.Errorf("Put(%q) error = %v", token, err)
				}
				if got, ok := store.Get(token); !ok || got == "" {
					t.Errorf("Get(%q) = %q, %v", token, got, ok)
				}
			}
		}()
	}
	wg.Wait()
}

func TestACMEHTTPChallengeHandlerServesChallenge(t *testing.T) {
	store := NewMemoryACMEHTTPChallengeStore()
	keyAuthorization := acmeHTTPTestToken + ".thumbprint"
	if err := store.Put(acmeHTTPTestToken, keyAuthorization); err != nil {
		t.Fatalf("Put() error = %v", err)
	}

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, ACMEHTTPChallengePathPrefix+acmeHTTPTestToken, nil)

	ACMEHTTPChallengeHandler(store).ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if rec.Body.String() != keyAuthorization {
		t.Fatalf("body = %q, want %q", rec.Body.String(), keyAuthorization)
	}
	if got := rec.Header().Get("Content-Type"); got != "text/plain; charset=utf-8" {
		t.Fatalf("Content-Type = %q, want text/plain; charset=utf-8", got)
	}
	if got := rec.Header().Get("Cache-Control"); got != "no-store" {
		t.Fatalf("Cache-Control = %q, want no-store", got)
	}
}

func TestACMEHTTPChallengeHandlerSupportsHEAD(t *testing.T) {
	store := NewMemoryACMEHTTPChallengeStore()
	keyAuthorization := acmeHTTPTestToken + ".thumbprint"
	if err := store.Put(acmeHTTPTestToken, keyAuthorization); err != nil {
		t.Fatalf("Put() error = %v", err)
	}

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodHead, ACMEHTTPChallengePathPrefix+acmeHTTPTestToken, nil)

	ACMEHTTPChallengeHandler(store).ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if rec.Body.Len() != 0 {
		t.Fatalf("body length = %d, want 0", rec.Body.Len())
	}
	if got := rec.Header().Get("Content-Length"); got != strconv.Itoa(len(keyAuthorization)) {
		t.Fatalf("Content-Length = %q, want %d", got, len(keyAuthorization))
	}
}

func TestACMEHTTPChallengeHandlerRejectsUnsupportedMethods(t *testing.T) {
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, ACMEHTTPChallengePathPrefix+acmeHTTPTestToken, nil)

	ACMEHTTPChallengeHandler(NewMemoryACMEHTTPChallengeStore()).ServeHTTP(rec, req)

	if rec.Code != http.StatusMethodNotAllowed {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusMethodNotAllowed)
	}
	if got := rec.Header().Get("Allow"); got != "GET, HEAD" {
		t.Fatalf("Allow = %q, want GET, HEAD", got)
	}
}

func TestACMEHTTPChallengeHandlerReturnsNotFound(t *testing.T) {
	store := NewMemoryACMEHTTPChallengeStore()
	if err := store.Put(acmeHTTPTestToken, acmeHTTPTestToken+".thumbprint"); err != nil {
		t.Fatalf("Put() error = %v", err)
	}

	tests := []string{
		"/",
		ACMEHTTPChallengePathPrefix,
		ACMEHTTPChallengePathPrefix + "short",
		ACMEHTTPChallengePathPrefix + acmeHTTPTestToken + "/extra",
		ACMEHTTPChallengePathPrefix + strings.ReplaceAll(acmeHTTPTestToken, "-", "%2D"),
		ACMEHTTPChallengePathPrefix + "abcABC123_-abcABC123_z",
	}

	for _, target := range tests {
		t.Run(target, func(t *testing.T) {
			rec := httptest.NewRecorder()
			req := httptest.NewRequest(http.MethodGet, target, nil)

			ACMEHTTPChallengeHandler(store).ServeHTTP(rec, req)

			if rec.Code != http.StatusNotFound {
				t.Fatalf("status = %d, want %d", rec.Code, http.StatusNotFound)
			}
		})
	}
}
