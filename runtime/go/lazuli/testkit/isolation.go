package testkit

import (
	"crypto/rand"
	"encoding/hex"
	"os"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
)

var tempNamespaceSeq atomic.Uint64

// TempNamespace returns a unique, lowercase namespace for resources created by
// a test. The prefix is sanitized to letters, digits, and dashes.
func TempNamespace(t testing.TB, prefix string) string {
	t.Helper()

	prefix = cleanTempNamespacePrefix(prefix)
	seq := tempNamespaceSeq.Add(1)
	random := make([]byte, 8)
	if _, err := rand.Read(random); err != nil {
		t.Fatalf("testkit: temp namespace random suffix: %v", err)
	}

	return prefix + "-" + base36(uint64(os.Getpid())) + "-" + base36(seq) + "-" + hex.EncodeToString(random)
}

// RestoreEnv restores key to its current value when the test ends. It is useful
// when a test needs to call os.Setenv or os.Unsetenv directly.
func RestoreEnv(t testing.TB, key string) {
	t.Helper()

	snapshot := captureEnv(t, key)
	t.Cleanup(func() {
		t.Helper()
		snapshot.restore(t)
	})
}

// Setenv sets key for the current test and restores its previous value when the
// test ends.
func Setenv(t testing.TB, key, value string) {
	t.Helper()

	snapshot := captureEnv(t, key)
	if err := os.Setenv(key, value); err != nil {
		snapshot.restore(t)
		t.Fatalf("testkit: set env %q: %v", key, err)
	}
	t.Cleanup(func() {
		t.Helper()
		snapshot.restore(t)
	})
}

// RestoreWorkingDir restores the current working directory when the test ends.
// It is useful when a test needs to call os.Chdir directly.
func RestoreWorkingDir(t testing.TB) {
	t.Helper()

	snapshot := captureWorkingDir(t)
	t.Cleanup(func() {
		t.Helper()
		snapshot.restore(t)
	})
}

// Chdir changes the current working directory for the current test and restores
// the previous directory when the test ends.
func Chdir(t testing.TB, dir string) {
	t.Helper()

	snapshot := captureWorkingDir(t)
	if err := os.Chdir(dir); err != nil {
		snapshot.restore(t)
		t.Fatalf("testkit: chdir %q: %v", dir, err)
	}
	t.Cleanup(func() {
		t.Helper()
		snapshot.restore(t)
	})
}

// CleanupStack runs cleanup functions in last-in, first-out order. A stack
// created with NewCleanupStack is automatically cleaned up when the test ends.
type CleanupStack struct {
	mu       sync.Mutex
	cleanups []func()
	ran      bool
}

// NewCleanupStack returns a cleanup stack tied to t.
func NewCleanupStack(t testing.TB) *CleanupStack {
	t.Helper()

	stack := &CleanupStack{}
	t.Cleanup(stack.Cleanup)
	return stack
}

// Push adds fn to the stack. Nil functions are ignored.
func (s *CleanupStack) Push(fn func()) {
	if s == nil || fn == nil {
		return
	}

	s.mu.Lock()
	defer s.mu.Unlock()
	if s.ran {
		panic("testkit: cleanup stack already ran")
	}
	s.cleanups = append(s.cleanups, fn)
}

// Defer is an alias for Push.
func (s *CleanupStack) Defer(fn func()) {
	s.Push(fn)
}

// Cleanup runs all pending cleanup functions in last-in, first-out order.
// Calling Cleanup more than once is safe.
func (s *CleanupStack) Cleanup() {
	if s == nil {
		return
	}

	s.mu.Lock()
	if s.ran {
		s.mu.Unlock()
		return
	}
	cleanups := s.cleanups
	s.cleanups = nil
	s.ran = true
	s.mu.Unlock()

	for i := len(cleanups) - 1; i >= 0; i-- {
		cleanups[i]()
	}
}

