package testkit_test

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/testkit"
)

func TestTempNamespaceSanitizesAndUniquifies(t *testing.T) {
	t.Parallel()

	first := testkit.TempNamespace(t, " Customer Reports! ")
	second := testkit.TempNamespace(t, " Customer Reports! ")

	if first == second {
		t.Fatalf("TempNamespace returned duplicate value %q", first)
	}
	if !strings.HasPrefix(first, "customer-reports-") {
		t.Fatalf("TempNamespace prefix = %q, want customer-reports-*", first)
	}
	assertNamespaceSafe(t, first)
	assertNamespaceSafe(t, second)

	digitPrefix := testkit.TempNamespace(t, "123")
	if !strings.HasPrefix(digitPrefix, "ns-123-") {
		t.Fatalf("TempNamespace digit prefix = %q, want ns-123-*", digitPrefix)
	}
}

func TestSetenvRestoresPreviousValue(t *testing.T) {
	key := testEnvKey(t, "setenv")
	if err := os.Setenv(key, "original"); err != nil {
		t.Fatalf("Setenv(%q) setup error = %v", key, err)
	}
	t.Cleanup(func() { _ = os.Unsetenv(key) })

	t.Run("child", func(t *testing.T) {
		t.Parallel()

		testkit.Setenv(t, key, "first")
		testkit.Setenv(t, key, "second")
		if got := os.Getenv(key); got != "second" {
			t.Fatalf("env %s = %q, want second", key, got)
		}
	})

	if got := os.Getenv(key); got != "original" {
		t.Fatalf("env %s after cleanup = %q, want original", key, got)
	}
}

func TestRestoreEnvRestoresUnsetValue(t *testing.T) {
	key := testEnvKey(t, "restore")
	if err := os.Unsetenv(key); err != nil {
		t.Fatalf("Unsetenv(%q) setup error = %v", key, err)
	}

	t.Run("child", func(t *testing.T) {
		t.Parallel()

		testkit.RestoreEnv(t, key)
		if err := os.Setenv(key, "temporary"); err != nil {
			t.Fatalf("Setenv(%q) error = %v", key, err)
		}
	})

	if got, ok := os.LookupEnv(key); ok {
		t.Fatalf("env %s after cleanup = %q, want unset", key, got)
	}
}

func TestChdirRestoresWorkingDirectory(t *testing.T) {
	original := currentWorkingDir(t)
	target := t.TempDir()

	t.Run("child", func(t *testing.T) {
		t.Parallel()

		testkit.Chdir(t, target)
		if got := currentWorkingDir(t); got != filepath.Clean(target) {
			t.Fatalf("working directory = %q, want %q", got, target)
		}
	})

	if got := currentWorkingDir(t); got != original {
		t.Fatalf("working directory after cleanup = %q, want %q", got, original)
	}
}

func TestRestoreWorkingDirRestoresManualChdir(t *testing.T) {
	original := currentWorkingDir(t)
	target := t.TempDir()

	t.Run("child", func(t *testing.T) {
		t.Parallel()

		testkit.RestoreWorkingDir(t)
		if err := os.Chdir(target); err != nil {
			t.Fatalf("Chdir(%q) error = %v", target, err)
		}
	})

	if got := currentWorkingDir(t); got != original {
		t.Fatalf("working directory after cleanup = %q, want %q", got, original)
	}
}

func TestCleanupStackRunsInLIFOOrderAndOnlyOnce(t *testing.T) {
	stack := testkit.NewCleanupStack(t)
	var order []string

	stack.Push(func() { order = append(order, "first") })
	stack.Defer(func() { order = append(order, "second") })
	stack.Cleanup()
	stack.Cleanup()

	if got := strings.Join(order, ","); got != "second,first" {
		t.Fatalf("cleanup order = %q, want second,first", got)
	}
}

func TestCleanupStackRunsAtTestCleanup(t *testing.T) {
	var ran bool

	t.Run("child", func(t *testing.T) {
		stack := testkit.NewCleanupStack(t)
		stack.Push(func() { ran = true })
	})

	if !ran {
		t.Fatal("cleanup stack did not run at test cleanup")
	}
}

func assertNamespaceSafe(t *testing.T, value string) {
	t.Helper()

	if value == "" {
		t.Fatal("namespace is empty")
	}
	if strings.Contains(value, "--") {
		t.Fatalf("namespace %q contains collapsed dash", value)
	}
	for _, r := range value {
		if (r >= 'a' && r <= 'z') || (r >= '0' && r <= '9') || r == '-' {
			continue
		}
		t.Fatalf("namespace %q contains unsafe rune %q", value, r)
	}
}

func testEnvKey(t *testing.T, prefix string) string {
	t.Helper()

	namespace := testkit.TempNamespace(t, prefix)
	namespace = strings.ToUpper(strings.ReplaceAll(namespace, "-", "_"))
	return "LAZULI_TESTKIT_" + namespace
}

func currentWorkingDir(t *testing.T) string {
	t.Helper()

	wd, err := os.Getwd()
	if err != nil {
		t.Fatalf("Getwd() error = %v", err)
	}
	return filepath.Clean(wd)
}
