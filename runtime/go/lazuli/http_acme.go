package lazuli

import (
	"errors"
	"io"
	"net/http"
	"strconv"
	"strings"
	"sync"
)

const (
	// ACMEHTTPChallengePathPrefix is the URL path prefix used by ACME HTTP-01
	// challenge validation requests.
	ACMEHTTPChallengePathPrefix = "/.well-known/acme-challenge/"

	minACMEHTTPChallengeTokenLength = 22
)

var (
	// ErrACMEHTTPChallengeTokenInvalid is returned when an HTTP-01 challenge
	// token is empty, too short, or not unpadded base64url text.
	ErrACMEHTTPChallengeTokenInvalid = errors.New("lazuli: acme http challenge token invalid")

	// ErrACMEHTTPChallengeKeyAuthorizationInvalid is returned when the response
	// body for a challenge token is empty.
	ErrACMEHTTPChallengeKeyAuthorizationInvalid = errors.New("lazuli: acme http challenge key authorization invalid")
)

// ValidateACMEHTTPChallengeToken validates an ACME HTTP-01 challenge token.
//
// Tokens are supplied by an ACME server and must be unpadded base64url text
// with enough length to carry at least 128 bits of entropy.
func ValidateACMEHTTPChallengeToken(token string) error {
	if len(token) < minACMEHTTPChallengeTokenLength {
		return ErrACMEHTTPChallengeTokenInvalid
	}
	for i := 0; i < len(token); i++ {
		if !isACMEHTTPChallengeTokenByte(token[i]) {
			return ErrACMEHTTPChallengeTokenInvalid
		}
	}
	return nil
}

// ACMEHTTPChallengeStore resolves ACME HTTP-01 challenge tokens to key
// authorization response bodies.
type ACMEHTTPChallengeStore interface {
	Get(token string) (keyAuthorization string, ok bool)
}

// MemoryACMEHTTPChallengeStore stores ACME HTTP-01 challenge responses in
// process memory. The zero value is ready to use.
type MemoryACMEHTTPChallengeStore struct {
	mu         sync.RWMutex
	challenges map[string]string
}

var _ ACMEHTTPChallengeStore = (*MemoryACMEHTTPChallengeStore)(nil)

// NewMemoryACMEHTTPChallengeStore returns an empty in-process challenge store.
func NewMemoryACMEHTTPChallengeStore() *MemoryACMEHTTPChallengeStore {
	return &MemoryACMEHTTPChallengeStore{
		challenges: make(map[string]string),
	}
}

// Put stores keyAuthorization for token.
func (s *MemoryACMEHTTPChallengeStore) Put(token, keyAuthorization string) error {
	if err := ValidateACMEHTTPChallengeToken(token); err != nil {
		return err
	}
	if strings.TrimSpace(keyAuthorization) == "" {
		return ErrACMEHTTPChallengeKeyAuthorizationInvalid
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	s.ensureLocked()
	s.challenges[token] = keyAuthorization
	return nil
}

// Get returns the key authorization body stored for token.
func (s *MemoryACMEHTTPChallengeStore) Get(token string) (string, bool) {
	if err := ValidateACMEHTTPChallengeToken(token); err != nil {
		return "", false
	}

	s.mu.RLock()
	defer s.mu.RUnlock()

	keyAuthorization, ok := s.challenges[token]
	return keyAuthorization, ok
}

// Delete removes token from the store.
func (s *MemoryACMEHTTPChallengeStore) Delete(token string) error {
	if err := ValidateACMEHTTPChallengeToken(token); err != nil {
		return err
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	delete(s.challenges, token)
	return nil
}

func (s *MemoryACMEHTTPChallengeStore) ensureLocked() {
	if s.challenges == nil {
		s.challenges = make(map[string]string)
	}
}

// ACMEHTTPChallengeHandler returns an HTTP handler for ACME HTTP-01 validation
// requests under /.well-known/acme-challenge/<token>.
func ACMEHTTPChallengeHandler(store ACMEHTTPChallengeStore) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ServeACMEHTTPChallenge(w, r, store)
	})
}

// ServeACMEHTTPChallenge handles one ACME HTTP-01 validation request.
func ServeACMEHTTPChallenge(w http.ResponseWriter, r *http.Request, store ACMEHTTPChallengeStore) {
	if r.Method != http.MethodGet && r.Method != http.MethodHead {
		w.Header().Set("Allow", "GET, HEAD")
		http.Error(w, http.StatusText(http.StatusMethodNotAllowed), http.StatusMethodNotAllowed)
		return
	}

	token, ok := acmeHTTPChallengeTokenFromRequest(r)
	if !ok || store == nil {
		http.NotFound(w, r)
		return
	}

	keyAuthorization, ok := store.Get(token)
	if !ok {
		http.NotFound(w, r)
		return
	}

	header := w.Header()
	header.Set("Cache-Control", "no-store")
	header.Set("Content-Type", "text/plain; charset=utf-8")
	header.Set("Content-Length", strconv.Itoa(len(keyAuthorization)))
	header.Set("X-Content-Type-Options", "nosniff")
	w.WriteHeader(http.StatusOK)

	if r.Method == http.MethodHead {
		return
	}
	_, _ = io.WriteString(w, keyAuthorization)
}

func acmeHTTPChallengeTokenFromRequest(r *http.Request) (string, bool) {
	if r == nil || r.URL == nil {
		return "", false
	}

	requestPath := r.URL.EscapedPath()
	if requestPath == "" {
		requestPath = r.URL.Path
	}
	token, ok := strings.CutPrefix(requestPath, ACMEHTTPChallengePathPrefix)
	if !ok || strings.Contains(token, "/") {
		return "", false
	}
	if err := ValidateACMEHTTPChallengeToken(token); err != nil {
		return "", false
	}
	return token, true
}

func isACMEHTTPChallengeTokenByte(b byte) bool {
	return ('a' <= b && b <= 'z') ||
		('A' <= b && b <= 'Z') ||
		('0' <= b && b <= '9') ||
		b == '-' ||
		b == '_'
}