type envSnapshot struct {
	key    string
	value  string
	ok     bool
	unlock func()
	once   sync.Once
}

func captureEnv(t testing.TB, key string) *envSnapshot {
	t.Helper()
	if err := validateEnvKey(key); err != nil {
		t.Fatalf("testkit: env key %q: %v", key, err)
	}

	unlock := processMutationLock.lock(t.Name())
	value, ok := os.LookupEnv(key)
	return &envSnapshot{
		key:    key,
		value:  value,
		ok:     ok,
		unlock: unlock,
	}
}

func (s *envSnapshot) restore(t testing.TB) {
	t.Helper()
	s.once.Do(func() {
		defer s.unlock()
		if s.ok {
			if err := os.Setenv(s.key, s.value); err != nil {
				t.Fatalf("testkit: restore env %q: %v", s.key, err)
			}
			return
		}
		if err := os.Unsetenv(s.key); err != nil {
			t.Fatalf("testkit: unset env %q: %v", s.key, err)
		}
	})
}

type workingDirSnapshot struct {
	dir    string
	unlock func()
	once   sync.Once
}

func captureWorkingDir(t testing.TB) *workingDirSnapshot {
	t.Helper()

	unlock := processMutationLock.lock(t.Name())
	dir, err := os.Getwd()
	if err != nil {
		unlock()
		t.Fatalf("testkit: get working directory: %v", err)
	}
	return &workingDirSnapshot{dir: dir, unlock: unlock}
}

func (s *workingDirSnapshot) restore(t testing.TB) {
	t.Helper()
	s.once.Do(func() {
		defer s.unlock()
		if err := os.Chdir(s.dir); err != nil {
			t.Fatalf("testkit: restore working directory %q: %v", s.dir, err)
		}
	})
}

type reentrantTestLock struct {
	mu    sync.Mutex
	cond  *sync.Cond
	owner string
	depth int
}

var processMutationLock reentrantTestLock

func (l *reentrantTestLock) lock(owner string) func() {
	l.mu.Lock()
	if l.cond == nil {
		l.cond = sync.NewCond(&l.mu)
	}
	for l.owner != "" && l.owner != owner {
		l.cond.Wait()
	}
	l.owner = owner
	l.depth++
	l.mu.Unlock()

	return func() {
		l.mu.Lock()
		defer l.mu.Unlock()
		if l.owner != owner {
			panic("testkit: process mutation lock released by non-owner")
		}
		l.depth--
		if l.depth == 0 {
			l.owner = ""
			l.cond.Broadcast()
		}
	}
}

func cleanTempNamespacePrefix(prefix string) string {
	prefix = strings.ToLower(strings.TrimSpace(prefix))
	var b strings.Builder
	lastDash := false
	for _, r := range prefix {
		if isASCIILetter(r) || isASCIIDigit(r) {
			b.WriteRune(r)
			lastDash = false
			continue
		}
		if b.Len() > 0 && !lastDash {
			b.WriteByte('-')
			lastDash = true
		}
	}

	cleaned := strings.Trim(b.String(), "-")
	if cleaned == "" {
		return "test"
	}
	if isASCIIDigit(rune(cleaned[0])) {
		return "ns-" + cleaned
	}
	return cleaned
}

func isASCIILetter(r rune) bool {
	return r >= 'a' && r <= 'z'
}

func isASCIIDigit(r rune) bool {
	return r >= '0' && r <= '9'
}

func validateEnvKey(key string) error {
	if key == "" {
		return errEmptyEnvKey{}
	}
	if strings.Contains(key, "=") {
		return errInvalidEnvKey{}
	}
	return nil
}

type errEmptyEnvKey struct{}

func (errEmptyEnvKey) Error() string {
	return "empty key"
}

type errInvalidEnvKey struct{}

func (errInvalidEnvKey) Error() string {
	return "contains equals sign"
}

func base36(value uint64) string {
	return strconv.FormatUint(value, 36)
}
